use embassy_rp::peripherals::{PIN_7, PWM_SLICE3};
use embassy_rp::pwm::{Config, Pwm};

/// Passive buzzer driver (PIN_7).
pub struct Buzzer<'d> {
    pwm: Pwm<'d>,
    config: Config,
    duty: u16,
}

impl<'d> Buzzer<'d> {
    pub fn new(pwm_p: PWM_SLICE3, pin: PIN_7) -> Self {
        let mut config = Config::default();
        config.divider = 250.into();
        config.top = 250; // 2 kHz
        config.compare_b = 0;
        let duty = config.top / 2;
        let pwm = Pwm::new_output_b(pwm_p, pin, config.clone());

        Self {
            pwm,
            config,
            duty, // 50%
        }
    }

    pub fn on(&mut self) {
        self.config.compare_b = self.duty;
        self.pwm.set_config(&self.config);
    }

    pub fn off(&mut self) {
        self.config.compare_b = 0;
        self.pwm.set_config(&self.config);
    }

    pub fn set_alarm(&mut self, blink_tick: u32) {
        let t = blink_tick % 90;
        let beeping = t < 10 || (15..25).contains(&t) || (30..40).contains(&t);
        if beeping {
            self.on();
        } else {
            self.off();
        }
    }
}
