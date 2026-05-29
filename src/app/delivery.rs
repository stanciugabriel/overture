use crate::{
    config::*,
    dosing::{
        DeliveryAlert, DeliveryState, DoseAccumulator, Prescription, approx_delivery_seconds,
        delivery_rate_ul_per_min, delivery_target_ul, dispense_step_period_us, prepare_dose,
        rate_step_period_us,
    },
    motor::MotorClient,
    types::RuntimeSettings,
};
use embassy_time::{Duration, Instant};

/// Prepares a new programmed dose and turns validation failures into a visible delivery fault.
pub(super) fn prepare_or_fault(
    dose: &mut DoseAccumulator,
    delivery: &mut DeliveryState,
    prescription: &Prescription,
) -> Result<(), ()> {
    match prepare_dose(dose, delivery, prescription) {
        Ok(steps) => {
            let expected_ul = steps as f32 * crate::dosing::ul_per_step(prescription);
            log::info!(
                "dose prepared: target={}uL rate={}uL/min steps={} expected={}uL period={}us approx_time={}s",
                delivery_target_ul(prescription),
                delivery_rate_ul_per_min(prescription),
                steps,
                expected_ul,
                dispense_step_period_us(prescription),
                approx_delivery_seconds(steps, dispense_step_period_us(prescription))
            );
            Ok(())
        }
        Err(error) => {
            delivery.stop();
            delivery.alert = DeliveryAlert::DosingFault;
            log::error!("dose preparation failed: {:?}", error);
            Err(())
        }
    }
}

/// Converts a percent-based flash-save policy into a step interval for the active dose.
pub(super) fn delivery_flash_checkpoint_interval(delivery: &DeliveryState) -> u32 {
    let percent_step = FLASH_DELIVERY_PROGRESS_PERCENT_STEP.max(1);
    let interval = delivery
        .dose_steps
        .saturating_mul(percent_step)
        .saturating_add(99)
        / 100;

    interval.max(1)
}

/// Returns the next delivered-step count at which delivery state should be persisted.
pub(super) fn next_delivery_flash_checkpoint(delivery: &DeliveryState) -> u32 {
    delivery
        .delivered_steps_this_dose
        .saturating_add(delivery_flash_checkpoint_interval(delivery))
}

/// Decides whether to save progress, either by dose progress or elapsed time.
pub(super) fn delivery_flash_checkpoint_due(
    delivery: &DeliveryState,
    next_step_checkpoint: u32,
    last_flash_save: Instant,
) -> bool {
    if !delivery.running || !(delivery.remaining_steps > 0 || delivery.kvo_active) {
        return false;
    }

    let step_checkpoint_due =
        delivery.dose_steps > 0 && delivery.delivered_steps_this_dose >= next_step_checkpoint;
    let time_checkpoint_due =
        last_flash_save.elapsed() >= Duration::from_millis(FLASH_DELIVERY_PROGRESS_MAX_INTERVAL_MS);

    step_checkpoint_due || time_checkpoint_due
}

/// Alerts with flashing overlays must be redrawn even when no values changed.
pub(super) fn alert_needs_periodic_redraw(delivery: &DeliveryState) -> bool {
    matches!(
        delivery.alert,
        DeliveryAlert::EndOfInfusion | DeliveryAlert::SyringeEmpty | DeliveryAlert::DosingFault
    )
}

/// Consumes motor status updates and converts newly completed steps into delivered volume.
pub(super) fn apply_delivery_motor_status(
    motor: MotorClient,
    dose: &mut DoseAccumulator,
    delivery: &mut DeliveryState,
    prescription: &Prescription,
    settings: &RuntimeSettings,
    active_command: &mut Option<u32>,
    seen_steps: &mut u32,
) -> u32 {
    let mut delivered_now = 0u32;

    while let Some(status) = motor.try_status() {
        let Some(command_id) = *active_command else {
            continue;
        };
        if status.command_id != command_id {
            continue;
        }

        let current_steps = status.command_steps;
        let delta = current_steps.saturating_sub(*seen_steps);
        if delta > 0 {
            dose.record_dispense(delta, prescription);
            update_delivery_after_motor_steps(dose, delivery, prescription, settings, delta);
            *seen_steps = current_steps;
            delivered_now = delivered_now.saturating_add(delta);
        }

        if status.completed_command_id == command_id {
            *active_command = None;
            *seen_steps = 0;
        }
    }

    delivered_now
}

/// Tracks manual/programmed bolus motor progress separately from normal delivery state.
pub(super) fn apply_bolus_motor_status(
    motor: MotorClient,
    dose: &mut DoseAccumulator,
    prescription: &Prescription,
    active_command: &mut Option<u32>,
    seen_steps: &mut u32,
) -> (u32, bool) {
    let mut delivered_now = 0u32;
    let mut completed = false;

    while let Some(status) = motor.try_status() {
        let Some(command_id) = *active_command else {
            continue;
        };
        if status.command_id != command_id {
            continue;
        }

        let current_steps = status.command_steps;
        let delta = current_steps.saturating_sub(*seen_steps);
        if delta > 0 {
            dose.record_dispense(delta, prescription);
            *seen_steps = current_steps;
            delivered_now = delivered_now.saturating_add(delta);
        }

        if status.completed_command_id == command_id {
            *active_command = None;
            *seen_steps = 0;
            completed = true;
        }
    }

    (delivered_now, completed)
}

/// Applies delivered steps to the active dose and transitions to KVO or end-of-infusion.
pub(super) fn update_delivery_after_motor_steps(
    dose: &DoseAccumulator,
    delivery: &mut DeliveryState,
    prescription: &Prescription,
    settings: &RuntimeSettings,
    steps: u32,
) {
    if delivery.remaining_steps == 0 {
        return;
    }

    let delivered_steps = steps.min(delivery.remaining_steps);
    delivery.remaining_steps -= delivered_steps;
    delivery.delivered_steps_this_dose = delivery
        .delivered_steps_this_dose
        .saturating_add(delivered_steps);

    if delivery.remaining_steps == 0 {
        if settings.kvo_enabled {
            delivery.kvo_active = true;
            delivery.running = true;
            delivery.ramp_progress_steps = 0;
            delivery.alert = DeliveryAlert::KvoRunning;
            log::info!(
                "dose complete, KVO started: delivered_total={}uL delivered_steps={} kvo_rate={}uL/min period={}us",
                dose.total_ul,
                delivery.delivered_steps_this_dose,
                settings.kvo_rate_ul_per_min,
                rate_step_period_us(settings.kvo_rate_ul_per_min, prescription.syringe)
            );
        } else {
            delivery.running = false;
            delivery.alert = DeliveryAlert::EndOfInfusion;
            log::info!(
                "delivery complete: delivered_total={}uL delivered_steps={} fractional_step_carry={}",
                dose.total_ul,
                delivery.delivered_steps_this_dose,
                dose.fractional_steps()
            );
        }
    }
}

/// Logs a human-readable explanation whenever delivery or KVO is started, paused, or resumed.
pub(super) fn log_delivery_toggle(
    delivery: &DeliveryState,
    dose: &DoseAccumulator,
    prescription: &Prescription,
    settings: &RuntimeSettings,
) {
    if delivery.running {
        if delivery.kvo_active {
            log::info!(
                "KVO resumed: rate={}uL/min period={}us",
                settings.kvo_rate_ul_per_min,
                rate_step_period_us(settings.kvo_rate_ul_per_min, prescription.syringe)
            );
        } else {
            log::info!(
                "delivery started/resumed: remaining_steps={} approx_time={}s",
                delivery.remaining_steps,
                approx_delivery_seconds(
                    delivery.remaining_steps,
                    dispense_step_period_us(prescription)
                )
            );
        }
    } else if delivery.kvo_active {
        log::info!("KVO paused: delivered_total={}uL", dose.total_ul);
    } else {
        log::info!(
            "delivery paused: remaining_steps={} approx_time_left={}s",
            delivery.remaining_steps,
            approx_delivery_seconds(
                delivery.remaining_steps,
                dispense_step_period_us(prescription)
            )
        );
    }
}

/// Selects SpreadCycle only for non-KVO high-flow delivery or explicit settings override.
pub(super) fn delivery_run_spreadcycle_enabled(
    kvo_active: bool,
    rate_ul_per_min: f32,
    settings: &RuntimeSettings,
) -> bool {
    let rate_ml_per_h = rate_ul_per_min * 60.0 / 1000.0;
    !kvo_active
        && (settings.delivery_spreadcycle_enabled
            || rate_ml_per_h > FAST_FLOW_SPREADCYCLE_RATE_ML_PER_H)
}
