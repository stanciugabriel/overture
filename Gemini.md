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
*   **Build System:**
    *   `Cargo.toml`: Configured with specific `embassy` dependencies from a git repository, `slint`, `mipidsi`, and other necessary embedded crates.
    *   `build.rs`: Handles copying the `memory.x` linker script and compiling `.slint` UI definitions.
    *   `memory.x`: Defines the RP2350's memory layout.
    *   `.cargo/config.toml`: Configured for `probe-rs` as the runner and `thumbv8m.main-none-eabihf` as the target.
*   **Display Wiring (Matching Reference Project):**
    *   SPI0:
        *   SCK: PIN_18
        *   MOSI: PIN_19
        *   MISO: PIN_16 (Used for SPI bus, though not explicitly by display)
        *   CS: PIN_22
        *   DC: PIN_21
        *   RST: PIN_20

## Hardware Stack
*   **Microcontroller:** Raspberry Pi Pico 2W (RP2350 based)
*   **Motion:** NEMA 17 Stepper Motor
*   **Motor Driver:** TMC2208 v2 (initial) -> TMC2209 (upgrade for StallGuard sensorless homing and kink detection)
*   **Drive Mechanism:** T8 Lead Screw (2mm pitch)
*   **Display:** SPI-based ST7789 Screen
*   **User Input:** Rotary Encoder (with push button) and tactile buttons
*   **Sensors:** NFC Module (for detecting drug info via stickers on syringes)
*   **Audio:** Buzzer for alarms

## Software Ecosystem
*   **Language:** Rust (Embedded)
*   **Frameworks:** `embassy-rp` (async execution), `Slint` (UI), `embedded-hal`

## Core Workflow
1.  **Syringe Insertion:** User places the syringe into the carriage.
2.  **Syringe Type:** System prompts for syringe volume/type.
3.  **Drug Identification:** System attempts to auto-detect the drug via NFC sticker. If absent, the user selects the drug manually from the drug library.
4.  **Patient Data:** User inputs patient weight.
5.  **Dose Configuration:**
    *   System auto-suggests a dose based on the selected drug.
    *   System enforces a maximum allowed dose for safety.
    *   User can modify the suggested dosage within safe limits.
6.  **Infusion Start:** The device begins pushing the syringe at the calculated rate.

## Key Features & Requirements

### Fluid Delivery & Motion Control
*   **Target Minimum Rate:** ~0.1 mL/hr (hardware permitting).
*   **Motion Profile:** Trapezoidal acceleration for smooth start/stop, eliminating jerk and preventing missed steps.
*   **Calculations:** Accurate mapping of stepper rotations to linear carriage movement on the 2mm pitch T8 lead screw to volume output based on syringe geometry.

### Clinical & Safety Features
*   **Drug Library:** Hardcoded library of ~10 common medications for testing, including standard concentrations and dose limits.
*   **Bolus Function:** Dedicated button. While held, delivers drug at a configurable maximum "bolus rate". Upon release, resumes the previously set continuous rate.
*   **Occlusion/Kink Detection:** Utilize TMC2209 StallGuard to detect increased backpressure (line kink or blockage) and trigger an alarm.
*   **Keep Veins Open (KVO):** Upon completing the primary infusion volume, automatically drop the flow rate to a minimum safe level to prevent the IV line from clotting.
*   **Alarm System:** Audible buzzer alerts combined with prominent visual warnings on the display for critical events (occlusion, near empty, empty, system error).

## "Awesome Extras" (Stretch Goals)
*   **Animated UI:** Real-time visual representation of the syringe emptying. Smooth fills and UI transitions utilizing partial screen updates for performance.
*   **Advanced Dose Calculation Mode:** Clinical standard input: Enter desired dose in `µg/kg/min` + `patient weight`. Firmware automatically calculates the mechanical `mL/hr` rate (similar to Braun Space pumps).
*   **Multi-Syringe Profiles:** Save a complete infusion configuration (drug + rate + volume + patient weight) to persistent flash memory as a named profile. Recallable with quick encoder actions.
*   **Rate Ramping:** Configurable gradual increase in flow rate over a set time period, crucial for safely administering specific medications like vasopressors (e.g., noradrenaline) where sudden onset is dangerous.