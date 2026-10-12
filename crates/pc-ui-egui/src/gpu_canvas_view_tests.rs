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
        assert!(i.name.contains("RTX 5090"), "{i:?}");
        assert_eq!(i.backend, wgpu::Backend::Vulkan);
        assert_eq!(i.device_type, wgpu::DeviceType::DiscreteGpu);
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
            let sx = (x as f32 + 0.5 - origin[0]) / physical * if cb.params.flip { -1.0 } else { 1.0 };
            let sy = (y as f32 + 0.5 - origin[1]) / physical;
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
        crate::canvas::ViewXform { rect: crate::rulers::content_rect(app, app.last_canvas_rect), zoom: v.zoom, center: v.center, flip: false }
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
        crate::canvas::ViewXform { rect: crate::rulers::content_rect(app, app.last_canvas_rect), zoom: v.zoom, center: v.center, flip: false }
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
    let p = crate::canvas::ViewXform { rect: crate::rulers::content_rect(app, app.last_canvas_rect), zoom: v.zoom, center: v.center, flip: false }
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
