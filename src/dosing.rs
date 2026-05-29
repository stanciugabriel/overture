use crate::config::*;
pub use crate::types::{
    DeliveryAlert, DeliveryState, DoseAccumulator, DosingError, DrugSpec, Prescription, SyringeSpec,
};

// Syringe presets and measured positions.

pub const SYRINGE_PRESETS: [SyringeSpec; 3] = [
    SyringeSpec {
        label: "2.5 mL",
        inner_diameter_mm: SYRINGE_2_5ML_INNER_DIAMETER_MM,
        nominal_volume_ul: SYRINGE_2_5ML_NOMINAL_VOLUME_UL,
        measured_full_position_steps_from_home: Some(SYRINGE_2_5ML_FULL_POSITION_STEPS_FROM_HOME),
        measured_empty_position_steps_from_home: Some(SYRINGE_2_5ML_EMPTY_POSITION_STEPS_FROM_HOME),
    },
    SyringeSpec {
        label: "5 mL",
        inner_diameter_mm: SYRINGE_5ML_INNER_DIAMETER_MM,
        nominal_volume_ul: SYRINGE_5ML_NOMINAL_VOLUME_UL,
        measured_full_position_steps_from_home: Some(SYRINGE_5ML_FULL_POSITION_STEPS_FROM_HOME),
        measured_empty_position_steps_from_home: Some(SYRINGE_5ML_EMPTY_POSITION_STEPS_FROM_HOME),
    },
    SyringeSpec {
        label: "20 mL",
        inner_diameter_mm: SYRINGE_20ML_INNER_DIAMETER_MM,
        nominal_volume_ul: SYRINGE_20ML_NOMINAL_VOLUME_UL,
        measured_full_position_steps_from_home: Some(SYRINGE_20ML_FULL_POSITION_STEPS_FROM_HOME),
        measured_empty_position_steps_from_home: Some(SYRINGE_20ML_EMPTY_POSITION_STEPS_FROM_HOME),
    },
];

pub const SYRINGE_2_5ML_INDEX: usize = 0;
pub const SYRINGE_5ML_INDEX: usize = 1;
pub const SYRINGE_20ML_INDEX: usize = 2;
pub const DEFAULT_SYRINGE_INDEX: usize = SYRINGE_20ML_INDEX;

// Drug library used by the setup flow.

pub const DRUG_LIBRARY: [DrugSpec; 7] = [
    DrugSpec {
        drug_name: "Propofol",
        class: "Induction Agent",
        typical_concentration: "10 mg/mL",
        concentration_mg_per_ml: 10.0,
        color_rgb: [255, 255, 0],
    },
    DrugSpec {
        drug_name: "Fentanyl",
        class: "Opioid",
        typical_concentration: "50 mcg/mL",
        concentration_mg_per_ml: 0.05,
        color_rgb: [173, 216, 230],
    },
    DrugSpec {
        drug_name: "Rocuronium",
        class: "Muscle Relaxant",
        typical_concentration: "10 mg/mL",
        concentration_mg_per_ml: 10.0,
        color_rgb: [255, 51, 51],
    },
    DrugSpec {
        drug_name: "Epinephrine",
        class: "Vasopressor",
        typical_concentration: "1 mg/mL",
        concentration_mg_per_ml: 1.0,
        color_rgb: [204, 153, 255],
    },
    DrugSpec {
        drug_name: "Midazolam",
        class: "Tranquilizer",
        typical_concentration: "1 mg/mL",
        concentration_mg_per_ml: 1.0,
        color_rgb: [255, 165, 0],
    },
    DrugSpec {
        drug_name: "Lidocaine",
        class: "Local Anesthetic",
        typical_concentration: "10 mg/mL",
        concentration_mg_per_ml: 10.0,
        color_rgb: [128, 128, 128],
    },
    DrugSpec {
        drug_name: "Atropine",
        class: "Anticholinergic",
        typical_concentration: "1 mg/mL",
        concentration_mg_per_ml: 1.0,
        color_rgb: [0, 255, 0],
    },
];

pub const FENTANYL_DRUG_INDEX: usize = 1;

// Prescription editing, validation, and drug-dose synchronization.

impl Prescription {
    pub fn new() -> Self {
        let flow_rate_ul_per_min = if USE_VTBI_TIME_MODE {
            VTBI_UL / VTBI_TIME_MIN
        } else {
            DISPENSE_RATE_UL_PER_MIN
        };
        Self {
            syringe: SYRINGE_PRESETS[DEFAULT_SYRINGE_INDEX],
            drug_index: None,
            flow_rate_ul_per_min,
            vtbi_ul: if USE_VTBI_TIME_MODE {
                VTBI_UL
            } else {
                PRESET_DOSE_UL
            },
            infusion_time_min: VTBI_TIME_MIN,
            dose_rate_ul_per_min: flow_rate_ul_per_min,
            patient_weight_kg: 60.0,
        }
        .clamped()
    }

    pub fn validate(self) -> Result<(), DosingError> {
        if !self.flow_rate_ul_per_min.is_finite()
            || !self.vtbi_ul.is_finite()
            || !self.infusion_time_min.is_finite()
            || !self.dose_rate_ul_per_min.is_finite()
            || !self.patient_weight_kg.is_finite()
        {
            return Err(DosingError::NonFiniteValue);
        }

        if !(MIN_FLOW_RATE_UL_PER_MIN..=MAX_FLOW_RATE_UL_PER_MIN)
            .contains(&self.flow_rate_ul_per_min)
            || !(MIN_VTBI_UL..=MAX_VTBI_UL).contains(&self.vtbi_ul)
            || !(MIN_INFUSION_TIME_MIN..=MAX_INFUSION_TIME_MIN).contains(&self.infusion_time_min)
        {
            return Err(DosingError::InvalidPrescription);
        }

        Ok(())
    }

    pub fn apply_delta(&mut self, selected: usize, delta: i32) {
        let delta = delta as f32;
        match selected {
            0 if self.drug_index.is_some() => {
                self.dose_rate_ul_per_min += delta * 0.1;
                self.sync_flow_from_dose();
            }
            1 => {
                self.vtbi_ul += delta * 100.0;
                self.sync_rate_from_time();
            }
            2 if self.drug_index.is_none() => {
                self.flow_rate_ul_per_min += delta * 100.0 / 60.0;
                self.sync_time_from_rate();
            }
            3 => {
                self.infusion_time_min += delta * 0.1;
                self.sync_rate_from_time();
            }
            _ => {}
        }
        *self = self.clamped();
    }

    fn sync_rate_from_time(&mut self) {
        self.infusion_time_min = self
            .infusion_time_min
            .clamp(MIN_INFUSION_TIME_MIN, MAX_INFUSION_TIME_MIN);
        self.flow_rate_ul_per_min = (self.vtbi_ul / self.infusion_time_min)
            .clamp(MIN_FLOW_RATE_UL_PER_MIN, MAX_FLOW_RATE_UL_PER_MIN);
        self.dose_rate_ul_per_min = self.flow_rate_ul_per_min;
        self.sync_dose_from_flow();
    }

    fn sync_time_from_rate(&mut self) {
        self.flow_rate_ul_per_min = self
            .flow_rate_ul_per_min
            .clamp(MIN_FLOW_RATE_UL_PER_MIN, MAX_FLOW_RATE_UL_PER_MIN);
        self.infusion_time_min = (self.vtbi_ul / self.flow_rate_ul_per_min)
            .clamp(MIN_INFUSION_TIME_MIN, MAX_INFUSION_TIME_MIN);
        self.sync_dose_from_flow();
    }

    pub fn sync_dose_from_flow(&mut self) {
        if let Some(drug) = self.selected_drug() {
            let weight = self.patient_weight_kg.max(1.0);
            let flow_ml_h = self.flow_rate_ul_per_min * 60.0 / 1000.0;
            self.dose_rate_ul_per_min = flow_ml_h * drug.concentration_mg_per_ml / weight;
        }
    }

    fn sync_flow_from_dose(&mut self) {
        if let Some(drug) = self.selected_drug() {
            let flow_ml_h = self.dose_rate_ul_per_min.max(0.0) * self.patient_weight_kg.max(1.0)
                / drug.concentration_mg_per_ml.max(0.001);
            self.flow_rate_ul_per_min = (flow_ml_h * 1000.0 / 60.0)
                .clamp(MIN_FLOW_RATE_UL_PER_MIN, MAX_FLOW_RATE_UL_PER_MIN);
            self.sync_time_from_rate();
        }
    }

    pub fn clamped(mut self) -> Self {
        self.flow_rate_ul_per_min = clamp_finite(
            self.flow_rate_ul_per_min,
            MIN_FLOW_RATE_UL_PER_MIN,
            MAX_FLOW_RATE_UL_PER_MIN,
            DISPENSE_RATE_UL_PER_MIN,
        );
        self.vtbi_ul = clamp_finite(self.vtbi_ul, MIN_VTBI_UL, MAX_VTBI_UL, VTBI_UL);
        self.infusion_time_min = clamp_finite(
            self.infusion_time_min,
            MIN_INFUSION_TIME_MIN,
            MAX_INFUSION_TIME_MIN,
            VTBI_TIME_MIN,
        );
        self.dose_rate_ul_per_min = clamp_finite(self.dose_rate_ul_per_min, 0.0, 999.9, 5.0);
        self.patient_weight_kg = clamp_finite(self.patient_weight_kg, 1.0, 300.0, 60.0);
        if self
            .drug_index
            .map(|index| index >= DRUG_LIBRARY.len())
            .unwrap_or(false)
        {
            self.drug_index = None;
        }
        self
    }

    pub fn selected_drug(self) -> Option<DrugSpec> {
        self.drug_index
            .and_then(|index| DRUG_LIBRARY.get(index).copied())
    }
}

fn clamp_finite(value: f32, min: f32, max: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        fallback.clamp(min, max)
    }
}

impl Default for Prescription {
    fn default() -> Self {
        Self::new()
    }
}

// Delivery state and dose accounting.

impl DeliveryAlert {
    pub fn is_active(self) -> bool {
        !matches!(self, Self::None | Self::Standby)
    }
}

impl DeliveryState {
    pub fn new() -> Self {
        Self {
            running: false,
            remaining_steps: 0,
            dose_steps: 0,
            ramp_progress_steps: 0,
            delivered_steps_this_dose: 0,
            kvo_active: false,
            alert: DeliveryAlert::None,
        }
    }

    pub fn stop(&mut self) {
        self.running = false;
        self.kvo_active = false;
        self.remaining_steps = 0;
        self.ramp_progress_steps = 0;
    }
}

impl Default for DeliveryState {
    fn default() -> Self {
        Self::new()
    }
}

impl DoseAccumulator {
    pub fn new() -> Self {
        Self {
            fractional_steps: 0.0,
            total_ul: 0.0,
        }
    }

    pub fn from_total_ul(total_ul: f32) -> Self {
        Self {
            fractional_steps: 0.0,
            total_ul: total_ul.max(0.0),
        }
    }

    fn next_dose_steps(
        &mut self,
        target_ul: f32,
        prescription: &Prescription,
    ) -> Result<u32, DosingError> {
        let exact_steps = target_ul / ul_per_step(prescription) + self.fractional_steps;
        if !exact_steps.is_finite() || exact_steps < 1.0 || exact_steps > MAX_STEPS_PER_DOSE as f32
        {
            return Err(DosingError::StepCountOutOfRange);
        }

        let whole_steps = exact_steps as u32;
        self.fractional_steps = exact_steps - whole_steps as f32;
        Ok(whole_steps)
    }

    pub fn record_dispense(&mut self, steps: u32, prescription: &Prescription) {
        self.total_ul += steps as f32 * ul_per_step(prescription);
    }

    pub fn fractional_steps(&self) -> f32 {
        self.fractional_steps
    }
}

impl Default for DoseAccumulator {
    fn default() -> Self {
        Self::new()
    }
}

/// Validates the prescription, converts its target volume to steps, and arms delivery counters.
pub fn prepare_dose(
    dose: &mut DoseAccumulator,
    delivery: &mut DeliveryState,
    prescription: &Prescription,
) -> Result<u32, DosingError> {
    prescription.validate()?;
    let target_ul = delivery_target_ul(prescription);
    let steps = dose.next_dose_steps(target_ul, prescription)?;

    delivery.remaining_steps = steps;
    delivery.dose_steps = steps;
    delivery.ramp_progress_steps = 0;
    delivery.delivered_steps_this_dose = 0;
    delivery.kvo_active = false;
    delivery.alert = DeliveryAlert::None;
    Ok(steps)
}

/// Converts a requested fluid volume into motor steps, rounding up to avoid under-delivery.
pub fn steps_for_volume_ul(
    target_ul: f32,
    prescription: &Prescription,
) -> Result<u32, DosingError> {
    let exact_steps = target_ul / ul_per_step(prescription);
    if !exact_steps.is_finite() || exact_steps < 1.0 || exact_steps > MAX_STEPS_PER_DOSE as f32 {
        return Err(DosingError::StepCountOutOfRange);
    }

    let whole_steps = exact_steps as u32;
    if exact_steps > whole_steps as f32 {
        Ok(whole_steps.saturating_add(1))
    } else {
        Ok(whole_steps)
    }
}

/// Converts linear travel into motor steps, rounding up to guarantee at least the requested travel.
pub fn steps_for_travel_mm(target_mm: f32) -> Result<u32, DosingError> {
    let exact_steps = target_mm * steps_per_mm();
    if !exact_steps.is_finite() || exact_steps < 1.0 || exact_steps > MAX_STEPS_PER_DOSE as f32 {
        return Err(DosingError::StepCountOutOfRange);
    }

    let whole_steps = exact_steps as u32;
    if exact_steps > whole_steps as f32 {
        Ok(whole_steps.saturating_add(1))
    } else {
        Ok(whole_steps)
    }
}

/// Converts motor and lead-screw geometry into microsteps per millimeter.
pub fn steps_per_mm() -> f32 {
    MOTOR_FULL_STEPS_PER_REV * MICROSTEPS as f32 / LEAD_SCREW_LEAD_MM_PER_REV
}

/// Computes the syringe barrel cross-section used for volume-per-step conversion.
pub fn syringe_area_mm2(syringe: SyringeSpec) -> f32 {
    let radius_mm = syringe.inner_diameter_mm / 2.0;
    core::f32::consts::PI * radius_mm * radius_mm
}

/// Computes nominal plunger travel from syringe volume and barrel area.
pub fn syringe_plunger_travel_mm(syringe: SyringeSpec) -> f32 {
    syringe.nominal_volume_ul / syringe_area_mm2(syringe)
}

/// Adds insertion clearance to the nominal plunger travel for the load-open position.
pub fn syringe_load_travel_mm(syringe: SyringeSpec) -> f32 {
    syringe_plunger_travel_mm(syringe) + LOAD_CLEARANCE_MM
}

/// Converts one microstep of plunger motion into delivered microliters.
pub fn ul_per_step(prescription: &Prescription) -> f32 {
    syringe_area_mm2(prescription.syringe) / steps_per_mm() * CALIBRATION_UL_PER_STEP_SCALE
}

/// Returns the step period for the programmed delivery rate.
pub fn dispense_step_period_us(prescription: &Prescription) -> u64 {
    rate_step_period_us(delivery_rate_ul_per_min(prescription), prescription.syringe)
}

pub fn bolus_step_period_us(prescription: &Prescription) -> u64 {
    rate_step_period_us(BOLUS_RATE_UL_PER_MIN, prescription.syringe)
}

pub fn kvo_step_period_us(prescription: &Prescription) -> u64 {
    rate_step_period_us(KVO_RATE_UL_PER_MIN, prescription.syringe)
}

pub fn delivery_target_ul(prescription: &Prescription) -> f32 {
    prescription.vtbi_ul
}

pub fn delivery_rate_ul_per_min(prescription: &Prescription) -> f32 {
    prescription.flow_rate_ul_per_min
}

/// Converts a volumetric flow rate into the STEP pulse period used by the motor task.
pub fn rate_step_period_us(rate_ul_per_min: f32, syringe: SyringeSpec) -> u64 {
    let steps_per_second = rate_ul_per_min
        / 60.0
        / (syringe_area_mm2(syringe) / steps_per_mm() * CALIBRATION_UL_PER_STEP_SCALE);
    let period_us = (1_000_000.0 / steps_per_second) as u64;
    period_us.max(MIN_STEP_PERIOD_US)
}

/// Starts fast deliveries at a slower period and linearly ramps to the target period.
pub fn ramped_dispense_step_period_us(
    ramp_progress_steps: u32,
    prescription: &Prescription,
) -> u64 {
    let target_period_us = dispense_step_period_us(prescription);

    if DISPENSE_START_STEP_PERIOD_US <= target_period_us || DISPENSE_RAMP_STEPS == 0 {
        return target_period_us;
    }

    let ramp_step = ramp_progress_steps.min(DISPENSE_RAMP_STEPS);
    let period_delta = DISPENSE_START_STEP_PERIOD_US - target_period_us;
    DISPENSE_START_STEP_PERIOD_US - period_delta * ramp_step as u64 / DISPENSE_RAMP_STEPS as u64
}

pub fn approx_delivery_seconds(steps: u32, period_us: u64) -> f32 {
    steps as f32 * period_us as f32 / 1_000_000.0
}

// Display-friendly helpers.

pub fn rate_ml_h(prescription: &Prescription) -> f32 {
    prescription.flow_rate_ul_per_min * 60.0 / 1000.0
}

pub fn ul_to_ml_parts(ul: f32) -> (u32, u32) {
    let ml_x100 = (ul / 10.0) as u32;
    (ml_x100 / 100, ml_x100 % 100)
}

pub fn delivery_chopper_mode_name() -> &'static str {
    chopper_mode_name(USE_SPREADCYCLE_FOR_DELIVERY)
}

pub fn chopper_mode_name(spreadcycle_enabled: bool) -> &'static str {
    if spreadcycle_enabled {
        "SpreadCycle"
    } else {
        "StealthChop"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(actual: f32, expected: f32, tolerance: f32) {
        assert!(
            (actual - expected).abs() <= tolerance,
            "actual={actual} expected={expected} tolerance={tolerance}"
        );
    }

    #[test]
    fn default_prescription_uses_vtbi_time_defaults() {
        let prescription = Prescription::new();

        assert_eq!(prescription.syringe, SYRINGE_PRESETS[DEFAULT_SYRINGE_INDEX]);
        assert_eq!(prescription.vtbi_ul, VTBI_UL);
        assert_eq!(prescription.infusion_time_min, VTBI_TIME_MIN);
        assert_close(
            prescription.flow_rate_ul_per_min,
            VTBI_UL / VTBI_TIME_MIN,
            0.01,
        );
        assert_eq!(prescription.validate(), Ok(()));
    }

    #[test]
    fn prescription_clamp_rejects_bad_drug_and_non_finite_values() {
        let mut prescription = Prescription::new();
        prescription.drug_index = Some(DRUG_LIBRARY.len());
        prescription.flow_rate_ul_per_min = f32::NAN;

        assert_eq!(prescription.validate(), Err(DosingError::NonFiniteValue));

        let clamped = prescription.clamped();
        assert_eq!(clamped.drug_index, None);
        assert_eq!(clamped.flow_rate_ul_per_min, DISPENSE_RATE_UL_PER_MIN);
    }

    #[test]
    fn drug_dose_rate_tracks_flow_rate_and_weight() {
        let mut prescription = Prescription::new();
        prescription.drug_index = Some(FENTANYL_DRUG_INDEX);
        prescription.patient_weight_kg = 50.0;
        prescription.flow_rate_ul_per_min = 1_000.0;

        prescription.sync_dose_from_flow();

        let fentanyl = DRUG_LIBRARY[FENTANYL_DRUG_INDEX];
        let expected = 60.0 * fentanyl.concentration_mg_per_ml / 50.0;
        assert_close(prescription.dose_rate_ul_per_min, expected, 0.001);
    }

    #[test]
    fn volume_step_conversion_rounds_up_partial_steps() {
        let prescription = Prescription::new();
        let single_step_ul = ul_per_step(&prescription);

        assert_eq!(
            steps_for_volume_ul(single_step_ul * 10.0, &prescription),
            Ok(10)
        );
        assert_eq!(
            steps_for_volume_ul(single_step_ul * 10.25, &prescription),
            Ok(11)
        );
    }

    #[test]
    fn prepare_dose_sets_delivery_counters() {
        let prescription = Prescription::new();
        let mut dose = DoseAccumulator::new();
        let mut delivery = DeliveryState::new();

        let steps = prepare_dose(&mut dose, &mut delivery, &prescription).unwrap();

        assert!(steps > 0);
        assert_eq!(delivery.remaining_steps, steps);
        assert_eq!(delivery.dose_steps, steps);
        assert_eq!(delivery.ramp_progress_steps, 0);
        assert_eq!(delivery.delivered_steps_this_dose, 0);
        assert!(!delivery.kvo_active);
        assert_eq!(delivery.alert, DeliveryAlert::None);
    }
}
