use embassy_time::Timer;

use crate::{
    config::*,
    display::{Display, FrameBuffer, draw_homing_screen, flush_frame},
    dosing::steps_for_travel_mm,
    input::button_pressed,
    motor::{MotionDirection, MotorClient, MotorRunKind},
    tmc::Tmc2209Uart,
    types::Inputs,
};

use super::positioning::move_positioning_steps;

/// Homes to the rear limit switch, defines step zero, then moves forward to the backoff point.
pub async fn run_homing_sequence(
    display: &mut Display,
    frame: &mut FrameBuffer,
    flush_buffer: &mut [u8],
    motor: MotorClient,
    inputs: &Inputs,
    tmc: &mut Tmc2209Uart,
) -> i32 {
    let mut homing_steps = 0u32;
    let mut position_from_home_steps = 0i32;
    let max_steps = steps_for_travel_mm(HOMING_MAX_TRAVEL_MM).unwrap_or(MAX_STEPS_PER_DOSE);
    log::info!("homing started");
    tmc.set_spreadcycle_enabled(false).await;
    Timer::after_millis(2).await;

    let homing_command = if button_pressed(&inputs.homing_limit_switch) {
        None
    } else {
        Some(
            motor
                .run_auto(
                    MotorRunKind::Positioning,
                    MotionDirection::RetractTowardLoad,
                    HOMING_APPROACH_STEP_PERIOD_US,
                    Some(max_steps),
                )
                .await,
        )
    };

    while !button_pressed(&inputs.homing_limit_switch) && homing_steps < max_steps {
        if let Some(status) = motor.try_status() {
            if Some(status.command_id) == homing_command {
                homing_steps = status.command_steps;
                if status.completed_command_id == status.command_id {
                    break;
                }
            }
        }

        Timer::after_millis(2).await;
    }

    let limit_reached = button_pressed(&inputs.homing_limit_switch);

    if let Some(command_id) = homing_command {
        if limit_reached {
            let status = motor.stop_now().await;
            if status.command_id == command_id || status.completed_command_id == command_id {
                homing_steps = homing_steps.max(status.command_steps);
            }
        }
    }

    if !limit_reached {
        motor.disable();
        log::error!("homing failed before GPIO15 limit switch was reached");
        draw_homing_screen(frame, false, true).ok();
        flush_frame(display, frame, flush_buffer).await;
        loop {
            Timer::after_millis(250).await;
        }
    }

    match steps_for_travel_mm(HOMING_BACKOFF_MM) {
        Ok(backoff_steps) => {
            position_from_home_steps = 0;
            log::info!(
                "homing limit reached: steps={} moving to {}mm backoff target_steps={}",
                homing_steps,
                HOMING_BACKOFF_MM,
                backoff_steps
            );
            position_from_home_steps =
                move_to_homing_backoff_position(motor, position_from_home_steps).await;
            tmc.set_spreadcycle_enabled(USE_SPREADCYCLE_FOR_DELIVERY)
                .await;
            log::info!("homing backoff complete");
        }
        Err(error) => {
            motor.disable();
            log::error!("homing backoff rejected: {:?}", error);
        }
    }

    log::info!("homing complete: steps={}", homing_steps);
    position_from_home_steps
}

/// Moves to the standard post-home position without redefining the home coordinate.
pub(super) async fn move_to_homing_backoff_position(
    motor: MotorClient,
    current_position_steps: i32,
) -> i32 {
    let target_position_steps = steps_for_travel_mm(HOMING_BACKOFF_MM).unwrap_or(0) as i32;
    let move_steps = (target_position_steps - current_position_steps).unsigned_abs();

    if move_steps == 0 {
        motor.disable();
        return current_position_steps;
    }

    let direction = if target_position_steps > current_position_steps {
        MotionDirection::DispenseTowardEmpty
    } else {
        MotionDirection::RetractTowardLoad
    };
    move_positioning_steps(motor, direction, move_steps, HOMING_BACKOFF_STEP_PERIOD_US).await;
    target_position_steps
}
