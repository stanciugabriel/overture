use embassy_time::Duration;

// Display and drawing

/// Display width in landscape pixels.
pub const DISPLAY_W: u32 = 480;
/// Display height in landscape pixels.
pub const DISPLAY_H: u32 = 320;
/// RGB565 full-frame backing buffer size.
pub const FRAME_BUFFER_SIZE: usize = DISPLAY_W as usize * DISPLAY_H as usize * 2;
/// Number of display rows pushed per SPI flush chunk.
pub const FLUSH_ROWS: usize = 8;
/// RGB666 flush scratch buffer size.
pub const FLUSH_BUFFER_SIZE: usize = DISPLAY_W as usize * FLUSH_ROWS * 3;
/// SPI clock used for the ILI9488 display.
pub const DISPLAY_SPI_HZ: u32 = 54_000_000;
/// Animated flow chevrons shown on the dashboard.
pub const FLOW_TRIANGLES: usize = 5;

// Temporary debug and calibration switches

/// Skips blocking startup faults and homing for bench work when set to 1.
pub const DEBUG_BYPASS_STARTUP_BLOCKS: u8 = 0;
/// Sends TMC writes to addresses 0..=3 for UART address probing when set to 1.
pub const DEBUG_TMC_WRITE_ALL_UART_ADDRS: u8 = 0;
// Startup recovery defaults

/// Development boot override for assuming a syringe is already mounted.
pub const BOOT_ASSUME_SYRINGE_MOUNTED: bool = false;
/// Development boot override for remembered carriage position.
pub const BOOT_CARRIAGE_POSITION_STEPS_FROM_HOME: i32 = 0;

// Measured carriage positions

/// Maximum safe carriage travel from the homing switch origin.
pub const CARRIAGE_HARD_LIMIT_STEPS_FROM_HOME: i32 = 46_600;
/// Measured 20 mL syringe load/full position from home.
pub const SYRINGE_20ML_FULL_POSITION_STEPS_FROM_HOME: i32 = 32_100;
/// Measured 20 mL syringe empty position from home.
pub const SYRINGE_20ML_EMPTY_POSITION_STEPS_FROM_HOME: i32 = 46_380;
/// Measured 5 mL syringe load/full position from home.
pub const SYRINGE_5ML_FULL_POSITION_STEPS_FROM_HOME: i32 = 37_900;
/// Measured 5 mL syringe empty position from home.
pub const SYRINGE_5ML_EMPTY_POSITION_STEPS_FROM_HOME: i32 = 47_700;
/// Measured 2.5 mL syringe load/full position from home.
pub const SYRINGE_2_5ML_FULL_POSITION_STEPS_FROM_HOME: i32 = 38_600;
/// Measured 2.5 mL syringe empty position from home.
pub const SYRINGE_2_5ML_EMPTY_POSITION_STEPS_FROM_HOME: i32 = 47_700;

// TMC2209 stepper driver

/// Current TMC2209 UART slave address.
pub const TMC2209_ADDR: u8 = 3;
/// TMC2209 UART baud rate.
pub const TMC_UART_BAUD: u32 = 57_600;
/// Default delivery chopper mode; false means StealthChop.
pub const USE_SPREADCYCLE_FOR_DELIVERY: bool = false;

// I2C devices and NFC tags

/// PN532 I2C address.
pub const PN532_I2C_ADDR: u8 = 0x24;
/// NFC polling cadence while waiting for syringe tags.
pub const PN532_POLL_INTERVAL_MS: u64 = 500;
/// Known 2.5 mL syringe NFC UID.
pub const NFC_2_5ML_TAG_UID: [u8; 7] = [0x5A, 0x04, 0xCE, 0x5D, 0x0F, 0x41, 0x89];
/// Known 5 mL syringe NFC UID.
pub const NFC_5ML_TAG_UID: [u8; 7] = [0x5A, 0xA4, 0x06, 0x5C, 0x0F, 0x41, 0x89];
/// Known 20 mL syringe NFC UID.
pub const NFC_20ML_TAG_UID: [u8; 7] = [0x5A, 0x04, 0x82, 0x5A, 0x0F, 0x41, 0x89];

// STUSB4500 USB-PD power validation

/// STUSB4500 I2C address.
pub const STUSB4500_I2C_ADDR: u8 = 0x2B;
/// STUSB4500 RDO MSB register used to decode the accepted PDO.
pub const STUSB4500_RDO_MSB_REG: u8 = 0x94;
/// Accepted PDO object positions for the required 20 V contract.
pub const STUSB4500_ALLOWED_20V_OBJECT_POSITIONS: [u8; 2] = [4, 5];
/// Retry delay while waiting for a valid USB-PD contract.
pub const STUSB4500_POWER_CHECK_RETRY_MS: u64 = 1_000;

// Mechanical scaling and syringe geometry

/// Motor full steps per revolution.
pub const MOTOR_FULL_STEPS_PER_REV: f32 = 200.0;
/// TMC microstep setting used for normal motion.
pub const MICROSTEPS: u32 = 8;
/// Scale factor from song full-step counts to normal microsteps.
pub const TONE_FULL_STEP_SCALE: u32 = MICROSTEPS;
/// Lead screw travel per revolution.
pub const LEAD_SCREW_LEAD_MM_PER_REV: f32 = 8.0;
/// 2.5 mL syringe inner diameter.
pub const SYRINGE_2_5ML_INNER_DIAMETER_MM: f32 = 8.9;
/// 5 mL syringe inner diameter.
pub const SYRINGE_5ML_INNER_DIAMETER_MM: f32 = 12.00;
/// 20 mL syringe inner diameter.
pub const SYRINGE_20ML_INNER_DIAMETER_MM: f32 = 19.13;
/// 2.5 mL syringe nominal capacity.
pub const SYRINGE_2_5ML_NOMINAL_VOLUME_UL: f32 = 2_500.0;
/// 5 mL syringe nominal capacity.
pub const SYRINGE_5ML_NOMINAL_VOLUME_UL: f32 = 5_000.0;
/// 20 mL syringe nominal capacity.
pub const SYRINGE_20ML_NOMINAL_VOLUME_UL: f32 = 20_000.0;
/// Global volume-per-step calibration multiplier.
pub const CALIBRATION_UL_PER_STEP_SCALE: f32 = 1.0;

// Default prescription and bolus settings

/// Legacy preset dose volume.
pub const PRESET_DOSE_UL: f32 = 1000.0;
/// Legacy default dispense rate.
pub const DISPENSE_RATE_UL_PER_MIN: f32 = 10000.0;
/// Starts setup in VTBI/time mode when true.
pub const USE_VTBI_TIME_MODE: bool = true;
/// Default volume to be infused.
pub const VTBI_UL: f32 = 1000.0;
/// Default infusion time in minutes.
pub const VTBI_TIME_MIN: f32 = 0.3;
/// Default confirmed bolus rate.
pub const BOLUS_RATE_UL_PER_MIN: f32 = 30000.0;
/// Default KVO enable state.
pub const KVO_ENABLED: bool = false;
/// Default KVO rate.
pub const KVO_RATE_UL_PER_MIN: f32 = 50.0;
/// Default confirmed bolus volume.
pub const DEFAULT_BOLUS_VOLUME_UL: f32 = 250.0;
/// Default held direct-bolus rate.
pub const DIRECT_BOLUS_RATE_UL_PER_MIN: f32 = 60_000.0;
/// Upper limit for held direct-bolus rate.
pub const MAX_BOLUS_RATE_UL_PER_MIN: f32 = 300_000.0;
/// Delivery rate above which SpreadCycle is selected automatically.
pub const FAST_FLOW_SPREADCYCLE_RATE_ML_PER_H: f32 = 200.0;

// Prescription and dosing limits

/// Minimum editable flow rate.
pub const MIN_FLOW_RATE_UL_PER_MIN: f32 = 100.0;
/// Maximum editable flow rate.
pub const MAX_FLOW_RATE_UL_PER_MIN: f32 = 60_000.0;
/// Minimum editable VTBI.
pub const MIN_VTBI_UL: f32 = 100.0;
/// Maximum editable VTBI.
pub const MAX_VTBI_UL: f32 = 60_000.0;
/// Minimum confirmed bolus volume.
pub const MIN_BOLUS_VOLUME_UL: f32 = 10.0;
/// Maximum confirmed bolus volume.
pub const MAX_BOLUS_VOLUME_UL: f32 = 10_000.0;
/// Minimum infusion duration.
pub const MIN_INFUSION_TIME_MIN: f32 = 0.1;
/// Maximum infusion duration.
pub const MAX_INFUSION_TIME_MIN: f32 = 1_440.0;
/// Minimum safe step period, keeping STEP high time valid.
pub const MIN_STEP_PERIOD_US: u64 = STEP_HIGH_US + 1;
/// Upper guard for generated dose step counts.
pub const MAX_STEPS_PER_DOSE: u32 = 1_000_000;

// Motion timing and travel

/// TMC2209 STEP high pulse width.
pub const STEP_HIGH_US: u64 = 4;
/// General retract period.
pub const RETRACT_STEP_PERIOD_US: u64 = 1_000;
/// Homing approach period.
pub const HOMING_APPROACH_STEP_PERIOD_US: u64 = 300;
/// Loader opening period.
pub const LOAD_OPEN_STEP_PERIOD_US: u64 = 500;
/// Load-adjust encoder nudge period.
pub const LOAD_APPROACH_STEP_PERIOD_US: u64 = 500;
/// Load-adjust held fine advance period.
pub const LOAD_FINE_ADVANCE_STEP_PERIOD_US: u64 = 2_000;
/// Syringe insertion approach period.
pub const SYRINGE_INSERT_APPROACH_STEP_PERIOD_US: u64 = 200;
/// Prime held-bolus period.
pub const PRIME_STEP_PERIOD_US: u64 = 20_000;
/// Encoder nudge size in load-adjust mode.
pub const MANUAL_POSITION_NUDGE_STEPS: u32 = 20;
/// Extra opening clearance used when no measured full position exists.
pub const LOAD_CLEARANCE_MM: f32 = 8.0;
/// Maximum homing travel before faulting.
pub const HOMING_MAX_TRAVEL_MM: f32 = 220.0;
/// Normal post-homing backoff distance from origin.
pub const HOMING_BACKOFF_MM: f32 = 30.0;
/// Post-homing backoff move period.
pub const HOMING_BACKOFF_STEP_PERIOD_US: u64 = 200;
/// Small retract used before syringe removal confirmation.
pub const SYRINGE_REMOVAL_BACKLASH_STEPS: u32 = 200;
/// Syringe removal pressure-relief period.
pub const SYRINGE_REMOVAL_BACKLASH_STEP_PERIOD_US: u64 = 1_000;
/// Reserved delivery ramp starting period.
pub const DISPENSE_START_STEP_PERIOD_US: u64 = 5_000;
/// Reserved delivery ramp length.
pub const DISPENSE_RAMP_STEPS: u32 = 250;

// UI timing and interaction

/// Poll delay while waiting for button release.
pub const BUTTON_RELEASE_POLL_MS: u64 = 10;
/// Normal dashboard refresh cadence.
pub const UI_REFRESH_INTERVAL: Duration = Duration::from_millis(200);
/// Hold time for Back-to-settings and similar actions.
pub const BUTTON_HOLD_ACTION_MS: u64 = 800;
/// Hold time before direct bolus starts.
pub const DIRECT_BOLUS_HOLD_ACTION_MS: u64 = 250;
/// Direct bolus command chunk size.
pub const DIRECT_BOLUS_BURST_STEPS: u32 = 48;
/// Per-hold direct bolus window before requiring release.
pub const DIRECT_BOLUS_WINDOW_UL: f32 = 1_000.0;
/// Direct bolus overlay auto-dismiss delay.
pub const DIRECT_BOLUS_OVERLAY_DISMISS_MS: u64 = 2_000;
/// Flash save checkpoint as percent of dose progress.
pub const FLASH_DELIVERY_PROGRESS_PERCENT_STEP: u32 = 5;
/// Maximum time between delivery progress flash saves.
pub const FLASH_DELIVERY_PROGRESS_MAX_INTERVAL_MS: u64 = 300_000;
/// Encoder quadrature debounce.
pub const ENCODER_DEBOUNCE: Duration = Duration::from_millis(3);
/// Encoder repeat interval.
pub const ENCODER_STEP_INTERVAL: Duration = Duration::from_millis(120);
/// Encoder OK debounce.
pub const ENCODER_BUTTON_DEBOUNCE: Duration = Duration::from_millis(40);
/// Number of setup rows including the start row.
pub const SETUP_ITEMS: usize = 5;

// GPIO pinout

/// Display SPI MOSI pin.
pub const DISPLAY_MOSI_GPIO: u8 = 0;
/// Display SPI SCLK pin.
pub const DISPLAY_SCLK_GPIO: u8 = 7;
/// Display chip-select pin.
pub const DISPLAY_CS_GPIO: u8 = 4;
/// Display data/command pin.
pub const DISPLAY_DC_GPIO: u8 = 6;
/// Display reset pin.
pub const DISPLAY_RESET_GPIO: u8 = 5;

/// TMC2209 enable pin, active low.
pub const TMC_ENABLE_GPIO: u8 = 1;
/// TMC2209 STEP pin driven by RMT.
pub const TMC_STEP_GPIO: u8 = 11;
/// TMC2209 DIR pin.
pub const TMC_DIR_GPIO: u8 = 10;
/// TMC2209 UART TX pin.
pub const TMC_UART_TX_GPIO: u8 = 18;
/// TMC2209 UART RX pin.
pub const TMC_UART_RX_GPIO: u8 = 19;

/// I2C SCL pin.
pub const I2C_SCL_GPIO: u8 = 2;
/// I2C SDA pin.
pub const I2C_SDA_GPIO: u8 = 3;

/// Bolus/dispense button pin.
pub const BUTTON_DISPENSE_GPIO: u8 = 17;
/// Back/retract button pin.
pub const BUTTON_RETRACT_GPIO: u8 = 16;
/// Homing limit switch pin.
pub const HOMING_LIMIT_GPIO: u8 = 15;
/// Rotary encoder A pin.
pub const ENCODER_A_GPIO: u8 = 21;
/// Rotary encoder B pin.
pub const ENCODER_B_GPIO: u8 = 22;
/// Rotary encoder OK button pin.
pub const ENCODER_BUTTON_GPIO: u8 = 23;
