# Overture | Automatic Perfusor
A precision, medical-grade syringe pump built with asynchronous Rust and custom hardware.

This project is a university project and not an actual medical device.

## Overview
Overture is an open-source hardware and software project designed to push a syringe plunger at a strictly controlled flow rate with **~0.01mL accuracy**. Built to rival industry standards like the B.Braun Space Plus, Overture combines extreme mechanical precision with an intuitive, modern user interface to reduce cognitive load in high-stress emergency medical environments.

## Key Features
* **Continuous Precision Infusion:** Calculates and delivers exact VTBI (Volume To Be Infused) over time for life-support medications.
* **Direct Bolus:** Hardware-locked rapid manual override limited to a strict 1 mL maximum window per press for patient safety.
* **KVO (Keep Vein Open):** Automatically throttles to a minimal continuous flow rate (1-3 mL/h) post-infusion to prevent IV line clotting.
* **NFC Syringe Detection:** Instantly populates the UI with syringe parameters and drug profiles by scanning an NFC sticker.
* **Color-Coded UI:** Mirrors international medical syringe labels for immediate visual confirmation of drug classes.

## Tech Stack
* **Firmware:** `#![no_std]` Rust utilizing the `embassy` async framework and `esp-rtos`.
* **MCU:** ESP32-C6 (utilizing the RMT peripheral for zero-jitter hardware motor stepping).
* **Motor Control:** NEMA-17 driven by a TMC2209 over single-wire UART.
* **Graphics:** `embedded-graphics` mapped to an ILI9488 SPI display.
* **Hardware:** Custom USB-PD (STUSB4500) capable of negotiating 20V.

## Repositories
This project is split into two main repositories:

* 💻 **Software & Mechanics (This Repo):** Contains the Rust firmware, UI graphics, and mechanical CAD references.
* ⚡ **Hardware & PCB:** [Overture PCB](https://github.com/stanciugabriel/overture-pcb) - Contains the complete KiCad schematics, custom footprints, and Gerber files for the ESP32-C6 / USB-PD motherboard.

## Getting Started
To flash the device run `cargo run --release`

---
**Author**: Gabriel Stanciu
