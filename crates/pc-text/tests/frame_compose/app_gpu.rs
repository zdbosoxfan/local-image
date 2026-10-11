//! Package 00 registers this in pc-ui-egui/tests/gpu/main.rs after integration/30.md.
//! This source intentionally depends on the real app State/Metadata/canvas hooks.
//! It is not a standalone widget test or a successful skipped-adapter gate.
use egui::{Event, Modifiers, PointerButton};
use egui_kittest::{Harness, kittest::Queryable};
use photocraft_doc::{Document, LayerContent, text::TextShape};
use photocraft_text::{TextEngine, frame_compose::compose};
use photocraft_ui_egui::{
    PhotocraftApp,
    canvas::ViewXform,
    frame_compose::{Metadata, NativeMetadata},
    gpu_canvas::GpuCanvas,
};
use serde_json::json;
use std::sync::{Arc, Mutex};
type H = Harness<'static, PhotocraftApp>;
fn harness() -> (H, GpuCanvas) {
    use eframe::egui_wgpu::{WgpuSetup, WgpuSetupCreateNew, wgpu};
    let mut create = WgpuSetupCreateNew::without_display_handle();
    create.instance_descriptor.backends = wgpu::Backends::VULKAN;
    create.native_adapter_selector = Some(Arc::new(|adapters, _| {
        let a = adapters
            .iter()
            .find(|a| {
                let i = a.get_info();
                i.name.contains("RTX 5090") && i.backend == wgpu::Backend::Vulkan && i.device_type == wgpu::DeviceType::DiscreteGpu
            })
            .ok_or_else(|| "frame gate requires RTX 5090 / Vulkan / discrete".to_owned())?;
        assert_eq!(photocraft_gpu::Compositor::preferred_acc_format(a), wgpu::TextureFormat::Rgba32Float);
        assert!(photocraft_ui_egui::gpu_canvas::supports_f16_canvas(a));
        eprintln!("frame30 GPU adapter {:?}", a.get_info());
        Ok(a.clone())
    }));
    let observer = Arc::new(Mutex::new(None));
    let capture = observer.clone();
    let mut h = Harness::builder().with_size(egui::vec2(1280.0, 900.0)).with_max_steps(64).wgpu_setup(WgpuSetup::CreateNew(create)).build_eframe(move |cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        let rs = cc.wgpu_render_state.as_ref().expect("real GPU renderer");
        *capture.lock().unwrap() = Some(GpuCanvas::new(rs));
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        app.set_wgpu(rs.clone());
        app.run("file.new", json!({"width":300,"height":200,"background":"transparent"})).unwrap();
        app.run("type.create", json!({"box":[10,10,200,120],"text":"one two three four five six seven eight nine","size":12,"color":"#923da1"})).unwrap();
        app.ui.tool = photocraft_ui_egui::state::Tool::Type;
        app.sync_views();
        app.ui.extras.rulers = false;
        app.ui.views[0].fit_pending = false;
        app.ui.views[0].zoom = 2.0;
        app.ui.views[0].center = [150.0, 100.0];
        app.session.edit_prefs(|p| p.interface.contextual_task_bar = false);
        app
    });
    h.run_steps(4);
    let gpu = observer.lock().unwrap().clone().unwrap();
    assert!(h.state().perf.gpu);
    (h, gpu)
}
fn pointer(h: &mut H, x: f32, press: Option<bool>) {
    let at = ViewXform::active(h.state()).unwrap().to_screen(x, 50.0);
    assert!(h.state().last_canvas_rect.contains(at));
    h.event(Event::PointerMoved(at));
    if let Some(pressed) = press {
        h.event(Event::PointerButton { pos: at, pressed, button: PointerButton::Primary, modifiers: Modifiers::NONE });
    }
    h.run_steps(2);
}
fn present(h: &mut H, gpu: &GpuCanvas, doc: &Document) {
    h.run_steps(3);
    h.render().expect("actual app GPU completion");
    assert!(h.state().perf.gpu && h.state().perf.gpu_fallback.is_none());
    let (_, size, pixels) = gpu.read_texels(doc.id.0).expect("actual app document texture");
    assert_eq!(size, [300, 200]);
    let cpu = photocraft_compose::flatten(doc);
    assert_eq!(pixels.len(), cpu.px.len());
    let max =
        cpu.px.iter().zip(&pixels).flat_map(|(c, g)| (0..4).map(move |i| ((if i == 3 { c[3] } else { c[i] * c[3] }) - g[i]).abs())).fold(0.0f32, f32::max);
    assert!(max <= 1.0 / 255.0, "premultiplied frame-text max diff {max}");
}
#[test]
fn frame_compose_full_app_rtx_option_width_live_preview_release_undo_cancel() {
    let _lock = super::gpu_lock();
    let (mut h, gpu) = harness();
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Frame text").click();
    h.run_steps(2);
    h.get_by_label("Paragraph composer").click();
    h.run_steps(2);
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Frame text").click();
    h.run_steps(2);
    h.get_by_label("Optical margins").click();
    h.run_steps(2);
    let preview = h.state().ui.frame_compose.preview.clone().expect("live option preview");
    present(&mut h, &gpu, &preview);
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Frame text").click();
    h.run_steps(2);
    h.get_by_label("Apply frame text").click();
    h.run_steps(3);
    let before = h.state().session.active().unwrap().doc.clone();
    let id = h.state().session.active().unwrap().active_layer.unwrap();
    assert!(NativeMetadata.load(&before, id).unwrap().enabled);
    present(&mut h, &gpu, &before);
    let history = h.state().session.active().unwrap().history.past_len();
    let revision = h.state().session.active().unwrap().revision;
    pointer(&mut h, 210.0, Some(true));
    pointer(&mut h, 100.0, None);
    assert!(h.state().ui.frame_compose.dragging);
    assert!(Arc::ptr_eq(&before, &h.state().session.active().unwrap().doc));
    assert_eq!(revision, h.state().session.active().unwrap().revision);
    let preview = h.state().ui.frame_compose.preview.clone().expect("actual app held width preview");
    present(&mut h, &gpu, &preview);
    let LayerContent::Text(a) = &before.layer(id).unwrap().content else { panic!("frame") };
    let LayerContent::Text(b) = &preview.layer(id).unwrap().content else { panic!("frame") };
    let options = NativeMetadata.load(&before, id).unwrap();
    let mut e = TextEngine::new();
    assert!(
        compose(&mut e, b, before.resolution_dpi, options).unwrap().layout.lines.len()
            > compose(&mut e, a, before.resolution_dpi, options).unwrap().layout.lines.len()
    );
    assert!(matches!(b.shape,TextShape::Box{width,..} if (width-90.0).abs()<0.1));
    pointer(&mut h, 100.0, Some(false));
    assert_eq!(h.state().session.active().unwrap().history.past_len(), history + 1);
    let released = h.state().session.active().unwrap().doc.clone();
    present(&mut h, &gpu, &released);
    assert!(h.state().ui.frame_compose.preview.is_none());
    h.state_mut().run("edit.undo", json!({})).unwrap();
    h.run_steps(3);
    assert_eq!(h.state().session.active().unwrap().doc, before);
    present(&mut h, &gpu, &before);
    pointer(&mut h, 210.0, Some(true));
    pointer(&mut h, 80.0, None);
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    pointer(&mut h, 80.0, Some(false));
    assert_eq!(h.state().session.active().unwrap().doc, before);
    present(&mut h, &gpu, &before);
}
