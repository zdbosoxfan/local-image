use lightcraft_develop::{DevelopSettings, Treatment};
use photocraft_algo::camera_raw::CameraRaw;
use photocraft_cms::{Builtin, Profile};
use photocraft_color::{ColorMode, PixelFormat, SampleType};
use photocraft_geom::Rect;
use photocraft_io::develop_filter::{
    Conv, beyond_camera_raw, develop_surface, develop_surface_always, filter_settings, from_camera_raw, identity, identity_json, kelvin_to_rel, parse_settings,
    rel_to_kelvin, render_working, sanitize, surface_to_working, to_camera_raw,
};
use photocraft_raster::{Surface, from_rgba_into};
use serde_json::json;

// ----------------------------------------------------------------------------- helpers

fn pattern(fmt: PixelFormat, w: i32, h: i32) -> Surface {
    let mut s = Surface::new(fmt);
    let mut data = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let (r, g, b) = ((x * 7 % 23) as f32 / 22.0, (y * 5 % 19) as f32 / 18.0, ((x + y) % 11) as f32 / 10.0);
            let a = if x < 3 { 0.0 } else { (x % 5) as f32 / 4.0 * 0.5 + 0.5 };
            let mut o = vec![0.0; fmt.channels()];
            from_rgba_into(&fmt, [r, g, b, a], &mut o);
            data.extend(o);
        }
    }
    s.write_region(Rect::new(0, 0, w, h), &data);
    s
}

fn profiles() -> Vec<(&'static str, Profile)> {
    vec![
        ("sRGB", Builtin::Srgb.profile().clone()),
        ("Adobe RGB", Builtin::AdobeRgbCompat.profile().clone()),
        ("ProPhoto", Builtin::ProPhotoCompat.profile().clone()),
        ("Display P3", Builtin::DisplayP3.profile().clone()),
        ("linear sRGB", Builtin::LinearSrgb.profile().clone()),
    ]
}

fn default_camera_raw() -> CameraRaw {
    CameraRaw::default()
}

// ----------------------------------------------------------------------------- tests

#[test]
fn parse_settings_defaults_and_meta_ignore() {
    let empty = json!({});
    let s = parse_settings(&empty).expect("empty object should parse");
    assert_eq!(s, DevelopSettings::default());

    let with_meta = json!({
        "layer": 3,
        "__private": "xyz",
        "index": 7,
        "target": "foo",
        "light": {"exposure": 0.5}
    });
    let s = filter_settings(&with_meta).expect("meta keys ignored");
    assert_eq!(s.light.exposure, 0.5);
    // sanitize also disables some tools
    assert!(!s.optics.lens_profile);
    assert_eq!(s.crop, DevelopSettings::default().crop);
    assert_eq!(s.geometry, DevelopSettings::default().geometry);
    assert_eq!(s.calibration, DevelopSettings::default().calibration);
    assert_eq!(s.enhance, DevelopSettings::default().enhance);
}

#[test]
fn parse_settings_invalid_type_errors() {
    // non-object, non-null
    let invalid = json!(42);
    assert!(parse_settings(&invalid).is_err());

    // wrong type for a nested field
    let wrong_type = json!({"light": {"exposure": "bright"}});
    assert!(parse_settings(&wrong_type).is_err());

    // malformed curve points
    let bad_curve = json!({"curve": {"master": [[0.0, 0.0], "not a point", [1.0, 1.0]]}});
    assert!(parse_settings(&bad_curve).is_err());
}

#[test]
#[allow(clippy::field_reassign_with_default)]
fn sanitize_disables_frame_and_raw_tools() {
    let mut s = DevelopSettings::default();
    s.crop.flip_h = true;
    s.geometry.rotate = 5.0;
    s.optics.lens_profile = true;
    s.enhance.super_resolution = true;
    s.calibration.shadows_tint = 10.0;
    s.light.exposure = 0.5;
    s.negative.enabled = true;

    sanitize(&mut s);

    assert!(!s.crop.flip_h);
    assert_eq!(s.geometry.rotate, 0.0);
    assert!(!s.optics.lens_profile);
    assert!(!s.enhance.super_resolution);
    assert_eq!(s.calibration, DevelopSettings::default().calibration);
    // tools a filter can use stay
    assert_eq!(s.light.exposure, 0.5);
    assert!(s.negative.enabled);
}

#[test]
fn identity_settings_are_sanitized_defaults() {
    let mut expected = DevelopSettings::default();
    sanitize(&mut expected);
    assert_eq!(*identity(), expected);
}

#[test]
fn identity_json_round_trips() {
    let json = identity_json();
    let parsed = parse_settings(&json).expect("identity_json should parse");
    assert_eq!(parsed, *identity());
}

#[test]
fn rel_to_kelvin_and_back() {
    for r in -100..=100 {
        let r = r as f64;
        let k = rel_to_kelvin(r);
        let back = kelvin_to_rel(k);
        assert!((back - r).abs() < 0.01, "round trip failed for r={r}: back={back}");
    }
    // clamping
    assert_eq!(rel_to_kelvin(200.0), rel_to_kelvin(100.0));
    assert_eq!(rel_to_kelvin(-200.0), rel_to_kelvin(-100.0));
    assert_eq!(kelvin_to_rel(0.0), -100.0);
    assert!(kelvin_to_rel(1e9) > 99.9);
}

#[test]
fn camera_raw_subset_round_trip_exactly() {
    let cr: CameraRaw = serde_json::from_value(json!({
        "exposure": 1.15, "contrast": -38, "highlights": 35, "shadows": -50, "whites": 49, "blacks": -34,
        "curveDarks": -21, "curveLights": 38, "hslHue": [-50, 0, 29, 1, 0, 0, 0, 0],
        "temperature": -26, "tint": -25, "texture": -31, "clarity": 30, "dehaze": -34, "vibrance": 33, "saturation": 33,
        "sharpenAmount": 43, "sharpenRadius": 1.2, "sharpenDetail": 36, "sharpenMasking": 30,
        "noiseLuminance": 47, "noiseColor": 25,
        "gradeShadows": {"hue": 117, "sat": 52, "lum": -24}, "gradeGlobal": {"hue": 20, "sat": 61, "lum": 45},
        "gradeBlending": 36, "gradeBalance": 35, "grainAmount": 45, "vignetteAmount": 31, "vignetteStyle": "colorPriority",
        "pointCurve": [[0, 0], [146, 102], [255, 255]], "pointCurveBlue": [[0, 0], [129, 202], [255, 255]]
    }))
    .expect("valid camera raw JSON");

    let s = from_camera_raw(&cr, &DevelopSettings::default());
    assert_eq!(to_camera_raw(&s), cr);
    assert!(!beyond_camera_raw(&s));
    assert_eq!(to_camera_raw(&DevelopSettings::default()), default_camera_raw());
}

#[test]
#[allow(clippy::field_reassign_with_default)]
fn beyond_camera_raw_detects_unsupported() {
    let mut s = DevelopSettings::default();
    s.treatment = Treatment::Bw;
    assert!(beyond_camera_raw(&s));

    let mut s2 = DevelopSettings::default();
    s2.negative.enabled = true;
    assert!(beyond_camera_raw(&s2));

    let s3 = from_camera_raw(&default_camera_raw(), &DevelopSettings::default());
    assert!(!beyond_camera_raw(&s3));
}

#[test]
fn develop_surface_identity_shortcut_matches_original() {
    let area = Rect::new(0, 0, 40, 24);
    for (name, profile) in profiles() {
        for sample in [SampleType::U8, SampleType::U16, SampleType::F32] {
            let fmt = PixelFormat::new(ColorMode::Rgb, sample, true);
            let src = pattern(fmt, 40, 24);
            let out = develop_surface(&src, area, identity(), &profile).expect("identity should not fail");
            assert_eq!(out.read_region(area), src.read_region(area), "{name} {sample:?}: shortcut identity not exact");
        }
    }
}

#[test]
fn develop_surface_full_pipeline_identity_is_close() {
    let area = Rect::new(0, 0, 40, 24);
    let profile = Builtin::Srgb.profile();
    let fmt = PixelFormat::new(ColorMode::Rgb, SampleType::U16, true);
    let src = pattern(fmt, 40, 24);
    let out = develop_surface_always(&src, area, identity(), profile).expect("full identity should not fail");
    let a = src.read_region(area);
    let b = out.read_region(area);
    let mut worst = 0.0f32;
    for (p, q) in a.as_chunks::<4>().0.iter().zip(b.as_chunks::<4>().0.iter()) {
        assert_eq!(p[3].to_bits(), q[3].to_bits(), "alpha changed");
        for c in 0..3 {
            worst = worst.max((p[c] - q[c]).abs());
        }
    }
    assert!(worst <= 1.0 / 255.0, "identity moved a channel by {worst}");
}

#[test]
fn develop_surface_exposure_brightens_and_alpha_unchanged() {
    let fmt = PixelFormat::new(ColorMode::Rgb, SampleType::U16, true);
    let src = pattern(fmt, 32, 16);
    let area = Rect::new(0, 0, 32, 16);
    let mut s = identity().clone();
    s.light.exposure = 1.0;
    let out = develop_surface(&src, area, &s, Builtin::Srgb.profile()).expect("exposure develop failed");
    let a = src.read_region(area);
    let b = out.read_region(area);
    let mean = |v: &[f32]| v.as_chunks::<4>().0.iter().map(|p| p[1]).sum::<f32>() / (v.len() / 4) as f32;
    assert!(mean(&b) > mean(&a) + 0.05);
    for (p, q) in a.as_chunks::<4>().0.iter().zip(b.as_chunks::<4>().0.iter()) {
        assert_eq!(p[3].to_bits(), q[3].to_bits(), "alpha must be unchanged");
    }
}

#[test]
fn render_working_empty_or_mismatched_errors() {
    let s = identity();
    assert!(render_working(Vec::new(), 0, 1, s).is_err());
    assert!(render_working(Vec::new(), 1, 0, s).is_err());
    assert!(render_working(vec![[0.0; 3]; 5], 3, 2, s).is_err());
    assert!(render_working(Vec::new(), 2, 2, s).is_err());
}

#[test]
fn render_working_identity_is_finite_and_close() {
    let w = 16;
    let h = 8;
    let mut rgb = Vec::with_capacity(w * h);
    for y in 0..h {
        for x in 0..w {
            rgb.push([(x as f32 / (w - 1) as f32) * 0.8, (y as f32 / (h - 1) as f32) * 0.8, 0.5]);
        }
    }
    let out = render_working(rgb.clone(), w, h, identity()).expect("render should succeed");
    assert_eq!(out.len(), w * h);
    let mut worst = 0.0f32;
    for (a, b) in rgb.iter().zip(out.iter()) {
        for c in 0..3 {
            assert!(a[c].is_finite() && b[c].is_finite());
            worst = worst.max((a[c] - b[c]).abs());
        }
    }
    assert!(worst < 5e-3, "identity render_working moved by {worst}");
}

#[test]
fn surface_to_working_alpha_and_dimensions() {
    let fmt = PixelFormat::new(ColorMode::Rgb, SampleType::U8, true);
    let src = pattern(fmt, 20, 10);
    let area = Rect::new(0, 0, 20, 10);
    let profile = Builtin::Srgb.profile();
    let (rgb, alpha) = surface_to_working(&src, area, profile).expect("conversion failed");
    assert_eq!(rgb.len(), 200);
    assert_eq!(alpha.len(), 200);
    let orig = src.read_region(area);
    for (i, a) in alpha.iter().enumerate() {
        let orig_a = orig[i * 4 + 3];
        assert!((a - orig_a).abs() < 1e-5, "alpha mismatch at {i}");
    }
    for p in rgb.iter() {
        assert!(p.iter().all(|v| v.is_finite()));
    }
}

#[test]
fn develop_surface_always_too_large_errors() {
    let fmt = PixelFormat::new(ColorMode::Rgb, SampleType::U8, true);
    let src = pattern(fmt, 4, 4);
    let huge_area = Rect::new(0, 0, 100_000, 100_000);
    assert!(develop_surface_always(&src, huge_area, identity(), Builtin::Srgb.profile()).is_err());
}

#[test]
fn develop_surface_deterministic() {
    let fmt = PixelFormat::new(ColorMode::Rgb, SampleType::U16, true);
    let src = pattern(fmt, 32, 24);
    let area = Rect::new(0, 0, 32, 24);
    let mut s = identity().clone();
    s.light.exposure = 0.7;
    s.color.vibrance = 20.0;
    s.effects.clarity = 15.0;
    let profile = Builtin::ProPhotoCompat.profile();
    let out1 = develop_surface(&src, area, &s, profile).expect("first run");
    let out2 = develop_surface(&src, area, &s, profile).expect("second run");
    assert_eq!(out1.read_region(area), out2.read_region(area));
}

#[test]
fn conv_new_roundtrip_for_matrix_profiles() {
    for (name, profile) in profiles() {
        let conv = Conv::new(&profile).unwrap_or_else(|_| panic!("Conv::new failed for {name}"));
        let input = [[0.25, 0.5, 0.75], [0.1, 0.2, 0.9], [0.0, 0.0, 1.0]];
        let mut working = input;
        conv.to_working(&mut working);
        conv.from_working(&mut working);
        for (orig, round) in input.iter().zip(working.iter()) {
            for c in 0..3 {
                assert!((orig[c] - round[c]).abs() < 1e-4, "{name}: roundtrip error on channel {c}: {} -> {}", orig[c], round[c]);
            }
        }
    }
}

#[test]
fn filter_settings_applies_sanitize() {
    let params = json!({
        "crop": {"flip_h": true},
        "geometry": {"rotate": 90.0},
        "optics": {"lens_profile": true},
        "enhance": {"super_resolution": true},
        "light": {"exposure": 0.3}
    });
    let s = filter_settings(&params).expect("filter_settings failed");
    assert!(!s.crop.flip_h);
    assert_eq!(s.geometry.rotate, 0.0);
    assert!(!s.optics.lens_profile);
    assert!(!s.enhance.super_resolution);
    assert_eq!(s.light.exposure, 0.3);
}

#[test]
fn grayscale_profile_works_with_identity() {
    let fmt = PixelFormat::new(ColorMode::Grayscale, SampleType::U8, false);
    let src = pattern(fmt, 16, 8);
    let profile = Builtin::SGray.profile().gray_as_rgb().expect("gray profile");
    let area = Rect::new(0, 0, 16, 8);
    let out = develop_surface(&src, area, identity(), &profile).expect("gray develop failed");
    assert_eq!(out.read_region(area), src.read_region(area));
}
