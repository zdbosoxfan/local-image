use photocraft_paint::presets;

#[test]
fn builtin_names_and_groups_are_nonempty() {
    let presets = presets::builtin();
    for p in &presets {
        assert!(!p.name.is_empty());
        assert!(!p.group.is_empty());
    }
}

#[test]
fn builtin_contains_expected_presets() {
    let presets = presets::builtin();
    for name in ["Hard Round", "Soft Round", "Hard Pencil", "Watercolor Wet Edges", "Spatter", "Dual Grain"] {
        assert!(presets::find(&presets, name).is_some(), "missing {name}");
    }
}

#[test]
fn find_is_case_insensitive() {
    let presets = presets::builtin();
    let a = presets::find(&presets, "hard round").map(|p| p.name.clone());
    let b = presets::find(&presets, "HARD ROUND").map(|p| p.name.clone());
    assert!(a.is_some() && b.is_some());
    assert_eq!(a, b);
}

#[test]
fn find_missing_name_returns_none() {
    let presets = presets::builtin();
    assert!(presets::find(&presets, "Definitely Not A Brush").is_none());
}

#[test]
fn brush_settings_are_finite() {
    let presets = presets::builtin();
    for p in &presets {
        let b = &p.brush;
        assert!(b.size.is_finite(), "{} size", p.name);
        assert!(b.spacing.is_finite(), "{} spacing", p.name);
        assert!(b.hardness.is_finite(), "{} hardness", p.name);
        assert!(b.angle.is_finite(), "{} angle", p.name);
        assert!(b.roundness.is_finite(), "{} roundness", p.name);
        assert!(b.flow.is_finite(), "{} flow", p.name);
        assert!(b.opacity.is_finite(), "{} opacity", p.name);
        assert!(b.build_up_rate.is_finite(), "{} build_up_rate", p.name);
    }
}

#[test]
fn brush_settings_sizes_positive() {
    let presets = presets::builtin();
    for p in &presets {
        assert!(p.brush.size > 0.0, "{} size <=0", p.name);
    }
}

#[test]
fn brush_settings_hardness_in_range() {
    let presets = presets::builtin();
    for p in &presets {
        assert!((0.0..=1.0).contains(&p.brush.hardness), "{} hardness {}", p.name, p.brush.hardness);
    }
}

#[test]
fn brush_settings_spacing_nonnegative() {
    let presets = presets::builtin();
    for p in &presets {
        assert!(p.brush.spacing >= 0.0, "{} spacing {}", p.name, p.brush.spacing);
    }
}

#[test]
fn brush_settings_roundness_in_range() {
    let presets = presets::builtin();
    for p in &presets {
        assert!((0.0..=1.0).contains(&p.brush.roundness), "{} roundness {}", p.name, p.brush.roundness);
    }
}

#[test]
fn find_returns_reference_with_expected_group() {
    let presets = presets::builtin();
    let group_of = |name: &str| presets::find(&presets, name).map(|p| p.group.as_str());

    assert_eq!(group_of("Hard Round"), Some("General"));
    assert_eq!(group_of("Chalk"), Some("Dry Media"));
    assert_eq!(group_of("Watercolor Wet Edges"), Some("Wet Media"));
    assert_eq!(group_of("Spatter"), Some("Special Effects"));
}
