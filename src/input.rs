use embassy_time::{Instant, Timer};
use esp_hal::gpio::Input;

use crate::config::{BUTTON_RELEASE_POLL_MS, ENCODER_BUTTON_DEBOUNCE};

const ENCODER_EDGES_PER_MENU_STEP: i8 = 2;

pub struct EncoderState {
    raw_ab: u8,
    edge_accumulator: i8,
    raw_button_pressed: bool,
    stable_button_pressed: bool,
    last_button_change: Instant,
}

impl EncoderState {
    pub fn new(a: &Input<'_>, b: &Input<'_>, button: &Input<'_>) -> Self {
        let now = Instant::now();
        let ab = encoder_ab(a, b);
        let pressed = button_pressed(button);
        Self {
            raw_ab: ab,
            edge_accumulator: 0,
            raw_button_pressed: pressed,
            stable_button_pressed: pressed,
            last_button_change: now,
        }
    }
    //code ported from an arduino library
    pub fn poll(&mut self, a: &Input<'_>, b: &Input<'_>, button: &Input<'_>) -> (i32, bool) {
        let ab = encoder_ab(a, b);
        let now = Instant::now();
        let mut delta = 0;

        if ab != self.raw_ab {
            let transition = (self.raw_ab << 2) | ab;
            self.raw_ab = ab;

            let edge = match transition {
                0b1101 | 0b0100 | 0b0010 | 0b1011 => 1,
                0b1110 | 0b0111 | 0b0001 | 0b1000 => -1,
                _ => 0,
            };

            if edge != 0 {
                self.edge_accumulator += edge;
                if self.edge_accumulator >= ENCODER_EDGES_PER_MENU_STEP {
                    delta = 1;
                    self.edge_accumulator = 0;
                } else if self.edge_accumulator <= -ENCODER_EDGES_PER_MENU_STEP {
                    delta = -1;
                    self.edge_accumulator = 0;
                }
            }
        }

        let mut press_edge = false;
        let pressed = button_pressed(button);
        if pressed != self.raw_button_pressed {
            self.raw_button_pressed = pressed;
            self.last_button_change = now;

            if pressed && !self.stable_button_pressed {
                // Latch the press immediately so short OK/pause clicks are not lost
                // during display refreshes or motor-status handling.
                self.stable_button_pressed = true;
                press_edge = true;
            }
        }

        if !pressed
            && self.stable_button_pressed
            && self.last_button_change.elapsed() >= ENCODER_BUTTON_DEBOUNCE
        {
            self.stable_button_pressed = false;
        }

        (delta, press_edge)
    }
}

fn encoder_ab(a: &Input<'_>, b: &Input<'_>) -> u8 {
    ((a.is_low() as u8) << 1) | (b.is_low() as u8)
}

pub fn button_pressed(button: &Input<'_>) -> bool {
    button.is_high()
}

pub async fn wait_for_both_released(first: &Input<'_>, second: &Input<'_>) {
    while button_pressed(first) || button_pressed(second) {
        Timer::after_millis(BUTTON_RELEASE_POLL_MS).await;
    }
}
