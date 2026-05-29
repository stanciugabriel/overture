use crate::{
    config::*,
    input::button_pressed,
    motor::{MotionDirection, MotorClient, MotorRunKind},
};
use embassy_time::Timer;
use esp_hal::gpio::Input;

/// Starts or continues a held manual positioning move while respecting carriage bounds.
pub(super) async fn start_or_update_positioning_hold(
    motor: MotorClient,
    direction: MotionDirection,
    period_us: u64,
    active_command: &mut Option<u32>,
    seen_steps: &mut u32,
    carriage_position_steps: &mut i32,
) {
    if period_us < MIN_STEP_PERIOD_US {
        motor.disable();
        log::error!("unsafe positioning hold period rejected: {} us", period_us);
        return;
    }

    if active_command.is_none() {
        let max_steps = match direction {
            MotionDirection::DispenseTowardEmpty => CARRIAGE_HARD_LIMIT_STEPS_FROM_HOME
                .saturating_sub(*carriage_position_steps)
                .max(0) as u32,
            MotionDirection::RetractTowardLoad => (*carriage_position_steps).max(0) as u32,
        };
        if max_steps == 0 {
            motor.disable();
            return;
        }
        *active_command = Some(
            motor
                .run_auto(
                    MotorRunKind::Positioning,
                    direction,
                    period_us,
                    Some(max_steps),
                )
                .await,
        );
        *seen_steps = 0;
    }

    let (steps, completed) =
        apply_positioning_motor_status(motor, active_command, seen_steps, carriage_position_steps);
    if steps == 0 && !completed {
        Timer::after_millis(2).await;
    }
}

/// Stops an active positioning command and syncs software position from motor status.
pub(super) async fn stop_positioning_command(
    motor: MotorClient,
    active_command: &mut Option<u32>,
    seen_steps: &mut u32,
    carriage_position_steps: &mut i32,
) {
    if active_command.is_some() {
        let status = motor.stop_now().await;
        *carriage_position_steps = status
            .position_steps
            .clamp(0, CARRIAGE_HARD_LIMIT_STEPS_FROM_HOME);
        *active_command = None;
        *seen_steps = 0;
    } else {
        motor.disable();
    }
}

/// Pulls completed positioning steps out of the motor task and updates carriage position.
pub(super) fn apply_positioning_motor_status(
    motor: MotorClient,
    active_command: &mut Option<u32>,
    seen_steps: &mut u32,
    carriage_position_steps: &mut i32,
) -> (u32, bool) {
    let mut moved_now = 0u32;
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
            *seen_steps = current_steps;
            moved_now = moved_now.saturating_add(delta);
            *carriage_position_steps = status
                .position_steps
                .clamp(0, CARRIAGE_HARD_LIMIT_STEPS_FROM_HOME);
        }

        if status.completed_command_id == command_id {
            *active_command = None;
            *seen_steps = 0;
            completed = true;
        }
    }

    (moved_now, completed)
}

/// Runs a blocking positioning move for a known number of steps.
pub(super) async fn move_positioning_steps(
    motor: MotorClient,
    direction: MotionDirection,
    steps: u32,
    period_us: u64,
) {
    motor.move_steps_auto(direction, steps, period_us).await;
}

/// Runs a positioning move but aborts if the homing switch is hit during travel.
pub(super) async fn move_positioning_steps_checked(
    motor: MotorClient,
    direction: MotionDirection,
    steps: u32,
    period_us: u64,
    homing_limit_switch: &Input<'_>,
) -> bool {
    let command_id = motor
        .run_auto(MotorRunKind::Positioning, direction, period_us, Some(steps))
        .await;
    loop {
        if button_pressed(homing_limit_switch) {
            motor.disable();
            return true;
        }
        if let Some(status) = motor.try_status() {
            if status.completed_command_id == command_id {
                return false;
            }
        }
        Timer::after_millis(2).await;
    }
}

/// Applies an expected manual nudge to the software carriage position with hard-limit clamps.
pub(super) fn apply_position_delta(
    current_position_steps: i32,
    direction: MotionDirection,
    steps: u32,
) -> i32 {
    match direction {
        MotionDirection::DispenseTowardEmpty => current_position_steps
            .saturating_add(steps as i32)
            .min(CARRIAGE_HARD_LIMIT_STEPS_FROM_HOME),
        MotionDirection::RetractTowardLoad => {
            current_position_steps.saturating_sub(steps as i32).max(0)
        }
    }
}

/// Retracts slightly before syringe removal so the plunger is no longer under pressure.
pub(super) async fn relieve_syringe_pressure(
    motor: MotorClient,
    carriage_position_steps: &mut i32,
) {
    let backlash_steps =
        SYRINGE_REMOVAL_BACKLASH_STEPS.min((*carriage_position_steps).max(0) as u32);

    if backlash_steps == 0 {
        motor.disable();
        return;
    }

    move_positioning_steps(
        motor,
        MotionDirection::RetractTowardLoad,
        backlash_steps,
        SYRINGE_REMOVAL_BACKLASH_STEP_PERIOD_US,
    )
    .await;
    *carriage_position_steps = (*carriage_position_steps).saturating_sub(backlash_steps as i32);
}
