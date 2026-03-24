/// A syringe size profile.
///
/// `volume_x10`  — total capacity in tenths of mL (e.g. 200 = 20.0 mL).
/// `ul_per_mm`   — µL delivered per mm of linear carriage travel.
///                 Calculated from inner diameter: π × (d/2)².
///                 Used later by the motion module to convert steps → volume.
///
/// Inner diameters (BD Plastipak, approximate):
///   10 mL → 14.5 mm →  165 µL/mm
///   20 mL → 19.1 mm →  286 µL/mm
///   50 mL → 28.6 mm →  643 µL/mm
#[derive(Clone, Copy)]
pub struct Syringe {
    pub label:      &'static str,
    pub volume_x10: i32,
    pub ul_per_mm:  u32,
}

pub const SYRINGES: &[Syringe] = &[
    Syringe { label: "10 mL · BD Plastipak", volume_x10: 100, ul_per_mm: 165 },
    Syringe { label: "20 mL · BD Plastipak", volume_x10: 200, ul_per_mm: 286 },
    Syringe { label: "50 mL · BD Plastipak", volume_x10: 500, ul_per_mm: 643 },
];
