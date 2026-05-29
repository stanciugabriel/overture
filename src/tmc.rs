use embassy_time::{Duration, Timer, with_timeout};
use esp_hal::uart::Uart;
use tmc2209::data::MicroStepResolution;
use tmc2209::reg::{
    CHOPCONF, DRV_STATUS, GCONF, GSTAT, IFCNT, IHOLD_IRUN, SLAVECONF, TPOWERDOWN, TPWMTHRS,
};
use tmc2209::{ReadableRegister, Reader, WritableRegister};

use crate::config::{
    DEBUG_TMC_WRITE_ALL_UART_ADDRS, MICROSTEPS, TMC2209_ADDR, USE_SPREADCYCLE_FOR_DELIVERY,
};
use crate::dosing::delivery_chopper_mode_name;
pub use crate::types::TmcStatus;

// UART-backed TMC2209 driver.

pub struct Tmc2209Uart {
    uart: Uart<'static, esp_hal::Async>,
}

impl Tmc2209Uart {
    pub fn new(uart: Uart<'static, esp_hal::Async>) -> Self {
        Self { uart }
    }

    // Writes all pending bytes before returning.
    async fn write_all(&mut self, mut bytes: &[u8]) {
        while !bytes.is_empty() {
            match self.uart.write_async(bytes).await {
                Ok(0) => {}
                Ok(written) => bytes = &bytes[written..],
                Err(error) => {
                    log::warn!("UART write error: {:?}", error);
                    break;
                }
            }
        }
        let _ = self.uart.flush_async().await;
    }

    // Writes a register to the configured driver address.
    async fn write_reg<R>(&mut self, register: R)
    where
        R: WritableRegister,
    {
        self.write_reg_to_addr(TMC2209_ADDR, register).await;
    }

    // Writes a register to a specific address for address probing.
    async fn write_reg_to_addr<R>(&mut self, address: u8, register: R)
    where
        R: WritableRegister,
    {
        let request = tmc2209::write_request(address, register);
        log::debug!(
            "TMC2209 write addr={} reg={:#04x} bytes={:02x?}",
            address,
            R::ADDRESS as u8,
            request.bytes()
        );
        self.write_all(request.bytes()).await;
    }

    // Sends a read request and waits for a valid decoded response.
    async fn read_reg<R>(&mut self) -> Option<R>
    where
        R: ReadableRegister,
    {
        let request = tmc2209::read_request::<R>(TMC2209_ADDR);
        self.write_all(request.bytes()).await;

        let mut reader = Reader::default();
        let mut byte = [0u8; 1];

        for _ in 0..96 {
            match with_timeout(Duration::from_millis(20), self.uart.read_async(&mut byte)).await {
                Ok(Ok(1)) => {
                    let (_, response) = reader.read_response(&byte);
                    if let Some(response) = response {
                        if !response.crc_is_valid() {
                            log::warn!("bad TMC2209 CRC on {:#04x}", R::ADDRESS as u8);
                            return None;
                        }

                        return response.register::<R>().ok();
                    }
                }
                Ok(Ok(_)) => {}
                Ok(Err(error)) => {
                    log::warn!("UART read error: {:?}", error);
                    return None;
                }
                Err(_) => return None,
            }
        }

        None
    }

    /// Applies the normal motion-current and microstep profile.
    pub async fn init_driver(&mut self) {
        log::info!("TMC2209 UART init start: addr={} baud=57600", TMC2209_ADDR);

        let mut slaveconf = SLAVECONF::default();
        slaveconf.set_send_delay(8);
        self.write_reg(slaveconf).await;
        Timer::after_millis(2).await;

        self.set_spreadcycle_enabled(USE_SPREADCYCLE_FOR_DELIVERY)
            .await;
        Timer::after_millis(2).await;

        self.configure_step_mode(MICROSTEPS, true).await;
        Timer::after_millis(2).await;

        let mut ihold_irun = IHOLD_IRUN::default();
        ihold_irun.set_ihold(5);
        ihold_irun.set_irun(15);
        ihold_irun.set_ihold_delay(6);
        self.write_reg(ihold_irun).await;
        Timer::after_millis(2).await;

        self.write_reg(TPOWERDOWN::default()).await;
        Timer::after_millis(2).await;

        let mut tpwmthrs = TPWMTHRS::default();
        tpwmthrs.set(0);
        self.write_reg(tpwmthrs).await;
        Timer::after_millis(2).await;

        log::info!(
            "TMC2209 configured: mode={}, {} microsteps, intpol=true, IRUN=15, IHOLD=5, IHOLDDELAY=6",
            delivery_chopper_mode_name(),
            MICROSTEPS,
        );
    }

    /// Configures CHOPCONF microsteps and interpolation.
    pub async fn configure_step_mode(&mut self, microsteps: u32, intpol: bool) {
        self.write_reg(chopconf_for_step_mode(microsteps, intpol))
            .await;
        log::info!(
            "TMC2209 step mode configured: {} microsteps, intpol={}",
            microsteps,
            intpol
        );
    }

    /// Selects SpreadCycle when enabled, otherwise StealthChop.
    pub async fn set_spreadcycle_enabled(&mut self, enabled: bool) {
        if DEBUG_TMC_WRITE_ALL_UART_ADDRS == 1 {
            for address in 0..=3 {
                self.write_reg_to_addr(address, gconf_for_spreadcycle(enabled))
                    .await;
                Timer::after_millis(2).await;
            }
        } else {
            self.write_reg(gconf_for_spreadcycle(enabled)).await;
        }

        log::info!(
            "TMC2209 write-only chopper mode command: {}",
            if enabled {
                "SpreadCycle"
            } else {
                "StealthChop"
            }
        );
    }

    /// Logs startup registers and reports whether any register responded.
    pub async fn log_startup_status(&mut self) -> TmcStatus {
        let mut verified = false;

        match self.read_reg::<IFCNT>().await {
            Some(ifcnt) => {
                log::info!("TMC2209 IFCNT={}", ifcnt.get());
                verified = true;
            }
            None => log::warn!("TMC2209 IFCNT read failed"),
        }

        match self.read_reg::<GSTAT>().await {
            Some(gstat) => {
                log::info!(
                    "TMC2209 GSTAT reset={} drv_err={} uv_cp={}",
                    gstat.reset(),
                    gstat.drv_err(),
                    gstat.uv_cp()
                );
                verified = true;
            }
            None => log::warn!("TMC2209 GSTAT read failed"),
        }

        match self.read_reg::<DRV_STATUS>().await {
            Some(drv) => {
                log::info!(
                    "TMC2209 DRV_STATUS stealth={} cs_actual={} standstill={}",
                    drv.stealth(),
                    drv.cs_actual(),
                    drv.stst()
                );
                verified = true;
            }
            None => log::warn!("TMC2209 DRV_STATUS read failed"),
        }

        if verified {
            TmcStatus::Verified
        } else {
            TmcStatus::NoResponse
        }
    }
}

/// Builds GCONF so the driver uses UART-selected microsteps and the requested chopper mode.
fn gconf_for_spreadcycle(enabled: bool) -> GCONF {
    let mut gconf = GCONF::default();
    gconf.set_en_spread_cycle(enabled);
    gconf.set_pdn_disable(true);
    gconf.set_mstep_reg_select(true);
    gconf.set_multistep_filt(true);
    gconf
}

/// Builds CHOPCONF for the configured microstep resolution and interpolation behavior.
fn chopconf_for_step_mode(microsteps: u32, intpol: bool) -> CHOPCONF {
    let mut chopconf = CHOPCONF::default();
    chopconf.set_toff(3);
    chopconf.set_hstrt(3);
    chopconf.set_hend(3);
    chopconf.set_tbl(2);
    chopconf.set_vsense(false);
    chopconf.set_mres(MicroStepResolution::from_microsteps(microsteps));
    chopconf.set_intpol(intpol);
    chopconf
}
