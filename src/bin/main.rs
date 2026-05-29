//! ESP32-C6 syringe pump firmware.
//!
//! Wiring:
//! - TMC2209 UART: GPIO18 TX through 4.7k to PDN_UART, GPIO19 RX on same node.
//! - TMC2209 motion: GPIO10 DIR, GPIO11 STEP, GPIO1 ENN active low.
//! - TFT SPI: GPIO0 MOSI, GPIO7 SCLK, GPIO4 CS, GPIO6 DC, GPIO5 RESET.
//! - Inputs: GPIO17 bolus, GPIO16 back, GPIO15 homing limit, GPIO21/22 encoder, GPIO23 OK.

#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types"
)]

use embassy_embedded_hal::shared_bus::asynch::spi::SpiDeviceWithConfig;
use embassy_executor::Spawner;
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, mutex::Mutex};
use embassy_time::{Delay, Duration, Timer, with_timeout};
use esp_hal::clock::CpuClock;
use esp_hal::dma::{DmaRxBuf, DmaTxBuf};
use esp_hal::gpio::{Input, InputConfig, Level, Output, OutputConfig, Pull};
use esp_hal::i2c::master::{Config as I2cConfig, I2c};
use esp_hal::rmt::{Rmt, TxChannelConfig, TxChannelCreator};
use esp_hal::spi::{
    Mode,
    master::{Config as SpiConfig, Spi},
};
use esp_hal::time::Rate;
use esp_hal::timer::timg::TimerGroup;
use esp_hal::uart::{Config as UartConfig, Uart};
use lcd_async::{
    Builder,
    interface::SpiInterface,
    models::ILI9488Rgb666,
    options::{Orientation, Rotation},
    raw_framebuf::RawFrameBuf,
};
use overture::{
    app::{Inputs, run_homing_sequence, ui_task},
    config::*,
    display::{DisplayInterface, DisplaySpiBus},
    motor::{MotorClient, MotorPins, motor_task},
    nfc::Pn532,
    persistent::PersistentStore,
    startup::{
        StartupRecoveryChoice, probe_i2c_device, prompt_boot_recovery,
        read_stusb4500_rdo_object_position, startup_debug_bypass_enabled,
        startup_fault_or_continue_if, update_startup_progress, wait_for_required_usb_pd_power,
    },
    tmc::{Tmc2209Uart, TmcStatus},
};
use static_cell::StaticCell;

// Static storage required by the async display stack.
static DISPLAY_FRAME_BUFFER: StaticCell<[u8; FRAME_BUFFER_SIZE]> = StaticCell::new();
static DISPLAY_FLUSH_BUFFER: StaticCell<[u8; FLUSH_BUFFER_SIZE]> = StaticCell::new();
static DISPLAY_SPI_BUS: StaticCell<Mutex<NoopRawMutex, DisplaySpiBus>> = StaticCell::new();

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    esp_println::println!("PANIC: {}", info);
    loop {}
}

esp_bootloader_esp_idf::esp_app_desc!();

#[allow(
    clippy::large_stack_frames,
    unreachable_code,
    unused_variables,
    unused_mut,
    reason = "display driver and framebuffer references live across awaits in main"
)]
#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    esp_println::println!("APP: entered main");
    esp_println::logger::init_logger_from_env();
    log::info!("ESP32-C6 syringe pump starting");

    // Core hardware and RTOS setup must happen before async peripherals.
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let p = esp_hal::init(config);

    let mut persistent_store = PersistentStore::new(p.FLASH);
    let mut persistent_config = persistent_store.load();
    persistent_config.startup_count = persistent_config.startup_count.saturating_add(1);
    persistent_store.save(persistent_config);

    let timg0 = TimerGroup::new(p.TIMG0);
    let sw_interrupt = esp_hal::interrupt::software::SoftwareInterruptControl::new(p.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_interrupt.software_interrupt0);

    let output_config = OutputConfig::default();
    let cs = Output::new(p.GPIO4, Level::High, output_config);
    let dc = Output::new(p.GPIO6, Level::Low, output_config);
    let rst = Output::new(p.GPIO5, Level::Low, output_config);

    let (rx_buffer, rx_descriptors, tx_buffer, tx_descriptors) = esp_hal::dma_buffers!(32_000);
    let dma_rx_buf = DmaRxBuf::new(rx_descriptors, rx_buffer).expect("DMA RX failed");
    let dma_tx_buf = DmaTxBuf::new(tx_descriptors, tx_buffer).expect("DMA TX failed");

    let spi_cfg = SpiConfig::default()
        .with_frequency(Rate::from_hz(DISPLAY_SPI_HZ))
        .with_mode(Mode::_0);
    let spi = Spi::new(p.SPI2, spi_cfg)
        .expect("SPI init failed")
        .with_sck(p.GPIO7)
        .with_mosi(p.GPIO0)
        .with_dma(p.DMA_CH0)
        .with_buffers(dma_rx_buf, dma_tx_buf)
        .into_async();

    let spi_bus = DISPLAY_SPI_BUS.init(Mutex::new(spi));
    let spi_device = SpiDeviceWithConfig::new(
        spi_bus,
        cs,
        SpiConfig::default().with_frequency(Rate::from_hz(DISPLAY_SPI_HZ)),
    );
    let di: DisplayInterface = SpiInterface::new(spi_device, dc);

    let mut display_delay = Delay;
    let display_init = Builder::new(ILI9488Rgb666, di)
        .reset_pin(rst)
        .display_size(320, 480)
        .orientation(Orientation::new().rotate(Rotation::Deg270).flip_vertical())
        .init(&mut display_delay);

    let mut display = match with_timeout(Duration::from_secs(2), display_init).await {
        Ok(Ok(display)) => display,
        _ => panic!("ILI9488 display init failed or timed out"),
    };

    let frame_buffer = DISPLAY_FRAME_BUFFER.init_with(|| [0; FRAME_BUFFER_SIZE]);
    let flush_buffer = DISPLAY_FLUSH_BUFFER.init_with(|| [0; FLUSH_BUFFER_SIZE]);
    let mut frame = RawFrameBuf::new(
        &mut frame_buffer[..],
        DISPLAY_W as usize,
        DISPLAY_H as usize,
    );

    update_startup_progress(
        &mut display,
        &mut frame,
        flush_buffer,
        "Display SPI ready",
        1,
    )
    .await;

    // RMT owns STEP timing so UI drawing cannot jitter motor pulses.
    let rmt = Rmt::new(p.RMT, Rate::from_mhz(80))
        .expect("RMT init failed")
        .into_async();
    let step_tx_config = TxChannelConfig::default()
        .with_clk_divider(80)
        .with_idle_output_level(Level::Low)
        .with_idle_output(true)
        .with_carrier_modulation(false);
    let step_channel = rmt
        .channel0
        .configure_tx(&step_tx_config)
        .expect("RMT config failed")
        .with_pin(p.GPIO11);

    let motor = MotorPins {
        dir: Output::new(p.GPIO10, Level::Low, output_config),
        step: step_channel,
        enable: Output::new(p.GPIO1, Level::High, output_config),
    };
    let motor_client = MotorClient::new();
    spawner.spawn(motor_task(motor).expect("Motor task spawn failed"));

    let pulldown_config = InputConfig::default();
    let encoder_config = InputConfig::default().with_pull(Pull::Up);
    let inputs = Inputs {
        dispense_button: Input::new(p.GPIO17, pulldown_config),
        retract_button: Input::new(p.GPIO16, pulldown_config),
        homing_limit_switch: Input::new(p.GPIO15, pulldown_config),
        encoder_a: Input::new(p.GPIO21, encoder_config),
        encoder_b: Input::new(p.GPIO22, encoder_config),
        encoder_button: Input::new(p.GPIO23, pulldown_config),
    };

    update_startup_progress(
        &mut display,
        &mut frame,
        flush_buffer,
        "Checking I2C devices",
        2,
    )
    .await;

    let i2c_cfg = I2cConfig::default().with_frequency(Rate::from_khz(100));
    let mut i2c = I2c::new(p.I2C0, i2c_cfg)
        .expect("I2C0 init failed")
        .with_scl(p.GPIO2)
        .with_sda(p.GPIO3)
        .into_async();

    match read_stusb4500_rdo_object_position(&mut i2c).await {
        Ok((rdo, pos)) => esp_println::println!("STUSB4500 OK: rdo=0x{:02X} pos={}", rdo, pos),
        Err(_) => {
            startup_fault_or_continue_if(
                &mut display,
                &mut frame,
                flush_buffer,
                "STUSB4500 missing",
                startup_debug_bypass_enabled(),
            )
            .await
        }
    }

    if probe_i2c_device(&mut i2c, PN532_I2C_ADDR).await {
        esp_println::println!("PN532 OK: addr=0x{:02X}", PN532_I2C_ADDR);
    } else {
        startup_fault_or_continue_if(
            &mut display,
            &mut frame,
            flush_buffer,
            "PN532 missing",
            startup_debug_bypass_enabled(),
        )
        .await;
    }

    update_startup_progress(
        &mut display,
        &mut frame,
        flush_buffer,
        "Checking TMC2209 UART",
        3,
    )
    .await;

    let uart_cfg = UartConfig::default().with_baudrate(TMC_UART_BAUD);
    let uart = Uart::new(p.UART1, uart_cfg)
        .expect("UART1 init failed")
        .with_tx(p.GPIO18)
        .with_rx(p.GPIO19)
        .into_async();
    let mut tmc = Tmc2209Uart::new(uart);

    Timer::after_millis(100).await;
    tmc.init_driver().await;

    if tmc.log_startup_status().await == TmcStatus::NoResponse {
        startup_fault_or_continue_if(
            &mut display,
            &mut frame,
            flush_buffer,
            "TMC2209 not responding",
            startup_debug_bypass_enabled(),
        )
        .await;
    }

    update_startup_progress(
        &mut display,
        &mut frame,
        flush_buffer,
        "Checking USB-C 20V power",
        4,
    )
    .await;
    wait_for_required_usb_pd_power(&mut i2c, &mut display, &mut frame, flush_buffer).await;

    // Mounted syringe recovery avoids unsafe startup homing.
    let resume_saved_operation = if persistent_config.syringe_mounted {
        match prompt_boot_recovery(
            &mut display,
            &mut frame,
            flush_buffer,
            &inputs,
            persistent_config,
        )
        .await
        {
            StartupRecoveryChoice::ResumeSaved => true,
            StartupRecoveryChoice::DiscardAndHome => {
                persistent_config.syringe_mounted = false;
                persistent_store.save(persistent_config);
                false
            }
        }
    } else {
        false
    };

    let syringe_mounted_at_boot = persistent_config.syringe_mounted && resume_saved_operation;

    update_startup_progress(&mut display, &mut frame, flush_buffer, "Homing carriage", 5).await;

    let carriage_position_steps = if syringe_mounted_at_boot {
        log::warn!("syringe-mounted boot override active; homing skipped");
        update_startup_progress(&mut display, &mut frame, flush_buffer, "Homing skipped", 5).await;
        Timer::after_millis(250).await;
        persistent_config
            .carriage_position_steps
            .clamp(0, CARRIAGE_HARD_LIMIT_STEPS_FROM_HOME)
    } else if startup_debug_bypass_enabled() {
        log::warn!("startup debug bypass active; homing skipped");
        update_startup_progress(&mut display, &mut frame, flush_buffer, "Homing bypassed", 5).await;
        Timer::after_millis(250).await;
        overture::dosing::steps_for_travel_mm(HOMING_BACKOFF_MM).unwrap_or(0) as i32
    } else {
        run_homing_sequence(
            &mut display,
            &mut frame,
            flush_buffer,
            motor_client,
            &inputs,
            &mut tmc,
        )
        .await
    };

    persistent_config.carriage_position_steps = carriage_position_steps;
    persistent_config.syringe_mounted = syringe_mounted_at_boot;
    persistent_store.save(persistent_config);

    update_startup_progress(
        &mut display,
        &mut frame,
        flush_buffer,
        "Startup complete",
        6,
    )
    .await;
    Timer::after_millis(250).await;

    let nfc = Pn532::new(i2c);

    esp_println::println!("APP: entering app loop");
    ui_task(
        &mut display,
        &mut frame,
        flush_buffer,
        motor_client,
        inputs,
        nfc,
        tmc,
        persistent_store,
        persistent_config,
        carriage_position_steps,
        syringe_mounted_at_boot,
        resume_saved_operation,
    )
    .await
}
