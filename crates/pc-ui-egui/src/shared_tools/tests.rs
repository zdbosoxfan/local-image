#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use egui::{Key, PointerButton};
use egui_kittest::{Harness, kittest::Queryable};
use photocraft_doc::{LayerContent, Path};
use serde_json::json;
struct Host {
    app: PhotocraftApp,
    tools: State,
    id: &'static str,
    persona: Persona,
    rect: Rect,
    flip: bool,
}
fn host(id: &'static str, persona: Persona) -> Host {
    let mut app = PhotocraftApp::new(Session::new(), crate::Services::default());
    app.run("file.new", json!({"width":240,"height":240})).unwrap();
    app.sync_views();
    app.ui.tool = crate::Tool::from_name(id).unwrap_or(crate::Tool::Move);
    app.ui.tool_options.move_auto_select = false;
    app.session.edit_prefs(|p| p.toolbar.persona = persona);
    let mut tools = State::default();
    tools.snapping.enabled = false;
    Host { app, tools, id, persona, rect: Rect::NOTHING, flip: false }
}
fn harness(host: Host) -> Harness<'static, Host> {
    harness_at_scale(host, 1.0)
}
fn harness_at_scale(host: Host, ppp: f32) -> Harness<'static, Host> {
    Harness::builder().with_size(egui::vec2(900., 700.)).with_pixels_per_point(ppp).build_ui_state(
        |ui, h| {
            h.tools.activate(&mut h.app, h.id, h.persona);
            if registry::handler(h.id) == Some(Handler::Pen) {
                pen::context(ui, &mut h.tools.pen);
            }
            let rect = ui.allocate_space(egui::vec2(260., 260.)).1;
            h.rect = rect;
            let xf = ViewXform { rect, zoom: 1., center: [130., 130.], flip: h.flip, rotation: 0.0 };
            h.tools.input(&mut h.app, ui, h.id, h.persona, &xf, rect);
            h.tools.overlay(ui.painter(), &xf, &h.app);
        },
        host,
    )
}
fn point_event(h: &mut Harness<'_, Host>, p: [f32; 2], pressed: Option<bool>, mods: Modifiers) {
    let xf = ViewXform { rect: h.state().rect, zoom: 1., center: [130., 130.], flip: h.state().flip, rotation: 0.0 };
    let pos = xf.to_screen(p[0], p[1]);
    h.event(egui::Event::ModifiersChanged(mods));
    h.event(egui::Event::PointerMoved(pos));
    if let Some(pressed) = pressed {
        h.event(egui::Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: mods });
    }
    h.step();
    assert!(!h.state().app.ui.status_error, "{} at {p:?} pressed={pressed:?}: {}", h.state().id, h.state().app.ui.status);
}
fn draw(h: &mut Harness<'_, Host>, start: [f32; 2], end: [f32; 2], mods: Modifiers) {
    point_event(h, start, Some(true), Modifiers::NONE);
    point_event(h, end, None, mods);
    point_event(h, end, Some(false), mods);
}
fn active_path(app: &PhotocraftApp) -> Path {
    let st = app.session.active().unwrap();
    st.doc
        .work_path
        .clone()
        .or_else(|| {
            st.active_layer.and_then(|id| st.doc.layer(id)).and_then(|l| if let LayerContent::Shape(s) = &l.content { Some(s.path.clone()) } else { None })
        })
        .unwrap()
}
fn add_curve(app: &mut PhotocraftApp) -> LayerId {
    let id=app.run("shape.create",json!({"kind":"path","path":{"subpaths":[{"closed":true,"knots":[{"anchor":[40,40]},{"anchor":[150,40]},{"anchor":[150,150]},{"anchor":[40,150]}]}]},"fill":"#4488cc"})).unwrap()["layer"].as_u64().unwrap();
    LayerId(id)
}
#[test]
fn shared47_dispatch_function_identity_covers_every_shared_tool() {
    for id in photocraft_engine::toolsets::ALL_TOOLS
        .iter()
        .copied()
        .filter(|id| registry::resolve(id, Persona::Vector).is_some())
        .chain(registry::SHARED_ADDITIONS.iter().copied())
    {
        let pixel = handler(id, Persona::Pixel).unwrap();
        let vector = handler(id, Persona::Vector).unwrap();
        assert!(std::ptr::fn_addr_eq(pixel, vector), "{id} uses a second handler");
    }
}
#[test]
fn shared47_pen_modes_click_drag_preview_output_and_undo_in_both_personas() {
    for mode in ["Pen", "Smart", "Polygon", "Line"] {
        let mut paths = Vec::new();
        for persona in [Persona::Pixel, Persona::Vector] {
            let mut h = harness(host("Pen", persona));
            h.get_by_label(mode).click();
            h.run_steps(2);
            let before = h.state().app.session.active().unwrap().doc.clone();
            let base = h.state().app.session.journal.len();
            if mode == "Line" {
                draw(&mut h, [30., 40.], [140., 90.], Modifiers::SHIFT);
            } else {
                for p in [[30., 40.], [80., 90.], [150., 40.]] {
                    draw(&mut h, p, p, Modifiers::NONE);
                }
                assert_eq!(h.state().app.session.active().unwrap().doc, before);
                assert_eq!(h.state().app.session.journal.len(), base);
                assert!(h.state().tools.preview().is_some());
                h.key_press(Key::Enter);
                h.step();
            }
            assert_eq!(h.state().app.session.journal.len(), base + 1, "{mode} {persona:?}");
            let st = h.state().app.session.active().unwrap();
            assert_eq!(st.doc.work_path.is_some(), persona == Persona::Pixel);
            paths.push(active_path(&h.state().app));
            assert!(h.state_mut().app.session.undo());
            assert_eq!(h.state().app.session.active().unwrap().doc, before);
        }
        assert_eq!(paths[0], paths[1], "{mode}");
    }
}
#[test]
fn shared47_all_live_shape_tools_drag_same_geometry_in_both_personas() {
    for entry in photocraft_engine::toolsets::VECTOR_TOOLS.iter().filter(|t| registry::handler(t.id) == Some(Handler::Shape)) {
        let mut paths = Vec::new();
        for persona in [Persona::Pixel, Persona::Vector] {
            let mut h = harness(host(entry.id, persona));
            let before = h.state().app.session.active().unwrap().doc.clone();
            let base = h.state().app.session.journal.len();
            point_event(&mut h, [50., 60.], Some(true), Modifiers::NONE);
            point_event(&mut h, [160., 140.], None, Modifiers::SHIFT);
            assert_eq!(h.state().app.session.active().unwrap().doc, before, "{}", entry.id);
            assert!(h.state().tools.shape_preview().is_some());
            let shown = h.state().tools.drawing.preview_layer.as_ref().unwrap();
            let LayerContent::Shape(sh) = &shown.content else { panic!("shape preview required") };
            let preview = sh.path.clone();
            point_event(&mut h, [160., 140.], Some(false), Modifiers::SHIFT);
            assert_eq!(h.state().app.session.journal.len(), base + 1, "{}", entry.id);
            let path = active_path(&h.state().app);
            assert_eq!(path, preview);
            paths.push(path);
            let st = h.state().app.session.active().unwrap();
            assert!(st.doc.layer(st.active_layer.unwrap()).unwrap().vector.live_shape.is_some());
            assert!(h.state_mut().app.session.undo());
            assert_eq!(h.state().app.session.active().unwrap().doc, before);
        }
        assert_eq!(paths[0], paths[1], "{}", entry.id);
    }
}
#[test]
fn shared47_pencil_drag_uses_same_fitter_and_one_undo_in_both_personas() {
    let mut paths = Vec::new();
    for persona in [Persona::Pixel, Persona::Vector] {
        let mut h = harness(host("VectorPencil", persona));
        let before = h.state().app.session.active().unwrap().doc.clone();
        let base = h.state().app.session.journal.len();
        point_event(&mut h, [30., 50.], Some(true), Modifiers::NONE);
        for p in [[60., 80.], [100., 100.], [140., 80.]] {
            point_event(&mut h, p, None, Modifiers::NONE);
        }
        assert_eq!(h.state().app.session.active().unwrap().doc, before);
        assert!(h.state().tools.pencil.preview().is_some());
        point_event(&mut h, [170., 50.], Some(false), Modifiers::NONE);
        assert_eq!(h.state().app.session.journal.len(), base + 1);
        paths.push(active_path(&h.state().app));
        assert!(h.state_mut().app.session.undo());
        assert_eq!(h.state().app.session.active().unwrap().doc, before);
    }
    assert_eq!(paths[0], paths[1]);
}
#[test]
fn shared47_node_drag_same_preview_and_commit_in_both_personas() {
    let mut paths = Vec::new();
    for persona in [Persona::Pixel, Persona::Vector] {
        let mut host = host("DirectSelection", persona);
        add_curve(&mut host.app);
        let mut h = harness(host);
        let before = h.state().app.session.active().unwrap().doc.clone();
        let base = h.state().app.session.journal.len();
        point_event(&mut h, [40., 40.], Some(true), Modifiers::NONE);
        point_event(&mut h, [60., 65.], None, Modifiers::NONE);
        assert_eq!(h.state().app.session.active().unwrap().doc, before);
        assert!(h.state().app.ui.direct_selection.preview_doc.is_some());
        point_event(&mut h, [60., 65.], Some(false), Modifiers::NONE);
        paths.push(active_path(&h.state().app));
        assert_eq!(h.state().app.session.journal.len(), base + 1);
        assert!(h.state_mut().app.session.undo());
        assert_eq!(h.state().app.session.active().unwrap().doc, before);
    }
    assert_eq!(paths[0], paths[1]);
}
#[test]
fn shared47_cancel_switch_stale_and_release_outside_canvas() {
    for persona in [Persona::Pixel, Persona::Vector] {
        let mut h = harness(host("Pen", persona));
        h.get_by_label("Line").click();
        h.run_steps(2);
        let before = h.state().app.session.active().unwrap().doc.clone();
        let base = h.state().app.session.journal.len();
        point_event(&mut h, [40., 40.], Some(true), Modifiers::NONE);
        point_event(&mut h, [90., 80.], None, Modifiers::NONE);
        h.state_mut().persona = if persona == Persona::Pixel { Persona::Vector } else { Persona::Pixel };
        h.step();
        point_event(&mut h, [100., 80.], Some(false), Modifiers::NONE);
        assert_eq!(h.state().app.session.active().unwrap().doc, before);
        assert_eq!(h.state().app.session.journal.len(), base);
        h.state_mut().persona = persona;
        h.step();
        point_event(&mut h, [40., 40.], Some(true), Modifiers::NONE);
        h.key_press(Key::Escape);
        h.step();
        assert!(h.state().tools.pen.gesture.is_none());
        point_event(&mut h, [100., 80.], Some(false), Modifiers::NONE);
        assert_eq!(h.state().app.session.journal.len(), base);
        point_event(&mut h, [40., 40.], Some(true), Modifiers::NONE);
        h.state_mut().app.run("layer.duplicate", json!({})).unwrap();
        h.step();
        let changed = h.state().app.session.active().unwrap().doc.clone();
        point_event(&mut h, [100., 80.], Some(false), Modifiers::NONE);
        assert_eq!(h.state().app.session.active().unwrap().doc, changed);
        point_event(&mut h, [40., 40.], Some(true), Modifiers::NONE);
        point_event(&mut h, [300., 80.], Some(false), Modifiers::NONE);
        assert!(h.state().tools.pen.gesture.is_none());
        assert_eq!(h.state().app.session.journal.len(), base + 2);
    }
}
#[test]
fn shared47_legacy_pen_and_shape_frontends_use_full_shared_cores() {
    for persona in [Persona::Pixel, Persona::Vector] {
        let mut app = host("Pen", persona).app;
        app.ui.tool_options.pen_options.mode = photocraft_engine::vector_packages::pen::Mode::Smart;
        for (x, y) in [(30., 40.), (80., 100.), (150., 40.)] {
            crate::vector_ui::pen_down(&mut app, x, y);
            crate::vector_ui::pen_up(&mut app);
        }
        crate::vector_ui::pen_commit(&mut app, false);
        assert!(active_path(&app).subpaths[0].knots.iter().all(|k| k.smooth));
        crate::vector_ui::finish_shape(&mut app, crate::Tool::Rectangle, [30., 30.], [100., 100.], Modifiers::NONE);
        let st = app.session.active().unwrap();
        assert!(st.doc.layer(st.active_layer.unwrap()).unwrap().vector.live_shape.is_some());
        let mut old_json = serde_json::to_value(crate::state::UiState::default()).unwrap();
        old_json["pen"] = json!({"knots":[[[1,2],[1,2],[1,2]],[[5,6],[5,6],[5,6]]]});
        old_json["tool_options"].as_object_mut().unwrap().remove("pen_options");
        let old: crate::state::UiState = serde_json::from_value(old_json).unwrap();
        assert_eq!(old.tool_options.pen_options, pen::Options::default());
        assert_eq!(old.pen.unwrap().knots.len(), 2);
    }
}
#[test]
fn shared47_pen_snapping_and_current_shift_share_cached_query() {
    for persona in [Persona::Pixel, Persona::Vector] {
        for automatic_angles in [true, false] {
            let mut h = harness(host("Pen", persona));
            h.get_by_label("Line").click();
            h.run_steps(2);
            {
                let s = &mut h.state_mut().tools.snapping;
                s.enabled = true;
                s.nodes = false;
                s.bounds = false;
                s.document = false;
                s.alignment = false;
                s.grid = true;
                s.grid_step = 20.;
                s.angle = automatic_angles;
            }
            point_event(&mut h, [41., 41.], Some(true), Modifiers::NONE);
            point_event(&mut h, [99., 99.], Some(false), Modifiers::SHIFT);
            let path = active_path(&h.state().app);
            assert_eq!(path.subpaths[0].knots[0].anchor, Point::new(40., 40.));
            let end = path.subpaths[0].knots[1].anchor;
            assert!((end.x - 100.).abs() < 1e-9 && (end.y - 100.).abs() < 1e-9, "{end:?}");
        }
    }
}
#[test]
fn shared47_corner_drag_same_live_metadata_in_both_personas() {
    let mut metadata = Vec::new();
    for persona in [Persona::Pixel, Persona::Vector] {
        let mut host = host("Corner", persona);
        let id = add_curve(&mut host.app);
        host.tools.corners.layer = Some(id);
        host.tools.corners.active = true;
        let widgets = host.tools.corners.widgets(&host.app.session, 1.);
        let (handle, start) = widgets[0];
        // Drag the first corner along its own bisector, chosen by the real corner provider.
        let end = handle.widget(20., 1.);
        let mut h = harness(host);
        let base = h.state().app.session.journal.len();
        let before = h.state().app.session.active().unwrap().doc.clone();
        point_event(&mut h, [start.x as f32, start.y as f32], Some(true), Modifiers::NONE);
        point_event(&mut h, [end.x as f32, end.y as f32], None, Modifiers::NONE);
        assert!(h.state().tools.corners.is_dragging());
        assert_eq!(h.state().app.session.active().unwrap().doc, before);
        point_event(&mut h, [end.x as f32, end.y as f32], Some(false), Modifiers::NONE);
        let st = h.state().app.session.active().unwrap();
        let corners = st.doc.layer(id).unwrap().vector.corners.clone();
        assert!(!corners.is_empty());
        metadata.push(corners);
        assert_eq!(h.state().app.session.journal.len(), base + 1);
        assert!(h.state_mut().app.session.undo());
        assert_eq!(h.state().app.session.active().unwrap().doc, before);
    }
    assert_eq!(metadata[0], metadata[1]);
}
#[test]
fn shared47_fill_and_transparency_drag_same_paint_in_both_personas() {
    for id in ["VectorFill", "Transparency"] {
        let mut appearances = Vec::new();
        for persona in [Persona::Pixel, Persona::Vector] {
            let mut host = host(id, persona);
            let layer = add_curve(&mut host.app);
            let mut h = harness(host);
            let base = h.state().app.session.journal.len();
            let before = h.state().app.session.active().unwrap().doc.clone();
            point_event(&mut h, [60., 70.], Some(true), Modifiers::NONE);
            point_event(&mut h, [130., 110.], None, Modifiers::NONE);
            assert!(h.state().tools.paint.preview.is_some());
            assert_eq!(h.state().app.session.active().unwrap().doc, before);
            point_event(&mut h, [130., 110.], Some(false), Modifiers::NONE);
            let st = h.state().app.session.active().unwrap();
            appearances.push(st.doc.layer(layer).unwrap().vector.appearance.clone());
            assert!(appearances.last().unwrap().is_some());
            assert_eq!(h.state().app.session.journal.len(), base + 1);
            assert!(h.state_mut().app.session.undo());
            assert_eq!(h.state().app.session.active().unwrap().doc, before);
        }
        assert_eq!(appearances[0], appearances[1], "{id}");
    }
}
fn app_harness(tool: crate::Tool, persona: Persona) -> Harness<'static, PhotocraftApp> {
    let mut app = host("Move", persona).app;
    app.ui.tool = tool;
    if matches!(tool, crate::Tool::Move | crate::Tool::PathSelection | crate::Tool::Eyedropper) {
        add_curve(&mut app);
    }
    let mut h = Harness::builder().with_size(egui::vec2(1000., 700.)).build_ui_state(
        |ui, app: &mut PhotocraftApp| {
            if !ui.ctx().fonts(|f| f.families().contains(&egui::FontFamily::Name("medium".into()))) {
                return;
            }
            let ctx = ui.ctx().clone();
            crate::vector_integration::frame(app, &ctx);
            crate::shortcuts::handle(app, &ctx);
            egui::CentralPanel::default().show(ui, |ui| crate::canvas::document_area(app, ui));
        },
        app,
    );
    PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::Pro);
    h.run_steps(4);
    h
}
fn app_point(h: &mut Harness<'_, PhotocraftApp>, p: [f32; 2], pressed: Option<bool>) {
    let xf = ViewXform::active(h.state()).unwrap();
    let pos = xf.to_screen(p[0], p[1]);
    h.event(egui::Event::PointerMoved(pos));
    if let Some(pressed) = pressed {
        h.event(egui::Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
    }
    h.step();
    assert!(!h.state().ui.status_error, "{}", h.state().ui.status);
}
#[test]
fn shared47_existing_move_path_crop_sampler_and_type_actual_canvas_in_both_personas() {
    for tool in [
        crate::Tool::Move,
        crate::Tool::PathSelection,
        crate::Tool::CustomShape,
        crate::Tool::Crop,
        crate::Tool::Eyedropper,
        crate::Tool::Type,
        crate::Tool::VerticalType,
    ] {
        let mut results = Vec::new();
        for persona in [Persona::Pixel, Persona::Vector] {
            let mut h = app_harness(tool, persona);
            let base = h.state().session.journal.len();
            app_point(&mut h, [70., 70.], Some(true));
            app_point(&mut h, [100., 100.], None);
            app_point(&mut h, [100., 100.], Some(false));
            let result = match tool {
                crate::Tool::Move | crate::Tool::PathSelection | crate::Tool::CustomShape => {
                    assert_eq!(h.state().session.journal.len(), base + 1);
                    json!(active_path(h.state()))
                }
                crate::Tool::Crop => {
                    assert!(h.state().ui.crop_rect.is_some());
                    assert_eq!(h.state().session.journal.len(), base);
                    json!(h.state().ui.crop_rect)
                }
                crate::Tool::Eyedropper => {
                    assert_ne!(h.state().session.tools.foreground, [0., 0., 0., 1.]);
                    json!(h.state().session.tools.foreground)
                }
                crate::Tool::Type | crate::Tool::VerticalType => {
                    let st = h.state().session.active().unwrap();
                    let LayerContent::Text(t) = &st.doc.layer(st.active_layer.unwrap()).unwrap().content else { panic!("type layer required") };
                    json!((t.shape, t.text.clone(), t.transform))
                }
                _ => panic!("unexpected tool"),
            };
            results.push(result);
        }
        assert_eq!(results[0], results[1], "{tool:?}");
    }
}
#[test]
fn shared47_hand_and_zoom_actual_canvas_in_both_personas() {
    for tool in [crate::Tool::Hand, crate::Tool::Zoom] {
        let mut results = Vec::new();
        for persona in [Persona::Pixel, Persona::Vector] {
            let mut h = app_harness(tool, persona);
            let before = h.state().ui.views[0].clone();
            let doc = h.state().session.active().unwrap().doc.clone();
            let pos = h.state().last_canvas_rect.center();
            h.event(egui::Event::PointerButton { pos, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
            h.step();
            if tool == crate::Tool::Hand {
                h.event(egui::Event::PointerMoved(pos + egui::vec2(35., 25.)));
                h.step();
            }
            h.event(egui::Event::PointerButton {
                pos: if tool == crate::Tool::Hand { pos + egui::vec2(35., 25.) } else { pos },
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            });
            h.step();
            let after = h.state().ui.views[0].clone();
            assert!(after.center != before.center || after.zoom != before.zoom, "{tool:?}");
            assert_eq!(h.state().session.active().unwrap().doc, doc);
            results.push((after.center, after.zoom));
        }
        assert_eq!(results[0], results[1]);
    }
}
#[test]
fn shared47_path_text_click_type_and_commit_in_both_personas() {
    let mut results = Vec::new();
    for persona in [Persona::Pixel, Persona::Vector] {
        let mut host = host("TextOnPath", persona);
        add_curve(&mut host.app);
        let mut h = harness(host);
        let before = h.state().app.session.active().unwrap().doc.clone();
        let base = h.state().app.session.journal.len();
        draw(&mut h, [70., 40.], [70., 40.], Modifiers::NONE);
        let empty_key = h.state().tools.preview().unwrap().1;
        h.event(egui::Event::Text("Shared".into()));
        h.step();
        assert!(h.state().tools.path_text.preview.is_some());
        let typed_key = h.state().tools.preview().unwrap().1;
        assert_ne!(typed_key, empty_key, "typing must invalidate the GPU transient key");
        h.step();
        assert_eq!(h.state().tools.preview().unwrap().1, typed_key, "idle preview stays cached");
        assert_eq!(h.state().app.session.active().unwrap().doc, before);
        h.key_press(Key::Enter);
        h.step();
        assert_eq!(h.state().app.session.journal.len(), base + 1);
        let st = h.state().app.session.active().unwrap();
        let layer = st.doc.layer(st.active_layer.unwrap()).unwrap();
        let LayerContent::Text(text) = &layer.content else { panic!("text required") };
        assert!(layer.vector.on_path.is_some());
        assert_eq!(text.text, "Shared");
        results.push((text.text.clone(), text.transform));
        assert!(h.state_mut().app.session.undo());
        assert_eq!(h.state().app.session.active().unwrap().doc, before);
    }
    assert_eq!(results[0], results[1]);
}
#[test]
fn shared47_vector_crop_and_point_transform_drag_in_both_personas() {
    for id in ["VectorCrop", "PointTransform"] {
        let mut results = Vec::new();
        for persona in [Persona::Pixel, Persona::Vector] {
            let mut host = host(id, persona);
            let layer = add_curve(&mut host.app);
            host.tools.layout.snapping.enabled = false;
            let mut h = harness(host);
            let base = h.state().app.session.journal.len();
            let before = h.state().app.session.active().unwrap().doc.clone();
            draw(&mut h, [40., 40.], [100., 100.], Modifiers::NONE);
            assert_eq!(h.state().app.session.journal.len(), base + 1, "{id}");
            let st = h.state().app.session.active().unwrap();
            let changed = st.doc.layer(layer).unwrap();
            if id == "VectorCrop" {
                assert!(changed.vector.clip.is_some());
                results.push(json!(changed.vector.clip));
            } else {
                results.push(json!(active_path(&h.state().app)));
            }
            assert!(h.state_mut().app.session.undo());
            assert_eq!(h.state().app.session.active().unwrap().doc, before);
        }
        assert_eq!(results[0], results[1], "{id}");
    }
}

#[test]
fn shared47_art_and_frame_text_defaults_share_creator_in_both_personas() {
    for id in ["ArtText", "FrameText"] {
        let mut results = Vec::new();
        for persona in [Persona::Pixel, Persona::Vector] {
            let mut h = harness(host(id, persona));
            draw(&mut h, [40., 40.], [130., 110.], Modifiers::NONE);
            let st = h.state().app.session.active().unwrap();
            let LayerContent::Text(text) = &st.doc.layer(st.active_layer.unwrap()).unwrap().content else { panic!("text required") };
            results.push(json!((text.shape, text.transform)));
            assert_eq!(matches!(text.shape, photocraft_doc::text::TextShape::Point), id == "ArtText");
        }
        assert_eq!(results[0], results[1]);
    }
}
#[test]
fn shared47_shape_snap_failure_cancels_without_edit_in_both_personas() {
    for persona in [Persona::Pixel, Persona::Vector] {
        let mut h = harness(host("Rectangle", persona));
        let before = h.state().app.session.active().unwrap().doc.clone();
        let base = h.state().app.session.journal.len();
        point_event(&mut h, [40., 40.], Some(true), Modifiers::NONE);
        point_event(&mut h, [120., 100.], None, Modifiers::NONE);
        assert!(h.state().tools.shape_preview().is_some());
        h.state_mut().tools.snapping.grid_step = 0.;
        let pos = h.state().rect.min + egui::vec2(130., 110.);
        h.event(egui::Event::PointerButton { pos, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
        h.step();
        assert!(h.state().app.ui.status_error);
        assert!(h.state().tools.shape_preview().is_none());
        assert_eq!(h.state().app.session.active().unwrap().doc, before);
        assert_eq!(h.state().app.session.journal.len(), base);
    }
}

#[test]
fn shared47_node_persona_switch_cancels_existing_capture_in_both_personas() {
    for persona in [Persona::Pixel, Persona::Vector] {
        let mut host = host("DirectSelection", persona);
        add_curve(&mut host.app);
        let mut h = harness(host);
        let before = h.state().app.session.active().unwrap().doc.clone();
        let base = h.state().app.session.journal.len();
        point_event(&mut h, [40., 40.], Some(true), Modifiers::NONE);
        point_event(&mut h, [60., 65.], None, Modifiers::NONE);
        assert!(h.state().app.ui.direct_selection.preview_doc.is_some());
        h.state_mut().persona = if persona == Persona::Pixel { Persona::Vector } else { Persona::Pixel };
        h.step();
        assert!(h.state().app.ui.direct_selection.preview_doc.is_none());
        point_event(&mut h, [60., 65.], Some(false), Modifiers::NONE);
        assert_eq!(h.state().app.session.active().unwrap().doc, before);
        assert_eq!(h.state().app.session.journal.len(), base);
    }
}

#[test]
fn shared47_flipped_shape_and_physical_snap_tolerance_use_host_transform() {
    for persona in [Persona::Pixel, Persona::Vector] {
        for flip in [false, true] {
            let mut host = host("Rectangle", persona);
            host.flip = flip;
            let s = &mut host.tools.snapping;
            s.enabled = true;
            s.nodes = false;
            s.bounds = false;
            s.document = false;
            s.alignment = false;
            s.grid = true;
            s.grid_step = 20.;
            let mut h = harness(host);
            h.ctx.set_pixels_per_point(2.);
            h.run_steps(2);
            point_event(&mut h, [47., 47.], Some(true), Modifiers::NONE);
            point_event(&mut h, [107., 103.], None, Modifiers::NONE);
            assert!(!h.state().tools.snap_result.as_ref().unwrap().guides.is_empty());
            assert_eq!(h.state().tools.pixels_per_point, Some(2.));
            point_event(&mut h, [107., 103.], Some(false), Modifiers::NONE);
            let st = h.state().app.session.active().unwrap();
            let design = st.doc.layer(st.active_layer.unwrap()).unwrap().vector.live_shape.as_ref().unwrap();
            assert_eq!(design.rect, [47., 47., 60., 53.]);
        }
    }
}

#[test]
fn shared47_stroke_only_tools_share_foreground_paint_defaults_in_both_personas() {
    for id in ["Line", "Arc", "Spiral", "RectGrid", "PolarGrid", "Pen"] {
        for persona in [Persona::Pixel, Persona::Vector] {
            let mut h = harness(host(id, persona));
            if id == "Pen" {
                h.get_by_label("Line").click();
                h.run_steps(2);
                h.state_mut().tools.shape_mode = true;
            }
            let fg = h.state().app.session.tools.foreground;
            let base = h.state().app.session.journal.len();
            draw(&mut h, [40., 40.], [140., 110.], Modifiers::NONE);
            let st = h.state().app.session.active().unwrap();
            let LayerContent::Shape(shape) = &st.doc.layer(st.active_layer.unwrap()).unwrap().content else { panic!("shape required") };
            assert!(shape.fill.is_none(), "{id}");
            let stroke = shape.stroke.as_ref().unwrap();
            assert_eq!(stroke.paint, Fill::Solid(Color::rgba(fg[0], fg[1], fg[2], fg[3])), "{id}");
            assert!(stroke.width > 0., "{id}");
            assert_eq!(h.state().app.session.journal.len(), base + 1);
        }
    }
}
#[test]
fn bugfix_vector_pen_keeps_draft_on_focus_loss_and_finishes_escape() {
    for persona in [Persona::Pixel, Persona::Vector] {
        let mut h = harness(host("Pen", persona));
        for p in [[40., 40.], [80., 80.], [120., 40.]] {
            point_event(&mut h, p, Some(true), Modifiers::NONE);
            point_event(&mut h, p, Some(false), Modifiers::NONE);
        }
        let base = h.state().app.session.journal.len();
        h.event(egui::Event::WindowFocused(false));
        h.step();
        assert_eq!(h.state().tools.pen.gesture.as_ref().unwrap().path().subpaths[0].knots.len(), 3);
        h.event(egui::Event::WindowFocused(true));
        h.step();
        h.key_press(Key::Backspace);
        h.step();
        assert_eq!(h.state().tools.pen.gesture.as_ref().unwrap().path().subpaths[0].knots.len(), 2);
        assert_eq!(h.state().app.session.journal.len(), base);
        h.key_press(Key::Escape);
        h.step();
        assert!(h.state().tools.pen.gesture.is_none());
        assert_eq!(active_path(&h.state().app).subpaths[0].knots.len(), 2);
        assert_eq!(h.state().app.session.journal.len(), base + 1);
    }
}
#[test]
fn bugfix_vector_path_text_survives_pointer_leaving_and_escape_keeps_text() {
    let mut host = host("TextOnPath", Persona::Vector);
    add_curve(&mut host.app);
    let mut h = harness(host);
    draw(&mut h, [70., 40.], [70., 40.], Modifiers::NONE);
    h.event(egui::Event::Text("Keep me".into()));
    h.step();
    h.event(egui::Event::PointerGone);
    h.step();
    assert!(h.state().tools.path_text.draft.is_some());
    h.key_press(Key::Escape);
    h.step();
    let s = h.state().app.session.active().unwrap();
    let LayerContent::Text(text) = &s.doc.layer(s.active_layer.unwrap()).unwrap().content else { panic!("text required") };
    assert_eq!(text.text, "Keep me");
}

#[test]
fn hidpi_pen_closure_keeps_six_logical_points_in_both_personas() {
    for persona in [Persona::Pixel, Persona::Vector] {
        for ppp in [1.0, 1.25, 2.0] {
            for (offset, closes) in [(5.0, true), (7.0, false)] {
                let mut h = harness_at_scale(host("Pen", persona), ppp);
                h.run_steps(3);
                let before = h.state().app.session.active().unwrap().doc.clone();
                let journal = h.state().app.session.journal.len();
                for point in [[30., 40.], [100., 40.], [100., 110.], [30. - offset, 40.]] {
                    draw(&mut h, point, point, Modifiers::NONE);
                }
                if closes {
                    assert!(h.state().tools.pen.gesture.is_none(), "five-point closure at {ppp}x/{persona:?}");
                    let path = active_path(&h.state().app);
                    assert!(path.subpaths[0].closed);
                    assert_eq!(path.subpaths[0].knots.len(), 3);
                    assert_eq!(h.state().app.session.journal.len(), journal + 1);
                    assert!(h.state_mut().app.session.undo());
                } else {
                    assert!(h.state().tools.pen.gesture.is_some(), "seven-point miss at {ppp}x/{persona:?}");
                    assert_eq!(h.state().app.session.journal.len(), journal);
                }
                assert!(h.state().app.session.active().unwrap().doc == before, "document changed at {ppp}x/{persona:?}, closes={closes}");
            }
        }
    }
}
