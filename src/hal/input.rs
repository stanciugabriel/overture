use embassy_rp::gpio::{Input, Pull};
use embassy_rp::Peripheral;

/// EC11 rotary encoder + two tactile buttons.
///
/// Wiring (all to GND, internal pull-ups enabled):
///   ENCODER CLK (A) → PIN_2
///   ENCODER DT  (B) → PIN_3
///   ENCODER SW      → PIN_4  (push-to-select / OK)
///   BACK button     → PIN_5
///   BOLUS button    → PIN_6  (hold to deliver)
pub struct InputPanel<'d> {
    clk:     Input<'d>,
    dt:      Input<'d>,
    sw:      Input<'d>,
    back:    Input<'d>,
    bolus:   Input<'d>,
    prev_ab: (bool, bool), // last (CLK, DT) sample for quadrature decode
}

impl<'d> InputPanel<'d> {
    pub fn new(
        pin_clk:   impl Peripheral<P = impl embassy_rp::gpio::Pin> + 'd,
        pin_dt:    impl Peripheral<P = impl embassy_rp::gpio::Pin> + 'd,
        pin_sw:    impl Peripheral<P = impl embassy_rp::gpio::Pin> + 'd,
        pin_back:  impl Peripheral<P = impl embassy_rp::gpio::Pin> + 'd,
        pin_bolus: impl Peripheral<P = impl embassy_rp::gpio::Pin> + 'd,
    ) -> Self {
        let clk     = Input::new(pin_clk, Pull::Up);
        let dt      = Input::new(pin_dt,  Pull::Up);
        let init_ab = (clk.is_high(), dt.is_high());
        Self {
            clk,
            dt,
            sw:      Input::new(pin_sw,    Pull::Up),
            back:    Input::new(pin_back,  Pull::Up),
            bolus:   Input::new(pin_bolus, Pull::Up),
            prev_ab: init_ab,
        }
    }

    pub fn read(&mut self) -> InputState {
        let a = self.clk.is_high();
        let b = self.dt.is_high();
        let (pa, _pb) = self.prev_ab;

        // Simple quadrature decode: detect CLK falling edge and sample DT.
        //   CLK falling while DT high  → CW  → "up"
        //   CLK falling while DT low   → CCW → "down"
        // At 30 Hz poll rate this reliably catches one detent per tick for
        // any human-speed rotation.
        let enc_up   = pa && !a &&  b;
        let enc_down = pa && !a && !b;

        self.prev_ab = (a, b);

        InputState {
            enc_up,
            enc_down,
            enc_press: self.sw.is_low(),
            back:      self.back.is_low(),
            bolus:     self.bolus.is_low(),
        }
    }
}

#[derive(Default, Clone, Copy)]
pub struct InputState {
    pub enc_up:    bool, // encoder rotated clockwise one detent
    pub enc_down:  bool, // encoder rotated counter-clockwise one detent
    pub enc_press: bool, // encoder shaft pressed
    pub back:      bool, // BACK button
    pub bolus:     bool, // BOLUS button (hold to deliver)
}

/// Detects rising edges (just-pressed) from a stream of raw input states.
pub struct InputEdges {
    prev: InputState,
}

impl InputEdges {
    pub const fn new() -> Self {
        Self {
            prev: InputState {
                enc_up:    false,
                enc_down:  false,
                enc_press: false,
                back:      false,
                bolus:     false,
            },
        }
    }

    /// Returns `(just_pressed, currently_held)`.
    pub fn update(&mut self, cur: InputState) -> (InputState, InputState) {
        let pressed = InputState {
            enc_up:    cur.enc_up    && !self.prev.enc_up,
            enc_down:  cur.enc_down  && !self.prev.enc_down,
            enc_press: cur.enc_press && !self.prev.enc_press,
            back:      cur.back      && !self.prev.back,
            bolus:     cur.bolus     && !self.prev.bolus,
        };
        self.prev = cur;
        (pressed, cur)
    }
}
