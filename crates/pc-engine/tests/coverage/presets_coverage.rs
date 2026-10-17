use photocraft_engine::Session;
use photocraft_engine::presets::{Group, Named};
use serde_json::json;

fn unique_names<T: Named>(items: &[T]) -> bool {
    let mut seen = std::collections::HashSet::new();
    items.iter().all(|item| seen.insert(item.name().to_string()))
}

#[test]
fn preset_state_roundtrip_preserves_content() {
    let mut s = Session::new();
    // Modify presets
    let fg = photocraft_engine::presets::gradients::GradientPreset::foreground_to_background();
    let group = Group::new("Custom Gradients", vec![fg.clone()]);
    s.presets.gradients.push(group);
    // The selected pattern is transient panel state and is NOT persisted by to_json.
    // We'll still set it locally to verify that it is not preserved after load.
    s.presets.pattern = Some("Test Pattern".to_string());
    s.presets.layer_defaults.insert("bevel".to_string(), json!({"size": 5}));
    s.presets.rev = 0; // keep rev at 0 before to_json

    let json = s.presets.to_json(&s);
    let mut s2 = Session::new();
    s2.load_presets_json(json);

    // Fields that ARE persisted should be preserved.
    assert_eq!(s2.presets.gradients, s.presets.gradients);
    assert_eq!(s2.presets.gradient, s.presets.gradient);
    assert_eq!(s2.presets.styles, s.presets.styles);
    assert_eq!(s2.presets.shapes, s.presets.shapes);
    assert_eq!(s2.presets.pattern_groups, s.presets.pattern_groups);
    assert_eq!(s2.presets.tool_presets, s.presets.tool_presets);
    assert_eq!(s2.presets.layer_defaults, s.presets.layer_defaults);
    // The pattern field is deliberately not persisted (transient panel state).
    assert_eq!(s2.presets.pattern, None);

    // rev is not persisted, load always increments from 0 to 1
    assert_eq!(s2.presets.rev, 1);
}

#[test]
fn load_presets_json_empty_object_keeps_defaults() {
    let mut s = Session::new();
    let before_gradients = s.presets.gradients.clone();
    let before_styles = s.presets.styles.clone();
    let before_shapes = s.presets.shapes.clone();
    let before_pattern_groups = s.presets.pattern_groups.clone();
    let before_tool_presets = s.presets.tool_presets.clone();
    let before_gradient = s.presets.gradient.clone();
    let before_pattern = s.presets.pattern.clone();
    let before_layer_defaults = s.presets.layer_defaults.clone();

    s.load_presets_json(json!({}));

    assert_eq!(s.presets.gradients, before_gradients);
    assert_eq!(s.presets.styles, before_styles);
    assert_eq!(s.presets.shapes, before_shapes);
    assert_eq!(s.presets.pattern_groups, before_pattern_groups);
    assert_eq!(s.presets.tool_presets, before_tool_presets);
    assert_eq!(s.presets.gradient, before_gradient);
    assert_eq!(s.presets.pattern, before_pattern);
    assert_eq!(s.presets.layer_defaults, before_layer_defaults);
    assert_eq!(s.presets.rev, 1, "loading valid JSON should increment rev");
}

#[test]
fn load_presets_json_malformed_is_ignored() {
    let mut s = Session::new();
    let rev_before = s.presets.rev;
    let keys_before = s.presets.gradients.len();

    s.load_presets_json(json!("not a valid presets blob"));
    assert_eq!(s.presets.rev, rev_before, "malformed load should not increment rev");
    assert_eq!(s.presets.gradients.len(), keys_before);

    s.load_presets_json(json!([1, 2, 3]));
    assert_eq!(s.presets.rev, rev_before);
    assert_eq!(s.presets.gradients.len(), keys_before);
}

#[test]
fn load_presets_json_partial_update() {
    let mut s = Session::new();
    let original_styles = s.presets.styles.clone();

    let custom_gradients = vec![Group::new("My Gradients", vec![photocraft_engine::presets::gradients::GradientPreset::foreground_to_background()])];
    s.load_presets_json(json!({
        "gradients": custom_gradients.clone(),
    }));

    assert_eq!(s.presets.gradients, custom_gradients);
    assert_eq!(s.presets.styles, original_styles, "unrelated fields should remain default");
    assert_eq!(s.presets.rev, 1);
}

#[test]
fn load_presets_json_null_field_keeps_default() {
    let mut s = Session::new();
    let before_gradients = s.presets.gradients.clone();
    s.load_presets_json(json!({"gradients": null}));
    assert_eq!(s.presets.gradients, before_gradients, "null should be treated as missing");
    assert_eq!(s.presets.rev, 1);
}

#[test]
fn builtin_groups_have_unique_names() {
    // Gradients
    let gradient_groups = photocraft_engine::presets::gradients::builtin();
    let all_gradients: Vec<_> = gradient_groups.iter().flat_map(|g| g.items.iter().cloned()).collect();
    assert!(unique_names(&all_gradients), "duplicate gradient preset names");

    // Styles
    let style_groups = photocraft_engine::presets::styles::builtin();
    let all_styles: Vec<_> = style_groups.iter().flat_map(|g| g.items.iter().cloned()).collect();
    assert!(unique_names(&all_styles), "duplicate style preset names");

    // Shapes
    let shape_groups = photocraft_engine::presets::shapes::builtin();
    let all_shapes: Vec<_> = shape_groups.iter().flat_map(|g| g.items.iter().cloned()).collect();
    assert!(unique_names(&all_shapes), "duplicate shape preset names");

    // Tool presets
    let tool_presets = photocraft_engine::presets::tools::builtin();
    assert!(unique_names(&tool_presets), "duplicate tool preset names");

    // Pattern groups: group names should be unique, and pattern names within each group unique
    let pattern_groups = photocraft_engine::presets::patterns::builtin_groups();
    let mut group_names = std::collections::HashSet::new();
    for group in &pattern_groups {
        assert!(group_names.insert(group.name.clone()), "duplicate pattern group name: {}", group.name);
        let pattern_items: Vec<String> = group.items.clone();
        assert!(unique_names(&pattern_items), "duplicate pattern name in group {}", group.name);
    }
}
