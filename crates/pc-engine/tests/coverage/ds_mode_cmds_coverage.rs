use photocraft_engine::Session;
use photocraft_engine::doc::{Duotone, DuotoneInk, LayerContent, Size};
use photocraft_engine::mode_cmds::{duotone_display_layer, preset_table, rotated_size, specs};
use serde_json::json;

#[test]
fn specs_contains_five_mode_commands() {
    let specs = specs();
    assert_eq!(specs.len(), 5, "expected exactly five mode_cmds specs");

    let expected = ["image.rotation.arbitrary", "image.mode.indexedColor", "image.mode.colorTable", "image.mode.bitmap", "image.mode.duotone"];
    for id in expected {
        assert!(specs.iter().any(|s| s.id == id), "missing spec {id}");
    }
}

#[test]
fn rotated_size_zero_and_full_turns_are_identity() {
    let size = Size::new(100, 50);
    assert_eq!(rotated_size(size, 0.0), size);
    assert_eq!(rotated_size(size, 360.0), size);
    assert_eq!(rotated_size(size, -360.0), size);
}

#[test]
fn rotated_size_quarter_turns_swap_dimensions() {
    let size = Size::new(100, 50);
    let swapped = Size::new(50, 100);
    assert_eq!(rotated_size(size, 90.0), swapped);
    assert_eq!(rotated_size(size, -90.0), swapped);
    assert_eq!(rotated_size(size, 270.0), swapped);
}

#[test]
fn rotated_size_square_stays_square() {
    let size = Size::new(64, 64);
    assert_eq!(rotated_size(size, 90.0), size);
    assert_eq!(rotated_size(size, 180.0), size);
}

#[test]
fn rotated_size_45_degrees_expands_canvas() {
    let size = Size::new(100, 50);
    let out = rotated_size(size, 45.0);
    assert!(out.width > size.width);
    assert!(out.height > size.height);
    assert!(out.width == out.height);
    // At 45° the bounding square of a non-square is (w+h)/√2, rounded up.
    // Here 106.066... -> 107.
    assert_eq!(out.width, 107);
    assert_eq!(out.height, 107);
}

#[test]
fn rotated_size_odd_sizes_never_panic_and_stay_at_least_one_pixel() {
    for deg in [1.0, 13.0, 45.0, 89.0, 123.456, 359.0, -15.0] {
        let a = rotated_size(Size::new(3, 5), deg);
        let b = rotated_size(Size::new(7, 2), deg);
        assert!(a.width >= 1 && a.height >= 1, "a for {deg}: {a:?}");
        assert!(b.width >= 1 && b.height >= 1, "b for {deg}: {b:?}");
    }
}

#[test]
fn rotated_size_is_symmetric_for_opposite_angles() {
    let size = Size::new(123, 77);
    for deg in [10.0, 37.5, 150.0, 359.0] {
        assert_eq!(rotated_size(size, deg), rotated_size(size, -deg));
    }
}

#[test]
fn rotated_size_nan_and_infinity_do_not_panic_and_yield_at_least_one_pixel() {
    let size = Size::new(20, 30);
    let nan = rotated_size(size, f64::NAN);
    let inf = rotated_size(size, f64::INFINITY);
    assert!(nan.width >= 1 && nan.height >= 1);
    assert!(inf.width >= 1 && inf.height >= 1);
}

#[test]
fn preset_table_known_ramps_have_256_entries() {
    for name in ["grayscale", "blackBody", "spectrum"] {
        let table = preset_table(name).expect("known preset");
        assert_eq!(table.len(), 256, "preset {name}");
    }
}

#[test]
fn preset_table_grayscale_is_linear_ramp() {
    let table = preset_table("grayscale").unwrap();
    assert_eq!(table[0], [0, 0, 0]);
    assert_eq!(table[128], [128, 128, 128]);
    assert_eq!(table[255], [255, 255, 255]);
}

#[test]
fn preset_table_blackbody_starts_black_ends_white() {
    let table = preset_table("blackBody").unwrap();
    assert_eq!(table[0], [0, 0, 0]);
    assert_eq!(table[255], [255, 255, 255]);
}

#[test]
fn preset_table_system_palettes_are_non_empty() {
    for name in ["systemMac", "systemWindows", "web"] {
        let table = preset_table(name).unwrap_or_else(|| panic!("missing {name}"));
        assert!(!table.is_empty(), "palette {name} is empty");
    }
}

#[test]
fn preset_table_unknown_returns_none() {
    assert!(preset_table("does-not-exist").is_none());
}

#[test]
fn duotone_display_layer_has_name_and_adjustment_content() {
    let inks = vec![DuotoneInk::new("Black", [0.0, 0.0, 0.0])];
    let duotone = Duotone { inks, psd_raw: None };
    let layer = duotone_display_layer(&duotone);
    assert_eq!(layer.name, "Duotone");
    assert!(matches!(layer.content, LayerContent::Adjustment(_)));
}

#[test]
fn executing_mode_commands_without_a_document_errors() {
    let mut session = Session::new();
    let commands = [
        ("image.rotation.arbitrary", json!({"angle": 15.0})),
        ("image.mode.indexedColor", json!({})),
        ("image.mode.colorTable", json!({})),
        ("image.mode.bitmap", json!({})),
        ("image.mode.duotone", json!({})),
    ];

    for (id, params) in commands {
        let result = session.execute(id, params);
        assert!(result.is_err(), "expected error for {id} without document");
    }
}
