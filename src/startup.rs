use embassy_time::{Duration, Timer, with_timeout};
use esp_hal::i2c::master::{Error as I2cError, I2c};

use crate::app::{BenchMode, Inputs};
use crate::config::*;
use crate::dosing::{DeliveryAlert, DeliveryState, DoseAccumulator};
use crate::input::button_pressed;
use crate::persistent::PersistentConfig;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum StartupRecoveryChoice {
    ResumeSaved,
    DiscardAndHome,
}

/// Shows saved-operation recovery and waits for a resume/discard choice.
pub async fn prompt_boot_recovery(
    display: &mut crate::display::Display,
    frame: &mut crate::display::FrameBuffer,
    flush_buffer: &mut [u8],
    inputs: &Inputs,
    persistent_config: PersistentConfig,
) -> StartupRecoveryChoice {
    let prescription = persistent_config.prescription();
    let dose = DoseAccumulator::from_total_ul(persistent_config.dose_total_ul);
    let mut delivery = DeliveryState::new();

    delivery.running = false;
    delivery.remaining_steps = persistent_config.delivery_remaining_steps;
    delivery.dose_steps = persistent_config.delivery_dose_steps;
    delivery.delivered_steps_this_dose = persistent_config
        .delivery_dose_steps
        .saturating_sub(persistent_config.delivery_remaining_steps);
    delivery.kvo_active = persistent_config.delivery_kvo_active;
    delivery.alert = DeliveryAlert::None;

    crate::display::draw_dashboard_frame(
        frame,
        &prescription,
        persistent_config.delivery_spreadcycle_enabled,
    )
    .ok();
    crate::display::draw_dashboard_values(
        frame,
        BenchMode::Delivery,
        &dose,
        &delivery,
        &prescription,
        0,
        true,
        false,
    )
    .ok();
    crate::display::draw_recover_perfusion_alert_overlay(frame).ok();
    crate::display::flush_frame(display, frame, flush_buffer).await;

    loop {
        if button_pressed(&inputs.encoder_button) {
            while button_pressed(&inputs.encoder_button) {
                Timer::after_millis(BUTTON_RELEASE_POLL_MS).await;
            }
            return StartupRecoveryChoice::ResumeSaved;
        }

        if button_pressed(&inputs.retract_button) {
            while button_pressed(&inputs.retract_button) {
                Timer::after_millis(BUTTON_RELEASE_POLL_MS).await;
            }
            return StartupRecoveryChoice::DiscardAndHome;
        }

        Timer::after_millis(10).await;
    }
}

pub async fn update_startup_progress(
    display: &mut crate::display::Display,
    frame: &mut crate::display::FrameBuffer,
    flush_buffer: &mut [u8],
    status: &str,
    completed_steps: u32,
) {
    crate::display::draw_startup_progress_screen(frame, status, completed_steps, 6).ok();
    crate::display::flush_frame(display, frame, flush_buffer).await;
}

/// Shows a startup fault and blocks unless the caller allows bypass.
pub async fn startup_fault_or_continue_if(
    display: &mut crate::display::Display,
    frame: &mut crate::display::FrameBuffer,
    flush_buffer: &mut [u8],
    fault: &str,
    continue_after_fault: bool,
) {
    crate::display::draw_startup_fault_screen(frame, fault).ok();
    crate::display::flush_frame(display, frame, flush_buffer).await;

    if continue_after_fault {
        log::warn!(
            "startup debug override active; continuing after fault: {}",
            fault
        );
        Timer::after_millis(750).await;
        return;
    }

    loop {
        Timer::after_millis(250).await;
    }
}

pub fn startup_debug_bypass_enabled() -> bool {
    DEBUG_BYPASS_STARTUP_BLOCKS == 1
}

/// Waits until the STUSB4500 reports an accepted 20V USB-PD contract.
pub async fn wait_for_required_usb_pd_power(
    i2c: &mut I2c<'static, esp_hal::Async>,
    display: &mut crate::display::Display,
    frame: &mut crate::display::FrameBuffer,
    flush_buffer: &mut [u8],
) {
    loop {
        let (rdo_msb, object_position) = match read_stusb4500_rdo_object_position(i2c).await {
            Ok((_msb, pos)) if STUSB4500_ALLOWED_20V_OBJECT_POSITIONS.contains(&pos) => {
                esp_println::println!("USB PD OK: accepted 20V object position");
                return;
            }
            Ok((msb, pos)) => {
                esp_println::println!("USB PD blocked: expected pos 4/5, got {}", pos);
                (Some(msb), Some(pos))
            }
            Err(e) => {
                esp_println::println!("USB PD blocked: read failed {:?}", e);
                (None, None)
            }
        };

        crate::display::draw_power_warning_screen(frame, rdo_msb, object_position).ok();
        crate::display::flush_frame(display, frame, flush_buffer).await;

        if startup_debug_bypass_enabled() {
            log::warn!("startup debug bypass active; continuing without verified USB-C PD");
            Timer::after_millis(750).await;
            return;
        }

        Timer::after_millis(STUSB4500_POWER_CHECK_RETRY_MS).await;
    }
}

/// Reads the STUSB4500 request-data-object register and extracts the negotiated PDO index.
pub async fn read_stusb4500_rdo_object_position(
    i2c: &mut I2c<'static, esp_hal::Async>,
) -> Result<(u8, u8), I2cError> {
    let mut rdo_msb = [0u8; 1];
    i2c.write_read_async(STUSB4500_I2C_ADDR, &[STUSB4500_RDO_MSB_REG], &mut rdo_msb)
        .await?;
    let object_position = (rdo_msb[0] >> 4) & 0x07;
    Ok((rdo_msb[0], object_position))
}

/// Performs a short non-blocking I2C write to confirm a device responds at an address.
pub async fn probe_i2c_device(i2c: &mut I2c<'static, esp_hal::Async>, address: u8) -> bool {
    matches!(
        with_timeout(Duration::from_millis(50), i2c.write_async(address, &[0x00])).await,
        Ok(Ok(()))
    )
}
