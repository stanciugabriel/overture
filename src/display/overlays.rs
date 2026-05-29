use super::components::*;
use super::*;

pub fn draw_direct_bolus_overlay<D>(
    display: &mut D,
    delivery: &DeliveryState,
    total_ul: f32,
    window_ul: f32,
    window_limit_ul: f32,
    wait_release: bool,
    bolus_active: bool,
    rate_ul_per_min: f32,
    flow_phase: usize,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    let black = bgr565(0, 0, 0);
    let white = bgr565(31, 63, 31);
    let orange = bgr565(31, 39, 3);
    let dark_orange = bgr565(12, 15, 1);
    let margin = 20;
    let bar_x = margin;
    let bar_y = DASHBOARD_GRID_Y;
    let bar_w = DISPLAY_W - (margin as u32 * 2);
    let bar_h = DASHBOARD_DRUG_H;

    display.clear(black)?;
    let (status, moving, status_color) = if bolus_active {
        ("Manual Bolus", true, orange)
    } else if delivery.running {
        ("Perfusing", true, bgr565(6, 50, 6))
    } else {
        ("Paused", false, bgr565(12, 30, 12))
    };
    draw_colored_status_bar(display, status, moving, flow_phase, status_color)?;

    Rectangle::new(Point::new(bar_x, bar_y), Size::new(bar_w, bar_h))
        .into_styled(PrimitiveStyle::with_stroke(white, 2))
        .draw(display)?;

    let bounded_limit = window_limit_ul.max(1.0);
    let bounded_window = window_ul.clamp(0.0, bounded_limit);
    let fill_width = ((bar_w - 4) as f32 * bounded_window / bounded_limit) as u32;
    if fill_width > 0 {
        Rectangle::new(
            Point::new(bar_x + 2, bar_y + 2),
            Size::new(fill_width, bar_h - 4),
        )
        .into_styled(PrimitiveStyle::with_fill(if wait_release {
            dark_orange
        } else {
            orange
        }))
        .draw(display)?;
    }

    let label_font = FontRenderer::new::<fonts::u8g2_font_helvR14_tf>();
    let value_font = FontRenderer::new::<fonts::u8g2_font_logisoso38_tn>();
    let _ = label_font.render_aligned(
        "Bolus delivered",
        Point::new(bar_x + 18, bar_y + 28),
        VerticalPosition::Baseline,
        HorizontalAlignment::Left,
        FontColor::Transparent(white),
        display,
    );

    let mut rate: String<24> = String::new();
    write_rate_ml_h(&mut rate, rate_ul_per_min);
    let mut rate_text: String<40> = String::new();
    let _ = write!(rate_text, "Bolus rate = {}", rate);
    let _ = label_font.render_aligned(
        rate_text.as_str(),
        Point::new(bar_x + bar_w as i32 - 18, bar_y + 28),
        VerticalPosition::Baseline,
        HorizontalAlignment::Right,
        FontColor::Transparent(white),
        display,
    );

    let mut total: String<20> = String::new();
    write_volume_ml(&mut total, total_ul);
    let _ = value_font.render_aligned(
        total.as_str(),
        Point::new(bar_x + 18, bar_y + bar_h as i32 - 16),
        VerticalPosition::Baseline,
        HorizontalAlignment::Left,
        FontColor::Transparent(white),
        display,
    );

    draw_text(
        display,
        if wait_release {
            "Release bolus button"
        } else {
            "BACK returns to main screen"
        },
        bar_x + 20,
        bar_y + bar_h as i32 + 18,
        &FONT_10X20,
        white,
    )?;

    Ok(())
}

pub fn draw_bolus_delivered_alert_overlay<D>(
    display: &mut D,
    delivered_ul: f32,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    let mut volume: String<20> = String::new();
    write_volume_ml(&mut volume, delivered_ul);
    let mut header: String<40> = String::new();
    let _ = write!(header, "{} Bolus Delivered", volume);

    draw_custom_alert_overlay(
        display,
        false,
        header.as_str(),
        "Press OK to dismiss",
        None,
        true,
    )
}

pub fn draw_bolus_administered_alert_overlay<D>(
    display: &mut D,
    delivered_ul: f32,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    let mut volume: String<20> = String::new();
    write_volume_ml(&mut volume, delivered_ul);
    let mut header: String<44> = String::new();
    let _ = write!(header, "{} Bolus Administered", volume);

    draw_custom_alert_overlay(
        display,
        false,
        header.as_str(),
        "Press OK to dismiss",
        None,
        true,
    )
}

pub fn draw_recover_perfusion_alert_overlay<D>(display: &mut D) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    draw_custom_alert_overlay(
        display,
        false,
        "Recover Perfusion",
        "Press OK to recover",
        Some("or press BACK to continue homing"),
        true,
    )
}
