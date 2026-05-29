use super::*;

pub fn draw_startup_progress_screen<D>(
    display: &mut D,
    _status: &str,
    completed_steps: u32,
    total_steps: u32,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    let red = bgr565(31, 0, 0);
    let white = bgr565(31, 63, 31);

    display.clear(red)?;
    draw_overture_logo(display)?;

    let bounded_total = total_steps.max(1);
    let bounded_done = completed_steps.min(bounded_total);
    let bar_w = 336;
    let bar_h = 18;
    let bar_x = (DISPLAY_W as i32 - bar_w as i32) / 2;
    let bar_y = 210;
    let progress_width = bar_w * bounded_done / bounded_total;

    Rectangle::new(Point::new(bar_x, bar_y), Size::new(bar_w, bar_h))
        .into_styled(PrimitiveStyle::with_stroke(white, 2))
        .draw(display)?;
    if progress_width > 0 {
        Rectangle::new(
            Point::new(bar_x + 2, bar_y + 2),
            Size::new(progress_width.saturating_sub(4), bar_h - 4),
        )
        .into_styled(PrimitiveStyle::with_fill(white))
        .draw(display)?;
    }

    Ok(())
}

fn draw_overture_logo<D>(display: &mut D) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    let origin_x = (DISPLAY_W as i32 - OVERTURE_LOGO_W) / 2;
    let origin_y = 130;
    let row_stride = OVERTURE_LOGO_W as usize * 4;

    for y in 0..OVERTURE_LOGO_H {
        let bmp_y = OVERTURE_LOGO_H - 1 - y;
        let row_start = OVERTURE_LOGO_DATA_OFFSET + bmp_y as usize * row_stride;
        for x in 0..OVERTURE_LOGO_W {
            let pixel_start = row_start + x as usize * 4;
            let b = OVERTURE_LOGO_BMP[pixel_start] >> 3;
            let g = OVERTURE_LOGO_BMP[pixel_start + 1] >> 2;
            let r = OVERTURE_LOGO_BMP[pixel_start + 2] >> 3;
            let a = OVERTURE_LOGO_BMP[pixel_start + 3];

            if a < 8 {
                continue;
            }

            Pixel(Point::new(origin_x + x, origin_y + y), bgr565(r, g, b)).draw(display)?;
        }
    }

    Ok(())
}

pub fn draw_startup_fault_screen<D>(display: &mut D, fault: &str) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    display.clear(Rgb565::BLACK)?;

    let amber = Rgb565::new(31, 42, 0);
    let red = Rgb565::new(31, 0, 0);
    let dark_red = Rgb565::new(10, 0, 0);

    Rectangle::new(Point::new(0, 0), Size::new(DISPLAY_W, DISPLAY_H))
        .into_styled(PrimitiveStyle::with_fill(dark_red))
        .draw(display)?;
    Rectangle::new(Point::new(38, 72), Size::new(404, 154))
        .into_styled(PrimitiveStyle::with_stroke(red, 3))
        .draw(display)?;
    draw_text(
        display,
        "STARTUP FAULT",
        132,
        104,
        &FONT_10X20,
        Rgb565::WHITE,
    )?;
    draw_text(display, fault, 86, 152, &FONT_6X10, amber)?;
    draw_text(
        display,
        "Power cycle after fixing hardware",
        104,
        190,
        &FONT_6X10,
        Rgb565::WHITE,
    )?;

    Ok(())
}

pub fn draw_boot_recovery_screen<D>(
    display: &mut D,
    carriage_position_steps: i32,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    display.clear(Rgb565::BLACK)?;

    let amber = Rgb565::new(31, 42, 0);
    let panel = Rgb565::new(2, 4, 2);
    let line = Rgb565::new(7, 14, 7);

    Rectangle::new(Point::new(0, 0), Size::new(DISPLAY_W, DISPLAY_H))
        .into_styled(PrimitiveStyle::with_fill(panel))
        .draw(display)?;
    Rectangle::new(Point::new(42, 62), Size::new(396, 190))
        .into_styled(PrimitiveStyle::with_stroke(amber, 2))
        .draw(display)?;

    draw_text(display, "RECOVERY", 184, 88, &FONT_10X20, Rgb565::WHITE)?;
    draw_text(
        display,
        "Flash says syringe is mounted",
        120,
        132,
        &FONT_6X10,
        amber,
    )?;

    let mut position: String<40> = String::new();
    let _ = write!(
        position,
        "Stored carriage position: {} steps",
        carriage_position_steps
    );
    draw_text(display, &position, 104, 160, &FONT_6X10, Rgb565::WHITE)?;

    draw_text(
        display,
        "OK: resume saved state",
        142,
        204,
        &FONT_6X10,
        Rgb565::WHITE,
    )?;
    draw_text(
        display,
        "BACK: discard and home",
        140,
        226,
        &FONT_6X10,
        Rgb565::WHITE,
    )?;

    Rectangle::new(Point::new(42, 260), Size::new(396, 2))
        .into_styled(PrimitiveStyle::with_fill(line))
        .draw(display)?;

    Ok(())
}

pub fn draw_homing_limit_alert_screen<D>(display: &mut D) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    display.clear(Rgb565::BLACK)?;

    let amber = Rgb565::new(31, 42, 0);
    let red = Rgb565::new(31, 0, 0);
    let panel = Rgb565::new(10, 0, 0);

    Rectangle::new(Point::new(0, 0), Size::new(DISPLAY_W, DISPLAY_H))
        .into_styled(PrimitiveStyle::with_fill(panel))
        .draw(display)?;
    Rectangle::new(Point::new(38, 64), Size::new(404, 182))
        .into_styled(PrimitiveStyle::with_stroke(red, 3))
        .draw(display)?;

    draw_text(
        display,
        "HOME SWITCH HIT",
        120,
        96,
        &FONT_10X20,
        Rgb565::WHITE,
    )?;
    draw_text(
        display,
        "Motor stopped. Position reset to 0.",
        96,
        146,
        &FONT_6X10,
        amber,
    )?;
    draw_text(
        display,
        "Press OK to move to backoff",
        128,
        194,
        &FONT_6X10,
        Rgb565::WHITE,
    )?;

    Ok(())
}

pub fn draw_power_warning_screen<D>(
    display: &mut D,
    _rdo_msb: Option<u8>,
    _object_position: Option<u8>,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    // 1. Dark gray outer border background
    let dark_gray = Rgb565::new(4, 8, 4);
    display.clear(dark_gray)?;

    // 2. Main flat red card
    let flat_red = Rgb565::new(26, 12, 6);

    Rectangle::new(
        Point::new(10, 10),
        Size::new((DISPLAY_W - 20) as u32, (DISPLAY_H - 20) as u32),
    )
    .into_styled(PrimitiveStyle::with_fill(flat_red))
    .draw(display)?;

    let cx: i32 = (DISPLAY_W / 2) as i32;
    let cy_icon: i32 = 50;

    // 3. Initialize the FontRenderers
    let icon_font = FontRenderer::new::<fonts::u8g2_font_unifont_t_77>();
    let title_font = FontRenderer::new::<fonts::u8g2_font_helvB18_tf>();
    let body_font = FontRenderer::new::<fonts::u8g2_font_helvR14_tf>();

    // 4. Draw the Warning Icon using Unicode 0x26A0 (⚠️)
    let _ = icon_font.render_aligned(
        "\u{26A0}",
        Point::new(cx, cy_icon + 40), // Adjusted Y position for the smaller Unifont size
        VerticalPosition::Baseline,
        HorizontalAlignment::Center,
        FontColor::Transparent(Rgb565::WHITE),
        display,
    );

    // 5. Draw the text
    let _ = title_font.render_aligned(
        "This Power Source Cannot Be Used",
        Point::new(cx, cy_icon + 100),
        VerticalPosition::Baseline,
        HorizontalAlignment::Center,
        FontColor::Transparent(Rgb565::WHITE),
        display,
    );

    let _ = body_font.render_aligned(
        "Please use a USB-C PD power source",
        Point::new(cx, cy_icon + 140),
        VerticalPosition::Baseline,
        HorizontalAlignment::Center,
        FontColor::Transparent(Rgb565::WHITE),
        display,
    );

    let _ = body_font.render_aligned(
        "that can deliver 20V. Actual plugged in",
        Point::new(cx, cy_icon + 170),
        VerticalPosition::Baseline,
        HorizontalAlignment::Center,
        FontColor::Transparent(Rgb565::WHITE),
        display,
    );

    let _ = body_font.render_aligned(
        "source can only provide 5V.",
        Point::new(cx, cy_icon + 200),
        VerticalPosition::Baseline,
        HorizontalAlignment::Center,
        FontColor::Transparent(Rgb565::WHITE),
        display,
    );

    Ok(())
}

pub fn draw_homing_screen<D>(display: &mut D, active: bool, failed: bool) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    display.clear(Rgb565::BLACK)?;

    let amber = Rgb565::new(31, 42, 0);
    let red = Rgb565::new(31, 0, 0);
    let panel = if failed {
        Rgb565::new(10, 0, 0)
    } else {
        Rgb565::new(2, 4, 2)
    };

    Rectangle::new(Point::new(0, 0), Size::new(DISPLAY_W, DISPLAY_H))
        .into_styled(PrimitiveStyle::with_fill(panel))
        .draw(display)?;
    Rectangle::new(Point::new(42, 78), Size::new(396, 132))
        .into_styled(PrimitiveStyle::with_stroke(
            if failed { red } else { amber },
            2,
        ))
        .draw(display)?;

    if failed {
        draw_text(
            display,
            "HOMING FAILED",
            126,
            106,
            &FONT_10X20,
            Rgb565::WHITE,
        )?;
        draw_text(
            display,
            "Check GPIO15 limit switch",
            154,
            146,
            &FONT_6X10,
            amber,
        )?;
    } else {
        draw_text(display, "HOMING", 190, 106, &FONT_10X20, Rgb565::WHITE)?;
        draw_text(
            display,
            if active {
                "Limit switch detected"
            } else {
                "Moving to empty-syringe end"
            },
            142,
            146,
            &FONT_6X10,
            amber,
        )?;
    }

    Ok(())
}
