use super::*;
use egui::{Event, PointerButton, pos2};
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};

fn harness() -> Harness<'static, PhotocraftApp> {
    harness_mode(false)
}

fn harness_mode(immediate: bool) -> Harness<'static, PhotocraftApp> {
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
        app.ui.ai.remove_immediately = immediate;
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
    let painted = h.state().ai_remove.as_ref().unwrap().mask.clone();
    h.state_mut().ui.tool = Tool::Move;
    h.run_steps(2);
    // U030: switching tools keeps the painted mask dormant instead of discarding it.
    assert_eq!(h.state().ai_remove.as_ref().unwrap().mask, painted);
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert_eq!(h.state().session.active().unwrap().history.past_len(), history);
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
    let before_clear = h.state().ai_remove.as_ref().unwrap().mask.clone();
    let doc = h.state().session.active().unwrap().doc.clone();
    let history = h.state().session.active().unwrap().history.past_len();
    h.get_all_by_label("Clear").next().unwrap().click();
    h.run_steps(2);
    assert!(h.state().ai_remove.as_ref().unwrap().mask.iter().all(|v| *v == 0));
    assert!(h.state().ai_remove.as_ref().unwrap().bounds.is_empty());
    assert_eq!(h.state().ui.tool, Tool::AiRemove);
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(2);
    assert_eq!(h.state().ai_remove.as_ref().unwrap().mask, before_clear);
    h.get_all_by_label("Clear").next().unwrap().click();
    h.run_steps(2);
    stroke(&mut h, 200.0, 190.0, Modifiers::NONE);
    h.get_all_by_label("Cancel").next().unwrap().click();
    h.run_steps(2);
    assert!(h.state().ai_remove.is_none());
    assert_eq!(h.state().ui.tool, Tool::AiRemove, "Cancel ends the removal but keeps the Remove tool");
    assert_eq!(h.state().session.active().unwrap().doc, doc);
    assert_eq!(h.state().session.active().unwrap().history.past_len(), history);
    h.state_mut().ui.tool = Tool::AiRemove;
    stroke(&mut h, 200.0, 190.0, Modifiers::NONE);
    let captured = h.state().ai_remove.as_ref().unwrap().doc;
    h.state_mut().run("file.new", json!({"width": 40, "height": 40})).unwrap();
    h.run_steps(3);
    assert_eq!(h.state().ai_remove.as_ref().unwrap().doc, captured);
    assert!(!target_active(h.state(), h.state().ai_remove.as_ref().unwrap()));
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

#[test]
fn retained_remove_mask_keys_are_dormant_in_rotate_and_resume_in_remove() {
    let calls = std::rc::Rc::new(std::cell::RefCell::new(Vec::<Vec<u8>>::new()));
    let seen = calls.clone();
    TEST_REMOVE.with(|mock| {
        *mock.borrow_mut() = Some(Box::new(move |_, params| {
            seen.borrow_mut().push(serde_json::from_value(params["mask"].clone()).unwrap());
            Ok(json!({}))
        }))
    });
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            TEST_REMOVE.with(|mock| *mock.borrow_mut() = None);
            crate::ai_ui::TEST_STATUS.with(|s| *s.borrow_mut() = None);
        }
    }
    let _reset = Reset;
    let mut h = harness();
    h.state_mut().ui.ai.remove_immediately = false;
    let before_fill = h.state().session.active().unwrap().doc.clone();
    h.state_mut().run("edit.fill", json!({"color":"#00ff00"})).unwrap();
    let filled = h.state().session.active().unwrap().doc.clone();
    let undo = h.state().session.active().unwrap().history.past_len();
    stroke(&mut h, 60.0, 70.0, Modifiers::NONE);
    stroke(&mut h, 190.0, 160.0, Modifiers::NONE);
    let expected = h.state().ai_remove.as_ref().unwrap().mask.clone();
    let mask_undo = h.state().ai_remove.as_ref().unwrap().undo.len();
    assert!(mask_undo > 0);
    h.key_press(Key::R);
    h.run_steps(3);
    assert_eq!(h.state().ui.tool, Tool::RotateView);
    assert_eq!(crate::shortcut_dispatch::Focus::of(&h.ctx), crate::shortcut_dispatch::Focus::None);
    for key in [Key::Enter, Key::Escape] {
        h.key_press(key);
        h.run_steps(2);
        assert_eq!(h.state().ai_remove.as_ref().unwrap().mask, expected);
        assert_eq!(h.state().ai_remove.as_ref().unwrap().undo.len(), mask_undo);
        assert!(calls.borrow().is_empty());
        assert_eq!(h.state().session.active().unwrap().doc, filled);
    }
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(3);
    assert_eq!(h.state().session.active().unwrap().doc, before_fill, "Rotate leaves Ctrl+Z to document Undo");
    assert_eq!(h.state().session.active().unwrap().history.past_len() + 1, undo);
    assert_eq!(h.state().ai_remove.as_ref().unwrap().mask, expected);
    assert_eq!(h.state().ai_remove.as_ref().unwrap().undo.len(), mask_undo);
    assert!(calls.borrow().is_empty());
    h.state_mut().session.edit_prefs(|p| {
        p.set_shortcut("tools.select.AiRemove", Some("F14"));
    });
    h.key_press(Key::F14);
    h.run_steps(3);
    assert_eq!(h.state().ui.tool, Tool::AiRemove);
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert_eq!(calls.borrow().as_slice(), &[expected]);
    assert!(h.state().ai_remove.is_none());
}

struct RemoveMockReset;
impl Drop for RemoveMockReset {
    fn drop(&mut self) {
        TEST_REMOVE.with(|mock| *mock.borrow_mut() = None);
        crate::ai_ui::TEST_STATUS.with(|status| *status.borrow_mut() = None);
    }
}

#[test]
fn fresh_remove_submits_only_on_release_and_keeps_unavailable_or_failed_work_for_manual_retry() {
    let calls = std::rc::Rc::new(std::cell::Cell::new(0));
    let seen = calls.clone();
    TEST_REMOVE.with(|mock| {
        *mock.borrow_mut() = Some(Box::new(move |_, _| {
            seen.set(seen.get() + 1);
            if seen.get() == 1 { Err("Synthetic submission failure".into()) } else { Ok(json!({})) }
        }))
    });
    let _reset = RemoveMockReset;
    // Opt-in apply-on-release mode (the default is brush, adjust, confirm).
    let mut h = harness_mode(true);
    assert!(h.state().ui.ai.remove_immediately);
    let doc = h.state().session.active().unwrap().doc.clone();
    let history = h.state().session.active().unwrap().history.past_len();
    let revision = h.state().session.active().unwrap().revision;
    let xf = ViewXform::active(h.state()).unwrap();
    let a = xf.to_screen(60.0, 70.0);
    let b = xf.to_screen(95.0, 70.0);
    h.hover_at(a);
    h.step();
    h.event(Event::PointerButton { pos: a, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.step();
    h.hover_at(b);
    h.step();
    assert_eq!(calls.get(), 0, "down and movement cannot submit");
    h.event(Event::PointerButton { pos: b, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(3);
    assert_eq!(calls.get(), 1, "apply-on-release mode submits at release");
    let mask = h.state().ai_remove.as_ref().unwrap().mask.clone();
    h.run_steps(4);
    assert_eq!(calls.get(), 1, "repaint cannot retry");
    stroke(&mut h, 180.0, 150.0, Modifiers::NONE);
    assert_eq!(calls.get(), 1, "refining a failed mask cannot silently retry");
    assert!(h.state().ai_remove.as_ref().unwrap().mask.iter().zip(mask).all(|(a, b)| *a >= b));
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert_eq!(calls.get(), 2);
    assert!(h.state().ai_remove.is_none());
    crate::ai_ui::TEST_STATUS.with(|status| status.borrow_mut().as_mut().unwrap().connected = false);
    stroke(&mut h, 60.0, 70.0, Modifiers::NONE);
    assert!(h.state().ai_remove.is_some());
    assert_eq!(calls.get(), 2);
    crate::ai_ui::TEST_STATUS.with(|status| status.borrow_mut().as_mut().unwrap().connected = true);
    h.run_steps(3);
    stroke(&mut h, 180.0, 150.0, Modifiers::NONE);
    assert_eq!(calls.get(), 2, "availability and later paint cannot auto-resubmit");
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert_eq!(calls.get(), 3);
    h.key_press(Key::Enter);
    h.run_steps(2);
    assert_eq!(calls.get(), 3);
    let st = h.state().session.active().unwrap();
    assert!(std::sync::Arc::ptr_eq(&doc, &st.doc));
    assert_eq!((st.revision, st.history.past_len()), (revision, history));
}

#[test]
fn pending_remove_terminal_identity_failure_and_cancel_retain_the_exact_mask() {
    use photocraft_engine::jobs::{JobEvent, JobId, JobOutcome};
    for outcome in [JobOutcome::Failed("Synthetic worker failure".into()), JobOutcome::Cancelled] {
        let calls = std::rc::Rc::new(std::cell::Cell::new(0));
        let seen = calls.clone();
        TEST_REMOVE.with(|mock| {
            *mock.borrow_mut() = Some(Box::new(move |_, _| {
                seen.set(seen.get() + 1);
                Ok(json!({"pending":true,"job":41}))
            }))
        });
        let _reset = RemoveMockReset;
        let mut h = harness_mode(true);
        let doc = h.state().session.active().unwrap().doc.clone();
        let revision = h.state().session.active().unwrap().revision;
        let history = h.state().session.active().unwrap().history.past_len();
        stroke(&mut h, 60.0, 70.0, Modifiers::NONE);
        let mask = h.state().ai_remove.as_ref().unwrap().mask.clone();
        assert_eq!(h.state().ai_remove.as_ref().unwrap().job, Some(JobId(41)));
        h.key_press(Key::Enter);
        h.run_steps(2);
        assert_eq!(calls.get(), 1);
        let mut event = JobEvent { id: JobId(42), command: "ai.remove".into(), label: "AI Remove".into(), document: Some(doc.id), outcome: outcome.clone() };
        on_job_event(h.state_mut(), &event);
        assert_eq!(h.state().ai_remove.as_ref().unwrap().job, Some(JobId(41)));
        event.id = JobId(41);
        event.document = Some(photocraft_doc::DocId(doc.id.0 + 1000));
        on_job_event(h.state_mut(), &event);
        assert_eq!(h.state().ai_remove.as_ref().unwrap().job, Some(JobId(41)));
        event.document = Some(doc.id);
        on_job_event(h.state_mut(), &event);
        h.run_steps(3);
        assert_eq!(h.state().ai_remove.as_ref().unwrap().mask, mask);
        assert_eq!(h.state().ai_remove.as_ref().unwrap().job, None);
        stroke(&mut h, 180.0, 150.0, Modifiers::NONE);
        assert_eq!(calls.get(), 1);
        h.key_press(Key::Enter);
        h.run_steps(2);
        assert_eq!(calls.get(), 2);
        event.outcome = JobOutcome::Done(json!({}));
        on_job_event(h.state_mut(), &event);
        assert!(h.state().ai_remove.is_none());
        let st = h.state().session.active().unwrap();
        assert!(std::sync::Arc::ptr_eq(&doc, &st.doc));
        assert_eq!((st.revision, st.history.past_len()), (revision, history));
    }
}

fn paints_texture(shape: &egui::Shape, texture: egui::TextureId) -> bool {
    match shape {
        egui::Shape::Mesh(mesh) => mesh.texture_id == texture,
        egui::Shape::Vec(shapes) => shapes.iter().any(|shape| paints_texture(shape, texture)),
        _ => false,
    }
}

#[test]
fn dormant_remove_mask_stays_visible_while_navigating_and_is_never_replaced_by_a_selection() {
    let mut h = harness();
    stroke(&mut h, 60.0, 70.0, Modifiers::NONE);
    let p = h.state().ai_remove.as_ref().unwrap();
    let (mask, texture) = (p.mask.clone(), p.texture.as_ref().unwrap().id());
    h.state_mut().ui.tool = Tool::Hand;
    h.run_steps(3);
    assert!(h.output().shapes.iter().any(|s| paints_texture(&s.shape, texture)), "navigating keeps the mask in view");
    assert!(h.query_by_label("Keep painting").is_none(), "no Remove controls over another tool");
    h.state_mut().run("select.rect", json!({"x": 150, "y": 120, "width": 50, "height": 30})).unwrap();
    selection(h.state_mut());
    h.run_steps(2);
    assert_eq!(h.query_all_by_label("Keep painting").count(), 2, "floating and options bars");
    assert_eq!(h.state().ai_remove.as_ref().unwrap().mask, mask, "Remove Selection cannot replace painted work");
    assert_eq!(h.state().ui.tool, Tool::AiRemove);
    assert_eq!(h.state().ui.status, "Finish or cancel the current removal mask first.");
    h.get_all_by_label("Clear").next().unwrap().click();
    h.run_steps(2);
    assert!(!h.state().ai_remove.as_ref().unwrap().show_bar, "nothing left to confirm after Clear");
    assert_eq!(h.query_all_by_label("Keep painting").count(), 1, "the floating bar closes; the options bar keeps the controls");
}

#[test]
fn dormant_remove_mask_is_not_painted_or_rebound_to_another_layer_or_document() {
    let mut h = harness();
    stroke(&mut h, 60.0, 70.0, Modifiers::NONE);
    stroke(&mut h, 180.0, 150.0, Modifiers::NONE);
    let p = h.state().ai_remove.as_ref().unwrap();
    let (mask, owner, layer, texture) = (p.mask.clone(), p.doc, p.layer, p.texture.as_ref().unwrap().id());
    assert!(h.output().shapes.iter().any(|s| paints_texture(&s.shape, texture)));
    h.key_press(Key::B);
    h.run_steps(3);
    assert_eq!(h.state().ui.tool, Tool::Brush);
    assert!(!h.output().shapes.iter().any(|s| paints_texture(&s.shape, texture)));
    assert!(h.query_by_label("Keep painting").is_none());
    for key in [Key::Enter, Key::Escape] {
        h.key_press(key);
        h.run_steps(2);
    }
    assert_eq!(h.state().ai_remove.as_ref().unwrap().mask, mask);
    h.state_mut().ui.tool = Tool::AiRemove;
    h.run_steps(2);
    assert!(h.output().shapes.iter().any(|s| paints_texture(&s.shape, texture)));
    h.state_mut().run("layer.new.layer", json!({})).unwrap();
    let other_layer_doc = h.state().session.active().unwrap().doc.clone();
    let other_layer_history = h.state().session.active().unwrap().history.past_len();
    stroke(&mut h, 100.0, 100.0, Modifiers::NONE);
    h.key_press(Key::Enter);
    h.run_steps(2);
    assert_eq!(h.state().ai_remove.as_ref().unwrap().mask, mask);
    assert_eq!(h.state().session.active().unwrap().doc, other_layer_doc);
    assert_eq!(h.state().session.active().unwrap().history.past_len(), other_layer_history);
    h.state_mut().session.active_mut().unwrap().active_layer = layer;
    h.state_mut().run("file.new", json!({"width":320,"height":240})).unwrap();
    let other_doc = h.state().session.active().unwrap().doc.clone();
    stroke(&mut h, 100.0, 100.0, Modifiers::NONE);
    h.key_press(Key::Enter);
    h.run_steps(2);
    assert_eq!(h.state().ai_remove.as_ref().unwrap().doc, owner);
    assert_eq!(h.state().ai_remove.as_ref().unwrap().mask, mask);
    assert_eq!(h.state().session.active().unwrap().doc, other_doc);
    h.state_mut().session.set_active(0);
    h.run_steps(3);
    assert!(target_active(h.state(), h.state().ai_remove.as_ref().unwrap()));
    assert_eq!(h.state().ai_remove.as_ref().unwrap().mask, mask);
    h.state_mut().run("file.close", json!({"force":true})).unwrap();
    h.run_steps(3);
    assert!(h.state().ai_remove.is_none(), "closed source cannot attach to the remaining document");
}

#[test]
fn detected_remove_mask_never_auto_submits_after_additive_refinement_or_clear_undo() {
    let calls = std::rc::Rc::new(std::cell::Cell::new(0));
    let seen = calls.clone();
    TEST_REMOVE.with(|mock| {
        *mock.borrow_mut() = Some(Box::new(move |_, _| {
            seen.set(seen.get() + 1);
            Ok(json!({}))
        }))
    });
    let _reset = RemoveMockReset;
    let mut h = harness_mode(true);
    let doc = h.state().session.active().unwrap().doc.clone();
    let history = h.state().session.active().unwrap().history.past_len();
    detected(
        h.state_mut(),
        image::GrayImage::from_fn(320, 240, |x, y| image::Luma([if (60..65).contains(&x) && (50..120).contains(&y) { 255 } else { 0 }])),
        false,
    )
    .unwrap();
    h.run_steps(3);
    stroke(&mut h, 180.0, 150.0, Modifiers::NONE);
    assert_eq!(calls.get(), 0, "detected masks require approval even after adding paint");
    let expected = h.state().ai_remove.as_ref().unwrap().mask.clone();
    h.get_all_by_label("Clear").next().unwrap().click();
    h.run_steps(2);
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(2);
    assert_eq!(h.state().ai_remove.as_ref().unwrap().mask, expected);
    assert_eq!(calls.get(), 0);
    assert_eq!(h.state().session.active().unwrap().doc, doc);
    assert_eq!(h.state().session.active().unwrap().history.past_len(), history);
    h.key_press(Key::Enter);
    h.run_steps(2);
    assert_eq!(calls.get(), 1);
    assert!(h.state().ai_remove.is_none());
}

#[test]
fn removal_mask_changed_dimensions_cannot_rebind_on_return() {
    let mut h = harness();
    stroke(&mut h, 60.0, 70.0, Modifiers::NONE);
    h.key_press(Key::B);
    h.run_steps(2);
    h.state_mut().run("image.canvasSize", json!({"width":400,"height":260,"anchor":"topLeft"})).unwrap();
    let resized = h.state().session.active().unwrap().doc.clone();
    let history = h.state().session.active().unwrap().history.past_len();
    h.run_steps(3);
    assert!(h.state().ai_remove.is_none());
    h.state_mut().ui.tool = Tool::AiRemove;
    h.key_press(Key::Enter);
    h.run_steps(2);
    assert!(h.state().ai_remove.is_none());
    assert_eq!(h.state().session.active().unwrap().doc, resized);
    assert_eq!(h.state().session.active().unwrap().history.past_len(), history);
}

#[test]
fn remove_confirmation_controls_fit_compact_and_large_light_dark_frames() {
    for theme in [crate::theme::ThemeKind::Pro, crate::theme::ThemeKind::StudioLight] {
        for size in [vec2(760.0, 480.0), vec2(1440.0, 900.0)] {
            let mut h = harness();
            h.set_size(size);
            h.set_pixels_per_point(1.25);
            h.state_mut().ui.theme = theme;
            PhotocraftApp::setup_context(&h.ctx, theme);
            h.run_steps(6);
            stroke(&mut h, 60.0, 70.0, Modifiers::NONE);
            h.run_steps(4);
            let area = h.ctx.memory(|m| m.area_rect(egui::Id::new("remove-confirm"))).unwrap();
            let canvas = ViewXform::active(h.state()).unwrap().rect;
            assert!(canvas.contains_rect(area), "{theme:?}/{size:?}: {area:?} outside {canvas:?}");
            for label in ["Remove", "Keep painting", "Clear", "Cancel"] {
                let node = h.get_all_by_label(label).find(|n| area.contains_rect(n.rect())).unwrap_or_else(|| panic!("{label} missing in {area:?}"));
                assert!(canvas.contains_rect(node.rect()), "{label} clipped");
            }
            let doc = h.state().session.active().unwrap().doc.clone();
            let history = h.state().session.active().unwrap().history.past_len();
            h.get_all_by_label("Clear").find(|n| area.contains_rect(n.rect())).unwrap().click();
            h.run_steps(3);
            assert!(h.state().ai_remove.as_ref().unwrap().bounds.is_empty());
            stroke(&mut h, 180.0, 150.0, Modifiers::NONE);
            h.run_steps(3);
            crate::ai_ui::TEST_STATUS.with(|status| status.borrow_mut().as_mut().unwrap().connected = false);
            h.run_steps(3);
            let area = h.ctx.memory(|m| m.area_rect(egui::Id::new("remove-confirm"))).unwrap();
            assert!(h.get_all_by_label("Remove").find(|n| area.contains_rect(n.rect())).unwrap().accesskit_node().is_disabled());
            h.get_all_by_label("Cancel").find(|n| area.contains_rect(n.rect())).unwrap().click();
            h.run_steps(3);
            assert!(h.state().ai_remove.is_none());
            assert_eq!(h.state().ui.tool, Tool::AiRemove, "Cancel ends the removal but keeps the Remove tool");
            assert_eq!(h.state().session.active().unwrap().doc, doc);
            assert_eq!(h.state().session.active().unwrap().history.past_len(), history);
        }
    }
    crate::ai_ui::TEST_STATUS.with(|status| *status.borrow_mut() = None);
}

#[test]
fn reasons_ux_selection_context_remove_adjusts_then_confirms_captured_mask() {
    let calls = std::rc::Rc::new(std::cell::RefCell::new(Vec::<Vec<u8>>::new()));
    let seen = calls.clone();
    TEST_REMOVE.with(|mock| {
        *mock.borrow_mut() = Some(Box::new(move |_, params| {
            seen.borrow_mut().push(serde_json::from_value(params["mask"].clone()).unwrap());
            Ok(json!({}))
        }))
    });
    let _reset = RemoveMockReset;
    let mut h = harness_mode(true);
    h.state_mut().ui.tool = Tool::RectMarquee;
    h.state_mut().run("select.rect", json!({"x":50,"y":60,"width":30,"height":25})).unwrap();
    let expected = photocraft_engine::ai_cmds::selection_gray(&h.state().session.active().unwrap().doc).unwrap().into_raw();
    let doc = h.state().session.active().unwrap().doc.clone();
    h.run_steps(3);
    let p = ViewXform::active(h.state()).unwrap().to_screen(60., 70.);
    for pressed in [true, false] {
        h.event(Event::PointerButton { pos: p, button: PointerButton::Secondary, pressed, modifiers: Modifiers::NONE });
        h.run_steps(2);
    }
    h.get_by_label("AI Remove…").click();
    h.run_steps(3);
    assert_eq!(h.state().ai_remove.as_ref().unwrap().mask, expected);
    assert!(h.state().ai_remove.as_ref().unwrap().require_confirmation);
    assert_eq!(calls.borrow().len(), 0);
    assert_eq!(h.state().session.active().unwrap().doc, doc);
    // Refinement must also wait for confirmation when immediate mode is enabled.
    stroke(&mut h, 180., 150., Modifiers::NONE);
    assert_eq!(calls.borrow().len(), 0);
    let adjusted = h.state().ai_remove.as_ref().unwrap().mask.clone();
    assert_ne!(adjusted, expected);
    h.get_all_by_label("Remove").next().unwrap().click();
    h.run_steps(3);
    assert_eq!(calls.borrow().as_slice(), &[adjusted]);
    assert!(h.state().ai_remove.is_none());
}

#[test]
fn lama_offline_brush_adjust_confirm_and_explicit_qwen_choice() {
    let mut h = harness();
    crate::ai_ui::TEST_STATUS.with(|s| s.borrow_mut().as_mut().unwrap().connected = false);
    crate::ai_ui::TEST_LAMA_READY.with(|r| *r.borrow_mut() = Some(true));
    let calls = std::rc::Rc::new(std::cell::Cell::new(0));
    let observed = calls.clone();
    TEST_REMOVE.with(|m| {
        *m.borrow_mut() = Some(Box::new(move |_, params| {
            assert_eq!(params["engine"], "lama");
            observed.set(observed.get() + 1);
            Ok(json!({"layer": 999}))
        }))
    });
    stroke(&mut h, 60.0, 80.0, Modifiers::NONE);
    assert_eq!(calls.get(), 0, "brushing never submits");
    let initial = h.state().ai_remove.as_ref().unwrap().mask.clone();
    stroke(&mut h, 75.0, 80.0, Modifiers::ALT);
    assert_ne!(initial, h.state().ai_remove.as_ref().unwrap().mask);
    assert_eq!(calls.get(), 0, "adjusting never submits");
    h.get_all_by_label("Remove").next().unwrap().click();
    h.run_steps(3);
    assert_eq!(calls.get(), 1);
    assert!(h.state().ai_remove.is_none());
    // Choosing a generative engine remains explicit and is not silently substituted.
    h.get_by_value("Automatic").click();
    h.run_steps(2);
    h.get_by_label("Qwen Compact").click();
    h.run_steps(3);
    assert_eq!(h.state().ui.ai.remove_engine, "qwen-int8");
    assert!(!ready(h.state()));
    crate::ai_ui::TEST_LAMA_READY.with(|r| *r.borrow_mut() = None);
    TEST_REMOVE.with(|m| *m.borrow_mut() = None);
}

#[test]
fn lama_missing_download_preserves_painted_mask_and_opens_tools() {
    let mut h = harness();
    crate::ai_ui::TEST_STATUS.with(|s| s.borrow_mut().as_mut().unwrap().connected = false);
    crate::ai_ui::TEST_LAMA_READY.with(|r| *r.borrow_mut() = Some(false));
    stroke(&mut h, 60.0, 80.0, Modifiers::NONE);
    let mask = h.state().ai_remove.as_ref().unwrap().mask.clone();
    assert!(!ready(h.state()));
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert_eq!(h.state().ai_remove.as_ref().unwrap().mask, mask);
    h.get_all_by_label("On-device Tools…").last().unwrap().click();
    h.run_steps(3);
    assert!(h.state().ui.dialogs.iter().any(|d| d.fields.get("section").and_then(serde_json::Value::as_str) == Some("ai.tools")));
    crate::ai_ui::TEST_LAMA_READY.with(|r| *r.borrow_mut() = None);
}

#[test]
fn lama_old_ai_options_load_and_explicit_engine_persists() {
    let empty: crate::ai_ui::AiOptions = serde_json::from_value(json!({})).unwrap();
    assert_eq!(empty.remove_engine, "auto");
    assert!(!empty.remove_immediately);
    for key in ["klein", "qwen-int8", "qwen-bf16", "lama"] {
        let options: crate::ai_ui::AiOptions = serde_json::from_value(json!({"remove_engine":key})).unwrap();
        assert_eq!(options.remove_engine, key);
    }
}

#[test]
#[ignore = "real local model and RTX 5090 offscreen app interaction; run explicitly"]
fn lama_real_brush_gpu_confirm_stays_responsive_and_undoes() {
    let _gpu_gate = crate::gpu_test_support::lock();
    crate::ai_ui::TEST_STATUS.with(|s| *s.borrow_mut() = Some(crate::ai_ui::EngineStatus { checked: true, connected: false, ..Default::default() }));
    crate::ai_ui::TEST_LAMA_READY.with(|r| *r.borrow_mut() = None);
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/lama/photos/interior.jpg");
    let input = image::imageops::resize(&image::open(source).unwrap().to_rgba8(), 640, 480, image::imageops::FilterType::Lanczos3);
    let format = photocraft_color::PixelFormat::RGBA8;
    let mut doc =
        photocraft_doc::Document::new("CC0 interior", photocraft_geom::Size::new(640, 480), photocraft_color::ColorMode::Rgb, photocraft_color::SampleType::U8);
    let mut layer = photocraft_doc::Layer::raster("CC0 source", format);
    *layer.surface_mut().unwrap() = photocraft_raster::Surface::from_interleaved(format, doc.bounds(), input.as_raw());
    doc.layers.push(layer);
    let mut session = photocraft_engine::Session::new();
    session.add_document(doc, None);
    let mut setup = crate::gpu_canvas::wgpu_setup();
    crate::gpu_canvas::use_adapter_limits_with(&mut setup, |adapter| {
        let info = adapter.get_info();
        assert_eq!(info.backend, eframe::wgpu::Backend::Vulkan);
        assert_eq!(info.device_type, eframe::wgpu::DeviceType::DiscreteGpu);
        assert!(info.name.contains("RTX 5090"), "{info:?}");
        eprintln!("LaMa real app adapter: {info:?}");
    });
    let mut h = Harness::builder().with_size(vec2(1280.0, 800.0)).with_max_steps(64).wgpu_setup(setup).build_eframe(move |cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        let mut app = PhotocraftApp::new(session, crate::Services::default());
        app.set_wgpu(cc.wgpu_render_state.as_ref().unwrap().clone());
        app.ui.tool = Tool::AiRemove;
        app.ui.ai.remove_engine = "lama".into();
        app.session.tools.brush.size = 32.0;
        app.background_jobs = true;
        app
    });
    h.run_steps(6);
    let before = h.state().session.active().unwrap().doc.clone();
    stroke(&mut h, 280.0, 220.0, Modifiers::NONE);
    stroke(&mut h, 290.0, 220.0, Modifiers::ALT);
    assert_eq!(h.state().session.active().unwrap().doc, before, "brush/adjust are transient");
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/lama/ui");
    std::fs::create_dir_all(&root).unwrap();
    h.render().unwrap().save(root.join("real-confirm.png")).unwrap();
    let start = std::time::Instant::now();
    h.get_all_by_label("Remove").next().unwrap().click();
    h.run_steps(1);
    let submit = start.elapsed();
    assert!(submit.as_secs_f64() < 1.0, "submission must not wait for inference: {submit:?}");
    let mut frames_ms = Vec::new();
    while h.state().ai_remove.is_some() {
        assert!(start.elapsed().as_secs() < 120, "real local removal timed out: {}", h.state().ui.status);
        let frame = std::time::Instant::now();
        h.run_steps(1);
        h.render().unwrap();
        frames_ms.push(frame.elapsed().as_secs_f64() * 1000.0);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let total = start.elapsed();
    assert!(h.state().gpu_active() && h.state().perf.gpu, "real GPU canvas must remain active");
    assert!(h.state().perf.gpu_fallback.is_none(), "{:?}", h.state().perf.gpu_fallback);
    assert!(!frames_ms.is_empty(), "inference must span interactive frames");
    let mean_frame = frames_ms.iter().sum::<f64>() / frames_ms.len() as f64;
    frames_ms.sort_by(f64::total_cmp);
    let p95_frame = frames_ms[(frames_ms.len() - 1) * 95 / 100];
    let slowest_frame = frames_ms.last().copied().unwrap_or(0.0);
    let repaired = h.state().session.active().unwrap().doc.clone();
    assert_eq!(repaired.layers.len(), before.layers.len() + 1);
    h.render().unwrap().save(root.join("real-repaired.png")).unwrap();
    h.state_mut().run("edit.undo", json!({})).unwrap();
    assert_eq!(h.state().session.active().unwrap().doc, before);
    h.state_mut().run("edit.redo", json!({})).unwrap();
    assert_eq!(h.state().session.active().unwrap().doc, repaired);
    eprintln!(
        "LaMa real app 640x480: submit_ms={} total_s={:.3} gpu_render_readback_mean_ms={mean_frame:.3} p95_ms={p95_frame:.3} max_ms={slowest_frame:.3}; brush/adjust/confirm/undo/redo passed",
        submit.as_millis(),
        total.as_secs_f64()
    );
}
