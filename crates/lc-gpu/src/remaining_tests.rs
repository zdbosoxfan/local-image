//! Native remaining-stage numerics use the same test-only device as primary tests.
use super::*;
use crate::primary::tests::device;
use lightcraft_develop::{Mask, MaskComponent, MaskShape};
use lightcraft_geom::Point;
use lightcraft_pipeline::primary::HsMethod;

fn compare(g: &Gpu, src: &Arc<Rgb32f>, info: &SourceInfo, s: &DevelopSettings, req: &RenderRequest, stages: &GpuStages) -> Rgba8 {
    crate::CPU_STAGES.with(|stages| stages.borrow_mut().clear());
    let errors = crate::ctx::ErrorScopes::push(g);
    let image = render(g, src, info, s, req, Some(stages), None, HsMethod::LiTone).unwrap().image;
    assert!(errors.pop().is_none());
    assert!(crate::ctx::take_failure().is_none());
    assert!(crate::last_cpu_stages().is_empty(), "native render used host stages: {:?}", crate::last_cpu_stages());
    let cpu = lightcraft_pipeline::render(src, info, s, req).image;
    let mut sum = 0u64;
    let mut max = 0;
    for (a, b) in cpu.data.iter().zip(&image.data) {
        for c in 0..3 {
            let d = a[c].abs_diff(b[c]);
            sum += d as u64;
            max = max.max(d);
        }
    }
    let mean = sum as f64 / (cpu.data.len() * 3) as f64;
    assert!(mean < 0.5 && max <= 3, "{}x{}: mean={mean}, max={max}", cpu.width, cpu.height);
    image
}
fn input(w: usize, h: usize) -> Arc<Rgb32f> {
    Arc::new(Rgb32f::from_fn(w, h, |x, y| {
        let v = 0.003 + x as f32 / w as f32 * 0.3 + if x > w / 2 { 1.2 } else { 0.0 };
        [v * (1.0 + 0.04 * (y as f32).sin()), v * 0.7, v * 0.3 + 0.002 * (x as f32 * 1.7).cos()]
    }))
}
fn mask() -> Mask {
    Mask {
        components: vec![MaskComponent {
            name: None,
            op: lightcraft_develop::MaskOp::Add,
            invert: false,
            shape: MaskShape::Linear { start: Point::new(0.1, 0.1), end: Point::new(0.9, 0.8) },
        }],
        opacity: 65.0,
        ..Default::default()
    }
}

#[test]
fn native_toneeq_filter_extremes_and_cached_overlay_match() {
    let Some(g) = device() else { return };
    let _scope = crate::ctx::RenderScope::new(g);
    let info = SourceInfo::default();
    let stages = GpuStages::default();
    for (w, h) in [(1, 1), (3, 7), (43, 29)] {
        let src = input(w, h);
        let req = RenderRequest::fit(w, h);
        let mut s = DevelopSettings::default();
        s.tone_eq.enabled = true;
        s.tone_eq.ev4 = 2.0;
        s.tone_eq.ev6 = -2.0;
        s.tone_eq.mask_contrast = 0.7;
        s.tone_eq.mask_exposure = -0.3;
        s.light.exposure = 0.8;
        let mut m = mask();
        m.adjust.exposure = 0.7;
        s.masks.push(m);
        let first = compare(g, &src, &info, &s, &req, &stages);
        s.tone_eq.size = 50.0;
        s.tone_eq.refine = 100.0;
        s.tone_eq.smoothing = -2.33;
        compare(g, &src, &info, &s, &req, &stages);
        compare(g, &src, &info, &s, &RenderRequest { overlay: lightcraft_pipeline::Overlay::ToneEqMask, ..req }, &stages);
        s.tone_eq.size = 10.0; // Restore via complete settings below for cache equality.
        let mut original = DevelopSettings::default();
        original.tone_eq.enabled = true;
        original.tone_eq.ev4 = 2.0;
        original.tone_eq.ev6 = -2.0;
        original.tone_eq.mask_contrast = 0.7;
        original.tone_eq.mask_exposure = -0.3;
        original.light.exposure = 0.8;
        let mut m = mask();
        m.adjust.exposure = 0.7;
        original.masks.push(m);
        assert_eq!(first, compare(g, &src, &info, &original, &req, &stages));
    }
}

#[test]
fn native_nr_statistics_match_robust_model_and_reconstruction() {
    use lightcraft_pipeline::detail::nr::*;
    let Some(g) = device() else { return };
    let _scope = crate::ctx::RenderScope::new(g);
    let mut cx = Cx::new(g);
    for (w, h) in [(1, 1), (7, 5), (67, 49), (265, 259)] {
        let src = Rgb32f::from_fn(w, h, |x, y| {
            let v = 0.01 + x as f32 / w as f32 * 0.7;
            let hash = (x as u32).wrapping_mul(1664525) ^ (y as u32).wrapping_mul(1013904223);
            let noise = (hash % 101) as f32 / 101.0 - 0.5;
            [v + noise * 0.015, v * 0.7 - noise * 0.01, v * 0.3 + noise * 0.02]
        });
        let image = g.upload(rgb_words(&src));
        let samples = g.buffer(16 * 16384);
        map(&mut cx, "nr_samples", 16 * 16384, &[w as u32, h as u32], [Some(&image), None, None], &samples);
        let slopes = g.buffer(16);
        cx.run("nr_median", &[], &[Some(&samples), None, None, Some(&slopes)], [16, 1, 1]);
        let p = NrParams { lum: 0.6, col: 0.5, detail: 0.4, contrast: 0.5, col_detail: 0.5, smoothness: 0.5, scale: 1.0 };
        let (to, from) = conversion_matrices([1.0; 3]);
        let mut block = vec![p.scale.to_bits(), p.lum.max(p.col).to_bits()];
        block.extend(to.into_iter().flatten().chain(from.into_iter().flatten()).map(f32::to_bits));
        let metadata = g.buffer(24);
        map(&mut cx, "nr_vst", 1, &block, [Some(&slopes), None, None], &metadata);
        let values: Vec<f32> = cx.read(&metadata, 24);
        let model = estimate_noise(&src);
        assert!((values[23] - model.a).abs() < model.a * 2e-5 + 1e-10, "{w}x{h}: {} vs {}", values[23], model.a);
        let output = denoise_native(&mut cx, image, w, h, &p);
        let gpu = cx.read_rgb(&output, w, h);
        let mut cpu = src.clone();
        denoise(&mut cpu, &p);
        let max = cpu.data.iter().flatten().zip(gpu.data.iter().flatten()).map(|(a, b)| (a - b).abs()).fold(0.0, f32::max);
        assert!(max < 0.003, "{w}x{h}: {max}");
    }
}

#[test]
fn native_calibration_patch_minification_and_depth_match() {
    use lightcraft_develop::{AiPatch, SegMask, Spot, SpotMode};
    let Some(g) = device() else { return };
    let _scope = crate::ctx::RenderScope::new(g);
    let src = input(47, 33);
    let info = SourceInfo::default();
    let req = RenderRequest::fit(31, 23);
    let stages = GpuStages::default();
    let patch = lightcraft_pipeline::patches::PatchPixels {
        width: 100,
        height: 80,
        data: (0..8000).map(|i| [0.3 + (i % 2) as f32 * 0.4, 0.2, 0.1, if i % 3 == 0 { 0.0 } else { 0.7 }]).collect(),
    };
    lightcraft_pipeline::patches::insert("gpu-remaining-minify", Arc::new(patch));
    let mut s = DevelopSettings::default();
    s.spots.push(Spot {
        mode: SpotMode::Ai,
        opacity: 40.0,
        patch: Some(AiPatch {
            key: "gpu-remaining-minify".into(),
            rect: [0.1, 0.2, 0.8, 0.9],
            source: String::new(),
            engine: "test".into(),
            seed: 1,
            geometry: String::new(),
        }),
        ..Default::default()
    });
    let mut m = mask();
    m.components[0].shape = MaskShape::DepthRange {
        lo: 0.3,
        hi: 0.7,
        feather: 0.08,
        seg: Some(SegMask::from_logits_in(5, &(0..25).map(|i| i as f32 * 0.25 - 3.0).collect::<Vec<_>>(), [0.1, 0.1, 0.9, 0.9])),
    };
    m.adjust.exposure = 0.5;
    s.masks.push(m);
    for adaptation in [lightcraft_develop::Adaptation::Cat16, lightcraft_develop::Adaptation::FullBradford, lightcraft_develop::Adaptation::Xyz] {
        s.color_cal.enabled = true;
        s.color_cal.adaptation = adaptation;
        s.color_cal.illuminant = lightcraft_develop::Illuminant::A;
        s.color_cal.gamut = 1.0;
        s.color_cal.clip = true;
        compare(g, &src, &info, &s, &req, &stages);
    }
    s.crop.geometry.angle = 4.0;
    s.crop.flip_h = true;
    compare(g, &src, &info, &s, &req, &stages);
    lightcraft_pipeline::patches::forget("gpu-remaining-minify");
}

#[test]
fn native_layer_stage_order_opacity_nr_sharpen_curve_and_grain_match() {
    use lightcraft_develop::{ColorAdj, Detail, Effects, Grain, LayerTools, Light, ToneCurve, Vignette};
    let Some(g) = device() else { return };
    let _scope = crate::ctx::RenderScope::new(g);
    let src = input(47, 33);
    let info = SourceInfo::default();
    let req = RenderRequest::fit(47, 33);
    let stages = GpuStages::default();
    let mut s = DevelopSettings::default();
    s.light.exposure = 0.3;
    s.curve.lights = -20.0;
    let mut m = mask();
    m.tools = LayerTools {
        light: Some(Light { exposure: 0.6, contrast: 30.0, ..Default::default() }),
        curve: Some(ToneCurve { darks: 40.0, refine_saturation: 30.0, ..Default::default() }),
        vignette: Some(Vignette { amount: -30.0, ..Default::default() }),
        grain: Some(Grain { amount: 30.0, ..Default::default() }),
        detail: Some(Detail { nr_luminance: 40.0, nr_color: 30.0, sharpen_amount: 80.0, sharpen_radius: 2.0, ..Default::default() }),
        effects: Some(Effects { dehaze: 20.0, ..Default::default() }),
        color: Some(ColorAdj { saturation: 30.0, ..Default::default() }),
        ..Default::default()
    };
    s.masks.push(m.clone());
    m.id = 2;
    m.invert = true;
    m.opacity = 30.0;
    m.tools.detail = None;
    m.tools.light.as_mut().unwrap().exposure = -0.4;
    s.masks.push(m);
    compare(g, &src, &info, &s, &req, &stages);
    s.masks[0].opacity = 100.0;
    s.masks[0].tools.curve.as_mut().unwrap().darks = -70.0;
    compare(g, &src, &info, &s, &RenderRequest::fit(31, 23), &stages);
    s.masks[0].visible = false;
    compare(g, &src, &info, &s, &req, &stages);
}

#[test]
fn native_haze_airlight_matches_reference_with_both_signs() {
    let Some(g) = device() else { return };
    let _scope = crate::ctx::RenderScope::new(g);
    let src = input(43, 29);
    let mut cx = Cx::new(g);
    let image = g.upload(rgb_words(&src));
    let errors = crate::ctx::ErrorScopes::push(g);
    let (_, air) = haze_planes(&mut cx, &image, 43, 29, 1.0);
    let gpu: Vec<f32> = cx.read(&air, 4);
    assert!(errors.pop().is_none());
    assert!(crate::ctx::take_failure().is_none());
    let (cpu, distance) = lightcraft_pipeline::detail::haze::ambient_light(&src, 6);
    for (a, b) in gpu.iter().zip(cpu.into_iter().chain([distance])) {
        assert!((a - b).abs() < 3e-5, "{a} vs {b}");
    }
    let info = SourceInfo::default();
    let req = RenderRequest::fit(43, 29);
    let mut s = DevelopSettings::default();
    for strength in [-100.0, 60.0] {
        s.effects.dehaze = strength;
        compare(g, &src, &info, &s, &req, &GpuStages::default());
    }
}

#[test]
fn native_calibration_negative_hdr_and_zero_luminance_match() {
    use lightcraft_pipeline::colorcal::{Cat, ColorCal, Standard};
    let Some(g) = device() else { return };
    let _scope = crate::ctx::RenderScope::new(g);
    let samples = [[0.0; 3], [-0.2, 0.1, 0.3], [3.0, -1.0, 0.2], [0.2, 0.8, 0.1], [1e-12; 3], [8.0, 2.0, -0.5]];
    let mut cx = Cx::new(g);
    let img = g.upload(samples.as_flattened());
    for cat in [Cat::Cat16, Cat::LinearBradford, Cat::FullBradford, Cat::Xyz] {
        for gamut in [0.0, 0.1, 12.0] {
            for clip in [false, true] {
                let cal = ColorCal::new(cat, Standard::F11.xy(), gamut, clip);
                let out = g.buffer(samples.len() * 3);
                map(&mut cx, "colour_cal", samples.len(), &cal.gpu_words(), [Some(&img), None, None], &out);
                let gpu = cx.read_rgb(&out, samples.len(), 1);
                for (rgb, actual) in samples.iter().zip(gpu.data) {
                    let expected = cal.apply(*rgb);
                    for (a, b) in expected.into_iter().zip(actual) {
                        assert!((a - b).abs() < 3e-4, "{cat:?} gamut{gamut} clip{clip} {rgb:?}: {a} vs {b}");
                    }
                }
            }
        }
    }
}
