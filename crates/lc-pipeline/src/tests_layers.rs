//! Develop layers (`Mask::tools`, `Mask::opacity`): pixel tests, and the golden hashes that pin
//! renders of settings written before layers existed (re-recorded for the unified Detail path).

use li_raster::{RgbFrame, Rgba8};
use lightcraft_develop::DevelopSettings;
use serde_json::{Value, json};

use crate::{RenderRequest, SourceInfo, render};

const W: usize = 240;
const H: usize = 160;

/// A deterministic hash in −1..1.
fn noise(x: usize, y: usize, k: u32) -> f32 {
    let mut v = (x as u32).wrapping_mul(0x8da6_b343) ^ (y as u32).wrapping_mul(0xd816_3841) ^ k.wrapping_mul(0xcb1a_b31f);
    v ^= v >> 13;
    v = v.wrapping_mul(0x5bd1_e995);
    v ^= v >> 15;
    (v & 0xffff) as f32 / 32768.0 - 1.0
}

/// A synthetic scene-linear photo: a luminance ramp left to right, hue bands top to bottom,
/// a few saturated patches, fine noise and a hard edge.
fn scene() -> RgbFrame {
    RgbFrame::from_fn(W, H, |x, y| {
        let l = 0.004 * 1.03f32.powi(x as i32);
        let band = (y * 6 / H) as f32;
        let tint = [[1.0, 0.45, 0.3], [0.9, 0.8, 0.25], [0.35, 0.9, 0.35], [0.3, 0.7, 1.0], [0.75, 0.4, 0.95], [0.8, 0.8, 0.8]][band as usize % 6];
        let edge = if (x / 20 + y / 20) % 2 == 0 { 1.0 } else { 0.7 };
        let n = 1.0 + 0.05 * noise(x, y, 7);
        [l * tint[0] * edge * n, l * tint[1] * edge * n, l * tint[2] * edge * n]
    })
}

fn shot(s: &DevelopSettings, info: &SourceInfo) -> Rgba8 {
    render(&scene(), info, s, &RenderRequest::fit(W, H)).image
}

fn fnv(img: &Rgba8) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for p in &img.data {
        for b in p {
            h ^= *b as u64;
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
    }
    h
}

fn radial(cx: f64, cy: f64) -> Value {
    json!({"op": "add", "invert": false, "shape": {"kind": "radial", "center": {"x": cx, "y": cy}, "rx": 0.25, "ry": 0.2, "angle": 10.0, "feather": 60.0, "invert": false}})
}

/// Settings JSON as written before develop layers existed (masks carry `adjust` only).
fn golden_cases() -> Vec<(&'static str, Value)> {
    vec![
        ("default", json!({})),
        (
            "global edits",
            json!({
                "wb": {"mode": "custom", "temp": 5200.0, "tint": 8.0},
                "light": {"exposure": 0.4, "contrast": 25.0, "highlights": -40.0, "shadows": 35.0, "whites": 10.0, "blacks": -12.0},
                "curve": {"lights": 20.0, "darks": -10.0, "master": [{"x": 0.0, "y": 0.0}, {"x": 0.5, "y": 0.56}, {"x": 1.0, "y": 1.0}], "refine_saturation": 70.0},
                "color": {"vibrance": 20.0, "saturation": -10.0},
                "mixer": {"blue": {"hue": 20.0, "sat": -30.0, "lum": 10.0}},
                "grading": {"shadows": {"hue": 220.0, "sat": 30.0, "lum": 0.0}, "highlights": {"hue": 40.0, "sat": 20.0, "lum": 5.0}},
                "effects": {"texture": 15.0, "clarity": 20.0, "dehaze": 10.0},
                "vignette": {"amount": -25.0},
                "grain": {"amount": 20.0, "seed": 3},
                "detail": {"sharpen_amount": 40.0, "nr_luminance": 20.0, "nr_color": 25.0}
            }),
        ),
        (
            "old masks",
            json!({
                "light": {"exposure": 0.2},
                "masks": [
                    {"id": 1, "name": "Mask 1", "visible": true, "invert": false, "components": [radial(0.3, 0.4)],
                     "adjust": {"exposure": 0.8, "saturation": 30.0, "temp": -20.0, "tint": 10.0, "contrast": 20.0, "highlights": -30.0, "shadows": 25.0}},
                    {"id": 2, "name": "Mask 2", "visible": true, "invert": true, "components": [
                        {"op": "add", "invert": false, "shape": {"kind": "linear", "start": {"x": 0.5, "y": 0.0}, "end": {"x": 0.5, "y": 0.7}}},
                        {"op": "subtract", "invert": false, "shape": {"kind": "luminanceRange", "lo": 0.7, "hi": 1.0, "lo_feather": 0.1, "hi_feather": 0.1}}
                     ],
                     "adjust": {"clarity": 40.0, "dehaze": 20.0, "texture": -20.0, "whites": 15.0, "blacks": -10.0, "hue": 30.0, "amount": 60.0}},
                    {"id": 3, "name": "Mask 3", "visible": true, "invert": false, "components": [radial(0.7, 0.6)],
                     "adjust": {"noise": 40.0, "moire": 30.0, "defringe": 50.0, "sharpness": 30.0, "color_hue": 200.0, "color_sat": 40.0, "amount": 150.0},
                     "refine": 40.0},
                    {"id": 4, "name": "Hidden", "visible": false, "invert": false, "components": [radial(0.5, 0.5)], "adjust": {"exposure": -3.0}}
                ]
            }),
        ),
    ]
}

/// Historical settings rendered by the unified Detail pipeline, recorded on x86_64 Linux
/// (other platforms' libm may round differently). Render hashes were re-recorded after
/// correcting haze quantiles and numerical edge cases; historical JSON still round-trips.
const GOLDEN: [(&str, u64); 6] = [
    ("default/rendered", 0x6dc2c87b1494a4a2),
    ("default/raw", 0x61dfdd1c1479866c),
    ("global edits/rendered", 0x09212f51c8d3d413),
    ("global edits/raw", 0x84b197c73fda45a1),
    ("old masks/rendered", 0x391f1b6ffe02a961),
    ("old masks/raw", 0xd9d3a39693a547f6),
];

#[test]
fn old_settings_load_and_match_detail_goldens() {
    let mut got = Vec::new();
    for (name, v) in golden_cases() {
        let s = DevelopSettings::from_json(&v).expect("old settings parse");
        // they round-trip to the same JSON (nothing new is written for them)
        assert_eq!(DevelopSettings::from_json(&s.to_json()).expect("roundtrip"), s, "{name}");
        for (kind, info) in [("rendered", SourceInfo::default()), ("raw", SourceInfo { raw: true, ..Default::default() })] {
            got.push((format!("{name}/{kind}"), fnv(&shot(&s, &info))));
        }
    }
    if std::env::var_os("LC_GOLDEN_PRINT").is_some() {
        for (n, h) in &got {
            println!("    (\"{n}\", 0x{h:016x}),");
        }
    }
    if cfg!(all(target_arch = "x86_64", target_os = "linux")) {
        for ((name, h), (gname, g)) in got.iter().zip(GOLDEN) {
            assert_eq!(name, gname);
            assert_eq!(*h, g, "{name}: render changed");
        }
    }
}

// Layer tools

use lightcraft_develop::{LayerTools, Mask, MaskComponent, MaskOp, MaskShape};
use lightcraft_geom::Point;

/// A mask selecting x < 0.45 fully, x > 0.46 not at all.
fn left_layer(tools: Value, opacity: f64) -> DevelopSettings {
    layer_over(MaskShape::Linear { start: Point::new(0.45, 0.5), end: Point::new(0.46, 0.5) }, tools, opacity)
}

/// A mask selecting everything (alpha exactly 1).
fn full_layer(tools: Value) -> DevelopSettings {
    layer_over(MaskShape::Linear { start: Point::new(2.0, 0.5), end: Point::new(3.0, 0.5) }, tools, 100.0)
}

fn layer_over(shape: MaskShape, tools: Value, opacity: f64) -> DevelopSettings {
    let tools: LayerTools = LayerTools::default().merged((6500.0, 0.0), &tools).expect("layer tools");
    let mut s = DevelopSettings::default();
    s.masks.push(Mask {
        id: 1,
        components: vec![MaskComponent { name: None, op: MaskOp::Add, invert: false, shape }],
        tools,
        opacity,
        ..Default::default()
    });
    s
}

/// (mean |Δ|, max |Δ|) over columns `x0..x1`.
fn diff(a: &Rgba8, b: &Rgba8, x0: usize, x1: usize) -> (f64, u8) {
    let (mut sum, mut max, mut n) = (0u64, 0u8, 0u64);
    for y in 0..H {
        for x in x0..x1 {
            let (p, q) = (a.data[y * W + x], b.data[y * W + x]);
            for c in 0..3 {
                let d = p[c].abs_diff(q[c]);
                sum += d as u64;
                max = max.max(d);
                n += 1;
            }
        }
    }
    (sum as f64 / n as f64, max)
}

const LEFT: (usize, usize) = (0, W * 44 / 100);
const RIGHT: (usize, usize) = (W * 47 / 100, W);

fn tool_cases() -> Vec<(&'static str, Value)> {
    vec![
        ("tone curve", json!({"curve": {"master": [{"x": 0.0, "y": 0.15}, {"x": 0.5, "y": 0.7}, {"x": 1.0, "y": 1.0}]}})),
        ("parametric curve", json!({"curve": {"darks": 60.0, "lights": -40.0}})),
        ("hsl", json!({"mixer": {"blue": {"hue": 60.0, "sat": -80.0, "lum": 30.0}, "red": {"sat": 60.0}}})),
        ("grading", json!({"grading": {"shadows": {"hue": 220.0, "sat": 70.0}, "highlights": {"hue": 40.0, "sat": 60.0}}})),
        ("exposure", json!({"light": {"exposure": 1.0}})),
        ("contrast whites blacks", json!({"light": {"contrast": 60.0, "whites": -40.0, "blacks": 30.0}})),
        ("highlights shadows", json!({"light": {"highlights": -80.0, "shadows": 80.0}})),
        ("white balance", json!({"wb": {"temp": 3500.0, "tint": 20.0}})),
        ("vibrance saturation", json!({"color": {"vibrance": 50.0, "saturation": -40.0}})),
        ("b&w", json!({"treatment": "bw", "bw_mix": {"blue": 50.0}})),
        (
            "point color",
            json!({"point_colors": [{"lum": 0.6, "chroma": 0.12, "hue": 250.0, "hue_shift": 100.0, "sat_shift": -90.0, "range": 100.0}]}),
        ),
        ("effects", json!({"effects": {"clarity": 80.0, "texture": 60.0, "dehaze": 50.0}})),
        ("vignette", json!({"vignette": {"amount": -80.0}})),
        ("grain", json!({"grain": {"amount": 80.0}})),
        ("sharpening", json!({"detail": {"sharpen_amount": 150.0}})),
        ("noise reduction", json!({"detail": {"nr_luminance": 90.0, "nr_color": 90.0}})),
    ]
}

#[test]
fn every_layer_tool_changes_only_the_masked_area() {
    for info in [SourceInfo::default(), SourceInfo { raw: true, ..Default::default() }] {
        let base = shot(&DevelopSettings::default(), &info);
        for (name, tools) in tool_cases() {
            let img = shot(&left_layer(tools, 100.0), &info);
            let (inside, _) = diff(&base, &img, LEFT.0, LEFT.1);
            let (outside, max_out) = diff(&base, &img, RIGHT.0, RIGHT.1);
            assert!(inside > 0.4, "{name}: no visible effect where masked ({inside:.3})");
            assert_eq!((outside, max_out), (0.0, 0), "{name}: changed the unmasked area");
        }
    }
}

#[test]
fn half_opacity_gives_half_the_effect() {
    // Curves now blend in working linear RGB, before output conversion and encoding.
    let curve = json!({"curve": {"master": [{"x": 0.0, "y": 0.2}, {"x": 0.5, "y": 0.75}, {"x": 1.0, "y": 1.0}]}});
    let info = SourceInfo::default();
    let base = shot(&DevelopSettings::default(), &info);
    let full = shot(&left_layer(curve.clone(), 100.0), &info);
    let half = shot(&left_layer(curve, 50.0), &info);
    for y in 0..H {
        for x in LEFT.0..LEFT.1 {
            let i = y * W + x;
            for c in 0..3 {
                use li_color::transfer::{linear_to_srgb, srgb_to_linear};
                let want =
                    linear_to_srgb((srgb_to_linear(base.data[i][c] as f32 / 255.0) + srgb_to_linear(full.data[i][c] as f32 / 255.0)) / 2.0) * 255.0;
                assert!((half.data[i][c] as f32 - want).abs() <= 2.0, "({x},{y}) c{c}: {} vs {want}", half.data[i][c]);
            }
        }
    }
    // other tools: half the mean change, roughly (they blend at earlier stages)
    for (name, tools) in tool_cases() {
        let (f, _) = diff(&base, &shot(&left_layer(tools.clone(), 100.0), &info), LEFT.0, LEFT.1);
        let (h, _) = diff(&base, &shot(&left_layer(tools, 50.0), &info), LEFT.0, LEFT.1);
        assert!(h > 0.3 * f && h < 0.7 * f, "{name}: opacity 50 % gives {h:.3} of {f:.3}");
    }
}

#[test]
fn layers_without_tools_render_like_no_layer() {
    let info = SourceInfo { raw: true, ..Default::default() };
    let mut photo = DevelopSettings::default();
    photo.light.exposure = 0.3;
    photo.effects.clarity = 20.0;
    let base = shot(&photo, &info);
    // no tools at all (any opacity), and sections set but neutral
    for (tools, opacity) in
        [(json!({}), 100.0), (json!({}), 35.0), (json!({"light": {}, "curve": {}, "mixer": {}, "grading": {}, "detail": {}}), 100.0)]
    {
        let mut s = left_layer(tools.clone(), opacity);
        s.light = photo.light;
        s.effects = photo.effects;
        assert_eq!(shot(&s, &info).data, base.data, "{tools} at {opacity}");
    }
    // a hidden layer does nothing
    let mut s = left_layer(json!({"light": {"exposure": 2.0}}), 100.0);
    s.masks[0].visible = false;
    assert_eq!(shot(&s, &SourceInfo::default()).data, shot(&DevelopSettings::default(), &SourceInfo::default()).data);
}

/// Layer tools act on the completed Background; RAW global exposure
/// happens before its tone map and therefore is deliberately a different operation.
#[test]
fn filled_layer_brightens_the_finished_raw_background_by_a_stop() {
    use std::sync::Arc;
    let info = SourceInfo { raw: true, ..Default::default() };
    let src = Arc::new(scene());
    let req = RenderRequest::fit(W, H);
    let cache = crate::StageCache::default();
    let mut s = full_layer(json!({"light": {"exposure": 1.0}}));
    s.light.exposure = 0.7;
    s.light.contrast = 45.0;
    s.light.highlights = -50.0;
    crate::render_cached(&src, &info, &s, &req, &cache);
    let entries = cache.lock();
    let outputs = &entries[0].stack.outputs;
    for (base, layer) in outputs[0].1.as_ref().unwrap().data.iter().zip(&outputs[1].1.as_ref().unwrap().data) {
        for c in 0..3 {
            assert!((layer[c] - base[c] * 2.0).abs() < 1e-6);
        }
    }
    drop(entries);
    let mut global = s.clone();
    global.masks.clear();
    global.light.exposure += 1.0;
    assert!(diff(&shot(&s, &info), &shot(&global, &info), 0, W).0 > 1.0);
}

/// Background exposure plus a layer is not summed before the tone map.
#[test]
fn layers_stack_on_the_photo_and_on_each_other() {
    let info = SourceInfo { raw: true, ..Default::default() };
    let mut s = full_layer(json!({"light": {"exposure": 0.5}}));
    s.light.exposure = 0.5;
    let mut two = s.clone();
    let mut m = two.masks[0].clone();
    m.id = 2;
    two.masks.push(m);
    let mut one = s.clone();
    one.masks[0].tools.light.as_mut().unwrap().exposure = 1.0;
    let (mean, max) = diff(&shot(&two, &info), &shot(&one, &info), 0, W);
    assert!(mean < 0.01 && max <= 1, "two half-stops equal one stop: {mean}, {max}");
}

#[test]
fn owner_colour_then_gradient_stack_uses_the_finished_colour_image() {
    use std::sync::Arc;
    let src = Arc::new(scene());
    let info = SourceInfo { raw: true, ..Default::default() };
    let req = RenderRequest::fit(W, H);
    let cache = crate::StageCache::default();
    let mut s = left_layer(json!({"mixer":{"red":{"hue":70.0,"sat":-30.0},"blue":{"hue":-50.0}}}), 100.0);
    s.light.exposure = 0.8;
    s.light.contrast = 45.0;
    s.light.highlights = -60.0;
    s.light.shadows = 50.0;
    s.curve.lights = 30.0;
    s.color.vibrance = 25.0;
    let mut gradient = full_layer(json!({"light":{"exposure":-1.0}})).masks.remove(0);
    gradient.id = 2;
    gradient.components[0].shape = MaskShape::Linear { start: Point::new(0.0, 0.0), end: Point::new(0.0, 1.0) };
    s.masks.push(gradient);
    crate::render_cached(&src, &info, &s, &req, &cache);
    let entries = cache.lock();
    let outputs = &entries[0].stack.outputs;
    let log = outputs[1].1.as_ref().unwrap().map(crate::local::log_lum);
    let frame = crate::frame_for(&src, &info, &s, true);
    let alpha = crate::masks::evaluate_one(&s.masks[1], &frame, W, H, outputs[1].1.as_ref().unwrap(), &log, 0.0);
    let mut changed = 0;
    for (i, &a) in alpha.data.iter().enumerate() {
        let colour = outputs[1].1.as_ref().unwrap().data[i];
        let top = outputs[2].1.as_ref().unwrap().data[i];
        for c in 0..3 {
            assert!((top[c] - colour[c] * (1.0 - 0.5 * a)).abs() < 1e-6, "pixel {i}");
            changed += usize::from((colour[c] - outputs[0].1.as_ref().unwrap().data[i][c]).abs() > 0.001);
        }
    }
    assert!(changed > 100, "the colour layer has a measurable effect before the gradient");
}

#[test]
fn layer_order_changes_the_expected_curve_exposure_result() {
    let src = std::sync::Arc::new(RgbFrame::from_fn(16, 16, |_, _| [0.08, 0.06, 0.04]));
    let info = SourceInfo::default();
    let req = RenderRequest::fit(16, 16);
    let mut s = full_layer(json!({"curve":{"master":[{"x":0.0,"y":0.12},{"x":1.0,"y":1.0}]}}));
    s.masks[0].components[0].shape = MaskShape::Full;
    let mut ev = s.masks[0].clone();
    ev.id = 2;
    ev.tools = LayerTools::default().merged((6500.0, 0.0), &json!({"light":{"exposure":-1.0}})).unwrap();
    s.masks.push(ev);
    let a = crate::render_cached(&src, &info, &s, &req, &crate::StageCache::default()).image;
    s.masks.swap(0, 1);
    let b = render(&src, &info, &s, &req).image;
    assert!(
        a.data.iter().zip(&b.data).all(|(a, b)| a[0] < b[0] && a[1] < b[1] && a[2] < b[2]),
        "a lifted curve applied after darkening lifts the shadow more"
    );
}

#[test]
fn stack_prefix_cache_invalidates_only_the_edited_layer_and_above() {
    let src = std::sync::Arc::new(scene());
    let info = SourceInfo { raw: true, ..Default::default() };
    let req = RenderRequest::fit(W, H);
    let cache = crate::StageCache::default();
    let mut s = full_layer(json!({"light":{"exposure":0.2}}));
    s.masks[0].components[0].shape = MaskShape::Full;
    for id in [2, 3] {
        let mut m = s.masks[0].clone();
        m.id = id;
        s.masks.push(m);
    }
    let render_check = |s: &DevelopSettings, expected| {
        let a = crate::render_cached(&src, &info, s, &req, &cache).image;
        assert_eq!(cache.stack_renders(), expected);
        assert_eq!(a, render(&src, &info, s, &req).image);
    };
    render_check(&s, (1, 3));
    render_check(&s, (0, 0));
    s.masks[2].tools.light.as_mut().unwrap().exposure = 0.5;
    render_check(&s, (0, 1));
    s.masks[1].tools.light.as_mut().unwrap().exposure = -0.1;
    render_check(&s, (0, 2));
    s.masks[1].opacity = 40.0;
    render_check(&s, (0, 2));
    s.light.contrast = 20.0;
    render_check(&s, (1, 3));
    s.masks.swap(0, 2);
    render_check(&s, (0, 3));
    s.masks[0].visible = false;
    render_check(&s, (0, 2));
}

#[test]
fn stacked_over_white_exposure_can_be_recovered_before_output() {
    let src = std::sync::Arc::new(RgbFrame::from_fn(24, 16, |_, _| [0.4, 0.3, 0.2]));
    let info = SourceInfo::default();
    let req = RenderRequest::fit(24, 16);
    let cache = crate::StageCache::default();
    let mut s = full_layer(json!({"light":{"exposure":3.0}}));
    s.masks[0].components[0].shape = MaskShape::Full;
    let mut restore = s.masks[0].clone();
    restore.id = 2;
    restore.tools.light.as_mut().unwrap().exposure = -3.0;
    s.masks.push(restore);
    let result = crate::render_cached(&src, &info, &s, &req, &cache).image;
    assert_eq!(result, render(&src, &info, &DevelopSettings::default(), &req).image);
    assert!(cache.lock()[0].stack.outputs[1].1.as_ref().unwrap().data.iter().all(|c| c[0] > 1.0));
}

#[test]
fn cached_renders_with_layers_match_uncached() {
    let src = std::sync::Arc::new(scene());
    let info = SourceInfo::default();
    let cache = crate::StageCache::default();
    let req = RenderRequest::fit(W, H);
    let mut s = left_layer(json!({"detail": {"nr_luminance": 60.0}, "effects": {"clarity": 40.0}, "curve": {"darks": 30.0}}), 80.0);
    for k in 0..3 {
        s.masks[0].tools.detail.as_mut().expect("detail").nr_luminance = 40.0 + 20.0 * k as f64;
        let a = crate::render_cached(&src, &info, &s, &req, &cache).image;
        let b = render(&src, &info, &s, &req).image;
        assert_eq!(a.data, b.data, "pass {k}");
    }
    assert!(!crate::layers_need_cpu(&s));
    assert!(!crate::layers_need_cpu(&left_layer(json!({}), 50.0)));
}

#[test]
fn filled_layer_exposure_matches_global_and_empty_is_inert() {
    use lightcraft_develop::{LayerTools, Light, Mask, MaskComponent, MaskShape};
    let src = RgbFrame::from_fn(32, 16, |_, _| [0.08, 0.04, 0.02]);
    let info = SourceInfo::default();
    let req = RenderRequest::fit(32, 16);
    let mut global = DevelopSettings::default();
    global.light.exposure = 1.0;
    let mut layer = DevelopSettings::default();
    layer.masks.push(Mask {
        components: vec![MaskComponent { name: None, op: Default::default(), invert: false, shape: MaskShape::Full }],
        tools: LayerTools { light: Some(Light { exposure: 1.0, ..Default::default() }), ..Default::default() },
        ..Default::default()
    });
    assert_eq!(render(&src, &info, &global, &req).image, render(&src, &info, &layer, &req).image);
    layer.masks[0].opacity = 50.0;
    // A full exposure layer mixes in scene-linear space: 50% of 1x and 2x is 1.5x.
    global.light.exposure = 1.5f64.log2();
    assert_eq!(render(&src, &info, &global, &req).image, render(&src, &info, &layer, &req).image);
    layer.masks[0].components.clear();
    assert_eq!(render(&src, &info, &DevelopSettings::default(), &req).image, render(&src, &info, &layer, &req).image);
}

#[test]
fn filled_alpha_is_scalar_until_a_spatial_component_is_added() {
    use lightcraft_develop::{Mask, MaskComponent, MaskOp, MaskShape};
    let mut m = Mask {
        opacity: 50.0,
        refine: 90.0,
        components: vec![MaskComponent { name: None, op: MaskOp::Add, invert: false, shape: MaskShape::Full }],
        ..Default::default()
    };
    assert_eq!(crate::masks::scalar_alpha(&m), Some(0.5));
    m.invert = true;
    assert_eq!(crate::masks::scalar_alpha(&m), Some(0.0));
    m.invert = false;
    m.components.push(MaskComponent {
        name: None,
        op: MaskOp::Subtract,
        invert: false,
        shape: MaskShape::Brush {
            strokes: vec![lightcraft_develop::BrushStroke {
                points: vec![lightcraft_geom::Point::new(0.5, 0.5)],
                size: 0.2,
                feather: 0.0,
                ..Default::default()
            }],
        },
    });
    m.refine = 0.0;
    assert_eq!(crate::masks::scalar_alpha(&m), None);
    let src = RgbFrame::from_fn(32, 16, |_, _| [0.1; 3]);
    let frame = crate::geometry::Frame::new(32, 16, &DevelopSettings::default(), true);
    let alpha = crate::masks::evaluate(&[m], &frame, 32, 16, &src, &src.map(crate::local::log_lum), 0.0);
    assert!(alpha[0].alpha.plane().is_some());
    assert!(alpha[0].alpha.get(16, 8) < 0.01);
    assert_eq!(alpha[0].alpha.get(0, 0), 0.5);
}

#[test]
fn stacked_spatial_overlay_is_identical_with_warm_prefixes() {
    let src = std::sync::Arc::new(scene());
    let info = SourceInfo { raw: true, ..Default::default() };
    let mut s = left_layer(json!({"mixer":{"red":{"hue":60.0}}}), 80.0);
    let mut top = s.masks[0].clone();
    top.id = 2;
    top.tools = LayerTools::default().merged((6500.0, 0.0), &json!({"light":{"exposure":-0.5}})).unwrap();
    s.masks.push(top);
    let cache = crate::StageCache::default();
    let req = RenderRequest {
        overlay: crate::Overlay::Mask { id: 2, view: crate::MaskView::WhiteOnBlack, color: [255, 0, 0], opacity: 50 },
        ..RenderRequest::fit(W, H)
    };
    let cold = crate::render_cached(&src, &info, &s, &req, &cache).image;
    let warm = crate::render_cached(&src, &info, &s, &req, &cache).image;
    assert!(cold.data == warm.data);
    assert_eq!(cache.stack_renders(), (0, 0));
}

#[test]
fn layer_whites_and_blacks_respond_in_the_developed_display_range() {
    use std::sync::Arc;
    for (input, name, tool) in [(0.65, "whites", json!({"light":{"whites":80.0}})), (0.005, "blacks", json!({"light":{"blacks":-80.0}}))] {
        let src = Arc::new(RgbFrame::from_fn(16, 16, |_, _| [input; 3]));
        let info = SourceInfo::default();
        let req = RenderRequest::fit(16, 16);
        let cache = crate::StageCache::default();
        let mut s = full_layer(tool);
        s.masks[0].components[0].shape = MaskShape::Full;
        let image = crate::render_cached(&src, &info, &s, &req, &cache).image;
        let base = render(&src, &info, &DevelopSettings::default(), &req).image;
        if name == "whites" {
            assert!(image.data[0][0] > base.data[0][0] + 5);
        } else {
            assert!(image.data[0][0] + 3 < base.data[0][0]);
        }
        assert!(image.data.iter().all(|c| c[0] == c[1] && c[1] == c[2]));
    }
}

#[test]
fn dsfix_scene_finish_retains_background_range_overlay_after_stack() {
    use lightcraft_develop::{Mask, MaskComponent, MaskShape};
    use std::sync::Arc;
    let src = RgbFrame::from_fn(32, 16, |x, _| [0.01 + x as f32 / 64.0; 3]);
    let info = SourceInfo::default();
    let mut settings = full_layer(json!({"light":{"exposure":3.0}}));
    settings.masks.push(Mask {
        id: 42,
        components: vec![MaskComponent {
            shape: MaskShape::LuminanceRange { lo: 0.0, hi: 0.4, lo_feather: 0.01, hi_feather: 0.01 },
            name: None,
            op: Default::default(),
            invert: false,
        }],
        adjust: lightcraft_develop::LocalAdjustments { exposure: 0.5, ..Default::default() },
        ..Default::default()
    });
    let req = RenderRequest {
        overlay: crate::Overlay::Mask { id: 42, view: crate::MaskView::WhiteOnBlack, color: [255, 0, 0], opacity: 100 },
        ..RenderRequest::fit(32, 16)
    };
    let plan = crate::plan(&src, &info, &settings, &req);
    let sampled = Arc::new(plan.frame.sample(&src, plan.w, plan.h));
    let gpu_finish = crate::finish_scene_stage(sampled, &plan, &info, &req).image;
    let cpu = crate::render(&src, &info, &settings, &req).image;
    assert_eq!(gpu_finish, cpu, "the scene-stage finishing path must reuse the Background mask evaluated before the bright layer");
}
