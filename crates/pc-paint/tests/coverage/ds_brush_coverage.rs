use photocraft_paint::brush::{
    BrushPreset, BrushSettings, ColorDynamics, Control, DualBrush, Dynamic, MAX_BRUSH_SIZE, MaskMode, PaintMode, Pattern, PatternStyle, Pose, Scattering,
    SectionLocks, ShapeDynamics, Smoothing, Texture, TipShape, Transfer,
};

#[test]
fn default_values_are_sane() {
    let b = BrushSettings::default();
    assert_eq!(b.size, 20.0);
    assert_eq!(b.hardness, 1.0);
    assert_eq!(b.spacing, 0.1);
    assert_eq!(b.opacity, 1.0);
    assert_eq!(b.flow, 1.0);
    assert!(b.spacing_enabled);
    assert!(b.pressure_size);
    assert!(!b.pressure_opacity);
    assert_eq!(b.color, [0.0, 0.0, 0.0, 1.0]);
    assert_eq!(b.background, [1.0, 1.0, 1.0, 1.0]);
    assert!(!b.erase);
    assert_eq!(b.tip, TipShape::Round);
    assert_eq!(b.angle, 0.0);
    assert_eq!(b.roundness, 1.0);
    assert!(!b.flip_x);
    assert!(!b.flip_y);
    assert!(!b.aliased);
    assert_eq!(b.locks, SectionLocks::default());
    assert_eq!(b.build_up_rate, 20.0);
    assert_eq!(b.seed, 0);
}

#[test]
fn dynamic_constructors_and_active() {
    let d = Dynamic::default();
    assert_eq!(d.jitter, 0.0);
    assert_eq!(d.control, Control::Off);
    assert_eq!(d.fade_steps, 25);
    assert_eq!(d.minimum, 0.0);
    assert!(!d.is_active());

    let j = Dynamic::jitter(0.3);
    assert!(j.is_active());

    let c = Dynamic::controlled(Control::PenPressure);
    assert!(c.is_active());
    assert_eq!(c.control, Control::PenPressure);
    assert_eq!(c.fade_steps, 25);
}

#[test]
fn dynamic_serde_roundtrip() {
    let d = Dynamic { jitter: 0.7, control: Control::Fade, fade_steps: 10, minimum: 0.2 };
    let json = serde_json::to_string(&d).unwrap();
    let back: Dynamic = serde_json::from_str(&json).unwrap();
    assert_eq!(d, back);
}

#[test]
fn enum_serde_roundtrips() {
    let controls = [
        Control::Off,
        Control::Fade,
        Control::PenPressure,
        Control::PenTilt,
        Control::StylusWheel,
        Control::Rotation,
        Control::InitialDirection,
        Control::Direction,
    ];
    for c in controls {
        let json = serde_json::to_string(&c).unwrap();
        let back: Control = serde_json::from_str(&json).unwrap();
        assert_eq!(c, back);
    }

    let modes = [
        MaskMode::Multiply,
        MaskMode::Subtract,
        MaskMode::Darken,
        MaskMode::Overlay,
        MaskMode::ColorDodge,
        MaskMode::ColorBurn,
        MaskMode::LinearBurn,
        MaskMode::HardMix,
        MaskMode::LinearHeight,
        MaskMode::Height,
    ];
    for m in modes {
        let json = serde_json::to_string(&m).unwrap();
        let back: MaskMode = serde_json::from_str(&json).unwrap();
        assert_eq!(m, back);
    }

    let styles = [PatternStyle::Noise, PatternStyle::Canvas, PatternStyle::Paper, PatternStyle::Dots];
    for s in styles {
        let json = serde_json::to_string(&s).unwrap();
        let back: PatternStyle = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
    }

    let paints = [PaintMode::Blend, PaintMode::Behind, PaintMode::Clear];
    for p in paints {
        let json = serde_json::to_string(&p).unwrap();
        let back: PaintMode = serde_json::from_str(&json).unwrap();
        assert_eq!(p, back);
    }
}

#[test]
fn pattern_procedural_serde_roundtrip() {
    let pat = Pattern::Procedural { style: PatternStyle::Canvas, size: 256, seed: 42 };
    let json = serde_json::to_string(&pat).unwrap();
    let back: Pattern = serde_json::from_str(&json).unwrap();
    assert_eq!(pat, back);
}

#[test]
fn brush_settings_serde_roundtrip_full() {
    let b = BrushSettings {
        size: 40.0,
        hardness: 0.6,
        spacing: 0.35,
        spacing_enabled: false,
        opacity: 0.8,
        flow: 0.7,
        pressure_size: false,
        pressure_opacity: true,
        color: [1.0, 0.5, 0.25, 0.9],
        background: [0.2, 0.3, 0.4, 1.0],
        erase: true,
        angle: 45.0,
        roundness: 0.3,
        flip_x: true,
        flip_y: false,
        aliased: true,
        noise: true,
        wet_edges: true,
        build_up: true,
        build_up_rate: 30.0,
        protect_texture: true,
        seed: 123456789,
        shape_dynamics: ShapeDynamics {
            enabled: true,
            size: Dynamic { jitter: 0.1, control: Control::PenPressure, fade_steps: 20, minimum: 0.1 },
            tilt_scale: 0.5,
            angle: Dynamic { jitter: 0.2, control: Control::Direction, fade_steps: 15, minimum: 0.0 },
            roundness: Dynamic { jitter: 0.3, control: Control::Fade, fade_steps: 10, minimum: 0.2 },
            flip_x_jitter: true,
            flip_y_jitter: true,
            brush_projection: true,
        },
        scattering: Scattering {
            enabled: true,
            scatter: Dynamic { jitter: 2.0, control: Control::Off, fade_steps: 25, minimum: 0.0 },
            both_axes: true,
            count: 4,
            count_jitter: Dynamic { jitter: 0.5, control: Control::PenPressure, fade_steps: 25, minimum: 0.0 },
        },
        texture: Texture {
            enabled: true,
            pattern: Pattern::Procedural { style: PatternStyle::Dots, size: 64, seed: 7 },
            invert: true,
            scale: 1.5,
            brightness: 0.2,
            contrast: -0.3,
            each_tip: true,
            mode: MaskMode::Overlay,
            depth: 0.6,
            depth_jitter: Dynamic { jitter: 0.1, control: Control::PenTilt, fade_steps: 25, minimum: 0.1 },
        },
        dual_brush: DualBrush {
            enabled: true,
            mode: MaskMode::Darken,
            tip: TipShape::Round,
            size: 25.0,
            hardness: 0.5,
            roundness: 0.8,
            angle: 30.0,
            spacing: 0.2,
            scatter: 0.6,
            both_axes: true,
            count: 2,
            flip: true,
        },
        color_dynamics: ColorDynamics {
            enabled: true,
            per_tip: false,
            fg_bg: Dynamic { jitter: 0.4, control: Control::PenPressure, fade_steps: 25, minimum: 0.0 },
            hue_jitter: 0.2,
            saturation_jitter: 0.3,
            brightness_jitter: 0.1,
            purity: -0.5,
        },
        transfer: Transfer {
            enabled: true,
            opacity: Dynamic { jitter: 0.1, control: Control::PenPressure, fade_steps: 25, minimum: 0.3 },
            flow: Dynamic { jitter: 0.2, control: Control::StylusWheel, fade_steps: 25, minimum: 0.0 },
            wetness: Dynamic { jitter: 0.3, control: Control::Off, fade_steps: 25, minimum: 0.0 },
            mix: Dynamic { jitter: 0.4, control: Control::Fade, fade_steps: 30, minimum: 0.1 },
        },
        pose: Pose {
            enabled: true,
            tilt_x: 20.0,
            tilt_y: -10.0,
            rotation: 90.0,
            pressure: 0.5,
            override_tilt: true,
            override_rotation: true,
            override_pressure: true,
        },
        smoothing: Smoothing { amount: 0.7, pulled_string: true, catch_up: false, catch_up_on_end: false, adjust_for_zoom: false },
        locks: SectionLocks {
            shape_dynamics: true,
            scattering: true,
            texture: true,
            dual_brush: true,
            color_dynamics: true,
            transfer: true,
            pose: true,
            noise: true,
            wet_edges: true,
            build_up: true,
            smoothing: true,
            protect_texture: true,
        },
        ..Default::default()
    };

    // Ensure freehand stays false for round-trip equality (it is skipped).
    assert!(!b.freehand);
    let json = serde_json::to_string(&b).unwrap();
    let back: BrushSettings = serde_json::from_str(&json).unwrap();
    assert_eq!(b, back);
}

#[test]
fn brush_settings_partial_json_uses_defaults() {
    let json = r#"{"size": 35.0, "scattering": {"enabled": true, "scatter": {"jitter": 2.0}}}"#;
    let b: BrushSettings = serde_json::from_str(json).unwrap();
    assert_eq!(b.size, 35.0);
    assert!(b.scattering.enabled);
    assert_eq!(b.scattering.scatter.jitter, 2.0);
    // Defaults for other fields:
    assert_eq!(b.hardness, 1.0);
    assert_eq!(b.spacing, 0.1);
    assert_eq!(b.opacity, 1.0);
    assert_eq!(b.color, [0.0, 0.0, 0.0, 1.0]);
}

#[test]
fn brush_settings_camel_case_keys() {
    let b = BrushSettings::default();
    let v = serde_json::to_value(&b).unwrap();
    let obj = v.as_object().unwrap();
    assert!(obj.contains_key("spacingEnabled"));
    assert!(obj.contains_key("pressureSize"));
    assert!(obj.contains_key("pressureOpacity"));
    assert!(obj.contains_key("shapeDynamics"));
    assert!(obj.contains_key("dualBrush"));
    assert!(obj.contains_key("buildUpRate"));
    // Serde skip means no freehand in output.
    assert!(!obj.contains_key("freehand"));
}

#[test]
fn brush_settings_skips_freehand() {
    let b = BrushSettings { freehand: true, ..Default::default() };
    let json = serde_json::to_string(&b).unwrap();
    assert!(!json.contains("freehand"));
    let back: BrushSettings = serde_json::from_str(&json).unwrap();
    assert!(!back.freehand);
}

#[test]
fn bounded_for_render_clamps_and_inf() {
    let b = BrushSettings { size: 10000.0, dual_brush: DualBrush { size: -3.0, ..Default::default() }, ..Default::default() };
    let bounded = b.bounded_for_render();
    assert_eq!(bounded.size, MAX_BRUSH_SIZE);
    assert_eq!(bounded.dual_brush.size, 0.5);

    let b2 = BrushSettings { size: f32::NAN, dual_brush: DualBrush { size: f32::INFINITY, ..Default::default() }, ..Default::default() };
    let bounded2 = b2.bounded_for_render();
    assert_eq!(bounded2.size, 0.5);
    assert_eq!(bounded2.dual_brush.size, 0.5);

    let b3 = BrushSettings { size: 0.1, dual_brush: DualBrush { size: 0.0, ..Default::default() }, ..Default::default() };
    let bounded3 = b3.bounded_for_render();
    assert_eq!(bounded3.size, 0.5);
    assert_eq!(bounded3.dual_brush.size, 0.5);
}

#[test]
fn bounded_for_render_does_not_clamp_valid() {
    let b = BrushSettings { size: 50.0, dual_brush: DualBrush { size: 30.0, ..Default::default() }, ..Default::default() };
    let bounded = b.bounded_for_render();
    assert_eq!(bounded.size, 50.0);
    assert_eq!(bounded.dual_brush.size, 30.0);
}

#[test]
fn bounded_for_render_preserves_other_fields() {
    let b = BrushSettings { size: 100.0, hardness: 0.4, opacity: 0.7, ..Default::default() };
    let bounded = b.bounded_for_render();
    assert_eq!(bounded.hardness, 0.4);
    assert_eq!(bounded.opacity, 0.7);
    assert_eq!(bounded.size, 100.0);
}

#[test]
fn continuous_coverage_soft_round_default_is_true() {
    let b = BrushSettings { hardness: 0.5, ..Default::default() };
    assert!(b.continuous_coverage());

    let b = BrushSettings {
        hardness: 0.5,
        shape_dynamics: ShapeDynamics { enabled: true, size: Dynamic::controlled(Control::PenPressure), ..Default::default() },
        ..Default::default()
    };
    assert!(b.continuous_coverage());

    let b = BrushSettings {
        hardness: 0.5,
        shape_dynamics: ShapeDynamics { enabled: true, size: Dynamic::controlled(Control::StylusWheel), ..Default::default() },
        ..Default::default()
    };
    assert!(b.continuous_coverage());
}

#[test]
fn continuous_coverage_hard_brush_is_false() {
    let b = BrushSettings { hardness: 1.0, ..Default::default() };
    assert!(!b.continuous_coverage());
}

#[test]
fn continuous_coverage_disabling_factors() {
    let b = BrushSettings {
        hardness: 0.5,
        shape_dynamics: ShapeDynamics { enabled: true, angle: Dynamic::jitter(0.1), ..Default::default() },
        ..Default::default()
    };
    assert!(!b.continuous_coverage());

    let b = BrushSettings {
        hardness: 0.5,
        shape_dynamics: ShapeDynamics { enabled: true, roundness: Dynamic::controlled(Control::PenPressure), ..Default::default() },
        ..Default::default()
    };
    assert!(!b.continuous_coverage());

    let b = BrushSettings { hardness: 0.5, shape_dynamics: ShapeDynamics { enabled: true, flip_x_jitter: true, ..Default::default() }, ..Default::default() };
    assert!(!b.continuous_coverage());

    let b =
        BrushSettings { hardness: 0.5, shape_dynamics: ShapeDynamics { enabled: true, brush_projection: true, ..Default::default() }, ..Default::default() };
    assert!(!b.continuous_coverage());

    let b = BrushSettings {
        hardness: 0.5,
        shape_dynamics: ShapeDynamics { enabled: true, size: Dynamic::controlled(Control::Fade), ..Default::default() },
        ..Default::default()
    };
    assert!(!b.continuous_coverage());

    let b = BrushSettings { hardness: 0.5, scattering: Scattering { enabled: true, ..Default::default() }, ..Default::default() };
    assert!(!b.continuous_coverage());

    let b = BrushSettings { hardness: 0.5, wet_edges: true, ..Default::default() };
    assert!(!b.continuous_coverage());

    let b = BrushSettings { hardness: 0.5, noise: true, ..Default::default() };
    assert!(!b.continuous_coverage());

    let b = BrushSettings { hardness: 0.5, dual_brush: DualBrush { enabled: true, ..Default::default() }, ..Default::default() };
    assert!(!b.continuous_coverage());

    let b = BrushSettings { hardness: 0.5, texture: Texture { enabled: true, each_tip: true, ..Default::default() }, ..Default::default() };
    assert!(!b.continuous_coverage());

    let b = BrushSettings { hardness: 0.5, color_dynamics: ColorDynamics { enabled: true, ..Default::default() }, ..Default::default() };
    assert!(!b.continuous_coverage());

    let b = BrushSettings { hardness: 0.5, build_up: true, ..Default::default() };
    assert!(!b.continuous_coverage());

    let b = BrushSettings { hardness: 0.5, transfer: Transfer { enabled: true, opacity: Dynamic::jitter(0.1), ..Default::default() }, ..Default::default() };
    assert!(!b.continuous_coverage());

    let b = BrushSettings {
        hardness: 0.5,
        transfer: Transfer { enabled: true, flow: Dynamic::controlled(Control::Direction), ..Default::default() },
        ..Default::default()
    };
    assert!(!b.continuous_coverage());

    let b = BrushSettings { hardness: 0.5, spacing: 0.0, ..Default::default() };
    assert!(!b.continuous_coverage());

    let b = BrushSettings { hardness: 0.5, spacing_enabled: false, ..Default::default() };
    assert!(!b.continuous_coverage());
}

#[test]
fn picked_over_applies_locks_and_smoothing() {
    let current = BrushSettings {
        size: 50.0,
        smoothing: Smoothing { amount: 0.9, ..Default::default() },
        shape_dynamics: ShapeDynamics { enabled: true, size: Dynamic::jitter(0.5), ..Default::default() },
        scattering: Scattering { enabled: true, count: 5, ..Default::default() },
        texture: Texture { enabled: true, scale: 2.0, ..Default::default() },
        dual_brush: DualBrush { enabled: true, size: 30.0, ..Default::default() },
        color_dynamics: ColorDynamics { enabled: true, hue_jitter: 0.3, ..Default::default() },
        transfer: Transfer { enabled: true, opacity: Dynamic::jitter(0.2), ..Default::default() },
        pose: Pose { enabled: true, pressure: 0.4, ..Default::default() },
        noise: true,
        wet_edges: true,
        build_up: true,
        build_up_rate: 40.0,
        protect_texture: true,
        locks: SectionLocks {
            shape_dynamics: true,
            scattering: true,
            texture: true,
            dual_brush: true,
            color_dynamics: true,
            transfer: true,
            pose: true,
            noise: true,
            wet_edges: true,
            build_up: true,
            smoothing: true,
            protect_texture: true,
        },
        ..Default::default()
    };

    let new_brush = BrushSettings {
        size: 10.0,
        smoothing: Smoothing { amount: 0.1, ..Default::default() },
        shape_dynamics: ShapeDynamics::default(),
        scattering: Scattering::default(),
        texture: Texture::default(),
        dual_brush: DualBrush::default(),
        color_dynamics: ColorDynamics::default(),
        transfer: Transfer::default(),
        pose: Pose::default(),
        noise: false,
        wet_edges: false,
        build_up: false,
        build_up_rate: 1.0,
        protect_texture: false,
        locks: SectionLocks::default(),
        ..Default::default()
    };

    let picked = new_brush.picked_over(&current);

    // Smoothing always copied from current.
    assert_eq!(picked.smoothing, current.smoothing);
    // Locks copied from current.
    assert_eq!(picked.locks, current.locks);
    // Each locked section copied from current.
    assert_eq!(picked.shape_dynamics, current.shape_dynamics);
    assert_eq!(picked.scattering, current.scattering);
    assert_eq!(picked.texture, current.texture);
    assert_eq!(picked.dual_brush, current.dual_brush);
    assert_eq!(picked.color_dynamics, current.color_dynamics);
    assert_eq!(picked.transfer, current.transfer);
    assert_eq!(picked.pose, current.pose);
    assert_eq!(picked.noise, current.noise);
    assert_eq!(picked.wet_edges, current.wet_edges);
    assert_eq!(picked.build_up, current.build_up);
    assert_eq!(picked.build_up_rate, current.build_up_rate);
    assert_eq!(picked.protect_texture, current.protect_texture);
}

#[test]
fn picked_over_no_locks_keeps_new_sections() {
    let current = BrushSettings {
        smoothing: Smoothing { amount: 0.8, ..Default::default() },
        shape_dynamics: ShapeDynamics { enabled: true, size: Dynamic::jitter(0.9), ..Default::default() },
        scattering: Scattering { enabled: true, count: 8, ..Default::default() },
        texture: Texture { enabled: true, scale: 3.0, ..Default::default() },
        dual_brush: DualBrush { enabled: true, size: 45.0, ..Default::default() },
        color_dynamics: ColorDynamics { enabled: true, hue_jitter: 0.7, ..Default::default() },
        transfer: Transfer { enabled: true, opacity: Dynamic::jitter(0.4), ..Default::default() },
        pose: Pose { enabled: true, pressure: 0.1, ..Default::default() },
        noise: true,
        wet_edges: true,
        build_up: true,
        build_up_rate: 50.0,
        protect_texture: true,
        // Locks default (all false)
        ..Default::default()
    };

    let new_brush = BrushSettings {
        smoothing: Smoothing { amount: 0.1, ..Default::default() },
        shape_dynamics: ShapeDynamics::default(),
        scattering: Scattering::default(),
        texture: Texture::default(),
        dual_brush: DualBrush::default(),
        color_dynamics: ColorDynamics::default(),
        transfer: Transfer::default(),
        pose: Pose::default(),
        noise: false,
        wet_edges: false,
        build_up: false,
        build_up_rate: 1.0,
        protect_texture: false,
        ..Default::default()
    };

    // Clone before moving so we can compare against original settings later.
    let original_new_brush = new_brush.clone();
    let picked = new_brush.picked_over(&current);

    // Smoothing still copied.
    assert_eq!(picked.smoothing, current.smoothing);
    // Locks copied (all false).
    assert_eq!(picked.locks, current.locks);
    // Sections remain new_brush's.
    assert_eq!(picked.shape_dynamics, original_new_brush.shape_dynamics);
    assert_eq!(picked.scattering, original_new_brush.scattering);
    assert_eq!(picked.texture, original_new_brush.texture);
    assert_eq!(picked.dual_brush, original_new_brush.dual_brush);
    assert_eq!(picked.color_dynamics, original_new_brush.color_dynamics);
    assert_eq!(picked.transfer, original_new_brush.transfer);
    assert_eq!(picked.pose, original_new_brush.pose);
    assert_eq!(picked.noise, original_new_brush.noise);
    assert_eq!(picked.wet_edges, original_new_brush.wet_edges);
    assert_eq!(picked.build_up, original_new_brush.build_up);
    assert_eq!(picked.build_up_rate, original_new_brush.build_up_rate);
    assert_eq!(picked.protect_texture, original_new_brush.protect_texture);
}

#[test]
fn with_protected_texture_keeps_pattern_and_scale() {
    let current = BrushSettings {
        protect_texture: true,
        texture: Texture { enabled: true, pattern: Pattern::Procedural { style: PatternStyle::Canvas, size: 128, seed: 99 }, scale: 1.7, ..Default::default() },
        ..Default::default()
    };

    let new_brush = BrushSettings {
        texture: Texture { enabled: true, pattern: Pattern::Procedural { style: PatternStyle::Dots, size: 32, seed: 1 }, scale: 0.5, ..Default::default() },
        ..Default::default()
    };

    let result = new_brush.with_protected_texture(&current);
    assert_eq!(result.texture.pattern, current.texture.pattern);
    assert_eq!(result.texture.scale, current.texture.scale);
    assert!(result.protect_texture);
}

#[test]
fn with_protected_texture_no_change_when_not_protecting() {
    let current = BrushSettings::default(); // protect_texture false
    let new_brush = BrushSettings {
        texture: Texture { enabled: true, pattern: Pattern::Procedural { style: PatternStyle::Dots, size: 32, seed: 1 }, scale: 0.5, ..Default::default() },
        ..Default::default()
    };

    let result = new_brush.clone().with_protected_texture(&current);
    assert_eq!(result.texture.pattern, new_brush.texture.pattern);
    assert_eq!(result.texture.scale, new_brush.texture.scale);
    assert!(!result.protect_texture);
}

#[test]
fn with_protected_texture_no_change_when_current_texture_disabled() {
    let current = BrushSettings { protect_texture: true, ..Default::default() };
    let new_brush = BrushSettings {
        texture: Texture { enabled: true, pattern: Pattern::Procedural { style: PatternStyle::Dots, size: 32, seed: 1 }, scale: 0.5, ..Default::default() },
        ..Default::default()
    };

    let result = new_brush.clone().with_protected_texture(&current);
    assert_eq!(result.texture.pattern, new_brush.texture.pattern);
    assert_eq!(result.texture.scale, new_brush.texture.scale);
    assert!(!result.protect_texture);
}

#[test]
fn brush_preset_serde_roundtrip() {
    let brush = BrushSettings { size: 42.0, ..Default::default() };
    let preset = BrushPreset { name: "Test Brush".to_string(), brush, builtin: true, group: "General".to_string() };
    let json = serde_json::to_string(&preset).unwrap();
    let back: BrushPreset = serde_json::from_str(&json).unwrap();
    assert_eq!(preset, back);
}

#[test]
fn malformed_json_deserialization_errors() {
    let invalid = "not json";
    assert!(serde_json::from_str::<BrushSettings>(invalid).is_err());

    let wrong_type = r#"{"size": "large"}"#;
    assert!(serde_json::from_str::<BrushSettings>(wrong_type).is_err());

    let null_type = r#"{"hardness": null}"#;
    assert!(serde_json::from_str::<BrushSettings>(null_type).is_err());

    // Wrong type inside a nested struct should also error.
    let nested_wrong_type = r#"{"shapeDynamics": {"enabled": "yes"}}"#;
    assert!(serde_json::from_str::<BrushSettings>(nested_wrong_type).is_err());
}

#[test]
fn non_finite_serialization_produces_null() {
    let b = BrushSettings { size: f32::NAN, ..Default::default() };
    let json = serde_json::to_string(&b).unwrap();
    assert!(json.contains("\"size\":null"));

    let b2 = BrushSettings { dual_brush: DualBrush { size: f32::INFINITY, ..Default::default() }, ..Default::default() };
    let json2 = serde_json::to_string(&b2).unwrap();
    let val: serde_json::Value = serde_json::from_str(&json2).unwrap();
    assert!(val["dualBrush"]["size"].is_null());
}
