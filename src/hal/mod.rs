pub mod buzzer;   // PIN_7 — active buzzer
pub mod display;
pub mod input;    // PIN_2/3/4 encoder, PIN_5 back, PIN_6 bolus
pub mod motor;    // PIN_8 step, PIN_9 dir, PIN_10 en
pub mod tmc2208;  // UART0: PIN_12 tx, PIN_13 rx  (PDN_UART via 1kΩ)
pub mod nfc;