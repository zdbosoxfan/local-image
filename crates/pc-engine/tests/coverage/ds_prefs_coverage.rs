use photocraft_engine::prefs;
use serde_json::{Value, from_value, json};

#[test]
fn units_pixels_identity() {
    let px = 123.0;
    assert_eq!(prefs::Unit::Pixels.from_px(px, 0.0, 0.0, 0.0), px);
    assert_eq!(prefs::Unit::Pixels.to_px(px, 0.0, 0.0, 0.0), px);
}

#[test]
fn units_length_conversions() {
    let dpi = 300.0;
    let px = 600.0;
    let ppi = 72.0;
    assert!((prefs::Unit::Inches.from_px(px, dpi, 1000.0, ppi) - 2.0).abs() < 1e-9);
    assert!((prefs::Unit::Centimeters.from_px(px, dpi, 1000.0, ppi) - 5.08).abs() < 1e-9);
    assert!((prefs::Unit::Millimeters.from_px(px, dpi, 1000.0, ppi) - 50.8).abs() < 1e-9);
    assert!((prefs::Unit::Points.from_px(px, dpi, 1000.0, ppi) - 144.0).abs() < 1e-9);
    assert!((prefs::Unit::Picas.from_px(px, dpi, 1000.0, ppi) - 12.0).abs() < 1e-9);
}

#[test]
fn units_percent_uses_extent() {
    let ppi = 72.0;
    assert!((prefs::Unit::Percent.from_px(50.0, 72.0, 200.0, ppi) - 25.0).abs() < 1e-9);
    // extent zero clamps to 1e-6 in from_px
    let v = prefs::Unit::Percent.from_px(50.0, 72.0, 0.0, ppi);
    assert!(v.is_finite());
    assert!(v > 1e9);
    // to_px does not clamp extent; 25% of 0 is 0
    assert_eq!(prefs::Unit::Percent.to_px(25.0, 72.0, 0.0, ppi), 0.0);
}

#[test]
fn units_guard_against_zero_and_nan_dpi() {
    let expected = 1.0 / 1e-6;
    let v = prefs::Unit::Inches.from_px(1.0, 0.0, 100.0, 72.0);
    assert!((v - expected).abs() < 1e-9);
    // f64::max ignores NaN and returns the other operand
    let v = prefs::Unit::Inches.from_px(1.0, f64::NAN, 100.0, 72.0);
    assert!((v - expected).abs() < 1e-9);
    assert!(v.is_finite());
}

#[test]
fn unit_suffix_and_decimals() {
    assert_eq!(prefs::Unit::Pixels.suffix(), "px");
    assert_eq!(prefs::Unit::Inches.suffix(), "in");
    assert_eq!(prefs::Unit::Centimeters.suffix(), "cm");
    assert_eq!(prefs::Unit::Millimeters.suffix(), "mm");
    assert_eq!(prefs::Unit::Points.suffix(), "pt");
    assert_eq!(prefs::Unit::Picas.suffix(), "pica");
    assert_eq!(prefs::Unit::Percent.suffix(), "%");

    assert_eq!(prefs::Unit::Pixels.decimals(), 0);
    assert_eq!(prefs::Unit::Inches.decimals(), 2);
    assert_eq!(prefs::Unit::Centimeters.decimals(), 2);
    assert_eq!(prefs::Unit::Millimeters.decimals(), 1);
    assert_eq!(prefs::Unit::Points.decimals(), 1);
    assert_eq!(prefs::Unit::Picas.decimals(), 2);
    assert_eq!(prefs::Unit::Percent.decimals(), 1);
}

#[test]
fn point_size_per_inch_values() {
    assert_eq!(prefs::PointSize::PostScript.per_inch(), 72.0);
    assert_eq!(prefs::PointSize::Traditional.per_inch(), 72.27);
}

#[test]
fn transparency_square_sizes() {
    let mut t = prefs::TransparencyAndGamut::default();
    assert_eq!(t.square(), Some(8.0));
    t.grid_size = prefs::CheckerSize::None;
    assert_eq!(t.square(), None);
    t.grid_size = prefs::CheckerSize::Small;
    assert_eq!(t.square(), Some(4.0));
    t.grid_size = prefs::CheckerSize::Large;
    assert_eq!(t.square(), Some(16.0));
}

#[test]
fn transparency_colors_builtin_and_custom() {
    let mut t = prefs::TransparencyAndGamut::default();
    assert_eq!(t.colors(), [[255, 255, 255], [204, 204, 204]]);
    t.grid_colors = prefs::CheckerColors::Medium;
    assert_eq!(t.colors(), [[153, 153, 153], [102, 102, 102]]);
    t.grid_colors = prefs::CheckerColors::Custom;
    t.custom_light = "#112233".into();
    t.custom_dark = "#445566".into();
    assert_eq!(t.colors(), [[0x11, 0x22, 0x33], [0x44, 0x55, 0x66]]);
    t.custom_light = "not-a-color".into();
    t.custom_dark = "#zzzzzz".into();
    assert_eq!(t.colors(), [[255, 255, 255], [204, 204, 204]]);
}

#[test]
fn parse_hex_accepts_variants_and_rejects_bad() {
    assert_eq!(prefs::parse_hex("#ff0000"), Some([255, 0, 0]));
    assert_eq!(prefs::parse_hex("  #00aabb  "), Some([0, 170, 187]));
    assert_eq!(prefs::parse_hex("#12345"), None);
    assert_eq!(prefs::parse_hex("#12345g"), None);
    assert_eq!(prefs::parse_hex(""), None);
    assert_eq!(prefs::parse_hex("#"), None);
}

#[test]
fn guides_major_px_clamps_and_converts() {
    let mut g = prefs::GuidesGridAndSlices::default();
    assert!((g.major_px(72.0, 1000.0, 72.0) - 72.0).abs() < 1e-9);
    g.grid_unit = prefs::Unit::Pixels;
    g.gridline_every = 10.0;
    assert!((g.major_px(72.0, 1000.0, 72.0) - 10.0).abs() < 1e-9);
    g.gridline_every = 0.0;
    assert!((g.major_px(72.0, 1000.0, 72.0) - 1e-3).abs() < 1e-9);
    let v = g.major_px(1e9, 1000.0, 72.0);
    assert!(v.is_finite());
}

#[test]
fn performance_rendering_mode_resolution() {
    let mut p = prefs::Performance::default();
    assert_eq!(p.effective_rendering_mode(), prefs::RenderingMode::Auto);
    p.rendering_mode = Some(prefs::RenderingMode::Cpu);
    assert_eq!(p.effective_rendering_mode(), prefs::RenderingMode::Cpu);
    p.rendering_mode = None;
    p.use_gpu = false;
    assert_eq!(p.effective_rendering_mode(), prefs::RenderingMode::Cpu);
    p.use_gpu = true;
    p.gpu_backend = prefs::GpuBackend::Cpu;
    assert_eq!(p.effective_rendering_mode(), prefs::RenderingMode::Cpu);
    p.gpu_backend = prefs::GpuBackend::Auto;
    assert_eq!(p.effective_rendering_mode(), prefs::RenderingMode::Auto);
}

#[test]
fn performance_history_budget_bytes() {
    let mut p = prefs::Performance::default();
    assert_eq!(p.history_budget_bytes(), 8192usize * (1 << 20));
    p.memory_usage_mb = 0;
    assert_eq!(p.history_budget_bytes(), 0);
    p.memory_usage_mb = u32::MAX;
    assert_eq!(p.history_budget_bytes(), (u32::MAX as usize).saturating_mul(1 << 20));
}

#[test]
fn preferences_get_set_roundtrip() {
    let mut p = prefs::Preferences::default();
    p.set("general.beepWhenDone", json!(true)).unwrap();
    assert_eq!(p.get("general.beepWhenDone"), Some(json!(true)));
    assert!(p.general.beep_when_done);
}

#[test]
fn preferences_set_unknown_path_errors() {
    let mut p = prefs::Preferences::default();
    assert!(p.set("no.such.key", json!(1)).is_err());
}

#[test]
fn preferences_set_enum_validation() {
    let mut p = prefs::Preferences::default();
    assert!(p.set("general.colorPicker", json!("invalid")).is_err());
    p.set("general.colorPicker", json!("adobe")).unwrap();
    assert_eq!(p.general.color_picker, prefs::ColorPicker::Adobe);
}

#[test]
fn preferences_set_range_validation() {
    let mut p = prefs::Preferences::default();
    assert!(p.set("export.jpegQuality", json!(0)).is_err());
    assert!(p.set("export.jpegQuality", json!(101)).is_err());
    p.set("export.jpegQuality", json!(1)).unwrap();
    p.set("export.jpegQuality", json!(100)).unwrap();
    assert_eq!(p.export.jpeg_quality, 100);
    assert!(p.set("export.jpegQuality", json!("85")).is_err());
}

#[test]
fn preferences_set_color_validation() {
    let mut p = prefs::Preferences::default();
    assert!(p.set("interface.canvasCustomColor", json!("#12345")).is_err());
    assert!(p.set("interface.canvasCustomColor", json!("#12345g")).is_err());
    p.set("interface.canvasCustomColor", json!("#abcdef")).unwrap();
    assert_eq!(p.interface.canvas_custom_color, "#abcdef");
}

#[test]
fn preferences_shortcut_and_reset() {
    let mut p = prefs::Preferences::default();
    p.set("shortcuts.edit.undo", json!("cmd+shift+z")).unwrap();
    assert_eq!(p.shortcut("edit.undo", Some("Cmd+Z")), Some("Cmd+Shift+Z"));
    p.set("shortcuts.edit.undo", Value::Null).unwrap();
    assert_eq!(p.shortcut("edit.undo", Some("Cmd+Z")), Some("Cmd+Z"));
    p.set("shortcuts.edit.undo", json!("")).unwrap();
    assert_eq!(p.shortcut("edit.undo", Some("Cmd+Z")), None);
}

#[test]
fn preferences_set_invalid_shortcut_errors() {
    let mut p = prefs::Preferences::default();
    assert!(p.set("shortcuts.edit.undo", json!("foo+bar")).is_err());
}

#[test]
fn preferences_reset_path_and_section() {
    let mut p = prefs::Preferences::default();
    p.set("general.beepWhenDone", json!(true)).unwrap();
    assert!(p.general.beep_when_done);
    p.reset(Some("general.beepWhenDone")).unwrap();
    assert!(!p.general.beep_when_done);
    p.set("general.beepWhenDone", json!(true)).unwrap();
    p.reset(Some("general")).unwrap();
    assert!(!p.general.beep_when_done);
    p.set("export.jpegQuality", json!(10)).unwrap();
    p.reset(None).unwrap();
    assert_eq!(p.export.jpeg_quality, 85);
}

#[test]
fn preferences_to_json_roundtrip() {
    let p = prefs::Preferences::default();
    let v = p.to_json();
    let p2: prefs::Preferences = from_value(v).unwrap();
    assert_eq!(p, p2);
}

#[test]
fn prefs_store_rev_bump_and_get() {
    let mut store = prefs::PrefsStore::default();
    assert_eq!(store.rev(), 0);
    assert_eq!(*store.get(), prefs::Preferences::default());
    store.edit(|p| p.general.beep_when_done = true);
    assert_eq!(store.rev(), 1);
    assert!(store.get().general.beep_when_done);
}

#[test]
fn normalize_shortcut_modifiers_order_and_key() {
    assert_eq!(prefs::normalize_shortcut("cmd+shift+n"), Some("Cmd+Shift+N".into()));
    assert_eq!(prefs::normalize_shortcut("shift+alt+ctrl+cmd+a"), Some("Cmd+Ctrl+Alt+Shift+A".into()));
    assert_eq!(prefs::normalize_shortcut("+"), Some("+".into()));
    assert_eq!(prefs::normalize_shortcut("Cmd++"), Some("Cmd++".into()));
    assert_eq!(prefs::normalize_shortcut(""), None);
    assert_eq!(prefs::normalize_shortcut("foo+bar"), None);
    assert_eq!(prefs::normalize_shortcut("Cmd+Shift+"), None);
}

#[test]
fn conflicts_finds_duplicate_shortcuts() {
    let bindings = [("cmd.a", "Cmd+Shift+X"), ("cmd.b", "cmd+shift+x"), ("cmd.c", "Cmd+Shift+Y")];
    let c = prefs::conflicts(bindings);
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].0, "Cmd+Shift+X");
    assert_eq!(c[0].1, vec!["cmd.a".to_string(), "cmd.b".to_string()]);
}

#[test]
fn helper_lookups() {
    assert_eq!(prefs::choices("general.colorPicker"), Some(prefs::ColorPicker::NAMES));
    assert!(prefs::choices("no.such").is_none());
    assert_eq!(prefs::range("export.jpegQuality"), Some((1.0, 100.0)));
    assert!(prefs::range("general.colorPicker").is_none());
    assert!(prefs::is_color("interface.canvasCustomColor"));
    assert!(!prefs::is_color("general.colorPicker"));
    assert!(!prefs::is_color("shortcuts.edit.undo"));
    assert!(prefs::is_hidden("general.colorPicker"));
    assert!(!prefs::is_hidden("general.imageInterpolation"));
    assert_eq!(prefs::SECTIONS.len(), 18);
    assert_eq!(prefs::SECTIONS[0], ("general", "General"));
    assert_eq!(prefs::SECTIONS[17], ("integrations", "Integrations"));
}
