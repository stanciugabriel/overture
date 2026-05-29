use super::*;

pub(super) fn draw_white_page_header<D>(display: &mut D, title: &str) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    let white = bgr565(31, 63, 31);
    let black = bgr565(0, 0, 0);
    let label_font = FontRenderer::new::<fonts::u8g2_font_helvR14_tf>();

    display.clear(white)?;
    let _ = label_font.render_aligned(
        title,
        Point::new(15, 24),
        VerticalPosition::Baseline,
        HorizontalAlignment::Left,
        FontColor::Transparent(black),
        display,
    );
    Line::new(Point::new(0, 34), Point::new(DISPLAY_W as i32, 34))
        .into_styled(
            PrimitiveStyleBuilder::new()
                .stroke_color(black)
                .stroke_width(2)
                .build(),
        )
        .draw(display)
}

pub(super) fn draw_white_option_row<D>(
    display: &mut D,
    index: usize,
    y: i32,
    label: &str,
    value: &str,
    selected: usize,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    let white = bgr565(31, 63, 31);
    let black = bgr565(0, 0, 0);
    let green = bgr565(11, 48, 6);
    let fill = if selected == index { green } else { white };
    let text = if selected == index { white } else { black };
    let label_font = FontRenderer::new::<fonts::u8g2_font_helvR14_tf>();
    let value_font = FontRenderer::new::<fonts::u8g2_font_helvB14_tf>();

    RoundedRectangle::new(
        Rectangle::new(Point::new(15, y), Size::new((DISPLAY_W - 30) as u32, 42)),
        CornerRadii::new(Size::new(8, 8)),
    )
    .into_styled(
        PrimitiveStyleBuilder::new()
            .stroke_color(black)
            .stroke_width(1)
            .fill_color(fill)
            .build(),
    )
    .draw(display)?;

    let _ = label_font.render_aligned(
        label,
        Point::new(30, y + 28),
        VerticalPosition::Baseline,
        HorizontalAlignment::Left,
        FontColor::Transparent(text),
        display,
    );

    if !value.is_empty() {
        let _ = value_font.render_aligned(
            value,
            Point::new((DISPLAY_W - 30) as i32, y + 28),
            VerticalPosition::Baseline,
            HorizontalAlignment::Right,
            FontColor::Transparent(text),
            display,
        );
    }

    Ok(())
}

pub(super) fn draw_instruction_panel<D>(display: &mut D, y: i32, h: u32) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    let black = bgr565(0, 0, 0);
    RoundedRectangle::new(
        Rectangle::new(Point::new(24, y), Size::new(DISPLAY_W - 48, h)),
        CornerRadii::new(Size::new(10, 10)),
    )
    .into_styled(
        PrimitiveStyleBuilder::new()
            .stroke_color(black)
            .stroke_width(1)
            .build(),
    )
    .draw(display)
}

pub(super) fn draw_instruction_text<D>(
    display: &mut D,
    y: i32,
    text: &str,
    color: Rgb565,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    let label_font = FontRenderer::new::<fonts::u8g2_font_helvR14_tf>();
    let _ = label_font.render_aligned(
        text,
        Point::new((DISPLAY_W / 2) as i32, y),
        VerticalPosition::Baseline,
        HorizontalAlignment::Center,
        FontColor::Transparent(color),
        display,
    );

    Ok(())
}

pub(super) fn syringe_display_name(syringe: SyringeSpec) -> &'static str {
    match syringe.label {
        "2.5 mL" => "2.5 mL BD PlastiPak",
        "5 mL" => "5 mL B.Braun Injekt",
        "20 mL" => "20 mL B.Braun OPS",
        _ => syringe.label,
    }
}

pub(super) fn concentration_per_1ml(concentration: &str) -> String<24> {
    let mut text = String::new();
    if let Some(prefix) = concentration.strip_suffix("/mL") {
        let _ = write!(text, "{} / 1mL", prefix.trim());
    } else {
        let _ = write!(text, "{}", concentration);
    }
    text
}

pub(super) fn draw_drug_option_row<D>(
    display: &mut D,
    index: usize,
    y: i32,
    selected: usize,
    name: &str,
    concentration: &str,
    color_rgb: [u8; 3],
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    let white = bgr565(31, 63, 31);
    let black = bgr565(0, 0, 0);
    let green = bgr565(11, 48, 6);
    let fill = if selected == index { green } else { white };
    let text = if selected == index { white } else { black };
    let label_font = FontRenderer::new::<fonts::u8g2_font_helvR12_tf>();
    let value_font = FontRenderer::new::<fonts::u8g2_font_helvB12_tf>();

    RoundedRectangle::new(
        Rectangle::new(Point::new(15, y), Size::new((DISPLAY_W - 30) as u32, 31)),
        CornerRadii::new(Size::new(7, 7)),
    )
    .into_styled(
        PrimitiveStyleBuilder::new()
            .stroke_color(black)
            .stroke_width(1)
            .fill_color(fill)
            .build(),
    )
    .draw(display)?;

    Rectangle::new(Point::new(27, y + 8), Size::new(18, 15))
        .into_styled(
            PrimitiveStyleBuilder::new()
                .fill_color(rgb888_to_bgr565(color_rgb))
                .stroke_color(black)
                .stroke_width(1)
                .build(),
        )
        .draw(display)?;

    let _ = value_font.render_aligned(
        name,
        Point::new(58, y + 22),
        VerticalPosition::Baseline,
        HorizontalAlignment::Left,
        FontColor::Transparent(text),
        display,
    );
    let _ = label_font.render_aligned(
        concentration,
        Point::new((DISPLAY_W - 30) as i32, y + 22),
        VerticalPosition::Baseline,
        HorizontalAlignment::Right,
        FontColor::Transparent(text),
        display,
    );

    Ok(())
}

pub(super) fn draw_setup_row<D>(
    display: &mut D,
    index: usize,
    y: i32,
    label: &str,
    value: &str,
    selected: usize,
    editing: bool,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    let active = selected == index;
    let background = if active && editing {
        Rgb565::new(31, 42, 0)
    } else if active {
        Rgb565::new(0, 22, 0)
    } else {
        Rgb565::BLACK
    };
    let value_color = if active && editing {
        Rgb565::BLACK
    } else {
        Rgb565::WHITE
    };
    let label_color = if active && editing {
        Rgb565::BLACK
    } else {
        Rgb565::new(31, 42, 0)
    };

    Rectangle::new(Point::new(18, y), Size::new(444, 36))
        .into_styled(PrimitiveStyle::with_fill(background))
        .draw(display)?;
    Rectangle::new(Point::new(18, y), Size::new(444, 36))
        .into_styled(PrimitiveStyle::with_stroke(Rgb565::new(7, 14, 7), 1))
        .draw(display)?;

    draw_text(display, label, 30, y + 7, &FONT_6X10, label_color)?;
    draw_text(display, value, 250, y + 8, &FONT_10X20, value_color)?;

    if active {
        draw_text(
            display,
            if editing { "EDIT" } else { "SEL" },
            420,
            y + 12,
            &FONT_6X10,
            value_color,
        )?;
    }

    Ok(())
}

pub(super) fn draw_custom_alert_overlay<D>(
    display: &mut D,
    warning: bool,
    header: &str,
    detail_line_1: &str,
    detail_line_2: Option<&str>,
    pulse_on: bool,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    let black = bgr565(0, 0, 0);
    let yellow = bgr565(28, 63, 14);
    let red = if pulse_on {
        bgr565(31, 0, 0)
    } else {
        bgr565(13, 0, 0)
    };
    let stroke = if warning { red } else { yellow };
    let header_font = FontRenderer::new::<fonts::u8g2_font_helvB18_tf>();
    let detail_font = FontRenderer::new::<fonts::u8g2_font_helvR12_tf>();

    let y = DASHBOARD_ALERT_Y;
    let h = DISPLAY_H - DASHBOARD_ALERT_Y as u32;
    RoundedRectangle::new(
        Rectangle::new(Point::new(0, y), Size::new(DISPLAY_W, h)),
        CornerRadiiBuilder::new().bottom(Size::new(14, 14)).build(),
    )
    .into_styled(
        PrimitiveStyleBuilder::new()
            .fill_color(black)
            .stroke_color(stroke)
            .stroke_width(5)
            .build(),
    )
    .draw(display)?;

    Triangle::new(
        Point::new(54, y + 44),
        Point::new(30, y + 86),
        Point::new(78, y + 86),
    )
    .into_styled(
        PrimitiveStyleBuilder::new()
            .stroke_color(stroke)
            .stroke_width(5)
            .build(),
    )
    .draw(display)?;
    let _ = header_font.render_aligned(
        "!",
        Point::new(54, y + 77),
        VerticalPosition::Baseline,
        HorizontalAlignment::Center,
        FontColor::Transparent(stroke),
        display,
    );
    let _ = header_font.render_aligned(
        header,
        Point::new(96, y + 60),
        VerticalPosition::Baseline,
        HorizontalAlignment::Left,
        FontColor::Transparent(stroke),
        display,
    );
    let _ = detail_font.render_aligned(
        detail_line_1,
        Point::new(96, y + 96),
        VerticalPosition::Baseline,
        HorizontalAlignment::Left,
        FontColor::Transparent(bgr565(31, 63, 31)),
        display,
    );
    if let Some(detail_line_2) = detail_line_2 {
        let _ = detail_font.render_aligned(
            detail_line_2,
            Point::new(96, y + 120),
            VerticalPosition::Baseline,
            HorizontalAlignment::Left,
            FontColor::Transparent(bgr565(31, 63, 31)),
            display,
        );
    }

    Ok(())
}

pub(super) fn draw_colored_status_bar<D>(
    display: &mut D,
    status: &str,
    running: bool,
    flow_phase: usize,
    color: Rgb565,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    let black = bgr565(0, 0, 0);
    let dark = bgr565(10, 10, 1);
    clear_rect_color(display, 0, 0, 328, 36, black)?;

    for i in 0..FLOW_TRIANGLES {
        let active = FLOW_TRIANGLES - 1 - flow_phase;
        let triangle_color = if running && (i == active || i + 1 == active) {
            color
        } else {
            dark
        };
        let x = 10 + i as i32 * 13;
        draw_left_triangle(display, x, 12, triangle_color)?;
    }

    let status_font = FontRenderer::new::<fonts::u8g2_font_helvR14_tf>();
    let _ = status_font.render_aligned(
        status,
        Point::new(82, 26),
        VerticalPosition::Baseline,
        HorizontalAlignment::Left,
        FontColor::Transparent(color),
        display,
    );

    Ok(())
}
