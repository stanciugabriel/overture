/// A drug entry in the library.
///
/// Rates are in tenths of mL/hr (e.g. 20 = 2.0 mL/hr).
/// `max_rate_x10` is a hard safety ceiling enforced during rate adjustment.
#[derive(Clone, Copy)]
pub struct Drug {
    pub name:            &'static str,
    pub concentration:   &'static str,
    pub default_rate_x10: i32,
    pub max_rate_x10:     i32,
}

pub const DRUGS: &[Drug] = &[
    Drug { name: "Morphine",       concentration: "10 mg/mL",     default_rate_x10: 20,  max_rate_x10: 100 },
    Drug { name: "Fentanyl",       concentration: "50 mcg/mL",    default_rate_x10: 30,  max_rate_x10: 150 },
    Drug { name: "Midazolam",      concentration: "5 mg/mL",      default_rate_x10: 20,  max_rate_x10: 100 },
    Drug { name: "Propofol",       concentration: "10 mg/mL",     default_rate_x10: 50,  max_rate_x10: 300 },
    Drug { name: "Noradrenaline",  concentration: "0.016 mg/mL",  default_rate_x10: 50,  max_rate_x10: 200 },
    Drug { name: "Adrenaline",     concentration: "0.1 mg/mL",    default_rate_x10: 20,  max_rate_x10: 100 },
    Drug { name: "Heparin",        concentration: "1000 IU/mL",   default_rate_x10: 20,  max_rate_x10: 50  },
    Drug { name: "Insulin",        concentration: "1 IU/mL",      default_rate_x10: 10,  max_rate_x10: 50  },
    Drug { name: "Ketamine",       concentration: "10 mg/mL",     default_rate_x10: 30,  max_rate_x10: 100 },
    Drug { name: "Labetalol",      concentration: "5 mg/mL",      default_rate_x10: 20,  max_rate_x10: 100 },
];
