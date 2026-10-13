//! Exercise the production final canvas pass on hardware, without opening a window.
use super::*;

#[test]
fn display_output_hardware_10bit_and_dithered_8bit_readback() {
    let _gate = crate::gpu_test_support::lock();
    let base = egui_kittest::wgpu::create_render_state(wgpu_setup(), Default::default());
    let info = base.adapter.get_info();
    assert_ne!(info.device_type, wgpu::DeviceType::Cpu, "hardware GPU required: {info:?}");
    eprintln!("display output GPU: {info:?}");
    let size = [1024u32, 16u32];
    let mut buf = photocraft_compose::Buffer::transparent(photocraft_geom::Rect::new(0, 0, size[0] as i32, size[1] as i32));
    for (i, p) in buf.px.iter_mut().enumerate() {
        let v = 0.2 + 0.6 * (i % size[0] as usize) as f32 / 1023.0;
        *p = [v, v, v, 1.0];
    }
    for format in [wgpu::TextureFormat::Rgba8Unorm, wgpu::TextureFormat::Rgb10a2Unorm] {
        let mut rs = base.clone();
        rs.target_format = format;
        let gpu = GpuCanvas::new(&rs);
        gpu.upload_buffer_full(1, &buf, photocraft_doc::SampleType::U16);
        assert_eq!(gpu.read_texels(1).unwrap().0, FORMAT_HIGH);
        let mut color = photocraft_engine::color_cmds::ColorState::default();
        color.settings.monitor_profile = "display-p3".into();
        let doc = photocraft_doc::Document::with_background(
            "ramp",
            photocraft_doc::Size::new(size[0], size[1]),
            photocraft_doc::ColorMode::Rgb,
            photocraft_doc::SampleType::U16,
            photocraft_doc::Color::WHITE,
        );
        let high = format == wgpu::TextureFormat::Rgb10a2Unorm;
        let bytes = if high { color.gpu_canvas_lut16f_for(&doc, 33, None) } else { color.gpu_canvas_lut_for(&doc, 33, None) }.unwrap().unwrap();
        gpu.set_display_lut_format(1, 0, 33, Some(&bytes), if high { FORMAT_HIGH } else { FORMAT });
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(size[0] as f32, size[1] as f32));
        let cb = CanvasCallback {
            rect,
            params: ViewParams {
                doc: 1,
                doc_size: size,
                zoom: 1.0,
                center: [512.0, 8.0],
                shadow: false,
                pixel_grid: false,
                view_key: 7,
                display: 1,
                output: 0,
                hdr: None,
                flip: false,
                rotation: 0.0,
            },
        };
        let tex = rs.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("display-output-readback"),
            size: wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let mut encoder = rs.device.create_command_encoder(&Default::default());
        let mut renderer = rs.renderer.write();
        cb.prepare(&rs.device, &rs.queue, &ScreenDescriptor { size_in_pixels: size, pixels_per_point: 1.0 }, &mut encoder, &mut renderer.callback_resources);
        {
            let view = tex.create_view(&Default::default());
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("display-output"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                })
                .forget_lifetime();
            cb.paint(
                egui::PaintCallbackInfo { viewport: rect, clip_rect: rect, pixels_per_point: 1.0, screen_size_px: size },
                &mut pass,
                &renderer.callback_resources,
            );
        }
        drop(renderer);
        let read = rs.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("display-output-copy"),
            size: u64::from(size[0] * size[1] * 4),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo { texture: &tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyBufferInfo {
                buffer: &read,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(size[0] * 4), rows_per_image: Some(size[1]) },
            },
            wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 },
        );
        rs.queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        read.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        assert!(gpu.health.wait(&rs.device, None));
        rx.recv().unwrap().unwrap();
        let data = read.slice(..).get_mapped_range().unwrap();
        let values: Vec<u16> = data.as_chunks::<4>().0.iter().map(|p| if high { (u32::from_le_bytes(*p) & 1023) as u16 } else { u16::from(p[0]) }).collect();
        let unique: std::collections::HashSet<_> = values.iter().copied().collect();
        if high {
            assert!(unique.len() > 500, "10-bit ramp only {} codes", unique.len());
        } else {
            assert!(unique.len() > 150 && unique.len() < 180, "8-bit ramp: {} codes", unique.len());
        }
        let levels = if high { 1023.0 } else { 255.0 };
        let mean = values.iter().map(|v| f32::from(*v) / levels).sum::<f32>() / values.len() as f32;
        assert!((mean - 0.5).abs() < 0.001, "mean {mean}");
        assert_eq!(gpu.output_depth(), if high { 10 } else { 8 });
        eprintln!("display readback {format:?}: {} distinct codes, mean {mean}", unique.len());
    }
}

#[test]
#[ignore = "interactive hardware timing: run explicitly, no GUI window"]
fn display_output_app_frame_timing() {
    let _gate = crate::gpu_test_support::lock();
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1100.0, 760.0)).wgpu_setup(wgpu_setup()).build_eframe(|cc| {
        crate::PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        let mut app = crate::PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        let rs = cc.wgpu_render_state.as_ref().unwrap();
        assert_ne!(rs.adapter.get_info().device_type, wgpu::DeviceType::Cpu);
        eprintln!("display frame timing adapter: {:?}", rs.adapter.get_info());
        app.set_wgpu(rs.clone());
        app
    });
    let doc = photocraft_doc::Document::with_background(
        "display timing",
        photocraft_doc::Size::new(1536, 1024),
        photocraft_doc::ColorMode::Rgb,
        photocraft_doc::SampleType::U16,
        photocraft_doc::Color::rgb(0.5, 0.5, 0.5),
    );
    h.state_mut().session.add_document(doc, None);
    h.state_mut().session.color.settings.monitor_profile = "display-p3".into();
    h.run_steps(8);
    h.render().unwrap();
    let mut times = Vec::new();
    for _ in 0..30 {
        let start = std::time::Instant::now();
        h.run_steps(1);
        h.render().unwrap();
        times.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    times.sort_by(f64::total_cmp);
    assert!(h.state().gpu_active());
    eprintln!("display real app frame + offscreen render/readback: median {:.3} ms, p95 {:.3} ms; {}", times[15], times[28], h.state().perf.last_refresh);
}
