use photocraft_paint::presets::{self, GROUPS};
use photocraft_paint::{BrushPreset, BrushSettings};

#[test]
fn groups_are_expected() {
    assert_eq!(GROUPS, ["General", "Dry Media", "Wet Media", "Special Effects"]);
}

#[test]
fn builtin_has_expected_shape() {
    let presets = presets::builtin();
    assert!(!presets.is_empty());
    assert!((30..=40).contains(&presets.len()), "{}", presets.len());
    for group in GROUPS {
        let count = presets.iter().filter(|p| p.group == group).count();
        assert!(count >= 6, "group {group} has {count}");
    }
}

#[test]
fn builtin_names_are_unique() {
    let presets = presets::builtin();
    let mut names: Vec<&str> = presets.iter().map(|p| p.name.as_str()).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), presets.len());
}

#[test]
fn builtin_names_and_groups_are_nonempty() {
    let presets = presets::builtin();
    for p in &presets {
        assert!(!p.name.is_empty());
        assert!(!p.group.is_empty());
    }
}

#[test]
fn builtin_groups_are_valid() {
    let presets = presets::builtin();
    for p in &presets {
        assert!(GROUPS.contains(&p.group.as_str()), "bad group {}", p.group);
    }
}

#[test]
fn builtin_flags_builtin_true() {
    let presets = presets::builtin();
    assert!(presets.iter().all(|p| p.builtin));
}

#[test]
fn builtin_groups_are_contiguous() {
    let presets = presets::builtin();
    let order: Vec<usize> = presets.iter().filter_map(|p| GROUPS.iter().position(|g| *g == p.group)).collect();
    assert_eq!(order.len(), presets.len());
    assert!(order.windows(2).all(|w| w[0] <= w[1]));
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
fn find_empty_slice_returns_none() {
    let empty: &[BrushPreset] = &[];
    assert!(presets::find(empty, "Hard Round").is_none());
    assert!(presets::find(empty, "").is_none());
}

#[test]
fn find_empty_name_returns_none_when_no_empty_name() {
    let presets = presets::builtin();
    assert!(presets::find(&presets, "").is_none());
}

#[test]
fn find_matches_empty_name_if_present() {
    let p = BrushPreset { name: String::new(), brush: BrushSettings::default(), builtin: false, group: "General".into() };
    let presets = vec![p];
    assert!(presets::find(&presets, "").is_some());
    assert_eq!(presets::find(&presets, "").map(|p| p.name.as_str()), Some(""));
}

#[test]
fn builtin_is_deterministic() {
    let a = presets::builtin();
    let b = presets::builtin();
    assert_eq!(a.len(), b.len());
    for (x, y) in a.iter().zip(&b) {
        assert_eq!(x.name, y.name);
        assert_eq!(x.group, y.group);
        assert_eq!(x.builtin, y.builtin);
    }
}

#[test]
fn clone_preserves_preset_equality() {
    let presets = presets::builtin();
    for p in &presets {
        let c = p.clone();
        assert_eq!(p.name, c.name);
        assert_eq!(p.group, c.group);
        assert_eq!(p.builtin, c.builtin);
    }
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
