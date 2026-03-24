use embassy_rp::gpio::{Level, Output};
use embassy_rp::Peripheral;

/// Minimal STEP/DIR driver for a TMC2208 (or any step-dir stepper driver).
///
/// Wiring:
///   STEP → PIN_8
///   DIR  → PIN_9
///   EN   → PIN_10  (active LOW — pull high to disable, low to enable)
///
/// TMC2208 also needs:
///   VIO  → 3.3 V
///   GND  → GND
///   VM   → motor supply (12–24 V)
///   MS1, MS2 left floating → 1/256 micro-stepping (stealthChop default)
///   PDN_UART left floating or pulled up
pub struct Stepper<'d> {
    step: Output<'d>,
    dir:  Output<'d>,
    en:   Output<'d>,
}

impl<'d> Stepper<'d> {
    pub fn new(
        pin_step: impl Peripheral<P = impl embassy_rp::gpio::Pin> + 'd,
        pin_dir:  impl Peripheral<P = impl embassy_rp::gpio::Pin> + 'd,
        pin_en:   impl Peripheral<P = impl embassy_rp::gpio::Pin> + 'd,
    ) -> Self {
        Self {
            step: Output::new(pin_step, Level::Low),
            dir:  Output::new(pin_dir,  Level::Low),
            en:   Output::new(pin_en,   Level::High), // disabled until explicitly enabled
        }
    }

    pub fn enable(&mut self)  { self.en.set_low();  }
    pub fn disable(&mut self) { self.en.set_high(); }

    pub fn forward(&mut self) { self.dir.set_high(); }
    pub fn reverse(&mut self) { self.dir.set_low();  }

    /// Send `n` steps at ~500 steps/sec (2 ms/step).
    /// Blocks for n × 2 ms — acceptable for a short test burst.
    /// At default 1/256 micro-stepping, 200 micro-steps ≈ 1/256 of a revolution.
    /// Increase n accordingly for visible motion (e.g. 51 200 = 1 full rev).
    pub fn step_n(&mut self, n: u32) {
        for _ in 0..n {
            self.step.set_high();
            // ~100 µs high pulse (TMC2208 minimum is 100 ns — plenty of margin)
            cortex_m::asm::delay(15_000); // 15 000 cycles @ 150 MHz ≈ 100 µs
            self.step.set_low();
            // ~1.9 ms low time → 2 ms total per step → 500 steps/sec
            cortex_m::asm::delay(285_000);
        }
    }
}
