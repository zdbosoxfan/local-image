//! Package 24 full-app gate. Register in the existing UI GPU aggregate after 00 wiring.
//! This requires PhotocraftApp::affinity plus the canvas preview/input hooks in 24.md.
use eframe::egui_wgpu::{self, wgpu};
use egui::{Event, Modifiers, PointerButton};
use egui_kittest::{Harness, kittest::Queryable};
use photocraft_ui_egui::{PhotocraftApp, canvas::ViewXform, state::Tool};
use serde_json::json;

fn setup() -> egui_wgpu::WgpuSetup {
    let mut setup = egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    setup.instance_descriptor.backends = wgpu::Backends::VULKAN;
    setup.native_adapter_selector = Some(std::sync::Arc::new(|adapters, _| {
        let a = adapters
            .iter()
            .find(|a| {
                let i = a.get_info();
                i.backend == wgpu::Backend::Vulkan && i.device_type == wgpu::DeviceType::DiscreteGpu && i.name.contains("5090")
            })
            .ok_or_else(|| "RTX 5090 Vulkan discrete adapter required".to_string())?;
        assert_eq!(photocraft_gpu::Compositor::preferred_acc_format(a), wgpu::TextureFormat::Rgba32Float);
        eprintln!("Affinity full app GPU: {:?}", a.get_info());
        Ok(a.clone())
    }));
    let mut setup = egui_wgpu::WgpuSetup::CreateNew(setup);
    photocraft_ui_egui::gpu_canvas::use_adapter_limits(&mut setup);
    setup
}
type H = Harness<'static, PhotocraftApp>;
fn screen(h: &H, x: f32, y: f32) -> egui::Pos2 {
    let app = h.state();
    let v = &app.ui.views[0];
    ViewXform { rect: photocraft_ui_egui::rulers::content_rect(app, app.last_canvas_rect), zoom: v.zoom, center: v.center, flip: false, rotation: v.rotation }
        .to_screen(x, y)
}
fn button(h: &mut H, x: f32, y: f32, pressed: bool) {
    h.event(Event::PointerButton { pos: screen(h, x, y), button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
    h.step();
}
#[test]
fn affinity_app_gpu_live_preview_release_editability_warning_and_undo() {
    let _gpu_gate = crate::gpu_lock();

    for ppp in [1.0, 2.0] {
        let mut h = Harness::builder().with_size(egui::vec2(1100.0, 800.0)).with_pixels_per_point(ppp).wgpu_setup(setup()).build_eframe(|cc| {
            PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
            let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
            app.set_wgpu(cc.wgpu_render_state.as_ref().expect("GPU required").clone());
            app
        });
        h.state_mut().run("file.new", json!({"width":128,"height":128,"background":"white"})).unwrap();
        h.state_mut().sync_views();
        {
            let app = h.state_mut();
            app.ui.tool = Tool::Move;
            app.ui.extras.rulers = false;
            app.ui.extras.snap = false;
            app.ui.views[0].zoom = 1.0;
            app.ui.views[0].center = [64.0, 64.0];
            app.ui.views[0].fit_pending = false;
            app.affinity_import.source = include_bytes!("../../../../pc-affinity/tests/fixtures/warnings.afphoto").to_vec();
            app.affinity_import.stage(&app.session).unwrap();
        }
        h.run_steps(3);
        assert!(h.state().gpu_active());
        assert!(h.state().affinity_import.pending(), "Affinity staging must survive idle app frames");
        let before = h.state().session.active().unwrap().doc.clone();
        let revision = h.state().session.active().unwrap().revision;
        let journal = h.state().session.journal.len();
        let initial = h.render().unwrap();
        button(&mut h, 20.0, 20.0, true);
        h.event(Event::PointerMoved(screen(&h, 40.0, 36.0)));
        h.step();

        assert_eq!(h.state().session.active().unwrap().doc, before);
        assert_eq!(h.state().session.active().unwrap().revision, revision);
        assert_eq!(h.state().session.journal.len(), journal);
        let detached = h.state().affinity_import.preview.clone().expect("actual app detached Affinity preview");
        let expected = photocraft_compose::flatten(&detached);
        let preview = h.render().unwrap();
        assert_ne!(initial, preview, "real app pointer frame must move Affinity pixels");
        button(&mut h, 40.0, 36.0, false);
        let after = h.render().unwrap();
        assert_eq!(expected, photocraft_compose::flatten(&h.state().session.active().unwrap().doc));
        assert_eq!(h.state().session.journal.len(), journal);
        let mut max = 0u8;
        let mut max_cpu = 0u8;
        let mut worst_cpu = None;
        // Harness::render adds a synthetic 16-point mouse triangle, unlike an
        // actual app screenshot. Omit its bounds plus stroke/AA from the ink oracle.
        let cursor = h.ctx.input(|i| i.pointer.hover_pos()).unwrap();
        // The acute triangle tip has a miter extending beyond its vertices.
        let cursor = egui::Rect::from_min_size(cursor - egui::vec2(5.0, 5.0), egui::vec2(26.0, 26.0));
        let mut checked = 0;
        for y in 28..50 {
            for x in 32..56 {
                let p = screen(&h, x as f32 + 0.5, y as f32 + 0.5) * ppp;
                if cursor.contains(p / ppp) {
                    continue;
                }
                checked += 1;
                let a = preview.get_pixel(p.x.floor() as u32, p.y.floor() as u32).0;
                let b = after.get_pixel(p.x.floor() as u32, p.y.floor() as u32).0;
                let cpu = expected.get(x, y);
                for c in 0..4 {
                    max = max.max(a[c].abs_diff(b[c]));
                    let value = (cpu[c].clamp(0.0, 1.0) * 255.0).round() as u8;
                    if b[c].abs_diff(value) > max_cpu {
                        worst_cpu = Some((x, y, b, cpu, p));
                    }
                    max_cpu = max_cpu.max(b[c].abs_diff(value));
                }
                assert!(b[1] < 16 && b[3] == 255, "imported shape ink at {x},{y} ({p:?}), {ppp}x: GPU {b:?}, CPU {cpu:?}");
            }
        }
        assert!(checked >= 120, "at least 120 unobstructed shape ink samples");
        assert!(max <= 1, "actual canvas preview/release max difference {max}/255");
        assert!(max_cpu <= 1, "actual canvas/CPU premultiplied ink max difference {max_cpu}/255 at {ppp}x: {worst_cpu:?}");
        eprintln!("Affinity app {ppp}x: preview/release {max}/255; CPU/canvas {max_cpu}/255");
        // The real app dialog's warning button selects the imported editable shape.
        photocraft_ui_egui::vector_integration::set_panel_open(h.state_mut(), "Affinity Import", true);
        h.run_steps(2);
        h.get_by_label("unsupported-record: Future").click();
        h.run_steps(2);
        let st = h.state().session.active().unwrap();
        assert_eq!(st.doc.layer(st.active_layer.unwrap()).unwrap().name, "Future");
        let id = st.doc.walk().into_iter().find(|(_, _, l)| l.name == "Visible badge").unwrap().2.id;
        assert!(matches!(st.doc.layer(id).unwrap().content, photocraft_doc::LayerContent::Shape(_)));
        photocraft_ui_egui::vector_integration::set_panel_open(h.state_mut(), "Affinity Import", false);
        let path = if let photocraft_doc::LayerContent::Shape(sh) = &h.state().session.active().unwrap().doc.layer(id).unwrap().content {
            sh.path.transform(&photocraft_doc::Affine::translate(2.0, 0.0))
        } else {
            panic!("editable shape required")
        };
        h.state_mut().run("shape.applyPaths", json!({"edits":[{"layer":id.0,"path":path}]})).unwrap();
        assert!(h.state_mut().session.undo());
        assert!(h.state_mut().session.undo());
        assert_eq!(h.state().session.active().unwrap().doc, before);
        {
            let app = h.state_mut();
            app.affinity_import.stage(&app.session).unwrap();
        }
        h.key_press(egui::Key::Escape);
        h.step();
        assert!(h.state().affinity_import.preview.is_none());
        assert_eq!(h.state().session.active().unwrap().doc, before);
        {
            let app = h.state_mut();
            app.affinity_import.stage(&app.session).unwrap();
        }
        h.step();
        h.state_mut().ui.tool = Tool::Brush;
        h.step();
        assert!(!h.state().affinity_import.pending(), "tool switch cancels detached Affinity placement");
        assert_eq!(h.state().session.active().unwrap().doc, before);
    }
}
