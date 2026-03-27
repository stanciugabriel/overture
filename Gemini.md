# Automated Perfusor (Syringe Pump) Project

## Overview
An embedded Rust project targeting the Raspberry Pi Pico 2W to build a highly capable, automated syringe pump (perfusor). The system prioritizes smooth, accurate medication delivery with advanced safety features and an intuitive user interface.

## Project Setup & Architecture
The project is structured with a layered architecture, utilizing `embassy` for asynchronous operations and `Slint` for the user interface.

### Key Architectural Decisions:
*   **Asynchronous Framework:** `embassy-rp` is used for managing asynchronous tasks and hardware interactions on the RP2350 microcontroller.
*   **User Interface:** `Slint` is the chosen UI framework, providing a declarative way to design the interface and efficient rendering on embedded hardware.
*   **Display Driver:** The `mipidsi` crate is used to drive the ST7789 SPI-based display, integrated with Slint's software renderer via a custom platform implementation.
*   **Module Organization:**
    *   `src/app_core`: Contains the core application logic, preventing shadowing of the `::core` crate.
    *   `src/hal`: Houses hardware abstraction layer modules for various peripherals.
    *   `src/ui`: Contains Slint UI definitions (`.slint` files) and Rust-side UI logic.
    *   `src/bin/`: Additional standalone binaries (e.g. hardware test programs).
*   **Build System:**
    *   `Cargo.toml`: Configured with specific `embassy` dependencies from a git repository, `slint`, `mipidsi`, `cortex-m`, and other necessary embedded crates.
    *   `build.rs`: Handles copying the `memory.x` linker script and compiling `.slint` UI definitions.
    *   `memory.x`: Defines the RP2350's memory layout.
    *   `.cargo/config.toml`: Configured for `probe-rs` as the runner and `thumbv8m.main-none-eabihf` as the target.

## Complete Pin Map

### Display — SPI0
| Pin    | Signal |
|--------|--------|
| PIN_16 | MISO   |
| PIN_18 | SCK    |
| PIN_19 | MOSI   |
| PIN_20 | RST    |
| PIN_21 | DC     |
| PIN_22 | CS     |

### User Input
| Pin    | Signal                        |
|--------|-------------------------------|
| PIN_2  | EC11 CLK (A)                  |
| PIN_3  | EC11 DT (B)                   |
| PIN_4  | EC11 SW (push = OK)           |
| PIN_5  | BACK button                   |
| PIN_6  | BOLUS button (hold to deliver)|

All input pins use internal pull-ups; buttons/encoder wired to GND.

### Audio
| Pin   | Signal              |
|-------|---------------------|
| PIN_7 | Buzzer (PWM_SLICE3) |

### Stepper Motor — TMC2208 v2
| Pin    | Signal   | Notes                        |
|--------|----------|------------------------------|
| PIN_8  | STEP     |                              |
| PIN_9  | DIR      |                              |
| —      | EN       | Wired directly to GND        |
| —      | MS1      | Wired directly to GND → 1/8 µstep |
| —      | MS2      | Wired directly to GND → 1/8 µstep |

At 1/8 µstep: **1600 steps = 1 full revolution**.
T8 lead screw pitch: 2 mm/rev → **800 steps/mm**.

### TMC2208 UART (configuration)
| Pin    | Signal       | Notes                              |
|--------|--------------|------------------------------------|
| PIN_12 | UART0 TX     | → 1 kΩ → PDN_UART on TMC2208      |
| PIN_13 | UART0 RX     | → PDN_UART directly (same node)   |

Single-wire half-duplex. Used to set current (IHOLD_IRUN) and microstepping (CHOPCONF) at boot. MS1/MS2 wired to GND so microstepping works even if UART is skipped.

### TMC2208 Power
| TMC2208 pin | Connects to       |
|-------------|-------------------|
| VIO         | 3.3 V             |
| GND         | GND               |
| VM          | Motor supply (12–24 V, separate rail) |

## Hardware Stack
*   **Microcontroller:** Raspberry Pi Pico 2W (RP2350 based)
*   **Motion:** NEMA 17 Stepper Motor
*   **Motor Driver:** TMC2208 v2 (initial) → TMC2209 (upgrade for StallGuard sensorless homing and kink detection)
*   **Drive Mechanism:** T8 Lead Screw (2mm pitch)
*   **Display:** SPI-based ST7789 (170×320, portrait)
*   **User Input:** EC11 rotary encoder (CLK/DT/SW) + 2 tactile buttons (BACK, BOLUS)
*   **Sensors:** NFC Module (for detecting drug info via stickers on syringes)
*   **Audio:** Buzzer for alarms

## HAL Modules (`src/hal/`)

| File          | What it does                                              |
|---------------|-----------------------------------------------------------|
| `display.rs`  | Custom `SPIDeviceInterface` wrapping mipidsi              |
| `input.rs`    | `InputPanel` — EC11 encoder + 2 buttons; `InputEdges` for edge detection |
| `motor.rs`    | `Stepper` — STEP/DIR blocking `step_n()` via cortex_m delay |
| `tmc2208.rs`  | `Tmc2208` — UART write/read, `set_current()`, `set_microsteps()`, CRC-8 |
| `buzzer.rs`   | PWM buzzer                                                |
| `nfc.rs`      | Stub                                                      |

### Input HAL detail
`InputPanel::read()` returns `InputState { enc_up, enc_down, enc_press, back, bolus }`.
`InputEdges::update()` converts raw state to `(just_pressed, currently_held)`.
Encoder decoded via CLK falling-edge + DT level sample at 30 Hz poll rate.

### TMC2208 UART detail
Single-wire half-duplex at 115 200 baud. CRC-8 (poly 0x07).
Write: 8-byte datagram (sync, addr, reg|0x80, data×4, CRC).
Read: 4-byte request → discard 4-byte echo → 8-byte response.
`set_current(irun, ihold, iholddelay)` writes IHOLD_IRUN (0x10).
`set_microsteps(mres)` read-modify-writes CHOPCONF (0x6C).

## Firmware — Main Binary (`src/main.rs`)

### UI States (Slint `state` field)
| State | Screen          |
|-------|-----------------|
| 7     | Motor Test      |
| 5     | List picker (syringe / drug select) |
| 6     | Setup confirm + rate adjust |
| 0     | Running         |
| 1     | Paused          |
| 3     | Rate adjust     |
| 4     | Bolus           |
| 2     | Alarm           |

Device boots into **state 7 (Motor Test)**. Press BACK to enter normal setup flow.

### Pump State Machine
`SetupSyringe → SetupDrug → SetupConfirm → Paused ↔ Running`
Side states: `RateAdjust`, `Bolus`, `Alarm`, `MotorTest`.

### Controls (main firmware)
| Action           | Input                         |
|------------------|-------------------------------|
| Navigate lists   | Encoder rotate                |
| Confirm / OK     | Encoder press                 |
| Cancel / Back    | BACK button                   |
| Bolus delivery   | Hold BOLUS button             |
| Pause / Resume   | Encoder press (while running) |
| Rate adjust      | Encoder rotate (while running/paused) |

### Volume / Rate Math
*   Rate stored as tenths of mL/hr (`rate_x10`).
*   Volume accumulator: `vol_acc += rate_x10 * TICK_MS` each tick; divide by 3 600 000 for mL×10.
*   Bolus: 0.5 mL in 9 s = 200 mL/hr equivalent.
*   Display tick: 33 ms (≈30 Hz).

## Firmware — Motor Test Binary (`src/bin/motor_test.rs`)

Standalone binary for hardware validation. Flash with `cargo run --bin motor_test`.
No display, no UI. Uses PIN_8 (STEP) and PIN_9 (DIR) only.
EC11 encoder (PIN_2 CLK, PIN_3 DT) controls speed in real time:
*   **CW** → speed up (~10% per click)
*   **CCW** → slow down (~10% per click)
*   Range: ~0.47 RPM (80 000 µs/step) to ~208 RPM (150 µs/step)
*   Start: ~12 RPM (5 000 µs/step)

## Syringe Geometry (BD Plastipak)
| Size  | Inner ⌀  | µL/mm |
|-------|----------|-------|
| 10 mL | 14.5 mm  | 165   |
| 20 mL | 19.1 mm  | 286   |
| 50 mL | 28.6 mm  | 643   |

Steps/mm (1/8 µstep, 2 mm pitch T8): **800 steps/mm**.

### Bolus rate example (20 mL syringe)
200 mL/hr → 55 556 µL/s ÷ 286 µL/mm = 0.194 mm/s × 800 = **155 steps/s** (6 452 µs/step).

## Core Workflow
1.  **Syringe Insertion:** User places the syringe into the carriage.
2.  **Syringe Type:** System prompts for syringe volume/type (encoder to scroll, press to select).
3.  **Drug Identification:** System attempts to auto-detect the drug via NFC sticker. If absent, user selects from drug library.
4.  **Dose Configuration:** System auto-suggests a dose; user adjusts with encoder within safe limits.
5.  **Infusion Start:** Device begins pushing the syringe at the calculated rate.

## Key Features & Requirements

### Fluid Delivery & Motion Control
*   **Target Minimum Rate:** ~0.1 mL/hr (hardware permitting).
*   **Motion Profile:** Trapezoidal acceleration for smooth start/stop, eliminating jerk and preventing missed steps.
*   **Calculations:** Accurate mapping of stepper rotations → linear carriage movement → volume output based on syringe geometry.

### Clinical & Safety Features
*   **Drug Library:** Hardcoded library of ~10 common medications for testing, including standard concentrations and dose limits.
*   **Bolus Function:** Dedicated BOLUS button. While held, delivers at maximum bolus rate. Upon release, resumes continuous rate.
*   **Occlusion/Kink Detection:** TMC2209 StallGuard (future upgrade) detects increased backpressure and triggers alarm.
*   **Keep Veins Open (KVO):** Upon completing infusion volume, drops flow rate to minimum safe level.
*   **Alarm System:** Audible buzzer + visual warnings for occlusion, near empty, empty, system error.

## "Awesome Extras" (Stretch Goals)
*   **Animated UI:** Real-time syringe fill animation with partial screen updates for performance.
*   **Advanced Dose Calculation:** Enter dose in `µg/kg/min` + patient weight; firmware calculates `mL/hr` automatically.
*   **Multi-Syringe Profiles:** Save full infusion config to flash; recall with encoder.
*   **Rate Ramping:** Gradual flow rate increase over time (important for vasopressors like noradrenaline).
