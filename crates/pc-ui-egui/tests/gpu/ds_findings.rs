use egui::{Event, Modifiers, PointerButton, pos2, vec2};
use photocraft_color::{Color, ColorMode, SampleType};
use photocraft_doc::{Document, Size};
use photocraft_geom::Rect;
use photocraft_ui_egui::{
    PhotocraftApp,
    gpu_canvas::{self, GpuCanvas},
};
use serde_json::json;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[test]
fn ds_finding_encode_pool_releases_unused_tiles() {
    let _lock = crate::gpu_lock();
    let rs = egui_kittest::wgpu::create_render_state(gpu_canvas::wgpu_setup(), Default::default());
    let info = rs.adapter.get_info();
    assert_ne!(info.device_type, eframe::wgpu::DeviceType::Cpu);
    eprintln!("DS encode-pool adapter: {info:?}");
    let gpu = GpuCanvas::with_tile(&rs, Some(128));
    let doc = Document::with_background("pool", Size::new(512, 512), ColorMode::Rgb, SampleType::U8, Color::rgb(0.2, 0.3, 0.4));
    gpu.composite(&doc, doc.bounds(), true).unwrap();
    let full = gpu.encode_bindings();
    assert!(full > 1);
    gpu.composite(&doc, Rect::new(0, 0, 32, 32), true).unwrap();
    assert_eq!(gpu.encode_bindings(), 1, "bindings from untouched tiles must be dropped (previously {full})");
    gpu.retain(&[]);
    assert_eq!(gpu.encode_bindings(), 0);
}

#[test]
fn ds_finding_real_app_navigator_job_finishes_during_drag() {
    let _lock = crate::gpu_lock();
    let mut h = egui_kittest::Harness::builder().with_size(vec2(1000.0, 720.0)).with_max_steps(64).wgpu_setup(gpu_canvas::wgpu_setup()).build_eframe(|cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        let rs = cc.wgpu_render_state.as_ref().expect("hardware renderer");
        let info = rs.adapter.get_info();
        assert_ne!(info.device_type, eframe::wgpu::DeviceType::Cpu);
        eprintln!("DS app adapter: {info:?}");
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        app.set_wgpu(rs.clone());
        app.run("file.new", json!({"width":5000,"height":1000})).unwrap();
        app.sync_views();
        app.ui.extras.rulers = false;
        app
    });
    h.run_steps(4);
    h.render().unwrap();
    assert!(h.state().perf.gpu);
    let doc = h.state().session.active().unwrap().doc.clone();
    let id = doc.id;
    let ctx = h.ctx.clone();
    let key = egui::Id::new(("navigator", id.0));
    h.state_mut().ui.tool = photocraft_ui_egui::state::Tool::RectMarquee;
    let at = h.state().last_canvas_rect.center();
    h.hover_at(at);
    h.run_steps(1);
    h.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.run_steps(1);
    h.event(Event::PointerMoved(at + vec2(40.0, 20.0)));
    h.run_steps(1);
    assert!(h.ctx.input(|i| i.pointer.primary_down()));
    type Cached = (std::sync::Weak<Document>, u64, egui::TextureHandle);
    type NavigatorJob = (std::sync::Weak<Document>, u64, Arc<std::sync::Mutex<Option<egui::ColorImage>>>, f64);
    ctx.data_mut(|d| {
        d.remove::<Cached>(key);
        d.remove::<NavigatorJob>(egui::Id::new(("navigator-job", id.0)));
    });
    photocraft_ui_egui::canvas::navigator_texture(h.state_mut(), &ctx, 0).unwrap();
    assert_eq!(ctx.data(|d| d.get_temp::<Cached>(key)).unwrap().1, u64::MAX, "regression starts with the initial placeholder while the pointer is held");
    let start = Instant::now();
    let mut frames = Vec::new();
    while start.elapsed() < Duration::from_secs(10) {
        let t = Instant::now();
        photocraft_ui_egui::canvas::navigator_texture(h.state_mut(), &ctx, 0).unwrap();
        h.run_steps(1);
        h.render().unwrap();
        frames.push(t.elapsed().as_secs_f64() * 1000.0);
        if ctx.data(|d| d.get_temp::<Cached>(key)).is_some_and(|(r, p, t)| r.ptr_eq(&Arc::downgrade(&doc)) && p != u64::MAX && t.size()[0] > 1) {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    let (_, p, tex) = ctx.data(|d| d.get_temp::<Cached>(key)).unwrap();
    assert_ne!(p, u64::MAX, "drag must keep polling the initial placeholder");
    assert!(tex.size()[0] > 1);
    frames.sort_by(f64::total_cmp);
    eprintln!(
        "DS actual app 5MP navigator during drag: {} frames, median {:.2} ms, max {:.2} ms",
        frames.len(),
        frames[frames.len() / 2],
        frames[frames.len() - 1]
    );
    h.event(Event::PointerButton { pos: pos2(at.x + 40.0, at.y + 20.0), button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(2);
}
