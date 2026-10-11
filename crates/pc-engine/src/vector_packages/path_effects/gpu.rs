//! Supplied integration gate; prohibited from execution by the final task speed rule.
//! Strict real RTX 5090/Vulkan/discrete/Rgba32Float. No window or software-adapter pass.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
#[path = "scaled_fixture.rs"]
pub mod scaled;
#[test]
#[ignore = "integration GPU gate; task speed rules prohibit execution"]
fn path_effects45_path_effects_on_shapes_rtx5090_preview_release_parity() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor { backends: wgpu::Backends::VULKAN, ..wgpu::InstanceDescriptor::new_without_display_handle() });
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).expect("RTX 5090 Vulkan required");
    let info = adapter.get_info();
    assert_eq!(info.backend, wgpu::Backend::Vulkan);
    assert_eq!(info.device_type, wgpu::DeviceType::DiscreteGpu);
    assert!(info.name.contains("RTX 5090"), "{info:?}");
    assert_eq!(photocraft_gpu::Compositor::preferred_acc_format(&adapter), wgpu::TextureFormat::Rgba32Float);
    eprintln!("path_effects_on_shapes adapter: {info:?}");
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let mut gpu = photocraft_gpu::Compositor::try_new_with_format(&device, wgpu::TextureFormat::Rgba32Float).unwrap();
    for mut effect in EffectKind::presets() {
        match &mut effect {
            EffectKind::PuckerBloat { amount } => *amount = 60.,
            EffectKind::Twist { angle } => *angle = 90.,
            EffectKind::Transform { scale, move_by, angle, copies, .. } => {
                *scale = [120., 80.];
                *move_by = [12., 4.];
                *angle = 15.;
                *copies = 3;
            }
            EffectKind::FreeDistort { corners } => *corners = [[0., 0.1], [1., 0.], [0.8, 1.], [0.1, 0.9]],
            _ => {}
        }
        let (mut s, ids) = tests::fixture();
        let before = s.active().unwrap().doc.clone();
        let mut g = Gesture::begin(&s, &ids, tests::access_test()).unwrap();
        g.update(&s, |e| {
            e.push(PathEffect::new(effect.clone()));
            Ok(())
        })
        .unwrap();
        let preview = prepare(&s, &g.request, tests::access_test()).unwrap().preview;
        assert_eq!(s.active().unwrap().doc, before);
        assert!(s.journal.is_empty());
        commit_with_access(&mut s, &g.request, tests::access_test()).unwrap();
        let released = project_document(&s.active().unwrap().doc, tests::access_test(), &mut ProjectionCache::default()).unwrap();
        assert_eq!(photocraft_compose::flatten(&preview).px, photocraft_compose::flatten(&released).px);
        for scale in [1., 2., 4., 8.] {
            let mut preview_pixels: Option<Vec<[f32; 4]>> = None;
            for shown in [&preview, &released] {
                let scaled = scaled::project(shown, scale).expect("09 shared scaled projection required");
                let shown = &scaled;
                let cpu = photocraft_compose::flatten(shown);
                let pixels = photocraft_gpu::render_to_vec(&mut gpu, &device, &queue, shown, shown.bounds()).expect("GPU rendering cannot skip");
                assert_eq!(cpu.px.len(), pixels.len());
                for (a, b) in cpu.px.iter().zip(&pixels) {
                    for c in 0..4 {
                        let x = if c == 3 { a[c] } else { a[c] * a[3] };
                        let y = if c == 3 { b[c] } else { b[c] * b[3] };
                        assert!(x.is_finite() && y.is_finite() && (x - y).abs() <= 1. / 255.);
                    }
                }
                if let Some(first) = &preview_pixels {
                    assert_eq!(first.len(), pixels.len());
                    for (a, b) in first.iter().zip(&pixels) {
                        for c in 0..4 {
                            let x = if c == 3 { a[c] } else { a[c] * a[3] };
                            let y = if c == 3 { b[c] } else { b[c] * b[3] };
                            assert!((x - y).abs() <= 1. / 255., "Preview/release changed at scale {scale}");
                        }
                    }
                } else {
                    preview_pixels = Some(pixels);
                }
            }
        }
        assert!(s.undo());
        assert_eq!(s.active().unwrap().doc, before);
    }
}
