//! Linear-float output, including cached adjustment layers, on a real GPU.
mod support;
use lightcraft_pipeline::{DeepSamples, OutputDepth, OutputSpace, RenderRequest, SourceInfo, StageCache};
use std::sync::Arc;

#[test]
#[ignore = "requires a real hardware GPU; run explicitly"]
fn cr_gpu_float_output_spaces_layers_and_limits() {
    let _guard = support::lock();
    assert!(lightcraft_gpu::available(), "{:?}", lightcraft_gpu::unavailable_reason());
    println!("Float GPU adapter: {:?}", lightcraft_gpu::adapter_name());
    let src = Arc::new(lightcraft_scenes::demo_library()[0].render(192, 128));
    let info = SourceInfo::default();
    let mut settings = lightcraft_develop::DevelopSettings::default();
    settings.light.exposure = 0.35;
    settings.grain.amount = 35.0;
    settings.grain.seed = 42;
    let mut mask = lightcraft_develop::Mask::default();
    mask.components.push(lightcraft_develop::MaskComponent {
        name: None,
        op: lightcraft_develop::MaskOp::Add,
        invert: false,
        shape: lightcraft_develop::MaskShape::Radial {
            center: lightcraft_geom::Point::new(0.4, 0.5),
            rx: 0.3,
            ry: 0.4,
            angle: 15.0,
            feather: 60.0,
            invert: false,
        },
    });
    mask.adjust.exposure = 0.3;
    mask.tools.curve = Some(lightcraft_develop::ToneCurve { lights: 15.0, ..Default::default() });
    settings.masks.push(mask);
    let stages = StageCache::default();
    for space in OutputSpace::ALL {
        let req = RenderRequest { space, depth: OutputDepth::F32Linear, ..RenderRequest::fit(192, 128) };
        let cpu = lightcraft_pipeline::render(&src, &info, &settings, &req);
        let DeepSamples::F32(cpu) = cpu.deep.unwrap().samples else { panic!() };
        let gpu = lightcraft_gpu::render(&src, &info, &settings, &req, Some(&stages)).unwrap();
        let DeepSamples::F32(gpu) = gpu.deep.unwrap().samples else { panic!() };
        let max = cpu.iter().zip(&gpu).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
        let mean = cpu.iter().zip(&gpu).map(|(a, b)| (a - b).abs()).sum::<f32>() / cpu.len() as f32;
        println!("Float stack {space:?} mean={mean:.8} max={max:.8}");
        assert!(mean * 255.0 < 0.5 && max * 255.0 <= 3.0);
        assert!(gpu.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v)));
        let again = lightcraft_gpu::render(&src, &info, &settings, &req, Some(&stages)).unwrap();
        assert_eq!(again.deep.unwrap().samples, DeepSamples::F32(gpu));
    }
    // 3-word RGB fits; 4-word float output + sentinel does not. Refuse before dispatch.
    let req = RenderRequest { depth: OutputDepth::F32Linear, ..RenderRequest::fit(192, 128) };
    lightcraft_gpu::inject_fault(lightcraft_gpu::Fault::Limit((192 * 128 * 3 * 4 + 16) as u64));
    assert!(lightcraft_gpu::render(&src, &info, &settings, &req, None).is_none());
    assert!(lightcraft_gpu::last_fallback().unwrap().contains("limit"));
    assert!(!lightcraft_gpu::recovering());
    drop(stages);
}
