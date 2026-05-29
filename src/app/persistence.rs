use crate::{
    dosing::{DeliveryState, DoseAccumulator, Prescription},
    persistent::{PersistentConfig, PersistentStore},
    types::RuntimeSettings,
};

/// Builds the flash snapshot from current UI, prescription, delivery, and dose state.
pub(super) fn persistent_config_snapshot(
    syringe_selected: usize,
    syringe_mounted: bool,
    carriage_position_steps: i32,
    prescription: &Prescription,
    settings: &RuntimeSettings,
    delivery: &DeliveryState,
    dose: &DoseAccumulator,
    bolus_volume_ul: f32,
    bolus_rate_ul_per_min: f32,
) -> PersistentConfig {
    PersistentConfig {
        syringe_index: syringe_selected,
        drug_index: prescription.drug_index,
        syringe_mounted,
        carriage_position_steps,
        flow_rate_ul_per_min: prescription.flow_rate_ul_per_min,
        vtbi_ul: prescription.vtbi_ul,
        infusion_time_min: prescription.infusion_time_min,
        dose_rate_ul_per_min: prescription.dose_rate_ul_per_min,
        patient_weight_kg: prescription.patient_weight_kg,
        kvo_enabled: settings.kvo_enabled,
        kvo_rate_ul_per_min: settings.kvo_rate_ul_per_min,
        direct_bolus_rate_ul_per_min: settings.direct_bolus_rate_ul_per_min,
        delivery_spreadcycle_enabled: settings.delivery_spreadcycle_enabled,
        bolus_volume_ul,
        bolus_rate_ul_per_min,
        delivery_running: delivery.running,
        delivery_kvo_active: delivery.kvo_active,
        delivery_remaining_steps: delivery.remaining_steps,
        delivery_dose_steps: delivery.dose_steps,
        dose_total_ul: dose.total_ul,
        startup_count: 0,
        flash_write_count: 0,
    }
    .clamped()
}

/// Saves current runtime state while preserving lifetime counters maintained by the store.
pub(super) fn save_persistent_config(
    store: &mut PersistentStore,
    syringe_selected: usize,
    syringe_mounted: bool,
    carriage_position_steps: i32,
    prescription: &Prescription,
    settings: &RuntimeSettings,
    delivery: &DeliveryState,
    dose: &DoseAccumulator,
    bolus_volume_ul: f32,
    bolus_rate_ul_per_min: f32,
) {
    let mut config = persistent_config_snapshot(
        syringe_selected,
        syringe_mounted,
        carriage_position_steps,
        prescription,
        settings,
        delivery,
        dose,
        bolus_volume_ul,
        bolus_rate_ul_per_min,
    );
    config.startup_count = store.startup_count();
    config.flash_write_count = store.flash_write_count();
    store.save(config);
}
