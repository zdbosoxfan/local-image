//! A stack whose uniform records don't fit the device's buffer limit is refused before any GPU
//! work, so the caller falls back to the CPU and the device stays healthy.

use li_gpu::health::DeviceHealth;
use photocraft_color::{Color, ColorMode, SampleType};
use photocraft_doc::{Document, Fill, Layer, LayerContent};
use photocraft_geom::{Rect, Size};
use photocraft_gpu::{Compositor, render_to_vec};

fn fills(count: usize, size: Size) -> Document {
    let mut doc = Document::new("Deep stack", size, ColorMode::Rgb, SampleType::U8);
    for _ in 0..count {
        doc.layers.push(Layer::new("Fill", LayerContent::Fill(Fill::Solid(Color::rgba(0.25, 0.5, 0.75, 0.5)))));
    }
    doc
}

/// A device limited to 64 KiB buffers: 256 uniform records of 256 bytes.
fn constrained_device() -> li_testkit::gpu::HardwareGpu {
    let gpu = li_testkit::gpu::hardware_device_with("uniform limits", |_| wgpu::DeviceDescriptor {
        required_limits: wgpu::Limits {
            max_buffer_size: 65_536,
            max_storage_buffer_binding_size: 65_536,
            max_uniform_buffer_binding_size: 65_536,
            ..Default::default()
        },
        ..Default::default()
    });
    assert_eq!(gpu.device.limits().max_buffer_size, 65_536);
    gpu
}

#[test]
fn oversized_uniforms_leave_the_device_healthy_and_exact_limit_renders() {
    let _lock = li_testkit::gpu::lock();
    let gpu = constrained_device();
    let mut comp = Compositor::try_new_with_format(&gpu.device, Compositor::preferred_acc_format(&gpu.adapter)).unwrap();
    let health = DeviceHealth::watch(&gpu.device);
    comp.set_health(health.clone());
    let deep = fills(150, Size::new(1, 1));
    assert!(render_to_vec(&mut comp, &gpu.device, &gpu.queue, &deep, deep.bounds()).is_err());
    assert_eq!(health.errors(), 0, "no invalid GPU operation is issued before falling back");
    assert!(health.is_ok());
    // One chunk record plus 255 pass records fill the buffer exactly.
    let boundary = fills(127, Size::new(1, 1));
    assert_eq!(photocraft_gpu::plan(&boundary).unwrap().passes.len(), 255);
    let actual = render_to_vec(&mut comp, &gpu.device, &gpu.queue, &boundary, boundary.bounds()).unwrap();
    let expected = photocraft_compose::flatten(&boundary);
    for (cpu, gpu) in expected.px[0].iter().zip(actual[0]) {
        assert!((cpu - gpu).abs() <= 1.0 / 255.0, "boundary render differs from the CPU: {cpu} vs {gpu}");
    }
    assert_eq!(health.errors(), 0);
}

#[test]
fn a_later_oversized_page_cell_is_rejected_before_render_or_encode_output() {
    let _lock = li_testkit::gpu::lock();
    let gpu = constrained_device();
    let health = DeviceHealth::watch(&gpu.device);
    // 253 passes: the narrow first page cell (2 chunks) fits, the later full one (4 chunks) doesn't.
    let doc = fills(126, Size::new(4096, 2048));
    assert_eq!(photocraft_gpu::plan(&doc).unwrap().passes.len(), 253);
    let region = Rect::new(2047, 0, 4096, 2048);
    for encode_only in [false, true] {
        let mut comp = Compositor::try_new_with_format(&gpu.device, Compositor::preferred_acc_format(&gpu.adapter)).unwrap();
        comp.set_health(health.clone());
        assert_eq!((comp.page_size(), photocraft_gpu::CHUNK), (2048, 1024));
        let mut calls = 0;
        let mut sink = |_: &mut wgpu::CommandEncoder, _: photocraft_gpu::ChunkOut<'_>| calls += 1;
        let result = if encode_only {
            let mut encoder = gpu.device.create_command_encoder(&Default::default());
            comp.encode(&gpu.device, &gpu.queue, &mut encoder, &doc, region, &mut sink)
        } else {
            comp.render(&gpu.device, &gpu.queue, &doc, region, &mut sink)
        };
        assert!(result.is_err());
        assert_eq!(calls, 0, "a later oversized cell must be rejected before any output");
        assert_eq!(health.errors(), 0);
        assert!(health.is_ok());
    }
}
