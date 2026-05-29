use super::components::*;
use super::*;

pub fn draw_dashboard_frame<D>(
    display: &mut D,
    prescription: &Prescription,
    _delivery_spreadcycle_enabled: bool, // <-- Added back here
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    let drug_selected = prescription.selected_drug().is_some();
    let black = bgr565(0, 0, 0);
    let white = bgr565(31, 63, 31);
    let grid_stroke = PrimitiveStyleBuilder::new()
        .stroke_color(white)
        .stroke_width(2)
        .build();

    let label_font = FontRenderer::new::<fonts::u8g2_font_helvR14_tf>();
    let subtitle_font = FontRenderer::new::<fonts::u8g2_font_helvR18_tf>();

    display.clear(black)?;

    Rectangle::new(
        Point::new(0, DASHBOARD_GRID_Y),
        Size::new(DASHBOARD_DRUG_W, DASHBOARD_DRUG_H),
    )
    .into_styled(grid_stroke)
    .draw(display)?;
    Rectangle::new(
        Point::new(DASHBOARD_FLOW_X, DASHBOARD_GRID_Y),
        Size::new(
            DASHBOARD_FLOW_W,
            if drug_selected {
                DASHBOARD_FLOW_H
            } else {
                DASHBOARD_FLOW_H + DASHBOARD_DOSING_H
            },
        ),
    )
    .into_styled(grid_stroke)
    .draw(display)?;
    let _ = label_font.render_aligned(
        "Flow Rate",
        Point::new(
            404,
            if drug_selected {
                DASHBOARD_GRID_Y + 30
            } else {
                DASHBOARD_GRID_Y + 55
            },
        ),
        VerticalPosition::Baseline,
        HorizontalAlignment::Center,
        FontColor::Transparent(white),
        display,
    );
    let _ = subtitle_font.render_aligned(
        "mL/h",
        Point::new(
            404,
            if drug_selected {
                DASHBOARD_GRID_Y + 135
            } else {
                DASHBOARD_GRID_Y + 165
            },
        ),
        VerticalPosition::Baseline,
        HorizontalAlignment::Center,
        FontColor::Transparent(white),
        display,
    );

    if drug_selected {
        Rectangle::new(
            Point::new(DASHBOARD_FLOW_X, DASHBOARD_DOSING_Y),
            Size::new(DASHBOARD_FLOW_W, DASHBOARD_DOSING_H),
        )
        .into_styled(grid_stroke)
        .draw(display)?;
        let mut dose_rate: String<24> = String::new();
        let _ = write!(
            dose_rate,
            "{:.1} mg/kg/h",
            prescription.dose_rate_ul_per_min
        );
        let _ = subtitle_font.render_aligned(
            dose_rate.as_str(),
            Point::new(404, DASHBOARD_DOSING_Y + 35),
            VerticalPosition::Baseline,
            HorizontalAlignment::Center,
            FontColor::Transparent(white),
            display,
        );
    }

    Rectangle::new(
        Point::new(0, DASHBOARD_INFO_Y),
        Size::new(DASHBOARD_INFO_W, DASHBOARD_INFO_H),
    )
    .into_styled(grid_stroke)
    .draw(display)?;
    let _ = label_font.render_aligned(
        "Time Remaining",
        Point::new(82, DASHBOARD_INFO_Y + 35),
        VerticalPosition::Baseline,
        HorizontalAlignment::Center,
        FontColor::Transparent(white),
        display,
    );

    Rectangle::new(
        Point::new(DASHBOARD_INFO_W as i32, DASHBOARD_INFO_Y),
        Size::new(DASHBOARD_INFO_W, DASHBOARD_INFO_H),
    )
    .into_styled(grid_stroke)
    .draw(display)?;
    let _ = label_font.render_aligned(
        "Remaining Volume",
        Point::new(246, DASHBOARD_INFO_Y + 35),
        VerticalPosition::Baseline,
        HorizontalAlignment::Center,
        FontColor::Transparent(white),
        display,
    );

    let _ = label_font.render_aligned(
        "Progress",
        Point::new(DASHBOARD_PROGRESS_X, DASHBOARD_PROGRESS_Y + 15),
        VerticalPosition::Baseline,
        HorizontalAlignment::Left,
        FontColor::Transparent(white),
        display,
    );
    Rectangle::new(
        Point::new(DASHBOARD_PROGRESS_X, DASHBOARD_PROGRESS_Y + 30),
        Size::new(DASHBOARD_PROGRESS_W, DASHBOARD_PROGRESS_H),
    )
    .into_styled(grid_stroke)
    .draw(display)?;

    Ok(())
}

// pub fn draw_dashboard_frame<D>(
//     display: &mut D,
//     prescription: &Prescription,
//     delivery_spreadcycle_enabled: bool,
// ) -> Result<(), D::Error>
// where
//     D: DrawTarget<Color = Rgb565>,
// {
//     display.clear(Rgb565::WHITE)?;

//     let amber = Rgb565::new(31, 42, 0);
//     let text = Rgb565::BLACK;
//     let line = Rgb565::new(18, 36, 18);
//     let soft = Rgb565::new(29, 61, 29);
//     let pale = Rgb565::new(31, 63, 28);

//     Rectangle::new(Point::new(0, 0), Size::new(DISPLAY_W, 30))
//         .into_styled(PrimitiveStyle::with_fill(Rgb565::WHITE))
//         .draw(display)?;
//     Rectangle::new(Point::new(0, 30), Size::new(DISPLAY_W, 2))
//         .into_styled(PrimitiveStyle::with_fill(line))
//         .draw(display)?;
//     draw_text(display, "ESP32-C6 Syringe Pump", 310, 8, &FONT_6X10, text)?;

//     Rectangle::new(Point::new(0, 32), Size::new(214, 96))
//         .into_styled(PrimitiveStyle::with_fill(pale))
//         .draw(display)?;
//     draw_text(display, "Test Syringe", 12, 58, &FONT_10X20, Rgb565::BLACK)?;
//     let mut syringe: String<24> = String::new();
//     let _ = write!(
//         syringe,
//         "{} ID {:.2} mm",
//         prescription.syringe.label, prescription.syringe.inner_diameter_mm
//     );
//     draw_text(display, &syringe, 14, 92, &FONT_6X10, Rgb565::BLACK)?;

//     Rectangle::new(Point::new(216, 32), Size::new(264, 96))
//         .into_styled(PrimitiveStyle::with_fill(Rgb565::WHITE))
//         .draw(display)?;
//     Rectangle::new(Point::new(216, 32), Size::new(264, 96))
//         .into_styled(PrimitiveStyle::with_stroke(line, 1))
//         .draw(display)?;
//     let mut rate: String<12> = String::new();
//     let _ = write!(rate, "{}", rate_ml_h(prescription) as u32);
//     draw_text(display, &rate, 318, 38, &FONT_10X20, text)?;
//     draw_text(display, "mL/h", 420, 56, &FONT_6X10, amber)?;
//     let mut dose_rate: String<24> = String::new();
//     write_rate_ml_h(&mut dose_rate, delivery_rate_ul_per_min(prescription));
//     draw_text(display, &dose_rate, 316, 92, &FONT_6X10, soft)?;

//     Rectangle::new(Point::new(0, 128), Size::new(DISPLAY_W, 2))
//         .into_styled(PrimitiveStyle::with_fill(line))
//         .draw(display)?;

//     draw_panel(display, 0, 130, 160, 66, "Time Remaining")?;
//     draw_panel(display, 160, 130, 160, 66, "VTBI")?;
//     let mut vtbi: String<16> = String::new();
//     write_volume_ml(&mut vtbi, delivery_target_ul(prescription));
//     draw_text(display, &vtbi, 186, 162, &FONT_10X20, text)?;

//     draw_panel(display, 320, 130, 160, 66, "Delivered")?;
//     draw_panel(display, 0, 196, 240, 68, "Mode")?;
//     draw_panel(display, 240, 196, 240, 68, "Driver")?;
//     draw_text(
//         display,
//         chopper_mode_name(delivery_spreadcycle_enabled),
//         252,
//         230,
//         &FONT_6X10,
//         text,
//     )?;
//     let mut microsteps: String<20> = String::new();
//     let _ = write!(microsteps, "{} microsteps", MICROSTEPS);
//     draw_text(display, &microsteps, 360, 230, &FONT_6X10, soft)?;

//     Rectangle::new(Point::new(16, 286), Size::new(448, 14))
//         .into_styled(PrimitiveStyle::with_fill(Rgb565::new(24, 49, 24)))
//         .draw(display)?;

//     Ok(())
// }

pub fn draw_dashboard_values<D>(
    display: &mut D,
    mode: BenchMode,
    dose: &DoseAccumulator,
    delivery: &DeliveryState,
    prescription: &Prescription,
    flow_phase: usize,
    alarm_flash_on: bool,
    bolus_active: bool,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    draw_syringe_status_block(display, prescription, delivery.alert, alarm_flash_on)?;

    let black = bgr565(0, 0, 0);
    let white = bgr565(31, 63, 31);
    let green = bgr565(6, 50, 6);
    let orange = bgr565(31, 39, 3);
    let neon_yellow = bgr565(28, 63, 14);
    let status_color = if bolus_active || delivery.running {
        green
    } else if matches!(mode, BenchMode::Delivery) {
        orange
    } else {
        bgr565(12, 30, 12)
    };

    clear_rect_color(display, 0, 0, 328, 36, black)?;
    draw_flow_indicator(display, bolus_active || delivery.running, flow_phase)?;
    let status = if bolus_active {
        "Bolus"
    } else if delivery.kvo_active {
        "KVO"
    } else if delivery.running {
        "Perfusing"
    } else if matches!(mode, BenchMode::Delivery) {
        "Paused"
    } else {
        "Manual"
    };
    let status_font = FontRenderer::new::<fonts::u8g2_font_helvR14_tf>();
    let _ = status_font.render_aligned(
        status,
        Point::new(82, 26),
        VerticalPosition::Baseline,
        HorizontalAlignment::Left,
        FontColor::Transparent(status_color),
        display,
    );

    let drug_selected = prescription.selected_drug().is_some();
    clear_rect_color(
        display,
        DASHBOARD_FLOW_X + 10,
        if drug_selected {
            DASHBOARD_GRID_Y + 52
        } else {
            DASHBOARD_GRID_Y + 82
        },
        132,
        48,
        black,
    )?;
    let mut rate: String<12> = String::new();
    let ml_h_x10 = (rate_ml_h(prescription) * 10.0) as u32;
    let _ = write!(rate, "{}.{}", ml_h_x10 / 10, ml_h_x10 % 10);
    let _ = FontRenderer::new::<fonts::u8g2_font_logisoso38_tn>().render_aligned(
        rate.as_str(),
        Point::new(
            404,
            if drug_selected {
                DASHBOARD_GRID_Y + 95
            } else {
                DASHBOARD_GRID_Y + 125
            },
        ),
        VerticalPosition::Baseline,
        HorizontalAlignment::Center,
        FontColor::Transparent(white),
        display,
    );

    clear_rect_color(display, 8, DASHBOARD_INFO_Y + 50, 148, 30, black)?;
    let seconds = approx_delivery_seconds(
        delivery.remaining_steps,
        dispense_step_period_us(prescription),
    ) as u32;
    let mut time: String<24> = String::new();
    let _ = write!(
        time,
        "{}h {}m {}s",
        seconds / 3600,
        (seconds / 60) % 60,
        seconds % 60
    );
    let value_font = FontRenderer::new::<fonts::u8g2_font_helvR14_tf>();
    let _ = value_font.render_aligned(
        time.as_str(),
        Point::new(82, DASHBOARD_INFO_Y + 76),
        VerticalPosition::Baseline,
        HorizontalAlignment::Center,
        FontColor::Transparent(white),
        display,
    );

    clear_rect_color(display, 172, DASHBOARD_INFO_Y + 50, 148, 30, black)?;
    let target_ul = delivery_target_ul(prescription).max(0.0);
    let remaining_ul = (target_ul - dose.total_ul).max(0.0);
    let (rem_ml, rem_dec) = ul_to_ml_parts(remaining_ul);
    let (target_ml, target_dec) = ul_to_ml_parts(target_ul);
    let mut remaining: String<24> = String::new();
    let _ = write!(
        remaining,
        "{}.{:02}/{}.{:02} mL",
        rem_ml, rem_dec, target_ml, target_dec
    );
    let _ = value_font.render_aligned(
        remaining.as_str(),
        Point::new(246, DASHBOARD_INFO_Y + 76),
        VerticalPosition::Baseline,
        HorizontalAlignment::Center,
        FontColor::Transparent(white),
        display,
    );

    let progress_width = if delivery.dose_steps > 0 {
        (DASHBOARD_PROGRESS_W - 4) * delivery.delivered_steps_this_dose / delivery.dose_steps
    } else {
        0
    };
    clear_rect_color(
        display,
        DASHBOARD_PROGRESS_X + 2,
        DASHBOARD_PROGRESS_Y + 32,
        DASHBOARD_PROGRESS_W - 4,
        DASHBOARD_PROGRESS_H - 4,
        black,
    )?;
    Rectangle::new(
        Point::new(DASHBOARD_PROGRESS_X + 2, DASHBOARD_PROGRESS_Y + 32),
        Size::new(progress_width, DASHBOARD_PROGRESS_H - 4),
    )
    .into_styled(PrimitiveStyle::with_fill(if progress_width > 0 {
        neon_yellow
    } else {
        black
    }))
    .draw(display)?;

    draw_dashboard_alert_overlay(display, delivery.alert, alarm_flash_on)?;

    Ok(())
}

pub fn draw_syringe_status_block<D>(
    display: &mut D,
    prescription: &Prescription,
    _alert: DeliveryAlert,
    _alarm_flash_on: bool,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    let black = bgr565(0, 0, 0);
    let white = bgr565(31, 63, 31);
    let grey = bgr565(10, 20, 10);
    let drug = prescription.selected_drug();

    let background = drug
        .map(|drug| rgb888_to_bgr565(drug.color_rgb))
        .unwrap_or(grey);
    let text_color = if drug.is_some() { black } else { white };
    let cx = (DASHBOARD_DRUG_W / 2) as i32;

    Rectangle::new(
        Point::new(0, DASHBOARD_GRID_Y),
        Size::new(DASHBOARD_DRUG_W, DASHBOARD_DRUG_H),
    )
    .into_styled(
        PrimitiveStyleBuilder::new()
            .fill_color(background)
            .stroke_color(white)
            .stroke_width(2)
            .build(),
    )
    .draw(display)?;

    let title_font = FontRenderer::new::<fonts::u8g2_font_helvB24_tf>();
    let subtitle_font = FontRenderer::new::<fonts::u8g2_font_helvR18_tf>();
    let small_font = FontRenderer::new::<fonts::u8g2_font_helvR14_tf>();

    let mut draw_center = |font: &FontRenderer, text: &str, y_pos: i32| -> Result<(), D::Error> {
        let _ = font.render_aligned(
            text,
            Point::new(cx, y_pos),
            VerticalPosition::Baseline,
            HorizontalAlignment::Center,
            FontColor::Transparent(text_color),
            display,
        );
        Ok(())
    };

    if let Some(drug) = drug {
        draw_center(&title_font, drug.drug_name, DASHBOARD_GRID_Y + 50)?;
        draw_center(
            &subtitle_font,
            drug.typical_concentration,
            DASHBOARD_GRID_Y + 90,
        )?;
    } else {
        draw_center(&small_font, "No Drug Selected", DASHBOARD_GRID_Y + 66)?;
    }

    Ok(())
}

fn draw_dashboard_alert_overlay<D>(
    display: &mut D,
    alert: DeliveryAlert,
    pulse_on: bool,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    let Some((warning, header, detail_line_1, detail_line_2)) = dashboard_alert_text(alert) else {
        return Ok(());
    };

    draw_custom_alert_overlay(
        display,
        warning,
        header,
        detail_line_1,
        detail_line_2,
        pulse_on,
    )
}

fn dashboard_alert_text(
    alert: DeliveryAlert,
) -> Option<(bool, &'static str, &'static str, Option<&'static str>)> {
    match alert {
        DeliveryAlert::EndOfInfusion => Some((
            true,
            "Perfusion End",
            "Press OK to add VTBI",
            Some("or BACK to remove syringe"),
        )),
        DeliveryAlert::KvoRunning => Some((false, "KVO started", "Press OK to dismiss", None)),
        DeliveryAlert::SyringeEmpty => Some((
            true,
            "Syringe Empty",
            "Press OK to remove",
            Some("the syringe"),
        )),
        DeliveryAlert::PressureRelieved => Some((
            false,
            "Pressure relieved",
            "Press OK after you",
            Some("removed the syringe"),
        )),
        DeliveryAlert::DosingFault => Some((
            true,
            "Dosing Fault",
            "Check setup.",
            Some("Press OK to dismiss"),
        )),
        DeliveryAlert::None | DeliveryAlert::Standby => None,
    }
}

fn draw_flow_indicator<D>(display: &mut D, running: bool, flow_phase: usize) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    let dark = Rgb565::new(18, 38, 18);
    let mid = Rgb565::new(0, 36, 0);
    let bright = Rgb565::new(0, 52, 0);

    for i in 0..FLOW_TRIANGLES {
        let color = if running {
            let active = FLOW_TRIANGLES - 1 - flow_phase;
            if i == active {
                bright
            } else if i + 1 == active {
                mid
            } else {
                dark
            }
        } else {
            dark
        };

        let x = 10 + i as i32 * 13;
        draw_left_triangle(display, x, 12, color)?;
    }

    Ok(())
}
