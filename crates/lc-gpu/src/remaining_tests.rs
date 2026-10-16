//! Native remaining-stage numerics use the same test-only device as primary tests.
use super::*;
use crate::primary::tests::device;
use lightcraft_develop::{Mask, MaskComponent, MaskShape};
use lightcraft_geom::Point;
use lightcraft_pipeline::primary::HsMethod;

fn compare(g: &Gpu, src: &Arc<Rgb32f>, info: &SourceInfo, s: &DevelopSettings, req: &RenderRequest, stages: &GpuStages) -> Rgba8 {
    compare_or_dump(g, src, info, s, req, stages, false)
}
fn compare_or_dump(g: &Gpu, src: &Arc<Rgb32f>, info: &SourceInfo, s: &DevelopSettings, req: &RenderRequest, stages: &GpuStages, dump: bool) -> Rgba8 {
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
    if max > 3 {
        for (i, (p, q)) in cpu.data.iter().zip(&image.data).enumerate().filter(|(_, (p, q))| (0..3).any(|c| p[c].abs_diff(q[c]) > 3)).take(32) {
            eprintln!("pixel ({}, {}) CPU={p:?} GPU={q:?}", i % cpu.width, i / cpu.width);
        }
    }
    if dump {
        eprintln!("toneeq final {}x{}: mean={mean:.12}, max={max}", cpu.width, cpu.height);
        let mut pixels: Vec<_> = cpu
            .data
            .iter()
            .zip(&image.data)
            .enumerate()
            .filter_map(|(i, (p, q))| {
                let d = (0..3).map(|ch| p[ch].abs_diff(q[ch])).max().unwrap_or(0);
                (d != 0).then_some((d, i, p, q))
            })
            .collect();
        pixels.sort_unstable_by_key(|(d, i, _, _)| (std::cmp::Reverse(*d), *i));
        eprintln!("toneeq final: {} differing pixels (largest 32 follow)", pixels.len());
        let mut points = Vec::new();
        for (d, i, p, q) in pixels.iter().take(32) {
            let (x, y) = (i % cpu.width, i / cpu.width);
            eprintln!("  pixel ({x}, {y}) CPU={p:?} GPU={q:?} max={d}");
            points.push((x, y));
        }
        // Include the original failing guidance coordinate even on a passing driver.
        let known = (333 * cpu.width / 641, 52 * cpu.height / 427);
        if !points.contains(&known) {
            points.push(known);
        }
        crate::primary::trace::dump(cpu.width, cpu.height, &points);
    } else {
        assert!(mean < 0.5 && max <= 3, "{}x{}: mean={mean}, max={max}", cpu.width, cpu.height);
    }
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
fn native_develop_gaussian_policy_matches_cpu_reference() {
    let Some(gpu_device) = device() else { return };
    let g = &*gpu_device;
    if std::env::var_os("LC_REQUIRE_PHYSICAL_GPU").is_some() {
        assert_ne!(g.info.device_type, wgpu::DeviceType::Cpu, "physical adapter required");
    }
    eprintln!("Develop Gaussian adapter: {:?}", g.info);
    let _scope = crate::ctx::RenderScope::new(g);
    let errors = crate::ctx::ErrorScopes::push(g);
    let mut cx = Cx::new(g);
    let (w, h) = (53, 37);
    for nc in [1, 2, 3, 4] {
        let input: Vec<f32> = (0..w * h)
            .flat_map(|i| {
                let (x, y) = (i % w, i / w);
                [
                    if x == 17 && y == 11 { 1.0 } else { 0.1 },
                    ((x * 17 + y * 7) % 31) as f32 / 31.0,
                    if x > w / 2 { 0.9 } else { 0.05 },
                    ((x + y) % 3) as f32 * 0.2,
                ][..nc]
                    .to_vec()
            })
            .collect();
        let uploaded = g.upload(&input.iter().map(|v| v.to_bits()).collect::<Vec<_>>());
        for sigma in [0.0, 0.3, 0.5, 0.8, 1.0, 2.0, 4.0, 6.0] {
            let out = gaussian(&mut cx, &uploaded, w, h, nc, sigma);
            let actual: Vec<f32> = cx.read(&out, w * h * nc);
            let mut expected = vec![0.0; input.len()];
            // Each component is a scalar field, including four-channel guidance moments.
            for c in 0..nc {
                let field = Plane::from_fn(w, h, |x, y| input[(y * w + x) * nc + c]);
                let cpu = lightcraft_pipeline::blur::gaussian(&field, sigma);
                for (i, v) in cpu.data.iter().enumerate() {
                    expected[i * nc + c] = *v;
                }
            }
            let max = actual.iter().zip(&expected).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
            assert!(actual.iter().all(|v| v.is_finite()));
            assert!(max < 3e-5, "nc={nc}, sigma={sigma}: max={max}");
            eprintln!("Develop Gaussian nc={nc} sigma={sigma} max_abs={max:.9}");
        }
    }
    assert!(errors.pop().is_none());
    assert!(crate::ctx::take_failure().is_none());
}

#[test]
fn native_toneeq_filter_extremes_and_cached_overlay_match() {
    let Some(gpu_device) = device() else { return };
    let g = &*gpu_device;
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
        compare(g, &src, &info, &s, &RenderRequest { overlay: lightcraft_pipeline::Overlay::ToneEqMask, ..req.clone() }, &stages);
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
fn native_toneeq_5090_extreme_fixture_matches() {
    toneeq_5090_fixture(false);
}

#[test]
#[ignore = "5090 diagnostic: prints final differences, inputs, bin decisions and intermediate float words"]
fn native_toneeq_5090_dump() {
    let _trace = crate::primary::trace::Scope::new();
    toneeq_5090_fixture(true);
}

fn toneeq_5090_fixture(dump: bool) {
    let Some(gpu_device) = crate::test_device().or_else(device) else { return };
    let g = &*gpu_device;
    eprintln!("toneeq fixture adapter: {:?}", g.info);
    let _scope = crate::ctx::RenderScope::new(g);
    let (w, h) = (641, 427);
    let src = Arc::new(Rgb32f::from_fn(w, h, |x, y| {
        let v = 0.0003 * 1.02f32.powf(x as f32 * 320.0 / w as f32) * (1.0 + 0.06 * (y as f32 * 0.7).sin());
        [v, v * 0.7, v * 0.4]
    }));
    let side = 96;
    let logits: Vec<_> = (0..side * side)
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
    let info = SourceInfo { raw: true, as_shot_temp: 5200.0, as_shot_tint: 4.0, ..Default::default() };
    let mut s = DevelopSettings::default();
    s.tone_eq.enabled = true;
    s.tone_eq.ev8 = -2.0;
    s.tone_eq.ev4 = 2.0;
    s.tone_eq.ev0 = -2.0;
    let mut m = mask();
    m.opacity = 100.0;
    m.components[0].shape =
        MaskShape::DepthRange { lo: 0.0, hi: 0.7, feather: 0.1, seg: Some(lightcraft_develop::SegMask::from_logits(side, &logits)) };
    m.adjust = lightcraft_develop::LocalAdjustments { exposure: 0.6, contrast: 20.0, ..Default::default() };
    s.masks.push(m);
    let cache = GpuStages::default();
    for (size, edges, smoothing, exposure, contrast) in [(0.1, 0.0, -2.33, -2.0, -1.0), (50.0, 100.0, 1.67, 2.0, 1.0), (0.1, 0.0, -2.33, -2.0, -1.0)]
    {
        s.tone_eq.size = size;
        s.tone_eq.refine = edges;
        s.tone_eq.smoothing = smoothing;
        s.tone_eq.mask_exposure = exposure;
        s.tone_eq.mask_contrast = contrast;
        for edge in [w, w / 2] {
            let req = RenderRequest::fit(edge, edge);
            if dump {
                eprintln!("toneeq fixture settings: {:?}, request={req:?}", s.tone_eq);
                // Always prepare spatial buffers so every case has complete snapshots.
                compare_or_dump(g, &src, &info, &s, &req, &GpuStages::default(), true);
            } else {
                compare(g, &src, &info, &s, &req, &cache);
            }
        }
    }
}

// Independent order-statistic oracle, rather than repeating the shader partition.
fn reference_select(mut v: Vec<f32>) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    let nth = ((v.len() as f32 * 0.95) as usize).min(v.len() - 1);
    v.sort_unstable_by(f32::total_cmp);
    v[nth]
}

#[test]
#[ignore = "required physical GPU acceptance; see docs/GPU-VALIDATION.md"]
fn ds_finding_native_haze_selection_matches_sorted_quantile() {
    let gpu_device = crate::test_device().expect("a real GPU is required");
    let g = &*gpu_device;
    eprintln!("haze selection adapter: {:?}", g.info);
    assert_ne!(g.info.device_type, wgpu::DeviceType::Cpu);
    haze_selection_matches_sorted_quantile(g);
}

#[test]
#[ignore = "required Linux CI Vulkan numerical conformance; software adapters allowed"]
fn haze_selection_numerical_vulkan_matches_sorted_quantile() {
    let gpu_device = device().expect("Vulkan numerical-test adapter required");
    let g = &*gpu_device;
    eprintln!("haze numerical conformance adapter (software allowed): {:?}", g.info);
    assert_eq!(g.info.backend, wgpu::Backend::Vulkan);
    haze_selection_matches_sorted_quantile(g);
}

// Both entry points execute every original case against the independent sorted oracle.
// Keep the owning TestGpu in the caller through readback and device teardown.
fn haze_selection_matches_sorted_quantile(g: &Gpu) {
    let _scope = crate::ctx::RenderScope::new(g);
    let mut cx = Cx::new(g);
    let errors = crate::ctx::ErrorScopes::push(g);
    for values in [vec![1., 4., 3., 2.], vec![1., 1., 1., 4.]] {
        let scratch = g.upload(&values);
        let zero = cx.zeroed(1);
        let out = haze_select(&mut cx, &scratch, values.len(), 0, &zero);
        let actual: Vec<f32> = cx.read(&out, 1);
        assert_eq!(actual[0], reference_select(values));
    }
    for n in [65537, 71003, 132097] {
        for mode in 0..5 {
            let v: Vec<f32> = (0..n)
                .map(|i| match mode {
                    0 => i as f32,
                    1 => (n - i) as f32,
                    2 => (i / 113) as f32,
                    3 => 0.125,
                    _ => ((i as u32).wrapping_mul(1664525).wrapping_add(1013904223) % 7919) as f32 - 3000.0,
                })
                .collect();
            let expected = reference_select(v.clone());
            let scratch = g.upload(&v);
            let zero = cx.zeroed(1);
            let out = haze_select(&mut cx, &scratch, n, 0, &zero);
            let actual: Vec<f32> = cx.read(&out, 1);
            assert_eq!(actual[0].to_bits(), expected.to_bits(), "n={n} mode={mode}: {} vs {expected}", actual[0]);
        }
    }
    // Brightness compaction can produce far fewer values than the allocated source
    // length, including none. Poison unused storage to expose an interval/rank leak.
    let n = 71003;
    for count in [0, 1, 17, 997, 64999, 65537, n] {
        let mut v: Vec<f32> = (0..count).map(|i| ((i as u32).wrapping_mul(1664525) % 4093) as f32).collect();
        let expected = reference_select(v.clone());
        v.resize(n, f32::NAN);
        let scratch = g.upload(&v);
        let total = g.upload(&[f32::from_bits(count as u32)]);
        let out = haze_select(&mut cx, &scratch, n, 1, &total);
        let actual: Vec<f32> = cx.read(&out, 1);
        assert_eq!(actual[0].to_bits(), expected.to_bits(), "brightness count={count}: {} vs {expected}", actual[0]);
    }
    assert!(errors.pop().is_none());
    assert!(crate::ctx::take_failure().is_none());
}

#[test]
fn native_nr_statistics_match_robust_model_and_reconstruction() {
    use lightcraft_pipeline::detail::nr::*;
    let Some(gpu_device) = device() else { return };
    let g = &*gpu_device;
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
    let Some(gpu_device) = device() else { return };
    let g = &*gpu_device;
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
            orientation: Default::default(),
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
    let Some(gpu_device) = device() else { return };
    let g = &*gpu_device;
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
    let Some(gpu_device) = device() else { return };
    let g = &*gpu_device;
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
    let Some(gpu_device) = device() else { return };
    let g = &*gpu_device;
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

#[test]
fn ds_finding_hidden_overlay_uses_primary_image() {
    let gpu_device = crate::test_device().or_else(device).expect("a real GPU is required");
    let g = &*gpu_device;
    let _scope = crate::ctx::RenderScope::new(g);
    let src = input(97, 73);
    let mut s = DevelopSettings::default();
    s.color.saturation = -70.;
    let mut m = mask();
    m.visible = false;
    m.components[0].shape = MaskShape::ColorRange { samples: vec![[0.7, 0.35, 0.15]], refine: 50. };
    let id = m.id;
    s.masks.push(m);
    let mut req = RenderRequest::fit(97, 73);
    req.overlay = lightcraft_pipeline::Overlay::Mask { id: id as u16, view: lightcraft_pipeline::MaskView::Color, color: [255, 0, 0], opacity: 100 };
    let info = SourceInfo::default();
    let errors = crate::ctx::ErrorScopes::push(g);
    let gpu = render(g, &src, &info, &s, &req, Some(&GpuStages::default()), None, HsMethod::LiTone).unwrap().image;
    let cpu = lightcraft_pipeline::render(&src, &info, &s, &req).image;
    let error: u64 = gpu.data.iter().zip(&cpu.data).flat_map(|(a, b)| (0..3).map(move |k| a[k].abs_diff(b[k]) as u64)).sum();
    assert!(error as f64 / ((gpu.data.len() * 3) as f64) < 0.5);
    assert!(errors.pop().is_none());
}

#[test]
fn ds_finding_zero_skin_hue_range_is_finite_on_gpu() {
    let gpu_device = crate::test_device().or_else(device).expect("a real GPU is required");
    let g = &*gpu_device;
    let _scope = crate::ctx::RenderScope::new(g);
    let src = input(97, 73);
    let mut s = DevelopSettings::default();
    s.skin_tone.uniformity = 80.;
    s.skin_tone.hue_range = 0.;
    let j = lightcraft_pipeline::ucs::xyy_to_jch(
        lightcraft_pipeline::ucs::xyz_to_xyy(lightcraft_pipeline::ucs::mul(&lightcraft_pipeline::ucs::mats().rgb_to_xyz, src.data[1000])),
        lightcraft_pipeline::ucs::y_to_l_star(1.),
    );
    s.skin_tone.reference = Some([j[0] as f64, j[1] as f64, j[2].to_degrees() as f64]);
    compare(g, &src, &SourceInfo::default(), &s, &RenderRequest::fit(97, 73), &GpuStages::default());
}

#[test]
fn develop_c_full_scalar_opacity_mixed_masks_and_skin_overlay_match() {
    use lightcraft_develop::{Effects, LayerTools, Light, MaskOp, SkinTone};
    let gpu_device = device().expect("develop-c acceptance requires a hardware Vulkan adapter");
    let g = &*gpu_device;
    eprintln!("develop-c GPU: {} {:?}", g.info.name, g.info.device_type);
    let _scope = crate::ctx::RenderScope::new(g);
    let src = input(47, 33);
    let info = SourceInfo::default();
    let req = RenderRequest::fit(47, 33);
    let stages = GpuStages::default();
    let mut s = DevelopSettings::default();
    let full = Mask {
        id: 1,
        opacity: 50.0,
        components: vec![MaskComponent { name: None, op: MaskOp::Add, invert: false, shape: MaskShape::Full }],
        tools: LayerTools { light: Some(Light { exposure: 1.0, ..Default::default() }), ..Default::default() },
        ..Default::default()
    };
    s.masks.push(full.clone());
    assert_eq!(crate::render::alpha_layout(&s, 47 * 33), vec![(u32::MAX, 0.5)]);
    compare(g, &src, &info, &s, &req, &stages);
    s.masks[0].tools.effects = Some(Effects { clarity: 20.0, ..Default::default() });
    compare(g, &src, &info, &s, &req, &stages); // primary's scalar mix
    let mut spatial = mask();
    spatial.id = 2;
    spatial.adjust.moire = 25.0;
    s.masks.push(spatial);
    s.masks.push(Mask { id: 3, ..full });
    assert_eq!(crate::render::alpha_layout(&s, 47 * 33), vec![(u32::MAX, 0.5), (0, 0.0), (u32::MAX, 0.5)]);
    compare(g, &src, &info, &s, &req, &stages); // packed chromaticity / spatial mask indexing
    s.tone_eq.enabled = true;
    s.tone_eq.ev4 = 0.5;
    compare(g, &src, &info, &s, &req, &stages);
    let overlay = lightcraft_pipeline::Overlay::Mask { id: 1, view: lightcraft_pipeline::MaskView::WhiteOnBlack, color: [255, 0, 0], opacity: 50 };
    compare(g, &src, &info, &s, &RenderRequest { overlay, ..req.clone() }, &stages);
    s.masks[0].components.push(MaskComponent {
        name: None,
        op: MaskOp::Subtract,
        invert: false,
        shape: MaskShape::Radial { center: Point::new(0.5, 0.5), rx: 0.2, ry: 0.2, angle: 0.0, feather: 30.0, invert: false },
    });
    compare(g, &src, &info, &s, &req, &stages); // Full kernel in a spatial mask
    s = DevelopSettings::default();
    let reference = lightcraft_pipeline::skin_reference_sample(&src, &info, &s, &req, Point::new(0.2, 0.5)).unwrap();
    s.skin_tone = SkinTone { reference: Some(reference), uniformity: 75.0, lightness: 20.0, ..Default::default() };
    s.color.saturation = 10.0; // range reference survives remaining primary settings
    compare(g, &src, &info, &s, &RenderRequest { overlay: lightcraft_pipeline::Overlay::SkinToneRange(None), ..req.clone() }, &stages);
    s.masks = vec![Mask {
        id: 7,
        components: vec![MaskComponent { name: None, op: MaskOp::Add, invert: false, shape: MaskShape::Full }],
        tools: LayerTools { skin_tone: Some(s.skin_tone), ..Default::default() },
        ..Default::default()
    }];
    s.skin_tone = Default::default();
    compare(g, &src, &info, &s, &RenderRequest { overlay: lightcraft_pipeline::Overlay::SkinToneRange(Some(7)), ..req }, &stages);
}

#[test]
fn sequential_stack_owner_order_composition_and_prefix_cache() {
    use lightcraft_develop::{LayerTools, Light, Mixer, ToneCurve};
    let gpu_device = crate::test_device().expect("stack acceptance requires physical GPU");
    let g = &*gpu_device;
    let adapter = format!("{} {:?}", g.info.name, g.info.backend);
    eprintln!("stack acceptance adapter: {adapter}");
    assert!(!adapter.to_lowercase().contains("llvmpipe"));
    let _scope = crate::ctx::RenderScope::new(g);
    let src = input(240, 160);
    let req = RenderRequest::fit(240, 160);
    let info = SourceInfo { raw: true, ..Default::default() };
    let stages = GpuStages::default();
    let mut s = DevelopSettings::default();
    s.light.exposure = 0.8;
    s.light.contrast = 45.0;
    s.light.highlights = -60.0;
    s.light.shadows = 50.0;
    s.curve.lights = 30.0;
    s.color.vibrance = 25.0;
    let mut colour = mask();
    colour.adjust = Default::default();
    colour.opacity = 100.0;
    let mut mixer = Mixer::default();
    mixer.red.hue = 70.0;
    mixer.red.sat = -30.0;
    mixer.blue.hue = -50.0;
    colour.tools = LayerTools { mixer: Some(mixer), ..Default::default() };
    s.masks.push(colour);
    let mut gradient = mask();
    gradient.id = 2;
    gradient.adjust = Default::default();
    gradient.opacity = 100.0;
    gradient.components[0].shape = MaskShape::Linear { start: Point::new(0., 0.), end: Point::new(0., 1.) };
    gradient.tools = LayerTools { light: Some(Light { exposure: -1.0, ..Default::default() }), ..Default::default() };
    s.masks.push(gradient);
    compare(g, &src, &info, &s, &req, &stages);
    assert_eq!(stages.stack_renders(), (1, 2));
    compare(g, &src, &info, &s, &req, &stages);
    assert_eq!(stages.stack_renders(), (0, 0));
    s.masks[1].tools.light.as_mut().unwrap().exposure = -0.5;
    compare(g, &src, &info, &s, &req, &stages);
    assert_eq!(stages.stack_renders(), (0, 1));
    s.light.contrast += 10.0;
    compare(g, &src, &info, &s, &req, &stages);
    assert_eq!(stages.stack_renders(), (1, 2));
    s.masks[0].components[0].shape = MaskShape::Full;
    s.masks[0].tools =
        LayerTools { curve: Some(ToneCurve { master: vec![Point::new(0., 0.12), Point::new(1., 1.)], ..Default::default() }), ..Default::default() };
    s.masks[1].components[0].shape = MaskShape::Full;
    let before = compare(g, &src, &info, &s, &req, &stages);
    s.masks.swap(0, 1);
    let after = compare(g, &src, &info, &s, &req, &stages);
    assert!(before.data != after.data, "layer order must change pixels");
    assert_eq!(stages.stack_renders(), (0, 2));
    let mut ev = Mask {
        components: vec![MaskComponent { name: None, op: Default::default(), invert: false, shape: MaskShape::Full }],
        tools: LayerTools { light: Some(Light { exposure: 0.5, ..Default::default() }), ..Default::default() },
        ..Default::default()
    };
    s.masks = vec![ev.clone(), Mask { id: 2, ..ev.clone() }];
    let halves = compare(g, &src, &info, &s, &req, &stages);
    s.masks.truncate(1);
    s.masks[0].tools.light.as_mut().unwrap().exposure = 1.0;
    let whole = compare(g, &src, &info, &s, &req, &stages);
    assert!(halves.data.iter().zip(&whole.data).all(|(a, b)| (0..3).all(|c| a[c].abs_diff(b[c]) <= 1)));
    s.masks[0].tools.light.as_mut().unwrap().exposure = 3.0;
    ev.tools.light.as_mut().unwrap().exposure = -3.0;
    s.masks.push(Mask { id: 2, ..ev });
    let recovered = compare(g, &src, &info, &s, &req, &stages);
    // Compare to the same developed Background boundary, without an exposure change.
    s.masks.truncate(1);
    s.masks[0].tools.light.as_mut().unwrap().exposure = 0.0;
    let baseline = compare(g, &src, &info, &s, &req, &stages);
    assert!(recovered.data == baseline.data, "over-white intermediate exposure must be recoverable");
}

#[test]
fn stacked_full_tool_sets_match_cpu_after_developed_inputs() {
    use lightcraft_develop::{
        ColorAdj, ColorGrading, Detail, Effects, Grain, LayerTools, Light, Mixer, PointColor, SkinTone, ToneCurve, Vignette, WhiteBalance,
    };
    let Some(gpu_device) = device() else { return };
    let g = &*gpu_device;
    let _scope = crate::ctx::RenderScope::new(g);
    let src = input(93, 61);
    let req = RenderRequest::fit(93, 61);
    let stages = GpuStages::default();
    let info = SourceInfo { raw: true, ..Default::default() };
    let mut mixer = Mixer::default();
    mixer.red.hue = 50.0;
    mixer.blue.sat = -50.0;
    mixer.orange.lum = 30.0;
    let mut grading = ColorGrading::default();
    grading.shadows.hue = 220.0;
    grading.shadows.sat = 40.0;
    grading.highlights.hue = 40.0;
    grading.highlights.sat = 30.0;
    let reference = lightcraft_pipeline::skin_reference_sample(&src, &info, &DevelopSettings::default(), &req, Point::new(0.2, 0.5)).unwrap();
    let cases = vec![
        LayerTools {
            light: Some(Light { exposure: 0.4, contrast: 30.0, highlights: -50.0, shadows: 40.0, whites: 20.0, blacks: -15.0 }),
            ..Default::default()
        },
        LayerTools { wb: Some(WhiteBalance { temp: 3500.0, tint: 20.0, ..Default::default() }), ..Default::default() },
        LayerTools {
            curve: Some(ToneCurve { lights: 35.0, blue: vec![Point::new(0.0, 0.03), Point::new(1.0, 0.9)], ..Default::default() }),
            ..Default::default()
        },
        LayerTools { color: Some(ColorAdj { vibrance: 35.0, saturation: -25.0 }), ..Default::default() },
        LayerTools { mixer: Some(mixer), ..Default::default() },
        LayerTools {
            point_colors: Some(vec![PointColor {
                lum: 0.6,
                chroma: 0.12,
                hue: 50.0,
                hue_shift: 70.0,
                sat_shift: -50.0,
                range: 100.0,
                ..Default::default()
            }]),
            ..Default::default()
        },
        LayerTools { grading: Some(grading), ..Default::default() },
        LayerTools {
            skin_tone: Some(SkinTone { reference: Some(reference), uniformity: 60.0, lightness: 25.0, ..Default::default() }),
            ..Default::default()
        },
        LayerTools {
            effects: Some(Effects { clarity: 25.0, texture: -35.0, structure: 40.0, dehaze: 20.0, ..Default::default() }),
            ..Default::default()
        },
        LayerTools {
            detail: Some(Detail { nr_luminance: 45.0, nr_color: 35.0, sharpen_amount: 80.0, sharpen_radius: 2.0, ..Default::default() }),
            ..Default::default()
        },
        LayerTools {
            vignette: Some(Vignette { amount: -40.0, ..Default::default() }),
            grain: Some(Grain { amount: 30.0, ..Default::default() }),
            ..Default::default()
        },
        LayerTools { treatment: Some(lightcraft_develop::Treatment::Bw), ..Default::default() },
    ];
    for (index, tools) in cases.into_iter().enumerate() {
        eprintln!("stacked tools parity case {index}");
        let mut s = DevelopSettings::default();
        s.light.exposure = 0.7;
        s.light.contrast = 35.0;
        s.light.highlights = -40.0;
        s.color.vibrance = 15.0;
        let mut lower = mask();
        lower.id = 1;
        lower.tools = LayerTools {
            light: Some(Light { exposure: 0.25, ..Default::default() }),
            curve: Some(ToneCurve { darks: 20.0, ..Default::default() }),
            ..Default::default()
        };
        let mut top = mask();
        top.id = 2;
        top.tools = tools;
        top.opacity = 80.0;
        s.masks = vec![lower, top];
        compare(g, &src, &info, &s, &req, &stages);
        s.masks[1].components = vec![MaskComponent { name: None, op: Default::default(), invert: false, shape: MaskShape::Full }];
        compare(g, &src, &info, &s, &req, &stages);
        let deep_req = RenderRequest { depth: lightcraft_pipeline::OutputDepth::U16, ..req.clone() };
        if index == 9 {
            // Existing 16-bit NR uses precise host VST transforms around native wavelets.
            // The preview above must stay fully native; export keeps its established precision path.
            crate::CPU_STAGES.with(|stages| stages.borrow_mut().clear());
            let errors = crate::ctx::ErrorScopes::push(g);
            let gpu = render(g, &src, &info, &s, &deep_req, Some(&stages), None, HsMethod::LiTone).unwrap();
            assert!(errors.pop().is_none());
            assert!(crate::ctx::take_failure().is_none());
            assert_eq!(crate::last_cpu_stages(), vec!["NR variance transform"]);
            let cpu = lightcraft_pipeline::render(&src, &info, &s, &deep_req);
            let mut sum = 0usize;
            let mut max = 0u8;
            for (a, b) in cpu.image.data.iter().zip(&gpu.image.data) {
                for c in 0..3 {
                    let d = a[c].abs_diff(b[c]);
                    sum += d as usize;
                    max = max.max(d);
                }
            }
            let mean = sum as f64 / (cpu.image.len() * 3) as f64;
            assert!(mean < 0.5 && max <= 3, "deep NR stack parity: mean {mean}, max {max}");
        } else {
            compare(g, &src, &info, &s, &deep_req, &stages);
        }
        assert_eq!(stages.stack_renders(), (1, 2), "deep NR/precision boundary has a distinct stack key");
    }
}

#[test]
fn stacked_hidden_and_imported_mask_overlays_match_warm_cpu() {
    use lightcraft_develop::{LayerTools, Light};
    let Some(gpu_device) = device() else { return };
    let g = &*gpu_device;
    let _scope = crate::ctx::RenderScope::new(g);
    let src = input(93, 61);
    let info = SourceInfo { raw: true, ..Default::default() };
    let req = RenderRequest::fit(93, 61);
    let stages = GpuStages::default();
    let mut s = DevelopSettings::default();
    s.light.contrast = 40.0;
    let mut layer = mask();
    layer.id = 1;
    layer.tools = LayerTools { light: Some(Light { exposure: 0.5, ..Default::default() }), ..Default::default() };
    let mut legacy = mask();
    legacy.id = 2;
    legacy.adjust.exposure = 0.3;
    let mut hidden = mask();
    hidden.id = 3;
    hidden.visible = false;
    s.masks = vec![layer, legacy, hidden];
    for id in [1, 2, 3] {
        let r = RenderRequest {
            overlay: lightcraft_pipeline::Overlay::Mask { id, view: lightcraft_pipeline::MaskView::WhiteOnBlack, color: [255, 0, 0], opacity: 50 },
            ..req.clone()
        };
        let cold = compare(g, &src, &info, &s, &r, &stages);
        let warm = compare(g, &src, &info, &s, &r, &stages);
        assert!(cold.data == warm.data);
    }
}

#[test]
fn camdata_profiled_nr_real_gpu_matches_cpu() {
    let gpu_device = device().expect("camera-profile test requires a real GPU");
    let g = &*gpu_device;
    let _scope = crate::ctx::RenderScope::new(g);
    let src = Arc::new(input(96, 64));
    let info = SourceInfo {
        camera_noise: Some(lightcraft_pipeline::detail::nr::NoiseCalibration {
            a: [0.001, 0.002, 0.003],
            b: [1e-6, -1e-7, 1e-6],
            camera_to_working: [[1.5, 0.1, -0.2], [-0.1, 0.9, 0.2], [0.0, -0.2, 1.3]],
        }),
        ..Default::default()
    };
    let mut settings = DevelopSettings::default();
    settings.detail.nr_luminance = 60.0;
    settings.detail.nr_color = 60.0;
    let req = RenderRequest::fit(96, 64);
    let errors = crate::ctx::ErrorScopes::push(g);
    let gpu = render(g, &src, &info, &settings, &req, Some(&GpuStages::default()), None, HsMethod::LiTone).unwrap().image;
    assert!(errors.pop().is_none());
    assert!(crate::last_cpu_stages().contains(&"profiled camera NR"));
    let cpu = lightcraft_pipeline::render(&src, &info, &settings, &req).image;
    let max = cpu.data.iter().zip(&gpu.data).flat_map(|(p, q)| (0..3).map(move |c| p[c].abs_diff(q[c]))).max().unwrap();
    assert!(max <= 3, "profiled NR CPU/GPU max difference {max}");
}
