use embassy_time::{Duration, Instant, Timer};

mod delivery;
mod homing;
mod loading;
mod music;
mod navigation;
mod persistence;
pub(super) mod positioning;
pub use crate::types::{AppScreen, BenchMode, Inputs};
pub use homing::run_homing_sequence;

use crate::{
    config::*,
    display::{
        Display, FrameBuffer, draw_bolus_administered_alert_overlay,
        draw_bolus_delivered_alert_overlay, draw_bolus_setup_screen, draw_dashboard_frame,
        draw_dashboard_values, draw_direct_bolus_overlay, draw_drug_select_screen,
        draw_homing_limit_alert_screen, draw_homing_screen, draw_load_adjust_screen,
        draw_nfc_syringe_detected_screen, draw_patient_weight_screen, draw_prime_screen,
        draw_remove_syringe_prompt_screen, draw_settings_screen, draw_setup_controls_help_screen,
        draw_setup_screen, draw_syringe_select_screen, flush_frame,
    },
    dosing::{
        DEFAULT_SYRINGE_INDEX, DRUG_LIBRARY, DeliveryAlert, DeliveryState, DoseAccumulator,
        Prescription, SYRINGE_PRESETS, chopper_mode_name, delivery_rate_ul_per_min,
        dispense_step_period_us, rate_step_period_us, steps_for_volume_ul, ul_per_step,
    },
    input::{EncoderState, button_pressed},
    motor::{MotionDirection, MotorClient, MotorRunKind},
    nfc::{NfcTag, Pn532},
    persistent::{PersistentConfig, PersistentStore},
    tmc::Tmc2209Uart,
    types::{RuntimeSettings, SetupContext},
};
use delivery::*;
use homing::move_to_homing_backoff_position;
use loading::*;
use music::*;
use navigation::*;
use persistence::*;
use positioning::*;

async fn handle_homing_limit_recalibration(
    display: &mut Display,
    frame: &mut FrameBuffer,
    flush_buffer: &mut [u8],
    motor: MotorClient,
    store: &mut PersistentStore,
    delivery: &mut DeliveryState,
    carriage_position_steps: &mut i32,
    syringe_selected: usize,
    syringe_mounted: bool,
    prescription: &Prescription,
    settings: &RuntimeSettings,
    dose: &DoseAccumulator,
    bolus_volume_ul: f32,
    bolus_rate_ul_per_min: f32,
) {
    motor.disable();
    delivery.stop();
    delivery.alert = DeliveryAlert::DosingFault;
    *carriage_position_steps = 0;
    save_persistent_config(
        store,
        syringe_selected,
        syringe_mounted,
        *carriage_position_steps,
        prescription,
        settings,
        delivery,
        dose,
        bolus_volume_ul,
        bolus_rate_ul_per_min,
    );
    log::warn!("homing limit switch hit: motor stopped and position reset to zero");
    draw_homing_limit_alert_screen(frame).ok();
    flush_frame(display, frame, flush_buffer).await;
}

pub async fn ui_task(
    display: &mut Display,
    frame: &mut FrameBuffer,
    flush_buffer: &mut [u8],
    motor: MotorClient,
    inputs: Inputs,
    mut nfc: Pn532,
    mut tmc: Tmc2209Uart,
    mut store: PersistentStore,
    saved_config: PersistentConfig,
    mut carriage_position_steps: i32,
    mut syringe_mounted: bool,
    resume_saved_operation: bool,
) -> ! {
    motor.disable();

    let mut encoder =
        EncoderState::new(&inputs.encoder_a, &inputs.encoder_b, &inputs.encoder_button);
    let mut dose = if resume_saved_operation {
        DoseAccumulator::from_total_ul(saved_config.dose_total_ul)
    } else {
        DoseAccumulator::new()
    };
    let mut prescription = saved_config.prescription();
    let mut app_screen = AppScreen::Syringe;
    let mut mode = if resume_saved_operation {
        BenchMode::Delivery
    } else {
        BenchMode::Manual
    };
    let mut delivery = DeliveryState::new();
    if resume_saved_operation {
        delivery.running = saved_config.delivery_running;
        delivery.remaining_steps = saved_config.delivery_remaining_steps;
        delivery.dose_steps = saved_config.delivery_dose_steps;
        delivery.delivered_steps_this_dose = saved_config
            .delivery_dose_steps
            .saturating_sub(saved_config.delivery_remaining_steps);
        delivery.ramp_progress_steps = delivery.delivered_steps_this_dose.min(DISPENSE_RAMP_STEPS);
        delivery.kvo_active = saved_config.delivery_kvo_active;
        delivery.alert = if delivery.kvo_active {
            DeliveryAlert::KvoRunning
        } else {
            DeliveryAlert::Standby
        };
    }
    let mut settings = RuntimeSettings::from_config(saved_config);
    tmc.set_spreadcycle_enabled(settings.delivery_spreadcycle_enabled)
        .await;
    let mut syringe_selected = saved_config.syringe_index;
    let mut drug_selected = saved_config
        .drug_index
        .map(|index| index + 1)
        .unwrap_or(0)
        .min(DRUG_LIBRARY.len());
    let mut weight_digit_selected = 0usize;
    let mut weight_editing = false;
    let mut setup_selected = FIRST_EDITABLE_SETUP_ITEM;
    let mut setup_editing = false;
    let mut settings_selected = 0usize;
    let mut settings_editing = false;
    let mut bolus_selected = 0usize;
    let mut bolus_editing = false;
    let mut bolus_volume_ul = saved_config.bolus_volume_ul;
    let mut bolus_rate_ul_per_min = saved_config.bolus_rate_ul_per_min;
    let mut resume_after_bolus = false;

    if syringe_mounted && resume_saved_operation {
        app_screen = AppScreen::Pump;
        draw_dashboard_frame(frame, &prescription, settings.delivery_spreadcycle_enabled).ok();
        draw_dashboard_values(frame, mode, &dose, &delivery, &prescription, 0, true, false).ok();
    } else if syringe_mounted {
        app_screen = AppScreen::RemoveSyringePrompt;
        draw_remove_syringe_prompt_screen(frame).ok();
    } else {
        draw_syringe_select_screen(frame, syringe_selected).ok();
    }
    flush_frame(display, frame, flush_buffer).await;
    let mut last_retract_pressed = false;
    let mut last_homing_limit_pressed = false;
    let mut bolus_press_started: Option<Instant> = None;
    let mut bolus_hold_active = false;
    let mut direct_bolus_window_ul = 0.0f32;
    let mut direct_bolus_total_ul = 0.0f32;
    let mut direct_bolus_wait_release = false;
    let mut direct_bolus_overlay_visible = false;
    let mut direct_bolus_summary_ul: Option<f32> = None;
    let mut configured_bolus_summary_ul: Option<f32> = None;
    let mut back_press_started: Option<Instant> = None;
    let mut back_hold_active = false;
    let mut last_ui_refresh = Instant::now();
    let mut last_nfc_poll = Instant::now() - Duration::from_millis(PN532_POLL_INTERVAL_MS);
    let mut flow_phase = 0usize;
    let mut alarm_flash_on = true;
    let mut next_delivery_flash_checkpoint_steps = next_delivery_flash_checkpoint(&delivery);
    let mut last_delivery_flash_save = Instant::now();
    let mut active_delivery_motor_command: Option<u32> = None;
    let mut active_delivery_motor_seen_steps = 0u32;
    let mut active_direct_bolus_command: Option<u32> = None;
    let mut active_direct_bolus_seen_steps = 0u32;
    let mut active_configured_bolus_command: Option<u32> = None;
    let mut active_configured_bolus_seen_steps = 0u32;
    let mut active_configured_bolus_total_ul = 0.0f32;
    let mut active_load_fine_command: Option<u32> = None;
    let mut active_load_fine_seen_steps = 0u32;
    let mut active_prime_command: Option<u32> = None;
    let mut active_prime_seen_steps = 0u32;

    loop {
        let dispense_pressed = button_pressed(&inputs.dispense_button);
        let retract_pressed = button_pressed(&inputs.retract_button);
        let homing_limit_pressed = button_pressed(&inputs.homing_limit_switch);
        let retract_edge = retract_pressed && !last_retract_pressed;
        let homing_limit_edge = homing_limit_pressed && !last_homing_limit_pressed;
        let (encoder_delta, encoder_press) =
            encoder.poll(&inputs.encoder_a, &inputs.encoder_b, &inputs.encoder_button);
        let mut redraw = false;
        let mut redraw_dashboard_frame = false;

        if homing_limit_edge && !matches!(app_screen, AppScreen::HomingLimitAlert) {
            handle_homing_limit_recalibration(
                display,
                frame,
                flush_buffer,
                motor,
                &mut store,
                &mut delivery,
                &mut carriage_position_steps,
                syringe_selected,
                syringe_mounted,
                &prescription,
                &settings,
                &dose,
                bolus_volume_ul,
                bolus_rate_ul_per_min,
            )
            .await;
            app_screen = AppScreen::HomingLimitAlert;
            last_retract_pressed = retract_pressed;
            last_homing_limit_pressed = homing_limit_pressed;
            continue;
        }

        if matches!(app_screen, AppScreen::HomingLimitAlert) {
            if encoder_press && !homing_limit_pressed {
                carriage_position_steps = move_to_homing_backoff_position(motor, 0).await;
                save_persistent_config(
                    &mut store,
                    syringe_selected,
                    syringe_mounted,
                    carriage_position_steps,
                    &prescription,
                    &settings,
                    &delivery,
                    &dose,
                    bolus_volume_ul,
                    bolus_rate_ul_per_min,
                );
                app_screen = if syringe_mounted {
                    AppScreen::RemoveSyringePrompt
                } else {
                    AppScreen::Syringe
                };
                if syringe_mounted {
                    draw_remove_syringe_prompt_screen(frame).ok();
                } else {
                    draw_syringe_select_screen(frame, syringe_selected).ok();
                }
                flush_frame(display, frame, flush_buffer).await;
            } else {
                Timer::after_millis(5).await;
            }
            last_retract_pressed = retract_pressed;
            last_homing_limit_pressed = homing_limit_pressed;
            continue;
        }

        if matches!(app_screen, AppScreen::Syringe) {
            if last_nfc_poll.elapsed() >= Duration::from_millis(PN532_POLL_INTERVAL_MS) {
                last_nfc_poll = Instant::now();
                match nfc.poll_known_tag().await {
                    Ok(Some(NfcTag::SyringePreset(index))) => {
                        syringe_selected = index;
                        prescription.drug_index = None;
                        drug_selected = 0;
                        app_screen = AppScreen::NfcSyringeDetected;
                        draw_nfc_syringe_detected_screen(
                            frame,
                            SYRINGE_PRESETS[syringe_selected],
                            prescription.selected_drug(),
                        )
                        .ok();
                        flush_frame(display, frame, flush_buffer).await;
                        last_retract_pressed = retract_pressed;
                        continue;
                    }
                    Ok(Some(NfcTag::SyringePresetWithDrug {
                        syringe_index,
                        drug_index,
                    })) => {
                        syringe_selected = syringe_index;
                        prescription.drug_index = Some(drug_index);
                        drug_selected = (drug_index + 1).min(DRUG_LIBRARY.len());
                        app_screen = AppScreen::NfcSyringeDetected;
                        draw_nfc_syringe_detected_screen(
                            frame,
                            SYRINGE_PRESETS[syringe_selected],
                            prescription.selected_drug(),
                        )
                        .ok();
                        flush_frame(display, frame, flush_buffer).await;
                        last_retract_pressed = retract_pressed;
                        continue;
                    }
                    Ok(None) => {}
                    Err(error) => {
                        log::debug!("PN532 poll skipped: {:?}", error);
                    }
                }
            }

            if encoder_delta > 0 {
                syringe_selected = next_syringe_item(syringe_selected);
                redraw = true;
            } else if encoder_delta < 0 {
                syringe_selected = previous_syringe_item(syringe_selected);
                redraw = true;
            }

            if encoder_press {
                prescription.syringe = SYRINGE_PRESETS[syringe_selected];
                prescription.drug_index = None;
                drug_selected = 0;
                app_screen = AppScreen::Drug;
                draw_drug_select_screen(frame, drug_selected).ok();
                flush_frame(display, frame, flush_buffer).await;
                last_retract_pressed = retract_pressed;
                continue;
            }

            if redraw {
                draw_syringe_select_screen(frame, syringe_selected).ok();
                flush_frame(display, frame, flush_buffer).await;
            } else {
                Timer::after_millis(5).await;
            }
            last_retract_pressed = retract_pressed;
            continue;
        }

        if matches!(app_screen, AppScreen::Drug) {
            if retract_edge {
                app_screen = AppScreen::Syringe;
                draw_syringe_select_screen(frame, syringe_selected).ok();
                flush_frame(display, frame, flush_buffer).await;
                last_retract_pressed = retract_pressed;
                continue;
            }

            if encoder_delta > 0 {
                drug_selected = next_drug_item(drug_selected);
                redraw = true;
            } else if encoder_delta < 0 {
                drug_selected = previous_drug_item(drug_selected);
                redraw = true;
            }

            if encoder_press {
                prescription.drug_index = if drug_selected == 0 {
                    None
                } else {
                    Some(drug_selected - 1)
                };
                if prescription.drug_index.is_some() {
                    prescription.sync_dose_from_flow();
                    weight_digit_selected = 1;
                    weight_editing = false;
                    app_screen = AppScreen::PatientWeight;
                    draw_patient_weight_screen(
                        frame,
                        prescription.patient_weight_kg,
                        weight_digit_selected,
                        weight_editing,
                    )
                    .ok();
                    flush_frame(display, frame, flush_buffer).await;
                    last_retract_pressed = retract_pressed;
                    continue;
                }
                let home_hit = confirm_syringe_and_open_loader(
                    display,
                    frame,
                    flush_buffer,
                    motor,
                    &mut tmc,
                    &inputs,
                    &prescription,
                    &mut carriage_position_steps,
                )
                .await;
                if home_hit {
                    handle_homing_limit_recalibration(
                        display,
                        frame,
                        flush_buffer,
                        motor,
                        &mut store,
                        &mut delivery,
                        &mut carriage_position_steps,
                        syringe_selected,
                        syringe_mounted,
                        &prescription,
                        &settings,
                        &dose,
                        bolus_volume_ul,
                        bolus_rate_ul_per_min,
                    )
                    .await;
                    app_screen = AppScreen::HomingLimitAlert;
                    last_retract_pressed = retract_pressed;
                    last_homing_limit_pressed = true;
                    continue;
                }
                save_persistent_config(
                    &mut store,
                    syringe_selected,
                    syringe_mounted,
                    carriage_position_steps,
                    &prescription,
                    &settings,
                    &delivery,
                    &dose,
                    bolus_volume_ul,
                    bolus_rate_ul_per_min,
                );
                app_screen = AppScreen::LoadAdjust;
                draw_load_adjust_screen(frame, prescription.syringe).ok();
                flush_frame(display, frame, flush_buffer).await;
                last_retract_pressed = retract_pressed;
                continue;
            }

            if redraw {
                draw_drug_select_screen(frame, drug_selected).ok();
                flush_frame(display, frame, flush_buffer).await;
            } else {
                Timer::after_millis(5).await;
            }
            last_retract_pressed = retract_pressed;
            continue;
        }

        if matches!(app_screen, AppScreen::PatientWeight) {
            if retract_edge {
                app_screen = AppScreen::Drug;
                weight_editing = false;
                draw_drug_select_screen(frame, drug_selected).ok();
                flush_frame(display, frame, flush_buffer).await;
                last_retract_pressed = retract_pressed;
                continue;
            }

            if encoder_delta != 0 {
                if weight_editing && weight_digit_selected < WEIGHT_DIGITS {
                    apply_weight_digit_delta(
                        &mut prescription.patient_weight_kg,
                        weight_digit_selected,
                        encoder_delta,
                    );
                    prescription.sync_dose_from_flow();
                } else if encoder_delta > 0 {
                    weight_digit_selected = next_weight_item(weight_digit_selected);
                } else {
                    weight_digit_selected = previous_weight_item(weight_digit_selected);
                }
                redraw = true;
            }

            if encoder_press {
                if weight_digit_selected < WEIGHT_DIGITS {
                    weight_editing = !weight_editing;
                    redraw = true;
                } else {
                    weight_editing = false;
                    let home_hit = confirm_syringe_and_open_loader(
                        display,
                        frame,
                        flush_buffer,
                        motor,
                        &mut tmc,
                        &inputs,
                        &prescription,
                        &mut carriage_position_steps,
                    )
                    .await;
                    if home_hit {
                        handle_homing_limit_recalibration(
                            display,
                            frame,
                            flush_buffer,
                            motor,
                            &mut store,
                            &mut delivery,
                            &mut carriage_position_steps,
                            syringe_selected,
                            syringe_mounted,
                            &prescription,
                            &settings,
                            &dose,
                            bolus_volume_ul,
                            bolus_rate_ul_per_min,
                        )
                        .await;
                        app_screen = AppScreen::HomingLimitAlert;
                        last_retract_pressed = retract_pressed;
                        last_homing_limit_pressed = true;
                        continue;
                    }
                    save_persistent_config(
                        &mut store,
                        syringe_selected,
                        syringe_mounted,
                        carriage_position_steps,
                        &prescription,
                        &settings,
                        &delivery,
                        &dose,
                        bolus_volume_ul,
                        bolus_rate_ul_per_min,
                    );
                    app_screen = AppScreen::LoadAdjust;
                    draw_load_adjust_screen(frame, prescription.syringe).ok();
                    flush_frame(display, frame, flush_buffer).await;
                    last_retract_pressed = retract_pressed;
                    continue;
                }
            }

            if redraw {
                draw_patient_weight_screen(
                    frame,
                    prescription.patient_weight_kg,
                    weight_digit_selected,
                    weight_editing,
                )
                .ok();
                flush_frame(display, frame, flush_buffer).await;
            } else {
                Timer::after_millis(5).await;
            }
            last_retract_pressed = retract_pressed;
            continue;
        }

        if matches!(app_screen, AppScreen::NfcSyringeDetected) {
            if retract_edge {
                app_screen = AppScreen::Syringe;
                prescription.drug_index = None;
                drug_selected = 0;
                draw_syringe_select_screen(frame, syringe_selected).ok();
                flush_frame(display, frame, flush_buffer).await;
            } else if encoder_press {
                prescription.syringe = SYRINGE_PRESETS[syringe_selected];
                let home_hit = confirm_syringe_and_open_loader(
                    display,
                    frame,
                    flush_buffer,
                    motor,
                    &mut tmc,
                    &inputs,
                    &prescription,
                    &mut carriage_position_steps,
                )
                .await;
                if home_hit {
                    handle_homing_limit_recalibration(
                        display,
                        frame,
                        flush_buffer,
                        motor,
                        &mut store,
                        &mut delivery,
                        &mut carriage_position_steps,
                        syringe_selected,
                        syringe_mounted,
                        &prescription,
                        &settings,
                        &dose,
                        bolus_volume_ul,
                        bolus_rate_ul_per_min,
                    )
                    .await;
                    app_screen = AppScreen::HomingLimitAlert;
                    last_retract_pressed = retract_pressed;
                    last_homing_limit_pressed = true;
                    continue;
                }
                save_persistent_config(
                    &mut store,
                    syringe_selected,
                    syringe_mounted,
                    carriage_position_steps,
                    &prescription,
                    &settings,
                    &delivery,
                    &dose,
                    bolus_volume_ul,
                    bolus_rate_ul_per_min,
                );
                app_screen = AppScreen::LoadAdjust;
                draw_load_adjust_screen(frame, prescription.syringe).ok();
                flush_frame(display, frame, flush_buffer).await;
            } else {
                Timer::after_millis(5).await;
            }
            last_retract_pressed = retract_pressed;
            continue;
        }

        if matches!(app_screen, AppScreen::LoadAdjust) {
            if retract_edge {
                stop_positioning_command(
                    motor,
                    &mut active_load_fine_command,
                    &mut active_load_fine_seen_steps,
                    &mut carriage_position_steps,
                )
                .await;
                app_screen = if prescription.drug_index.is_some() {
                    AppScreen::PatientWeight
                } else {
                    AppScreen::Drug
                };
                save_persistent_config(
                    &mut store,
                    syringe_selected,
                    syringe_mounted,
                    carriage_position_steps,
                    &prescription,
                    &settings,
                    &delivery,
                    &dose,
                    bolus_volume_ul,
                    bolus_rate_ul_per_min,
                );
                if matches!(app_screen, AppScreen::PatientWeight) {
                    draw_patient_weight_screen(
                        frame,
                        prescription.patient_weight_kg,
                        weight_digit_selected,
                        weight_editing,
                    )
                    .ok();
                } else {
                    draw_drug_select_screen(frame, drug_selected).ok();
                }
                flush_frame(display, frame, flush_buffer).await;
                last_retract_pressed = retract_pressed;
                continue;
            }

            if encoder_press {
                stop_positioning_command(
                    motor,
                    &mut active_load_fine_command,
                    &mut active_load_fine_seen_steps,
                    &mut carriage_position_steps,
                )
                .await;
                syringe_mounted = true;
                save_persistent_config(
                    &mut store,
                    syringe_selected,
                    syringe_mounted,
                    carriage_position_steps,
                    &prescription,
                    &settings,
                    &delivery,
                    &dose,
                    bolus_volume_ul,
                    bolus_rate_ul_per_min,
                );
                app_screen = AppScreen::Prime;
                draw_prime_screen(frame, prescription.syringe).ok();
                flush_frame(display, frame, flush_buffer).await;
                last_retract_pressed = retract_pressed;
                continue;
            }

            if dispense_pressed {
                start_or_update_positioning_hold(
                    motor,
                    MotionDirection::DispenseTowardEmpty,
                    LOAD_FINE_ADVANCE_STEP_PERIOD_US,
                    &mut active_load_fine_command,
                    &mut active_load_fine_seen_steps,
                    &mut carriage_position_steps,
                )
                .await;
            } else if encoder_delta != 0 {
                stop_positioning_command(
                    motor,
                    &mut active_load_fine_command,
                    &mut active_load_fine_seen_steps,
                    &mut carriage_position_steps,
                )
                .await;
                let direction = if encoder_delta > 0 {
                    MotionDirection::DispenseTowardEmpty
                } else {
                    MotionDirection::RetractTowardLoad
                };
                let move_steps = MANUAL_POSITION_NUDGE_STEPS * encoder_delta.unsigned_abs();
                move_positioning_steps(motor, direction, move_steps, LOAD_APPROACH_STEP_PERIOD_US)
                    .await;
                carriage_position_steps =
                    apply_position_delta(carriage_position_steps, direction, move_steps);
            } else {
                stop_positioning_command(
                    motor,
                    &mut active_load_fine_command,
                    &mut active_load_fine_seen_steps,
                    &mut carriage_position_steps,
                )
                .await;
                Timer::after_millis(5).await;
            }
            last_retract_pressed = retract_pressed;
            continue;
        }

        if matches!(app_screen, AppScreen::Prime) {
            if retract_edge {
                stop_positioning_command(
                    motor,
                    &mut active_prime_command,
                    &mut active_prime_seen_steps,
                    &mut carriage_position_steps,
                )
                .await;
                app_screen = AppScreen::LoadAdjust;
                draw_load_adjust_screen(frame, prescription.syringe).ok();
                flush_frame(display, frame, flush_buffer).await;
                last_retract_pressed = retract_pressed;
                continue;
            }

            if encoder_press {
                stop_positioning_command(
                    motor,
                    &mut active_prime_command,
                    &mut active_prime_seen_steps,
                    &mut carriage_position_steps,
                )
                .await;
                app_screen = AppScreen::Setup;
                setup_selected = first_setup_item(&prescription);
                setup_editing = false;
                draw_setup_screen(frame, &prescription, setup_selected, setup_editing).ok();
                flush_frame(display, frame, flush_buffer).await;
                last_retract_pressed = retract_pressed;
                continue;
            }

            if dispense_pressed {
                start_or_update_positioning_hold(
                    motor,
                    MotionDirection::DispenseTowardEmpty,
                    PRIME_STEP_PERIOD_US,
                    &mut active_prime_command,
                    &mut active_prime_seen_steps,
                    &mut carriage_position_steps,
                )
                .await;
            } else {
                stop_positioning_command(
                    motor,
                    &mut active_prime_command,
                    &mut active_prime_seen_steps,
                    &mut carriage_position_steps,
                )
                .await;
                Timer::after_millis(5).await;
            }
            last_retract_pressed = retract_pressed;
            continue;
        }

        if matches!(app_screen, AppScreen::Setup) {
            if retract_edge {
                setup_editing = false;
                app_screen = AppScreen::Prime;
                draw_prime_screen(frame, prescription.syringe).ok();
                flush_frame(display, frame, flush_buffer).await;
                last_retract_pressed = retract_pressed;
                continue;
            }

            let mut setup = SetupContext {
                app_screen: &mut app_screen,
                mode: &mut mode,
                selected: &mut setup_selected,
                editing: &mut setup_editing,
                redraw: &mut redraw,
            };
            handle_setup_input(&mut prescription, encoder_delta, encoder_press, &mut setup);

            if matches!(app_screen, AppScreen::Pump) {
                if prepare_or_fault(&mut dose, &mut delivery, &prescription).is_ok() {
                    tmc.set_spreadcycle_enabled(delivery_run_spreadcycle_enabled(
                        false,
                        delivery_rate_ul_per_min(&prescription),
                        &settings,
                    ))
                    .await;
                    delivery.running = true;
                    mode = BenchMode::Delivery;
                    next_delivery_flash_checkpoint_steps =
                        next_delivery_flash_checkpoint(&delivery);
                    last_delivery_flash_save = Instant::now();
                    log::info!("infusion started immediately after setup confirmation");
                }
                save_persistent_config(
                    &mut store,
                    syringe_selected,
                    syringe_mounted,
                    carriage_position_steps,
                    &prescription,
                    &settings,
                    &delivery,
                    &dose,
                    bolus_volume_ul,
                    bolus_rate_ul_per_min,
                );
                draw_dashboard_frame(frame, &prescription, settings.delivery_spreadcycle_enabled)
                    .ok();
                draw_dashboard_values(frame, mode, &dose, &delivery, &prescription, 0, true, false)
                    .ok();
                flush_frame(display, frame, flush_buffer).await;
                last_ui_refresh = Instant::now();
                last_retract_pressed = retract_pressed;
                continue;
            }

            if redraw {
                draw_setup_screen(frame, &prescription, setup_selected, setup_editing).ok();
                flush_frame(display, frame, flush_buffer).await;
            } else {
                Timer::after_millis(5).await;
            }
            last_retract_pressed = retract_pressed;
            continue;
        }

        if matches!(app_screen, AppScreen::Settings) {
            if !retract_pressed {
                back_hold_active = false;
                back_press_started = None;
            }

            if retract_pressed && !back_hold_active {
                settings_editing = false;
                app_screen = AppScreen::Pump;
                delivery.alert = DeliveryAlert::Standby;
                back_press_started = None;
                save_persistent_config(
                    &mut store,
                    syringe_selected,
                    syringe_mounted,
                    carriage_position_steps,
                    &prescription,
                    &settings,
                    &delivery,
                    &dose,
                    bolus_volume_ul,
                    bolus_rate_ul_per_min,
                );
                draw_dashboard_frame(frame, &prescription, settings.delivery_spreadcycle_enabled)
                    .ok();
                draw_dashboard_values(frame, mode, &dose, &delivery, &prescription, 0, true, false)
                    .ok();
                flush_frame(display, frame, flush_buffer).await;
                last_ui_refresh = Instant::now();
                last_retract_pressed = retract_pressed;
                continue;
            }

            let previous_delivery_spreadcycle = settings.delivery_spreadcycle_enabled;
            let settings_action = handle_settings_input(
                &mut settings,
                encoder_delta,
                encoder_press,
                &mut settings_selected,
                &mut settings_editing,
                &mut app_screen,
                &mut redraw,
            );
            if previous_delivery_spreadcycle != settings.delivery_spreadcycle_enabled {
                tmc.set_spreadcycle_enabled(settings.delivery_spreadcycle_enabled)
                    .await;
                save_persistent_config(
                    &mut store,
                    syringe_selected,
                    syringe_mounted,
                    carriage_position_steps,
                    &prescription,
                    &settings,
                    &delivery,
                    &dose,
                    bolus_volume_ul,
                    bolus_rate_ul_per_min,
                );
                log::info!(
                    "delivery chopper mode set from settings: {}",
                    chopper_mode_name(settings.delivery_spreadcycle_enabled)
                );
            }

            if matches!(settings_action, SettingsAction::EndPerfusion) {
                settings_editing = false;
                let delivered_now = apply_delivery_motor_status(
                    motor,
                    &mut dose,
                    &mut delivery,
                    &prescription,
                    &settings,
                    &mut active_delivery_motor_command,
                    &mut active_delivery_motor_seen_steps,
                );
                if delivered_now > 0 {
                    carriage_position_steps = carriage_position_steps
                        .saturating_add(delivered_now as i32)
                        .min(CARRIAGE_HARD_LIMIT_STEPS_FROM_HOME);
                }
                if active_delivery_motor_command.take().is_some() {
                    motor.stop_now().await;
                    active_delivery_motor_seen_steps = 0;
                } else {
                    motor.disable();
                }
                delivery.running = false;
                delivery.kvo_active = false;
                delivery.remaining_steps = 0;
                delivery.alert = DeliveryAlert::EndOfInfusion;
                app_screen = AppScreen::Pump;
                tmc.set_spreadcycle_enabled(settings.delivery_spreadcycle_enabled)
                    .await;
                save_persistent_config(
                    &mut store,
                    syringe_selected,
                    syringe_mounted,
                    carriage_position_steps,
                    &prescription,
                    &settings,
                    &delivery,
                    &dose,
                    bolus_volume_ul,
                    bolus_rate_ul_per_min,
                );
                draw_dashboard_frame(frame, &prescription, settings.delivery_spreadcycle_enabled)
                    .ok();
                draw_dashboard_values(frame, mode, &dose, &delivery, &prescription, 0, true, false)
                    .ok();
                flush_frame(display, frame, flush_buffer).await;
                last_ui_refresh = Instant::now();
            } else if matches!(settings_action, SettingsAction::ShowControls) {
                app_screen = AppScreen::ControlsHelp;
                draw_setup_controls_help_screen(frame).ok();
                flush_frame(display, frame, flush_buffer).await;
                last_retract_pressed = retract_pressed;
                continue;
            } else if matches!(settings_action, SettingsAction::TuttiFrutti) {
                settings_editing = false;
                delivery.running = false;
                delivery.alert = DeliveryAlert::Standby;
                carriage_position_steps =
                    move_to_homing_backoff_position(motor, carriage_position_steps).await;
                log::info!(
                    "Tutti Frutti start position: {} steps from home",
                    carriage_position_steps
                );
                let _ = sing_stepper_song(&mut tmc, motor, TUTTI_FRUTTI_MELODY).await;
                draw_homing_screen(frame, true, false).ok();
                flush_frame(display, frame, flush_buffer).await;
                carriage_position_steps =
                    run_homing_sequence(display, frame, flush_buffer, motor, &inputs, &mut tmc)
                        .await;
                tmc.set_spreadcycle_enabled(settings.delivery_spreadcycle_enabled)
                    .await;
                save_persistent_config(
                    &mut store,
                    syringe_selected,
                    syringe_mounted,
                    carriage_position_steps,
                    &prescription,
                    &settings,
                    &delivery,
                    &dose,
                    bolus_volume_ul,
                    bolus_rate_ul_per_min,
                );
                app_screen = AppScreen::Pump;
                draw_dashboard_frame(frame, &prescription, settings.delivery_spreadcycle_enabled)
                    .ok();
                draw_dashboard_values(frame, mode, &dose, &delivery, &prescription, 0, true, false)
                    .ok();
                flush_frame(display, frame, flush_buffer).await;
                last_ui_refresh = Instant::now();
            } else if matches!(app_screen, AppScreen::Pump) {
                save_persistent_config(
                    &mut store,
                    syringe_selected,
                    syringe_mounted,
                    carriage_position_steps,
                    &prescription,
                    &settings,
                    &delivery,
                    &dose,
                    bolus_volume_ul,
                    bolus_rate_ul_per_min,
                );
                draw_dashboard_frame(frame, &prescription, settings.delivery_spreadcycle_enabled)
                    .ok();
                draw_dashboard_values(frame, mode, &dose, &delivery, &prescription, 0, true, false)
                    .ok();
                flush_frame(display, frame, flush_buffer).await;
                last_ui_refresh = Instant::now();
            } else if redraw {
                draw_settings_screen(
                    frame,
                    settings.kvo_enabled,
                    settings.kvo_rate_ul_per_min,
                    settings.direct_bolus_rate_ul_per_min,
                    settings.delivery_spreadcycle_enabled,
                    store.flash_write_count(),
                    settings_selected,
                    settings_editing,
                )
                .ok();
                flush_frame(display, frame, flush_buffer).await;
            } else {
                if delivery.running
                    && (delivery.remaining_steps > 0 || delivery.kvo_active)
                    && active_delivery_motor_command.is_none()
                {
                    let period_us = if delivery.kvo_active {
                        rate_step_period_us(settings.kvo_rate_ul_per_min, prescription.syringe)
                    } else {
                        dispense_step_period_us(&prescription)
                    };
                    let max_steps = if delivery.kvo_active {
                        None
                    } else {
                        Some(delivery.remaining_steps)
                    };
                    tmc.set_spreadcycle_enabled(delivery_run_spreadcycle_enabled(
                        delivery.kvo_active,
                        if delivery.kvo_active {
                            settings.kvo_rate_ul_per_min
                        } else {
                            delivery_rate_ul_per_min(&prescription)
                        },
                        &settings,
                    ))
                    .await;
                    active_delivery_motor_command = Some(
                        motor
                            .run_auto(
                                MotorRunKind::Delivery,
                                MotionDirection::DispenseTowardEmpty,
                                period_us,
                                max_steps,
                            )
                            .await,
                    );
                    active_delivery_motor_seen_steps = 0;
                }

                let delivered_now = apply_delivery_motor_status(
                    motor,
                    &mut dose,
                    &mut delivery,
                    &prescription,
                    &settings,
                    &mut active_delivery_motor_command,
                    &mut active_delivery_motor_seen_steps,
                );
                if delivered_now > 0 {
                    carriage_position_steps = carriage_position_steps
                        .saturating_add(delivered_now as i32)
                        .min(CARRIAGE_HARD_LIMIT_STEPS_FROM_HOME);
                } else {
                    Timer::after_millis(5).await;
                }
                if delivery_flash_checkpoint_due(
                    &delivery,
                    next_delivery_flash_checkpoint_steps,
                    last_delivery_flash_save,
                ) {
                    save_persistent_config(
                        &mut store,
                        syringe_selected,
                        syringe_mounted,
                        carriage_position_steps,
                        &prescription,
                        &settings,
                        &delivery,
                        &dose,
                        bolus_volume_ul,
                        bolus_rate_ul_per_min,
                    );
                    next_delivery_flash_checkpoint_steps =
                        next_delivery_flash_checkpoint(&delivery);
                    last_delivery_flash_save = Instant::now();
                }
            }
            last_retract_pressed = retract_pressed;
            continue;
        }

        if matches!(app_screen, AppScreen::ControlsHelp) {
            if retract_edge {
                app_screen = AppScreen::Settings;
                draw_settings_screen(
                    frame,
                    settings.kvo_enabled,
                    settings.kvo_rate_ul_per_min,
                    settings.direct_bolus_rate_ul_per_min,
                    settings.delivery_spreadcycle_enabled,
                    store.flash_write_count(),
                    settings_selected,
                    settings_editing,
                )
                .ok();
                flush_frame(display, frame, flush_buffer).await;
            } else {
                Timer::after_millis(5).await;
            }
            last_retract_pressed = retract_pressed;
            continue;
        }

        if matches!(app_screen, AppScreen::BolusSetup) {
            if retract_edge {
                bolus_editing = false;
                app_screen = AppScreen::Pump;
                save_persistent_config(
                    &mut store,
                    syringe_selected,
                    syringe_mounted,
                    carriage_position_steps,
                    &prescription,
                    &settings,
                    &delivery,
                    &dose,
                    bolus_volume_ul,
                    bolus_rate_ul_per_min,
                );
                draw_dashboard_frame(frame, &prescription, settings.delivery_spreadcycle_enabled)
                    .ok();
                draw_dashboard_values(frame, mode, &dose, &delivery, &prescription, 0, true, false)
                    .ok();
                flush_frame(display, frame, flush_buffer).await;
                last_ui_refresh = Instant::now();
                last_retract_pressed = retract_pressed;
                continue;
            }

            let start_bolus = handle_bolus_setup_input(
                &mut bolus_volume_ul,
                &mut bolus_rate_ul_per_min,
                encoder_delta,
                encoder_press,
                &mut bolus_selected,
                &mut bolus_editing,
                &mut redraw,
            );

            if start_bolus {
                resume_after_bolus = delivery.running;
                delivery.running = false;
                if active_delivery_motor_command.take().is_some() {
                    motor.stop_now().await;
                    active_delivery_motor_seen_steps = 0;
                }
                let bolus_steps = match steps_for_volume_ul(bolus_volume_ul, &prescription) {
                    Ok(steps) => steps,
                    Err(error) => {
                        log::error!("confirmed bolus rejected: {:?}", error);
                        draw_bolus_setup_screen(
                            frame,
                            bolus_volume_ul,
                            bolus_rate_ul_per_min,
                            bolus_selected,
                            bolus_editing,
                        )
                        .ok();
                        flush_frame(display, frame, flush_buffer).await;
                        last_retract_pressed = retract_pressed;
                        continue;
                    }
                };
                if bolus_steps == 0 {
                    last_retract_pressed = retract_pressed;
                    continue;
                }
                let max_steps = CARRIAGE_HARD_LIMIT_STEPS_FROM_HOME
                    .saturating_sub(carriage_position_steps)
                    .max(0) as u32;
                let command_steps = bolus_steps.min(max_steps);
                if command_steps == 0 {
                    delivery.alert = DeliveryAlert::SyringeEmpty;
                    app_screen = AppScreen::Pump;
                    draw_dashboard_frame(
                        frame,
                        &prescription,
                        settings.delivery_spreadcycle_enabled,
                    )
                    .ok();
                    draw_dashboard_values(
                        frame,
                        mode,
                        &dose,
                        &delivery,
                        &prescription,
                        flow_phase,
                        true,
                        false,
                    )
                    .ok();
                    flush_frame(display, frame, flush_buffer).await;
                    last_retract_pressed = retract_pressed;
                    continue;
                }
                tmc.set_spreadcycle_enabled(true).await;
                active_configured_bolus_command = Some(
                    motor
                        .run_auto(
                            MotorRunKind::DirectBolus,
                            MotionDirection::DispenseTowardEmpty,
                            rate_step_period_us(bolus_rate_ul_per_min, prescription.syringe),
                            Some(command_steps),
                        )
                        .await,
                );
                active_configured_bolus_seen_steps = 0;
                active_configured_bolus_total_ul = 0.0;
                app_screen = AppScreen::Pump;
                save_persistent_config(
                    &mut store,
                    syringe_selected,
                    syringe_mounted,
                    carriage_position_steps,
                    &prescription,
                    &settings,
                    &delivery,
                    &dose,
                    bolus_volume_ul,
                    bolus_rate_ul_per_min,
                );
                draw_dashboard_frame(frame, &prescription, settings.delivery_spreadcycle_enabled)
                    .ok();
                draw_dashboard_values(frame, mode, &dose, &delivery, &prescription, 0, true, false)
                    .ok();
                flush_frame(display, frame, flush_buffer).await;
                last_ui_refresh = Instant::now();
            } else if redraw {
                draw_bolus_setup_screen(
                    frame,
                    bolus_volume_ul,
                    bolus_rate_ul_per_min,
                    bolus_selected,
                    bolus_editing,
                )
                .ok();
                flush_frame(display, frame, flush_buffer).await;
            } else {
                if delivery.running
                    && (delivery.remaining_steps > 0 || delivery.kvo_active)
                    && active_delivery_motor_command.is_none()
                {
                    let period_us = if delivery.kvo_active {
                        rate_step_period_us(settings.kvo_rate_ul_per_min, prescription.syringe)
                    } else {
                        dispense_step_period_us(&prescription)
                    };
                    let max_steps = if delivery.kvo_active {
                        None
                    } else {
                        Some(delivery.remaining_steps)
                    };
                    tmc.set_spreadcycle_enabled(delivery_run_spreadcycle_enabled(
                        delivery.kvo_active,
                        if delivery.kvo_active {
                            settings.kvo_rate_ul_per_min
                        } else {
                            delivery_rate_ul_per_min(&prescription)
                        },
                        &settings,
                    ))
                    .await;
                    active_delivery_motor_command = Some(
                        motor
                            .run_auto(
                                MotorRunKind::Delivery,
                                MotionDirection::DispenseTowardEmpty,
                                period_us,
                                max_steps,
                            )
                            .await,
                    );
                    active_delivery_motor_seen_steps = 0;
                }

                let delivered_now = apply_delivery_motor_status(
                    motor,
                    &mut dose,
                    &mut delivery,
                    &prescription,
                    &settings,
                    &mut active_delivery_motor_command,
                    &mut active_delivery_motor_seen_steps,
                );
                if delivered_now > 0 {
                    carriage_position_steps = carriage_position_steps
                        .saturating_add(delivered_now as i32)
                        .min(CARRIAGE_HARD_LIMIT_STEPS_FROM_HOME);
                } else {
                    Timer::after_millis(5).await;
                }
            }
            last_retract_pressed = retract_pressed;
            continue;
        }

        if matches!(app_screen, AppScreen::RemoveSyringePrompt) {
            if retract_edge {
                app_screen = AppScreen::Pump;
                draw_dashboard_frame(frame, &prescription, settings.delivery_spreadcycle_enabled)
                    .ok();
                draw_dashboard_values(frame, mode, &dose, &delivery, &prescription, 0, true, false)
                    .ok();
                flush_frame(display, frame, flush_buffer).await;
                last_ui_refresh = Instant::now();
            } else if encoder_press {
                relieve_syringe_pressure(motor, &mut carriage_position_steps).await;
                delivery.alert = DeliveryAlert::PressureRelieved;
                save_persistent_config(
                    &mut store,
                    syringe_selected,
                    syringe_mounted,
                    carriage_position_steps,
                    &prescription,
                    &settings,
                    &delivery,
                    &dose,
                    bolus_volume_ul,
                    bolus_rate_ul_per_min,
                );
                app_screen = AppScreen::ConfirmSyringeRemoved;
                draw_dashboard_frame(frame, &prescription, settings.delivery_spreadcycle_enabled)
                    .ok();
                draw_dashboard_values(
                    frame,
                    mode,
                    &dose,
                    &delivery,
                    &prescription,
                    flow_phase,
                    true,
                    false,
                )
                .ok();
                flush_frame(display, frame, flush_buffer).await;
                last_ui_refresh = Instant::now();
            } else {
                Timer::after_millis(5).await;
            }
            last_retract_pressed = retract_pressed;
            continue;
        }

        if matches!(app_screen, AppScreen::ConfirmSyringeRemoved) {
            if encoder_press {
                delivery.stop();
                delivery.alert = DeliveryAlert::None;
                dose = DoseAccumulator::new();
                carriage_position_steps =
                    move_to_homing_backoff_position(motor, carriage_position_steps).await;
                syringe_mounted = false;
                app_screen = AppScreen::Syringe;
                syringe_selected = DEFAULT_SYRINGE_INDEX;
                save_persistent_config(
                    &mut store,
                    syringe_selected,
                    syringe_mounted,
                    carriage_position_steps,
                    &prescription,
                    &settings,
                    &delivery,
                    &dose,
                    bolus_volume_ul,
                    bolus_rate_ul_per_min,
                );
                draw_syringe_select_screen(frame, syringe_selected).ok();
                flush_frame(display, frame, flush_buffer).await;
            } else {
                Timer::after_millis(5).await;
            }
            last_retract_pressed = retract_pressed;
            continue;
        }

        if matches!(app_screen, AppScreen::Pump) {
            if dispense_pressed && bolus_press_started.is_none() && !bolus_hold_active {
                bolus_press_started = Some(Instant::now());
                bolus_hold_active = false;
            }

            if retract_edge {
                if direct_bolus_overlay_visible {
                    if active_direct_bolus_command.is_some() {
                        let (burst_steps, _) = apply_bolus_motor_status(
                            motor,
                            &mut dose,
                            &prescription,
                            &mut active_direct_bolus_command,
                            &mut active_direct_bolus_seen_steps,
                        );
                        if burst_steps > 0 {
                            let burst_ul = burst_steps as f32 * ul_per_step(&prescription);
                            direct_bolus_total_ul += burst_ul;
                            carriage_position_steps = carriage_position_steps
                                .saturating_add(burst_steps as i32)
                                .min(CARRIAGE_HARD_LIMIT_STEPS_FROM_HOME);
                        }
                        if active_direct_bolus_command.take().is_some() {
                            motor.stop_now().await;
                            active_direct_bolus_seen_steps = 0;
                        }
                    }
                    bolus_press_started = None;
                    bolus_hold_active = false;
                    direct_bolus_wait_release = false;
                    direct_bolus_overlay_visible = false;
                    direct_bolus_summary_ul = Some(direct_bolus_total_ul);
                    direct_bolus_window_ul = 0.0;
                    delivery.running =
                        resume_after_bolus && (delivery.remaining_steps > 0 || delivery.kvo_active);
                    if !delivery.running {
                        tmc.set_spreadcycle_enabled(settings.delivery_spreadcycle_enabled)
                            .await;
                    }
                    draw_dashboard_frame(
                        frame,
                        &prescription,
                        settings.delivery_spreadcycle_enabled,
                    )
                    .ok();
                    draw_dashboard_values(
                        frame,
                        mode,
                        &dose,
                        &delivery,
                        &prescription,
                        flow_phase,
                        true,
                        false,
                    )
                    .ok();
                    if let Some(summary_ul) = direct_bolus_summary_ul {
                        draw_bolus_delivered_alert_overlay(frame, summary_ul).ok();
                    }
                    flush_frame(display, frame, flush_buffer).await;
                    last_ui_refresh = Instant::now();
                    last_retract_pressed = retract_pressed;
                    continue;
                } else if matches!(delivery.alert, DeliveryAlert::EndOfInfusion) {
                    if !syringe_mounted {
                        log::warn!("remove syringe requested while mounted state was false");
                    }
                    delivery.stop();
                    motor.disable();
                    relieve_syringe_pressure(motor, &mut carriage_position_steps).await;
                    delivery.alert = DeliveryAlert::PressureRelieved;
                    save_persistent_config(
                        &mut store,
                        syringe_selected,
                        syringe_mounted,
                        carriage_position_steps,
                        &prescription,
                        &settings,
                        &delivery,
                        &dose,
                        bolus_volume_ul,
                        bolus_rate_ul_per_min,
                    );
                    app_screen = AppScreen::ConfirmSyringeRemoved;
                    draw_dashboard_frame(
                        frame,
                        &prescription,
                        settings.delivery_spreadcycle_enabled,
                    )
                    .ok();
                    draw_dashboard_values(
                        frame,
                        mode,
                        &dose,
                        &delivery,
                        &prescription,
                        flow_phase,
                        true,
                        false,
                    )
                    .ok();
                    flush_frame(display, frame, flush_buffer).await;
                    last_ui_refresh = Instant::now();
                    last_retract_pressed = retract_pressed;
                    continue;
                }
                back_press_started = Some(Instant::now());
                back_hold_active = false;
            }

            if retract_pressed
                && !back_hold_active
                && back_press_started
                    .map(|started| {
                        started.elapsed() >= Duration::from_millis(BUTTON_HOLD_ACTION_MS)
                    })
                    .unwrap_or(false)
            {
                back_hold_active = true;
                app_screen = AppScreen::Settings;
                settings_selected = 0;
                settings_editing = false;
                draw_settings_screen(
                    frame,
                    settings.kvo_enabled,
                    settings.kvo_rate_ul_per_min,
                    settings.direct_bolus_rate_ul_per_min,
                    settings.delivery_spreadcycle_enabled,
                    store.flash_write_count(),
                    settings_selected,
                    settings_editing,
                )
                .ok();
                flush_frame(display, frame, flush_buffer).await;
                last_retract_pressed = retract_pressed;
                continue;
            }

            if !retract_pressed && last_retract_pressed {
                back_press_started = None;
                back_hold_active = false;
            }

            if !dispense_pressed && bolus_press_started.is_some() {
                let was_direct_bolus = bolus_hold_active;
                let was_short_press = !bolus_hold_active
                    && bolus_press_started
                        .map(|started| {
                            started.elapsed() < Duration::from_millis(DIRECT_BOLUS_HOLD_ACTION_MS)
                        })
                        .unwrap_or(false);
                let block_bolus_setup = direct_bolus_overlay_visible
                    || direct_bolus_summary_ul.is_some()
                    || configured_bolus_summary_ul.is_some()
                    || delivery.alert.is_active();
                bolus_press_started = None;
                bolus_hold_active = false;

                if was_short_press && !block_bolus_setup {
                    direct_bolus_overlay_visible = false;
                    direct_bolus_window_ul = 0.0;
                    direct_bolus_total_ul = 0.0;
                    direct_bolus_wait_release = false;
                    resume_after_bolus = delivery.running;
                    app_screen = AppScreen::BolusSetup;
                    bolus_selected = 0;
                    bolus_editing = false;
                    draw_bolus_setup_screen(
                        frame,
                        bolus_volume_ul,
                        bolus_rate_ul_per_min,
                        bolus_selected,
                        bolus_editing,
                    )
                    .ok();
                    flush_frame(display, frame, flush_buffer).await;
                    last_retract_pressed = retract_pressed;
                    continue;
                } else if was_direct_bolus {
                    if active_direct_bolus_command.is_some() {
                        let (burst_steps, _) = apply_bolus_motor_status(
                            motor,
                            &mut dose,
                            &prescription,
                            &mut active_direct_bolus_command,
                            &mut active_direct_bolus_seen_steps,
                        );
                        if burst_steps > 0 {
                            let burst_ul = burst_steps as f32 * ul_per_step(&prescription);
                            direct_bolus_total_ul += burst_ul;
                            carriage_position_steps = carriage_position_steps
                                .saturating_add(burst_steps as i32)
                                .min(CARRIAGE_HARD_LIMIT_STEPS_FROM_HOME);
                        }
                        if active_direct_bolus_command.take().is_some() {
                            motor.stop_now().await;
                            active_direct_bolus_seen_steps = 0;
                        }
                    }
                    direct_bolus_window_ul = 0.0;
                    direct_bolus_wait_release = false;
                    delivery.running =
                        resume_after_bolus && (delivery.remaining_steps > 0 || delivery.kvo_active);
                    if !delivery.running {
                        tmc.set_spreadcycle_enabled(settings.delivery_spreadcycle_enabled)
                            .await;
                    }
                    save_persistent_config(
                        &mut store,
                        syringe_selected,
                        syringe_mounted,
                        carriage_position_steps,
                        &prescription,
                        &settings,
                        &delivery,
                        &dose,
                        bolus_volume_ul,
                        bolus_rate_ul_per_min,
                    );
                    redraw = true;
                }
            }

            if encoder_press {
                if direct_bolus_summary_ul.is_some() || configured_bolus_summary_ul.is_some() {
                    direct_bolus_summary_ul = None;
                    configured_bolus_summary_ul = None;
                    redraw_dashboard_frame = true;
                } else if matches!(delivery.alert, DeliveryAlert::KvoRunning) {
                    delivery.alert = DeliveryAlert::None;
                    redraw_dashboard_frame = true;
                    save_persistent_config(
                        &mut store,
                        syringe_selected,
                        syringe_mounted,
                        carriage_position_steps,
                        &prescription,
                        &settings,
                        &delivery,
                        &dose,
                        bolus_volume_ul,
                        bolus_rate_ul_per_min,
                    );
                } else if matches!(delivery.alert, DeliveryAlert::EndOfInfusion) {
                    delivery.stop();
                    delivery.alert = DeliveryAlert::None;
                    dose = DoseAccumulator::new();
                    setup_selected = first_setup_item(&prescription);
                    setup_editing = false;
                    app_screen = AppScreen::Setup;
                    save_persistent_config(
                        &mut store,
                        syringe_selected,
                        syringe_mounted,
                        carriage_position_steps,
                        &prescription,
                        &settings,
                        &delivery,
                        &dose,
                        bolus_volume_ul,
                        bolus_rate_ul_per_min,
                    );
                    draw_setup_screen(frame, &prescription, setup_selected, setup_editing).ok();
                    flush_frame(display, frame, flush_buffer).await;
                    last_ui_refresh = Instant::now();
                    last_retract_pressed = retract_pressed;
                    continue;
                } else if matches!(delivery.alert, DeliveryAlert::SyringeEmpty) {
                    if !syringe_mounted {
                        log::warn!("remove syringe prompt requested while mounted state was false");
                    }
                    delivery.stop();
                    motor.disable();
                    relieve_syringe_pressure(motor, &mut carriage_position_steps).await;
                    delivery.alert = DeliveryAlert::PressureRelieved;
                    save_persistent_config(
                        &mut store,
                        syringe_selected,
                        syringe_mounted,
                        carriage_position_steps,
                        &prescription,
                        &settings,
                        &delivery,
                        &dose,
                        bolus_volume_ul,
                        bolus_rate_ul_per_min,
                    );
                    app_screen = AppScreen::ConfirmSyringeRemoved;
                    draw_dashboard_frame(
                        frame,
                        &prescription,
                        settings.delivery_spreadcycle_enabled,
                    )
                    .ok();
                    draw_dashboard_values(
                        frame,
                        mode,
                        &dose,
                        &delivery,
                        &prescription,
                        flow_phase,
                        true,
                        false,
                    )
                    .ok();
                    flush_frame(display, frame, flush_buffer).await;
                    last_ui_refresh = Instant::now();
                    last_retract_pressed = retract_pressed;
                    continue;
                } else if delivery.alert.is_active() {
                    delivery.alert = DeliveryAlert::None;
                    redraw_dashboard_frame = true;
                } else if delivery.remaining_steps > 0 || delivery.kvo_active {
                    let was_delivery_running = delivery.running;
                    delivery.running = !delivery.running;
                    if was_delivery_running {
                        let delivered_now = apply_delivery_motor_status(
                            motor,
                            &mut dose,
                            &mut delivery,
                            &prescription,
                            &settings,
                            &mut active_delivery_motor_command,
                            &mut active_delivery_motor_seen_steps,
                        );
                        if delivered_now > 0 {
                            carriage_position_steps = carriage_position_steps
                                .saturating_add(delivered_now as i32)
                                .min(syringe_empty_position_steps(&prescription));
                        }
                        if active_delivery_motor_command.take().is_some() {
                            motor.stop_now().await;
                            active_delivery_motor_seen_steps = 0;
                        }
                    }
                    log_delivery_toggle(&delivery, &dose, &prescription, &settings);
                    last_delivery_flash_save = Instant::now();
                    save_persistent_config(
                        &mut store,
                        syringe_selected,
                        syringe_mounted,
                        carriage_position_steps,
                        &prescription,
                        &settings,
                        &delivery,
                        &dose,
                        bolus_volume_ul,
                        bolus_rate_ul_per_min,
                    );
                } else if prepare_or_fault(&mut dose, &mut delivery, &prescription).is_ok() {
                    tmc.set_spreadcycle_enabled(delivery_run_spreadcycle_enabled(
                        false,
                        delivery_rate_ul_per_min(&prescription),
                        &settings,
                    ))
                    .await;
                    delivery.running = true;
                    next_delivery_flash_checkpoint_steps =
                        next_delivery_flash_checkpoint(&delivery);
                    last_delivery_flash_save = Instant::now();
                    save_persistent_config(
                        &mut store,
                        syringe_selected,
                        syringe_mounted,
                        carriage_position_steps,
                        &prescription,
                        &settings,
                        &delivery,
                        &dose,
                        bolus_volume_ul,
                        bolus_rate_ul_per_min,
                    );
                }
                redraw = true;
            }

            let was_running = delivery.running;
            if dispense_pressed
                && bolus_press_started
                    .map(|started| {
                        started.elapsed() >= Duration::from_millis(DIRECT_BOLUS_HOLD_ACTION_MS)
                    })
                    .unwrap_or(false)
            {
                if !bolus_hold_active {
                    if !direct_bolus_overlay_visible {
                        direct_bolus_total_ul = 0.0;
                    }
                    resume_after_bolus = delivery.running;
                    bolus_hold_active = true;
                    direct_bolus_overlay_visible = true;
                    delivery.running = false;
                    redraw = true;
                    if active_delivery_motor_command.take().is_some() {
                        motor.stop_now().await;
                        active_delivery_motor_seen_steps = 0;
                    }
                }

                if direct_bolus_window_ul >= DIRECT_BOLUS_WINDOW_UL {
                    direct_bolus_wait_release = true;
                    if active_direct_bolus_command.take().is_some() {
                        motor.stop_now().await;
                        active_direct_bolus_seen_steps = 0;
                    } else {
                        motor.disable();
                    }
                }

                if !direct_bolus_wait_release {
                    let remaining_window_ul =
                        (DIRECT_BOLUS_WINDOW_UL - direct_bolus_window_ul).max(0.0);
                    let remaining_window_steps =
                        (remaining_window_ul / ul_per_step(&prescription)) as u32;
                    if remaining_window_steps == 0 {
                        direct_bolus_window_ul = DIRECT_BOLUS_WINDOW_UL;
                        direct_bolus_wait_release = true;
                        if active_direct_bolus_command.take().is_some() {
                            motor.stop_now().await;
                            active_direct_bolus_seen_steps = 0;
                        } else {
                            motor.disable();
                        }
                        redraw = true;
                    } else {
                        let max_steps = CARRIAGE_HARD_LIMIT_STEPS_FROM_HOME
                            .saturating_sub(carriage_position_steps)
                            .max(0) as u32;
                        if active_direct_bolus_command.is_none() {
                            let command_steps = max_steps.min(remaining_window_steps);
                            if command_steps == 0 {
                                delivery.stop();
                                delivery.alert = DeliveryAlert::SyringeEmpty;
                                direct_bolus_wait_release = true;
                                motor.disable();
                                redraw = true;
                            } else {
                                tmc.set_spreadcycle_enabled(true).await;
                                active_direct_bolus_command = Some(
                                    motor
                                        .run_auto(
                                            MotorRunKind::DirectBolus,
                                            MotionDirection::DispenseTowardEmpty,
                                            rate_step_period_us(
                                                settings.direct_bolus_rate_ul_per_min,
                                                prescription.syringe,
                                            ),
                                            Some(command_steps),
                                        )
                                        .await,
                                );
                                active_direct_bolus_seen_steps = 0;
                                redraw = true;
                            }
                        }
                        let (burst_steps, bolus_command_done) = apply_bolus_motor_status(
                            motor,
                            &mut dose,
                            &prescription,
                            &mut active_direct_bolus_command,
                            &mut active_direct_bolus_seen_steps,
                        );
                        if burst_steps > 0 {
                            let burst_ul = burst_steps as f32 * ul_per_step(&prescription);
                            direct_bolus_window_ul =
                                (direct_bolus_window_ul + burst_ul).min(DIRECT_BOLUS_WINDOW_UL);
                            direct_bolus_total_ul += burst_ul;
                            carriage_position_steps = carriage_position_steps
                                .saturating_add(burst_steps as i32)
                                .min(CARRIAGE_HARD_LIMIT_STEPS_FROM_HOME);
                            redraw = true;
                        }

                        if direct_bolus_window_ul >= DIRECT_BOLUS_WINDOW_UL {
                            direct_bolus_wait_release = true;
                            if active_direct_bolus_command.take().is_some() {
                                motor.stop_now().await;
                                active_direct_bolus_seen_steps = 0;
                            } else {
                                motor.disable();
                            }
                        } else if bolus_command_done {
                            redraw = true;
                        }
                    }
                }
            } else {
                if active_direct_bolus_command.is_some() {
                    let (burst_steps, _) = apply_bolus_motor_status(
                        motor,
                        &mut dose,
                        &prescription,
                        &mut active_direct_bolus_command,
                        &mut active_direct_bolus_seen_steps,
                    );
                    if burst_steps > 0 {
                        let burst_ul = burst_steps as f32 * ul_per_step(&prescription);
                        direct_bolus_window_ul =
                            (direct_bolus_window_ul + burst_ul).min(DIRECT_BOLUS_WINDOW_UL);
                        direct_bolus_total_ul += burst_ul;
                        carriage_position_steps = carriage_position_steps
                            .saturating_add(burst_steps as i32)
                            .min(CARRIAGE_HARD_LIMIT_STEPS_FROM_HOME);
                        redraw = true;
                    }
                    if active_direct_bolus_command.take().is_some() {
                        motor.stop_now().await;
                        active_direct_bolus_seen_steps = 0;
                    }
                }

                if active_configured_bolus_command.is_some() {
                    let (bolus_steps, bolus_done) = apply_bolus_motor_status(
                        motor,
                        &mut dose,
                        &prescription,
                        &mut active_configured_bolus_command,
                        &mut active_configured_bolus_seen_steps,
                    );
                    if bolus_steps > 0 {
                        let bolus_ul = bolus_steps as f32 * ul_per_step(&prescription);
                        active_configured_bolus_total_ul += bolus_ul;
                        carriage_position_steps = carriage_position_steps
                            .saturating_add(bolus_steps as i32)
                            .min(CARRIAGE_HARD_LIMIT_STEPS_FROM_HOME);
                        redraw = true;
                    }
                    if button_pressed(&inputs.homing_limit_switch) {
                        if active_configured_bolus_command.take().is_some() {
                            motor.stop_now().await;
                            active_configured_bolus_seen_steps = 0;
                        }
                        handle_homing_limit_recalibration(
                            display,
                            frame,
                            flush_buffer,
                            motor,
                            &mut store,
                            &mut delivery,
                            &mut carriage_position_steps,
                            syringe_selected,
                            syringe_mounted,
                            &prescription,
                            &settings,
                            &dose,
                            bolus_volume_ul,
                            bolus_rate_ul_per_min,
                        )
                        .await;
                        app_screen = AppScreen::HomingLimitAlert;
                        last_retract_pressed = retract_pressed;
                        last_homing_limit_pressed = true;
                        continue;
                    }
                    if bolus_done {
                        configured_bolus_summary_ul = Some(active_configured_bolus_total_ul);
                        active_configured_bolus_total_ul = 0.0;
                        delivery.running = resume_after_bolus
                            && (delivery.remaining_steps > 0 || delivery.kvo_active);
                        if !delivery.running {
                            tmc.set_spreadcycle_enabled(settings.delivery_spreadcycle_enabled)
                                .await;
                        }
                        save_persistent_config(
                            &mut store,
                            syringe_selected,
                            syringe_mounted,
                            carriage_position_steps,
                            &prescription,
                            &settings,
                            &delivery,
                            &dose,
                            bolus_volume_ul,
                            bolus_rate_ul_per_min,
                        );
                        redraw = true;
                    } else if bolus_steps == 0 {
                        Timer::after_millis(1).await;
                    }
                } else if delivery.running && (delivery.remaining_steps > 0 || delivery.kvo_active)
                {
                    let empty_position_steps = syringe_empty_position_steps(&prescription);
                    if carriage_position_steps >= empty_position_steps {
                        delivery.stop();
                        delivery.alert = DeliveryAlert::SyringeEmpty;
                        motor.disable();
                        log::warn!(
                            "delivery stopped because syringe is empty: position={} empty_position={}",
                            carriage_position_steps,
                            empty_position_steps
                        );
                        redraw = true;
                    } else {
                        if active_delivery_motor_command.is_none() {
                            let steps_to_empty =
                                (empty_position_steps - carriage_position_steps).max(0) as u32;
                            let period_us = if delivery.kvo_active {
                                rate_step_period_us(
                                    settings.kvo_rate_ul_per_min,
                                    prescription.syringe,
                                )
                            } else {
                                dispense_step_period_us(&prescription)
                            };
                            let max_steps = if delivery.kvo_active {
                                Some(steps_to_empty)
                            } else {
                                Some(delivery.remaining_steps.min(steps_to_empty))
                            };
                            tmc.set_spreadcycle_enabled(delivery_run_spreadcycle_enabled(
                                delivery.kvo_active,
                                if delivery.kvo_active {
                                    settings.kvo_rate_ul_per_min
                                } else {
                                    delivery_rate_ul_per_min(&prescription)
                                },
                                &settings,
                            ))
                            .await;
                            active_delivery_motor_command = Some(
                                motor
                                    .run_auto(
                                        MotorRunKind::Delivery,
                                        MotionDirection::DispenseTowardEmpty,
                                        period_us,
                                        max_steps,
                                    )
                                    .await,
                            );
                            active_delivery_motor_seen_steps = 0;
                        }

                        let delivered_now = apply_delivery_motor_status(
                            motor,
                            &mut dose,
                            &mut delivery,
                            &prescription,
                            &settings,
                            &mut active_delivery_motor_command,
                            &mut active_delivery_motor_seen_steps,
                        );
                        if delivered_now > 0 {
                            carriage_position_steps = carriage_position_steps
                                .saturating_add(delivered_now as i32)
                                .min(empty_position_steps);
                            if carriage_position_steps >= empty_position_steps {
                                delivery.stop();
                                delivery.alert = DeliveryAlert::SyringeEmpty;
                                motor.disable();
                                active_delivery_motor_command = None;
                                active_delivery_motor_seen_steps = 0;
                                log::warn!(
                                    "syringe empty reached during delivery: position={} empty_position={}",
                                    carriage_position_steps,
                                    empty_position_steps
                                );
                                redraw = true;
                            }
                        } else {
                            Timer::after_millis(1).await;
                        }

                        if delivery_flash_checkpoint_due(
                            &delivery,
                            next_delivery_flash_checkpoint_steps,
                            last_delivery_flash_save,
                        ) {
                            save_persistent_config(
                                &mut store,
                                syringe_selected,
                                syringe_mounted,
                                carriage_position_steps,
                                &prescription,
                                &settings,
                                &delivery,
                                &dose,
                                bolus_volume_ul,
                                bolus_rate_ul_per_min,
                            );
                            next_delivery_flash_checkpoint_steps =
                                next_delivery_flash_checkpoint(&delivery);
                            last_delivery_flash_save = Instant::now();
                        }
                    }
                } else {
                    if active_delivery_motor_command.take().is_some() {
                        motor.stop_now().await;
                        active_delivery_motor_seen_steps = 0;
                    }
                    Timer::after_millis(5).await;
                }
            }

            if was_running && !delivery.running {
                save_persistent_config(
                    &mut store,
                    syringe_selected,
                    syringe_mounted,
                    carriage_position_steps,
                    &prescription,
                    &settings,
                    &delivery,
                    &dose,
                    bolus_volume_ul,
                    bolus_rate_ul_per_min,
                );
                redraw = true;
            }
        }

        if (delivery.running
            || bolus_hold_active
            || direct_bolus_overlay_visible
            || direct_bolus_summary_ul.is_some()
            || configured_bolus_summary_ul.is_some()
            || alert_needs_periodic_redraw(&delivery))
            && last_ui_refresh.elapsed() >= UI_REFRESH_INTERVAL
        {
            redraw = true;
        }

        if redraw {
            if delivery.running || bolus_hold_active {
                flow_phase = (flow_phase + 1) % FLOW_TRIANGLES;
            }
            if alert_needs_periodic_redraw(&delivery)
                && last_ui_refresh.elapsed() >= UI_REFRESH_INTERVAL
            {
                alarm_flash_on = !alarm_flash_on;
            } else if !delivery.alert.is_active() {
                alarm_flash_on = true;
            }
            if direct_bolus_overlay_visible {
                draw_direct_bolus_overlay(
                    frame,
                    &delivery,
                    direct_bolus_total_ul,
                    direct_bolus_window_ul,
                    DIRECT_BOLUS_WINDOW_UL,
                    direct_bolus_wait_release,
                    bolus_hold_active,
                    settings.direct_bolus_rate_ul_per_min,
                    flow_phase,
                )
                .ok();
            } else {
                if redraw_dashboard_frame {
                    draw_dashboard_frame(
                        frame,
                        &prescription,
                        settings.delivery_spreadcycle_enabled,
                    )
                    .ok();
                }
                draw_dashboard_values(
                    frame,
                    mode,
                    &dose,
                    &delivery,
                    &prescription,
                    flow_phase,
                    alarm_flash_on,
                    false,
                )
                .ok();
                if let Some(summary_ul) = direct_bolus_summary_ul {
                    draw_bolus_delivered_alert_overlay(frame, summary_ul).ok();
                }
                if let Some(summary_ul) = configured_bolus_summary_ul {
                    draw_bolus_administered_alert_overlay(frame, summary_ul).ok();
                }
            }
            flush_frame(display, frame, flush_buffer).await;
            last_ui_refresh = Instant::now();
        }
        last_retract_pressed = retract_pressed;
    }
}
