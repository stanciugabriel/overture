use embassy_rp::peripherals::UART0;
use embassy_rp::uart::{Blocking, Config as UartConfig, Uart};
use embassy_rp::Peripheral;

// ── Register map ──────────────────────────────────────────────────────────────
pub const REG_GCONF:      u8 = 0x00;
pub const REG_GSTAT:      u8 = 0x01;
pub const REG_IFCNT:      u8 = 0x02; // write-counter, good for comms test
pub const REG_IHOLD_IRUN: u8 = 0x10;
pub const REG_TPOWERDOWN: u8 = 0x11;
pub const REG_TPWMTHRS:   u8 = 0x13;
pub const REG_CHOPCONF:   u8 = 0x6C;
pub const REG_PWMCONF:    u8 = 0x70;
pub const REG_DRV_STATUS: u8 = 0x6F;

const SYNC:       u8 = 0x05;
const SLAVE_ADDR: u8 = 0x00; // MS1=MS2=low → device address 0

/// TMC2208 driver over single-wire UART (half-duplex).
///
/// Wiring:
///   PIN_12 (UART0 TX) ──[1 kΩ]──┬── PDN_UART (TMC2208)
///   PIN_13 (UART0 RX) ────────────┘
///
/// TMC2208 also needs:
///   VIO    → 3.3 V
///   GND    → GND
///   VM     → motor supply (12–24 V)
///   EN     → controlled separately (PIN_10)
///   STEP   → PIN_8
///   DIR    → PIN_9
///   MS1/MS2 → set via UART (no resistors needed)
pub struct Tmc2208<'d> {
    uart: Uart<'d, UART0, Blocking>,
}

impl<'d> Tmc2208<'d> {
    pub fn new(
        uart:   impl Peripheral<P = UART0> + 'd,
        pin_tx: impl Peripheral<P = impl embassy_rp::uart::TxPin<UART0>> + 'd,
        pin_rx: impl Peripheral<P = impl embassy_rp::uart::RxPin<UART0>> + 'd,
    ) -> Self {
        let mut cfg = UartConfig::default();
        cfg.baudrate = 115_200;
        Self {
            uart: Uart::new_blocking(uart, pin_tx, pin_rx, cfg),
        }
    }

    // ── Low-level protocol ────────────────────────────────────────────────────

    /// Write a 32-bit value to a register.
    pub fn write_reg(&mut self, reg: u8, val: u32) {
        let mut buf = [0u8; 8];
        buf[0] = SYNC;
        buf[1] = SLAVE_ADDR;
        buf[2] = reg | 0x80;                   // set write-flag MSB
        buf[3] = ((val >> 24) & 0xFF) as u8;
        buf[4] = ((val >> 16) & 0xFF) as u8;
        buf[5] = ((val >>  8) & 0xFF) as u8;
        buf[6] = ( val        & 0xFF) as u8;
        buf[7] = crc8(&buf[..7]);
        let _ = self.uart.blocking_write(&buf);
        // Because TX and RX share the same wire, the RP2350 RX FIFO will fill
        // with the 8 echo bytes we just sent.  Drain them so future reads are clean.
        let mut echo = [0u8; 8];
        let _ = self.uart.blocking_read(&mut echo);
    }

    /// Read a 32-bit value from a register.
    /// Returns `None` if the TMC2208 did not respond or the CRC is wrong.
    ///
    /// NOTE: `blocking_read` will hang indefinitely if PDN_UART is not wired
    /// correctly and the driver does not reply.  Only call this when you are
    /// confident the hardware is connected.
    pub fn read_reg(&mut self, reg: u8) -> Option<u32> {
        // Send 4-byte read request
        let mut req = [0u8; 4];
        req[0] = SYNC;
        req[1] = SLAVE_ADDR;
        req[2] = reg & 0x7F;   // no write-flag
        req[3] = crc8(&req[..3]);
        let _ = self.uart.blocking_write(&req);

        // Discard the 4-byte echo of our own request
        let mut echo = [0u8; 4];
        let _ = self.uart.blocking_read(&mut echo);

        // Read the 8-byte reply from the TMC2208
        let mut resp = [0u8; 8];
        self.uart.blocking_read(&mut resp).ok()?;

        // Validate CRC
        if crc8(&resp[..7]) != resp[7] {
            return None;
        }

        let val = ((resp[3] as u32) << 24)
                | ((resp[4] as u32) << 16)
                | ((resp[5] as u32) <<  8)
                |  (resp[6] as u32);
        Some(val)
    }

    // ── High-level helpers ────────────────────────────────────────────────────

    /// Set run/hold current and hold delay.
    ///
    /// `irun`      — run current  (0 = min, 31 = max RMS rated current)
    /// `ihold`     — hold current (0 = min, 31 = max); 25–50% of irun typical
    /// `iholddelay`— time to ramp down to hold (0–15, ~130 ms steps)
    pub fn set_current(&mut self, irun: u8, ihold: u8, iholddelay: u8) {
        let val = ((iholddelay as u32 & 0x0F) << 16)
                | ((irun        as u32 & 0x1F) <<  8)
                |  (ihold       as u32 & 0x1F);
        self.write_reg(REG_IHOLD_IRUN, val);
    }

    /// Set micro-stepping resolution.
    ///
    /// | mres | steps/rev (200-step motor) |
    /// |------|---------------------------|
    /// |  0   | 51 200  (256 µstep)       |
    /// |  1   | 25 600  (128 µstep)       |
    /// |  2   | 12 800  ( 64 µstep)       |
    /// |  3   |  6 400  ( 32 µstep)       |
    /// |  4   |  3 200  ( 16 µstep)       |
    /// |  5   |  1 600  (  8 µstep)       |
    /// |  6   |    800  (  4 µstep)       |
    /// |  7   |    400  (  2 µstep)       |
    /// |  8   |    200  ( full step)      |
    pub fn set_microsteps(&mut self, mres: u8) {
        // Read-modify-write CHOPCONF to preserve all other bits.
        // Fall back to TMC2208 power-on default (0x10000053) if read fails.
        let mut chopconf = self.read_reg(REG_CHOPCONF).unwrap_or(0x10000053);
        chopconf = (chopconf & !(0xF << 24)) | ((mres as u32 & 0xF) << 24);
        self.write_reg(REG_CHOPCONF, chopconf);
    }

    /// Read driver status.  Useful for sanity-checking UART comms.
    /// Returns the raw DRV_STATUS register or None if comms failed.
    pub fn drv_status(&mut self) -> Option<u32> {
        self.read_reg(REG_DRV_STATUS)
    }
}

// ── CRC-8 (polynomial 0x07, TMC2208 UART protocol) ───────────────────────────
fn crc8(data: &[u8]) -> u8 {
    let mut crc: u8 = 0;
    for &byte in data {
        let mut b = byte;
        for _ in 0..8 {
            if (crc ^ b) & 0x80 != 0 {
                crc = (crc << 1) ^ 0x07;
            } else {
                crc <<= 1;
            }
            b <<= 1;
        }
    }
    crc
}
