#[cfg(target_os = "none")]
use esp_hal::{gpio::Input, i2c::master::Error as I2cError};

// UI state and hardware inputs.

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AppScreen {
    Syringe,
    Drug,
    PatientWeight,
    NfcSyringeDetected,
    LoadAdjust,
    Prime,
    Setup,
    Settings,
    ControlsHelp,
    BolusSetup,
    RemoveSyringePrompt,
    ConfirmSyringeRemoved,
    HomingLimitAlert,
    Pump,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BenchMode {
    Manual,
    Delivery,
}

#[cfg(target_os = "none")]
pub struct Inputs {
    pub dispense_button: Input<'static>,
    pub retract_button: Input<'static>,
    pub homing_limit_switch: Input<'static>,
    pub encoder_a: Input<'static>,
    pub encoder_b: Input<'static>,
    pub encoder_button: Input<'static>,
}

#[cfg(target_os = "none")]
pub(crate) struct SetupContext<'a> {
    pub app_screen: &'a mut AppScreen,
    pub mode: &'a mut BenchMode,
    pub selected: &'a mut usize,
    pub editing: &'a mut bool,
    pub redraw: &'a mut bool,
}

#[cfg(target_os = "none")]
pub(crate) struct RuntimeSettings {
    pub kvo_enabled: bool,
    pub kvo_rate_ul_per_min: f32,
    pub direct_bolus_rate_ul_per_min: f32,
    pub delivery_spreadcycle_enabled: bool,
}

#[cfg(target_os = "none")]
impl RuntimeSettings {
    pub(crate) fn from_config(config: PersistentConfig) -> Self {
        Self {
            kvo_enabled: config.kvo_enabled,
            kvo_rate_ul_per_min: config.kvo_rate_ul_per_min,
            direct_bolus_rate_ul_per_min: config.direct_bolus_rate_ul_per_min,
            delivery_spreadcycle_enabled: config.delivery_spreadcycle_enabled,
        }
    }
}

// Dosing domain types.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DosingError {
    NonFiniteValue,
    InvalidPrescription,
    StepCountOutOfRange,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SyringeSpec {
    pub label: &'static str,
    pub inner_diameter_mm: f32,
    pub nominal_volume_ul: f32,
    pub measured_full_position_steps_from_home: Option<i32>,
    pub measured_empty_position_steps_from_home: Option<i32>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DrugSpec {
    pub drug_name: &'static str,
    pub class: &'static str,
    pub typical_concentration: &'static str,
    pub concentration_mg_per_ml: f32,
    pub color_rgb: [u8; 3],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Prescription {
    pub syringe: SyringeSpec,
    pub drug_index: Option<usize>,
    pub flow_rate_ul_per_min: f32,
    pub vtbi_ul: f32,
    pub infusion_time_min: f32,
    pub dose_rate_ul_per_min: f32,
    pub patient_weight_kg: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeliveryAlert {
    None,
    Standby,
    EndOfInfusion,
    KvoRunning,
    SyringeEmpty,
    PressureRelieved,
    DosingFault,
}

pub struct DeliveryState {
    pub running: bool,
    pub remaining_steps: u32,
    pub dose_steps: u32,
    pub ramp_progress_steps: u32,
    pub delivered_steps_this_dose: u32,
    pub kvo_active: bool,
    pub alert: DeliveryAlert,
}

pub struct DoseAccumulator {
    pub(crate) fractional_steps: f32,
    pub total_ul: f32,
}

// Motor command/status types.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MotionDirection {
    DispenseTowardEmpty,
    RetractTowardLoad,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MotorRunKind {
    Idle,
    Positioning,
    Delivery,
    DirectBolus,
    Tone,
}

#[derive(Clone, Copy, Debug)]
pub struct MotorStatus {
    pub command_id: u32,
    pub completed_command_id: u32,
    pub running: bool,
    pub kind: MotorRunKind,
    pub position_steps: i32,
    pub command_steps: u32,
    pub total_steps: u64,
}

// NFC domain types.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NfcTag {
    SyringePreset(usize),
    SyringePresetWithDrug {
        syringe_index: usize,
        drug_index: usize,
    },
}

#[cfg(target_os = "none")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NfcError {
    I2c(I2cError),
    NotReady,
    Protocol,
}

#[cfg(target_os = "none")]
impl From<I2cError> for NfcError {
    fn from(error: I2cError) -> Self {
        Self::I2c(error)
    }
}

// Persistent storage snapshot.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PersistentConfig {
    pub syringe_index: usize,
    pub drug_index: Option<usize>,
    pub syringe_mounted: bool,
    pub carriage_position_steps: i32,
    pub flow_rate_ul_per_min: f32,
    pub vtbi_ul: f32,
    pub infusion_time_min: f32,
    pub dose_rate_ul_per_min: f32,
    pub patient_weight_kg: f32,
    pub kvo_enabled: bool,
    pub kvo_rate_ul_per_min: f32,
    pub direct_bolus_rate_ul_per_min: f32,
    pub bolus_volume_ul: f32,
    pub bolus_rate_ul_per_min: f32,
    pub delivery_spreadcycle_enabled: bool,
    pub delivery_running: bool,
    pub delivery_kvo_active: bool,
    pub delivery_remaining_steps: u32,
    pub delivery_dose_steps: u32,
    pub dose_total_ul: f32,
    pub startup_count: u32,
    pub flash_write_count: u32,
}

// TMC2209 startup verification.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TmcStatus {
    Verified,
    NoResponse,
}
