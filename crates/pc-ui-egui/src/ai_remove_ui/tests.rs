use super::*;
use egui::{Event, PointerButton, pos2};
use egui_kittest::{Harness, kittest::Queryable};

fn harness() -> Harness<'static, PhotocraftApp> {
    crate::ai_ui::TEST_STATUS.with(|s| {
        *s.borrow_mut() = Some(crate::ai_ui::EngineStatus {
            checked: true,
            connected: true,
            presets: [("flux2-klein-remove:bf16".into(), Ok(()))].into(),
            ..Default::default()
        })
    });
    // Use the actual catalogue id rather than relying on the display name.
    crate::ai_ui::TEST_STATUS.with(|s| s.borrow_mut().as_mut().unwrap().presets.insert(format!("{}:bf16", li_ai::ModelId::KleinRemove.key()), Ok(())));
    let mut s = photocraft_engine::Session::new();
    s.execute("file.new", json!({"width": 320, "height": 240})).unwrap();
    let mut h = Harness::builder().with_size(vec2(1280.0, 800.0)).with_max_steps(64).build_eframe(move |cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        let mut app = PhotocraftApp::new(s, crate::Services::default());
        app.ui.tool = Tool::AiRemove;
        app.session.tools.brush.size = 24.0;
        app.session.tools.brush.hardness = 1.0;
        app
    });
    h.state_mut().background_jobs = false;
    h.run_steps(6);
    h
}

fn stroke(h: &mut Harness<'_, PhotocraftApp>, x: f32, y: f32, mods: Modifiers) {
    h.event(Event::ModifiersChanged(mods));
    let xf = ViewXform::active(h.state()).unwrap();
    let a = xf.to_screen(x, y);
    let b = xf.to_screen(x + 35.0, y);
    h.hover_at(a);
    h.step();
    h.event(Event::PointerButton { pos: a, button: PointerButton::Primary, pressed: true, modifiers: mods });
    h.step();
    h.event(Event::PointerMoved(b));
    h.step();
    h.event(Event::PointerButton { pos: b, button: PointerButton::Primary, pressed: false, modifiers: mods });
    h.run_steps(3);
    h.event(Event::ModifiersChanged(Modifiers::NONE));
}

#[test]
fn remove_confirmation_strokes_merge_subtract_and_escape_without_edits() {
    let mut h = harness();
    let before = h.state().session.active().unwrap().doc.clone();
    let history = h.state().session.active().unwrap().history.past_len();
    stroke(&mut h, 60.0, 70.0, Modifiers::NONE);
    assert!(h.state().ai_remove.is_some());
    assert_eq!(h.get_all_by_label("Remove").count(), 2, "floating and options bars");
    stroke(&mut h, 190.0, 160.0, Modifiers::NONE);
    let p = h.state().ai_remove.as_ref().unwrap();
    assert!(p.mask[70 * 320 + 70] > 0 && p.mask[160 * 320 + 200] > 0);
    stroke(&mut h, 60.0, 70.0, Modifiers::ALT);
    let p = h.state().ai_remove.as_ref().unwrap();
    assert_eq!(p.mask[70 * 320 + 70], 0);
    assert!(p.mask[160 * 320 + 200] > 0);
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert_eq!(h.state().session.active().unwrap().history.past_len(), history);
    assert!(crate::ai_ui::ai_jobs(h.state()).is_empty());
    h.key_press(Key::Escape);
    h.run_steps(2);
    assert!(h.state().ai_remove.is_none());
    stroke(&mut h, 100.0, 100.0, Modifiers::NONE);
    h.state_mut().ui.tool = Tool::Move;
    h.run_steps(2);
    assert!(h.state().ai_remove.is_none());
}

#[test]
fn remove_confirmation_selection_clear_cancel_keep_painting_and_document_switch() {
    let mut h = harness();
    h.state_mut().run("select.rect", json!({"x": 40, "y": 40, "width": 50, "height": 30})).unwrap();
    h.run_steps(2);
    h.get_by_label("Remove Selection").click();
    h.run_steps(3);
    assert_eq!(h.state().ai_remove.as_ref().unwrap().mask[50 * 320 + 50], 255);
    let button = h.get_all_by_label("Keep painting").next().unwrap();
    button.click();
    h.run_steps(2);
    assert!(h.state().ai_remove.is_some());
    assert!(!h.state().ai_remove.as_ref().unwrap().show_bar, "Keep painting hides the floating bar");
    h.get_all_by_label("Clear").next().unwrap().click();
    h.run_steps(2);
    assert!(h.state().ai_remove.is_none());
    stroke(&mut h, 200.0, 190.0, Modifiers::NONE);
    h.get_all_by_label("Cancel").next().unwrap().click();
    h.run_steps(2);
    assert!(h.state().ai_remove.is_none());
    stroke(&mut h, 200.0, 190.0, Modifiers::NONE);
    h.state_mut().run("file.new", json!({"width": 40, "height": 40})).unwrap();
    h.run_steps(3);
    assert!(h.state().ai_remove.is_none());
}

#[test]
fn remove_confirmation_http_command_repairs_mask_in_one_undo_step() {
    let _lock = crate::ai_ui::AI_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let server = match li_ai::mock::MockComfy::start() {
        Ok(s) => s,
        Err(e)
            if e.kind() == std::io::ErrorKind::PermissionDenied
                || e.get_ref().and_then(|e| e.downcast_ref::<std::io::Error>()).is_some_and(|e| e.kind() == std::io::ErrorKind::PermissionDenied) =>
        {
            eprintln!("skipped: sandbox denies mock sockets");
            return;
        }
        Err(e) => panic!("{e}"),
    };
    struct ResetService;
    impl Drop for ResetService {
        fn drop(&mut self) {
            li_ai::set_service_override(None);
        }
    }
    let _reset = ResetService;
    li_ai::set_service_override(Some(server.host().into()));
    let mut h = harness();
    for action in 0..3 {
        let history = h.state().session.active().unwrap().history.past_len();
        if action == 2 {
            h.get_by_label("Remove immediately on release").click();
            h.run_steps(2);
        }
        stroke(&mut h, 70.0, 80.0, Modifiers::NONE);
        if action != 2 {
            assert_eq!(server.prompts().len(), action);
            if action == 0 {
                h.key_press(Key::Enter);
            } else {
                h.get_all_by_label("Remove").next().unwrap().click();
            }
            h.run_steps(3);
        }
        assert_eq!(server.prompts().len(), action + 1);
        assert!(h.state().ai_remove.is_none());
        assert_eq!(h.state().session.active().unwrap().history.past_len(), history + 1);
        let d = h.state().session.active().unwrap();
        let layer = d.doc.layer(d.active_layer.unwrap()).unwrap();
        let photocraft_doc::LayerContent::Raster(s) = &layer.content else { panic!("raster repair") };
        assert!(s.sample_channel(80, 80, 3) > 0.0);
        h.state_mut().run("edit.undo", json!({})).unwrap();
        h.hover_at(pos2(20.0, 200.0));
        h.run_steps(2);
    }
    li_ai::set_service_override(None);
}

#[test]
fn remove_confirmation_enter_click_and_immediate_submit_the_painted_mask_once() {
    let calls = std::rc::Rc::new(std::cell::RefCell::new(Vec::<Vec<u8>>::new()));
    let seen = calls.clone();
    TEST_REMOVE.with(|m| {
        *m.borrow_mut() = Some(Box::new(move |app, params| {
            let mask: Vec<u8> = serde_json::from_value(params["mask"].clone()).unwrap();
            seen.borrow_mut().push(mask.clone());
            app.session
                .edit("AI Remove", move |doc, active| {
                    let image =
                        image::RgbaImage::from_fn(doc.size.width, doc.size.height, |x, y| image::Rgba([20, 30, 40, mask[(y * doc.size.width + x) as usize]]));
                    let layer = photocraft_engine::ai_cmds::layer_from_rgba(doc, "AI Remove", &image, 0, 0)?;
                    let id = doc.insert_above(*active, layer);
                    *active = Some(id);
                    Ok(json!({"layer":id.0}))
                })
                .map_err(|e| e.to_string())
        }))
    });
    let mut h = harness();
    for action in 0..3 {
        let history = h.state().session.active().unwrap().history.past_len();
        if action == 2 {
            h.get_by_label("Remove immediately on release").click();
            h.run_steps(2);
        }
        stroke(&mut h, 70.0, 80.0, Modifiers::NONE);
        if action != 2 {
            assert_eq!(calls.borrow().len(), action);
            let expected = h.state().ai_remove.as_ref().unwrap().mask.clone();
            if action == 0 {
                h.key_press(Key::Enter);
            } else {
                h.get_all_by_label("Remove").next().unwrap().click();
            }
            h.run_steps(3);
            assert_eq!(calls.borrow()[action], expected);
        }
        assert_eq!(calls.borrow().len(), action + 1);
        assert!(calls.borrow()[action][80 * 320 + 80] > 0);
        assert_eq!(calls.borrow()[action][10 * 320 + 10], 0);
        assert!(h.state().ai_remove.is_none());
        assert_eq!(h.state().session.active().unwrap().history.past_len(), history + 1);
        h.key_press(Key::Enter);
        h.run_steps(2);
        assert_eq!(calls.borrow().len(), action + 1, "never submit twice");
        h.state_mut().run("edit.undo", json!({})).unwrap();
    }
    TEST_REMOVE.with(|m| *m.borrow_mut() = None);
}

#[test]
fn ds_finding_remove_bounds_and_partial_texture_follow_actual_strokes() {
    let mut h = harness();
    stroke(&mut h, 60., 70., Modifiers::NONE);
    let id = h.state().ai_remove.as_ref().unwrap().texture.as_ref().unwrap().id();
    paint(h.state_mut(), &[[190., 160., 1.], [215., 160., 1.]], false);
    let xf = ViewXform::active(h.state()).unwrap();
    let painter = h.ctx.layer_painter(egui::LayerId::background());
    overlay(h.state_mut(), &painter, &xf);
    // The kittest renderer consumes deltas before exposing FullOutput. Inspect the
    // texture manager immediately after the same overlay path used by the canvas.
    let mut updates = h.ctx.tex_manager().write().take_delta();
    let delta = updates.set.iter().find(|(texture, _)| **texture == id).unwrap();
    let delta = delta.1.last().unwrap();
    assert!(delta.pos.is_some(), "a dab updates only its rectangle");
    let size = delta.image.size();
    assert!(size[0] < 100 && size[1] < 100);
    updates.clear();
    for subtract in [true, false, true] {
        stroke(&mut h, 60., 70., if subtract { Modifiers::ALT } else { Modifiers::NONE });
        let p = h.state().ai_remove.as_ref().unwrap();
        assert_eq!(p.bounds, scan_bounds(p, Rect::from_xywh(0, 0, 320, 240)));
    }
    h.get_by_label("Remove immediately on release").click();
    h.run_steps(2);
    TEST_REMOVE.with(|m| *m.borrow_mut() = Some(Box::new(|_, _| panic!("unchanged paint must not submit removal"))));
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            TEST_REMOVE.with(|m| *m.borrow_mut() = None);
        }
    }
    let _reset = Reset;
    paint(h.state_mut(), &[[-100., -100., 1.]], false);
    assert!(h.state().ai_remove.is_some());
}

#[test]
fn restoration_detected_mask_fill_once_then_optional_colourise_preview() {
    let server = match li_ai::mock::MockComfy::start() {
        Ok(s) => s,
        Err(e)
            if e.kind() == std::io::ErrorKind::PermissionDenied
                || e.get_ref().and_then(|e| e.downcast_ref::<std::io::Error>()).is_some_and(|e| e.kind() == std::io::ErrorKind::PermissionDenied) =>
        {
            eprintln!("skipped: sandbox denies mock sockets");
            return;
        }
        Err(e) => panic!("{e}"),
    };
    let _lock = crate::ai_ui::AI_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    li_ai::set_service_override(Some(server.host().into()));
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            li_ai::set_service_override(None);
        }
    }
    let _reset = Reset;
    let mut h = harness();
    h.state_mut().ui.ai.remove_immediately = true;
    let before = h.state().session.active().unwrap().doc.clone();
    let history = h.state().session.active().unwrap().history.past_len();
    let mask = image::GrayImage::from_fn(320, 240, |x, y| image::Luma([if x > 90 && x < 96 && y > 60 && y < 180 { 255 } else { 0 }]));
    detected(h.state_mut(), mask.clone(), true).unwrap();
    h.run_steps(4);
    assert_eq!(server.prompts().len(), 0, "detection cannot submit an immediate fill");
    assert_eq!(h.state().session.active().unwrap().doc, before);
    // The imported mask is adjustable with the same brush before confirmation.
    paint(h.state_mut(), &[[92., 80., 1.]], true);
    assert_eq!(h.state().ai_remove.as_ref().unwrap().mask[80 * 320 + 92], 0);
    h.run_steps(2);
    h.get_all_by_label("Remove").next().unwrap().click();
    h.run_steps(3);
    assert_eq!(server.prompts().len(), 1);
    assert!(h.state().ai_remove.is_none());
    assert_eq!(h.state().session.active().unwrap().history.past_len(), history + 1);
    assert!(h.query_by_label("Restore Photo").is_some(), "optional Colourise waits for successful fill");
    h.state_mut().run("edit.undo", json!({})).unwrap();
    assert_eq!(h.state().session.active().unwrap().doc, before);
}
#[test]
fn bugfix_vector_remove_undo_keeps_document_and_removes_last_stroke() {
    let mut h = harness();
    let before = h.state().session.active().unwrap().doc.clone();
    let history = h.state().session.active().unwrap().history.past_len();
    stroke(&mut h, 60., 70., Modifiers::NONE);
    let mask = h.state().ai_remove.as_ref().unwrap().mask.clone();
    stroke(&mut h, 190., 160., Modifiers::NONE);
    h.event(Event::Key { key: Key::Z, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::COMMAND });
    h.run_steps(3);
    assert_eq!(h.state().ai_remove.as_ref().unwrap().mask, mask);
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert_eq!(h.state().session.active().unwrap().history.past_len(), history);
}
