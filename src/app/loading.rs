use super::positioning::move_positioning_steps_checked;
use crate::{
    config::*,
    display::{Display, FrameBuffer, draw_load_opening_screen, flush_frame},
    dosing::{
        Prescription, bolus_step_period_us, delivery_rate_ul_per_min, delivery_target_ul,
        dispense_step_period_us, rate_step_period_us, steps_for_travel_mm, syringe_load_travel_mm,
        syringe_plunger_travel_mm,
    },
    motor::{MotionDirection, MotorClient},
    tmc::Tmc2209Uart,
    types::Inputs,
};
use embassy_time::Timer;

/// Confirms syringe choice and moves the carriage to the load/open position for that syringe.
pub(super) async fn confirm_syringe_and_open_loader(
    display: &mut Display,
    frame: &mut FrameBuffer,
    flush_buffer: &mut [u8],
    motor: MotorClient,
    tmc: &mut Tmc2209Uart,
    inputs: &Inputs,
    prescription: &Prescription,
    carriage_position_steps: &mut i32,
) -> bool {
    log::info!(
        "syringe confirmed: {} ID={}mm nominal_volume={}uL",
        prescription.syringe.label,
        prescription.syringe.inner_diameter_mm,
        prescription.syringe.nominal_volume_ul
    );
    log_startup(prescription);

    draw_load_opening_screen(frame, prescription.syringe).ok();
    flush_frame(display, frame, flush_buffer).await;
    tmc.set_spreadcycle_enabled(false).await;
    Timer::after_millis(2).await;

    let load_travel_mm = syringe_load_travel_mm(prescription.syringe);
    let target_position_steps = match syringe_load_target_position_steps(prescription.syringe) {
        Some(position) => position.clamp(0, CARRIAGE_HARD_LIMIT_STEPS_FROM_HOME),
        None => match steps_for_travel_mm(load_travel_mm) {
            Ok(steps) => (steps as i32).min(CARRIAGE_HARD_LIMIT_STEPS_FROM_HOME),
            Err(error) => {
                motor.disable();
                log::error!("load opening rejected: {:?}", error);
                return false;
            }
        },
    };

    if target_position_steps == *carriage_position_steps {
        motor.disable();
        log::info!(
            "load opening skipped: syringe={} already at position_from_home_steps={}",
            prescription.syringe.label,
            *carriage_position_steps
        );
        return false;
    }

    let steps = (target_position_steps - *carriage_position_steps).unsigned_abs();
    let direction = if target_position_steps > *carriage_position_steps {
        MotionDirection::DispenseTowardEmpty
    } else {
        MotionDirection::RetractTowardLoad
    };

    if steps == 0 {
        motor.disable();
        return false;
    }

    {
        log::info!(
            "load opening started: syringe={} travel={}mm current_steps={} target_steps={} move_steps={}",
            prescription.syringe.label,
            load_travel_mm,
            *carriage_position_steps,
            target_position_steps,
            steps
        );
        if move_positioning_steps_checked(
            motor,
            direction,
            steps,
            SYRINGE_INSERT_APPROACH_STEP_PERIOD_US,
            &inputs.homing_limit_switch,
        )
        .await
        {
            *carriage_position_steps = 0;
            return true;
        }
        *carriage_position_steps = target_position_steps;
        log::info!("load opening complete");
    }

    false
}

/// Returns the absolute load/open target from home, using measured data when available.
pub(super) fn syringe_load_target_position_steps(
    syringe: crate::dosing::SyringeSpec,
) -> Option<i32> {
    if let Some(position) = syringe.measured_full_position_steps_from_home {
        return Some(position);
    }

    let empty_position = syringe.measured_empty_position_steps_from_home?;
    let plunger_steps = steps_for_travel_mm(syringe_plunger_travel_mm(syringe)).ok()? as i32;
    Some(empty_position.saturating_sub(plunger_steps))
}

/// Returns the absolute empty-syringe stop for delivery safety checks.
pub(super) fn syringe_empty_position_steps(prescription: &Prescription) -> i32 {
    prescription
        .syringe
        .measured_empty_position_steps_from_home
        .unwrap_or(CARRIAGE_HARD_LIMIT_STEPS_FROM_HOME)
        .clamp(0, CARRIAGE_HARD_LIMIT_STEPS_FROM_HOME)
}

/// Logs calibration and timing values for the selected syringe at the start of setup.
pub(super) fn log_startup(prescription: &Prescription) {
    log::info!(
        "bench calibration: syringe={} syringe_id={}mm area={}mm2 steps_per_mm={} uL_per_step={} scale={}",
        prescription.syringe.label,
        prescription.syringe.inner_diameter_mm,
        crate::dosing::syringe_area_mm2(prescription.syringe),
        crate::dosing::steps_per_mm(),
        crate::dosing::ul_per_step(prescription),
        CALIBRATION_UL_PER_STEP_SCALE
    );
    log::info!(
        "delivery timing: target_period={}us bolus_period={}us kvo_period={}us start_period={}us ramp_steps={} kvo_enabled={}",
        dispense_step_period_us(prescription),
        bolus_step_period_us(prescription),
        rate_step_period_us(KVO_RATE_UL_PER_MIN, prescription.syringe),
        DISPENSE_START_STEP_PERIOD_US,
        DISPENSE_RAMP_STEPS,
        KVO_ENABLED
    );
    log::info!(
        "delivery prescription: vtbi_mode={} target={}uL rate={}uL/min vtbi_time={}min",
        USE_VTBI_TIME_MODE,
        delivery_target_ul(prescription),
        delivery_rate_ul_per_min(prescription),
        prescription.infusion_time_min
    );
    log::info!(
        "controls: encoder OK pause/resume, GPIO16 hold settings, GPIO17 press bolus menu, GPIO17 hold direct bolus"
    );
}
