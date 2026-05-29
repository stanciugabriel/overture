use core::fmt::Write;

use embassy_embedded_hal::shared_bus::asynch::spi::SpiDeviceWithConfig;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embedded_graphics::{
    Drawable,
    mono_font::{
        MonoFont, MonoTextStyle,
        ascii::{FONT_6X10, FONT_10X20},
    },
    pixelcolor::{Rgb565, RgbColor},
    prelude::*,
    primitives::{
        CornerRadii, CornerRadiiBuilder, Line, PrimitiveStyle, PrimitiveStyleBuilder, Rectangle,
        RoundedRectangle, Triangle,
    },
    text::{Baseline, Text},
};
use esp_hal::{Async, gpio::Output, spi::master::SpiDmaBus};
use heapless::String;
use lcd_async::{interface::SpiInterface, models::ILI9488Rgb666, raw_framebuf::RawFrameBuf};
use u8g2_fonts::{
    FontRenderer, fonts,
    types::{FontColor, HorizontalAlignment, VerticalPosition},
};

use crate::{
    app::BenchMode,
    config::*,
    dosing::{
        DRUG_LIBRARY, DeliveryAlert, DeliveryState, DoseAccumulator, DrugSpec, Prescription,
        SyringeSpec, approx_delivery_seconds, delivery_target_ul, dispense_step_period_us,
        rate_ml_h, syringe_load_travel_mm, ul_to_ml_parts,
    },
};

pub type DisplaySpiBus = SpiDmaBus<'static, Async>;
pub type DisplaySpiDevice =
    SpiDeviceWithConfig<'static, NoopRawMutex, DisplaySpiBus, Output<'static>>;
pub type DisplayInterface = SpiInterface<DisplaySpiDevice, Output<'static>>;
pub type Display = lcd_async::Display<DisplayInterface, ILI9488Rgb666, Output<'static>>;
pub type FrameBuffer = RawFrameBuf<Rgb565, &'static mut [u8]>;

mod components;
mod dashboard;
mod overlays;
mod setup;
mod startup;

pub use dashboard::{draw_dashboard_frame, draw_dashboard_values, draw_syringe_status_block};
pub use overlays::{
    draw_bolus_administered_alert_overlay, draw_bolus_delivered_alert_overlay,
    draw_direct_bolus_overlay, draw_recover_perfusion_alert_overlay,
};
pub use setup::{
    draw_bolus_setup_screen, draw_confirm_syringe_removed_screen, draw_drug_select_screen,
    draw_load_adjust_screen, draw_load_opening_screen, draw_nfc_syringe_detected_screen,
    draw_patient_weight_screen, draw_prime_screen, draw_remove_syringe_prompt_screen,
    draw_settings_screen, draw_setup_controls_help_screen, draw_setup_screen,
    draw_syringe_select_screen,
};
pub use startup::{
    draw_boot_recovery_screen, draw_homing_limit_alert_screen, draw_homing_screen,
    draw_power_warning_screen, draw_startup_fault_screen, draw_startup_progress_screen,
};

pub async fn flush_frame(display: &mut Display, frame: &FrameBuffer, flush_buffer: &mut [u8]) {
    let frame_bytes = frame.as_bytes();
    let mut y = 0usize;

    while y < DISPLAY_H as usize {
        let rows = FLUSH_ROWS.min(DISPLAY_H as usize - y);
        let src_start = y * DISPLAY_W as usize * 2;
        let src_len = DISPLAY_W as usize * rows * 2;
        let src = &frame_bytes[src_start..src_start + src_len];
        let dst_len = DISPLAY_W as usize * rows * 3;
        let dst = &mut flush_buffer[..dst_len];

        for (src_pixel, dst_pixel) in src.chunks_exact(2).zip(dst.chunks_exact_mut(3)) {
            let hi = src_pixel[0];
            let lo = src_pixel[1];
            let r5 = hi >> 3;
            let g6 = ((hi & 0x07) << 3) | (lo >> 5);
            let b5 = lo & 0x1f;
            let r6 = (r5 << 1) | (r5 >> 4);
            let b6 = (b5 << 1) | (b5 >> 4);

            dst_pixel[0] = r6 << 2;
            dst_pixel[1] = g6 << 2;
            dst_pixel[2] = b6 << 2;
        }

        display
            .show_raw_data(0, y as u16, DISPLAY_W as u16, rows as u16, dst)
            .await
            .map_err(|error| log::error!("display flush failed at row {}: {:?}", y, error))
            .ok();
        y += rows;
    }
}

pub(super) const fn bgr565(r: u8, g: u8, b: u8) -> Rgb565 {
    // By swapping the R and B inputs, the BGR hardware reads the bits correctly.
    Rgb565::new(b, g, r)
}

pub(super) fn rgb888_to_bgr565(rgb: [u8; 3]) -> Rgb565 {
    bgr565(rgb[0] >> 3, rgb[1] >> 2, rgb[2] >> 3)
}

pub(super) const DASHBOARD_GRID_Y: i32 = 40;
pub(super) const DASHBOARD_DRUG_W: u32 = 328;
pub(super) const DASHBOARD_DRUG_H: u32 = 110;
pub(super) const DASHBOARD_FLOW_X: i32 = 328;
pub(super) const DASHBOARD_FLOW_W: u32 = DISPLAY_W - DASHBOARD_DRUG_W;
pub(super) const DASHBOARD_FLOW_H: u32 = 160;
pub(super) const DASHBOARD_DOSING_Y: i32 = DASHBOARD_GRID_Y + 160;
pub(super) const DASHBOARD_DOSING_H: u32 = 50;
pub(super) const DASHBOARD_INFO_Y: i32 = DASHBOARD_GRID_Y + 110;
pub(super) const DASHBOARD_INFO_W: u32 = 164;
pub(super) const DASHBOARD_INFO_H: u32 = 100;
pub(super) const DASHBOARD_PROGRESS_Y: i32 = DASHBOARD_GRID_Y + 220;
pub(super) const DASHBOARD_PROGRESS_X: i32 = 20;
pub(super) const DASHBOARD_PROGRESS_W: u32 = DISPLAY_W - 40;
pub(super) const DASHBOARD_PROGRESS_H: u32 = 24;
pub(super) const DASHBOARD_ALERT_Y: i32 = DASHBOARD_GRID_Y + DASHBOARD_DRUG_H as i32;
pub(super) const OVERTURE_LOGO_BMP: &[u8] = include_bytes!("../assets/overture_logo.bmp");
pub(super) const OVERTURE_LOGO_W: i32 = 283;
pub(super) const OVERTURE_LOGO_H: i32 = 34;
pub(super) const OVERTURE_LOGO_DATA_OFFSET: usize = 138;

pub(super) fn draw_left_triangle<D>(
    display: &mut D,
    x: i32,
    y: i32,
    color: Rgb565,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    Triangle::new(
        Point::new(x, y + 6),
        Point::new(x + 9, y),
        Point::new(x + 9, y + 12),
    )
    .into_styled(PrimitiveStyle::with_fill(color))
    .draw(display)
}

pub(super) fn clear_rect_color<D>(
    display: &mut D,
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    color: Rgb565,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    Rectangle::new(Point::new(x, y), Size::new(w, h))
        .into_styled(PrimitiveStyle::with_fill(color))
        .draw(display)
}

pub(super) fn draw_text<D>(
    display: &mut D,
    text: &str,
    x: i32,
    y: i32,
    font: &'static MonoFont<'static>,
    color: Rgb565,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    Text::with_baseline(
        text,
        Point::new(x, y),
        MonoTextStyle::new(font, color),
        Baseline::Top,
    )
    .draw(display)
    .map(|_| ())
}

pub(super) fn write_volume_ml<const N: usize>(out: &mut String<N>, ul: f32) {
    let ml_x100 = (ul / 10.0) as u32;
    let _ = write!(out, "{}.{:02} mL", ml_x100 / 100, ml_x100 % 100);
}

pub(super) fn write_rate_ml_h<const N: usize>(out: &mut String<N>, ul_per_min: f32) {
    let ml_h_x10 = (ul_per_min * 0.6) as u32;
    let _ = write!(out, "{}.{} mL/h", ml_h_x10 / 10, ml_h_x10 % 10);
}
