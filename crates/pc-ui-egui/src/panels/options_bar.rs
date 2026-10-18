//! The options bar (per-tool controls and brush pickers) and the status bar.
use super::*;

/// Decimals a custom crop-ratio side keeps and shows: 1.414 : 1 (ISO A sizes), as in Photoshop.
const CROP_RATIO_DECIMALS: u32 = 3;

/// local-image: the painting tools' Mode list: the blend modes with Photoshop's Behind and Clear
/// after Dissolve.
pub(super) fn paint_mode_dropdown(ui: &mut egui::Ui, id: &str, b: &mut photocraft_engine::BrushSettings) {
    use photocraft_engine::paint::brush::PaintMode;
    let mut opts: Vec<((BlendMode, PaintMode), &str)> = BlendMode::LAYER_MODES.iter().map(|m| ((*m, PaintMode::Blend), m.label())).collect();
    let at = opts.iter().position(|((m, _), _)| *m == BlendMode::Dissolve).map_or(opts.len(), |i| i + 1);
    opts.insert(at, ((BlendMode::Normal, PaintMode::Clear), "Clear"));
    opts.insert(at, ((BlendMode::Normal, PaintMode::Behind), "Behind"));
    let mut current = (if b.paint_mode == PaintMode::Blend { b.mode } else { BlendMode::Normal }, b.paint_mode);
    if widgets::dropdown(ui, id, &mut current, &opts, 96.0) {
        (b.mode, b.paint_mode) = current;
    }
}

// Options bar

pub fn options_bar(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    crate::toolsets_ui::persona::frame(app, ui.ctx());
    let t = Tokens::get(ui.ctx());
    let panel = egui::Panel::top("options_bar")
        .exact_size(if t.pro { 36.0 } else { 42.0 })
        .frame(egui::Frame::NONE.fill(t.chrome).inner_margin(egui::Margin::symmetric(10, 0)))
        .show(ui, |ui| {
            let r = ui.max_rect();
            ui.painter().line_segment([r.left_top(), r.right_top()], Stroke::new(1.0, t.separator));
            ui.painter().line_segment([r.left_bottom(), r.right_bottom()], Stroke::new(1.0, t.separator));
            tool_options_scroll(ui, app, |app, ui| {
                if t.pro {
                    crate::chrome_ui::home_button(app, ui);
                    widgets::vline(ui, 22.0);
                }
                // The tool's icon opens the Tool Presets panel (Photoshop's tool preset picker).
                if icons::tool_button(
                    ui,
                    icons::tool_icon_name_for(app.ui.tool, app.session.prefs().toolbar.persona),
                    if t.pro { 26.0 } else { 28.0 },
                    !t.pro,
                    tl!("Tool presets"),
                )
                .clicked()
                {
                    let ctx = ui.ctx().clone();
                    let _ = crate::menus::invoke(app, &ctx, "window.panel.toolPresets", json!({}));
                }
                if t.pro {
                    let (r, _) = ui.allocate_exact_size(vec2(10.0, 20.0), Sense::hover());
                    icons::paint(ui, r, "chevron-down", 10.0, t.text_faint);
                }
                widgets::vline(ui, 22.0);
                if app.ui.tool == Tool::RotateView {
                    crate::canvas::rotate_view::options(app, ui);
                    return;
                }
                if app.session.prefs().toolbar.persona == photocraft_engine::toolsets::Persona::Vector {
                    crate::toolsets_ui::persona::geometry_bar(app, ui);
                    widgets::vline(ui, 22.0);
                }
                if app.ui.transform.is_some() {
                    crate::transform_tool::options_bar(app, ui);
                    return;
                }
                if crate::distort_ui::options_bar(app, ui) {
                    return;
                }
                let tool = app.ui.tool;
                // Brush edits here go through `tools.setBrush`, one journal entry per gesture (Rule 1).
                if (tool.is_brushlike() && !matches!(tool, Tool::Brush | Tool::Pencil | Tool::MixerBrush | Tool::Eraser)) || tool == Tool::QuickSelection {
                    let before = app.session.tools.brush.clone();
                    let mut b = before.clone();
                    let pick = brush_preset_chip(ui, &mut b, &app.session.tools.presets);
                    crate::brush_panel::commit_gesture(app, ui.ctx(), &before, &b);
                    crate::brush_picker::apply(app, ui.ctx(), pick);
                    crate::brush_picker::settings_toggle(app, ui);
                    widgets::vline(ui, 22.0);
                }
                if crate::eraser_ui::options_bar(app, ui, tool)
                    || crate::retouch_ui::options_bar(app, ui, tool)
                    || crate::vector_ui::options_bar(app, ui, tool)
                    || crate::analysis_ui::options_bar(app, ui, tool)
                    || crate::slice_ui::options_bar(app, ui, tool)
                {
                    return;
                }
                crate::paint_mouse::sync_tool_smoothing(app);
                let brush_before = app.session.tools.brush.clone();
                let mut brush = brush_before.clone();
                let b = &mut brush;
                let vector_options = crate::vector_integration::context(app, ui);
                if vector_options {
                    return;
                }
                let mut picked = None;
                match app.ui.tool {
                    Tool::Brush | Tool::Eraser => {
                        picked = brush_preset_chip(ui, b, &app.session.tools.presets);
                        crate::brush_picker::settings_toggle(app, ui);
                        widgets::vline(ui, 22.0);
                        opt_label(ui, tl!("Size"));
                        widgets::value_field(ui, &mut b.size, 1.0..=photocraft_engine::paint::MAX_BRUSH_SIZE, "px", 76.0);
                        percent_field(ui, tl!("Hardness"), &mut b.hardness, 0.0..=100.0, 66.0);
                        opt_label(ui, tl!("Mode"));
                        paint_mode_dropdown(ui, "brush-mode", b);
                        percent_field(ui, tl!("Opacity"), &mut b.opacity, 0.0..=100.0, 62.0);
                        if icons::button(ui, "circle-dot", 24.0, b.pressure_opacity, tl!("Always use pressure for opacity")).clicked() {
                            b.pressure_opacity = !b.pressure_opacity;
                        }
                        percent_field(ui, tl!("Flow"), &mut b.flow, 1.0..=100.0, 62.0);
                        if icons::button(ui, "sparkles", 24.0, b.build_up, tl!("Enable airbrush-style build-up effects")).clicked() {
                            b.build_up = !b.build_up;
                        }
                        opt_label(ui, tl!("Smoothing"));
                        smoothing_field(ui, b, 58.0);
                        smoothing_options(ui, b);
                        widgets::vline(ui, 22.0);
                        if icons::button(ui, "circle-dot", 24.0, b.pressure_size, tl!("Always use pressure for size")).clicked() {
                            b.pressure_size = !b.pressure_size;
                        }
                        let current = app.session.active().and_then(|st| st.symmetry_path.as_ref().map(|a| a.source.clone()));
                        if let Some((cmd, params)) = symmetry_menu(ui, current.as_deref()) {
                            let r = app.run(cmd, params);
                            if let Err(e) = r {
                                app.ui.status = e;
                                app.ui.status_error = true;
                            }
                        }
                    }
                    // Pencil: Photoshop's options (no hardness or flow: the pencil is always hard).
                    Tool::Pencil => {
                        picked = brush_preset_chip(ui, b, &app.session.tools.presets);
                        crate::brush_picker::settings_toggle(app, ui);
                        widgets::vline(ui, 22.0);
                        opt_label(ui, tl!("Size"));
                        widgets::value_field(ui, &mut b.size, 1.0..=photocraft_engine::paint::MAX_BRUSH_SIZE, "px", 76.0);
                        widgets::vline(ui, 22.0);
                        opt_label(ui, tl!("Mode"));
                        paint_mode_dropdown(ui, "pencil-mode", b);
                        percent_field(ui, tl!("Opacity"), &mut b.opacity, 0.0..=100.0, if t.pro { 62.0 } else { 66.0 });
                        opt_label(ui, tl!("Smoothing"));
                        smoothing_field(ui, b, if t.pro { 58.0 } else { 66.0 });
                        widgets::vline(ui, 22.0);
                        widgets::checkbox(ui, &mut app.ui.tool_options.pencil_auto_erase, tl!("Auto Erase"));
                    }
                    Tool::MixerBrush => {
                        picked = brush_preset_chip(ui, b, &app.session.tools.presets);
                        crate::brush_picker::settings_toggle(app, ui);
                        widgets::vline(ui, 22.0);
                        for (label, value) in
                            [(tl!("Wet"), &mut b.mixer.wet), (tl!("Load"), &mut b.mixer.load), (tl!("Mix"), &mut b.mixer.mix), (tl!("Flow"), &mut b.mixer.flow)]
                        {
                            percent_field(ui, label, value, 0.0..=100.0, 62.0);
                        }
                        widgets::vline(ui, 22.0);
                        widgets::checkbox(ui, &mut b.mixer.sample_all_layers, tl!("Sample All Layers"));
                    }
                    Tool::RectMarquee | Tool::EllipseMarquee => {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        for (i, (icon, tip)) in [
                            ("square", tl!("New selection").to_string()),
                            ("plus", li_i18n::fmt(tl!("Add to selection  ({key})"), &[("key", &crate::shortcuts::pretty("Shift"))])),
                            ("minus", li_i18n::fmt(tl!("Subtract from selection  ({key})"), &[("key", &crate::shortcuts::pretty("Alt"))])),
                            ("squares-subtract", li_i18n::fmt(tl!("Intersect with selection  ({key})"), &[("key", &crate::shortcuts::pretty("Shift+Alt"))])),
                        ]
                        .iter()
                        .enumerate()
                        {
                            if icons::button(ui, icon, 24.0, app.ui.selection_mode == i as u8, tip).clicked() {
                                app.ui.selection_mode = i as u8;
                            }
                        }
                        ui.spacing_mut().item_spacing.x = 8.0;
                        widgets::vline(ui, 22.0);
                        let o = &mut app.ui.tool_options;
                        opt_label(ui, tl!("Feather"));
                        widgets::value_field(ui, &mut o.feather, 0.0..=1000.0, "px", 62.0);
                        // Photoshop greys Anti-alias for the Rectangular Marquee (its edges are always hard).
                        ui.add_enabled_ui(app.ui.tool == Tool::EllipseMarquee, |ui| widgets::checkbox(ui, &mut o.anti_alias, tl!("Anti-alias")));
                        widgets::vline(ui, 22.0);
                        opt_label(ui, tl!("Style"));
                        let styles = [
                            ("normal".to_string(), tl!("Normal")),
                            ("fixedRatio".to_string(), tl!("Fixed Ratio")),
                            ("fixedSize".to_string(), tl!("Fixed Size")),
                        ];
                        if widgets::dropdown(ui, "marquee-style", &mut o.marquee_style, &styles, 96.0) {
                            (o.marquee_width, o.marquee_height) = if o.marquee_style == "fixedSize" { (64.0, 64.0) } else { (1.0, 1.0) };
                        }
                        let fixed = o.marquee_style != "normal";
                        let (unit, range) = if o.marquee_style == "fixedSize" { ("px", 1.0..=300_000.0) } else { ("", 0.001..=999.0) };
                        ui.add_enabled_ui(fixed, |ui| {
                            opt_label(ui, tl!("Width"));
                            widgets::value_field(ui, &mut o.marquee_width, range.clone(), unit, 62.0);
                            if icons::button(ui, "arrow-left-right", 22.0, false, tl!("Swaps height and width")).clicked() {
                                std::mem::swap(&mut o.marquee_width, &mut o.marquee_height);
                            }
                            opt_label(ui, tl!("Height"));
                            widgets::value_field(ui, &mut o.marquee_height, range, unit, 62.0);
                        });
                        widgets::vline(ui, 22.0);
                        if widgets::secondary_button(ui, tl!("Select and Mask…"), 0.0).clicked() {
                            let _ = crate::menus::invoke(app, ui.ctx(), "select.selectAndMask", json!({}));
                        }
                    }
                    Tool::Lasso | Tool::PolygonLasso | Tool::MagneticLasso => {
                        selection_mode_buttons(app, ui);
                        widgets::vline(ui, 22.0);
                        opt_label(ui, tl!("Feather"));
                        widgets::value_field(ui, &mut app.ui.tool_options.feather, 0.0..=1000.0, "px", 62.0);
                        widgets::checkbox(ui, &mut app.ui.tool_options.anti_alias, tl!("Anti-alias"));
                        widgets::vline(ui, 22.0);
                        if app.ui.tool == Tool::MagneticLasso {
                            let o = &mut app.ui.tool_options;
                            opt_label(ui, tl!("Width"));
                            widgets::value_field(ui, &mut o.magnetic_width, 1.0..=256.0, "px", 58.0)
                                .on_hover_text(tl!("Follows only edges this close to the pointer ([ and ] change it)"));
                            opt_label(ui, tl!("Contrast"));
                            widgets::value_field(ui, &mut o.magnetic_contrast, 1.0..=100.0, "%", 58.0)
                                .on_hover_text(tl!("Higher values follow only edges that contrast sharply with their surroundings"));
                            opt_label(ui, tl!("Frequency"));
                            widgets::value_field(ui, &mut o.magnetic_frequency, 0.0..=100.0, "", 50.0)
                                .on_hover_text(tl!("How often fastening points are placed by themselves"));
                            if icons::button(ui, "circle-dot", 24.0, o.magnetic_pressure, tl!("Use tablet pressure to change pen width")).clicked() {
                                o.magnetic_pressure = !o.magnetic_pressure;
                            }
                            widgets::vline(ui, 22.0);
                        }
                        if widgets::secondary_button(ui, tl!("Select and Mask…"), 0.0).clicked() {
                            let _ = crate::menus::invoke(app, ui.ctx(), "select.selectAndMask", json!({}));
                        }
                        if crate::lasso_ui::active(app) || (app.ui.tool == Tool::PolygonLasso && !app.ui.polygon.is_empty()) {
                            hint(
                                ui,
                                &li_i18n::fmt(
                                    tl!("Click the first point or press {key} to close · Esc cancels"),
                                    &[("key", &crate::shortcuts::pretty("Enter"))],
                                ),
                            );
                        } else if app.ui.tool == Tool::Lasso {
                            hint(ui, tl!("Hold Alt while drawing for straight segments"));
                        }
                        if app.ui.tool == Tool::MagneticLasso && app.ui.magnetic.active() {
                            hint(
                                ui,
                                &li_i18n::fmt(
                                    tl!("Click the first point or press {key} to close · {del} removes a point · Esc cancels"),
                                    &[("key", &crate::shortcuts::pretty("Enter")), ("del", &crate::shortcuts::pretty("Backspace"))],
                                ),
                            );
                        }
                    }
                    // local-image: Red Eye options (Photoshop's).
                    Tool::RedEye => {
                        opt_label(ui, tl!("Pupil Size"));
                        widgets::value_field(ui, &mut app.ui.tool_options.red_eye_pupil, 1.0..=100.0, "%", 58.0);
                        opt_label(ui, tl!("Darken Amount"));
                        widgets::value_field(ui, &mut app.ui.tool_options.red_eye_darken, 1.0..=100.0, "%", 58.0);
                        let o = &mut app.ui.tool_options;
                        widgets::checkbox(ui, &mut o.red_eye_pet, "Pet Eye");
                        if o.red_eye_pet {
                            widgets::checkbox(ui, &mut o.red_eye_catchlight, "Add Catchlight");
                        }
                        hint(ui, if o.red_eye_pet { tl!("Click a pet's glowing pupil") } else { tl!("Click a red pupil") });
                    }
                    Tool::MagicWand => {
                        selection_mode_buttons(app, ui);
                        widgets::vline(ui, 22.0);
                        // local-image: Sample Size, as in Photoshop.
                        opt_label(ui, tl!("Sample Size"));
                        let sizes: [(u32, &str); 7] = [
                            (1, "Point Sample"),
                            (3, "3 by 3 Average"),
                            (5, "5 by 5 Average"),
                            (11, "11 by 11 Average"),
                            (31, "31 by 31 Average"),
                            (51, "51 by 51 Average"),
                            (101, "101 by 101 Average"),
                        ];
                        widgets::dropdown(ui, "wand-sample", &mut app.ui.tool_options.wand_sample, &sizes, 130.0);
                        opt_label(ui, tl!("Tolerance"));
                        widgets::value_field(ui, &mut app.ui.tool_options.tolerance, 0.0..=255.0, "", 58.0);
                        widgets::checkbox(ui, &mut app.ui.tool_options.anti_alias, tl!("Anti-alias"));
                        widgets::checkbox(ui, &mut app.ui.tool_options.contiguous, tl!("Contiguous"));
                        widgets::checkbox(ui, &mut app.ui.tool_options.sample_all_layers, tl!("Sample All Layers"));
                        widgets::vline(ui, 22.0);
                        if widgets::secondary_button(ui, tl!("Select Subject"), 0.0).clicked() {
                            let _ = app.run("select.subject", json!({}));
                        }
                        if widgets::secondary_button(ui, tl!("Select and Mask…"), 0.0).clicked() {
                            let _ = crate::menus::invoke(app, ui.ctx(), "select.selectAndMask", json!({}));
                        }
                    }
                    Tool::PaintBucket => {
                        opt_label(ui, tl!("Fill"));
                        let mut src = u8::from(app.ui.tool_options.bucket_fill_pattern);
                        if widgets::dropdown(ui, "bucket-src", &mut src, &[(0u8, tl!("Foreground")), (1, tl!("Pattern"))], 100.0) {
                            app.ui.tool_options.bucket_fill_pattern = src == 1;
                        }
                        opt_label(ui, tl!("Opacity"));
                        widgets::value_field(ui, &mut app.ui.tool_options.fill_opacity, 1.0..=100.0, "%", 62.0);
                        opt_label(ui, tl!("Tolerance"));
                        widgets::value_field(ui, &mut app.ui.tool_options.tolerance, 0.0..=255.0, "", 58.0);
                        widgets::checkbox(ui, &mut app.ui.tool_options.anti_alias, tl!("Anti-alias"));
                        widgets::checkbox(ui, &mut app.ui.tool_options.contiguous, tl!("Contiguous"));
                        widgets::checkbox(ui, &mut app.ui.tool_options.sample_all_layers, tl!("All Layers"));
                    }
                    Tool::Gradient => {
                        // Live "Gradient" (a Gradient Fill layer) or "Classic gradient" (pixels).
                        let mut classic = app.ui.tool_options.gradient_classic;
                        if widgets::dropdown(ui, "gradient-mode", &mut classic, &[(false, "Gradient"), (true, "Classic gradient")], 118.0) {
                            app.ui.tool_options.gradient_classic = classic;
                        }
                        let (fg, bg) = (app.session.tools.foreground, app.session.tools.background);
                        if classic {
                            gradient_swatch(ui, fg, bg);
                        } else {
                            crate::gradient_ui::preset_swatch(ui, &app.session.presets.gradient.resolve(fg, bg));
                        }
                        widgets::vline(ui, 22.0);
                        ui.spacing_mut().item_spacing.x = 2.0;
                        let before = app.ui.tool_options.clone();
                        for (style, icon, tip) in [
                            ("linear", "blend", tl!("Linear Gradient")),
                            ("radial", "circle", tl!("Radial Gradient")),
                            ("angle", "rotate-cw", tl!("Angle Gradient")),
                            ("reflected", "arrow-left-right", tl!("Reflected Gradient")),
                            ("diamond", "diamond", tl!("Diamond Gradient")),
                        ] {
                            if icons::button(ui, icon, 24.0, app.ui.tool_options.gradient_style == style, tip).clicked() {
                                app.ui.tool_options.gradient_style = style.into();
                            }
                        }
                        ui.spacing_mut().item_spacing.x = 8.0;
                        widgets::vline(ui, 22.0);
                        opt_label(ui, tl!("Mode"));
                        let opts: Vec<(BlendMode, &str)> = BlendMode::LAYER_MODES.iter().map(|m| (*m, m.label())).collect();
                        widgets::dropdown(ui, "gradient-blend-mode", &mut app.ui.tool_options.gradient_blend_mode, &opts, 96.0);
                        opt_label(ui, tl!("Opacity"));
                        widgets::value_field(ui, &mut app.ui.tool_options.fill_opacity, 1.0..=100.0, "%", 62.0);
                        widgets::checkbox(ui, &mut app.ui.tool_options.gradient_reverse, tl!("Reverse"));
                        widgets::checkbox(ui, &mut app.ui.tool_options.gradient_dither, tl!("Dither"));
                        // Live mode: the options also change a selected gradient fill layer.
                        crate::gradient_ui::options_changed(app, &before);
                    }
                    Tool::Crop => {
                        let o = &mut app.ui.tool_options;
                        let ratios: Vec<(String, &str)> = crate::chrome_ui::CROP_RATIOS.iter().map(|(k, l)| (k.to_string(), *l)).collect();
                        widgets::dropdown(ui, "crop-ratio", &mut o.crop_ratio, &ratios, 120.0);
                        // Custom ratio fields (Photoshop shows the preset's numbers here).
                        let (mut rw, mut rh) = crate::chrome_ui::crop_ratio(&o.crop_ratio, 0.0, 0.0).map_or((0.0, 0.0), |(w, h)| (w as f32, h as f32));
                        let decimals = CROP_RATIO_DECIMALS;
                        let cw = widgets::value_field_prec(ui, &mut rw, 0.0..=99_999.0, "", 54.0, decimals).changed();
                        let swap = icons::button(ui, "arrow-left-right", 22.0, false, tl!("Swaps height and width")).clicked();
                        let ch = widgets::value_field_prec(ui, &mut rh, 0.0..=99_999.0, "", 54.0, decimals).changed();
                        if swap {
                            std::mem::swap(&mut rw, &mut rh);
                        }
                        if (cw || ch || swap) && rw > 0.0 && rh > 0.0 {
                            o.crop_ratio = format!("{}:{}", widgets::fmt_num_prec(f64::from(rw), decimals), widgets::fmt_num_prec(f64::from(rh), decimals));
                        }
                        widgets::vline(ui, 22.0);
                        if widgets::secondary_button(ui, tl!("Clear"), 0.0).clicked() {
                            o.crop_ratio.clear();
                        }
                        // local-image: Straighten (the frame's tilt); 0 = a plain crop.
                        let (ir, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
                        icons::paint(ui, ir, "rotate-cw", 14.0, t.text_dim);
                        widgets::value_field(ui, &mut o.crop_angle, -45.0..=45.0, "°", 56.0)
                            .on_hover_text(tl!("Straighten: tilt the frame to level a horizon"));
                        let overlay = icons::button(ui, "grid-3x3", 24.0, o.crop_overlay != "none", tl!("Set the overlay options for the Crop tool"));
                        egui::Popup::menu(&overlay).show(|ui| {
                            ui.set_min_width(170.0);
                            for (key, label) in [
                                ("thirds", tl!("Rule of Thirds")),
                                ("grid", tl!("Grid")),
                                ("diagonal", tl!("Diagonal")),
                                ("triangle", tl!("Triangle")),
                                ("golden", tl!("Golden Ratio")),
                                ("none", tl!("None")),
                            ] {
                                if ui.add(egui::Button::selectable(o.crop_overlay == key, label)).clicked() {
                                    o.crop_overlay = key.into();
                                    ui.close();
                                }
                            }
                        });
                        widgets::checkbox(ui, &mut o.crop_delete, tl!("Delete Cropped Pixels"));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if icons::button(
                                ui,
                                "check",
                                26.0,
                                false,
                                &li_i18n::fmt(tl!("Commit current crop operation  ({key})"), &[("key", &crate::shortcuts::pretty("Enter"))]),
                            )
                            .clicked()
                            {
                                crate::canvas::commit_crop(app);
                            }
                            if icons::button(ui, "ban", 26.0, false, tl!("Cancel current crop operation  (Esc)")).clicked() {
                                app.ui.crop_rect = None;
                            }
                        });
                    }
                    Tool::PerspectiveCrop => crate::perspective_crop_ui::options_bar(app, ui),
                    Tool::Type | Tool::VerticalType => crate::type_tool::options_bar(app, ui),
                    Tool::Move => {
                        let o = &mut app.ui.tool_options;
                        widgets::checkbox(ui, &mut o.move_auto_select, tl!("Auto-Select:"));
                        widgets::dropdown(
                            ui,
                            "move-target",
                            &mut o.move_target,
                            &[("layer".to_string(), tl!("Layer")), ("group".to_string(), tl!("Group"))],
                            76.0,
                        );
                        widgets::vline(ui, 22.0);
                        widgets::checkbox(ui, &mut o.move_show_transform, tl!("Show Transform Controls"));
                        widgets::vline(ui, 22.0);
                        ui.spacing_mut().item_spacing.x = 0.0;
                        for (icon, tip, cmd) in [
                            ("panels-top-left", tl!("Align left edges"), "layer.align.leftEdges"),
                            ("app-window", tl!("Align horizontal centers"), "layer.align.horizontalCenters"),
                            ("panel-right", tl!("Align right edges"), "layer.align.rightEdges"),
                        ] {
                            let on = app.session.is_enabled(cmd);
                            if ui.add_enabled_ui(on, |ui| icons::button(ui, icon, 24.0, false, tip)).inner.clicked() {
                                let _ = app.run(cmd, json!({}));
                            }
                        }
                        ui.add_space(6.0);
                        let more = icons::button(ui, "ellipsis", 24.0, false, tl!("Align and distribute"));
                        egui::Popup::menu(&more).show(|ui| {
                            ui.set_min_width(200.0);
                            let mut section = |ui: &mut egui::Ui, title: &str, prefix: &str| {
                                ui.label(egui::RichText::new(tl!(&title)).small().color(t.text_dim));
                                for c in photocraft_engine::command_specs().iter().filter(|c| c.id.starts_with(prefix)) {
                                    let reason = (!crate::menus::is_enabled(app, c.id)).then(|| crate::command_feedback::disabled_reason(app, c.id));
                                    let shortcut = crate::shortcuts::shortcut_label(app, c.id);
                                    if widgets::command_menu_item(ui, tl!(c.label), shortcut.as_deref(), None, reason.as_deref()).clicked() {
                                        let _ = app.run(c.id, json!({}));
                                        ui.close();
                                    }
                                }
                            };
                            section(ui, tl!("Align"), "layer.align.");
                            ui.separator();
                            section(ui, tl!("Distribute"), "layer.distribute.");
                        });
                        // local-image: a selected shape or type layer's own options, bound to it.
                        if crate::context_bar::has_layer_options(app) {
                            widgets::vline(ui, 22.0);
                            crate::context_bar::layer_options(app, ui);
                        }
                    }
                    Tool::Eyedropper => hint(
                        ui,
                        &li_i18n::fmt(
                            tl!("Click to sample the foreground colour  ·  {key}-click for background"),
                            &[("key", &crate::shortcuts::pretty("Alt"))],
                        ),
                    ),
                    Tool::RotateView => crate::canvas::rotate_view::options(app, ui),
                    Tool::Zoom => {
                        widgets::checkbox(ui, &mut app.ui.tool_options.zoom_scrubby, tl!("Scrubby Zoom"));
                        hint(
                            ui,
                            &li_i18n::fmt(
                                tl!("Click to zoom in  ·  {key}-click to zoom out  ·  drag right/left to zoom in/out"),
                                &[("key", &crate::shortcuts::pretty("Alt"))],
                            ),
                        );
                        if widgets::secondary_button(ui, tl!("Fit Screen"), 0.0).clicked()
                            && let Some(i) = app.session.active_index()
                        {
                            app.ui.views[i].fit_pending = true;
                        }
                        if widgets::secondary_button(ui, "100%", 0.0).clicked()
                            && let Some(i) = app.session.active_index()
                        {
                            app.ui.views[i].zoom = 1.0;
                        }
                    }
                    Tool::Hand => hint(ui, tl!("Drag to pan  ·  hold Space with any tool")),
                    // Retouching and smart-selection tools draw their bar in `retouch_ui::options_bar`.
                    _ => {}
                }
                crate::brush_panel::commit_gesture(app, ui.ctx(), &brush_before, &brush);
                crate::brush_picker::apply(app, ui.ctx(), picked);
            });
        });
    #[cfg(test)]
    ui.ctx().data_mut(|data| data.insert_temp(egui::Id::new("theme-options-panel"), panel.response.rect));
    #[cfg(not(test))]
    let _ = panel;
}

/// Scroll the same controls in every theme. Arrow buttons remain visible at
/// narrow widths; the explicit child height prevents an unbounded centered row.
pub(super) fn tool_options_scroll(ui: &mut egui::Ui, app: &mut PhotocraftApp, controls: impl FnOnce(&mut PhotocraftApp, &mut egui::Ui)) {
    let height = ui.available_height();
    ui.spacing_mut().scroll.floating = true;
    ui.spacing_mut().scroll.floating_allocated_width = 0.0;
    ui.horizontal_centered(|ui| {
        crate::toolsets_ui::persona::switcher(app, ui);
        widgets::vline(ui, 22.0);
        let tool = app.ui.tool;
        let salt = ("tool-options-scroll", tool);
        let id = ui.make_persistent_id(salt);
        let previous_offset = egui::scroll_area::State::load(ui.ctx(), id).map_or(0.0, |state| state.offset.x);
        let previous =
            ui.add_enabled_ui(previous_offset > 0.0, |ui| icons::button(ui, "chevron-left", 24.0, false, tl!("Previous tool options"))).inner.clicked();
        let width = (ui.available_width() - 24.0 - ui.spacing().item_spacing.x).max(1.0);
        // ScrollArea snaps its content origin to physical pixels. A small content gutter
        // keeps both end controls fully inside the clip after fractional-scale rounding.
        let mut scroll = egui::ScrollArea::horizontal()
            .id_salt(salt)
            .max_width(width)
            .max_height(height)
            .content_margin(egui::Margin::symmetric(2, 0))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_height(height);
                // Native crop/type controls place their commit actions at the row end.
                // A finite virtual width prevents right-to-left layouts using infinity.
                ui.set_max_width(width.max(1200.0));
                ui.horizontal_centered(|ui| controls(app, ui));
            });
        #[cfg(test)]
        {
            let clip = scroll.inner_rect.intersect(ui.clip_rect());
            ui.ctx().data_mut(|data| data.insert_temp(egui::Id::new(("theme-options-clip", tool)), clip));
        }
        let limit = (scroll.content_size.x - scroll.inner_rect.width()).max(0.0);
        let next =
            ui.add_enabled_ui(scroll.state.offset.x < limit, |ui| icons::button(ui, "chevron-right", 24.0, false, tl!("More tool options"))).inner.clicked();
        if previous || next {
            scroll.state.offset.x = (scroll.state.offset.x + if previous { -width * 0.75 } else { width * 0.75 }).clamp(0.0, limit);
            scroll.state.store(ui.ctx(), scroll.id);
            ui.ctx().request_repaint();
        }
    });
}

pub(super) fn label(ui: &mut egui::Ui, s: &str) {
    let t = Tokens::get(ui.ctx());
    ui.label(RichText::new(s).color(t.text_dim));
}

/// Width of `s` as [`label`] draws it (body text).
pub(super) fn body_text_width(ui: &egui::Ui, s: &str) -> f32 {
    let font = egui::TextStyle::Body.resolve(ui.style());
    ui.painter().layout_no_wrap(s.to_string(), font, Color32::WHITE).size().x
}

/// Options-bar label: Photoshop writes "Size:" with a colon.
pub(super) fn opt_label(ui: &mut egui::Ui, s: &str) {
    let t = Tokens::get(ui.ctx());
    let text = if t.pro && !s.ends_with(':') { format!("{s}:") } else { s.to_string() };
    ui.label(RichText::new(tl!(&text)).color(t.text_dim));
}

pub(super) fn percent_field(ui: &mut egui::Ui, label: &str, value: &mut f32, range: std::ops::RangeInclusive<f32>, width: f32) {
    opt_label(ui, label);
    let mut percent = *value * 100.0;
    if widgets::value_field(ui, &mut percent, range, "%", width).changed() {
        *value = percent / 100.0;
    }
}

pub(super) fn hint(ui: &mut egui::Ui, s: &str) {
    let t = Tokens::get(ui.ctx());
    ui.label(RichText::new(s).color(t.text_faint));
}

// Status bar

pub fn status_bar(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    // A caller may forward a raw error returned by `run`; normalize at the last report
    // sink too, retaining its original detail through the same bounded notice history.
    if app.ui.status_error && crate::command_feedback::message(&app.ui.status) != app.ui.status {
        crate::command_feedback::report(app, app.ui.status.clone());
    }
    let t = Tokens::get(ui.ctx());
    egui::Panel::bottom("status_bar")
        .exact_size(if t.pro { 24.0 } else { 30.0 })
        .frame(egui::Frame::NONE.fill(t.chrome).inner_margin(egui::Margin::symmetric(10, 0)))
        .show(ui, |ui| {
            let r = ui.max_rect();
            ui.painter().line_segment([r.left_top(), r.right_top()], Stroke::new(1.0, t.separator));
            ui.horizontal_centered(|ui| {
                if crate::notices::status_history_only(app) {
                    crate::notices::history_button(app, ui);
                }
                if t.pro {
                    if app.session.active().is_none() && !app.ui.status.is_empty() {
                        ui.add(
                            egui::Label::new(egui::RichText::new(&app.ui.status).color(if app.ui.status_error { t.warning } else { t.text_dim })).truncate(),
                        );
                    } else {
                        crate::chrome_ui::status_bar_pro(app, ui);
                    }
                    // Background job progress with Cancel, at the right end (#210).
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // local-image: the AI engine status pill.
                        crate::ai_ui::status_pill(app, ui);
                        // CPU mode after a GPU failure: why, and Retry GPU.
                        crate::gpu_status::status_pill(app, ui);
                        // Stop generation and unload models, left of the GPU stats.
                        crate::ai_ui::status_controls(app, ui);
                        crate::jobs_ui::status_progress(app, ui)
                    });
                    return;
                }
                if let (Some(st), Some(i)) = (app.session.active(), app.session.active_index()) {
                    let (w, h, mode, bits, layers) =
                        (st.doc.size.width, st.doc.size.height, crate::canvas::mode_label(&st.doc), st.doc.depth.bits(), st.doc.layer_count());
                    let mut pct = app.ui.views[i].zoom * 100.0;
                    if widgets::value_field(ui, &mut pct, 1.0..=3200.0, "%", 78.0).changed() {
                        app.ui.views[i].zoom = pct / 100.0;
                        app.ui.views[i].fit_pending = false;
                    }
                    widgets::vline(ui, 16.0);
                    label(ui, &li_i18n::fmt(tl!("{mode} Color · {bits} bit"), &[("mode", mode), ("bits", &bits.to_string())]));
                    widgets::vline(ui, 16.0);
                    label(ui, &format!("{w} × {h} px"));
                    widgets::vline(ui, 16.0);
                    label(ui, &li_i18n::trn(li_i18n::current(), layers as u64, "{n} layer", "{n} layers"));
                } else {
                    label(ui, tl!("No document"));
                }
                if !app.ui.status.is_empty() {
                    widgets::vline(ui, 16.0);
                    let is_err = app.ui.status_error || app.ui.status.starts_with("Couldn");
                    ui.label(RichText::new(&app.ui.status).color(if is_err { t.warning } else { t.text_faint }));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    crate::gpu_status::status_pill(app, ui);
                    if app.session.active().is_some()
                        && !t.pro
                        && widgets::secondary_button(ui, tl!("Fit"), 0.0).clicked()
                        && let Some(i) = app.session.active_index()
                    {
                        app.ui.views[i].fit_pending = true;
                    }
                    if app.session.has_jobs() {
                        ui.add_space(6.0);
                        crate::jobs_ui::status_progress(app, ui);
                    }
                });
            });
        });
}

/// Photoshop's brush preset picker chip: a soft/hard round tip preview with the size underneath.
/// Draw a round brush tip preview (hard core fading to a soft edge).
pub(super) fn brush_tip(p: &egui::Painter, c: egui::Pos2, rad: f32, hardness: f32, color: Color32) {
    let inner = rad * hardness.clamp(0.05, 1.0);
    let steps = 8;
    for k in (0..=steps).rev() {
        let f = k as f32 / steps as f32;
        let rr = inner + (rad - inner) * f;
        let a = if hardness >= 0.99 { 1.0 } else { (1.0 - f).powf(1.5) };
        p.circle_filled(c, rr, color.gamma_multiply(a));
    }
}

/// Options-bar Smoothing % (the brush's stroke smoothing; the live stroke and the commit use it).
/// The cog beside Smoothing: Photoshop's smoothing options.
pub(super) fn smoothing_options(ui: &mut egui::Ui, b: &mut photocraft_engine::BrushSettings) {
    let resp = icons::button(ui, "settings", 24.0, false, tl!("Set additional smoothing options"));
    egui::Popup::from_toggle_button_response(&resp).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
        ui.set_min_width(220.0);
        let sm = &mut b.smoothing;
        widgets::checkbox(ui, &mut sm.pulled_string, tl!("Pulled String Mode")).on_hover_text(tl!("Paint only when the string is pulled taut"));
        widgets::checkbox(ui, &mut sm.catch_up, tl!("Stroke Catch-up")).on_hover_text(tl!("Keep painting toward the pointer while it pauses"));
        widgets::checkbox(ui, &mut sm.catch_up_on_end, tl!("Catch-up on Stroke End")).on_hover_text(tl!("Finish the stroke where the pointer was released"));
        widgets::checkbox(ui, &mut sm.adjust_for_zoom, tl!("Adjust for Zoom")).on_hover_text(tl!("Less smoothing when zoomed in, more when zoomed out"));
    });
}

/// The butterfly menu: symmetry presets and the path option. Returns the command to run.
pub(super) fn symmetry_menu(ui: &mut egui::Ui, current: Option<&str>) -> Option<(&'static str, serde_json::Value)> {
    let resp = icons::button(ui, "arrow-left-right", 24.0, current.is_some(), tl!("Set painting symmetry options"));
    let mut out = None;
    egui::Popup::menu(&resp).show(|ui| {
        ui.set_min_width(200.0);
        for (label, kind) in [(tl!("Vertical"), "vertical"), (tl!("Horizontal"), "horizontal"), (tl!("Diagonal"), "diagonal")] {
            if ui.add(egui::Button::selectable(current == Some(kind), label)).clicked() {
                out = Some(("paint.symmetry", serde_json::json!({ "type": kind })));
                ui.close();
            }
        }
        let from_path = current.is_some_and(|c| !matches!(c, "vertical" | "horizontal" | "diagonal"));
        if ui
            .add(egui::Button::selectable(from_path, tl!("Along the Work Path")))
            .on_hover_text(tl!("Mirror across the current path (draw it with the Pen tool first)"))
            .clicked()
        {
            out = Some(("paint.symmetryFromPath", serde_json::json!({ "name": "work" })));
            ui.close();
        }
        ui.separator();
        if ui.add_enabled(current.is_some(), egui::Button::new(tl!("Symmetry Off"))).clicked() {
            out = Some(("paint.symmetry", serde_json::json!({ "type": "off" })));
            ui.close();
        }
    });
    out
}

pub(super) fn smoothing_field(ui: &mut egui::Ui, b: &mut photocraft_engine::BrushSettings, width: f32) {
    let mut sm = (b.smoothing.amount * 100.0).round();
    if widgets::value_field(ui, &mut sm, 0.0..=100.0, "%", width).changed() {
        b.smoothing.amount = (sm / 100.0).clamp(0.0, 1.0);
    }
}

/// Options-bar brush chip; opens Photoshop's Brush Preset picker (size, hardness, the preset
/// library). Returns what the picker asked for beyond the size and hardness edits in `b`.
pub(super) fn brush_preset_chip(
    ui: &mut egui::Ui,
    b: &mut photocraft_engine::BrushSettings,
    presets: &[photocraft_engine::paint::BrushPreset],
) -> Option<crate::brush_picker::Pick> {
    let t = Tokens::get(ui.ctx());
    let (r, resp) = ui.allocate_exact_size(vec2(44.0, 30.0), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(r, t.radius_sm, t.hover);
    }
    let c = pos2(r.left() + 14.0, r.top() + 11.0);
    brush_tip(ui.painter(), c, 7.0, b.hardness, Color32::WHITE);
    ui.painter().text(pos2(c.x, r.bottom() - 5.0), Align2::CENTER_CENTER, format!("{}", b.size.round() as i64), egui::FontId::proportional(9.5), t.text_dim);
    icons::paint(ui, Rect::from_center_size(pos2(r.right() - 9.0, c.y), vec2(10.0, 10.0)), "chevron-down", 9.0, t.text_faint);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tl!("Brush Preset picker")));
    let resp = resp.on_hover_text(tl!("Brush Preset picker"));
    egui::Popup::from_toggle_button_response(&resp)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| crate::brush_picker::body(ui, b, presets))
        .and_then(|r| r.inner)
}

/// New / add / subtract / intersect selection buttons (shared by selection tools).
pub(super) fn selection_mode_buttons(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    ui.spacing_mut().item_spacing.x = 2.0;
    for (i, (icon, tip)) in [
        ("square", tl!("New selection").to_string()),
        ("plus", li_i18n::fmt(tl!("Add to selection  ({key})"), &[("key", &crate::shortcuts::pretty("Shift"))])),
        ("minus", li_i18n::fmt(tl!("Subtract from selection  ({key})"), &[("key", &crate::shortcuts::pretty("Alt"))])),
        ("squares-subtract", li_i18n::fmt(tl!("Intersect with selection  ({key})"), &[("key", &crate::shortcuts::pretty("Shift+Alt"))])),
    ]
    .iter()
    .enumerate()
    {
        if icons::button(ui, icon, 24.0, app.ui.selection_mode == i as u8, tip).clicked() {
            app.ui.selection_mode = i as u8;
        }
    }
    ui.spacing_mut().item_spacing.x = 8.0;
}

/// Gradient picker swatch (foreground → background), Photoshop options-bar style.
pub(super) fn gradient_swatch(ui: &mut egui::Ui, a: [f32; 4], b: [f32; 4]) {
    let t = Tokens::get(ui.ctx());
    let (r, resp) = ui.allocate_exact_size(vec2(96.0, 20.0), Sense::click());
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(r.left_top(), c32(a));
    mesh.colored_vertex(r.right_top(), c32(b));
    mesh.colored_vertex(r.right_bottom(), c32(b));
    mesh.colored_vertex(r.left_bottom(), c32(a));
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    ui.painter().add(mesh);
    ui.painter().rect_stroke(r, 0.0, Stroke::new(1.0, t.field_border), StrokeKind::Outside);
    let _ = resp.on_hover_text(tl!("Click to edit the gradient"));
}
