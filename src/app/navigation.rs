use crate::{
    config::*,
    dosing::{DRUG_LIBRARY, Prescription, SYRINGE_PRESETS},
    types::{AppScreen, BenchMode, RuntimeSettings, SetupContext},
};

pub(super) const FIRST_EDITABLE_SETUP_ITEM: usize = 0;
pub(super) const WEIGHT_DIGITS: usize = 5;

const SETTINGS_ITEMS: usize = 9;
const WEIGHT_CONTINUE_ITEM: usize = WEIGHT_DIGITS;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SettingsAction {
    None,
    EndPerfusion,
    ShowControls,
    TuttiFrutti,
}

/// Handles the setup page state machine: select fields, edit values, or start infusion.
pub(super) fn handle_setup_input(
    prescription: &mut Prescription,
    encoder_delta: i32,
    encoder_press: bool,
    context: &mut SetupContext<'_>,
) {
    if encoder_delta != 0 {
        if *context.editing {
            if is_editable_setup_item(prescription, *context.selected) {
                prescription.apply_delta(*context.selected, encoder_delta);
            }
        } else if encoder_delta > 0 {
            *context.selected = next_setup_item(prescription, *context.selected);
        } else {
            *context.selected = previous_setup_item(prescription, *context.selected);
        }
        *context.redraw = true;
    }

    if encoder_press {
        if *context.selected == SETUP_ITEMS - 1 {
            if prescription.validate().is_ok() {
                *context.app_screen = AppScreen::Pump;
                *context.mode = BenchMode::Delivery;
            } else {
                log::error!("invalid prescription rejected at start");
                *context.redraw = true;
            }
        } else if is_editable_setup_item(prescription, *context.selected) {
            *context.editing = !*context.editing;
            *context.redraw = true;
        }
    }
}

pub(super) fn first_setup_item(prescription: &Prescription) -> usize {
    if prescription.drug_index.is_some() {
        0
    } else {
        1
    }
}

pub(super) fn next_syringe_item(index: usize) -> usize {
    if index + 1 >= SYRINGE_PRESETS.len() {
        0
    } else {
        index + 1
    }
}

pub(super) fn previous_syringe_item(index: usize) -> usize {
    if index == 0 {
        SYRINGE_PRESETS.len() - 1
    } else {
        index - 1
    }
}

pub(super) fn next_drug_item(index: usize) -> usize {
    if index + 1 > DRUG_LIBRARY.len() {
        0
    } else {
        index + 1
    }
}

pub(super) fn previous_drug_item(index: usize) -> usize {
    if index == 0 {
        DRUG_LIBRARY.len()
    } else {
        index - 1
    }
}

/// Updates one patient-weight digit without affecting the other displayed digits.
pub(super) fn apply_weight_digit_delta(weight_kg: &mut f32, digit_index: usize, delta: i32) {
    let multipliers = [10000i32, 1000, 100, 10, 1];
    let mut value = (*weight_kg * 100.0) as i32;
    let multiplier = multipliers[digit_index.min(multipliers.len() - 1)];
    let current = (value / multiplier) % 10;
    let next = (current + delta).rem_euclid(10);
    value += (next - current) * multiplier;
    value = value.clamp(100, 30000);
    *weight_kg = value as f32 / 100.0;
}

pub(super) fn next_weight_item(index: usize) -> usize {
    if index >= WEIGHT_CONTINUE_ITEM {
        0
    } else {
        index + 1
    }
}

pub(super) fn previous_weight_item(index: usize) -> usize {
    if index == 0 {
        WEIGHT_CONTINUE_ITEM
    } else {
        index - 1
    }
}

fn next_settings_item(index: usize) -> usize {
    if index + 1 >= SETTINGS_ITEMS {
        0
    } else {
        index + 1
    }
}

fn previous_settings_item(index: usize) -> usize {
    if index == 0 {
        SETTINGS_ITEMS - 1
    } else {
        index - 1
    }
}

fn is_editable_setup_item(prescription: &Prescription, index: usize) -> bool {
    match index {
        0 => prescription.drug_index.is_some(),
        1 | 3 => true,
        2 => prescription.drug_index.is_none(),
        _ => false,
    }
}

fn next_setup_item(prescription: &Prescription, index: usize) -> usize {
    let mut next = index;
    for _ in 0..SETUP_ITEMS {
        next = if next + 1 >= SETUP_ITEMS {
            FIRST_EDITABLE_SETUP_ITEM
        } else {
            next + 1
        };
        if next == SETUP_ITEMS - 1 || is_editable_setup_item(prescription, next) {
            return next;
        }
    }
    SETUP_ITEMS - 1
}

fn previous_setup_item(prescription: &Prescription, index: usize) -> usize {
    let mut previous = index;
    for _ in 0..SETUP_ITEMS {
        previous = if previous == FIRST_EDITABLE_SETUP_ITEM {
            SETUP_ITEMS - 1
        } else {
            previous - 1
        };
        if previous == SETUP_ITEMS - 1 || is_editable_setup_item(prescription, previous) {
            return previous;
        }
    }
    SETUP_ITEMS - 1
}

/// Handles Settings navigation and returns actions that need work outside the menu module.
pub(super) fn handle_settings_input(
    settings: &mut RuntimeSettings,
    encoder_delta: i32,
    encoder_press: bool,
    selected: &mut usize,
    editing: &mut bool,
    app_screen: &mut AppScreen,
    redraw: &mut bool,
) -> SettingsAction {
    let mut action = SettingsAction::None;

    if encoder_delta != 0 {
        if *editing {
            let delta = if encoder_delta > 0 { 1.0 } else { -1.0 };
            match *selected {
                3 => {
                    settings.kvo_rate_ul_per_min = (settings.kvo_rate_ul_per_min + delta * 10.0)
                        .clamp(MIN_FLOW_RATE_UL_PER_MIN, MAX_FLOW_RATE_UL_PER_MIN);
                }
                4 => {
                    settings.direct_bolus_rate_ul_per_min = (settings.direct_bolus_rate_ul_per_min
                        + delta * 5_000.0)
                        .clamp(MIN_FLOW_RATE_UL_PER_MIN, MAX_BOLUS_RATE_UL_PER_MIN);
                }
                _ => {}
            }
        } else if encoder_delta > 0 {
            *selected = next_settings_item(*selected);
        } else {
            *selected = previous_settings_item(*selected);
        }
        *redraw = true;
    }

    if encoder_press {
        match *selected {
            0 => {
                *editing = false;
                *app_screen = AppScreen::Pump;
                action = SettingsAction::EndPerfusion;
                *redraw = true;
            }
            1 => {
                *editing = false;
                action = SettingsAction::ShowControls;
                *redraw = true;
            }
            2 => {
                settings.kvo_enabled = !settings.kvo_enabled;
                *redraw = true;
            }
            3 | 4 => {
                *editing = !*editing;
                *redraw = true;
            }
            5 => {
                settings.delivery_spreadcycle_enabled = !settings.delivery_spreadcycle_enabled;
                *redraw = true;
            }
            7 => {
                *editing = false;
                action = SettingsAction::TuttiFrutti;
                *redraw = true;
            }
            8 => {
                *editing = false;
                *app_screen = AppScreen::Pump;
                *redraw = true;
            }
            _ => {}
        }
    }

    action
}

/// Handles programmed bolus setup and returns true when the user confirms start.
pub(super) fn handle_bolus_setup_input(
    volume_ul: &mut f32,
    rate_ul_per_min: &mut f32,
    encoder_delta: i32,
    encoder_press: bool,
    selected: &mut usize,
    editing: &mut bool,
    redraw: &mut bool,
) -> bool {
    if encoder_delta != 0 {
        if *editing {
            let delta = encoder_delta as f32;
            match *selected {
                0 => {
                    *volume_ul =
                        (*volume_ul + delta * 10.0).clamp(MIN_BOLUS_VOLUME_UL, MAX_BOLUS_VOLUME_UL);
                }
                1 => {
                    *rate_ul_per_min = (*rate_ul_per_min + delta * 1_000.0)
                        .clamp(MIN_FLOW_RATE_UL_PER_MIN, MAX_FLOW_RATE_UL_PER_MIN);
                }
                _ => {}
            }
        } else if encoder_delta > 0 {
            *selected = (*selected + 1) % 3;
        } else if *selected == 0 {
            *selected = 2;
        } else {
            *selected -= 1;
        }
        *redraw = true;
    }

    if encoder_press {
        if *selected == 2 {
            *editing = false;
            return true;
        }

        *editing = !*editing;
        *redraw = true;
    }

    false
}
