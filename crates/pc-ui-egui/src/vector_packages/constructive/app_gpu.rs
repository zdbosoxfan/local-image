//! Full app, offscreen live pointer frames, GPU canvas, release/undo/cancel gate.
//! No software-adapter fallback and no skip.
use egui::{Event, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;
use photocraft_doc::{Color, ColorMode, Document, Fill, Layer, LayerContent, SampleType, ShapeLayer, Size};
use photocraft_engine::Session;
use photocraft_ui_egui::{PhotocraftApp, canvas::ViewXform, constructive::Tool};

type H = Harness<'static, PhotocraftApp>;
fn harness(tool: Tool) -> H {
    let mut setup = photocraft_ui_egui::gpu_canvas::wgpu_setup();
    photocraft_ui_egui::gpu_canvas::use_adapter_limits_with(&mut setup, |adapter| {
        let info = adapter.get_info();
        #[cfg(not(target_os = "macos"))]
        assert!(info.name.contains("RTX 5090"), "RTX 5090 required: {info:?}");
        assert_eq!(info.backend, if cfg!(target_os = "macos") { eframe::wgpu::Backend::Metal } else { eframe::wgpu::Backend::Vulkan });
        #[cfg(not(target_os = "macos"))]
        assert_eq!(info.device_type, eframe::wgpu::DeviceType::DiscreteGpu);
        #[cfg(target_os = "macos")]
        assert_ne!(info.device_type, eframe::wgpu::DeviceType::Cpu, "hardware Metal adapter required");
        assert_eq!(photocraft_gpu::Compositor::preferred_acc_format(adapter), eframe::wgpu::TextureFormat::Rgba32Float);
        assert!(photocraft_ui_egui::gpu_canvas::supports_f16_canvas(adapter));
        eprintln!("constructive app GPU: {info:?}");
    });
    let mut doc = Document::with_background("Constructive GPU", Size::new(160, 160), ColorMode::Rgb, SampleType::F32, Color::WHITE);
    let paths = if tool == Tool::Builder {
        vec![photocraft_vector::shapes::ellipse(16., 24., 80., 80.), photocraft_vector::shapes::ellipse(56., 24., 80., 80.)]
    } else {
        vec![photocraft_vector::shapes::rect(20., 20., 100., 100.)]
    };
    let mut ids = Vec::new();
    for path in paths {
        let mut sh = ShapeLayer { path, fill: Some(Fill::Solid(Color::BLACK)), ..Default::default() };
        photocraft_engine::vector_cmds::refresh_shape(&doc, &mut sh);
        let l = Layer::new("Shape", LayerContent::Shape(sh));
        ids.push(l.id);
        doc.layers.push(l);
    }
    let mut s = Session::new();
    s.add_document(doc, None);
    s.select_layer(ids[0]).unwrap();
    s.active_mut().unwrap().selected_layers = ids;
    let mut h = Harness::builder().with_size(egui::vec2(1100., 780.)).wgpu_setup(setup).build_eframe(move |cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        let mut app = PhotocraftApp::new(s, Default::default());
        app.set_wgpu(cc.wgpu_render_state.as_ref().expect("real GPU required").clone());
        app.sync_views();
        app.ui.views[0].zoom = 1.;
        app.ui.views[0].fit_pending = false;
        app.ui.views[0].center = [80., 80.];
        app.ui.extras.rulers = false;
        app.ui.extras.snap = false;
        app.ui.view.show.layer_edges = false;
        app.ui.view.show.smart_guides = false;
        app.ui.tool_options.move_auto_select = false;
        app.ui.tool_options.move_show_transform = false;
        app.session.edit_prefs(|p| p.interface.contextual_task_bar = false);
        app.ui.tool = match tool {
            Tool::Builder => photocraft_ui_egui::state::Tool::ShapeBuilder,
            Tool::Knife => photocraft_ui_egui::state::Tool::Knife,
            Tool::Scissors => photocraft_ui_egui::state::Tool::Scissors,
            Tool::LineCut => photocraft_ui_egui::state::Tool::LineCut,
            Tool::Contour => photocraft_ui_egui::state::Tool::Contour,
            Tool::Smooth => photocraft_ui_egui::state::Tool::Smooth,
            Tool::Eraser => photocraft_ui_egui::state::Tool::PathEraser,
            Tool::Join => photocraft_ui_egui::state::Tool::Join,
        };
        app.constructive_state.set_tool(Some(tool));
        app
    });
    h.run_steps(4);
    let center = xf(&h).rect.center();
    h.state_mut().ui.views[0].center = [80. + center.x.fract(), 80. + center.y.fract()];
    h.run_steps(3);
    assert!(h.state().gpu_active());
    h
}
fn xf(h: &H) -> ViewXform {
    ViewXform::active(h.state()).unwrap()
}
fn pointer(h: &mut H, p: Pos2) {
    h.event(Event::PointerMoved(p));
    h.step();
}
fn button(h: &mut H, p: Pos2, pressed: bool, mods: Modifiers) {
    h.event(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed, modifiers: mods });
    h.step();
}
fn assert_canvas(h: &mut H, expected: &Document) {
    h.render().expect("actual app GPU paint callbacks");
    let (_, size, pixels) = h.state().read_canvas_texels(expected.id.0).expect("actual float canvas texture");
    assert_eq!(size, [expected.size.width, expected.size.height]);
    let cpu = photocraft_compose::render(expected, expected.bounds());
    let mut max = 0.0f32;
    let mut checked = 0;
    for y in 4..156 {
        for x in 4..156 {
            let actual = pixels[(y * size[0] + x) as usize];
            let want = cpu.get(x as i32, y as i32);
            for c in 0..4 {
                assert!(actual[c].is_finite());
                let premultiplied = if c == 3 { want[c] } else { want[c] * want[3] };
                max = max.max((actual[c] - premultiplied).abs());
            }
            checked += 1;
        }
    }
    assert_eq!(checked, 152 * 152);
    assert!(max <= 1. / 255. + 1e-7, "premultiplied app canvas max diff={max}");
    assert!(h.state().perf.gpu && h.state().perf.gpu_fallback.is_none());
}

#[test]
fn constructive_full_app_gpu_live_preview_release_remove_cancel_and_undo() {
    let _gpu_gate = crate::gpu_lock();

    for (tool, remove) in [(Tool::Builder, false), (Tool::Builder, true), (Tool::Knife, false), (Tool::Contour, false)] {
        let mut h = harness(tool);
        let before = h.state().session.active().unwrap().doc.clone();
        assert_canvas(&mut h, &before);
        let (from, to) = match tool {
            Tool::Builder => ([32., 64.], [120., 64.]),
            Tool::Knife => ([70., 0.], [70., 150.]),
            Tool::Contour => ([120., 70.], [132., 70.]),
            _ => unreachable!(),
        };
        let a = xf(&h).to_screen(from[0], from[1]);
        let b = xf(&h).to_screen(to[0], to[1]);
        pointer(&mut h, a);
        if tool == Tool::Builder {
            assert!(h.state().constructive_state.hover.is_some());
        }
        button(&mut h, a, true, Modifiers::NONE);
        pointer(&mut h, b);
        let mods = if remove { Modifiers::ALT } else { Modifiers::NONE };
        h.event(Event::ModifiersChanged(mods));
        h.step();
        assert!(h.state().constructive_state.is_dragging());
        assert_eq!(h.state().session.active().unwrap().doc, before);
        assert!(h.state().session.journal.is_empty());
        let preview = h.state().constructive_state.display_document(&h.state().session).unwrap().0;
        assert_ne!(preview, before);
        assert_canvas(&mut h, &preview);
        button(&mut h, b, false, mods);
        h.run_steps(3);
        assert!(h.state().constructive_state.error.is_none(), "{:?}", h.state().constructive_state.error);
        let committed = h.state().session.active().unwrap().doc.clone();
        let c = photocraft_compose::render(&committed, committed.bounds());
        let p = photocraft_compose::render(&preview, preview.bounds());
        for y in 0..160 {
            for x in 0..160 {
                assert_eq!(c.get(x, y), p.get(x, y));
            }
        }
        assert_eq!(h.state().session.journal.len(), 1);
        assert_canvas(&mut h, &committed);
        assert!(h.state_mut().session.undo());
        h.run_steps(3);
        assert_eq!(h.state().session.active().unwrap().doc, before);
        assert_canvas(&mut h, &before);
        assert!(!h.state_mut().session.undo());
    }
    let mut h = harness(Tool::Contour);
    let before = h.state().session.active().unwrap().doc.clone();
    let a = xf(&h).to_screen(120., 70.);
    let b = xf(&h).to_screen(130., 70.);
    pointer(&mut h, a);
    button(&mut h, a, true, Modifiers::NONE);
    pointer(&mut h, b);
    let preview = h.state().constructive_state.display_document(&h.state().session).unwrap().0;
    assert_canvas(&mut h, &preview);
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    button(&mut h, b, false, Modifiers::NONE);
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert!(h.state().session.journal.is_empty());
    assert_canvas(&mut h, &before);
}
