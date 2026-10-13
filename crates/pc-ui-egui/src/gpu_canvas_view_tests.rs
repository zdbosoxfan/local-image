//! Owned package 09 gates: offscreen app, strict hardware identity, real pointer input.
use super::*;
use crate::PhotocraftApp;
use egui_kittest::Harness;
use photocraft_doc::{Color, ColorMode, Document, Fill, Layer, LayerContent, Path, SampleType, ShapeLayer, Size, Subpath};
type H = Harness<'static, PhotocraftApp>;
static GPU: std::sync::Mutex<()> = std::sync::Mutex::new(());
fn harness(ppp: f32) -> H {
    let mut setup = wgpu_setup();
    use_adapter_limits_with(&mut setup, |a| {
        let i = a.get_info();
        #[cfg(not(target_os = "macos"))]
        assert!(i.name.contains("RTX 5090"), "{i:?}");
        assert_eq!(i.backend, if cfg!(target_os = "macos") { wgpu::Backend::Metal } else { wgpu::Backend::Vulkan });
        #[cfg(not(target_os = "macos"))]
        assert_eq!(i.device_type, wgpu::DeviceType::DiscreteGpu);
        #[cfg(target_os = "macos")]
        assert_ne!(i.device_type, wgpu::DeviceType::Cpu, "hardware Metal adapter required");
        assert_eq!(photocraft_gpu::Compositor::preferred_acc_format(a), wgpu::TextureFormat::Rgba32Float);
        assert!(supports_f16_canvas(a));
        eprintln!("scaled-view gate {i:?}");
    });
    let mut h = Harness::builder().with_size(egui::vec2(1000.0, 700.0)).with_pixels_per_point(ppp).wgpu_setup(setup).build_eframe(|cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        app.set_wgpu(cc.wgpu_render_state.as_ref().expect("required real GPU").clone());
        app
    });
    h.state_mut().ui.extras.rulers = false;
    h.state_mut().ui.extras.snap = false;
    h.state_mut().ui.view.show.pixel_grid = false;
    h.state_mut().ui.view.show.layer_edges = false;
    h.state_mut().session.prefs.edit(|p| p.interface.contextual_task_bar = false);
    h
}
fn fixture() -> Document {
    let mut d = Document::with_background("scaled app", Size::new(160, 120), ColorMode::Rgb, SampleType::U8, Color::WHITE);
    let sh = ShapeLayer {
        path: Path::new(vec![Subpath::polygon(&[(28.25, 30.5), (128.25, 34.5), (96.5, 90.25)])]),
        fill: Some(Fill::Solid(Color::BLACK)),
        ..Default::default()
    };
    d.layers.push(Layer::new("crisp triangle", LayerContent::Shape(sh)));
    d
}
fn add(h: &mut H, d: Document, zoom: f32) {
    h.state_mut().session.add_document(d, None);
    h.state_mut().sync_views();
    h.state_mut().ui.views[0].zoom = zoom;
    h.state_mut().ui.views[0].center = [80.0, 60.0];
    h.state_mut().ui.views[0].fit_pending = false;
    h.run_steps(3);
}
fn assert_app_pixels(h: &mut H, doc: &Document, scale: f64) {
    h.event(egui::Event::PointerGone);
    h.step();
    let bucket = 2.0f64.powf((scale.log2() * 4.0).ceil() / 4.0);
    let reference = scaled_view::cpu::flatten_scaled_with(doc, doc.bounds(), bucket, scaled_view::project).unwrap().buffer();
    let img = h.render().expect("live offscreen GPU canvas");
    let app = h.state();
    let view = &app.ui.views[0];
    let rect = crate::rulers::content_rect(app, app.last_canvas_rect);
    let cb = CanvasCallback {
        rect,
        params: ViewParams {
            doc: doc.id.0,
            doc_size: [doc.size.width, doc.size.height],
            zoom: view.zoom,
            center: view.center,
            shadow: false,
            pixel_grid: false,
            view_key: 0,
            display: 0,
            output: 0,
            hdr: None,
            flip: app.ui.view.flip_horizontal,
            rotation: view.rotation,
        },
    };
    let ppp = h.ctx.pixels_per_point();
    let (origin, physical) = cb.placement(ppp);
    assert!((f64::from(physical) - scale).abs() < 1e-6);
    let mut count = 0;
    let mut max = 0.0f32;
    let mut worst = None;
    for y in 0..img.height() {
        for x in 0..img.width() {
            let dx = (x as f32 + 0.5 - origin[0]) / physical;
            let dy = (y as f32 + 0.5 - origin[1]) / physical;
            let (sin, cos) = cb.params.rotation.sin_cos();
            let sx = (cos * dx + sin * dy) * if cb.params.flip { -1.0 } else { 1.0 };
            let sy = -sin * dx + cos * dy;
            if !rect.shrink(16.0).contains(egui::pos2((x as f32 + 0.5) / ppp, (y as f32 + 0.5) / ppp)) || sx < 1.0 || sy < 1.0 || sx >= 159.0 || sy >= 119.0 {
                continue;
            }
            // Independent level-zero bilinear oracle, including samples across tile edges.
            let (rx, ry) = (f64::from(sx) * bucket - 0.5, f64::from(sy) * bucket - 0.5);
            let (ix, iy) = (rx.floor() as i32, ry.floor() as i32);
            let (fx, fy) = ((rx - rx.floor()) as f32, (ry - ry.floor()) as f32);
            let mut ex = [0.0; 4];
            for (dx, wx) in [(0, 1.0 - fx), (1, fx)] {
                for (dy, wy) in [(0, 1.0 - fy), (1, fy)] {
                    let p = reference.get(ix + dx, iy + dy);
                    for c in 0..4 {
                        ex[c] += p[c] * wx * wy;
                    }
                }
            }
            let ac = img.get_pixel(x, y).0;
            for c in 0..3 {
                let diff = (f32::from(ac[c]) / 255.0 - ex[c]).abs();
                if diff > max {
                    max = diff;
                    worst = Some((x, y, sx, sy, ac, ex));
                }
            }
            count += 1;
        }
    }
    assert!(count > 10000, "{count}");
    if max > 1.0 / 255.0 + 1e-6 {
        img.save(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/scaled-app-failed.png")).unwrap();
    }
    assert!(max <= 1.0 / 255.0 + 1e-6, "actual app scaled pixels scale={scale} flip={} max={max} worst={worst:?}", cb.params.flip);
}
// Read the actual app view textures, keeping node handles/outlines out of the pixel oracle.
// This observes the production callback resources; it never installs a preview document.
fn assert_live_view_texels(h: &H, expected: &Document, scale: f64) {
    let reference = scaled_view::cpu::flatten_scaled_with(expected, expected.bounds(), scale, scaled_view::project).unwrap().buffer();
    let gpu = h.state().gpu.as_ref().unwrap();
    let renderer = gpu.rs.renderer.read();
    let res = renderer.callback_resources.get::<Resources>().unwrap();
    let frame = res.scaled_frames.values().find(|f| f.source == expected.id.0).expect("actual scaled app frame");
    assert_eq!(
        frame.previous,
        photocraft_engine::vector_integration::project_document(expected).unwrap(),
        "production callback must receive detached preview/released projection"
    );
    let (device, queue) = (&gpu.rs.device, &gpu.rs.queue);
    let bpp = texel_bytes(frame.textures.format);
    let mut max = 0.0f32;
    let mut count = 0;
    for tile in &frame.textures.tiles {
        let [tx, ty, w, height] = tile.rect;
        let stride = (u64::from(w) * bpp).next_multiple_of(u64::from(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT));
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("scaled app gate readback"),
            size: stride * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            tile.texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(stride as u32), rows_per_image: Some(height) },
            },
            wgpu::Extent3d { width: w, height, depth_or_array_layers: 1 },
        );
        queue.submit([encoder.finish()]);
        buffer.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        assert!(gpu.health.wait(device, None));
        let data = buffer.slice(..).get_mapped_range().unwrap();
        for y in 0..height {
            for x in 0..w {
                let at = y as usize * stride as usize + x as usize * bpp as usize;
                let actual = texel_to_f32(frame.textures.format, &data[at..at + bpp as usize]);
                let expected = reference.get((tx + x) as i32, (ty + y) as i32);
                for c in 0..4 {
                    let expected = expected[c] * if c == 3 { 1.0 } else { expected[3] };
                    max = max.max((actual[c] - expected).abs());
                }
                count += 1;
            }
        }
    }
    assert!(count > 10000);
    assert!(max <= 1.0 / 255.0 + 1e-6, "live app texture premultiplied max={max}");
}
#[test]
#[ignore = "required physical GPU acceptance: run explicitly on RTX 5090 Vulkan with --ignored --exact --nocapture"]
fn scaled_full_app_rtx5090_parity_2_4_8x_hidpi_flip_and_warm_cache() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let _lock = GPU.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    for (ppp, zoom, flip) in [(1.0, 2.0, false), (1.0, 4.0, false), (1.0, 8.0, false), (2.0, 2.0, true), (1.0, 1.7, false), (2.0, 1.7, true)] {
        let mut h = harness(ppp);
        let doc = fixture();
        let before = doc.clone();
        add(&mut h, doc.clone(), zoom);
        h.state_mut().ui.view.flip_horizontal = flip;
        h.run_steps(2);
        assert_app_pixels(&mut h, &doc, f64::from(ppp * zoom));
        let gpu = h.state().gpu.as_ref().unwrap();
        let renderer = gpu.rs.renderer.read();
        let res = renderer.callback_resources.get::<Resources>().unwrap();
        assert!(!res.scaled_frames.is_empty());
        let rebuilds = res.scaled_frames.values().map(|f| f.rebuilds).sum::<u64>();
        drop(renderer);
        h.step();
        h.render().unwrap();
        let gpu = h.state().gpu.as_ref().unwrap();
        let renderer = gpu.rs.renderer.read();
        let res = renderer.callback_resources.get::<Resources>().unwrap();
        assert_eq!(res.scaled_frames.values().map(|f| f.rebuilds).sum::<u64>(), rebuilds);
        assert_eq!(*h.state().session.active().unwrap().doc, before);
        drop(renderer);
        h.state_mut()
            .session
            .edit("scaled damage fixture", |doc, _| {
                doc.layers[1].opacity = 0.5;
                Ok(())
            })
            .unwrap();
        h.run_steps(2);
        let edited = (*h.state().session.active().unwrap().doc).clone();
        assert_app_pixels(&mut h, &edited, f64::from(ppp * zoom));
        let gpu = h.state().gpu.as_ref().unwrap();
        let renderer = gpu.rs.renderer.read();
        let res = renderer.callback_resources.get::<Resources>().unwrap();
        assert_eq!(res.scaled_frames.values().map(|f| f.rebuilds).sum::<u64>(), rebuilds + 1, "only the edited shape projection rebuilds");
    }
}
#[test]
#[ignore = "required physical GPU acceptance: run explicitly on RTX 5090 Vulkan with --ignored --exact --nocapture"]
fn scaled_full_app_actual_pan_zoom_pointer_events() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let _lock = GPU.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut h = harness(1.0);
    let doc = fixture();
    add(&mut h, doc.clone(), 4.0);
    let p = h.state().last_canvas_rect.center();
    let center = h.state().ui.views[0].center;
    h.event(egui::Event::PointerMoved(p));
    h.event(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Middle, pressed: true, modifiers: egui::Modifiers::NONE });
    h.step();
    h.event(egui::Event::PointerMoved(p + egui::vec2(48.0, 24.0)));
    h.step();
    h.event(egui::Event::PointerButton {
        pos: p + egui::vec2(48.0, 24.0),
        button: egui::PointerButton::Middle,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    h.step();
    assert_ne!(h.state().ui.views[0].center, center, "actual canvas pan");
    let zoom = h.state().ui.views[0].zoom;
    h.event(egui::Event::Zoom(2.0));
    h.step();
    assert!(h.state().ui.views[0].zoom > zoom, "actual pinch zoom");
    h.render().unwrap();
    assert_eq!(*h.state().session.active().unwrap().doc, doc);
}
#[test]
#[ignore = "required physical GPU acceptance: run explicitly on RTX 5090 Vulkan with --ignored --exact --nocapture"]
fn scaled_full_app_node_drag_preview_release_and_cancel_rtx5090() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let _lock = GPU.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut h = harness(1.0);
    let d = fixture();
    let id = d.layers[1].id;
    add(&mut h, d.clone(), 4.0);
    h.state_mut().session.select_layer(id).unwrap();
    h.state_mut().ui.tool = crate::state::Tool::DirectSelection;
    let screen = |h: &H, p: [f32; 2]| {
        let app = h.state();
        let v = &app.ui.views[0];
        crate::canvas::ViewXform {
            rect: crate::rulers::content_rect(app, app.last_canvas_rect),
            zoom: v.zoom,
            center: v.center,
            flip: false,
            rotation: v.rotation,
        }
        .to_screen(p[0], p[1])
    };
    let p = screen(&h, [28.25, 30.5]);
    let q = p + egui::vec2(16.0, 8.0);
    let rev = h.state().session.active().unwrap().revision;
    let journal = h.state().session.journal.len();
    h.event(egui::Event::PointerMoved(p));
    h.event(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
    h.step();
    h.event(egui::Event::PointerMoved(q));
    h.step();
    assert_eq!(h.state().session.active().unwrap().revision, rev);
    assert_eq!(h.state().session.journal.len(), journal);
    let (preview, _) = crate::direct_select::display_doc(h.state(), 0).expect("production detached node preview");
    h.render().unwrap();
    assert_live_view_texels(&h, &preview, 4.0);
    h.event(egui::Event::PointerButton { pos: q, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE });
    h.step();
    assert_eq!(h.state().session.journal.len(), journal + 1);
    h.render().unwrap();
    assert_live_view_texels(&h, &h.state().session.active().unwrap().doc, 4.0);
    h.state_mut().session.undo();
    assert_eq!(*h.state().session.active().unwrap().doc, d);
    h.event(egui::Event::PointerMoved(p));
    h.event(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
    h.step();
    h.event(egui::Event::PointerMoved(q));
    h.step();
    let before = h.state().session.journal.len();
    h.key_press(egui::Key::Escape);
    h.step();
    h.event(egui::Event::PointerButton { pos: q, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE });
    h.step();
    assert_eq!(h.state().session.journal.len(), before);
    assert_eq!(*h.state().session.active().unwrap().doc, d);
    h.render().unwrap();
    assert_live_view_texels(&h, &d, 4.0);
}

#[test]
#[ignore = "release acceptance benchmark; run explicitly with --release --ignored"]
#[allow(clippy::assertions_on_constants)]
fn scaled_release_6000x4000_500_shapes_node_pointer_frames_rtx5090() {
    let _gpu_gate = crate::gpu_test_support::lock();
    use std::time::Instant;
    assert!(!cfg!(debug_assertions), "run the acceptance benchmark with --release --ignored");
    let _lock = GPU.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut h = harness(1.0);
    let mut doc = Document::new("scaled node benchmark", Size::new(6000, 4000), ColorMode::Rgb, SampleType::U8);
    let mut selected = None;
    for i in 0..500 {
        let (x, y) = (200.0 + f64::from(i % 25) * 32.0, 200.0 + f64::from(i / 25) * 32.0);
        let mut shape = ShapeLayer {
            path: Path::new(vec![Subpath::polygon(&[(x, y), (x + 20.0, y), (x + 20.0, y + 20.0), (x, y + 20.0)])]),
            fill: Some(Fill::Solid(Color::BLACK)),
            ..Default::default()
        };
        photocraft_engine::vector_cmds::refresh_shape(&doc, &mut shape);
        let layer = Layer::new("curve", LayerContent::Shape(shape));
        selected.get_or_insert(layer.id);
        doc.layers.push(layer);
    }
    add(&mut h, doc, 2.0);
    h.state_mut().ui.views[0].center = [320.0, 320.0];
    h.state_mut().session.select_layer(selected.unwrap()).unwrap();
    h.state_mut().ui.tool = crate::state::Tool::DirectSelection;
    h.run_steps(3);
    let screen = |h: &H, p: [f32; 2]| {
        let app = h.state();
        let v = &app.ui.views[0];
        crate::canvas::ViewXform {
            rect: crate::rulers::content_rect(app, app.last_canvas_rect),
            zoom: v.zoom,
            center: v.center,
            flip: false,
            rotation: v.rotation,
        }
        .to_screen(p[0], p[1])
    };
    let p = screen(&h, [220.0, 220.0]);
    assert!(h.state().last_canvas_rect.contains(p), "benchmark node must be visible");
    let revision = h.state().session.active().unwrap().revision;
    let journal = h.state().session.journal.len();
    let first = Instant::now();
    h.event(egui::Event::PointerMoved(p));
    h.event(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
    h.step();
    h.render().unwrap();
    let first_ms = first.elapsed().as_secs_f64() * 1000.0;
    let counters = |h: &H| {
        let gpu = h.state().gpu.as_ref().unwrap();
        let renderer = gpu.rs.renderer.read();
        let res = renderer.callback_resources.get::<Resources>().unwrap();
        (res.scaled_frames.values().map(|f| f.uploads).sum::<u64>(), res.scaled_frames.values().map(|f| f.rebuilds).sum::<u64>())
    };
    let initial = counters(&h);
    let mut samples = Vec::new();
    for i in 1..=64 {
        let p = screen(&h, [220.0 + (i % 17) as f32, 220.0 + (i % 11) as f32]);
        let start = Instant::now();
        h.event(egui::Event::PointerMoved(p));
        h.step();
        assert!(crate::direct_select::display_doc(h.state(), 0).is_some(), "production detached preview routing required");
        assert_eq!(h.state().session.active().unwrap().revision, revision);
        h.render().unwrap();
        samples.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    let end = counters(&h);
    let p = screen(&h, [224.0, 228.0]);
    let at = Instant::now();
    h.event(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE });
    h.step();
    h.render().unwrap();
    assert_eq!(h.state().session.journal.len(), journal + 1);
    let release_ms = at.elapsed().as_secs_f64() * 1000.0;
    samples.sort_by(f64::total_cmp);
    eprintln!(
        "scaled 6000x4000/500-shape real-app pointer+GPU/readback: adapter={:?} first={first_ms:.3}ms median={:.3}ms p95={:.3}ms p99={:.3}ms release={release_ms:.3}ms uploads={} rebuilds={}",
        h.state().perf.gpu_info,
        samples[32],
        samples[60],
        samples[63],
        end.0 - initial.0,
        end.1 - initial.1
    );
    assert!(samples[32] < 16.0, "warm median must be under 16ms");
}

#[test]
#[ignore = "required physical GPU acceptance: run explicitly on RTX 5090 Vulkan with --ignored --exact --nocapture"]
fn scaled_full_app_display_p3_and_float_hdr_texture_rtx5090() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let _lock = GPU.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut h = harness(2.0);
    let mut doc = fixture();
    if let LayerContent::Shape(s) = &mut doc.layers[1].content {
        s.fill = Some(Fill::Solid(Color::rgb(0.8, 0.5, 0.3)));
    }
    add(&mut h, doc, 2.0);
    h.state_mut().run("edit.assignProfile", serde_json::json!({"profile":"display-p3"})).unwrap();
    h.run_steps(3);
    let image = h.render().unwrap();
    let app = h.state();
    let v = &app.ui.views[0];
    let p = crate::canvas::ViewXform {
        rect: crate::rulers::content_rect(app, app.last_canvas_rect),
        zoom: v.zoom,
        center: v.center,
        flip: false,
        rotation: v.rotation,
    }
    .to_screen(80.0, 60.0)
        * h.ctx.pixels_per_point();
    let pixel = image.get_pixel(p.x as u32, p.y as u32).0;
    // Independently established P3(0.8,0.5,0.3) -> sRGB reference from CMS tests.
    for (a, b) in pixel[..3].iter().zip([216.7, 122.9, 64.3]) {
        assert!((f32::from(*a) - b).abs() <= 1.0, "P3 scaled canvas {pixel:?}");
    }
    let gpu = app.gpu.as_ref().unwrap();
    let renderer = gpu.rs.renderer.read();
    let res = renderer.callback_resources.get::<Resources>().unwrap();
    assert!(res.scaled_frames.values().any(|f| f.scale == 4.0));
    drop(renderer);
    h.state_mut()
        .session
        .edit("32 bit fixture", |doc, _| {
            doc.depth = SampleType::F32;
            Ok(())
        })
        .unwrap();
    h.run_steps(3);
    h.render().unwrap();
    let gpu = h.state().gpu.as_ref().unwrap();
    let renderer = gpu.rs.renderer.read();
    let res = renderer.callback_resources.get::<Resources>().unwrap();
    assert!(res.scaled_frames.values().any(|f| f.textures.format == FORMAT_HIGH));
    drop(renderer);
    h.state_mut().run("edit.assignProfile", serde_json::json!({"profile":"srgb"})).unwrap();
    h.state_mut()
        .session
        .edit("HDR shape fixture", |doc, _| {
            if let LayerContent::Shape(s) = &mut doc.layers[1].content {
                s.fill = Some(Fill::Solid(Color::rgb(2.0, 2.0, 2.0)));
            }
            Ok(())
        })
        .unwrap();
    h.state_mut().run("view.thirtyTwoBitPreviewOptions", serde_json::json!({"exposure":-3.0,"gamma":1.0})).unwrap();
    h.run_steps(3);
    let image = h.render().unwrap();
    let pixel = image.get_pixel(p.x as u32, p.y as u32).0;
    let linear = ((2.0f64 + 0.055) / 1.055).powf(2.4) / 8.0;
    let expected = (1.055 * linear.powf(1.0 / 2.4) - 0.055) * 255.0;
    for a in &pixel[..3] {
        assert!((f64::from(*a) - expected).abs() <= 1.0, "HDR scaled shape {pixel:?}, expected {expected}");
    }
}

#[test]
#[ignore = "required physical GPU acceptance: run explicitly on RTX 5090 Vulkan with --ignored --exact --nocapture"]
fn scaled_compositor_reuses_pipelines_across_zoom_buckets() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let _lock = GPU.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut h = harness(1.0);
    let doc = fixture();
    add(&mut h, doc.clone(), 2.0);
    assert_app_pixels(&mut h, &doc, 2.0);
    let creations = |h: &H| {
        let renderer = h.state().gpu.as_ref().unwrap().rs.renderer.read();
        renderer.callback_resources.get::<Resources>().unwrap().scaled_compositor_creations
    };
    assert_eq!(creations(&h), 1);
    for zoom in [4.0, 8.0, 2.0, 4.0] {
        h.state_mut().ui.views[0].zoom = zoom;
        h.run_steps(2);
        assert_app_pixels(&mut h, &doc, f64::from(zoom));
        assert_eq!(creations(&h), 1, "zoom bucket {zoom} rebuilt GPU pipelines");
    }
    assert_eq!(*h.state().session.active().unwrap().doc, doc);
}

fn scaled_test_callback(h: &H, zoom: f32) -> CanvasCallback {
    let app = h.state();
    let d = &app.session.active().unwrap().doc;
    CanvasCallback {
        rect: crate::rulers::content_rect(app, app.last_canvas_rect),
        params: ViewParams {
            doc: d.id.0,
            doc_size: [d.size.width, d.size.height],
            zoom,
            center: app.ui.views[0].center,
            shadow: false,
            pixel_grid: false,
            view_key: 0,
            display: 0,
            output: 0,
            hdr: None,
            flip: false,
            rotation: 0.0,
        },
    }
}
fn prepare_scaled_test(h: &H, callback: &CanvasCallback) -> (f64, u64, u64) {
    let rs = &h.state().gpu.as_ref().unwrap().rs;
    let mut renderer = rs.renderer.write();
    let res = renderer.callback_resources.get_mut::<Resources>().unwrap();
    let screen = ScreenDescriptor { size_in_pixels: [1000, 700], pixels_per_point: 1.0 };
    let scale = callback.prepare_scaled(&rs.device, &rs.queue, &screen, res);
    (scale, res.scaled_compositor_creations, res.scaled_projection_attempts)
}

#[test]
#[ignore = "required physical GPU acceptance: run explicitly on RTX 5090 Vulkan with --ignored --exact --nocapture"]
fn scaled_compositor_reuses_pipelines_across_viewport_aprons() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let _lock = GPU.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut h = harness(1.0);
    let mut doc = fixture();
    doc.size = Size::new(640, 480);
    add(&mut h, doc.clone(), 2.0);
    for center in [[80.0, 60.0], [300.0, 220.0], [500.0, 370.0], [80.0, 60.0]] {
        h.state_mut().ui.views[0].center = center;
        h.run_steps(2);
        h.render().unwrap();
        assert_live_view_texels(&h, &doc, 2.0);
        let renderer = h.state().gpu.as_ref().unwrap().rs.renderer.read();
        let res = renderer.callback_resources.get::<Resources>().unwrap();
        assert_eq!(res.scaled_compositor_creations, 1, "panning recreated pipelines");
    }
}

#[test]
#[ignore = "required physical GPU acceptance: run explicitly on RTX 5090 Vulkan with --ignored --exact --nocapture"]
fn scaled_projection_refusal_is_memoized_only_for_exact_source_and_view() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let _lock = GPU.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut h = harness(1.0);
    let doc = fixture();
    add(&mut h, doc.clone(), 2.0);
    let mut invalid = doc.clone();
    let LayerContent::Shape(shape) = &mut invalid.layers[1].content else { panic!() };
    shape.fill = Some(Fill::Pattern { name: "missing".into(), id: "missing".into(), scale: 1.0, angle: 0.0, link: true, phase: (0.0, 0.0) });
    shape.cache = None;
    h.state().gpu.as_ref().unwrap().refresh(doc.id.0, &invalid, None, None);
    let callback = scaled_test_callback(&h, 2.0);
    let first = prepare_scaled_test(&h, &callback);
    assert_eq!(first.0, 1.0, "failed scaled projection must display the 1x fallback");
    assert!(first.2 > 0);
    for _ in 0..4 {
        assert_eq!(prepare_scaled_test(&h, &callback), first);
    }
    let second = prepare_scaled_test(&h, &scaled_test_callback(&h, 4.0));
    assert_eq!(second.0, 1.0);
    assert_eq!(second.2, first.2 + 1, "a different view must retry");
    let mut deps = HashMap::new();
    deps.insert(doc.layers[1].id, 7);
    h.state().gpu.as_ref().unwrap().set_scaled_dependencies(doc.id.0, deps);
    let third = prepare_scaled_test(&h, &scaled_test_callback(&h, 4.0));
    assert_eq!(third.2, second.2 + 1, "changed dependencies must retry");
    let repaired_doc = photocraft_engine::vector_integration::project_document(&doc).unwrap();
    h.state().gpu.as_ref().unwrap().refresh(doc.id.0, &repaired_doc, None, None);
    let repaired = prepare_scaled_test(&h, &callback);
    assert_eq!(repaired.0, 2.0, "repaired source must regain scaled rendering");
    assert_eq!(repaired.2, third.2 + 1);
    assert_live_view_texels(&h, &doc, 2.0);
}

#[test]
#[ignore = "required physical GPU acceptance: run explicitly on RTX 5090 Vulkan with --ignored --exact --nocapture"]
fn scaled_pipeline_refusal_keeps_correct_cpu_rendering_without_repeated_construction() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let _lock = GPU.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut h = harness(1.0);
    let doc = fixture();
    add(&mut h, doc.clone(), 2.0);
    {
        let mut renderer = h.state().gpu.as_ref().unwrap().rs.renderer.write();
        let res = renderer.callback_resources.get_mut::<Resources>().unwrap();
        res.scaled_frames.clear();
        res.scaled_compositor_failed = Some(photocraft_gpu::Unsupported("simulated unsupported device pipeline".into()));
    }
    let first = prepare_scaled_test(&h, &scaled_test_callback(&h, 2.0));
    assert_eq!(first.0, 2.0);
    assert_live_view_texels(&h, &doc, 2.0);
    for zoom in [4.0, 2.0, 4.0] {
        let result = prepare_scaled_test(&h, &scaled_test_callback(&h, zoom));
        assert_eq!(result.0, f64::from(zoom));
        assert_eq!(result.1, first.1, "cached device refusal must not rebuild pipelines");
        assert_live_view_texels(&h, &doc, f64::from(zoom));
    }
}

#[derive(Clone, Copy, Debug)]
enum RotationPath {
    Raster,
    Proxy,
    Move,
}

struct RotationFrame {
    gpu: GpuCanvas,
    params: ViewParams,
    path: RotationPath,
    rect: egui::Rect,
}

fn rotation_color(x: f64, y: f64) -> [u8; 4] {
    [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255], [255, 255, 0, 255]][usize::from(x >= 48.0) + 2 * usize::from(y >= 40.0)]
}

#[test]
#[ignore = "required physical GPU acceptance: run explicitly on RTX 5090 Vulkan with --ignored --exact --nocapture"]
fn rotated_presentation_rtx5090_raster_proxy_and_live_move_keep_pixels_and_coverage() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let _lock = GPU.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    for (degrees, flip, ppp, path) in [
        (0.0_f32, true, 1.25, RotationPath::Raster),
        (37.0, false, 1.25, RotationPath::Raster),
        (-90.0, true, 2.0, RotationPath::Raster),
        (37.0, true, 1.25, RotationPath::Proxy),
        (37.0, true, 1.25, RotationPath::Move),
    ] {
        // Independent egui contexts need fresh renderer texture namespaces (including fonts).
        let mut setup = wgpu_setup();
        use_adapter_limits_with(&mut setup, |adapter| {
            let info = adapter.get_info();
            #[cfg(not(target_os = "macos"))]
            assert!(info.name.contains("RTX 5090"), "{info:?}");
            assert_eq!(info.backend, if cfg!(target_os = "macos") { wgpu::Backend::Metal } else { wgpu::Backend::Vulkan });
            #[cfg(not(target_os = "macos"))]
            assert_eq!(info.device_type, wgpu::DeviceType::DiscreteGpu);
            #[cfg(target_os = "macos")]
            assert_ne!(info.device_type, wgpu::DeviceType::Cpu, "hardware Metal adapter required");
            eprintln!("rotated presentation gate {info:?}");
        });
        let rs = egui_kittest::wgpu::create_render_state(setup, eframe::egui_wgpu::RendererOptions::PREDICTABLE);
        let gpu = GpuCanvas::with_tile(&rs, Some(64));
        let reduced = matches!(path, RotationPath::Proxy);
        let factor = if reduced { 2 } else { 1 };
        let pixels: Vec<u8> =
            (0..96 / factor).flat_map(|y| (0..128 / factor).flat_map(move |x| rotation_color(f64::from(x * factor), f64::from(y * factor)))).collect();
        gpu.upload_full(9001, [128 / factor, 96 / factor], &pixels);
        for (key, start) in [(9002, 0), (9003, 64)] {
            let pixels: Vec<u8> = (0..96).flat_map(|y| (0..64).flat_map(move |x| rotation_color(f64::from(x + start), f64::from(y)))).collect();
            gpu.upload_full(key, [64, 96], &pixels);
        }
        let state = RotationFrame {
            gpu,
            params: ViewParams {
                doc: 9001,
                doc_size: [128 / factor, 96 / factor],
                zoom: 1.5 * factor as f32,
                center: [64.0 / factor as f32, 48.0 / factor as f32],
                shadow: false,
                pixel_grid: false,
                view_key: 9100,
                display: 0,
                output: 0,
                hdr: None,
                flip,
                rotation: degrees.to_radians(),
            },
            path,
            rect: egui::Rect::NOTHING,
        };
        let background = egui::Color32::from_rgb(19, 27, 41);
        let mut h = Harness::builder()
            .with_size(egui::vec2(320.0, 260.0))
            .with_pixels_per_point(ppp)
            .renderer(egui_kittest::wgpu::WgpuTestRenderer::from_render_state(rs.clone()))
            .build_ui_state(
                move |ui, state: &mut RotationFrame| {
                    state.rect = ui.max_rect();
                    let painter = ui.painter_at(state.rect);
                    painter.rect_filled(state.rect, 0, background);
                    if matches!(state.path, RotationPath::Move) {
                        GpuCanvas::paint_move(
                            &painter,
                            state.rect,
                            state.params,
                            vec![
                                MovePlane { key: 9002, size: [64, 96], origin: [0, 0], moving: false, offset: [0, 0] },
                                MovePlane { key: 9003, size: [64, 96], origin: [64, 0], moving: true, offset: [9, -5] },
                            ],
                        );
                    } else {
                        GpuCanvas::paint(&painter, state.rect, state.params);
                    }
                },
                state,
            );
        h.run_steps(2);
        let output = h.render().expect("required hardware presentation readback");
        if degrees == 37.0 {
            let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/rotate-view-foundation");
            std::fs::create_dir_all(&dir).unwrap();
            output.save(dir.join(format!("{path:?}-37deg-{ppp}x-flip{flip}.png"))).unwrap();
        }
        assert!(h.state().gpu.fault().is_none());
        let rect = h.state().rect;
        let physical = 1.5 * ppp;
        let (sin, cos) = f64::from(degrees).to_radians().sin_cos();
        // Independent screen inverse. Only the upright case uses legacy snapped device origin.
        let mut anchor = [f64::from(rect.center().x * ppp), f64::from(rect.center().y * ppp)];
        if degrees == 0.0 {
            let dx = f64::from(64.0 * physical * if flip { -1.0 } else { 1.0 });
            let dy = f64::from(48.0 * physical);
            anchor = [(anchor[0] - dx).round() + dx, (anchor[1] - dy).round() + dy];
        }
        let mut covered = 0;
        let mut clear = 0;
        for y in 0..output.height() {
            for x in 0..output.width() {
                let point = egui::pos2((x as f32 + 0.5) / ppp, (y as f32 + 0.5) / ppp);
                if !rect.shrink(1.0).contains(point) {
                    continue;
                }
                let sx = (f64::from(x) + 0.5 - anchor[0]) / f64::from(physical);
                let sy = (f64::from(y) + 0.5 - anchor[1]) / f64::from(physical);
                let dx = (cos * sx + sin * sy) * if flip { -1.0 } else { 1.0 } + 64.0;
                let dy = -sin * sx + cos * sy + 48.0;
                let actual = output.get_pixel(x, y).0;
                if dx < -2.0 || dy < -2.0 || dx > 130.0 || dy > 98.0 {
                    assert_eq!(actual, background.to_array(), "{path:?} {degrees}/{flip}: pasteboard at {x},{y} was filled by the rotated AABB");
                    clear += 1;
                    continue;
                }
                if !(2.0..126.0).contains(&dx) || !(2.0..94.0).contains(&dy) {
                    continue;
                }
                let (sample_x, sample_y) = if matches!(path, RotationPath::Move) {
                    if dx < 62.0 {
                        (dx, dy)
                    } else if dx > 75.0 && dy < 89.0 {
                        (dx - 9.0, dy + 5.0)
                    } else {
                        continue;
                    }
                } else {
                    (dx, dy)
                };
                if (sample_x - 48.0).abs() < 2.0 || (sample_y - 40.0).abs() < 2.0 {
                    continue;
                }
                let expected = rotation_color(sample_x, sample_y);
                for c in 0..4 {
                    assert!(actual[c].abs_diff(expected[c]) <= 1, "{path:?} {degrees}/{flip} {x},{y}: {actual:?} != {expected:?}, doc={dx},{dy}");
                }
                covered += 1;
            }
        }
        assert!(covered > 10000 && clear > 10000, "nonvacuous coverage {path:?}: {covered}/{clear}");
        if matches!(path, RotationPath::Move) {
            // An actual translated plane leaves a gap, which must show checker, not stale pixels.
            let d = [68.0_f64 - 64.0, 20.0_f64 - 48.0];
            let dx = d[0] * if flip { -1.0 } else { 1.0 };
            let px = anchor[0] + (cos * dx - sin * d[1]) * f64::from(physical);
            let py = anchor[1] + (sin * dx + cos * d[1]) * f64::from(physical);
            let gap = output.get_pixel(px as u32, py as u32).0;
            assert_eq!(gap[0], gap[1]);
            assert_eq!(gap[1], gap[2]);
            assert_ne!(gap, background.to_array());
        }
    }
}

#[test]
#[ignore = "required physical GPU acceptance: run explicitly on RTX 5090 Vulkan with --ignored --exact --nocapture"]
fn rotated_scaled_projection_covers_inverse_viewport_corners() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let _lock = GPU.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut h = harness(1.0);
    let mut doc = fixture();
    doc.size = Size::new(1200, 900);
    add(&mut h, doc, 4.0);
    let mut callback = scaled_test_callback(&h, 4.0);
    callback.params.center = [600.0, 400.0];
    callback.params.rotation = std::f32::consts::FRAC_PI_2;
    callback.params.view_key = 9931;
    callback.rect = egui::Rect::from_min_size(egui::pos2(50.0, 50.0), egui::vec2(800.0, 80.0));
    let (scale, _, _) = prepare_scaled_test(&h, &callback);
    assert_eq!(scale, 4.0);
    let renderer = h.state().gpu.as_ref().unwrap().rs.renderer.read();
    let frame = renderer.callback_resources.get::<Resources>().unwrap().scaled_frames.get(&9931).unwrap();
    // A 90° rotation swaps this wide/short viewport: its document y span is300..500.
    // The old axis-aligned ROI plus its sampling apron began at320, clipping the top corner.
    for [x, y] in [[590.0, 300.0], [610.0, 300.0], [590.0, 500.0], [610.0, 500.0]] {
        let (x, y) = ((x * scale) as i32, (y * scale) as i32);
        assert!(
            frame.rect.x0 < x && frame.rect.y0 < y && frame.rect.x1 > x && frame.rect.y1 > y,
            "scaled ROI {:?} omits rotated visible corner {x},{y} plus filter gutter",
            frame.rect
        );
    }
}

#[test]
#[ignore = "required physical GPU acceptance: run explicitly on RTX 5090 Vulkan with --ignored --exact --nocapture"]
fn rotated_shadow_rtx5090_preserves_interior_and_screen_down_light() {
    let _gpu_gate = crate::gpu_test_support::lock();
    let _lock = GPU.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut setup = wgpu_setup();
    use_adapter_limits_with(&mut setup, |adapter| {
        let info = adapter.get_info();
        #[cfg(not(target_os = "macos"))]
        assert!(info.name.contains("RTX 5090"), "{info:?}");
        assert_eq!(info.backend, if cfg!(target_os = "macos") { wgpu::Backend::Metal } else { wgpu::Backend::Vulkan });
        #[cfg(not(target_os = "macos"))]
        assert_eq!(info.device_type, wgpu::DeviceType::DiscreteGpu);
        #[cfg(target_os = "macos")]
        assert_ne!(info.device_type, wgpu::DeviceType::Cpu, "hardware Metal adapter required");
    });
    let rs = egui_kittest::wgpu::create_render_state(setup, eframe::egui_wgpu::RendererOptions::PREDICTABLE);
    let linear_target = rs.target_format.is_srgb();
    let gpu = GpuCanvas::with_tile(&rs, Some(64));
    let pixels: Vec<u8> = (0..96).flat_map(|y| (0..128).flat_map(move |x| rotation_color(f64::from(x), f64::from(y)))).collect();
    gpu.upload_full(9201, [128, 96], &pixels);
    let state = RotationFrame {
        gpu,
        params: ViewParams {
            doc: 9201,
            doc_size: [128, 96],
            zoom: 1.5,
            center: [64.0, 48.0],
            shadow: false,
            pixel_grid: false,
            view_key: 9202,
            display: 0,
            output: 0,
            hdr: None,
            flip: false,
            rotation: 37.0_f32.to_radians(),
        },
        path: RotationPath::Raster,
        rect: egui::Rect::NOTHING,
    };
    let mut h = Harness::builder()
        .with_size(egui::vec2(480.0, 400.0))
        .with_pixels_per_point(1.25)
        .renderer(egui_kittest::wgpu::WgpuTestRenderer::from_render_state(rs))
        .build_ui_state(
            |ui, state: &mut RotationFrame| {
                state.rect = ui.max_rect();
                let painter = ui.painter_at(state.rect);
                painter.rect_filled(state.rect, 0, egui::Color32::WHITE);
                GpuCanvas::paint(&painter, state.rect, state.params);
            },
            state,
        );
    h.run_steps(2);
    let off = h.render().expect("required shadow-off hardware readback");
    h.state_mut().params.shadow = true;
    h.step();
    let on = h.render().expect("required shadow-on hardware readback");
    assert!(h.state().gpu.fault().is_none());
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/rotate-view-foundation");
    std::fs::create_dir_all(&dir).unwrap();
    on.save(dir.join("shadow-37deg-1.25x.png")).unwrap();

    // Independent scalar coordinates and numerically integrated Gaussian CDF. This does
    // not reuse Camera, shader uniforms, or the shader's fast erf approximation.
    let (sin, cos) = 37.0_f64.to_radians().sin_cos();
    let rect = h.state().rect;
    let anchor = [f64::from(rect.center().x) * 1.25, f64::from(rect.center().y) * 1.25];
    let inverse = |p: [f64; 2]| {
        let (x, y) = ((p[0] - anchor[0]) / 1.875, (p[1] - anchor[1]) / 1.875);
        [cos * x + sin * y + 64.0, -sin * x + cos * y + 48.0]
    };
    let sample = |d: [f64; 2]| {
        let (x, y) = ((d[0] - 64.0) * 1.875, (d[1] - 48.0) * 1.875);
        [(anchor[0] + cos * x - sin * y).floor() as u32, (anchor[1] + sin * x + cos * y).floor() as u32]
    };
    let mut interior = 0;
    for y in 0..on.height() {
        for x in 0..on.width() {
            let d = inverse([f64::from(x) + 0.5, f64::from(y) + 0.5]);
            if (2.0..126.0).contains(&d[0]) && (2.0..94.0).contains(&d[1]) {
                assert_eq!(on.get_pixel(x, y), off.get_pixel(x, y), "shadow changed opaque document pixel {x},{y}");
                interior += 1;
            }
        }
    }
    assert!(interior > 10000);
    for p in [
        rect.left_top() + egui::vec2(8.0, 8.0),
        rect.right_top() + egui::vec2(-8.0, 8.0),
        rect.left_bottom() + egui::vec2(8.0, -8.0),
        rect.right_bottom() - egui::vec2(8.0, 8.0),
    ] {
        let (x, y) = ((p.x * 1.25) as u32, (p.y * 1.25) as u32);
        assert_eq!(off.get_pixel(x, y).0, [255; 4]);
        assert_eq!(on.get_pixel(x, y), off.get_pixel(x, y), "far pasteboard changed");
    }
    fn cdf(x: f64) -> f64 {
        let end = x.abs().min(10.0);
        let step = end / 256.0;
        let integral: f64 = (0..=256)
            .map(|i| {
                let t = f64::from(i) * step;
                let weight = if i == 0 || i == 256 {
                    1.0
                } else if i % 2 == 0 {
                    2.0
                } else {
                    4.0
                };
                weight * (-0.5 * t * t).exp()
            })
            .sum();
        0.5 + x.signum() * integral * step / (3.0 * std::f64::consts::TAU.sqrt())
    }
    let alpha = |p: [f64; 2], rotated_light: bool| {
        let coverage = |offset: f64, sigma: f64| {
            let delta = if rotated_light { [-sin * offset * 1.25, cos * offset * 1.25] } else { [0.0, offset * 1.25] };
            let d = inverse([p[0] - delta[0], p[1] - delta[1]]);
            let sigma = sigma / 1.5;
            (cdf((128.0 - d[0]) / sigma) - cdf(-d[0] / sigma)) * (cdf((96.0 - d[1]) / sigma) - cdf(-d[1] / sigma))
        };
        1.0 - (1.0 - 0.40 * coverage(7.0, 16.0)) * (1.0 - 0.30 * coverage(0.75, 1.25))
    };
    let intensity = |alpha: f64| {
        let c = 1.0 - alpha;
        255.0 * if linear_target { 1.055 * c.powf(1.0 / 2.4) - 0.055 } else { c }
    };
    let mut levels = Vec::new();
    let mut light_space_separation = 0.0_f64;
    for d in [[64.0, -4.0], [64.0, 100.0], [-4.0, 48.0], [132.0, 48.0]] {
        let [x, y] = sample(d);
        let p = [f64::from(x) + 0.5, f64::from(y) + 0.5];
        let expected = intensity(alpha(p, false));
        light_space_separation = light_space_separation.max((expected - intensity(alpha(p, true))).abs());
        let actual = on.get_pixel(x, y).0;
        assert_eq!(off.get_pixel(x, y).0, [255; 4], "paired sample must be exterior");
        assert!(actual[0] < 250, "near-edge exterior needs visible shadow: {actual:?}");
        for channel in &actual[..3] {
            assert!((f64::from(*channel) - expected).abs() <= 2.0, "screen-down shadow at {d:?}: {actual:?}, Gaussian oracle={expected}");
        }
        levels.push(actual[0]);
    }
    assert!(levels[1] < levels[0], "screen-lower edge must cast the stronger shadow");
    assert!(levels[3] < levels[2], "screen-down light also projects onto the rotated document x axis");
    assert!(light_space_separation > 4.0, "fixture must distinguish screen-down from a light vector rotated with the page");
}

#[test]
#[ignore = "required physical GPU acceptance: run explicitly on RTX 5090 Vulkan with --ignored --exact --nocapture"]
fn rotate_view_full_app_rtx5090_pointer_angle_pixels_and_reset() {
    use egui_kittest::kittest::Queryable;
    let _gpu_gate = crate::gpu_test_support::lock();
    let _lock = GPU.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut h = harness(1.25);
    let doc = fixture();
    add(&mut h, doc.clone(), 2.0);
    h.state_mut().session.edit_prefs(|p| {
        p.interface.canvas_border = photocraft_engine::prefs::CanvasBorder::None;
        p.tools.overscroll = true;
    });
    let revision = h.state().session.active().unwrap().revision;
    let history = h.state().session.active().unwrap().history.past_len();
    h.key_press(egui::Key::R);
    h.run_steps(3);
    assert_eq!(h.state().ui.tool, crate::state::Tool::RotateView);
    let center = crate::canvas::ViewXform::active(h.state()).unwrap().rect.center();
    let start = center + egui::vec2(100.0, 0.0);
    let (sin, cos) = 37.0_f32.to_radians().sin_cos();
    let end = center + egui::vec2(100.0 * cos, 100.0 * sin);
    h.hover_at(start);
    h.step();
    h.event(egui::Event::PointerButton { pos: start, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
    h.step();
    h.hover_at(end);
    h.step();
    h.event(egui::Event::PointerButton { pos: end, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE });
    h.run_steps(2);
    assert!((h.state().ui.views[0].rotation.to_degrees() - 37.0).abs() < 0.01);
    assert_app_pixels(&mut h, &doc, 2.5);
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/rotate-view-activation");
    std::fs::create_dir_all(&dir).unwrap();
    h.render().unwrap().save(dir.join("full-app-gpu-37deg-125.png")).unwrap();
    h.get_by_label("Reset View").click();
    h.run_steps(3);
    assert_eq!(h.state().ui.views[0].rotation, 0.0);
    assert_app_pixels(&mut h, &doc, 2.5);
    let st = h.state().session.active().unwrap();
    assert_eq!(*st.doc, doc);
    assert_eq!(st.revision, revision);
    assert_eq!(st.history.past_len(), history);
}
