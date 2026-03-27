#![no_std]
#![no_main]

use defmt::info;
use defmt_rtt as _;
use panic_probe as _;

use embassy_executor::Spawner;
use embassy_rp::gpio::{Input, Level, Output, Pull};
use embassy_time::Timer;

// Encoder: PIN_2 = CLK, PIN_3 = DT  (same as main firmware)
// Motor:   PIN_8 = STEP, PIN_9 = DIR
// MS1/MS2 wired to GND → 1/8 µstep → 1600 steps/rev
//
// Each encoder click changes period by ±10% (logarithmic feel).
//   CW  = faster   CCW = slower
//   Min = 150 µs/step  (~208 RPM)
//   Max = 80 000 µs/step  (~0.47 RPM, barely crawling)

const STEP_HIGH_US: u64 =    10;
const PERIOD_MIN:   u64 =   150;
const PERIOD_MAX:   u64 = 80_000;
const PERIOD_START: u64 =  5_000; // ~12 RPM — comfortable mid-point

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_rp::init(Default::default());

    let _dir     = Output::new(p.PIN_9, Level::High);
    let mut step = Output::new(p.PIN_8, Level::Low);

    let clk_pin  = Input::new(p.PIN_2, Pull::Up);
    let dt_pin   = Input::new(p.PIN_3, Pull::Up);

    let mut prev_clk  = clk_pin.is_high();
    let mut period_us = PERIOD_START;

    info!("encoder speed control — CW=faster  CCW=slower");
    info!("period: {} us", period_us);

    loop {
        // Step pulse
        step.set_high();
        Timer::after_micros(STEP_HIGH_US).await;
        step.set_low();
        Timer::after_micros(period_us - STEP_HIGH_US).await;

        // Read encoder after each step
        let clk = clk_pin.is_high();
        let dt  = dt_pin.is_high();

        if prev_clk && !clk {
            // CLK falling edge — direction from DT
            period_us = if dt {
                // CW → speed up (lower period)
                (period_us * 9 / 10).max(PERIOD_MIN)
            } else {
                // CCW → slow down (higher period)
                (period_us * 11 / 10).min(PERIOD_MAX)
            };
            info!("period: {} us", period_us);
        }

        prev_clk = clk;
    }
}
