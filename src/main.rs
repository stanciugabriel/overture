#![no_std]
#![no_main]

extern crate alloc;
use alloc::boxed::Box;
use alloc::rc::Rc;

use defmt::{info, Display2Format};
use defmt_rtt as _;
use panic_probe as _;

use embassy_executor::Spawner;
use embassy_rp::gpio::{Level, Output};
use embassy_rp::spi::{Config as SpiConfig, Spi};
use embassy_time::{Delay, Duration, Timer};

use embedded_graphics::pixelcolor::{raw::RawU16, Rgb565};
use embedded_hal_bus::spi::ExclusiveDevice;
use mipidsi::{models::ST7789, options::*, Builder};

use slint::{
    platform::{
        software_renderer::{MinimalSoftwareWindow, RepaintBufferType, Rgb565Pixel},
        Platform, WindowAdapter,
    },
    ComponentHandle,
};

use embedded_alloc::Heap;
#[global_allocator]
static HEAP: Heap = Heap::empty();

mod app_core;
mod hal;
mod ui;

use app_core::drug_library::DRUGS;
use app_core::syringe::SYRINGES;
use hal::buzzer::Buzzer;
use hal::input::{InputEdges, InputPanel};
use hal::motor::Stepper;
use hal::tmc2208::Tmc2208;

const DISPLAY_W: u32 = 170;
const DISPLAY_H: u32 = 320;

// Full-frame pixel buffer — 170 × 320 × 2 bytes = 108 800 bytes.
// Lives in static memory (outside the 64 KB heap) so it doesn't eat into
// Slint's allocation budget.
static mut FRAME_BUF: [Rgb565Pixel; (DISPLAY_W * DISPLAY_H) as usize] =
    [Rgb565Pixel(0); (DISPLAY_W * DISPLAY_H) as usize];

const TICK_MS: u64 = 33;
const BOLUS_VOL_X10: i32 = 5; // 0.5 mL
const BOLUS_DURATION_MS: i32 = 9_000;

// ── State machine ─────────────────────────────────────────────────────────────
#[derive(Clone, Copy, PartialEq)]
enum PumpState {
    MotorTest,
    SetupSyringe,
    SetupDrug,
    SetupConfirm,
    Running,
    Paused,
    RateAdjust,
    Bolus,
    Alarm,
}

// ── Slint platform ────────────────────────────────────────────────────────────
struct PicoPlatform {
    window: Rc<MinimalSoftwareWindow>,
}

impl Platform for PicoPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        Ok(self.window.clone())
    }
    fn duration_since_start(&self) -> ::core::time::Duration {
        use embassy_time::Instant;
        ::core::time::Duration::from_micros(Instant::now().as_micros())
    }
    fn debug_log(&self, s: ::core::fmt::Arguments) {
        info!("{}", Display2Format(&s));
    }
}

// ── List-picker helper ────────────────────────────────────────────────────────
/// Manages a scrolling window of 5 visible rows over a list of `len` items.
struct ListNav {
    cursor: usize, // absolute index in the full list
    scroll: usize, // index of the top-visible row
    len: usize,
}

impl ListNav {
    fn new(len: usize) -> Self {
        Self {
            cursor: 0,
            scroll: 0,
            len,
        }
    }

    fn up(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            if self.cursor < self.scroll {
                self.scroll = self.cursor;
            }
        }
    }

    fn down(&mut self) {
        if self.cursor + 1 < self.len {
            self.cursor += 1;
            if self.cursor >= self.scroll + 5 {
                self.scroll = self.cursor.saturating_sub(4);
            }
        }
    }

    /// Highlighted row within the visible window (0–4).
    fn highlighted(&self) -> i32 {
        (self.cursor - self.scroll) as i32
    }

    /// Absolute index of the selected item.
    fn selected(&self) -> usize {
        self.cursor
    }
}

// ── Entry point ───────────────────────────────────────────────────────────────
#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    {
        const HEAP_SIZE: usize = 64 * 1024;
        static mut HEAP_MEM: [u8; HEAP_SIZE] = [0u8; HEAP_SIZE];
        unsafe { HEAP.init(HEAP_MEM.as_ptr() as usize, HEAP_SIZE) }
    }

    let p = embassy_rp::init(Default::default());
    info!("syringe pump starting");

    // ── Display ───────────────────────────────────────────────────────────────
    let mut spi_cfg = SpiConfig::default();
    // ST7789 write cycle minimum is ~15 ns → max safe SPI ≈ 62.5 MHz.
    // At 125 MHz sys-clk the divider is 2 → exactly 62.5 MHz.
    // Pushing to 75 MHz (150 MHz ÷ 2) violates the spec on longer bursts.
    spi_cfg.frequency = 62_500_000;

    let spi = Spi::new_blocking(p.SPI0, p.PIN_18, p.PIN_19, p.PIN_16, spi_cfg);
    let cs = Output::new(p.PIN_22, Level::High);
    let dc = Output::new(p.PIN_21, Level::Low);
    let rst = Output::new(p.PIN_20, Level::High);

    let spi = ExclusiveDevice::new_no_delay(spi, cs);
    let di = crate::hal::display::SPIDeviceInterface::new(spi, dc);

    let mut display = Builder::new(ST7789, di)
        .display_size(DISPLAY_W as u16, DISPLAY_H as u16)
        .display_offset(35, 0)
        .invert_colors(ColorInversion::Inverted)
        .reset_pin(rst)
        .init(&mut Delay)
        .unwrap();

    info!("display ready");

    // ── Slint ─────────────────────────────────────────────────────────────────
    let window = MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer);
    window.set_size(slint::PhysicalSize::new(DISPLAY_W, DISPLAY_H));
    slint::platform::set_platform(Box::new(PicoPlatform {
        window: window.clone(),
    }))
    .unwrap();
    let ui = ui::SyringePump::new().unwrap();
    ui.show().unwrap();

    // ── Peripherals ───────────────────────────────────────────────────────────
    let mut input = InputPanel::new(p.PIN_2, p.PIN_3, p.PIN_4, p.PIN_5, p.PIN_6);
    let mut buzzer = Buzzer::new(p.PWM_SLICE3, p.PIN_7);
    let mut edges = InputEdges::new();
    let mut stepper = Stepper::new(p.PIN_8, p.PIN_9, p.PIN_10);

    // ── TMC2208 UART config ────────────────────────────────────────────────────
    // PIN_12 TX → 1 kΩ → PDN_UART, PIN_13 RX → PDN_UART (same node).
    let mut tmc = Tmc2208::new(p.UART0, p.PIN_12, p.PIN_13);
    tmc.set_current(16, 8, 6);   // irun=16 (~50%), ihold=8 (~25%), delay=6
    tmc.set_microsteps(4);        // 1/16 µstep: 200 steps = 1/16 rev (visible)
    info!("tmc2208 configured");

    // ── Pump state ────────────────────────────────────────────────────────────
    let mut pump_state = PumpState::MotorTest;
    let mut motor_steps: i32 = 0;

    let mut syringe_nav = ListNav::new(SYRINGES.len());
    let mut drug_nav = ListNav::new(DRUGS.len());

    // Selected indices (confirmed by OK)
    let mut syringe_idx: usize = 0;
    let mut drug_idx: usize = 0;

    // Rate in tenths of mL/hr; clamped to the drug's safety ceiling.
    let mut rate_x10: i32 = DRUGS[0].default_rate_x10;
    let mut pending_x10: i32 = rate_x10;
    let mut was_running = false;

    // Volume accumulator (units: tenths-mL × 3_600_000)
    let mut vol_acc: i64 = 0;
    let mut bolus_ms: i32 = 0;
    let mut blink_tick: u32 = 0;

    loop {
        let raw = input.read();
        let (pressed, held) = edges.update(raw);

        blink_tick = blink_tick.wrapping_add(1);
        let blink = (blink_tick / 15) % 2 == 0;

        // ── Advance simulation (motor not yet present) ────────────────────────
        if pump_state == PumpState::Running {
            vol_acc += rate_x10 as i64 * TICK_MS as i64;
        }
        if pump_state == PumpState::Bolus && held.bolus {
            bolus_ms = (bolus_ms + TICK_MS as i32).min(BOLUS_DURATION_MS);
        }

        // ── Buzzer ────────────────────────────────────────────────────────────
        if pump_state == PumpState::Alarm {
            buzzer.set_alarm(blink_tick);
        } else if pump_state == PumpState::Bolus {
            buzzer.on();
        } else {
            buzzer.off();
        }

        // ── State machine ─────────────────────────────────────────────────────
        match pump_state {
            PumpState::MotorTest => {
                if pressed.enc_press {
                    stepper.enable();
                    stepper.forward();
                    stepper.step_n(51_200);
                    stepper.disable();
                    motor_steps += 200;
                }
                if pressed.back {
                    pump_state = PumpState::SetupSyringe;
                }
            }

            PumpState::SetupSyringe => {
                if pressed.enc_up {
                    syringe_nav.up();
                }
                if pressed.enc_down {
                    syringe_nav.down();
                }
                if pressed.enc_press {
                    syringe_idx = syringe_nav.selected();
                    pump_state = PumpState::SetupDrug;
                }
            }

            PumpState::SetupDrug => {
                if pressed.enc_up {
                    drug_nav.up();
                }
                if pressed.enc_down {
                    drug_nav.down();
                }
                if pressed.enc_press {
                    drug_idx = drug_nav.selected();
                    // Clamp suggested rate to safety ceiling
                    rate_x10 = DRUGS[drug_idx]
                        .default_rate_x10
                        .min(DRUGS[drug_idx].max_rate_x10);
                    pending_x10 = rate_x10;
                    pump_state = PumpState::SetupConfirm;
                }
                if pressed.back {
                    pump_state = PumpState::SetupSyringe;
                }
            }

            PumpState::SetupConfirm => {
                if pressed.enc_up {
                    pending_x10 = (pending_x10 + 1).min(DRUGS[drug_idx].max_rate_x10);
                }
                if pressed.enc_down {
                    pending_x10 = (pending_x10 - 1).max(1);
                }
                if pressed.enc_press {
                    rate_x10 = pending_x10;
                    vol_acc = 0; // reset volume for new infusion
                    pump_state = PumpState::Paused;
                }
                if pressed.back {
                    pump_state = PumpState::SetupDrug;
                }
            }

            PumpState::Running => {
                if pressed.enc_press {
                    pump_state = PumpState::Paused;
                } else if pressed.enc_up || pressed.enc_down {
                    pending_x10 = rate_x10;
                    was_running = true;
                    pump_state = PumpState::RateAdjust;
                } else if held.bolus {
                    bolus_ms = 0;
                    pump_state = PumpState::Bolus;
                }
            }

            PumpState::Paused => {
                if pressed.enc_press {
                    pump_state = PumpState::Running;
                } else if pressed.enc_up || pressed.enc_down {
                    pending_x10 = rate_x10;
                    was_running = false;
                    pump_state = PumpState::RateAdjust;
                } else if pressed.back {
                    // Re-enter setup to change drug / syringe
                    pump_state = PumpState::SetupSyringe;
                }
            }

            PumpState::RateAdjust => {
                let max = DRUGS[drug_idx].max_rate_x10;
                if pressed.enc_up {
                    pending_x10 = (pending_x10 + 1).min(max);
                }
                if pressed.enc_down {
                    pending_x10 = (pending_x10 - 1).max(1);
                }
                if pressed.enc_press {
                    rate_x10 = pending_x10;
                    pump_state = if was_running {
                        PumpState::Running
                    } else {
                        PumpState::Paused
                    };
                }
                if pressed.back {
                    pump_state = if was_running {
                        PumpState::Running
                    } else {
                        PumpState::Paused
                    };
                }
            }

            PumpState::Bolus => {
                if !held.bolus || bolus_ms >= BOLUS_DURATION_MS {
                    pump_state = PumpState::Running;
                    bolus_ms = 0;
                }
            }

            PumpState::Alarm => {
                if pressed.enc_press || pressed.back {
                    pump_state = PumpState::Paused;
                }
            }
        }

        // ── Compute display values ────────────────────────────────────────────
        let syringe_vol_x10 = SYRINGES[syringe_idx].volume_x10;
        let vol_x10 = (vol_acc / 3_600_000) as i32;
        let remaining_x10 = (syringe_vol_x10 - vol_x10).max(0);
        let syringe_pct = remaining_x10 * 100 / syringe_vol_x10;
        let time_rem_ms: i64 = if rate_x10 > 0 {
            remaining_x10 as i64 * 3_600_000 / rate_x10 as i64
        } else {
            0
        };
        let time_rem_h = (time_rem_ms / 3_600_000) as i32;
        let time_rem_m = ((time_rem_ms % 3_600_000) / 60_000) as i32;
        let bolus_progress = bolus_ms * 100 / BOLUS_DURATION_MS;

        // ── Build PumpData ────────────────────────────────────────────────────
        let drug = &DRUGS[drug_idx];
        let syringe = &SYRINGES[syringe_idx];

        let data = match pump_state {
            PumpState::MotorTest => ui::PumpData {
                state: 7,
                motor_steps,
                ..base_data()
            },

            PumpState::SetupSyringe => {
                let scroll = syringe_nav.scroll;
                list_data(
                    "SELECT SYRINGE",
                    syringe_nav.highlighted(),
                    [
                        SYRINGES.get(scroll).map(|s| s.label).unwrap_or(""),
                        SYRINGES.get(scroll + 1).map(|s| s.label).unwrap_or(""),
                        SYRINGES.get(scroll + 2).map(|s| s.label).unwrap_or(""),
                        SYRINGES.get(scroll + 3).map(|s| s.label).unwrap_or(""),
                        SYRINGES.get(scroll + 4).map(|s| s.label).unwrap_or(""),
                    ],
                )
            }

            PumpState::SetupDrug => {
                let scroll = drug_nav.scroll;
                list_data(
                    "SELECT DRUG",
                    drug_nav.highlighted(),
                    [
                        DRUGS.get(scroll).map(|d| d.name).unwrap_or(""),
                        DRUGS.get(scroll + 1).map(|d| d.name).unwrap_or(""),
                        DRUGS.get(scroll + 2).map(|d| d.name).unwrap_or(""),
                        DRUGS.get(scroll + 3).map(|d| d.name).unwrap_or(""),
                        DRUGS.get(scroll + 4).map(|d| d.name).unwrap_or(""),
                    ],
                )
            }

            PumpState::SetupConfirm => ui::PumpData {
                state: 6,
                confirm_drug: slint::SharedString::from(drug.name),
                confirm_syringe: slint::SharedString::from(syringe.label),
                confirm_rate: pending_x10 / 10,
                confirm_rate_dec: pending_x10 % 10,
                ..base_data()
            },

            PumpState::Running => ui::PumpData {
                state: 0,
                drug_name: slint::SharedString::from(drug.name),
                concentration: slint::SharedString::from(drug.concentration),
                syringe_size: slint::SharedString::from(syringe.label),
                rate: rate_x10 / 10,
                rate_dec: rate_x10 % 10,
                vol_del_int: vol_x10 / 10,
                vol_del_dec: vol_x10 % 10,
                time_rem_h,
                time_rem_m,
                syringe_pct,
                blink,
                ..base_data()
            },

            PumpState::Paused => ui::PumpData {
                state: 1,
                drug_name: slint::SharedString::from(drug.name),
                concentration: slint::SharedString::from(drug.concentration),
                syringe_size: slint::SharedString::from(syringe.label),
                rate: rate_x10 / 10,
                rate_dec: rate_x10 % 10,
                vol_del_int: vol_x10 / 10,
                vol_del_dec: vol_x10 % 10,
                time_rem_h,
                time_rem_m,
                syringe_pct,
                blink,
                ..base_data()
            },

            PumpState::RateAdjust => ui::PumpData {
                state: 3,
                old_rate: rate_x10 / 10,
                old_rate_dec: rate_x10 % 10,
                new_rate: pending_x10 / 10,
                new_rate_dec: pending_x10 % 10,
                blink,
                ..base_data()
            },

            PumpState::Bolus => ui::PumpData {
                state: 4,
                bolus_vol_int: BOLUS_VOL_X10 / 10,
                bolus_vol_dec: BOLUS_VOL_X10 % 10,
                bolus_progress,
                bolus_active: true,
                blink,
                ..base_data()
            },

            PumpState::Alarm => ui::PumpData {
                state: 2,
                alarm_title: slint::SharedString::from("OCCLUSION"),
                alarm_body: slint::SharedString::from(
                    "Line blocked or kinked.\nMotor stopped safely.",
                ),
                alarm_time: slint::SharedString::from("--:--:--"),
                blink,
                ..base_data()
            },
        };

        ui.set_data(data);

        slint::platform::update_timers_and_animations();
        window.draw_if_needed(|renderer| {
            // Render the full frame into the static buffer, then push it to the
            // display in a single set_pixels call.  This replaces 320 separate
            // CASET+RASET+RAMWR+data transactions with one address setup + one
            // continuous pixel stream, nearly doubling effective throughput.
            let buf = unsafe { &mut FRAME_BUF };
            renderer.render(buf, DISPLAY_W as usize);
            display
                .set_pixels(
                    0,
                    0,
                    DISPLAY_W as u16 - 1,
                    DISPLAY_H as u16 - 1,
                    buf.iter().map(|p| Rgb565::from(RawU16::new(p.0))),
                )
                .ok();
        });

        Timer::after(Duration::from_millis(TICK_MS)).await;
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn list_data(title: &str, cursor: i32, rows: [&str; 5]) -> ui::PumpData {
    ui::PumpData {
        state: 5,
        sel_title: slint::SharedString::from(title),
        sel_0: slint::SharedString::from(rows[0]),
        sel_1: slint::SharedString::from(rows[1]),
        sel_2: slint::SharedString::from(rows[2]),
        sel_3: slint::SharedString::from(rows[3]),
        sel_4: slint::SharedString::from(rows[4]),
        sel_cursor: cursor,
        ..base_data()
    }
}

fn base_data() -> ui::PumpData {
    ui::PumpData {
        state: 0,
        drug_name: slint::SharedString::from(""),
        concentration: slint::SharedString::from(""),
        syringe_size: slint::SharedString::from(""),
        rate: 0,
        rate_dec: 0,
        vol_del_int: 0,
        vol_del_dec: 0,
        time_rem_h: 0,
        time_rem_m: 0,
        syringe_pct: 100,
        nfc_ok: false,
        battery_pct: 100,
        new_rate: 0,
        new_rate_dec: 0,
        old_rate: 0,
        old_rate_dec: 0,
        bolus_vol_int: 0,
        bolus_vol_dec: 5,
        bolus_progress: 0,
        bolus_active: false,
        alarm_title: slint::SharedString::from(""),
        alarm_body: slint::SharedString::from(""),
        alarm_time: slint::SharedString::from(""),
        blink: false,
        sel_title: slint::SharedString::from(""),
        sel_0: slint::SharedString::from(""),
        sel_1: slint::SharedString::from(""),
        sel_2: slint::SharedString::from(""),
        sel_3: slint::SharedString::from(""),
        sel_4: slint::SharedString::from(""),
        sel_cursor: 0,
        confirm_drug: slint::SharedString::from(""),
        confirm_syringe: slint::SharedString::from(""),
        confirm_rate: 0,
        confirm_rate_dec: 0,
        motor_steps: 0,
    }
}
