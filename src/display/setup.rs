use super::components::*;
use super::*;

pub fn draw_syringe_select_screen<D>(display: &mut D, selected: usize) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    draw_white_page_header(display, "Select Syringe")?;

    let labels = [
        "2.5 mL BD PlastiPak",
        "5 mL B.Braun Injekt",
        "20 mL B.Braun OPS",
    ];

    for (index, label) in labels.iter().enumerate() {
        draw_white_option_row(display, index, 62 + index as i32 * 58, label, "", selected)?;
    }

    Ok(())
}

pub fn draw_drug_select_screen<D>(display: &mut D, selected: usize) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    draw_white_page_header(display, "Select Drug")?;
    draw_white_option_row(display, 0, 44, "No Drug Selected", "Skip", selected)?;

    for (index, drug) in DRUG_LIBRARY.iter().enumerate() {
        let y = 82 + index as i32 * 31;
        draw_drug_option_row(
            display,
            index + 1,
            y,
            selected,
            drug.drug_name,
            drug.typical_concentration,
            drug.color_rgb,
        )?;
    }

    Ok(())
}

pub fn draw_patient_weight_screen<D>(
    display: &mut D,
    weight_kg: f32,
    selected_item: usize,
    editing: bool,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    draw_white_page_header(display, "Patient Weight")?;

    let white = bgr565(31, 63, 31);
    let black = bgr565(0, 0, 0);
    let green = bgr565(11, 48, 6);
    let edit_green = bgr565(7, 38, 4);
    let label_font = FontRenderer::new::<fonts::u8g2_font_helvR14_tf>();
    let button_font = FontRenderer::new::<fonts::u8g2_font_helvB18_tf>();
    let digit_font = FontRenderer::new::<fonts::u8g2_font_logisoso38_tn>();

    let bounded = weight_kg.clamp(0.0, 999.99);
    let value = (bounded * 100.0) as u32;
    let digits = [
        ((value / 10000) % 10) as u8,
        ((value / 1000) % 10) as u8,
        ((value / 100) % 10) as u8,
        ((value / 10) % 10) as u8,
        (value % 10) as u8,
    ];
    let xs = [80, 135, 190, 278, 333];
    let y = 104;

    for (index, digit) in digits.iter().enumerate() {
        let selected = selected_item == index;
        let fill = if selected && editing {
            edit_green
        } else if selected {
            green
        } else {
            white
        };
        let text = if selected { white } else { black };
        RoundedRectangle::new(
            Rectangle::new(Point::new(xs[index], y), Size::new(42, 62)),
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

        let mut digit_text: String<2> = String::new();
        let _ = write!(digit_text, "{}", digit);
        let _ = digit_font.render_aligned(
            digit_text.as_str(),
            Point::new(xs[index] + 21, y + 52),
            VerticalPosition::Baseline,
            HorizontalAlignment::Center,
            FontColor::Transparent(text),
            display,
        );
    }

    let _ = digit_font.render_aligned(
        ".",
        Point::new(248, y + 52),
        VerticalPosition::Baseline,
        HorizontalAlignment::Center,
        FontColor::Transparent(black),
        display,
    );
    let _ = label_font.render_aligned(
        "kg",
        Point::new(390, y + 44),
        VerticalPosition::Baseline,
        HorizontalAlignment::Left,
        FontColor::Transparent(black),
        display,
    );
    let continue_selected = selected_item == 5;
    RoundedRectangle::new(
        Rectangle::new(Point::new(60, 226), Size::new((DISPLAY_W - 120) as u32, 44)),
        CornerRadii::new(Size::new(12, 12)),
    )
    .into_styled(
        PrimitiveStyleBuilder::new()
            .fill_color(if continue_selected { green } else { white })
            .stroke_color(black)
            .stroke_width(1)
            .build(),
    )
    .draw(display)?;
    let _ = button_font.render_aligned(
        "Continue",
        Point::new((DISPLAY_W / 2) as i32, 255),
        VerticalPosition::Baseline,
        HorizontalAlignment::Center,
        FontColor::Transparent(if continue_selected { white } else { black }),
        display,
    );

    let _ = label_font.render_aligned(
        if editing {
            "Rotate to edit digit. Press OK to lock."
        } else {
            "Rotate to select. Press OK to edit."
        },
        Point::new(34, 296),
        VerticalPosition::Baseline,
        HorizontalAlignment::Left,
        FontColor::Transparent(black),
        display,
    );

    Ok(())
}

pub fn draw_nfc_syringe_detected_screen<D>(
    display: &mut D,
    syringe: SyringeSpec,
    drug: Option<DrugSpec>,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    draw_white_page_header(display, "Syringe Detected")?;

    let black = bgr565(0, 0, 0);
    let grey = bgr565(23, 46, 23);
    let card_fill = drug
        .map(|spec| rgb888_to_bgr565(spec.color_rgb))
        .unwrap_or(grey);
    let medication_font = FontRenderer::new::<fonts::u8g2_font_helvB24_tf>();
    let syringe_font = FontRenderer::new::<fonts::u8g2_font_helvB24_tf>();
    let prompt_font = FontRenderer::new::<fonts::u8g2_font_helvR14_tf>();

    RoundedRectangle::new(
        Rectangle::new(Point::new(24, 78), Size::new(DISPLAY_W - 48, 130)),
        CornerRadii::new(Size::new(10, 10)),
    )
    .into_styled(PrimitiveStyle::with_fill(card_fill))
    .draw(display)?;

    let mut medication_text: String<48> = String::new();
    if let Some(drug) = drug {
        let _ = write!(
            medication_text,
            "{} {}",
            drug.drug_name,
            concentration_per_1ml(drug.typical_concentration)
        );
        let _ = medication_font.render_aligned(
            medication_text.as_str(),
            Point::new((DISPLAY_W / 2) as i32, 128),
            VerticalPosition::Baseline,
            HorizontalAlignment::Center,
            FontColor::Transparent(black),
            display,
        );
    }

    let _ = syringe_font.render_aligned(
        syringe_display_name(syringe),
        Point::new((DISPLAY_W / 2) as i32, 176),
        VerticalPosition::Baseline,
        HorizontalAlignment::Center,
        FontColor::Transparent(black),
        display,
    );

    let _ = prompt_font.render_aligned(
        "Press OK to confirm or Back to manually",
        Point::new((DISPLAY_W / 2) as i32, 252),
        VerticalPosition::Baseline,
        HorizontalAlignment::Center,
        FontColor::Transparent(black),
        display,
    );
    let _ = prompt_font.render_aligned(
        "select parameters",
        Point::new((DISPLAY_W / 2) as i32, 280),
        VerticalPosition::Baseline,
        HorizontalAlignment::Center,
        FontColor::Transparent(black),
        display,
    );

    Ok(())
}

pub fn draw_load_opening_screen<D>(display: &mut D, syringe: SyringeSpec) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    draw_white_page_header(display, "Load Syringe")?;

    let black = bgr565(0, 0, 0);
    let label_font = FontRenderer::new::<fonts::u8g2_font_helvR14_tf>();

    let mut distance: String<32> = String::new();
    let _ = write!(
        distance,
        "Opening {:.1} mm",
        syringe_load_travel_mm(syringe)
    );

    draw_instruction_panel(display, 76, 156)?;
    let _ = label_font.render_aligned(
        distance.as_str(),
        Point::new((DISPLAY_W / 2) as i32, 136),
        VerticalPosition::Baseline,
        HorizontalAlignment::Center,
        FontColor::Transparent(black),
        display,
    );
    let _ = label_font.render_aligned(
        "Wait before inserting syringe",
        Point::new((DISPLAY_W / 2) as i32, 190),
        VerticalPosition::Baseline,
        HorizontalAlignment::Center,
        FontColor::Transparent(black),
        display,
    );

    Ok(())
}

pub fn draw_load_adjust_screen<D>(display: &mut D, _syringe: SyringeSpec) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    draw_white_page_header(display, "Seat Syringe")?;

    let black = bgr565(0, 0, 0);
    let soft = bgr565(16, 32, 16);
    draw_instruction_panel(display, 70, 206)?;
    draw_instruction_text(display, 118, "Rotate encoder for rough adjust", soft)?;
    draw_instruction_text(display, 156, "Hold BOLUS for fine advance", soft)?;
    draw_instruction_text(display, 194, "BACK returns to drug selection", soft)?;
    draw_instruction_text(display, 232, "Press OK when syringe is seated", black)?;

    Ok(())
}

pub fn draw_setup_controls_help_screen<D>(display: &mut D) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    draw_white_page_header(display, "Controls")?;

    let black = bgr565(0, 0, 0);
    let soft = bgr565(16, 32, 16);
    let label_font = FontRenderer::new::<fonts::u8g2_font_helvR14_tf>();
    let action_font = FontRenderer::new::<fonts::u8g2_font_helvB18_tf>();
    let hint_font = FontRenderer::new::<fonts::u8g2_font_helvR12_tf>();
    let right_x = DISPLAY_W as i32 - 24;

    let mut draw_pair =
        |y: i32, control: &str, tap_action: &str, hold_action: &str| -> Result<(), D::Error> {
            let _ = label_font.render_aligned(
                control,
                Point::new(right_x, y),
                VerticalPosition::Baseline,
                HorizontalAlignment::Right,
                FontColor::Transparent(soft),
                display,
            );
            let mut action: String<48> = String::new();
            let _ = write!(action, "{} / {}", tap_action, hold_action);
            let _ = action_font.render_aligned(
                action.as_str(),
                Point::new(right_x, y + 28),
                VerticalPosition::Baseline,
                HorizontalAlignment::Right,
                FontColor::Transparent(black),
                display,
            );
            let _ = hint_font.render_aligned(
                "short press / hold",
                Point::new(right_x, y + 50),
                VerticalPosition::Baseline,
                HorizontalAlignment::Right,
                FontColor::Transparent(soft),
                display,
            );
            Ok(())
        };

    draw_pair(72, "BOLUS button", "Bolus menu", "Immediate bolus")?;
    draw_pair(148, "BACK button", "Back", "Settings")?;

    let _ = label_font.render_aligned(
        "ENCODER",
        Point::new(right_x, 248),
        VerticalPosition::Baseline,
        HorizontalAlignment::Right,
        FontColor::Transparent(soft),
        display,
    );
    let _ = action_font.render_aligned(
        "Navigate / OK",
        Point::new(right_x, 276),
        VerticalPosition::Baseline,
        HorizontalAlignment::Right,
        FontColor::Transparent(black),
        display,
    );
    let _ = hint_font.render_aligned(
        "rotate / press knob   BACK exits",
        Point::new(right_x, 298),
        VerticalPosition::Baseline,
        HorizontalAlignment::Right,
        FontColor::Transparent(soft),
        display,
    );

    Ok(())
}

pub fn draw_prime_screen<D>(display: &mut D, _syringe: SyringeSpec) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    draw_white_page_header(display, "Prime Syringe")?;

    let black = bgr565(0, 0, 0);
    let red = bgr565(31, 0, 0);
    let soft = bgr565(16, 32, 16);
    draw_instruction_panel(display, 62, 224)?;
    Rectangle::new(Point::new(32, 86), Size::new(DISPLAY_W - 64, 58))
        .into_styled(
            PrimitiveStyleBuilder::new()
                .stroke_color(red)
                .stroke_width(2)
                .build(),
        )
        .draw(display)?;
    draw_instruction_text(display, 109, "Make sure IV is disconnected", red)?;
    draw_instruction_text(display, 129, "from the patient", red)?;
    draw_instruction_text(display, 178, "Hold BOLUS to prime slowly", soft)?;
    draw_instruction_text(display, 216, "Release when liquid reaches tube end", soft)?;
    draw_instruction_text(display, 250, "Press OK to continue to setup", black)?;

    Ok(())
}

pub fn draw_setup_screen<D>(
    display: &mut D,
    prescription: &Prescription,
    selected: usize,
    _editing: bool, // Passed in case you need to blink text or add an edit cursor later
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    // Define UI Colors
    let white = bgr565(31, 63, 31);
    let black = bgr565(0, 0, 0);
    let inactive_color = bgr565(16, 32, 16); // Gray for inactive strokes and text
    let green_btn = bgr565(11, 48, 6); // Vibrant Green

    // 1. Clear background
    display.clear(white)?;

    // 2. Initialize Fonts
    let title_font = FontRenderer::new::<fonts::u8g2_font_helvB18_tf>();
    let label_font = FontRenderer::new::<fonts::u8g2_font_helvR14_tf>();
    let val_font = FontRenderer::new::<fonts::u8g2_font_helvB14_tf>();

    // 3. Draw Header (Switched to label_font as requested)
    let _ = label_font.render_aligned(
        "Perfusion Setup",
        Point::new(15, 24),
        VerticalPosition::Baseline,
        HorizontalAlignment::Left,
        FontColor::Transparent(black),
        display,
    );

    // Thick underline
    Line::new(Point::new(0, 34), Point::new(DISPLAY_W as i32, 34))
        .into_styled(
            PrimitiveStyleBuilder::new()
                .stroke_color(black)
                .stroke_width(2)
                .build(),
        )
        .draw(display)?;

    // 4. Helper Closure for drawing rows
    let mut draw_row =
        |idx: usize, y: i32, label: &str, val: &str, is_inactive: bool| -> Result<(), D::Error> {
            let is_selected = selected == idx;

            // Selection = Green Background, otherwise White
            let fill_color = if is_selected { green_btn } else { white };

            // Inactive = Gray Stroke, otherwise standard Black stroke
            let stroke_color = if is_inactive { inactive_color } else { black };

            let rect_style = PrimitiveStyleBuilder::new()
                .stroke_color(stroke_color)
                .stroke_width(1)
                .fill_color(fill_color)
                .build();

            // Draw Rounded Input Box
            RoundedRectangle::new(
                Rectangle::new(Point::new(15, y), Size::new((DISPLAY_W - 30) as u32, 38)),
                CornerRadii::new(Size::new(8, 8)),
            )
            .into_styled(rect_style)
            .draw(display)?;

            // Text Color Logic: Gray if inactive, White if selected (for contrast against green), Black otherwise
            let text_color = if is_inactive {
                inactive_color
            } else if is_selected {
                white
            } else {
                black
            };

            // Draw Left-Aligned Label
            let _ = label_font.render_aligned(
                label,
                Point::new(30, y + 26),
                VerticalPosition::Baseline,
                HorizontalAlignment::Left,
                FontColor::Transparent(text_color),
                display,
            );

            // Draw Right-Aligned Value
            let _ = val_font.render_aligned(
                val,
                Point::new((DISPLAY_W - 30) as i32, y + 26),
                VerticalPosition::Baseline,
                HorizontalAlignment::Right,
                FontColor::Transparent(text_color),
                display,
            );

            Ok(())
        };

    let drug_selected = prescription.selected_drug().is_some();

    // 5. Draw Row 1: Dose
    let mut dose_str: String<24> = String::new();
    let _ = write!(dose_str, "{:.1} mg/kg/h", prescription.dose_rate_ul_per_min);
    draw_row(0, 48, "Dose", &dose_str, !drug_selected)?;

    // 6. Draw Row 2: VTBI
    let mut vtbi_str: String<16> = String::new();
    write_volume_ml(&mut vtbi_str, prescription.vtbi_ul);
    draw_row(1, 94, "VTBI", &vtbi_str, false)?;

    // 7. Draw Row 3: Flow Rate
    let mut flow_str: String<16> = String::new();
    write_rate_ml_h(&mut flow_str, prescription.flow_rate_ul_per_min);
    draw_row(2, 140, "Flow Rate", &flow_str, drug_selected)?;

    // 8. Draw Row 4: Perfusion Time
    let total_seconds = (prescription.infusion_time_min * 60.0) as u32;
    let mut time_str: String<20> = String::new();
    let _ = write!(
        time_str,
        "{}h {}min {}s",
        total_seconds / 3600,
        (total_seconds / 60) % 60,
        total_seconds % 60
    );
    draw_row(3, 186, "Perfusion Time", &time_str, false)?;

    // 9. Draw "Start Perfusion" Button
    let is_start_selected = selected == 4;

    // Start button now mirrors the rows: green fill when selected, white otherwise. Always black stroke.
    let btn_fill = if is_start_selected { green_btn } else { white };
    let btn_text_color = if is_start_selected { white } else { black };

    let btn_style = PrimitiveStyleBuilder::new()
        .fill_color(btn_fill)
        .stroke_color(black)
        .stroke_width(1)
        .build();

    RoundedRectangle::new(
        Rectangle::new(Point::new(60, 240), Size::new((DISPLAY_W - 120) as u32, 44)),
        CornerRadii::new(Size::new(12, 12)),
    )
    .into_styled(btn_style)
    .draw(display)?;

    let _ = title_font.render_aligned(
        "Start Perfusion",
        Point::new((DISPLAY_W / 2) as i32, 269),
        VerticalPosition::Baseline,
        HorizontalAlignment::Center,
        FontColor::Transparent(btn_text_color),
        display,
    );

    Ok(())
}

pub fn draw_settings_screen<D>(
    display: &mut D,
    kvo_enabled: bool,
    kvo_rate_ul_per_min: f32,
    direct_bolus_rate_ul_per_min: f32,
    delivery_spreadcycle_enabled: bool,
    flash_write_count: u32,
    selected: usize,
    editing: bool,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    display.clear(Rgb565::BLACK)?;

    let amber = Rgb565::new(31, 42, 0);
    let line = Rgb565::new(7, 14, 7);
    let panel = Rgb565::new(2, 4, 2);

    Rectangle::new(Point::new(0, 0), Size::new(DISPLAY_W, 34))
        .into_styled(PrimitiveStyle::with_fill(panel))
        .draw(display)?;
    Rectangle::new(Point::new(0, 34), Size::new(DISPLAY_W, 2))
        .into_styled(PrimitiveStyle::with_fill(line))
        .draw(display)?;
    draw_text(display, "Settings", 14, 9, &FONT_10X20, Rgb565::WHITE)?;
    draw_text(
        display,
        "Rotate select/change  Press edit",
        254,
        12,
        &FONT_6X10,
        amber,
    )?;

    const SETTINGS_DRAW_ITEMS: usize = 9;
    const SETTINGS_VISIBLE_ROWS: usize = 6;
    const SETTINGS_ROW_Y: [i32; SETTINGS_VISIBLE_ROWS] = [44, 82, 120, 158, 196, 234];
    let max_first = SETTINGS_DRAW_ITEMS.saturating_sub(SETTINGS_VISIBLE_ROWS);
    let first_visible = if selected >= SETTINGS_VISIBLE_ROWS {
        selected + 1 - SETTINGS_VISIBLE_ROWS
    } else {
        0
    }
    .min(max_first);
    let mut kvo_rate: String<20> = String::new();
    write_rate_ml_h(&mut kvo_rate, kvo_rate_ul_per_min);
    let mut bolus_rate: String<20> = String::new();
    write_rate_ml_h(&mut bolus_rate, direct_bolus_rate_ul_per_min);
    let mut flash_writes: String<20> = String::new();
    let _ = write!(flash_writes, "{}", flash_write_count);

    for (row, y) in SETTINGS_ROW_Y.iter().enumerate() {
        let index = first_visible + row;
        let (label, value, editable) = match index {
            0 => ("End Perfusion", "Show alert", false),
            1 => ("Controls Help", "Open", false),
            2 => (
                "KVO",
                if kvo_enabled { "Enabled" } else { "Disabled" },
                false,
            ),
            3 => ("KVO Rate", kvo_rate.as_str(), true),
            4 => ("Hold Bolus Rate", bolus_rate.as_str(), true),
            5 => (
                "Perfusion Driver",
                if delivery_spreadcycle_enabled {
                    "SpreadCycle"
                } else {
                    "StealthChop"
                },
                false,
            ),
            6 => ("Flash Writes", flash_writes.as_str(), false),
            7 => ("Tutti Frutti", "Play motor song", false),
            8 => ("Back", "Return", false),
            _ => ("", "", false),
        };
        draw_setup_row(
            display,
            index,
            *y,
            label,
            value,
            selected,
            editing && editable,
        )?;
    }

    let position_text = if first_visible == 0 {
        "More below"
    } else if first_visible == max_first {
        "More above"
    } else {
        "More above/below"
    };
    draw_text(display, position_text, 354, 294, &FONT_6X10, amber)?;

    Ok(())
}

pub fn draw_bolus_setup_screen<D>(
    display: &mut D,
    bolus_volume_ul: f32,
    bolus_rate_ul_per_min: f32,
    selected: usize,
    editing: bool,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    let black = bgr565(0, 0, 0);
    let white = bgr565(31, 63, 31);
    let green = bgr565(11, 48, 6);
    let soft = bgr565(18, 36, 18);
    let label_font = FontRenderer::new::<fonts::u8g2_font_helvR14_tf>();
    let title_font = FontRenderer::new::<fonts::u8g2_font_helvB18_tf>();

    display.clear(black)?;
    let _ = label_font.render_aligned(
        "Programmed Bolus",
        Point::new(15, 24),
        VerticalPosition::Baseline,
        HorizontalAlignment::Left,
        FontColor::Transparent(white),
        display,
    );
    Line::new(Point::new(0, 34), Point::new(DISPLAY_W as i32, 34))
        .into_styled(
            PrimitiveStyleBuilder::new()
                .stroke_color(white)
                .stroke_width(2)
                .build(),
        )
        .draw(display)?;

    let mut draw_row = |idx: usize, y: i32, label: &str, value: &str| -> Result<(), D::Error> {
        let is_selected = selected == idx;
        let fill = if is_selected { green } else { black };
        RoundedRectangle::new(
            Rectangle::new(Point::new(15, y), Size::new((DISPLAY_W - 30) as u32, 42)),
            CornerRadii::new(Size::new(8, 8)),
        )
        .into_styled(
            PrimitiveStyleBuilder::new()
                .stroke_color(if editing && is_selected { green } else { soft })
                .stroke_width(if editing && is_selected { 2 } else { 1 })
                .fill_color(fill)
                .build(),
        )
        .draw(display)?;
        let _ = label_font.render_aligned(
            label,
            Point::new(30, y + 28),
            VerticalPosition::Baseline,
            HorizontalAlignment::Left,
            FontColor::Transparent(white),
            display,
        );
        let _ = label_font.render_aligned(
            value,
            Point::new((DISPLAY_W - 30) as i32, y + 28),
            VerticalPosition::Baseline,
            HorizontalAlignment::Right,
            FontColor::Transparent(white),
            display,
        );
        Ok(())
    };

    let mut volume: String<20> = String::new();
    write_volume_ml(&mut volume, bolus_volume_ul);
    draw_row(0, 70, "Volume", &volume)?;

    let mut rate: String<20> = String::new();
    write_rate_ml_h(&mut rate, bolus_rate_ul_per_min);
    draw_row(1, 124, "Rate", &rate)?;

    let is_start_selected = selected == 2;
    RoundedRectangle::new(
        Rectangle::new(Point::new(60, 222), Size::new((DISPLAY_W - 120) as u32, 44)),
        CornerRadii::new(Size::new(12, 12)),
    )
    .into_styled(
        PrimitiveStyleBuilder::new()
            .fill_color(if is_start_selected { green } else { black })
            .stroke_color(white)
            .stroke_width(1)
            .build(),
    )
    .draw(display)?;
    let _ = title_font.render_aligned(
        "Start Bolus",
        Point::new((DISPLAY_W / 2) as i32, 251),
        VerticalPosition::Baseline,
        HorizontalAlignment::Center,
        FontColor::Transparent(white),
        display,
    );

    Ok(())
}

pub fn draw_remove_syringe_prompt_screen<D>(display: &mut D) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    display.clear(Rgb565::BLACK)?;

    let amber = Rgb565::new(31, 42, 0);
    let panel = Rgb565::new(2, 4, 2);
    let line = Rgb565::new(7, 14, 7);

    Rectangle::new(Point::new(0, 0), Size::new(DISPLAY_W, 34))
        .into_styled(PrimitiveStyle::with_fill(panel))
        .draw(display)?;
    Rectangle::new(Point::new(0, 34), Size::new(DISPLAY_W, 2))
        .into_styled(PrimitiveStyle::with_fill(line))
        .draw(display)?;
    draw_text(
        display,
        "Infusion Complete",
        14,
        9,
        &FONT_10X20,
        Rgb565::WHITE,
    )?;

    draw_text(
        display,
        "Remove syringe?",
        146,
        98,
        &FONT_10X20,
        Rgb565::WHITE,
    )?;
    draw_text(
        display,
        "Press OK to relieve plunger pressure",
        100,
        148,
        &FONT_6X10,
        amber,
    )?;
    draw_text(
        display,
        "Back keeps syringe mounted",
        154,
        180,
        &FONT_6X10,
        amber,
    )?;

    Ok(())
}

pub fn draw_confirm_syringe_removed_screen<D>(display: &mut D) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    display.clear(Rgb565::BLACK)?;

    let amber = Rgb565::new(31, 42, 0);
    let panel = Rgb565::new(2, 4, 2);
    let line = Rgb565::new(7, 14, 7);

    Rectangle::new(Point::new(0, 0), Size::new(DISPLAY_W, 34))
        .into_styled(PrimitiveStyle::with_fill(panel))
        .draw(display)?;
    Rectangle::new(Point::new(0, 34), Size::new(DISPLAY_W, 2))
        .into_styled(PrimitiveStyle::with_fill(line))
        .draw(display)?;
    draw_text(
        display,
        "Pressure Relieved",
        14,
        9,
        &FONT_10X20,
        Rgb565::WHITE,
    )?;

    draw_text(
        display,
        "Pressure relieved",
        120,
        98,
        &FONT_10X20,
        Rgb565::WHITE,
    )?;
    draw_text(
        display,
        "Press OK after syringe is removed",
        112,
        148,
        &FONT_6X10,
        amber,
    )?;

    Ok(())
}
