//! Full-app RTX acceptance source for 00, registered only AFTER its canvas/menu hooks.
//! No test-only projection injection: real PhotocraftApp events, GpuCanvas and UI controls.
use eframe::egui_wgpu::{self, wgpu};
use egui::{Event, Modifiers, PointerButton};
use egui_kittest::{Harness, kittest::Queryable};
use photocraft_doc::{Color, Fill, Layer, LayerContent, ShapeLayer};
use photocraft_engine::vector_packages::blend as engine;
use photocraft_ui_egui::{PhotocraftApp, canvas::ViewXform};
use serde_json::json;
fn setup() -> egui_wgpu::WgpuSetup {
    let mut create = egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    create.instance_descriptor.backends = wgpu::Backends::VULKAN;
    create.native_adapter_selector = Some(std::sync::Arc::new(|adapters, _| {
        let adapter = adapters
            .iter()
            .find(|a| {
                let i = a.get_info();
                i.backend == wgpu::Backend::Vulkan && i.device_type == wgpu::DeviceType::DiscreteGpu && i.name.contains("RTX 5090")
            })
            .ok_or_else(|| "RTX 5090 Vulkan discrete required".to_string())?;
        assert_eq!(photocraft_gpu::Compositor::preferred_acc_format(adapter), wgpu::TextureFormat::Rgba32Float);
        eprintln!("blend18 full-app GPU {:?}", adapter.get_info());
        Ok(adapter.clone())
    }));
    let mut setup = egui_wgpu::WgpuSetup::CreateNew(create);
    photocraft_ui_egui::gpu_canvas::use_adapter_limits(&mut setup);
    setup
}
fn pointer(h: &mut Harness<'_, PhotocraftApp>, x: f32, y: f32, pressed: Option<bool>) {
    let p = ViewXform::active(h.state()).unwrap().to_screen(x, y);
    h.event(Event::PointerMoved(p));
    if let Some(pressed) = pressed {
        h.event(Event::PointerButton { pos: p, pressed, button: PointerButton::Primary, modifiers: Modifiers::NONE });
    }
    h.run_steps(1);
}
#[test]
#[ignore = "requires 00 canvas/menu integration and RTX GPU; speed rules defer execution"]
fn blend18_full_app_rtx_live_preview_release_expand_cancel_and_undo() {
    let mut h = Harness::builder().with_size(egui::vec2(1500., 1000.)).wgpu_setup(setup()).build_eframe(|cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        app.set_wgpu(cc.wgpu_render_state.as_ref().unwrap().clone());
        app
    });
    h.state_mut().run("file.new", json!({"width":240,"height":180,"background":"white"})).unwrap();
    h.state_mut().sync_views();
    let ids = h
        .state_mut()
        .session
        .edit("Blend fixture", |doc, _| {
            let mut ids = Vec::new();
            for (x, r, c) in [(40., 10., Color::BLACK), (160., 16., Color::WHITE)] {
                let mut shape =
                    ShapeLayer { path: photocraft_vector::shapes::ellipse(x - r, 60. - r, 2. * r, 2. * r), fill: Some(Fill::Solid(c)), ..Default::default() };
                shape.cache = Some(photocraft_vector::render_shape(&shape, doc.pixel_format(), doc.bounds()));
                let layer = Layer::new("Blend key", LayerContent::Shape(shape));
                ids.push(layer.id);
                doc.layers.push(layer);
            }
            Ok(ids)
        })
        .unwrap();
    h.state_mut().run("layer.select", json!({"layer":ids[0]})).unwrap();
    h.state_mut().run("layer.select", json!({"layer":ids[1],"mode":"toggle"})).unwrap();
    h.state_mut().run("prefs.set", json!({"values":{"toolbar.persona":"vector"}})).unwrap();
    h.state_mut().ui.views[0].zoom = 2.;
    h.state_mut().ui.views[0].center = [100., 90.];
    h.state_mut().ui.views[0].fit_pending = false;
    h.state_mut().ui.extras.rulers = false;
    h.run_steps(5);
    h.get_by_label("Blend Options").click();
    h.run_steps(3);
    h.get_by_label("Specified steps").click();
    h.run_steps(2);
    h.get_by_label("Create Blend").click();
    h.run_steps(3);
    let id = h.state().session.active().unwrap().active_layer.unwrap();
    h.state_mut()
        .session
        .edit("Blend clip/effects", |doc, _| {
            doc.layer_mut(id).unwrap().blend = photocraft_doc::BlendMode::Normal;
            doc.layer_mut(id).unwrap().effects.items.push(photocraft_doc::effects::Effect::default_drop_shadow());
            let mut clipped = Layer::raster("Clipped overlay", doc.pixel_format());
            clipped.clipped = true;
            clipped.surface_mut().unwrap().fill_rect(doc.bounds(), &[0.2, 0.4, 0.7, 0.3]);
            doc.layers.push(clipped);
            Ok(())
        })
        .unwrap();
    h.run_steps(3);
    h.get_by_label("Edit blend spine").click();
    h.run_steps(2);
    let before = h.state().session.active().unwrap().doc.clone();
    let journal = h.state().session.journal.len();
    pointer(&mut h, 100., 60., Some(true));
    pointer(&mut h, 100., 110., None);
    h.run_steps(2);
    assert!(h.state().ui.vector_blend.preview.is_some());
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert_eq!(h.state().session.journal.len(), journal);
    let live = h.render().unwrap();
    assert!(h.state().gpu_active());
    pointer(&mut h, 100., 110., Some(false));
    h.run_steps(3);
    let release = h.render().unwrap();
    assert_eq!(h.state().session.journal.len(), journal + 1);
    let xf = ViewXform::active(h.state()).unwrap();
    let ppp = h.ctx.pixels_per_point();
    for y in 80..135 {
        for x in 60..140 {
            let p = xf.to_screen(x as f32 + 0.5, y as f32 + 0.5) * ppp;
            let a = live.get_pixel(p.x.floor() as u32, p.y.floor() as u32);
            let b = release.get_pixel(p.x.floor() as u32, p.y.floor() as u32);
            for i in 0..4 {
                let (a, b) = if i == 3 {
                    (u32::from(a[i]) * 255, u32::from(b[i]) * 255)
                } else {
                    (u32::from(a[i]) * u32::from(a[3]), u32::from(b[i]) * u32::from(b[3]))
                };
                assert!(a.abs_diff(b) <= 255);
            }
        }
    }
    h.get_by_label("Expand Blend").click();
    h.run_steps(3);
    assert!(h.state().session.active().unwrap().doc.layer(id).unwrap().vector.live.is_none());
    assert!(h.state_mut().session.undo());
    assert!(h.state_mut().session.undo());
    h.run_steps(3);
    assert_eq!(h.state().session.active().unwrap().doc, before);
    pointer(&mut h, 100., 60., Some(true));
    pointer(&mut h, 100., 110., None);
    h.key_press(egui::Key::Escape);
    pointer(&mut h, 100., 110., Some(false));
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert!(h.state().ui.vector_blend.preview.is_none());
    let spec = engine::spec(&h.state().session, id).unwrap();
    assert!(spec.spine.is_none());
}
