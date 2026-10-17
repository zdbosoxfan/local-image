//! Opt-in hardware regression for the editor's shared Develop bridge.
use crate::develop_filter::*;
use lightcraft_pipeline::{DeepSamples, OutputDepth, OutputSpace, RenderRequest, SourceInfo};
use std::sync::Arc;

struct Shutdown;
impl Drop for Shutdown {
    fn drop(&mut self) {
        assert!(lightcraft_gpu::shutdown(std::time::Duration::from_secs(120)) || std::thread::panicking());
    }
}

#[test]
#[ignore = "requires a real hardware GPU; run this isolated test explicitly"]
fn cr_gpu_camera_raw_controls_parity_and_recovery() {
    let _shutdown = Shutdown;
    assert!(lightcraft_gpu::available(), "{:?}", lightcraft_gpu::unavailable_reason());
    let adapter = lightcraft_gpu::adapter_name().unwrap();
    assert!(!adapter.to_lowercase().contains("llvmpipe"));
    println!("CR GPU adapter: {adapter}");
    let (w, h) = (192, 128);
    // Smooth colour gradients plus fine detail: neither a constant image nor white noise alone.
    let rgba: Vec<_> = (0..w * h)
        .map(|i| {
            let x = (i % w) as f32 / (w - 1) as f32;
            let y = (i / w) as f32 / (h - 1) as f32;
            let noise = (i * 17 % 31) as f32 / 30.0 * 0.025;
            [0.02 + 0.8 * x + noise, 0.04 + 0.7 * y + noise, 0.15 + 0.35 * x * y, (i % 7) as f32 / 6.0]
        })
        .collect();
    let src = Arc::new(li_raster::RgbFrame { width: w, height: h, data: rgba.iter().map(|p| [p[0], p[1], p[2]]).collect() });
    let req = RenderRequest { space: OutputSpace::Rec2020, depth: OutputDepth::F32Linear, ..RenderRequest::fit(w, h) };
    let cases = [
        ("exposure", serde_json::json!({"exposure":0.7})),
        ("tone", serde_json::json!({"contrast":25,"highlights":-35,"shadows":30,"whites":20,"blacks":-15})),
        ("WB", serde_json::json!({"temperature":20,"tint":-15})),
        ("colour", serde_json::json!({"vibrance":30,"saturation":-20})),
        ("curves", serde_json::json!({"curveHighlights":20,"curveLights":15,"curveDarks":-12,"curveShadows":-15,"pointCurveRed":[[0,0],[128,145],[255,255]]})),
        ("mixer", serde_json::json!({"hslHue":[15,0,-20,0,0,0,0,0],"hslSat":[0,20,0,0,0,0,0,0],"hslLum":[0,0,0,15,0,0,0,0]})),
        (
            "grading",
            serde_json::json!({"gradeShadows":{"hue":220,"sat":25,"lum":10},"gradeHighlights":{"hue":40,"sat":20,"lum":-10},"gradeBalance":15,"gradeBlending":40}),
        ),
        ("dehaze +", serde_json::json!({"dehaze":40})),
        ("dehaze -", serde_json::json!({"dehaze":-30})),
        ("texture +", serde_json::json!({"texture":35})),
        ("texture -", serde_json::json!({"texture":-35})),
        ("clarity +", serde_json::json!({"clarity":30})),
        ("clarity -", serde_json::json!({"clarity":-30})),
        ("NR luminance", serde_json::json!({"noiseLuminance":45,"noiseLuminanceDetail":60})),
        ("NR colour", serde_json::json!({"noiseColor":35,"noiseColorDetail":60})),
        ("sharpen", serde_json::json!({"sharpenAmount":45,"sharpenRadius":1.5,"sharpenDetail":35,"sharpenMasking":25})),
        ("grain", serde_json::json!({"grainAmount":45,"grainSize":70,"grainRoughness":60,"seed":42})),
        ("vignette", serde_json::json!({"vignetteAmount":-30,"vignetteMidpoint":35,"vignetteRoundness":20,"vignetteFeather":65,"vignetteHighlights":20})),
        ("combined", serde_json::json!({"exposure":0.3,"dehaze":20,"texture":25,"clarity":15,"noiseLuminance":35,"noiseColor":30,"grainAmount":25,"seed":42})),
    ];
    for scale in [1.0f32, 0.25] {
        let info = SourceInfo { sensor_scale: scale.recip(), ..Default::default() };
        for (name, params) in &cases {
            let cr = serde_json::from_value(params.clone()).unwrap();
            let settings = from_camera_raw(&cr, identity());
            let cpu = lightcraft_pipeline::render(&src, &info, &settings, &req);
            let gpu = lightcraft_gpu::render(&src, &info, &settings, &req, None).expect("must actually render on GPU");
            let DeepSamples::F32(cpu) = cpu.deep.unwrap().samples else { panic!() };
            let DeepSamples::F32(gpu) = gpu.deep.unwrap().samples else { panic!() };
            let diffs: Vec<_> = cpu.iter().zip(&gpu).map(|(a, b)| (a - b).abs()).collect();
            let mean = diffs.iter().sum::<f32>() / diffs.len() as f32;
            let max = diffs.iter().copied().fold(0.0f32, f32::max);
            let rmse = (diffs.iter().map(|v| v * v).sum::<f32>() / diffs.len() as f32).sqrt();
            println!("CR parity {name} scale={scale} mean={mean:.8} max={max:.8} rmse={rmse:.8}");
            // Existing GPU acceptance bounds, applied to unquantized linear samples.
            assert!(mean * 255.0 < 0.5 && max * 255.0 <= 3.0, "{name}: {mean} / {max}");
            let conv = Conv::new(photocraft_cms::Builtin::Srgb.profile()).unwrap();
            let mut cpu_display = cpu.as_chunks::<3>().0.to_vec();
            let mut gpu_display = gpu.as_chunks::<3>().0.to_vec();
            conv.from_working(&mut cpu_display);
            conv.from_working(&mut gpu_display);
            let quant = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            let display_diff: Vec<_> = cpu_display.as_flattened().iter().zip(gpu_display.as_flattened()).map(|(a, b)| quant(*a).abs_diff(quant(*b))).collect();
            let display_mean = display_diff.iter().map(|v| f64::from(*v)).sum::<f64>() / display_diff.len() as f64;
            let display_max = display_diff.iter().copied().max().unwrap();
            println!("CR display {name} scale={scale} mean_lsb={display_mean:.6} max_lsb={display_max}");
            assert!(display_mean < 0.5 && display_max <= 3, "{name}: display {display_mean} / {display_max}");
            let mut bridge = rgba.clone();
            develop_rgba_scaled(&mut bridge, w, h, &settings, linear_rec2020(), scale).unwrap();
            for ((actual, original), expected) in bridge.iter().zip(&rgba).zip(gpu.as_chunks::<3>().0) {
                for c in 0..3 {
                    assert!((actual[c] - expected[c]).abs() < 2e-6, "bridge must use float GPU output: {name}");
                }
                assert_eq!(actual[3].to_bits(), original[3].to_bits());
            }
        }
    }
    let mut settings = identity().clone();
    settings.light.exposure = 0.5;
    let DeepSamples::F32(cpu) = lightcraft_pipeline::render(&src, &SourceInfo::default(), &settings, &req).deep.unwrap().samples else { panic!() };
    for fault in [lightcraft_gpu::Fault::Limit(1024), lightcraft_gpu::Fault::DropWork, lightcraft_gpu::Fault::Validation] {
        lightcraft_gpu::inject_fault(fault);
        let actual = render_working(src.data.clone(), w, h, &settings).unwrap();
        assert_eq!(actual.as_flattened(), cpu, "failed GPU work must return CPU pixels");
        assert!(lightcraft_gpu::last_fallback().is_some());
        if fault != lightcraft_gpu::Fault::Limit(1024) {
            assert!(lightcraft_gpu::recovering());
            // Use the production automatic retry, without reset_failures.
            std::thread::sleep(std::time::Duration::from_millis(4100));
        }
        assert!(lightcraft_gpu::render(&src, &SourceInfo::default(), &settings, &req, None).is_some());
        assert!(!lightcraft_gpu::recovering());
    }
    // Native Apply and the legacy Lens Correction adapter use the same bridge,
    // preserving document encoding, HDR/gamut residuals and exact alpha bits.
    let area = photocraft_geom::Rect::new(0, 0, w as i32, h as i32);
    let mut surface = photocraft_raster::Surface::new(photocraft_color::PixelFormat::RGBA32F);
    let mut pixels = rgba.clone();
    pixels[0] = [-0.05, 1.2, 0.3, 0.123456];
    surface.write_region(area, pixels.as_flattened());
    let profile = photocraft_cms::Builtin::ProPhotoCompat.profile();
    let cr = photocraft_algo::camera_raw::CameraRaw { exposure: 0.5, texture: 25.0, grain_amount: 20.0, seed: 42, ..Default::default() };
    let developed = camera_raw_surface(&surface, area, &cr, profile).unwrap();
    lightcraft_gpu::inject_fault(lightcraft_gpu::Fault::Limit(1024));
    let cpu_surface = camera_raw_surface(&surface, area, &cr, profile).unwrap();
    let gpu_pixels = developed.read_region(area);
    let cpu_pixels = cpu_surface.read_region(area);
    let original = surface.read_region(area);
    for ((gpu, cpu), original) in gpu_pixels.as_chunks::<4>().0.iter().zip(cpu_pixels.as_chunks::<4>().0).zip(original.as_chunks::<4>().0) {
        for c in 0..3 {
            assert!((gpu[c] - cpu[c]).abs() * 255.0 <= 3.0);
        }
        assert_eq!(gpu[3].to_bits(), original[3].to_bits());
        assert_eq!(cpu[3].to_bits(), original[3].to_bits());
    }
    assert_eq!(develop_surface(&surface, area, identity(), profile).unwrap().read_region(area), original);
    // Unsupported full-settings stage uses the Library's existing CPU policy.
    let mut mask = lightcraft_develop::Mask { refine: 25.0, ..Default::default() };
    mask.components.push(lightcraft_develop::MaskComponent {
        name: None,
        op: lightcraft_develop::MaskOp::Add,
        invert: false,
        shape: lightcraft_develop::MaskShape::Radial {
            center: lightcraft_geom::Point::new(0.5, 0.5),
            rx: 0.3,
            ry: 0.3,
            angle: 0.0,
            feather: 50.0,
            invert: false,
        },
    });
    mask.adjust.exposure = 0.5;
    settings.masks.push(mask);
    let DeepSamples::F32(cpu) = lightcraft_pipeline::render(&src, &SourceInfo::default(), &settings, &req).deep.unwrap().samples else { panic!() };
    assert_eq!(render_working(src.data.clone(), w, h, &settings).unwrap().as_flattened(), cpu);
}

#[test]
#[ignore = "requires a real hardware GPU; run this isolated test explicitly"]
fn cr_gpu_raw_smart_refresh_matches_library_and_falls_back() {
    let _shutdown = Shutdown;
    assert!(lightcraft_gpu::available());
    use lightcraft_raw::testgen::{DngSpec, mosaic, scene};
    let (w, h) = (96, 64);
    let mut spec = DngSpec::cfa(w, h, mosaic(&scene(w, h), w, [0, 1, 1, 2], 0, 65535));
    spec.as_shot_neutral = Some([0.6, 1.0, 0.8]);
    let bytes = spec.build();
    let (src, info) = lightcraft_engine::files::load_bytes(&bytes, usize::MAX).unwrap();
    assert!(info.raw);
    let src = Arc::new(src);
    let mut settings = DevelopSettings::for_raw(info.as_shot_temp, info.as_shot_tint);
    settings.light.exposure = 0.5;
    settings.effects.dehaze = 20.0;
    settings.detail.nr_luminance = 25.0;
    let req = RenderRequest { space: OutputSpace::ProPhoto, depth: OutputDepth::U16, ..RenderRequest::fit(src.width, src.height) };
    let gpu = lightcraft_gpu::render(&src, &info, &settings, &req, None).expect("raw GPU must render");
    let deep = gpu.deep.unwrap();
    let DeepSamples::U16(samples) = deep.samples else { panic!() };
    let image = photocraft_codecs::Image::from_u16(deep.width as u32, deep.height as u32, photocraft_codecs::ChannelLayout::Rgb, &samples).unwrap();
    let expected = crate::flat::image_to_document("refresh.dng", &image).unwrap();
    let actual = crate::raw::develop_with_settings("refresh.dng", &bytes, &settings.to_json()).unwrap();
    let pixels = |r: &crate::ImportResult| r.document.layers[0].surface().unwrap().read_region(r.document.bounds());
    assert_eq!(pixels(&actual), pixels(&expected));
    lightcraft_gpu::set_enabled(false);
    let fallback = crate::raw::develop_with_settings("refresh.dng", &bytes, &settings.to_json()).unwrap();
    let cpu = lightcraft_pipeline::render(&src, &info, &settings, &req).deep.unwrap();
    let DeepSamples::U16(samples) = cpu.samples else { panic!() };
    let image = photocraft_codecs::Image::from_u16(cpu.width as u32, cpu.height as u32, photocraft_codecs::ChannelLayout::Rgb, &samples).unwrap();
    assert_eq!(pixels(&fallback), pixels(&crate::flat::image_to_document("cpu.dng", &image).unwrap()));
    let float_req = RenderRequest { space: OutputSpace::Rec2020, depth: OutputDepth::F32Linear, ..RenderRequest::fit(src.width, src.height) };
    let rendered = lightcraft_pipeline::render(&src, &SourceInfo::default(), &settings, &float_req);
    let DeepSamples::F32(cpu) = rendered.deep.unwrap().samples else { panic!() };
    assert_eq!(render_working(src.data.clone(), src.width, src.height, &settings).unwrap().as_flattened(), cpu);
    lightcraft_gpu::set_enabled(true);
    assert!(lightcraft_gpu::available());
}
