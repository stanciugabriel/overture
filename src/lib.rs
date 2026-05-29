#![cfg_attr(not(test), no_std)]

#[cfg(target_os = "none")]
pub mod app;
pub mod config;
#[cfg(target_os = "none")]
pub mod display;
pub mod dosing;
#[cfg(target_os = "none")]
pub mod input;
#[cfg(target_os = "none")]
pub mod motor;
#[cfg(target_os = "none")]
pub mod nfc;
pub mod persistent;
#[cfg(target_os = "none")]
pub mod startup;
#[cfg(target_os = "none")]
pub mod tmc;
pub mod types;
