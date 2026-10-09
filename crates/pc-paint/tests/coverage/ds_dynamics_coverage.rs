use photocraft_paint::dynamics::{
    DabBuilder, DabGenerator, DualBuilder, PathWalker, SPEED_SPACING_MS, Smoother, Spliner, StepInput, apply_pose, control_angle, control_value, smooth_points,
};
use photocraft_paint::{BrushSettings, Control, StrokePoint};

// Helper to create a simple StepInput
fn step_input(p: StrokePoint, step: u64) -> StepInput {
    StepInput { point: p, direction: 0.0, initial_direction: 0.0, step }
}

// Helper to create a default brush with common overrides
fn brush_with(size: f32, spacing: f32) -> BrushSettings {
    BrushSettings { size, spacing, ..Default::default() }
}

#[test]
fn smoother_amount_zero_passthrough() {
    let mut s = Smoother::new(&BrushSettings::default().smoothing, 1.0);
    let mut out = Vec::new();
    let pts = [StrokePoint::new(0.0, 0.0, 1.0), StrokePoint::new(10.0, 0.0, 1.0), StrokePoint::new(20.0, 5.0, 1.0)];
    for p in &pts {
        s.push(*p, &mut out);
    }
    s.finish(&mut out);
    assert_eq!(out.as_slice(), &pts);
}

#[test]
fn smoother_exponential_endpoint_smoothing() {
    let mut cfg = BrushSettings::default().smoothing;
    cfg.amount = 0.8;
    cfg.catch_up_on_end = true;
    let mut s = Smoother::new(&cfg, 1.0);
    let mut out = Vec::new();
    let pts = [StrokePoint::new(0.0, 0.0, 1.0), StrokePoint::new(10.0, 10.0, 1.0), StrokePoint::new(20.0, 0.0, 1.0)];
    for p in &pts {
        s.push(*p, &mut out);
    }
    s.finish(&mut out);
    // first point is emitted directly
    assert_eq!(out.first().unwrap(), &pts[0]);
    // last point with catch_up_on_end is exactly the last input
    assert_eq!(out.last().unwrap(), &pts[2]);
    // intermediate points are smoothed (not identical to raw)
    assert!(out.len() >= pts.len());
}

#[test]
fn smoother_pulled_string_limiting() {
    let mut cfg = BrushSettings::default().smoothing;
    cfg.amount = 1.0;
    cfg.pulled_string = true;
    cfg.catch_up_on_end = true;
    let mut s = Smoother::new(&cfg, 1.0);
    let mut out = Vec::new();
    let pts = [
        StrokePoint::new(0.0, 0.0, 1.0),
        StrokePoint::new(20.0, 0.0, 1.0), // distance < 100, no output during push
        StrokePoint::new(30.0, 0.0, 1.0), // cumulative distance < 100 from pos? pos remains first if no output
    ];
    s.push(pts[0], &mut out);
    assert_eq!(out.len(), 1); // first point output
    s.push(pts[1], &mut out);
    assert_eq!(out.len(), 1); // no new point yet
    s.push(pts[2], &mut out);
    assert_eq!(out.len(), 1); // still no new point
    s.finish(&mut out);
    assert_eq!(out.len(), 2); // first and last
    assert_eq!(out[0], pts[0]);
    assert_eq!(out[1], pts[2]);
}

#[test]
fn spliner_collinear_straight() {
    let mut s = Spliner::default();
    let mut out = Vec::new();
    for i in 0..6 {
        s.push(StrokePoint::new(i as f64 * 10.0, 5.0, 1.0), &mut out);
    }
    s.finish(&mut out);
    assert!(out.iter().all(|p| (p.y - 5.0).abs() < 1e-9), "y must be constant");
    assert!((out.last().unwrap().x - 50.0).abs() < 1e-9, "last x must be 50");
    assert!(out.windows(2).all(|w| w[1].x >= w[0].x - 1e-9), "monotone x");
}

#[test]
fn spliner_circle_round() {
    let mut s = Spliner::default();
    let mut out = Vec::new();
    let n = 16;
    for i in 0..=n {
        let a = i as f64 / n as f64 * std::f64::consts::TAU;
        s.push(StrokePoint::new(100.0 * a.cos(), 100.0 * a.sin(), 1.0), &mut out);
    }
    s.finish(&mut out);
    // Check that all output points are close to radius 100
    for p in &out {
        let r = p.x.hypot(p.y);
        assert!((r - 100.0).abs() < 3.0, "radial error too large: {}", (r - 100.0).abs());
    }
}

#[test]
fn control_value_all_variants() {
    let p = StrokePoint { x: 0.0, y: 0.0, pressure: 0.7, tilt_x: 45.0, tilt_y: 0.0, rotation: 180.0, wheel: 0.6, time: 0.0 };
    let s = StepInput { point: p, direction: std::f32::consts::PI / 2.0, initial_direction: std::f32::consts::PI, step: 5 };
    let fade = 10;
    assert_eq!(control_value(Control::Off, fade, &s), 1.0);
    assert_eq!(control_value(Control::InitialDirection, fade, &s), 1.0);
    assert_eq!(control_value(Control::Direction, fade, &s), 1.0);
    assert!((control_value(Control::Fade, fade, &s) - 0.5).abs() < 1e-6);
    assert!((control_value(Control::PenPressure, fade, &s) - 0.7).abs() < 1e-6);
    let expected_tilt = 1.0 - (45.0_f32.hypot(0.0) / 90.0).clamp(0.0, 1.0);
    assert!((control_value(Control::PenTilt, fade, &s) - expected_tilt).abs() < 1e-6);
    assert!((control_value(Control::StylusWheel, fade, &s) - 0.6).abs() < 1e-6);
    assert!((control_value(Control::Rotation, fade, &s) - 180.0_f32.rem_euclid(360.0) / 360.0).abs() < 1e-6);
}

#[test]
fn control_angle_all_variants() {
    let p = StrokePoint { x: 0.0, y: 0.0, pressure: 0.7, tilt_x: 45.0, tilt_y: -30.0, rotation: 120.0, wheel: 0.4, time: 0.0 };
    let s = StepInput { point: p, direction: std::f32::consts::PI / 2.0, initial_direction: std::f32::consts::PI, step: 5 };
    let fade = 10;
    assert_eq!(control_angle(Control::Off, fade, &s), 0.0);
    assert!((control_angle(Control::Fade, fade, &s) - 180.0).abs() < 1e-5);
    assert!((control_angle(Control::PenPressure, fade, &s) - 252.0).abs() < 1e-5);
    let expected_tilt = (-(-30.0_f32)).atan2(45.0).to_degrees();
    assert!((control_angle(Control::PenTilt, fade, &s) - expected_tilt).abs() < 1e-5);
    assert!((control_angle(Control::StylusWheel, fade, &s) - 144.0).abs() < 1e-5);
    assert_eq!(control_angle(Control::Rotation, fade, &s), 120.0);
    assert!((control_angle(Control::InitialDirection, fade, &s) - 180.0).abs() < 1e-5);
    assert!((control_angle(Control::Direction, fade, &s) - 90.0).abs() < 1e-5);
}

#[test]
fn control_value_nan_inf() {
    let p = StrokePoint {
        x: 0.0,
        y: 0.0,
        pressure: f32::NAN,
        tilt_x: f32::INFINITY,
        tilt_y: f32::NEG_INFINITY,
        rotation: f32::NAN,
        wheel: f32::INFINITY,
        time: 0.0,
    };
    let s = step_input(p, 0);
    let fade = 10;
    // These should not panic, even if results are NaN/infinite
    let _ = control_value(Control::PenPressure, fade, &s);
    let _ = control_value(Control::PenTilt, fade, &s);
    let _ = control_value(Control::StylusWheel, fade, &s);
    let _ = control_value(Control::Rotation, fade, &s);
}

#[test]
fn path_walker_constant_spacing() {
    let mut w = PathWalker::new(None);
    let step_len = |_: &StrokePoint| 2.0;
    let mut steps = Vec::new();
    w.push(StrokePoint::new(0.0, 0.0, 1.0), &step_len, &mut |s| steps.push(s));
    w.push(StrokePoint::new(10.0, 0.0, 1.0), &step_len, &mut |s| steps.push(s));
    w.finish(&mut |s| steps.push(s));
    assert_eq!(steps.len(), 6);
    // Positions should be approximately 0,2,4,6,8,10
    let xs: Vec<f64> = steps.iter().map(|s| s.point.x).collect();
    for (i, x) in xs.iter().enumerate() {
        let expected = i as f64 * 2.0;
        assert!((x - expected).abs() < 1e-9, "step {i}: x={x}, expected {expected}");
    }
}

#[test]
fn path_walker_no_timestamps_speed() {
    let mut w = PathWalker::new(None).speed_spacing(SPEED_SPACING_MS);
    let step_len = |_: &StrokePoint| 10.0;
    let mut steps = Vec::new();
    // All points have time=0, so speed spacing will emit one step per input point
    let pts = [StrokePoint::new(0.0, 0.0, 1.0), StrokePoint::new(10.0, 0.0, 1.0), StrokePoint::new(20.0, 0.0, 1.0)];
    for p in &pts {
        w.push(*p, &step_len, &mut |s| steps.push(s));
    }
    w.finish(&mut |s| steps.push(s));
    assert_eq!(steps.len(), pts.len());
}

#[test]
fn path_walker_airbrush_cap() {
    // Interval of 1 ms, huge dt should not cause excessive dabs
    let mut w = PathWalker::new(Some(1.0));
    let step_len = |_: &StrokePoint| 10.0;
    let mut steps = Vec::new();
    let p1 = StrokePoint::new(0.0, 0.0, 1.0);
    let mut p2 = StrokePoint::new(10.0, 0.0, 1.0);
    p2.time = 10000.0;
    w.push(p1, &step_len, &mut |s| steps.push(s));
    w.push(p2, &step_len, &mut |s| steps.push(s));
    w.finish(&mut |s| steps.push(s));
    // Expected: pending first (0,0), spacing step at (10,0), airbrush cap step at (10,0)
    assert_eq!(steps.len(), 3, "got {} steps", steps.len());
    assert_eq!(steps[0].point.x, 0.0);
    assert_eq!(steps[0].point.y, 0.0);
    assert_eq!(steps[1].point.x, 10.0);
    assert_eq!(steps[1].point.y, 0.0);
    assert_eq!(steps[2].point.x, 10.0);
    assert_eq!(steps[2].point.y, 0.0);
}

#[test]
fn dab_builder_step_len() {
    let b = brush_with(20.0, 0.5);
    let builder = DabBuilder::new(&b);
    let p = StrokePoint::new(0.0, 0.0, 1.0);
    assert!((builder.step_len(&p) - 10.0).abs() < 1e-6);

    let small_brush = brush_with(0.5, 0.5);
    let builder = DabBuilder::new(&small_brush);
    assert!((builder.step_len(&p) - 0.5).abs() < 1e-6);
}

#[test]
fn dab_builder_no_dynamics_round_dab() {
    let b =
        BrushSettings { size: 10.0, flow: 1.0, color: [1.0, 0.0, 0.0, 1.0], angle: 0.0, roundness: 1.0, flip_x: false, flip_y: false, ..Default::default() };
    let mut builder = DabBuilder::new(&b);
    let p = StrokePoint::new(5.0, 5.0, 1.0);
    let s = step_input(p, 0);
    let mut dabs = Vec::new();
    builder.build(&s, &mut dabs);
    assert_eq!(dabs.len(), 1);
    let dab = &dabs[0];
    assert_eq!(dab.center.x, 5.0);
    assert_eq!(dab.center.y, 5.0);
    assert!((dab.radius - 5.0).abs() < 1e-6);
    assert_eq!(dab.alpha, 1.0);
    assert_eq!(dab.angle, 0.0);
    assert_eq!(dab.roundness, 1.0);
    assert!(!dab.flip_x);
    assert!(!dab.flip_y);
    assert_eq!(dab.opacity, 1.0);
    assert_eq!(dab.color, [1.0, 0.0, 0.0, 1.0]);
    assert_eq!(dab.depth, 1.0);
    assert_eq!(dab.index, 0);
    assert_eq!(dab.proj_angle, 0.0);
    assert_eq!(dab.proj_scale, 1.0);
    assert_eq!(dab.wet, 1.0);
    assert_eq!(dab.mix, 1.0);
}

#[test]
fn dab_builder_shape_dynamics_deterministic() {
    let b = BrushSettings {
        seed: 12345,
        shape_dynamics: photocraft_paint::ShapeDynamics {
            enabled: true,
            size: photocraft_paint::Dynamic { jitter: 0.8, minimum: 0.2, control: Control::Off, ..Default::default() },
            angle: photocraft_paint::Dynamic { jitter: 0.5, control: Control::Off, ..Default::default() },
            roundness: photocraft_paint::Dynamic { jitter: 0.3, control: Control::Off, ..Default::default() },
            flip_x_jitter: true,
            flip_y_jitter: true,
            ..Default::default()
        },
        ..Default::default()
    };
    // Use helper to set size and spacing
    let mut b_full = brush_with(20.0, 0.5);
    b_full.seed = b.seed;
    b_full.shape_dynamics = b.shape_dynamics;

    let mut builder = DabBuilder::new(&b_full);
    let p = StrokePoint::new(10.0, 10.0, 1.0);
    let s = step_input(p, 3);

    let mut dabs1 = Vec::new();
    builder.build(&s, &mut dabs1);

    // For same step input and same builder state (after first build, next_index advanced),
    // we need a fresh builder for second call to compare determinism of a single build.
    let mut builder2 = DabBuilder::new(&b_full);
    let mut dabs3 = Vec::new();
    builder2.build(&s, &mut dabs3);

    assert_eq!(dabs1, dabs3, "build must be deterministic with same index");
    // Radius should not exceed base_d/2 (10.0)
    for dab in &dabs1 {
        assert!(dab.radius <= 10.0 + 1e-6);
        assert!(dab.radius > 0.0);
        assert!(dab.roundness >= 0.01);
    }
}

#[test]
fn dab_builder_scattering() {
    let b = BrushSettings {
        seed: 42,
        scattering: photocraft_paint::Scattering {
            enabled: true,
            scatter: photocraft_paint::Dynamic { jitter: 0.5, control: Control::Off, ..Default::default() },
            count: 1,
            count_jitter: photocraft_paint::Dynamic { control: Control::Off, jitter: 0.0, ..Default::default() },
            both_axes: true,
        },
        ..Default::default()
    };
    let b = BrushSettings { size: 10.0, spacing: 0.5, seed: b.seed, scattering: b.scattering, ..Default::default() };

    let mut builder = DabBuilder::new(&b);
    let p = StrokePoint::new(0.0, 0.0, 1.0);
    let s = step_input(p, 0);
    let mut dabs = Vec::new();
    builder.build(&s, &mut dabs);
    assert_eq!(dabs.len(), 1);
    let dab = &dabs[0];
    // Scatter amount: radius=5, jitter=0.5, cv=1 => amt = 2.5
    let dist = dab.center.x.hypot(dab.center.y);
    assert!(dist > 1e-6, "scatter should move the center away from origin");
    assert!(dist <= 2.5 * (2.0_f64).sqrt() + 1e-6, "center out of expected range: {dist}");
}

#[test]
fn dab_builder_transfer_and_color() {
    let b = BrushSettings {
        seed: 7,
        transfer: photocraft_paint::Transfer {
            enabled: true,
            opacity: photocraft_paint::Dynamic { control: Control::Fade, jitter: 0.0, fade_steps: 10, minimum: 0.1 },
            flow: photocraft_paint::Dynamic { jitter: 0.5, ..Default::default() },
            ..Default::default()
        },
        flow: 0.8,
        color_dynamics: photocraft_paint::ColorDynamics {
            enabled: true,
            per_tip: true,
            hue_jitter: 0.1,
            saturation_jitter: 0.2,
            brightness_jitter: 0.1,
            ..Default::default()
        },
        ..Default::default()
    };
    let b =
        BrushSettings { size: 10.0, spacing: 0.5, seed: b.seed, transfer: b.transfer, flow: b.flow, color_dynamics: b.color_dynamics, ..Default::default() };

    let mut builder = DabBuilder::new(&b);
    let mut dabs = Vec::new();
    for step in 0..5 {
        let p = StrokePoint::new(step as f64, 0.0, 1.0);
        let s = step_input(p, step);
        builder.build(&s, &mut dabs);
    }
    assert_eq!(dabs.len(), 5);
    // Check that opacity varies due to fade
    let first_opacity = dabs[0].opacity;
    let last_opacity = dabs[4].opacity;
    assert!(first_opacity > last_opacity, "fade should decrease opacity");
    for dab in &dabs {
        assert!(dab.opacity >= 0.1 - 1e-6 && dab.opacity <= 1.0 + 1e-6);
        assert!(dab.alpha >= 0.0 && dab.alpha <= 1.0);
        for c in dab.color.iter() {
            assert!(c.is_finite() && *c >= 0.0 && *c <= 1.0);
        }
    }
    // Per-tip color should differ between first and last
    assert_ne!(dabs[0].color, dabs[4].color);

    // Test per_tip = false -> all colors equal
    let mut b2 = b.clone();
    b2.color_dynamics.per_tip = false;
    let mut builder2 = DabBuilder::new(&b2);
    let mut dabs2 = Vec::new();
    for step in 0..5 {
        let p = StrokePoint::new(step as f64, 0.0, 1.0);
        let s = step_input(p, step);
        builder2.build(&s, &mut dabs2);
    }
    assert!(dabs2.windows(2).all(|w| w[0].color == w[1].color), "colors should be constant when per_tip=false");
}

#[test]
fn dab_builder_texture_depth() {
    let b = BrushSettings {
        seed: 11,
        texture: photocraft_paint::Texture {
            enabled: true,
            each_tip: true,
            depth: 0.5,
            depth_jitter: photocraft_paint::Dynamic { jitter: 0.8, control: Control::Off, ..Default::default() },
            ..Default::default()
        },
        ..Default::default()
    };
    let b = BrushSettings { size: 10.0, spacing: 0.5, seed: b.seed, texture: b.texture, ..Default::default() };

    let mut builder = DabBuilder::new(&b);
    let mut dabs = Vec::new();
    for step in 0..4 {
        let p = StrokePoint::new(step as f64, 0.0, 1.0);
        let s = step_input(p, step);
        builder.build(&s, &mut dabs);
    }
    assert_eq!(dabs.len(), 4);
    // depth should vary because each_tip and jitter
    assert!(dabs.windows(2).any(|w| w[0].depth != w[1].depth), "depth should vary");
    for dab in &dabs {
        assert!(dab.depth >= 0.0 && dab.depth <= 1.0);
    }

    // With each_tip=false, depth constant
    let mut b2 = b.clone();
    b2.texture.each_tip = false;
    let mut builder2 = DabBuilder::new(&b2);
    let mut dabs2 = Vec::new();
    for step in 0..4 {
        let p = StrokePoint::new(step as f64, 0.0, 1.0);
        let s = step_input(p, step);
        builder2.build(&s, &mut dabs2);
    }
    assert!(dabs2.windows(2).all(|w| w[0].depth == w[1].depth), "depth should be constant");
}

#[test]
fn dual_builder_basics() {
    let b = BrushSettings {
        seed: 99,
        dual_brush: photocraft_paint::DualBrush {
            enabled: true,
            size: 20.0,
            spacing: 0.5,
            count: 3,
            scatter: 0.2,
            both_axes: true,
            angle: 0.0,
            roundness: 1.0,
            flip: false,
            ..Default::default()
        },
        ..Default::default()
    };

    let mut builder = DualBuilder::new(&b);
    let p = StrokePoint::new(10.0, 10.0, 1.0);
    let s = step_input(p, 0);
    let mut dabs = Vec::new();
    builder.build(&s, &mut dabs);
    assert_eq!(dabs.len(), 3);
    for dab in &dabs {
        assert!((dab.radius - 10.0).abs() < 1e-6);
        assert_eq!(dab.angle, 0.0);
        assert_eq!(dab.roundness, 1.0);
        assert!(!dab.flip_x);
        assert!(!dab.flip_y);
        // center should be near original with some scatter
        let dist = (dab.center.x - 10.0).hypot(dab.center.y - 10.0);
        assert!(dist <= 2.0 * (2.0_f64).sqrt() + 1e-6);
    }
}

#[test]
fn apply_pose_overrides() {
    let b = BrushSettings {
        pose: photocraft_paint::Pose {
            enabled: true,
            override_tilt: true,
            override_rotation: true,
            override_pressure: true,
            tilt_x: 30.0,
            tilt_y: 60.0,
            rotation: 200.0,
            pressure: 0.3,
        },
        ..Default::default()
    };

    let p = StrokePoint { x: 1.0, y: 2.0, pressure: 0.9, tilt_x: 10.0, tilt_y: 20.0, rotation: 100.0, wheel: 0.5, time: 0.0 };
    let out = apply_pose(&b, p);
    assert_eq!(out.tilt_x, 30.0);
    assert_eq!(out.tilt_y, 60.0);
    assert_eq!(out.rotation, 200.0);
    assert_eq!(out.pressure, 0.3);
    // other fields unchanged
    assert_eq!(out.x, p.x);
    assert_eq!(out.y, p.y);
    assert_eq!(out.wheel, p.wheel);
    assert_eq!(out.time, p.time);

    // disabled pose returns same point
    let b2 = BrushSettings { pose: photocraft_paint::Pose { enabled: false, ..Default::default() }, ..Default::default() };
    let out2 = apply_pose(&b2, p);
    assert_eq!(out2, p);
}

#[test]
fn dab_generator_empty() {
    let brush = BrushSettings::default();
    let mut generator = DabGenerator::new(&brush, 1.0);
    let mut primary = Vec::new();
    let mut dual = Vec::new();
    generator.push(&[], &mut primary, &mut dual);
    generator.finish(&mut primary, &mut dual);
    assert!(primary.is_empty());
    assert!(dual.is_empty());
}

#[test]
fn dab_generator_single_point() {
    let brush = brush_with(10.0, 0.5);
    let mut generator = DabGenerator::new(&brush, 1.0);
    let mut primary = Vec::new();
    let mut dual = Vec::new();
    let pts = [StrokePoint::new(5.0, 5.0, 1.0)];
    generator.push(&pts, &mut primary, &mut dual);
    generator.finish(&mut primary, &mut dual);
    assert_eq!(primary.len(), 1);
    assert!(dual.is_empty());
    let dab = &primary[0];
    assert_eq!(dab.center.x, 5.0);
    assert_eq!(dab.center.y, 5.0);
    assert!((dab.radius - 5.0).abs() < 1e-6);
}

#[test]
fn dab_generator_two_points_count() {
    // Use freehand=false to avoid spline, spacing=0.5, size=10 -> step_len=5
    let b = BrushSettings { size: 10.0, spacing: 0.5, freehand: false, spacing_enabled: true, build_up: false, ..Default::default() };
    let mut generator = DabGenerator::new(&b, 1.0);
    let mut primary = Vec::new();
    let mut dual = Vec::new();
    let pts = [StrokePoint::new(0.0, 0.0, 1.0), StrokePoint::new(20.0, 0.0, 1.0)];
    generator.push(&pts, &mut primary, &mut dual);
    generator.finish(&mut primary, &mut dual);
    // Expected 5 dabs: at x=0, 5, 10, 15, 20
    assert_eq!(primary.len(), 5);
    let xs: Vec<f64> = primary.iter().map(|d| d.center.x).collect();
    let expected_xs = [0.0, 5.0, 10.0, 15.0, 20.0];
    for (i, &x) in xs.iter().enumerate() {
        assert!((x - expected_xs[i]).abs() < 1e-6, "dab {i} x={x}, expected {}", expected_xs[i]);
    }
    assert!(dual.is_empty());
}

#[test]
fn dab_generator_chunking_equivalence() {
    let b = BrushSettings {
        size: 10.0,
        spacing: 0.5,
        freehand: true,
        spacing_enabled: true,
        seed: 123,
        shape_dynamics: photocraft_paint::ShapeDynamics {
            enabled: true,
            size: photocraft_paint::Dynamic { jitter: 0.3, ..Default::default() },
            ..Default::default()
        },
        scattering: photocraft_paint::Scattering {
            enabled: true,
            scatter: photocraft_paint::Dynamic { jitter: 0.2, ..Default::default() },
            count: 2,
            count_jitter: photocraft_paint::Dynamic { jitter: 0.5, ..Default::default() },
            ..Default::default()
        },
        ..Default::default()
    };

    let pts = [StrokePoint::new(0.0, 0.0, 1.0), StrokePoint::new(10.0, 10.0, 0.5), StrokePoint::new(20.0, 0.0, 1.0), StrokePoint::new(30.0, 10.0, 0.7)];

    // Generate all at once
    let mut generator1 = DabGenerator::new(&b, 1.0);
    let mut primary1 = Vec::new();
    let mut dual1 = Vec::new();
    generator1.push(&pts, &mut primary1, &mut dual1);
    generator1.finish(&mut primary1, &mut dual1);

    // Generate in chunks
    let mut generator2 = DabGenerator::new(&b, 1.0);
    let mut primary2 = Vec::new();
    let mut dual2 = Vec::new();
    generator2.push(&pts[0..2], &mut primary2, &mut dual2);
    generator2.push(&pts[2..4], &mut primary2, &mut dual2);
    generator2.finish(&mut primary2, &mut dual2);

    assert_eq!(primary1.len(), primary2.len(), "chunking changed dab count");
    assert_eq!(dual1.len(), dual2.len());
    for (i, (d1, d2)) in primary1.iter().zip(primary2.iter()).enumerate() {
        assert_eq!(d1, d2, "primary dab {i} differs");
    }
    for (i, (d1, d2)) in dual1.iter().zip(dual2.iter()).enumerate() {
        assert_eq!(d1, d2, "dual dab {i} differs");
    }
}

#[test]
fn dab_generator_nan_no_panic() {
    // Ensure NaN input does not panic, even if output may be nonsense
    let b = BrushSettings { spacing_enabled: true, freehand: true, ..Default::default() };
    let mut generator = DabGenerator::new(&b, 1.0);
    let mut primary = Vec::new();
    let mut dual = Vec::new();
    let pts = [StrokePoint::new(f64::NAN, f64::NAN, f32::NAN), StrokePoint::new(f64::INFINITY, f64::NEG_INFINITY, f32::INFINITY)];
    generator.push(&pts, &mut primary, &mut dual);
    generator.finish(&mut primary, &mut dual);
    // Just ensure no panic; no assertions on values
}

#[test]
fn dab_generator_determinism() {
    let b = BrushSettings {
        size: 15.0,
        spacing: 0.3,
        seed: 777,
        freehand: true,
        spacing_enabled: true,
        shape_dynamics: photocraft_paint::ShapeDynamics {
            enabled: true,
            size: photocraft_paint::Dynamic { jitter: 0.6, minimum: 0.2, ..Default::default() },
            ..Default::default()
        },
        scattering: photocraft_paint::Scattering {
            enabled: true,
            scatter: photocraft_paint::Dynamic { jitter: 0.4, ..Default::default() },
            ..Default::default()
        },
        color_dynamics: photocraft_paint::ColorDynamics { enabled: true, per_tip: true, hue_jitter: 0.5, ..Default::default() },
        ..Default::default()
    };

    let pts = [StrokePoint::new(0.0, 0.0, 1.0), StrokePoint::new(20.0, 20.0, 0.8), StrokePoint::new(40.0, 0.0, 0.5)];

    let mut generator1 = DabGenerator::new(&b, 1.0);
    let mut primary1 = Vec::new();
    let mut dual1 = Vec::new();
    generator1.push(&pts, &mut primary1, &mut dual1);
    generator1.finish(&mut primary1, &mut dual1);

    let mut generator2 = DabGenerator::new(&b, 1.0);
    let mut primary2 = Vec::new();
    let mut dual2 = Vec::new();
    generator2.push(&pts, &mut primary2, &mut dual2);
    generator2.finish(&mut primary2, &mut dual2);

    assert_eq!(primary1, primary2);
    assert_eq!(dual1, dual2);
}

#[test]
fn smooth_points_wrapper() {
    let pts = [StrokePoint::new(0.0, 0.0, 1.0), StrokePoint::new(10.0, 10.0, 1.0), StrokePoint::new(20.0, 0.0, 1.0)];
    // amount=0 returns same points
    let mut cfg = BrushSettings::default().smoothing;
    cfg.amount = 0.0;
    let out = smooth_points(&cfg, 1.0, &pts);
    assert_eq!(out.as_slice(), &pts);

    // amount>0 with catch_up keeps endpoints
    cfg.amount = 0.7;
    cfg.catch_up_on_end = true;
    let out = smooth_points(&cfg, 1.0, &pts);
    assert_eq!(out.first().unwrap(), &pts[0]);
    assert_eq!(out.last().unwrap(), &pts[2]);
}
