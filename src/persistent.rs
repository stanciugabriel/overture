#![cfg_attr(not(target_os = "none"), allow(dead_code))]

#[cfg(target_os = "none")]
use embedded_storage::nor_flash::NorFlash;
#[cfg(target_os = "none")]
use embedded_storage::{ReadStorage, Storage};
#[cfg(target_os = "none")]
use esp_bootloader_esp_idf::partitions::{
    DataPartitionSubType, PARTITION_TABLE_MAX_LEN, PartitionType, read_partition_table,
};
#[cfg(target_os = "none")]
use esp_hal::peripherals::FLASH;
#[cfg(target_os = "none")]
use esp_storage::FlashStorage;

pub use crate::types::PersistentConfig;
use crate::{
    config::*,
    dosing::{DEFAULT_SYRINGE_INDEX, DRUG_LIBRARY, Prescription, SYRINGE_PRESETS},
};

// Flash record format.

const STORE_MAGIC: u32 = 0x4354_564F; // "OVTC"
const STORE_VERSION: u16 = 6;
const LEGACY_RECORD_SIZE: usize = 64;
const V2_RECORD_SIZE: usize = 80;
const RECORD_SIZE: usize = 88;
const NO_DRUG_INDEX: u8 = 0xFF;

// Persistent snapshot defaults and validation.

impl PersistentConfig {
    pub fn from_defaults() -> Self {
        let prescription = Prescription::new();
        Self {
            syringe_index: DEFAULT_SYRINGE_INDEX,
            drug_index: None,
            syringe_mounted: BOOT_ASSUME_SYRINGE_MOUNTED,
            carriage_position_steps: BOOT_CARRIAGE_POSITION_STEPS_FROM_HOME
                .clamp(0, CARRIAGE_HARD_LIMIT_STEPS_FROM_HOME),
            flow_rate_ul_per_min: prescription.flow_rate_ul_per_min,
            vtbi_ul: prescription.vtbi_ul,
            infusion_time_min: prescription.infusion_time_min,
            dose_rate_ul_per_min: prescription.dose_rate_ul_per_min,
            patient_weight_kg: prescription.patient_weight_kg,
            kvo_enabled: KVO_ENABLED,
            kvo_rate_ul_per_min: KVO_RATE_UL_PER_MIN,
            direct_bolus_rate_ul_per_min: DIRECT_BOLUS_RATE_UL_PER_MIN,
            bolus_volume_ul: DEFAULT_BOLUS_VOLUME_UL,
            bolus_rate_ul_per_min: BOLUS_RATE_UL_PER_MIN,
            delivery_spreadcycle_enabled: USE_SPREADCYCLE_FOR_DELIVERY,
            delivery_running: false,
            delivery_kvo_active: false,
            delivery_remaining_steps: 0,
            delivery_dose_steps: 0,
            dose_total_ul: 0.0,
            startup_count: 0,
            flash_write_count: 0,
        }
        .clamped()
    }

    pub fn prescription(self) -> Prescription {
        Prescription {
            syringe: SYRINGE_PRESETS[self.syringe_index],
            drug_index: self.drug_index,
            flow_rate_ul_per_min: self.flow_rate_ul_per_min,
            vtbi_ul: self.vtbi_ul,
            infusion_time_min: self.infusion_time_min,
            dose_rate_ul_per_min: self.dose_rate_ul_per_min,
            patient_weight_kg: self.patient_weight_kg,
        }
        .clamped()
    }

    pub fn clamped(mut self) -> Self {
        if self.syringe_index >= SYRINGE_PRESETS.len() {
            self.syringe_index = DEFAULT_SYRINGE_INDEX;
        }
        if self
            .drug_index
            .map(|index| index >= DRUG_LIBRARY.len())
            .unwrap_or(false)
        {
            self.drug_index = None;
        }
        self.carriage_position_steps = self
            .carriage_position_steps
            .clamp(0, CARRIAGE_HARD_LIMIT_STEPS_FROM_HOME);
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
        self.kvo_rate_ul_per_min = clamp_finite(
            self.kvo_rate_ul_per_min,
            MIN_FLOW_RATE_UL_PER_MIN,
            MAX_FLOW_RATE_UL_PER_MIN,
            KVO_RATE_UL_PER_MIN,
        );
        self.direct_bolus_rate_ul_per_min = clamp_finite(
            self.direct_bolus_rate_ul_per_min,
            MIN_FLOW_RATE_UL_PER_MIN,
            MAX_BOLUS_RATE_UL_PER_MIN,
            DIRECT_BOLUS_RATE_UL_PER_MIN,
        );
        self.bolus_volume_ul = clamp_finite(
            self.bolus_volume_ul,
            MIN_BOLUS_VOLUME_UL,
            MAX_BOLUS_VOLUME_UL,
            DEFAULT_BOLUS_VOLUME_UL,
        );
        self.bolus_rate_ul_per_min = clamp_finite(
            self.bolus_rate_ul_per_min,
            MIN_FLOW_RATE_UL_PER_MIN,
            MAX_BOLUS_RATE_UL_PER_MIN,
            BOLUS_RATE_UL_PER_MIN,
        );
        self.dose_total_ul = clamp_finite(self.dose_total_ul, 0.0, MAX_VTBI_UL, 0.0);
        if self.delivery_remaining_steps == 0 && !self.delivery_kvo_active {
            self.delivery_running = false;
        }
        self
    }
}

// Flash-backed persistent store.

#[cfg(target_os = "none")]
pub struct PersistentStore {
    flash: FlashStorage<'static>,
    offset: u32,
    last_saved: PersistentConfig,
}

#[cfg(target_os = "none")]
impl PersistentStore {
    pub fn new(flash: FLASH<'static>) -> Self {
        let mut flash = FlashStorage::new(flash);
        let offset = config_sector_offset(&mut flash).unwrap_or_else(|| {
            let capacity = flash.capacity() as u32;
            capacity.saturating_sub(FlashStorage::SECTOR_SIZE)
        });
        Self {
            flash,
            offset,
            last_saved: PersistentConfig::from_defaults(),
        }
    }

    pub fn load(&mut self) -> PersistentConfig {
        let mut bytes = [0xFF; RECORD_SIZE];
        let loaded = if self.flash.read(self.offset, &mut bytes).is_ok() {
            decode_config_record(&bytes).unwrap_or_else(PersistentConfig::from_defaults)
        } else {
            PersistentConfig::from_defaults()
        };
        self.last_saved = loaded;
        loaded
    }

    pub fn save(&mut self, config: PersistentConfig) {
        let mut config = config.clamped();
        config.flash_write_count = self.last_saved.flash_write_count;
        if config == self.last_saved {
            return;
        }
        config.flash_write_count = config.flash_write_count.saturating_add(1);

        let mut sector = [0xFF; FlashStorage::SECTOR_SIZE as usize];
        encode_config_record(config, &mut sector[..RECORD_SIZE]);

        if self
            .flash
            .erase(self.offset, self.offset + FlashStorage::SECTOR_SIZE)
            .is_err()
        {
            log::warn!("persistent config erase failed");
            return;
        }

        if Storage::write(&mut self.flash, self.offset, &sector).is_err() {
            log::warn!("persistent config write failed");
            return;
        }

        self.last_saved = config;
        log::info!(
            "persistent config saved at flash offset 0x{:X}",
            self.offset
        );
    }

    pub fn startup_count(&self) -> u32 {
        self.last_saved.startup_count
    }

    pub fn flash_write_count(&self) -> u32 {
        self.last_saved.flash_write_count
    }
}

// Partition selection.

#[cfg(target_os = "none")]
fn config_sector_offset(flash: &mut FlashStorage<'static>) -> Option<u32> {
    let mut table = [0u8; PARTITION_TABLE_MAX_LEN];
    let partition_table = read_partition_table(flash, &mut table).ok()?;

    let partition = partition_table
        .iter()
        .find(|entry| entry.label_as_str() == "storage")
        .or_else(|| {
            partition_table
                .find_partition(PartitionType::Data(DataPartitionSubType::Nvs))
                .ok()
                .flatten()
        })?;

    let sector_size = FlashStorage::SECTOR_SIZE;
    if partition.len() < sector_size {
        return None;
    }

    Some(partition.offset() + partition.len() - sector_size)
}

/// Serializes the current configuration into the fixed flash record layout.
fn encode_config_record(config: PersistentConfig, out: &mut [u8]) {
    out.fill(0xFF);
    put_u32(out, 0, STORE_MAGIC);
    put_u16(out, 4, STORE_VERSION);
    put_u16(out, 6, RECORD_SIZE as u16);
    out[8] = config.syringe_index as u8;
    out[9] = u8::from(config.syringe_mounted);
    out[10] = config
        .drug_index
        .map(|index| index as u8)
        .unwrap_or(NO_DRUG_INDEX);
    put_i32(out, 12, config.carriage_position_steps);
    put_f32(out, 16, config.flow_rate_ul_per_min);
    put_f32(out, 20, config.vtbi_ul);
    put_f32(out, 24, config.infusion_time_min);
    put_f32(out, 28, config.dose_rate_ul_per_min);
    put_f32(out, 68, config.patient_weight_kg);
    out[32] = u8::from(config.kvo_enabled);
    put_f32(out, 36, config.kvo_rate_ul_per_min);
    put_f32(out, 40, config.direct_bolus_rate_ul_per_min);
    put_f32(out, 44, config.bolus_volume_ul);
    put_f32(out, 48, config.bolus_rate_ul_per_min);
    out[52] = u8::from(config.delivery_running);
    out[53] = u8::from(config.delivery_kvo_active);
    out[54] = u8::from(config.delivery_spreadcycle_enabled);
    put_u32(out, 56, config.delivery_remaining_steps);
    put_u32(out, 60, config.delivery_dose_steps);
    put_f32(out, 64, config.dose_total_ul);
    put_u32(out, 72, config.startup_count);
    put_u32(out, 76, config.flash_write_count);

    let checksum = checksum(&out[..RECORD_SIZE - 4]);
    put_u32(out, RECORD_SIZE - 4, checksum);
}

/// Decodes the current record format after validating magic, version, size, and checksum.
fn decode_config_record(bytes: &[u8; RECORD_SIZE]) -> Option<PersistentConfig> {
    if get_u32(bytes, 0) != STORE_MAGIC
        || get_u16(bytes, 4) != STORE_VERSION
        || get_u16(bytes, 6) as usize != RECORD_SIZE
    {
        return decode_legacy_config_record(bytes);
    }

    let expected = get_u32(bytes, RECORD_SIZE - 4);
    if checksum(&bytes[..RECORD_SIZE - 4]) != expected {
        return None;
    }

    Some(
        PersistentConfig {
            syringe_index: bytes[8] as usize,
            drug_index: decode_drug_index(bytes[10]),
            syringe_mounted: bytes[9] != 0,
            carriage_position_steps: get_i32(bytes, 12),
            flow_rate_ul_per_min: get_f32(bytes, 16),
            vtbi_ul: get_f32(bytes, 20),
            infusion_time_min: get_f32(bytes, 24),
            dose_rate_ul_per_min: get_f32(bytes, 28),
            patient_weight_kg: get_f32(bytes, 68),
            kvo_enabled: bytes[32] != 0,
            kvo_rate_ul_per_min: get_f32(bytes, 36),
            direct_bolus_rate_ul_per_min: get_f32(bytes, 40),
            bolus_volume_ul: get_f32(bytes, 44),
            bolus_rate_ul_per_min: get_f32(bytes, 48),
            delivery_spreadcycle_enabled: bytes[54] != 0,
            delivery_running: bytes[52] != 0,
            delivery_kvo_active: bytes[53] != 0,
            delivery_remaining_steps: get_u32(bytes, 56),
            delivery_dose_steps: get_u32(bytes, 60),
            dose_total_ul: get_f32(bytes, 64),
            startup_count: get_u32(bytes, 72),
            flash_write_count: get_u32(bytes, 76),
        }
        .clamped(),
    )
}

/// Keeps older records readable so firmware updates do not erase saved pump state.
fn decode_legacy_config_record(bytes: &[u8; RECORD_SIZE]) -> Option<PersistentConfig> {
    if get_u32(bytes, 0) != STORE_MAGIC {
        return None;
    }

    let version = get_u16(bytes, 4);
    let record_size = get_u16(bytes, 6) as usize;
    if !((version == 1 && record_size == LEGACY_RECORD_SIZE)
        || ((version == 2 || version == 3 || version == 4 || version == 5)
            && record_size == V2_RECORD_SIZE))
    {
        return None;
    }

    let expected = get_u32(bytes, record_size - 4);
    if checksum(&bytes[..record_size - 4]) != expected {
        return None;
    }

    let mut config = PersistentConfig {
        syringe_index: bytes[8] as usize,
        drug_index: None,
        syringe_mounted: bytes[9] != 0,
        carriage_position_steps: get_i32(bytes, 12),
        flow_rate_ul_per_min: get_f32(bytes, 16),
        vtbi_ul: get_f32(bytes, 20),
        infusion_time_min: get_f32(bytes, 24),
        dose_rate_ul_per_min: get_f32(bytes, 28),
        patient_weight_kg: 60.0,
        kvo_enabled: bytes[32] != 0,
        kvo_rate_ul_per_min: get_f32(bytes, 36),
        direct_bolus_rate_ul_per_min: get_f32(bytes, 40),
        bolus_volume_ul: get_f32(bytes, 44),
        bolus_rate_ul_per_min: get_f32(bytes, 48),
        delivery_spreadcycle_enabled: USE_SPREADCYCLE_FOR_DELIVERY,
        delivery_running: false,
        delivery_kvo_active: false,
        delivery_remaining_steps: 0,
        delivery_dose_steps: 0,
        dose_total_ul: 0.0,
        startup_count: 0,
        flash_write_count: 0,
    };

    if version == 2 || version == 3 {
        config.delivery_running = bytes[52] != 0;
        config.delivery_kvo_active = bytes[53] != 0;
        if version == 3 {
            config.delivery_spreadcycle_enabled = bytes[54] != 0;
        }
        config.delivery_remaining_steps = get_u32(bytes, 56);
        config.delivery_dose_steps = get_u32(bytes, 60);
        config.dose_total_ul = get_f32(bytes, 64);
    }
    if version == 4 {
        config.drug_index = decode_drug_index(bytes[10]);
        config.delivery_running = bytes[52] != 0;
        config.delivery_kvo_active = bytes[53] != 0;
        config.delivery_spreadcycle_enabled = bytes[54] != 0;
        config.delivery_remaining_steps = get_u32(bytes, 56);
        config.delivery_dose_steps = get_u32(bytes, 60);
        config.dose_total_ul = get_f32(bytes, 64);
    }
    if version == 5 {
        config.drug_index = decode_drug_index(bytes[10]);
        config.delivery_running = bytes[52] != 0;
        config.delivery_kvo_active = bytes[53] != 0;
        config.delivery_spreadcycle_enabled = bytes[54] != 0;
        config.delivery_remaining_steps = get_u32(bytes, 56);
        config.delivery_dose_steps = get_u32(bytes, 60);
        config.dose_total_ul = get_f32(bytes, 64);
        config.patient_weight_kg = get_f32(bytes, 68);
    }

    Some(config.clamped())
}

// Small binary helpers.

fn decode_drug_index(value: u8) -> Option<usize> {
    if value == NO_DRUG_INDEX {
        None
    } else {
        Some(value as usize)
    }
}

/// Lightweight checksum used to reject partially-written or corrupted records.
fn checksum(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0x811C_9DC5, |hash, byte| {
        hash.wrapping_mul(0x0100_0193) ^ u32::from(*byte)
    })
}

fn clamp_finite(value: f32, min: f32, max: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        fallback.clamp(min, max)
    }
}

fn put_u16(out: &mut [u8], offset: usize, value: u16) {
    out[offset..][..2].copy_from_slice(&value.to_le_bytes());
}

fn put_u32(out: &mut [u8], offset: usize, value: u32) {
    out[offset..][..4].copy_from_slice(&value.to_le_bytes());
}

fn put_i32(out: &mut [u8], offset: usize, value: i32) {
    out[offset..][..4].copy_from_slice(&value.to_le_bytes());
}

fn put_f32(out: &mut [u8], offset: usize, value: f32) {
    out[offset..][..4].copy_from_slice(&value.to_le_bytes());
}

fn get_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(bytes[offset..][..2].try_into().unwrap_or([0; 2]))
}

fn get_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..][..4].try_into().unwrap_or([0; 4]))
}

fn get_i32(bytes: &[u8], offset: usize) -> i32 {
    i32::from_le_bytes(bytes[offset..][..4].try_into().unwrap_or([0; 4]))
}

fn get_f32(bytes: &[u8], offset: usize) -> f32 {
    f32::from_le_bytes(bytes[offset..][..4].try_into().unwrap_or([0; 4]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_config() -> PersistentConfig {
        PersistentConfig {
            syringe_index: 1,
            drug_index: Some(2),
            syringe_mounted: true,
            carriage_position_steps: 12_345,
            flow_rate_ul_per_min: 2_500.0,
            vtbi_ul: 1_750.0,
            infusion_time_min: 42.0,
            dose_rate_ul_per_min: 7.5,
            patient_weight_kg: 72.5,
            kvo_enabled: true,
            kvo_rate_ul_per_min: 125.0,
            direct_bolus_rate_ul_per_min: 15_000.0,
            bolus_volume_ul: 300.0,
            bolus_rate_ul_per_min: 12_000.0,
            delivery_spreadcycle_enabled: false,
            delivery_running: true,
            delivery_kvo_active: false,
            delivery_remaining_steps: 321,
            delivery_dose_steps: 654,
            dose_total_ul: 987.0,
            startup_count: 4,
            flash_write_count: 9,
        }
    }

    #[test]
    fn current_record_round_trips_all_persisted_fields() {
        let config = sample_config().clamped();
        let mut bytes = [0xFF; RECORD_SIZE];

        encode_config_record(config, &mut bytes);
        let decoded = decode_config_record(&bytes).unwrap();

        assert_eq!(decoded, config);
    }

    #[test]
    fn current_record_rejects_checksum_mismatch() {
        let config = sample_config().clamped();
        let mut bytes = [0xFF; RECORD_SIZE];

        encode_config_record(config, &mut bytes);
        bytes[16] ^= 0x01;

        assert_eq!(decode_config_record(&bytes), None);
    }

    #[test]
    fn persistent_config_clamps_invalid_saved_values() {
        let mut config = sample_config();
        config.syringe_index = usize::MAX;
        config.drug_index = Some(usize::MAX);
        config.carriage_position_steps = CARRIAGE_HARD_LIMIT_STEPS_FROM_HOME + 100;
        config.flow_rate_ul_per_min = f32::INFINITY;
        config.vtbi_ul = f32::NAN;
        config.patient_weight_kg = -1.0;
        config.delivery_running = true;
        config.delivery_remaining_steps = 0;
        config.delivery_kvo_active = false;

        let clamped = config.clamped();

        assert_eq!(clamped.syringe_index, DEFAULT_SYRINGE_INDEX);
        assert_eq!(clamped.drug_index, None);
        assert_eq!(
            clamped.carriage_position_steps,
            CARRIAGE_HARD_LIMIT_STEPS_FROM_HOME
        );
        assert_eq!(clamped.flow_rate_ul_per_min, DISPENSE_RATE_UL_PER_MIN);
        assert_eq!(clamped.vtbi_ul, VTBI_UL);
        assert_eq!(clamped.patient_weight_kg, 1.0);
        assert!(!clamped.delivery_running);
    }

    #[test]
    fn prescription_from_persistent_config_uses_selected_syringe_and_drug() {
        let config = sample_config().clamped();
        let prescription = config.prescription();

        assert_eq!(prescription.syringe, SYRINGE_PRESETS[1]);
        assert_eq!(prescription.drug_index, Some(2));
        assert_eq!(
            prescription.flow_rate_ul_per_min,
            config.flow_rate_ul_per_min
        );
        assert_eq!(prescription.patient_weight_kg, config.patient_weight_kg);
    }
}
