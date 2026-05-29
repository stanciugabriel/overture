use embassy_time::Timer;

use crate::{
    config::{MICROSTEPS, USE_SPREADCYCLE_FOR_DELIVERY},
    motor::MotorClient,
    tmc::Tmc2209Uart,
};

const PITCH_VALS: [u32; 128] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 32258, 30303, 28571,
    27027, 25641, 24390, 22727, 21739, 20408, 19230, 18182, 17241, 16129, 15385, 14493, 13699,
    12821, 12195, 11494, 10753, 10204, 9615, 9091, 8547, 8130, 7634, 7194, 6803, 6410, 6061, 5714,
    5405, 5102, 4808, 4545, 4292, 4049, 3817, 3610, 3401, 3215, 3030, 2865, 2703, 2551, 2410, 2273,
    2146, 2024, 1912, 1805, 1704, 1608, 1517, 1433, 1351, 1276, 1203, 1136, 1073, 1012, 955, 902,
    851, 803, 758, 716, 676, 638, 602, 568, 536, 506, 478, 451, 426, 402, 379, 358, 338, 315, 301,
    284, 268, 253, 239, 225, 213, 201, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

const NOTE_REST: usize = 0;
const NOTE_C3: usize = 48;
const NOTE_CS3: usize = 49;
const NOTE_E3: usize = 52;
const NOTE_F3: usize = 53;
const NOTE_G3: usize = 55;
const NOTE_GS3: usize = 56;

pub(super) const TUTTI_FRUTTI_MELODY: &[(usize, u64)] = &[
    (NOTE_REST, 911),
    (NOTE_GS3, 298),
    (NOTE_G3, 128),
    (NOTE_REST, 53),
    (NOTE_F3, 184),
    (NOTE_REST, 127),
    (NOTE_F3, 182),
    (NOTE_REST, 162),
    (NOTE_F3, 136),
    (NOTE_G3, 148),
    (NOTE_F3, 139),
    (NOTE_E3, 131),
    (NOTE_REST, 50),
    (NOTE_E3, 533),
    (NOTE_CS3, 556),
    (NOTE_REST, 46),
    (NOTE_F3, 126),
    (NOTE_REST, 19),
    (NOTE_E3, 151),
    (NOTE_REST, 44),
    (NOTE_CS3, 194),
    (NOTE_REST, 123),
    (NOTE_CS3, 157),
    (NOTE_REST, 162),
    (NOTE_CS3, 161),
    (NOTE_E3, 146),
    (NOTE_CS3, 105),
    (NOTE_REST, 32),
    (NOTE_C3, 95),
    (NOTE_REST, 85),
    (NOTE_C3, 651),
    (NOTE_REST, 310),
    (NOTE_GS3, 127),
    (NOTE_G3, 109),
    (NOTE_REST, 83),
    (NOTE_F3, 251),
    (NOTE_REST, 104),
    (NOTE_GS3, 602),
    (NOTE_G3, 167),
    (NOTE_GS3, 171),
    (NOTE_G3, 294),
    (NOTE_REST, 56),
    (NOTE_F3, 686),
    (NOTE_REST, 89),
    (NOTE_F3, 144),
    (NOTE_E3, 162),
    (NOTE_REST, 10),
    (NOTE_CS3, 190),
    (NOTE_REST, 150),
    (NOTE_CS3, 157),
    (NOTE_REST, 167),
    (NOTE_CS3, 135),
    (NOTE_REST, 1),
    (NOTE_E3, 156),
    (NOTE_CS3, 124),
    (NOTE_REST, 17),
    (NOTE_C3, 107),
    (NOTE_REST, 76),
    (NOTE_C3, 885),
    (NOTE_REST, 236),
    (NOTE_C3, 90),
    (NOTE_REST, 50),
    (NOTE_C3, 95),
    (NOTE_REST, 54),
    (NOTE_C3, 152),
    (NOTE_REST, 5),
    (NOTE_CS3, 278),
    (NOTE_REST, 107),
    (NOTE_CS3, 243),
    (NOTE_REST, 92),
    (NOTE_E3, 91),
    (NOTE_REST, 107),
    (NOTE_E3, 360),
    (NOTE_REST, 170),
    (NOTE_CS3, 347),
    (NOTE_REST, 22),
    (NOTE_C3, 646),
    (NOTE_REST, 32),
    (NOTE_E3, 125),
    (NOTE_REST, 79),
    (NOTE_F3, 115),
    (NOTE_REST, 79),
    (NOTE_F3, 224),
    (NOTE_REST, 81),
    (NOTE_F3, 217),
    (NOTE_REST, 127),
    (NOTE_F3, 107),
    (NOTE_REST, 68),
    (NOTE_F3, 357),
    (NOTE_REST, 144),
    (NOTE_E3, 292),
    (NOTE_REST, 44),
    (NOTE_CS3, 556),
    (NOTE_REST, 139),
    (NOTE_CS3, 108),
    (NOTE_REST, 70),
    (NOTE_CS3, 122),
    (NOTE_REST, 61),
    (NOTE_E3, 191),
    (NOTE_REST, 147),
    (NOTE_E3, 235),
    (NOTE_REST, 124),
    (NOTE_CS3, 119),
    (NOTE_REST, 57),
    (NOTE_CS3, 494),
    (NOTE_REST, 81),
    (NOTE_C3, 1012),
];

/// Plays a melody by temporarily switching the stepper to full-step tone generation.
pub(super) async fn sing_stepper_song(
    tmc: &mut Tmc2209Uart,
    motor: MotorClient,
    song: &[(usize, u64)],
) -> u32 {
    let mut emitted_full_steps = 0u32;

    tmc.set_spreadcycle_enabled(true).await;
    tmc.configure_step_mode(1, false).await;
    Timer::after_millis(2).await;

    for &(pitch, duration_ms) in song {
        let Some(&delay_us) = PITCH_VALS.get(pitch) else {
            Timer::after_millis(duration_ms).await;
            continue;
        };

        if delay_us == 0 {
            Timer::after_millis(duration_ms).await;
            continue;
        }

        let status = motor.tone_auto(delay_us as u64, duration_ms).await;
        emitted_full_steps = emitted_full_steps.saturating_add(status.command_steps);
    }

    motor.disable();

    tmc.configure_step_mode(MICROSTEPS, true).await;
    tmc.set_spreadcycle_enabled(USE_SPREADCYCLE_FOR_DELIVERY)
        .await;
    Timer::after_millis(2).await;

    emitted_full_steps.saturating_mul(MICROSTEPS)
}
