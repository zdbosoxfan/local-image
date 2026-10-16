//! Offscreen FULL APP regression for Move's guide bridge. The new node/spacing/manager
//! router still needs package 00 registration; this test explicitly covers the existing
//! live preview/release path against the new shared query's geometry.
use egui::{Event, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;
use photocraft_doc::{LayerContent, LayerId};
use photocraft_engine::vector_packages::snapping::query::{Cache, Exclusions, Settings, View};
use photocraft_ui_egui::{
    PhotocraftApp,
    canvas::ViewXform,
    control::{ControlRequest, handle},
};
use serde_json::json;

type H = Harness<'static, PhotocraftApp>;
fn screen(h: &H, p: [f32; 2]) -> Pos2 {
    let app = h.state();
    let v = &app.ui.views[0];
    ViewXform { rect: photocraft_ui_egui::rulers::content_rect(app, app.last_canvas_rect), zoom: v.zoom, center: v.center, flip: false, rotation: v.rotation }
        .to_screen(p[0], p[1])
}
fn pointer(h: &mut H, p: Pos2, mods: Modifiers) {
    h.event(Event::ModifiersChanged(mods));
    h.event(Event::PointerMoved(p));
    h.step();
}
fn button(h: &mut H, p: Pos2, pressed: bool, mods: Modifiers) {
    h.event(Event::ModifiersChanged(mods));
    h.event(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed, modifiers: mods });
    h.step();
}
fn canvas_pixels(h: &mut H) -> Vec<[u8; 4]> {
    // Move uses cached GPU planes with a shader translation while held. Read the
    // presented canvas, retaining pointer capture; its source texture is untranslated.
    let image = h.render().expect("real app canvas readback");
    if let Ok(dir) = std::env::var("SNAPPING_DIAGNOSTIC_DIR") {
        std::fs::create_dir_all(&dir).unwrap();
        let st = h.state().session.active().unwrap();
        image.save(std::path::Path::new(&dir).join(format!("ppp{}-revision{}.png", h.ctx.pixels_per_point(), st.revision))).unwrap();
    }
    assert!(h.state().gpu_active() && h.state().perf.gpu);
    let ppp = h.ctx.pixels_per_point();
    (20..140)
        .flat_map(|y| (20..300).map(move |x| (x, y)))
        .map(|(x, y)| {
            let p = screen(h, [x as f32 + 0.5, y as f32 + 0.5]) * ppp;
            image.get_pixel(p.x.floor() as u32, p.y.floor() as u32).0
        })
        .collect()
}
fn shape_path(h: &H, id: LayerId) -> photocraft_doc::Path {
    let LayerContent::Shape(sh) = &h.state().session.active().unwrap().doc.layer(id).unwrap().content else { panic!("shape") };
    sh.path.clone()
}
#[test]
fn snapping_full_app_gpu_guide_preview_release_no_jump_and_override() {
    let _gpu_gate = crate::gpu_test_support::lock();
    // No adapter skip: the required owner hardware/format are asserted in the app creator.
    for ppp in [1.0, 2.0] {
        let mut h = Harness::builder()
            .with_size(egui::vec2(1200.0, 800.0))
            .with_pixels_per_point(ppp)
            .wgpu_setup(photocraft_ui_egui::gpu_canvas::wgpu_setup())
            .build_eframe(|cc| {
                let rs = cc.wgpu_render_state.as_ref().expect("real GPU required");
                let info = rs.adapter.get_info();
                use eframe::wgpu::{Backend, DeviceType, TextureFormat};
                assert_eq!(info.backend, if cfg!(target_os = "macos") { Backend::Metal } else { Backend::Vulkan });
                #[cfg(not(target_os = "macos"))]
                assert_eq!(info.device_type, DeviceType::DiscreteGpu);
                #[cfg(target_os = "macos")]
                assert_ne!(info.device_type, DeviceType::Cpu, "hardware Metal adapter required");
                #[cfg(not(target_os = "macos"))]
                assert!(info.name.contains("5090"), "RTX 5090 required: {info:?}");
                assert_eq!(photocraft_gpu::Compositor::preferred_acc_format(&rs.adapter), TextureFormat::Rgba32Float);
                eprintln!("snapping full app GPU: {info:?}; PPP={ppp}");
                PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
                let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
                app.set_wgpu(rs.clone());
                app
            });
        h.state_mut().run("file.new", json!({"width":400,"height":300,"background":"white"})).unwrap();
        h.state_mut().sync_views();
        let ctx = h.ctx.clone();
        let (req, _) = ControlRequest::new("ui.set", json!({"tool":"move","zoom":1.0,"center":[200,150]}));
        handle(h.state_mut(), &ctx, &req);
        let id = LayerId(h.state_mut().run("shape.create", json!({"kind":"rect","rect":[40,40,20,20],"fill":"#0000ff"})).unwrap()["layer"].as_u64().unwrap());
        h.state_mut().run("view.newGuide", json!({"orientation":"vertical","position":120})).unwrap();
        {
            let app = h.state_mut();
            app.ui.tool_options.move_auto_select = false;
            app.ui.tool_options.move_show_transform = false;
            app.ui.extras.snap = true;
            app.ui.extras.guides = true;
            app.ui.extras.rulers = false;
            app.ui.view.snap_to.layers = false;
            app.ui.view.snap_to.document_bounds = false;
            app.ui.view.show.smart_guides = false;
            app.ui.view.show.layer_edges = false;
            app.session.edit_prefs(|p| {
                p.interface.contextual_task_bar = false;
                p.interface.show_bounding_box_when_dragging_layer = false;
            });
        }
        h.run_steps(4);
        assert!(h.state().gpu_active());
        let st = h.state().session.active().unwrap();
        let history = st.history.past_len();
        let revision = st.revision;
        let cache = Cache::build(&st.doc, revision, Exclusions { layers: vec![id], ..Default::default() }, None).unwrap();
        let settings = Settings { nodes: false, bounds: false, alignment: false, artboard: false, document: false, spacing: false, ..Default::default() };
        let snap = cache.rect([121.0, 65.0, 141.0, 85.0], &settings, View { zoom: 1.0, pixels_per_point: f64::from(ppp) }, false).unwrap();
        assert_eq!(snap.delta, [-1.0, 0.0]);
        let p = screen(&h, [50.0, 50.0]);
        let end = screen(&h, [131.0, 75.0]);
        pointer(&mut h, p, Modifiers::NONE);
        button(&mut h, p, true, Modifiers::NONE);
        // The press cached the guide. Hide guide and transient snap-bounds decorations
        // for the paint oracle; apply() still uses the captured targets through release.
        h.state_mut().ui.extras.guides = false;
        h.state_mut().ui.extras.snap = false;
        pointer(&mut h, end, Modifiers::NONE);
        assert_eq!(h.state().session.active().unwrap().revision, revision, "preview remains detached");
        assert_eq!(h.state().session.active().unwrap().history.past_len(), history);
        let preview = canvas_pixels(&mut h);
        button(&mut h, end, false, Modifiers::NONE);
        h.run_steps(3);
        let release = canvas_pixels(&mut h);
        let max = preview.iter().zip(&release).flat_map(|(a, b)| (0..4).map(move |i| (i16::from(a[i]) - i16::from(b[i])).abs())).max().unwrap();
        let worst = preview.iter().zip(&release).enumerate().max_by_key(|(_, (a, b))| (0..4).map(|i| a[i].abs_diff(b[i])).max().unwrap()).unwrap();
        assert!(max <= 1, "PPP={ppp} preview/release max diff={max}/255; worst={worst:?}; bounds={:?}", shape_path(&h, id).control_bounds());
        let expected = [120.0, 65.0, 140.0, 85.0];
        let bounds = shape_path(&h, id).control_bounds().unwrap();
        assert_eq!([bounds.0, bounds.1, bounds.2, bounds.3], expected);
        assert_eq!(h.state().session.active().unwrap().history.past_len(), history + 1);
        // The presented frames above include the unchanged pointer decoration. Check
        // every document texel independently after commit, without that UI overlay.
        let doc = &h.state().session.active().unwrap().doc;
        let (_, size, pixels) = h.state().read_canvas_texels(doc.id.0).expect("committed GPU canvas texture");
        for (x, y) in (20..140).flat_map(|y| (20..300).map(move |x| (x, y))) {
            let pixel = pixels[(y * size[0] + x) as usize];
            let want: [u8; 4] = if (120..140).contains(&x) && (65..85).contains(&y) { [0, 0, 255, 255] } else { [255; 4] };
            for i in 0..4 {
                assert!((pixel[i] - f32::from(want[i]) / 255.).abs() <= 1. / 255., "canvas ({x},{y}): {pixel:?} != {want:?}");
            }
        }
        assert!(h.state_mut().session.undo());
        h.run_steps(3);
        h.state_mut().ui.extras.guides = true;
        h.state_mut().ui.extras.snap = true;
        let p = screen(&h, [50.0, 50.0]);
        pointer(&mut h, p, Modifiers::NONE);
        button(&mut h, p, true, Modifiers::NONE);
        h.state_mut().ui.extras.guides = false;
        h.state_mut().ui.extras.snap = false;
        pointer(&mut h, end, Modifiers::CTRL);
        let preview = canvas_pixels(&mut h);
        button(&mut h, end, false, Modifiers::CTRL);
        h.run_steps(3);
        let release = canvas_pixels(&mut h);
        assert!(preview.iter().zip(&release).all(|(a, b)| (0..4).all(|i| (i16::from(a[i]) - i16::from(b[i])).abs() <= 1)));
        assert_eq!(shape_path(&h, id).control_bounds().unwrap(), (121.0, 65.0, 141.0, 85.0));
    }
}
