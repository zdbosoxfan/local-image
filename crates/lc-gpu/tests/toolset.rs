//! CPU ↔ GPU equivalence for the toolset upgrades (docs/DEVELOP-DESIGN.md §4.1): lens profiles
//! from the lens database, the tone equalizer, colour calibration, capture sharpening, depth
//! masks, film looks and AI Remove patches. Same bounds as `equivalence.rs` (mean |Δ| < 0.5 LSB,
//! max |Δ| ≤ 3 LSB per channel). Tools the GPU has no kernel for must come back as `None` with
//! the reason recorded (the caller renders on the CPU). Skips (passes with a note) when no GPU
//! adapter exists, e.g. on CI.

use std::sync::Arc;

use lightcraft_develop::{
    Adaptation, AiPatch, ColorCal, DevelopSettings, Illuminant, LocalAdjustments, Mask, MaskComponent, MaskOp, MaskShape, SegMask, Spot, SpotMode,
};
use lightcraft_geom::{Orientation, Point, Rect};
use lightcraft_pipeline::lensdb::{Distortion, LensCorrection, Tca};
use lightcraft_pipeline::{Overlay, RenderRequest, SourceInfo, StageCache, render};
use lightcraft_raster::{Rgb32f, Rgba8};

const MEAN_LSB: f64 = 0.5;
const MAX_LSB: u8 = 3;

fn gpu() -> bool {
    let ok = lightcraft_gpu::available();
    if ok {
        eprintln!("adapter: {}", lightcraft_gpu::adapter_name().unwrap_or_default());
    } else {
        eprintln!("skipped: no GPU adapter");
    }
    ok
}

fn scene(i: usize, w: usize, h: usize) -> Arc<Rgb32f> {
    Arc::new(lightcraft_scenes::demo_library()[i].render(w, h))
}

fn raw() -> SourceInfo {
    SourceInfo { raw: true, as_shot_temp: 5200.0, as_shot_tint: 4.0, ..Default::default() }
}

/// (mean |Δ|, max |Δ|).
fn diff(a: &Rgba8, b: &Rgba8) -> (f64, u8) {
    assert_eq!((a.width, a.height), (b.width, b.height));
    let (mut sum, mut max) = (0u64, 0u8);
    for (p, q) in a.data.iter().zip(&b.data) {
        for c in 0..3 {
            let d = p[c].abs_diff(q[c]);
            sum += d as u64;
            max = max.max(d);
        }
    }
    (sum as f64 / (a.data.len() * 3) as f64, max)
}

fn gpu_render(src: &Arc<Rgb32f>, info: &SourceInfo, s: &DevelopSettings, req: &RenderRequest, stages: Option<&StageCache>) -> Rgba8 {
    lightcraft_gpu::render(src, info, s, req, stages).unwrap_or_else(|| panic!("gpu render ({:?})", lightcraft_gpu::last_fallback())).image
}

fn check(name: &str, src: &Arc<Rgb32f>, info: &SourceInfo, s: &DevelopSettings, req: &RenderRequest) -> Rgba8 {
    let cpu = render(src, info, s, req).image;
    let gpu = gpu_render(src, info, s, req, None);
    let (mean, max) = diff(&cpu, &gpu);
    eprintln!("{name:<44} {}x{}  mean {mean:.4}  max {max}", cpu.width, cpu.height);
    assert!(mean < MEAN_LSB && max <= MAX_LSB, "{name}: mean {mean:.4} LSB, max {max} LSB");
    gpu
}

/// Share of pixels that differ between two renders.
fn changed(a: &Rgba8, b: &Rgba8) -> f64 {
    a.data.iter().zip(&b.data).filter(|(p, q)| p != q).count() as f64 / a.data.len() as f64
}

// ------------------------------------------------------------------------------------------------
// Lens profiles (lens database): geometry resampled on the CPU, the rest on the GPU

fn barrel() -> LensCorrection {
    LensCorrection {
        diag_norm: 36f64.hypot(24.0) / 24.0,
        center: [0.0, 0.0],
        distortion: Distortion::Poly3 { k1: -0.03 },
        tca: Tca::Linear { kr: 1.0008, kb: 0.9992 },
        vignetting: Some([-0.5, 0.1, 0.0]),
    }
}

/// Other lensfun models: poly5 / ptlens distortion, poly3 TCA, an off-centre optical axis.
fn lenses() -> Vec<(&'static str, LensCorrection)> {
    vec![
        ("poly3 + linear tca + vignetting", barrel()),
        (
            "poly5 + poly3 tca",
            LensCorrection {
                diag_norm: 36f64.hypot(24.0) / 35.0,
                center: [0.04, -0.03],
                distortion: Distortion::Poly5 { k1: 0.02, k2: -0.004 },
                tca: Tca::Poly3 { red: [1.0004, 0.0002, -0.0003], blue: [0.9996, -0.0001, 0.0002] },
                vignetting: Some([-0.3, -0.05, 0.02]),
            },
        ),
        (
            "ptlens, no tca",
            LensCorrection {
                diag_norm: 36f64.hypot(24.0) / 18.0,
                center: [0.0, 0.0],
                distortion: Distortion::Ptlens { a: 0.01, b: -0.03, c: 0.005 },
                tca: Tca::None,
                vignetting: None,
            },
        ),
        (
            "vignetting only",
            LensCorrection { diag_norm: 1.5, center: [0.0, 0.0], distortion: Distortion::None, tca: Tca::None, vignetting: Some([-0.6, 0.2, -0.05]) },
        ),
    ]
}

fn lens_on(s: &mut DevelopSettings, strength: f64) {
    s.lens_db.enabled = true;
    s.lens_db.distortion = strength;
    s.lens_db.tca = strength;
    s.lens_db.vignetting = strength;
}

#[test]
fn lens_profiles_match() {
    if !gpu() {
        return;
    }
    let src = scene(2, 900, 600);
    let req = RenderRequest::fit(720, 720);
    for (name, c) in lenses() {
        let info = SourceInfo { lens_db: Some(c), ..raw() };
        let mut prev: Option<Rgba8> = None;
        for strength in [0.0, 100.0, 200.0] {
            let mut s = DevelopSettings::default();
            lens_on(&mut s, strength);
            let g = check(&format!("lens {name} {strength}%"), &src, &info, &s, &req);
            if let Some(p) = &prev {
                assert!(changed(p, &g) > 0.05, "{name}: {strength}% changes the render");
            }
            prev = Some(g);
        }
        // each part on its own at full and double strength
        for (part, set) in [
            ("distortion", (|s, v| s.lens_db.distortion = v) as fn(&mut DevelopSettings, f64)),
            ("tca", |s, v| s.lens_db.tca = v),
            ("vignetting", |s, v| s.lens_db.vignetting = v),
        ] {
            for v in [100.0, 200.0] {
                let mut s = DevelopSettings::default();
                lens_on(&mut s, 0.0);
                set(&mut s, v);
                check(&format!("lens {name} {part} {v}%"), &src, &info, &s, &req);
            }
        }
    }
}

#[test]
fn lens_profiles_with_crop_rotation_and_perspective_match() {
    if !gpu() {
        return;
    }
    let src = scene(2, 900, 600);
    let info = SourceInfo { lens_db: Some(barrel()), ..raw() };
    for o in [Orientation::Normal, Orientation::Rotate90, Orientation::Rotate270, Orientation::FlipH] {
        for strength in [0.0, 100.0, 200.0] {
            let mut s = DevelopSettings { orientation: o, ..Default::default() };
            lens_on(&mut s, strength);
            s.crop.geometry.rect = Rect::new(0.1, 0.05, 0.85, 0.9);
            s.crop.geometry.angle = 6.5;
            s.crop.flip_v = true;
            check(&format!("lens crop+rotate {o:?} {strength}%"), &src, &info, &s, &RenderRequest::fit(640, 640));
            s.geometry.vertical = 20.0;
            s.geometry.constrain_crop = true;
            check(&format!("lens crop+rotate+persp {o:?} {strength}%"), &src, &info, &s, &RenderRequest::fit(640, 640));
        }
    }
    // with the rest of the pipeline and a mask on top (masks are pushed through the warp)
    let mut s = DevelopSettings::default();
    lens_on(&mut s, 100.0);
    s.light.exposure = 0.4;
    s.effects.clarity = 20.0;
    s.detail.nr_luminance = 30.0;
    s.optics.ca_red = 30.0;
    s.masks = vec![Mask {
        components: vec![MaskComponent {
            name: None,
            op: MaskOp::Add,
            invert: false,
            shape: MaskShape::Radial { center: Point::new(0.3, 0.3), rx: 0.2, ry: 0.15, angle: 10.0, feather: 50.0, invert: false },
        }],
        adjust: LocalAdjustments { exposure: -0.7, ..Default::default() },
        ..Default::default()
    }];
    check("lens + manual ca + edits + mask", &src, &info, &s, &RenderRequest::fit(720, 720));
}

#[test]
fn lens_profile_strength_changes_reach_cached_renders() {
    if !gpu() {
        return;
    }
    // A strength drag changes the geometry: a cached view must re-sample, not reuse the old frame.
    let src = scene(1, 900, 600);
    let info = SourceInfo { lens_db: Some(barrel()), ..raw() };
    let cache = StageCache::default();
    let req = RenderRequest::fit(640, 640);
    for strength in [100.0, 0.0, 200.0, 100.0] {
        let mut s = DevelopSettings::default();
        lens_on(&mut s, strength);
        let warm = gpu_render(&src, &info, &s, &req, Some(&cache));
        let fresh = gpu_render(&src, &info, &s, &req, None);
        assert_eq!(warm, fresh, "cached render at {strength}%");
    }
}

// ------------------------------------------------------------------------------------------------
// Tone equalizer: no kernel, a clean CPU fallback

#[test]
fn tone_equalizer_falls_back_to_the_cpu() {
    if !lightcraft_gpu::enabled() {
        eprintln!("skipped: GPU rendering disabled ({:?})", lightcraft_gpu::unavailable_reason());
        return;
    }
    let src = scene(0, 640, 427);
    let info = raw();
    let mut s = DevelopSettings::default();
    s.tone_eq.enabled = true;
    s.tone_eq.ev6 = 1.2;
    s.tone_eq.ev5 = 0.8;
    s.tone_eq.ev1 = -0.6;
    let req = RenderRequest::fit(640, 640);
    assert!(lightcraft_pipeline::tools_need_cpu(&s, &req));
    assert!(lightcraft_gpu::render(&src, &info, &s, &req, None).is_none());
    let why = lightcraft_gpu::last_fallback().unwrap_or_default();
    assert!(why.contains("tone equalizer"), "{why}");
    // the Show Mask overlay: CPU too, even with every zone at 0 (the mask is still shown)
    let mut shown = s.clone();
    shown.tone_eq.ev6 = 0.0;
    shown.tone_eq.ev5 = 0.0;
    shown.tone_eq.ev1 = 0.0;
    let mreq = RenderRequest { overlay: Overlay::ToneEqMask, ..req };
    assert!(lightcraft_pipeline::tools_need_cpu(&shown, &mreq));
    assert!(lightcraft_gpu::render(&src, &info, &shown, &mreq, None).is_none());
    let mask = render(&src, &info, &shown, &mreq).image;
    assert!(mask.data.iter().all(|p| p[0] == p[1] && p[1] == p[2]), "the mask is drawn in grey");
    let levels = mask.data.iter().map(|p| p[0]).fold((255u8, 0u8), |(lo, hi), v| (lo.min(v), hi.max(v)));
    assert!(levels.1 > levels.0 + 40, "the mask follows the image: {levels:?}");
    // off (or with the section turned off): the GPU renders again
    s.set_section_enabled("toneEq", false);
    assert!(!lightcraft_pipeline::tools_need_cpu(&s, &req));
    if lightcraft_gpu::available() {
        check("tone eq section off", &src, &info, &s, &req);
    }
}

// ------------------------------------------------------------------------------------------------
// Colour calibration: linear cases on the GPU, the rest through the CPU's linear stage

fn cal(adaptation: Adaptation, illuminant: Illuminant, gamut: f64, clip: bool) -> DevelopSettings {
    let mut s =
        DevelopSettings { color_cal: ColorCal { enabled: true, adaptation, illuminant, gamut, clip, ..Default::default() }, ..Default::default() };
    if illuminant == Illuminant::Custom {
        s.color_cal.x = 0.36;
        s.color_cal.y = 0.37;
    }
    s
}

#[test]
fn color_calibration_matches() {
    if !gpu() {
        return;
    }
    let src = scene(0, 900, 600);
    let req = RenderRequest::fit(720, 720);
    use Adaptation::*;
    use Illuminant as I;
    for info in [raw(), SourceInfo::default()] {
        let kind = if info.raw { "raw" } else { "rendered" };
        let reference = render(&src, &info, &DevelopSettings::default(), &req).image;
        // linear: folded into the white-balance matrix
        for a in [Cat16, Bradford, Xyz] {
            for il in [I::WhiteBalance, I::A, I::D50, I::D75, I::F11, I::Custom] {
                let s = cal(a, il, 0.0, false);
                assert!(!lightcraft_pipeline::lin_needs_cpu(&s), "{a:?} {il:?} is linear");
                let g = check(&format!("cal {kind} linear {a:?} {il:?}"), &src, &info, &s, &req);
                if il != I::WhiteBalance && il != I::D75 {
                    assert!(changed(&reference, &g) > 0.05, "{a:?} {il:?} changes the render");
                }
            }
        }
        // non-linear: the CPU's linear stage, then the GPU
        for (name, s) in [
            ("full bradford", cal(FullBradford, I::A, 0.0, false)),
            ("gamut 1", cal(Cat16, I::F11, 1.0, false)),
            ("gamut 0.3", cal(Bradford, I::D50, 0.3, false)),
            ("clip", cal(Xyz, I::A, 0.0, true)),
            ("defaults (gamut 1 + clip)", cal(Cat16, I::WhiteBalance, 1.0, true)),
            ("full bradford + gamut + clip", cal(FullBradford, I::Custom, 0.6, true)),
        ] {
            assert!(lightcraft_pipeline::lin_needs_cpu(&s), "{name} takes the CPU's linear stage");
            check(&format!("cal {kind} mixed {name}"), &src, &info, &s, &req);
        }
    }
    // with white balance, the rest of the pipeline, masks and a saturated scene
    let mut s = cal(Cat16, Illuminant::A, 0.0, false);
    s.wb.mode = lightcraft_develop::WbMode::Custom;
    s.wb.temp = 3800.0;
    s.wb.tint = -10.0;
    s.color.saturation = 60.0;
    s.effects.dehaze = 20.0;
    check("cal linear + wb + edits", &src, &raw(), &s, &req);
    s.color_cal.gamut = 0.8;
    s.color_cal.clip = true;
    check("cal mixed + wb + edits", &src, &raw(), &s, &req);
    let sat = scene(7, 900, 600);
    check("cal mixed, saturated scene", &sat, &raw(), &cal(FullBradford, Illuminant::F2, 1.0, true), &req);
}

#[test]
fn color_calibration_changes_reach_cached_renders() {
    if !gpu() {
        return;
    }
    // Switching between the linear (GPU) and mixed (CPU linear stage) paths on one view.
    let src = scene(0, 900, 600);
    let info = raw();
    let cache = StageCache::default();
    let req = RenderRequest::fit(640, 640);
    for (a, il, gamut, clip) in [
        (Adaptation::Cat16, Illuminant::A, 0.0, false),
        (Adaptation::Cat16, Illuminant::A, 0.5, false),
        (Adaptation::FullBradford, Illuminant::A, 0.0, false),
        (Adaptation::Bradford, Illuminant::F11, 0.0, false),
        (Adaptation::Bradford, Illuminant::F11, 0.0, true),
        (Adaptation::Cat16, Illuminant::A, 0.0, false),
    ] {
        let s = cal(a, il, gamut, clip);
        let warm = gpu_render(&src, &info, &s, &req, Some(&cache));
        let fresh = gpu_render(&src, &info, &s, &req, None);
        assert_eq!(warm, fresh, "{a:?} {il:?} gamut {gamut} clip {clip}");
    }
}

// ------------------------------------------------------------------------------------------------
// Capture sharpening: sharpen the uploaded source before geometry; keep it resident

fn capture(s: &mut DevelopSettings, radius: f64, threshold: f64, iterations: f64) {
    s.raw.capture.enabled = true;
    s.raw.capture.radius = radius;
    s.raw.capture.threshold = threshold;
    s.raw.capture.iterations = iterations;
}

#[test]
fn capture_sharpening_matches() {
    if !gpu() {
        return;
    }
    let src = scene(3, 900, 600);
    let info = SourceInfo { capture_radius: Some(0.8), ..raw() };
    let req = RenderRequest::fit(900, 900);
    let plain = render(&src, &info, &DevelopSettings::default(), &req).image;
    for (r, t, i) in [(0.0, 0.0, 8.0), (0.7, 0.0, 8.0), (1.2, 30.0, 20.0)] {
        let mut s = DevelopSettings::default();
        capture(&mut s, r, t, i);
        s.raw.capture.corner_boost = 30.0;
        let g = check(&format!("capture r={r} t={t} it={i}"), &src, &info, &s, &req);
        assert!(changed(&plain, &g) > 0.05, "capture sharpening changes the render");
    }
    // a binned preview scales the radius
    let small = scene(3, 450, 300);
    let mut s = DevelopSettings::default();
    capture(&mut s, 1.2, 0.0, 10.0);
    check("capture, binned preview", &small, &SourceInfo { sensor_scale: 2.0, ..info.clone() }, &s, &RenderRequest::fit(450, 450));
    // not for rendered sources
    check("capture on a rendered source", &src, &SourceInfo::default(), &s, &req);
}

#[test]
fn capture_sharpening_changes_re_upload_the_source() {
    if !gpu() {
        return;
    }
    // Render, change one capture value, render again on the same view: the device's sharpened
    // source and downstream stages must be rebuilt (the original upload can now stay resident).
    let src = scene(3, 900, 600);
    let info = SourceInfo { capture_radius: Some(0.8), ..raw() };
    let cache = StageCache::default();
    let req = RenderRequest::fit(640, 640);
    let mut s = DevelopSettings::default();
    s.light.exposure = 0.2;
    s.effects.clarity = 15.0;
    capture(&mut s, 0.8, 10.0, 8.0);
    let first = gpu_render(&src, &info, &s, &req, Some(&cache));
    assert_eq!(first, gpu_render(&src, &info, &s, &req, None));
    let edits: [(&str, fn(&mut DevelopSettings)); 6] = [
        ("radius", |s| s.raw.capture.radius = 1.4),
        ("threshold", |s| s.raw.capture.threshold = 60.0),
        ("iterations", |s| s.raw.capture.iterations = 25.0),
        ("corner boost", |s| s.raw.capture.corner_boost = 80.0),
        ("off", |s| s.raw.capture.enabled = false),
        ("on again", |s| s.raw.capture.enabled = true),
    ];
    let mut prev = first.clone();
    for (what, edit) in edits {
        edit(&mut s);
        let warm = gpu_render(&src, &info, &s, &req, Some(&cache));
        let fresh = gpu_render(&src, &info, &s, &req, None);
        let cpu = render(&src, &info, &s, &req).image;
        assert_eq!(warm, fresh, "{what}: the cached view equals a fresh render");
        let (mean, max) = diff(&cpu, &warm);
        eprintln!("capture edit {what:<34} changed {:.2}%  vs cpu mean {mean:.4} max {max}", changed(&prev, &warm) * 100.0);
        assert!(mean < MEAN_LSB && max <= MAX_LSB, "{what}: mean {mean:.4} LSB, max {max} LSB");
        assert!(changed(&prev, &warm) > 0.01, "{what}: the render changed");
        prev = warm;
    }
    // and back to the first values: the first image
    capture(&mut s, 0.8, 10.0, 8.0);
    s.raw.capture.corner_boost = 0.0;
    assert_eq!(gpu_render(&src, &info, &s, &req, Some(&cache)), first, "back to the first values");
}

/// Soft edges, fine colour texture, black/negative pixels and HDR near every border.
fn capture_chart(w: usize, h: usize) -> Arc<Rgb32f> {
    Arc::new(Rgb32f::from_fn(w, h, |x, y| {
        let fx = x as f32;
        let fy = y as f32;
        let edge = 0.5 + 0.5 * ((fx - 0.43 * w as f32) * 0.9 + fy * 0.04).tanh();
        let texture = 0.015 * (fx * 1.4 + fy * 0.7).sin();
        let v = 0.035 + 0.6 * edge + texture;
        if (x < 4 && y < h / 2) || (x == w - 5 && y == h - 5) {
            [-0.01, 0.0, 0.005]
        } else if y < 3 || (x > w.saturating_sub(8) && y > h / 2) {
            [2.0, 1.2, 0.8]
        } else {
            [v * 0.8 + texture, v, v * 1.15 - texture]
        }
    }))
}

#[test]
fn capture_sharpening_parameter_sweeps_match() {
    if !gpu() {
        return;
    }
    let src = capture_chart(193, 129);
    let info = SourceInfo { capture_radius: Some(0.8), capture_threshold: 0.32, ..raw() };
    let req = RenderRequest::fit(193, 193);
    let mut s = DevelopSettings::default();
    capture(&mut s, 0.8, 25.0, 8.0);
    for radius in [0.0, 0.25, 0.26, 0.3, 0.5, 0.65, 0.66, 0.67, 1.0, 1.5, 2.55, 3.0] {
        s.raw.capture.radius = radius;
        check(&format!("capture radius {radius}"), &src, &info, &s, &req);
    }
    s.raw.capture.radius = 0.8;
    for threshold in [0.0, 1.0, 10.0, 40.0, 80.0, 100.0] {
        s.raw.capture.threshold = threshold;
        check(&format!("capture threshold {threshold}"), &src, &info, &s, &req);
    }
    s.raw.capture.threshold = 25.0;
    for iterations in [1.0, 2.0, 8.0, 15.0, 25.0] {
        s.raw.capture.iterations = iterations;
        check(&format!("capture iterations {iterations}"), &src, &info, &s, &req);
    }
    s.raw.capture.iterations = 8.0;
    for boost in [0.0, 10.0, 50.0, 100.0, 150.0] {
        s.raw.capture.corner_boost = boost;
        check(&format!("capture corner boost {boost}"), &src, &info, &s, &req);
    }
}

#[test]
fn capture_sharpening_binned_previews_edges_and_borders_match() {
    // Build tiny fixtures even without an adapter so border arithmetic is exercised.
    let fixtures = [(7, 5), (8, 33), (9, 9), (17, 41), (257, 193)].map(|(w, h)| ((w, h), capture_chart(w, h)));
    if !gpu() {
        return;
    }
    for ((w, h), src) in fixtures {
        for scale in [1.0, 2.0, 4.0] {
            let info = SourceInfo { sensor_scale: scale, capture_radius: Some(1.2), ..raw() };
            let mut s = DevelopSettings::default();
            capture(&mut s, 0.0, 10.0, 25.0);
            s.raw.capture.corner_boost = 150.0;
            for edge in [w.max(h), 31] {
                check(&format!("capture {w}x{h} sensor {scale} output {edge}"), &src, &info, &s, &RenderRequest::fit(edge, edge));
            }
        }
    }
}

#[test]
fn capture_sharpening_source_info_and_geometry_changes_invalidate_cache() {
    if !gpu() {
        return;
    }
    let mut src = capture_chart(193, 129);
    let mut info = SourceInfo { capture_radius: Some(0.8), capture_threshold: 0.2, ..raw() };
    let stages = StageCache::default();
    let mut s = DevelopSettings::default();
    capture(&mut s, 0.0, 0.0, 8.0);
    s.raw.capture.corner_boost = 50.0;
    for step in 0..10 {
        match step {
            1 => info.capture_radius = Some(1.4),
            2 => info.capture_threshold = 0.65,
            3 => info.sensor_scale = 2.0,
            4 => s.light.exposure = 0.7,
            5 => src = Arc::new((*src).clone()), // new decoder source, identical dimensions
            6 => info.raw = false,
            7 => info.raw = true,
            8 => s.disabled_sections.push("raw".into()),
            9 => s.disabled_sections.clear(),
            _ => {}
        }
        let req = RenderRequest::fit(if step % 2 == 0 { 193 } else { 97 }, 193);
        let cached = gpu_render(&src, &info, &s, &req, Some(&stages));
        let fresh = check(&format!("capture info/cache step {step}"), &src, &info, &s, &req);
        assert_eq!(cached, fresh, "capture source-info invalidation {step}");
    }
    // Lens-database geometry is CPU-only. It must resample the GPU's sharpened
    // source, including on a cached view, rather than resampling the original.
    info.lens_db = Some(barrel());
    lens_on(&mut s, 100.0);
    let req = RenderRequest::fit(151, 151);
    check("capture before CPU lens geometry", &src, &info, &s, &req);
    assert_eq!(gpu_render(&src, &info, &s, &req, Some(&stages)), gpu_render(&src, &info, &s, &req, None));
}

#[test]
fn capture_sharpening_reuses_device_source_for_other_tools_and_sizes() {
    if !gpu() {
        return;
    }
    let src = capture_chart(193, 129);
    let info = raw();
    let stages = StageCache::default();
    let mut s = DevelopSettings::default();
    capture(&mut s, 0.8, 25.0, 8.0);
    for (edge, exposure, clarity) in [(193, 0.0, 0.0), (97, 0.7, 0.0), (193, 0.7, 25.0), (53, -0.4, 25.0)] {
        s.light.exposure = exposure;
        s.effects.clarity = clarity;
        let req = RenderRequest::fit(edge, edge);
        let cached = gpu_render(&src, &info, &s, &req, Some(&stages));
        let fresh = check(&format!("capture + edits/size {edge}/{exposure}/{clarity}"), &src, &info, &s, &req);
        assert_eq!(cached, fresh);
    }
    // GPU renders must not populate StageCache.pre with a CPU-sharpened copy.
    assert_eq!(stages.bytes(), 0, "all capture and downstream image stages are device-resident");
    assert!(lightcraft_gpu::stage_bytes(&stages) > 0);
    stages.clear();
    assert_eq!(lightcraft_gpu::stage_bytes(&stages), 0);
}

#[test]
fn capture_sharpening_over_limit_source_falls_back_without_device_failure() {
    if !gpu() {
        return;
    }
    let src = capture_chart(257, 193);
    let info = raw();
    let mut s = DevelopSettings::default();
    capture(&mut s, 0.8, 25.0, 8.0);
    let req = RenderRequest::fit(31, 31);
    lightcraft_gpu::inject_fault(lightcraft_gpu::Fault::Limit(64 << 10));
    assert!(lightcraft_gpu::render(&src, &info, &s, &req, None).is_none());
    assert!(lightcraft_gpu::last_fallback().is_some_and(|r| r.contains("capture sharpening") && r.contains("limit")));
    assert!(lightcraft_gpu::available());
    check("capture after source-limit refusal", &src, &info, &s, &req);
}

#[test]
fn capture_sharpening_before_source_analysis_matches() {
    if !gpu() {
        return;
    }
    let src = Arc::new(Rgb32f::from_fn(193, 129, |x, y| {
        let dx = x as f32 - 105.0;
        let dy = y as f32 - 61.0;
        let pupil = (-0.5 * (dx * dx + dy * dy) / 36.0).exp();
        let v = 0.15 + 0.12 * (0.5 + 0.5 * (x as f32 * 0.45 + y as f32 * 0.1).sin());
        [v + 0.45 * pupil, v * (1.0 - 0.8 * pupil), v * (1.0 - 0.8 * pupil)]
    }));
    let info = raw();
    let mut s = DevelopSettings::default();
    capture(&mut s, 1.2, 10.0, 8.0);
    let stages = StageCache::default();
    let req = RenderRequest::fit(193, 193);
    s.red_eye.push(lightcraft_develop::RedEye { center: Point::new(0.55, 0.48), rx: 0.1, ry: 0.1, ..Default::default() });
    for upright in [lightcraft_develop::Upright::Off, lightcraft_develop::Upright::Level] {
        s.geometry.upright = upright;
        let fresh = check(&format!("capture before pupil/Upright {upright:?}"), &src, &info, &s, &req);
        assert_eq!(gpu_render(&src, &info, &s, &req, Some(&stages)), fresh);
    }
}

/// Fresh full render reruns capture; exposure on the same view reuses its device
/// source. RTX command: --test toolset bench_capture_sharpening_24mp -- --ignored --exact --nocapture
#[test]
#[ignore = "24 MP benchmark: run with --release --ignored"]
fn bench_capture_sharpening_24mp() {
    let has_gpu = gpu();
    let (w, h) = (6000, 4000);
    let src = scene(3, w, h);
    let info = raw();
    let req = RenderRequest::fit(w, h);
    let mut s = DevelopSettings::default();
    capture(&mut s, 0.8, 25.0, 8.0);
    s.raw.capture.corner_boost = 30.0;
    let ms = |t: std::time::Instant| t.elapsed().as_secs_f64() * 1e3;
    let stages = StageCache::default();
    if has_gpu {
        let _ = gpu_render(&src, &info, &s, &req, Some(&stages)); // warm device/pool
        let t = std::time::Instant::now();
        let _ = gpu_render(&src, &info, &s, &req, None);
        eprintln!("24 MP capture r0.8 / 8 it / boost30, fresh GPU: {:.1} ms", ms(t));
        s.light.exposure = 0.3;
        let t = std::time::Instant::now();
        let _ = gpu_render(&src, &info, &s, &req, Some(&stages));
        eprintln!("24 MP capture cached + exposure GPU: {:.1} ms", ms(t));
        s.raw.capture.radius = 1.0;
        let t = std::time::Instant::now();
        let _ = gpu_render(&src, &info, &s, &req, Some(&stages));
        eprintln!("24 MP capture radius change GPU (upload reused): {:.1} ms", ms(t));
    }
    capture(&mut s, 0.8, 25.0, 8.0);
    s.light.exposure = 0.0;
    let t = std::time::Instant::now();
    std::hint::black_box(render(&src, &info, &s, &req));
    eprintln!("24 MP capture r0.8 / 8 it / boost30, fresh CPU: {:.1} ms", ms(t));
}

// ------------------------------------------------------------------------------------------------
// Depth masks: the stored depth map, evaluated on the CPU, used by the GPU

fn depth_map(side: usize) -> SegMask {
    // distance rising left (near) → right (far), with a nearer blob in the middle
    let logits: Vec<f32> = (0..side * side)
        .map(|i| {
            let (x, y) = ((i % side) as f32 + 0.5, (i / side) as f32 + 0.5);
            let mut d = x / side as f32;
            let r = ((x - side as f32 * 0.55).powi(2) + (y - side as f32 * 0.5).powi(2)).sqrt() / side as f32;
            if r < 0.15 {
                d *= 0.3;
            }
            let d = d.clamp(0.01, 0.99);
            (d / (1.0 - d)).ln()
        })
        .collect();
    SegMask::from_logits(side, &logits)
}

fn depth_mask(lo: f64, hi: f64, feather: f64, adjust: LocalAdjustments) -> Mask {
    Mask {
        id: 7,
        components: vec![MaskComponent {
            name: None,
            op: MaskOp::Add,
            invert: false,
            shape: MaskShape::DepthRange { lo, hi, feather, seg: Some(depth_map(96)) },
        }],
        adjust,
        ..Default::default()
    }
}

#[test]
fn depth_masks_match() {
    if !gpu() {
        return;
    }
    let src = scene(4, 900, 600);
    let info = raw();
    let req = RenderRequest::fit(720, 720);
    let plain = render(&src, &info, &DevelopSettings::default(), &req).image;
    let adj = LocalAdjustments { exposure: 0.8, saturation: -40.0, clarity: 30.0, ..Default::default() };
    for (lo, hi, feather) in [(0.0, 0.4, 0.1), (0.5, 1.0, 0.0), (0.2, 0.6, 0.3)] {
        let s = DevelopSettings { masks: vec![depth_mask(lo, hi, feather, adj)], ..Default::default() };
        let g = check(&format!("depth band {lo}..{hi} feather {feather}"), &src, &info, &s, &req);
        let c = changed(&plain, &g);
        assert!(c > 0.05 && c < 0.95, "{lo}..{hi}: only the band changes ({:.1}%)", c * 100.0);
    }
    // combined with other components, inverted, an opacity, crop + rotation + orientation
    let mut s = DevelopSettings { masks: vec![depth_mask(0.0, 0.5, 0.15, adj)], ..Default::default() };
    s.masks[0].components.push(MaskComponent {
        name: None,
        op: MaskOp::Intersect,
        invert: true,
        shape: MaskShape::Linear { start: Point::new(0.5, 0.0), end: Point::new(0.5, 0.4) },
    });
    s.masks[0].opacity = 60.0;
    check("depth ∩ ¬linear, opacity 60", &src, &info, &s, &req);
    s.masks[0].invert = true;
    s.orientation = Orientation::Rotate90;
    s.crop.geometry.rect = Rect::new(0.1, 0.1, 0.9, 0.85);
    s.crop.geometry.angle = -5.0;
    check("depth inverted, rotated + cropped", &src, &info, &s, &req);
    // the mask overlay shows the same band
    let s = DevelopSettings { masks: vec![depth_mask(0.0, 0.4, 0.1, adj)], ..Default::default() };
    let req = RenderRequest {
        overlay: Overlay::Mask { id: 7, view: lightcraft_pipeline::MaskView::WhiteOnBlack, color: [230, 30, 40], opacity: 50 },
        ..RenderRequest::fit(720, 720)
    };
    check("depth mask overlay", &src, &info, &s, &req);
}

// ------------------------------------------------------------------------------------------------
// Film looks: profile deltas on the existing mechanism

const FILM_LOOKS: [&str; 8] = [
    "lc.filmsim.vivid-slide",
    "lc.filmsim.natural-slide",
    "lc.filmsim.portrait-negative",
    "lc.filmsim.consumer-negative",
    "lc.filmsim.cinema-negative",
    "lc.filmsim.instant",
    "lc.filmsim.classic-bw",
    "lc.filmsim.high-speed-bw",
];

#[test]
fn film_looks_match_the_cpu_export() {
    if !gpu() {
        return;
    }
    let req = RenderRequest::fit(720, 720);
    for (i, info) in [(0, raw()), (5, SourceInfo::default())] {
        let src = scene(i, 900, 600);
        let plain = render(&src, &info, &DevelopSettings::default(), &req).image;
        for id in FILM_LOOKS {
            for amount in [100.0, 50.0, 200.0] {
                let mut s = DevelopSettings::default();
                s.profile.id = id.into();
                s.profile.amount = amount;
                let g = check(&format!("{} {amount}% scene {i}", id.trim_start_matches("lc.filmsim.")), &src, &info, &s, &req);
                if amount == 100.0 {
                    assert!(changed(&plain, &g) > 0.05, "{id} changes the render");
                }
            }
        }
        // with edits on top, and in a wide-gamut export
        let mut s = DevelopSettings::default();
        s.profile.id = "lc.filmsim.cinema-negative".into();
        s.profile.amount = 100.0;
        s.light.exposure = 0.3;
        s.color.vibrance = 30.0;
        s.grading.shadows = lightcraft_develop::Wheel { hue: 200.0, sat: 30.0, lum: 0.0 };
        check(&format!("cinema negative + edits scene {i}"), &src, &info, &s, &req);
        let p3 = RenderRequest { space: lightcraft_pipeline::OutputSpace::DisplayP3, ..req };
        check(&format!("cinema negative + edits, P3, scene {i}"), &src, &info, &s, &p3);
    }
}

// ------------------------------------------------------------------------------------------------
// AI Remove patches: composited in the CPU's linear stage, before the GPU's per-pixel work

fn ai_spot(key: &str, rect: [f64; 4], opacity: f64) -> Spot {
    Spot {
        mode: SpotMode::Ai,
        points: vec![Point::new((rect[0] + rect[2]) / 2.0, (rect[1] + rect[3]) / 2.0)],
        opacity,
        patch: Some(AiPatch { key: key.into(), source: String::new(), rect, engine: "test".into(), seed: 1, geometry: String::new() }),
        ..Default::default()
    }
}

#[test]
fn ai_remove_patches_are_in_the_gpu_render() {
    if !gpu() {
        return;
    }
    use lightcraft_pipeline::patches::{PatchPixels, forget, insert};
    let src = scene(2, 900, 600);
    let info = raw();
    let req = RenderRequest::fit(900, 900);
    // a stub patch: a red-to-green gradient with a soft edge
    let (pw, ph) = (60, 40);
    let data = (0..pw * ph)
        .map(|i| {
            let (x, y) = ((i % pw) as f32 / pw as f32, (i / pw) as f32 / ph as f32);
            let a = (1.0 - ((x - 0.5).abs().max((y - 0.5).abs()) * 2.0).powi(4)).clamp(0.0, 1.0);
            [0.6 * (1.0 - x), 0.5 * x, 0.05, a]
        })
        .collect();
    insert("lc-gpu-test-patch", Arc::new(PatchPixels { width: pw, height: ph, data }));
    let plain = gpu_render(&src, &info, &DevelopSettings::default(), &req, None);
    let mut s = DevelopSettings { spots: vec![ai_spot("lc-gpu-test-patch", [0.3, 0.3, 0.6, 0.6], 100.0)], ..Default::default() };
    let g = check("ai patch", &src, &info, &s, &req);
    let (c, o) = (g.data[450 * 900 / 2 + 400], plain.data[450 * 900 / 2 + 400]);
    assert!(c != o, "the GPU render shows the patch: {c:?} vs {o:?}");
    assert_eq!(g.data[10 * 900 + 10], plain.data[10 * 900 + 10], "outside the patch nothing changes");
    s.spots[0].opacity = 40.0;
    check("ai patch, opacity 40", &src, &info, &s, &req);
    // with edits, geometry and a heal spot: the patch stays in the source's light and place
    s.spots[0].opacity = 100.0;
    s.spots.push(Spot { points: vec![Point::new(0.8, 0.2)], size: 0.03, source_offset: Some(Point::new(-0.1, 0.0)), ..Default::default() });
    s.light.exposure = 0.5;
    s.wb.mode = lightcraft_develop::WbMode::Custom;
    s.wb.temp = 4200.0;
    s.detail.nr_luminance = 30.0;
    s.crop.geometry.rect = Rect::new(0.1, 0.1, 0.9, 0.9);
    s.crop.geometry.angle = 4.0;
    s.orientation = Orientation::Rotate270;
    check("ai patch + heal + edits + geometry", &src, &info, &s, &RenderRequest::fit(720, 720));
    // a cached view picks the patch up when it is added and drops it when it is removed
    let cache = StageCache::default();
    let base = DevelopSettings::default();
    let with = DevelopSettings { spots: vec![ai_spot("lc-gpu-test-patch", [0.3, 0.3, 0.6, 0.6], 100.0)], ..Default::default() };
    for (what, st) in [("without", &base), ("with", &with), ("without again", &base)] {
        assert_eq!(gpu_render(&src, &info, st, &req, Some(&cache)), gpu_render(&src, &info, st, &req, None), "cached view {what} the patch");
    }
    // a patch that is missing from the store renders as if there were none
    let missing = DevelopSettings { spots: vec![ai_spot("lc-gpu-test-patch-missing", [0.3, 0.3, 0.6, 0.6], 100.0)], ..Default::default() };
    assert_eq!(gpu_render(&src, &info, &missing, &req, None), plain);
    forget("lc-gpu-test-patch");
}

// ------------------------------------------------------------------------------------------------
// Timings at 24 MP (`cargo test --release -p lightcraft-gpu --test toolset -- --ignored --nocapture`)

#[test]
#[ignore = "benchmark: run with --release --ignored"]
fn bench_toolset_24mp() {
    let has_gpu = gpu();
    let (w, h) = (6000, 4000);
    let src = scene(3, w, h);
    let req = RenderRequest::fit(w, h);
    let info = SourceInfo { capture_radius: Some(0.8), ..raw() };
    lightcraft_pipeline::patches::insert(
        "lc-gpu-bench-patch",
        Arc::new(lightcraft_pipeline::patches::PatchPixels { width: 600, height: 400, data: vec![[0.3, 0.2, 0.1, 1.0]; 600 * 400] }),
    );
    let typical = |s: &mut DevelopSettings| {
        s.light.exposure = 0.3;
        s.light.highlights = -40.0;
        s.light.shadows = 30.0;
        s.effects.clarity = 15.0;
        s.detail.nr_luminance = 30.0;
        s.detail.sharpen_amount = 40.0;
    };
    let cases: Vec<(&str, SourceInfo, DevelopSettings)> = vec![
        ("detail sharpening", info.clone(), {
            let mut s = DevelopSettings::default();
            s.detail.sharpen_amount = 100.0;
            s.detail.sharpen_radius = 1.0;
            s.detail.sharpen_detail = 70.0;
            s
        }),
        ("detail wavelet NR", info.clone(), {
            let mut s = DevelopSettings::default();
            s.detail.nr_luminance = 60.0;
            s.detail.nr_color = 50.0;
            s
        }),
        ("detail dehaze", info.clone(), {
            let mut s = DevelopSettings::default();
            s.effects.dehaze = 60.0;
            s
        }),
        ("typical edit", info.clone(), DevelopSettings::default()),
        ("lens profile (db)", SourceInfo { lens_db: Some(barrel()), ..info.clone() }, {
            let mut s = DevelopSettings::default();
            lens_on(&mut s, 100.0);
            s
        }),
        ("tone equalizer", info.clone(), {
            let mut s = DevelopSettings::default();
            s.tone_eq.enabled = true;
            s.tone_eq.ev6 = 1.0;
            s
        }),
        ("colour cal linear", info.clone(), cal(Adaptation::Cat16, Illuminant::A, 0.0, false)),
        ("colour cal gamut + clip", info.clone(), cal(Adaptation::Cat16, Illuminant::A, 1.0, true)),
        ("colour cal non-linear Bradford", info.clone(), cal(Adaptation::FullBradford, Illuminant::A, 0.0, false)),
        ("capture sharpening", info.clone(), {
            let mut s = DevelopSettings::default();
            capture(&mut s, 0.0, 0.0, 8.0);
            s
        }),
        (
            "depth mask",
            info.clone(),
            DevelopSettings {
                masks: vec![depth_mask(0.0, 0.4, 0.1, LocalAdjustments { exposure: 0.5, ..Default::default() })],
                ..Default::default()
            },
        ),
        ("film look", info.clone(), {
            let mut s = DevelopSettings::default();
            s.profile.id = "lc.filmsim.cinema-negative".into();
            s.profile.amount = 100.0;
            s
        }),
        ("ai patch", info.clone(), DevelopSettings { spots: vec![ai_spot("lc-gpu-bench-patch", [0.3, 0.3, 0.6, 0.6], 100.0)], ..Default::default() }),
        ("layer tools (curve on a mask)", info.clone(), {
            let mut s = DevelopSettings { masks: vec![depth_mask(0.0, 0.4, 0.1, LocalAdjustments::default())], ..Default::default() };
            s.masks[0].tools.curve = Some(lightcraft_develop::ToneCurve { darks: 40.0, ..Default::default() });
            s
        }),
    ];
    let ms = |t: std::time::Instant| t.elapsed().as_secs_f64() * 1e3;
    eprintln!("{:<34} {:>10} {:>10}  renderer", "24 MP (6000×4000), full size", "GPU path", "CPU only");
    for (name, info, mut s) in cases {
        if std::env::var_os("LC_DETAIL_BENCH_ONLY").is_some() && !name.starts_with("detail ") {
            continue;
        }
        if !name.starts_with("detail ") {
            typical(&mut s);
        }
        // warm-up (device, kernels, patch cache), then the timed renders
        if has_gpu {
            let _ = lightcraft_gpu::render(&src, &info, &s, &req, None);
        }
        let t = std::time::Instant::now();
        let on_gpu = lightcraft_gpu::render(&src, &info, &s, &req, None).is_some();
        let gpu_ms = if on_gpu { ms(t) } else { f64::NAN };
        let t = std::time::Instant::now();
        let _ = render(&src, &info, &s, &req);
        let cpu_ms = ms(t);
        // what runs on the CPU inside a GPU render (per-stage hybrid)
        let plan = lightcraft_pipeline::plan(&src, &info, &s, &req);
        let mut cpu_parts = Vec::new();
        if !plan.frame.gpu_samplable() {
            cpu_parts.push("geometry");
        }
        if lightcraft_pipeline::lin_needs_cpu(&plan.settings) {
            cpu_parts.push("linear stage");
        }
        if plan.settings.masks.iter().flat_map(|m| &m.components).any(|c| matches!(c.shape, MaskShape::DepthRange { .. })) {
            cpu_parts.push("mask shape");
        }
        let path = match (on_gpu, cpu_parts.is_empty()) {
            (false, _) => "CPU fallback".to_string(),
            (true, true) => "GPU".to_string(),
            (true, false) => format!("GPU + CPU {}", cpu_parts.join(", ")),
        };
        let gpu_col = if on_gpu { format!("{gpu_ms:.0} ms") } else { "—".into() };
        eprintln!("{name:<34} {gpu_col:>10} {:>10}  {path}", format!("{cpu_ms:.0} ms"));
    }
    lightcraft_pipeline::patches::forget("lc-gpu-bench-patch");
}

#[test]
fn dual_illuminant_wb_and_dcp_inside_gpu_render() {
    if !gpu() {
        return;
    }
    use lightcraft_color::{
        Mat3,
        camera::{CameraColor, CameraWhite},
        profile::{HsvTable, ProfileLook},
    };
    use lightcraft_develop::WbMode;
    let src = scene(2, 192, 128);
    let color = CameraColor {
        illuminant: [17, 21],
        color_matrix: [
            Some(Mat3([[0.9, 0.2, -0.15], [-0.3, 1.25, 0.08], [0.02, -0.12, 0.85]])),
            Some(Mat3([[0.7, 0.3, -0.1], [-0.35, 1.3, 0.1], [0.05, -0.2, 1.0]])),
        ],
        ..Default::default()
    };
    let total = lightcraft_color::camera::to_working(&color, lightcraft_color::cct::temp_tint_to_xy(5200., 4.));
    let mut info = SourceInfo { camera_color: Some(CameraWhite { color, undo: total.inverse().unwrap() }), ..raw() };
    let mut a = HsvTable { hue_divisions: 4, sat_divisions: 3, val_divisions: 2, srgb_value: true, data: vec![[0., 1., 1.]; 24] };
    for (i, node) in a.data.iter_mut().enumerate() {
        *node = [(i % 3) as f32 * 5., 0.9 + (i % 4) as f32 * 0.05, 0.95];
    }
    let mut b = a.clone();
    for node in &mut b.data {
        node[0] *= -1.;
        node[1] *= 0.95;
    }
    let stages = StageCache::default();
    let req = RenderRequest::fit(192, 128);
    for use_profile in [false, true] {
        info.camera_profile = use_profile
            .then(|| Arc::new(ProfileLook { hue_sat_map: [Some(a.clone()), Some(b.clone())], look_table: Some(a.clone()), tone_curve: None }));
        for temp in [2856., 4500., 6504., 9000.] {
            for tint in [-15., 0., 25.] {
                let mut s = DevelopSettings::default();
                s.wb.mode = WbMode::Custom;
                s.wb.temp = temp;
                s.wb.tint = tint;
                check(&format!("dual WB {temp}/{tint} DCP {use_profile}"), &src, &info, &s, &req);
                let cached = gpu_render(&src, &info, &s, &req, Some(&stages));
                let fresh = gpu_render(&src, &info, &s, &req, None);
                assert_eq!(cached.data, fresh.data, "WB/profile cache invalidation");
            }
        }
    }
}

fn detail_scene() -> Arc<Rgb32f> {
    Arc::new(Rgb32f::from_fn(257, 193, |x, y| {
        let base = if x < 128 { [0.035, 0.015, 0.009] } else { [0.24, 0.36, 0.18] };
        std::array::from_fn(|k| {
            let v = (x as u32).wrapping_mul(7919) ^ (y as u32).wrapping_mul(104729) ^ (k as u32).wrapping_mul(941083);
            let noise = ((v.wrapping_mul(1664525).wrapping_add(1013904223) >> 16) as f32 / 65535.0 - 0.5) * 0.05;
            base[k] + noise + 0.02 * (x as f32 * 0.8).sin() + if y < 24 { 1.5 } else { 0.0 }
        })
    }))
}
#[test]
fn detail_sharpen_radius_detail_masking_and_alt_preview_match() {
    if !gpu() {
        return;
    }
    let src = detail_scene();
    let mut s = DevelopSettings::default();
    for scale in [1.0, 2.0] {
        let info = SourceInfo { sensor_scale: scale, ..raw() };
        for edge in [257, 137, 31] {
            for (radius, detail, masking) in [(0.5, 0.0, 0.0), (1.0, 50.0, 60.0), (3.0, 100.0, 100.0)] {
                s.detail.sharpen_amount = 150.0;
                s.detail.sharpen_radius = radius;
                s.detail.sharpen_detail = detail;
                s.detail.sharpen_masking = masking;
                let req = RenderRequest::fit(edge, edge);
                check(&format!("detail sharp r{radius} d{detail} m{masking} sensor{scale}"), &src, &info, &s, &req);
                check("detail Alt Masking", &src, &info, &s, &RenderRequest { overlay: Overlay::SharpenMask, ..req });
            }
        }
    }
}
#[test]
fn detail_y0u0v0_wavelets_six_sliders_and_preview_match() {
    if !gpu() {
        return;
    }
    let src = detail_scene();
    let info = raw();
    for edge in [257, 137, 31] {
        for (lum, col, detail, contrast, col_detail, smoothness) in [
            (60.0, 0.0, 0.0, 0.0, 50.0, 50.0),
            (60.0, 0.0, 100.0, 100.0, 50.0, 50.0),
            (0.0, 100.0, 50.0, 0.0, 0.0, 0.0),
            (0.0, 100.0, 50.0, 0.0, 100.0, 100.0),
            (80.0, 70.0, 50.0, 50.0, 50.0, 50.0),
        ] {
            let mut s = DevelopSettings::default();
            s.detail.nr_luminance = lum;
            s.detail.nr_color = col;
            s.detail.nr_detail = detail;
            s.detail.nr_contrast = contrast;
            s.detail.nr_color_detail = col_detail;
            s.detail.nr_color_smoothness = smoothness;
            s.light.exposure = 1.7;
            check(
                &format!("detail NR Y{lum} UV{col} D{detail} C{contrast} CD{col_detail} S{smoothness}"),
                &src,
                &info,
                &s,
                &RenderRequest::fit(edge, edge),
            );
        }
    }
}
#[test]
fn detail_dark_channel_rgb_guidance_positive_negative_match() {
    if !gpu() {
        return;
    }
    let src = detail_scene();
    for scale in [1.0, 2.0] {
        for edge in [257, 137, 31] {
            for strength in [-100.0, -35.0, 35.0, 100.0] {
                let mut s = DevelopSettings::default();
                s.effects.dehaze = strength;
                let info = SourceInfo { sensor_scale: scale, ..raw() };
                check(&format!("detail haze {strength} sensor{scale}"), &src, &info, &s, &RenderRequest::fit(edge, edge));
            }
        }
    }
}
#[test]
fn detail_slider_and_sensor_scale_changes_invalidate_cached_planes() {
    if !gpu() {
        return;
    }
    let src = detail_scene();
    let stages = StageCache::default();
    let mut info = raw();
    let req = RenderRequest::fit(200, 200);
    let mut s = DevelopSettings::default();
    for step in 0..12 {
        match step {
            0 => s.detail.sharpen_amount = 90.0,
            1 => s.detail.sharpen_radius = 3.0,
            2 => s.detail.sharpen_masking = 80.0,
            3 => s.detail.sharpen_detail = 90.0,
            4 => s.effects.dehaze = 60.0,
            5 => s.effects.dehaze = -60.0,
            6 => s.detail.nr_luminance = 65.0,
            7 => s.detail.nr_contrast = 100.0,
            8 => s.detail.nr_color = 60.0,
            9 => s.detail.nr_color_smoothness = 100.0,
            10 => info.sensor_scale = 2.0,
            _ => s.detail.nr_detail = 100.0,
        }
        let cached = gpu_render(&src, &info, &s, &req, Some(&stages));
        let fresh = check(&format!("detail cached step {step}"), &src, &info, &s, &req);
        assert_eq!(cached, fresh, "cache at step {step}");
    }
}
