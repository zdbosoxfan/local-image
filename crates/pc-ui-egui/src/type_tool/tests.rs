use super::*;

fn app() -> PhotocraftApp {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.session.execute("file.new", json!({"width": 400, "height": 200})).unwrap();
    app.sync_views();
    app.ui.tool = crate::state::Tool::Type;
    app
}

fn layer_text(app: &PhotocraftApp) -> String {
    let ed = app.ui.text_edit.as_ref().unwrap();
    current_text(app, LayerId(ed.layer)).unwrap()
}

#[test]
fn arrow_keys_reset_caret_blink_phase() {
    let mut app = app();
    pointer_up(&mut app, [10.0, 50.0], [10.0, 50.0]);
    insert(&mut app, "Hello");
    app.ui.caret_blink_time = 0.0;
    let ctx = egui::Context::default();

    let has_caret = |app: &mut PhotocraftApp, ctx: &egui::Context, time: f64| -> bool {
        let mut out = ctx.run_ui(egui::RawInput { time: Some(time), ..Default::default() }, |ui| {
            let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 400.0));
            let painter = ui.ctx().layer_painter(egui::LayerId::background()).with_clip_rect(rect);
            let xf = ViewXform { rect, zoom: 1.0, center: [100.0, 100.0], flip: false, rotation: 0.0 };
            draw_overlay(app, &painter, &xf);
        });
        out.textures_delta.clear();
        out.shapes.iter().any(|s| matches!(&s.shape, egui::Shape::LineSegment { stroke, .. } if (stroke.width - 1.5).abs() < 1e-4))
    };

    // t = 10.5 s is in the off half of the blink cycle (10.5 / 0.53 = 19, odd) for phase 0.
    assert!(!has_caret(&mut app, &ctx, 10.5), "caret is off at t = 10.5 with phase 0");

    ctx.begin_pass(egui::RawInput {
        time: Some(10.5),
        events: vec![egui::Event::Key { key: egui::Key::ArrowRight, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE }],
        ..Default::default()
    });
    handle_keys(&mut app, &ctx);
    assert_eq!(app.ui.caret_blink_time, 10.5);

    assert!(has_caret(&mut app, &ctx, 10.5), "the caret is solid right after an arrow key");
}

#[test]
fn pointer_up_click_resets_caret_blink_phase() {
    let mut app = app();
    pointer_up(&mut app, [10.0, 50.0], [10.0, 50.0]);
    insert(&mut app, "Hello");
    app.ui.caret_blink_time = 0.0;
    app.last_frame_time = 12.3;
    pointer_up(&mut app, [10.0, 50.0], [10.0, 50.0]);
    assert_eq!(app.ui.caret_blink_time, 12.3);
}

#[test]
fn click_creates_placeholder_selected_and_typing_replaces_it() {
    let mut app = app();
    assert!(!pointer_down(&mut app, 50.0, 100.0, false));
    pointer_up(&mut app, [50.0, 100.0], [50.0, 100.0]);
    let ed = app.ui.text_edit.clone().unwrap();
    assert_eq!((ed.anchor, ed.caret), (0, PLACEHOLDER.chars().count()));
    insert(&mut app, "Héllo");
    insert(&mut app, " world");
    assert_eq!(layer_text(&app), "Héllo world");
    // Creating and typing share the session key: one step after the initial snapshot.
    let steps = app.session.active().unwrap().history.entries();
    assert_eq!(steps, ["Open", "New Type Layer"]);
    commit(&mut app);
    let doc = &app.session.active().unwrap().doc;
    assert_eq!(doc.layers.last().unwrap().name, "Héllo world");
    assert!(app.ui.text_edit.is_none());
}

#[test]
fn a_name_from_text_after_blank_lines_keeps_following_the_text() {
    let mut app = app();
    pointer_up(&mut app, [50.0, 100.0], [50.0, 100.0]);
    insert(&mut app, "\n\nTitle");
    let id = app.ui.text_edit.as_ref().unwrap().layer;
    commit(&mut app);
    let name = |app: &PhotocraftApp| app.session.active().unwrap().doc.layers.last().unwrap().name.clone();
    assert_eq!(name(&app), "Title");
    // Edited later, outside the session that created it (#483).
    app.run("type.edit", json!({"layer": id, "text": "\nSubtitle"})).unwrap();
    assert_eq!(name(&app), "Subtitle");
}

#[test]
fn dragging_a_box_handle_resizes_the_paragraph_box() {
    let mut app = app();
    pointer_up(&mut app, [10.0, 10.0], [110.0, 60.0]);
    let id = LayerId(app.ui.text_edit.as_ref().unwrap().layer);
    let steps = app.session.active().unwrap().history.entries().len();
    // Top-left corner: the box keeps its bottom-right corner.
    assert!(pointer_down(&mut app, 10.0, 10.0, false));
    assert_eq!(app.ui.text_edit.as_ref().unwrap().resize, Some(0));
    pointer_move(&mut app, 20.0, 25.0);
    pointer_move(&mut app, 30.0, 30.0);
    pointer_up(&mut app, [10.0, 10.0], [30.0, 30.0]);
    assert_eq!(box_shape(&app, id), Some((0.0, 0.0, 80.0, 30.0)));
    let aff = layout(&mut app, id).unwrap().1;
    assert_eq!((aff.m[4], aff.m[5]), (30.0, 30.0));
    // The whole drag is one history step, and the edit session survives it.
    assert_eq!(app.session.active().unwrap().history.entries().len(), steps);
    assert!(app.ui.text_edit.is_some());
    // Right edge: only the width changes, and never below the minimum.
    assert!(pointer_down(&mut app, 110.0, 45.0, false));
    pointer_move(&mut app, 0.0, 99.0);
    pointer_up(&mut app, [110.0, 45.0], [0.0, 99.0]);
    assert_eq!(box_shape(&app, id), Some((0.0, 0.0, MIN_BOX, 30.0)));
}

#[test]
fn ime_preedit_is_replaced_by_each_update_and_commit_finalises() {
    let mut app = app();
    pointer_up(&mut app, [50.0, 100.0], [50.0, 100.0]);
    // Each preedit update replaces the previous one (and the placeholder selection first).
    ime_update(&mut app, "に", false);
    assert_eq!(layer_text(&app), "に");
    ime_update(&mut app, "にほん", false);
    ime_update(&mut app, "にほんご", false);
    assert_eq!(layer_text(&app), "にほんご");
    assert_eq!(app.ui.text_edit.as_ref().unwrap().preedit, Some((0, 4)));
    ime_update(&mut app, "日本語", true);
    assert_eq!(layer_text(&app), "日本語");
    let ed = app.ui.text_edit.clone().unwrap();
    assert_eq!((ed.caret, ed.anchor, ed.preedit), (3, 3, None));
    // The next composition starts after the committed text; cancelling removes it again.
    ime_update(&mut app, "あ", false);
    assert_eq!(layer_text(&app), "日本語あ");
    ime_update(&mut app, "", false);
    assert_eq!(layer_text(&app), "日本語");
    assert_eq!(app.ui.text_edit.as_ref().unwrap().preedit, None);
    // A stale preedit range (text shortened by undo) must not panic.
    app.ui.text_edit.as_mut().unwrap().preedit = Some((10, 5));
    ime_update(&mut app, "x", true);
    assert_eq!(layer_text(&app), "日本語x");
}

/// Upstream c1cb455c (#1381): on Wayland the input method sends an empty preedit (and
/// sometimes an empty commit) whenever the caret rectangle moves, e.g. while ⌘A, ⇧-arrows or
/// a drag select text. With nothing being composed that must leave the text and the selection
/// alone; a real composition still replaces the selection, and cancelling it removes only the
/// composed text.
#[test]
fn empty_ime_events_never_delete_the_selection() {
    let mut app = app();
    pointer_up(&mut app, [50.0, 100.0], [50.0, 100.0]);
    ime_update(&mut app, "Hello", true);
    assert_eq!(layer_text(&app), "Hello");
    if let Some(e) = app.ui.text_edit.as_mut() {
        (e.caret, e.anchor) = (5, 0);
    }
    ime_update(&mut app, "", false);
    ime_update(&mut app, "", true);
    assert_eq!(layer_text(&app), "Hello", "empty IME events keep the text");
    let ed = app.ui.text_edit.clone().unwrap();
    assert_eq!((ed.caret, ed.anchor, ed.preedit), (5, 0, None), "and the selection");
    // Composing replaces the selection; cancelling the composition leaves the rest.
    ime_update(&mut app, "か", false);
    assert_eq!(layer_text(&app), "か");
    ime_update(&mut app, "", false);
    assert_eq!(layer_text(&app), "");
}

#[test]
fn empty_new_layer_is_deleted_on_commit() {
    let mut app = app();
    pointer_up(&mut app, [10.0, 50.0], [10.0, 50.0]);
    let n = app.session.active().unwrap().doc.layers.len();
    insert(&mut app, "");
    if let Some(e) = app.ui.text_edit.as_mut() {
        e.anchor = 0;
    }
    insert(&mut app, "");
    commit(&mut app);
    assert_eq!(app.session.active().unwrap().doc.layers.len(), n - 1);
}

#[test]
fn ds_finding_type_quick_bar_uses_layer_size_and_preserves_available_style() {
    use egui_kittest::kittest::Queryable;
    let mut app = app();
    let id = app.run("type.create", json!({"text":"","size":47})).unwrap()["layer"].as_u64().unwrap();
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(600.0, 120.0)).build_ui_state(
        |ui, app: &mut PhotocraftApp| {
            quick_bar(app, ui);
        },
        app,
    );
    PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::Studio);
    h.run_steps(3);
    h.get_by_value("47").click();
    h.run_steps(2);
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    // Pick two installed families with a shared non-Regular style.
    let pair = families().iter().find_map(|a| {
        styles(a)
            .into_iter()
            .filter(|s| s != "Regular")
            .find_map(|style| families().iter().find(|b| *b != a && styles(b).contains(&style)).map(|b| (a.clone(), b.clone(), style)))
    });
    let (a, b, style) = pair.expect("test system has at least two families sharing a style");
    h.state_mut().run("type.edit", json!({"layer":id,"text":"styled"})).unwrap();
    h.state_mut().run("type.setStyle", json!({"layer":id,"font":a,"fontStyle":style})).unwrap();
    h.run_steps(3);
    h.get_by_value(&a).click();
    h.run_steps(2);
    h.event(egui::Event::Text(b.clone()));
    h.run_steps(2);
    h.get_by_label(&b).click();
    h.run_steps(3);
    let st = h.state().session.active().unwrap();
    let layer = text_layer(&st.doc, LayerId(id)).unwrap();
    assert_eq!(layer.font_family, b);
    assert_eq!(layer.char_runs()[0].style.font_style, style);
}

#[test]
fn word_boundaries() {
    assert_eq!(word_boundary("hello big world", 0, true), 5);
    assert_eq!(word_boundary("hello big world", 7, false), 6);
    assert_eq!(word_boundary("hello big world", 15, false), 10);
}
