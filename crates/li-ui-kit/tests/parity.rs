#![recursion_limit = "256"]
//! Frozen earlier values/styles/Latin shaping, captured from reference modules.
use egui::{Color32, FontData, FontDefinitions, FontFamily};
use li_ui_kit::{fonts, library, theme};
fn hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |h, b| (h ^ u64::from(*b)).wrapping_mul(0x100000001b3))
}
fn editor_snapshot(t: li_ui_kit::theme::Tokens) -> serde_json::Value {
    serde_json::json!({"kind": format!("{:?}", t.kind),
        "chrome": format!("{:?}", t.chrome),
        "canvas": format!("{:?}", t.canvas),
        "canvas_dot": format!("{:?}", t.canvas_dot),
        "dock": format!("{:?}", t.dock),
        "card": format!("{:?}", t.card),
        "card_border": format!("{:?}", t.card_border),
        "field": format!("{:?}", t.field),
        "field_border": format!("{:?}", t.field_border),
        "hover": format!("{:?}", t.hover),
        "pressed": format!("{:?}", t.pressed),
        "text": format!("{:?}", t.text),
        "text_dim": format!("{:?}", t.text_dim),
        "text_faint": format!("{:?}", t.text_faint),
        "icon": format!("{:?}", t.icon),
        "accent": format!("{:?}", t.accent),
        "accent_soft": format!("{:?}", t.accent_soft),
        "accent_border": format!("{:?}", t.accent_border),
        "accent_text": format!("{:?}", t.accent_text),
        "separator": format!("{:?}", t.separator),
        "shadow": format!("{:?}", t.shadow),
        "primary_bg": format!("{:?}", t.primary_bg),
        "primary_text": format!("{:?}", t.primary_text),
        "danger": format!("{:?}", t.danger),
        "warning": format!("{:?}", t.warning),
        "radius_sm": format!("{:?}", t.radius_sm),
        "radius": format!("{:?}", t.radius),
        "radius_lg": format!("{:?}", t.radius_lg),
        "bevel": format!("{:?}", t.bevel),
        "pro": format!("{:?}", t.pro),
        "tab_strip": format!("{:?}", t.tab_strip),
        "row_selected": format!("{:?}", t.row_selected),
        "histogram_bg": format!("{:?}", t.histogram_bg),
        "histogram_level": format!("{:?}", t.histogram_level),
        "caption_close": format!("{:?}", t.caption_close),
        "caption_close_text": format!("{:?}", t.caption_close_text)})
}
fn library_snapshot(t: li_ui_kit::theme::Tokens) -> serde_json::Value {
    serde_json::json!({"dark": format!("{:?}", t.library.dark),
        "selection": format!("{:?}", t.library.selection),
        "selection_text": format!("{:?}", t.library.selection_text),
        "chrome": format!("{:?}", t.chrome),
        "canvas": format!("{:?}", t.canvas),
        "grid_bg": format!("{:?}", t.library.grid_bg),
        "cell": format!("{:?}", t.library.cell),
        "cell_selected": format!("{:?}", t.library.cell_selected),
        "divider": format!("{:?}", t.library.divider),
        "inset": format!("{:?}", t.library.inset),
        "field": format!("{:?}", t.field),
        "field_border": format!("{:?}", t.field_border),
        "button": format!("{:?}", t.library.button),
        "button_border": format!("{:?}", t.library.button_border),
        "hover": format!("{:?}", t.hover),
        "pressed": format!("{:?}", t.pressed),
        "tool_active": format!("{:?}", t.library.tool_active),
        "text": format!("{:?}", t.text),
        "text_label": format!("{:?}", t.library.text_label),
        "text_dim": format!("{:?}", t.text_dim),
        "text_disabled": format!("{:?}", t.library.text_disabled),
        "icon": format!("{:?}", t.icon),
        "track": format!("{:?}", t.library.track),
        "thumb": format!("{:?}", t.library.thumb),
        "thumb_hover": format!("{:?}", t.library.thumb_hover),
        "accent": format!("{:?}", t.accent),
        "star": format!("{:?}", t.library.star),
        "pick": format!("{:?}", t.library.pick),
        "reject": format!("{:?}", t.library.reject),
        "caution": format!("{:?}", t.library.caution),
        "mask_overlay": format!("{:?}", t.library.mask_overlay),
        "overlay_bg": format!("{:?}", t.library.overlay_bg),
        "overlay_fg": format!("{:?}", t.library.overlay_fg),
        "overlay_reject": format!("{:?}", t.library.overlay_reject),
        "top_bar_h": format!("{:?}", t.library.metrics.top_bar_h),
        "bottom_bar_h": format!("{:?}", t.library.metrics.bottom_bar_h),
        "panel_w": format!("{:?}", t.library.metrics.panel_w),
        "strip_w": format!("{:?}", t.library.metrics.strip_w),
        "slider_row_h": format!("{:?}", t.library.metrics.slider_row_h),
        "section_h": format!("{:?}", t.library.metrics.section_h),
        "film_h": format!("{:?}", t.library.metrics.film_h),
        "panel_inset_left": format!("{:?}", t.library.metrics.panel_inset_left),
        "panel_inset_right": format!("{:?}", t.library.metrics.panel_inset_right)})
}
fn font_snapshot(definitions: FontDefinitions) -> serde_json::Value {
    let families: std::collections::BTreeMap<_, _> = definitions.families.iter().map(|(name, stack)| (format!("{name:?}"), stack.clone())).collect();
    let faces: std::collections::BTreeMap<_, _> =
        definitions.font_data.iter().map(|(name, face)| (name.clone(), (hash(face.font.as_ref()), format!("{:?}", face.tweak)))).collect();
    let ctx = egui::Context::default();
    ctx.set_fonts(definitions);
    let mut sizes = Vec::new();
    for _ in 0..2 {
        ctx.run_ui(Default::default(), |ui| {
            sizes.clear();
            for font in [egui::FontId::proportional(11.5), egui::FontId::new(12.5, FontFamily::Name("semibold".into())), egui::FontId::monospace(12.0)] {
                let s = ui.painter().layout_no_wrap("Interface ▸ 123 café".into(), font, Color32::WHITE).size();
                sizes.push([s.x, s.y]);
            }
        })
        .textures_delta
        .clear();
    }
    serde_json::json!({"families":families,
        "faces":faces,
        "sizes":sizes})
}

#[test]
fn all_theme_and_font_goldens_match_previous_backends() {
    let golden: serde_json::Value = serde_json::from_str(include_str!("golden/previous_backends.json")).unwrap();
    for kind in theme::ThemeKind::ALL {
        let ctx = egui::Context::default();
        theme::apply(&ctx, kind);
        assert_eq!(editor_snapshot(theme::Tokens::for_kind(kind)), golden[format!("editor/{kind:?}")]["tokens"]);
        assert_eq!(hash(format!("{:?}", ctx.global_style()).as_bytes()), golden[format!("editor/{kind:?}")]["style"].as_u64().unwrap());
        let ctx = egui::Context::default();
        let t = library::for_kind(kind);
        library::apply_tokens(&ctx, t);
        assert_eq!(library_snapshot(t), golden[format!("library/{kind:?}")]["tokens"]);
        assert_eq!(hash(format!("{:?}", ctx.global_style()).as_bytes()), golden[format!("library/{kind:?}")]["style"].as_u64().unwrap());
    }
    let ctx = egui::Context::default();
    library::apply(&ctx);
    assert_eq!(library_snapshot(library::standalone()), golden["library/standalone"]["tokens"]);
    assert_eq!(hash(format!("{:?}", ctx.global_style()).as_bytes()), golden["library/standalone"]["style"].as_u64().unwrap());
    for (key, profile) in [("fonts/editor", fonts::Profile::Editor), ("fonts/library", fonts::Profile::Library)] {
        assert_eq!(font_snapshot(fonts::definitions(profile)), golden[key]);
    }
}

fn previous_raster(bytes: &[u8], pixels: u32, interface: bool, colour: Color32, disabled: bool) -> egui::ColorImage {
    // Frozen algorithms from the earlier interface/tool/colour implementations.
    let hex = format!("#{:02x}{:02x}{:02x}", colour.r(), colour.g(), colour.b());
    let svg = String::from_utf8_lossy(bytes).replace("currentColor", &hex);
    let svg = if interface { svg.replace("stroke-width=\"2\"", "stroke-width=\"1.75\"") } else { svg };
    let mut image = egui_extras::image::load_svg_bytes_with_size(svg.as_bytes(), egui::load::SizeHint::Width(pixels), &Default::default()).unwrap();
    if disabled {
        for p in &mut image.pixels {
            let [r, g, b, a] = p.to_srgba_unmultiplied();
            let grey = ((u32::from(r) * 54 + u32::from(g) * 183 + u32::from(b) * 19) / 256) as u8;
            *p = Color32::from_rgba_unmultiplied(grey, grey, grey, (u16::from(a) * 45 / 100) as u8);
        }
    }
    image
}
#[test]
fn svg_pixels_match_every_replaced_icon_profile() {
    use li_ui_kit::{color_icons, icon_data, icons, library_icons, tool_icon_data, vector_icon_data};
    let interfaces = icon_data::ICONS.iter().copied().chain(library_icons::ALL.iter().map(|icon| ("library", library_icons::svg(*icon))));
    let tools = tool_icon_data::TOOL_ICONS.iter().copied().chain(vector_icon_data::ICONS.iter().map(|(_, name, bytes)| (*name, *bytes)));
    let mut cases = 0;
    for (interface, entries) in [(true, interfaces.collect::<Vec<_>>()), (false, tools.collect::<Vec<_>>())] {
        for (name, bytes) in entries {
            for pixels in [16, 20, 25, 32, 40, 50, 80] {
                let style = if interface { icons::StrokeStyle::Interface } else { icons::StrokeStyle::Tool };
                let expected = previous_raster(bytes, pixels, interface, Color32::WHITE, false);
                let actual = icons::raster(bytes, style, pixels, Color32::WHITE, false).unwrap();
                assert_eq!(actual, expected, "{name}/{pixels}/{style:?}");
                cases += 1;
                if !interface && color_icons::exists(name) {
                    for (icon, dark, disabled) in
                        [(Color32::from_gray(222), true, false), (Color32::from_gray(52), false, false), (Color32::from_gray(222), true, true)]
                    {
                        let colour = color_icons::tint(name, icon, dark);
                        assert_eq!(
                            icons::raster(bytes, style, pixels, colour, disabled).unwrap(),
                            previous_raster(bytes, pixels, false, colour, disabled),
                            "colour {name}/{pixels}/{dark}/{disabled}"
                        );
                        cases += 1;
                    }
                }
            }
        }
    }
    println!("pixel parity: {cases} rasters, maximum RGBA difference 0; physical sizes 16/20/25/32/40/50/80");
}
#[test]
fn font_faces_share_original_storage_and_scaling_survives_install() {
    for profile in [fonts::Profile::Editor, fonts::Profile::Library] {
        let definitions = fonts::definitions(profile);
        for (name, bytes) in [
            ("Inter", photocraft_text::fonts::INTER_REGULAR),
            ("Inter-SemiBold", photocraft_text::fonts::INTER_SEMIBOLD),
            ("JetBrainsMono", photocraft_text::fonts::JETBRAINS_MONO_REGULAR),
        ] {
            assert!(std::ptr::eq(definitions.font_data[name].font.as_ref().as_ptr(), bytes.as_ptr()));
        }
    }
    // Saved size is loaded in the first UI pass, before set_fonts has taken effect.
    let ctx = egui::Context::default();
    ctx.run_ui(Default::default(), |ui| {
        fonts::install_editor(ui.ctx());
        fonts::set_ui_font_size(ui.ctx(), fonts::FontSize::Large);
    })
    .textures_delta
    .clear();
    let mut original = 0.0;
    ctx.run_ui(Default::default(), |ui| {
        original = ui.painter().layout_no_wrap("Interface 123".into(), fonts::medium(12.0), Color32::WHITE).size().x;
    })
    .textures_delta
    .clear();
    ctx.run_ui(Default::default(), |_| {}).textures_delta.clear();
    let mut enlarged = 0.0;
    ctx.run_ui(Default::default(), |ui| {
        enlarged = ui.painter().layout_no_wrap("Interface 123".into(), fonts::medium(12.0), Color32::WHITE).size().x;
    })
    .textures_delta
    .clear();
    assert!((enlarged - original * 4.0 / 3.0).abs() < 1.0);
    fonts::install_editor(&ctx);
    for _ in 0..2 {
        ctx.run_ui(Default::default(), |ui| {
            let width = ui.painter().layout_no_wrap("Interface 123".into(), fonts::medium(12.0), Color32::WHITE).size().x;
            assert!((width - enlarged).abs() < 1.0);
        })
        .textures_delta
        .clear();
    }
    let mut lazy = FontData::from_static(photocraft_text::fonts::INTER_REGULAR);
    lazy.tweak.scale = 0.9;
    fonts::size_ui_font(&ctx, "lazy-fallback", &mut lazy);
    assert!((lazy.tweak.scale - 1.2).abs() < 1e-6);
}
#[test]
fn theme_serialization_keeps_saved_names_and_defaults() {
    for kind in theme::ThemeKind::ALL {
        let name = format!("\"{kind:?}\"");
        assert_eq!(serde_json::to_string(&kind).unwrap(), name);
        assert_eq!(serde_json::from_str::<theme::ThemeKind>(&name).unwrap(), kind);
    }
    assert_eq!(theme::ThemeKind::default(), theme::ThemeKind::ProMedium);
}

#[test]
fn registry_names_keep_previous_lookup_priority_and_white_geometry() {
    use li_ui_kit::{icon_data, icons, mono_tool_icons, tool_icon_data, vector_icon_data};
    let ctx = egui::Context::default();
    let actual_bytes = |image: egui::Image<'static>| match image.source(&ctx) {
        egui::ImageSource::Bytes { bytes, .. } => bytes.as_ref().to_vec(),
        source => panic!("unexpected source: {source:?}"),
    };
    for &(name, bytes) in icon_data::ICONS {
        let expected = String::from_utf8_lossy(bytes).replace("currentColor", "#ffffff").replace("stroke-width=\"2\"", "stroke-width=\"1.75\"");
        assert_eq!(actual_bytes(icons::image(name, 20.0, Color32::WHITE)), expected.as_bytes(), "generic {name}");
    }
    // The old svg lookup selected the first row, while its white-image map used the last.
    // Checking both catches a future conflicting alias instead of silently changing its geometry.
    let previous = tool_icon_data::TOOL_ICONS.iter().copied().chain(vector_icon_data::ICONS.iter().map(|(_, name, bytes)| (*name, *bytes))).collect::<Vec<_>>();
    let last: std::collections::HashMap<_, _> = previous.iter().copied().collect();
    for (name, bytes) in &last {
        let first = previous.iter().find(|(id, _)| id == name).unwrap().1;
        assert_eq!(mono_tool_icons::svg(name).unwrap(), first, "SVG {name}");
        let expected = String::from_utf8_lossy(bytes).replace("currentColor", "#ffffff");
        assert_eq!(actual_bytes(mono_tool_icons::image(name, 20.0, Color32::WHITE).unwrap()), expected.as_bytes(), "tool {name}");
    }
}
