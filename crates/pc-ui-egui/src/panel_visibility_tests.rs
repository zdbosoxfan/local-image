//! Temporary panel visibility through real shell frames and keyboard routing.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use egui::{Key, Modifiers, Pos2, Shape, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use photocraft_engine::toolsets::Persona;
use serde_json::json;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

use crate::{PhotocraftApp, theme::ThemeKind};

fn harness(theme: ThemeKind, persona: Persona, size: egui::Vec2, ppp: f32) -> Harness<'static, PhotocraftApp> {
    let mut s = photocraft_engine::Session::new();
    s.execute("file.new", json!({"width":64,"height":48})).unwrap();
    s.execute("prefs.set", json!({"path":"interface.theme","value":theme.id()})).unwrap();
    s.edit_prefs(|p| p.toolbar.persona = persona);
    let mut h = Harness::builder().with_size(size).with_pixels_per_point(ppp).build_eframe(move |cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, theme);
        let mut app = PhotocraftApp::new(s, Default::default());
        app.ui.theme = theme;
        app
    });
    h.run_steps(6);
    h
}

fn tab(h: &mut Harness<'_, PhotocraftApp>) {
    h.key_press(Key::Tab);
    h.run_steps(3);
}

fn painted(h: &Harness<'_, PhotocraftApp>) -> Vec<String> {
    fn collect(shape: &Shape, out: &mut Vec<String>) {
        match shape {
            Shape::Text(t) => out.push(t.galley.job.text.clone()),
            Shape::Vec(v) => v.iter().for_each(|s| collect(s, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    for s in &h.output().shapes {
        collect(&s.shape, &mut out);
    }
    out
}

#[test]
fn tab_hides_and_restores_shell_without_changing_camera_document_or_preferences() {
    for theme in ThemeKind::ALL {
        for persona in [Persona::Pixel, Persona::Vector] {
            for (size, ppp) in [(vec2(1440., 900.), 1.), (vec2(760., 480.), 1.), (vec2(760., 480.), 1.25), (vec2(760., 480.), 2.)] {
                let mut h = harness(theme, persona, size, ppp);
                assert_eq!(h.state().ui.theme, theme);
                let doc = h.state().session.active().unwrap().doc.clone();
                let view = h.state().ui.views[0].clone();
                let dock = h.state().ui.dock.clone();
                let panels = h.state().ui.panels.clone();
                let prefs = h.state().session.prefs().clone();
                h.get_by_label("Move Tool");
                tab(&mut h);
                assert!(h.state().ui.view.panels_hidden, "{theme:?}/{persona:?}/{ppp}");
                assert!(h.query_by_label("Move Tool").is_none());
                assert!(h.query_by_label("Pixel persona").is_none());
                assert_eq!(crate::shortcut_dispatch::Focus::of(&h.ctx), crate::shortcut_dispatch::Focus::None, "Tab must not focus an invisible widget");
                assert_eq!(h.state().ui.views[0], view);
                assert_eq!(h.state().ui.dock, dock);
                assert_eq!(h.state().ui.panels, panels);
                assert_eq!(h.state().session.prefs(), &prefs);
                assert!(Arc::ptr_eq(&h.state().session.active().unwrap().doc, &doc));
                tab(&mut h);
                assert!(!h.state().ui.view.panels_hidden);
                h.get_by_label("Move Tool");
                assert_eq!(h.state().ui.views[0], view);
                assert_eq!(h.state().ui.dock, dock);
                assert_eq!(h.state().ui.panels, panels);
                assert_eq!(h.state().session.prefs(), &prefs);
                assert!(Arc::ptr_eq(&h.state().session.active().unwrap().doc, &doc));
            }
        }
    }
}

#[test]
fn tab_restores_floating_and_vector_panels_without_closing_or_moving_them() {
    let mut h = harness(ThemeKind::Studio, Persona::Vector, vec2(1440., 900.), 1.25);
    let app = h.state_mut();
    app.ui.panels.brush_settings = true;
    app.ui.presets_ui.tool_presets = true;
    app.ui.type_panels.styles = true;
    app.ui.type_panels.glyphs = true;
    app.ui.analysis.measurement_log = true;
    app.ui.timeline.open = true;
    app.ui.shell.modifier_keys = true;
    app.shared_tools.layout.open = true;
    for name in ["Appearance", "Document Palette", "Graphic Styles", "Frame Composition", "Snapping Manager"] {
        crate::vector_integration::set_panel_open(app, name, true);
    }
    app.ui.dock.float(&[crate::dock::Tab::Channels], Some(crate::dock::Tab::Channels), Pos2::new(120., 180.), vec2(240., 200.), false);
    h.run_steps(6);
    let titles = [
        "Brush Settings",
        "Character Styles",
        "Glyphs",
        "Measurement Log",
        "Timeline",
        "Modifier Keys",
        "Appearance",
        "Document Palette",
        "Graphic Styles",
        "Frame Composition",
        "Snapping Manager",
        "Transform and Layout",
    ];
    let text = painted(&h);
    for title in titles {
        assert!(text.iter().any(|s| s == title), "fixture must paint {title}");
    }
    let snapshot = |app: &PhotocraftApp| json!({"dock":app.ui.dock,"panels":app.ui.panels,"presets":app.ui.presets_ui,"type":app.ui.type_panels,"analysis":app.ui.analysis,"timeline":app.ui.timeline,"shell":app.ui.shell,"vector":crate::vector_integration::open_panels_for_test(app),"layout":app.shared_tools.layout.open});
    let before = snapshot(h.state());
    let positions: Vec<_> = ["type-styles-window", "glyphs-window", "measurement-log", "timeline", "modifier-keys", "vector layout panel"]
        .into_iter()
        .map(|id| (id, h.ctx.memory(|m| m.area_rect(egui::Id::new(id))).expect("floating panel has a real area")))
        .collect();
    tab(&mut h);
    assert!(h.state().ui.view.panels_hidden);
    let text = painted(&h);
    for title in titles {
        assert!(!text.iter().any(|s| s == title), "hidden panel still paints {title}");
    }
    assert_eq!(snapshot(h.state()), before);
    tab(&mut h);
    assert_eq!(snapshot(h.state()), before);
    for (id, rect) in positions {
        assert_eq!(h.ctx.memory(|m| m.area_rect(egui::Id::new(id))), Some(rect), "{id} moved");
    }
    let text = painted(&h);
    for title in titles {
        assert!(text.iter().any(|s| s == title), "{title} was not restored");
    }
}

#[test]
fn tab_keeps_timeline_and_background_jobs_running_and_is_not_persisted() {
    let mut h = harness(ThemeKind::ProMedium, Persona::Pixel, vec2(1440., 900.), 1.);
    h.state_mut().run("timeline.create", json!({"duration":120,"fps":30})).unwrap();
    h.state_mut().ui.timeline.open = true;
    h.state_mut().ui.timeline.playing = true;
    let gate = Arc::new(AtomicBool::new(false));
    let worker = gate.clone();
    let job = h
        .state_mut()
        .session
        .start_job(
            "test.panel-progress",
            json!({}),
            "Panel test job",
            false,
            move |ctx| {
                let end = Instant::now() + Duration::from_secs(10);
                while !worker.load(Ordering::SeqCst) {
                    ctx.check()?;
                    if Instant::now() >= end {
                        return Err(photocraft_engine::EngineError::Other("fixture deadline".into()));
                    }
                    std::thread::sleep(Duration::from_millis(2));
                }
                Ok(())
            },
            |_, ()| Ok(json!({"done":true})),
        )
        .unwrap();
    let photocraft_engine::jobs::Started::Job(job) = job else { panic!("worker") };
    tab(&mut h);
    assert!(h.state().ui.view.panels_hidden);
    let before = h.state().session.active().unwrap().doc.timeline.as_ref().unwrap().current;
    h.run_steps(6);
    assert!(h.state().ui.timeline.open && h.state().ui.timeline.playing);
    assert_ne!(h.state().session.active().unwrap().doc.timeline.as_ref().unwrap().current, before);
    assert!(h.state().session.job(job).is_some());
    let saved = serde_json::to_value(&h.state().ui.view).unwrap();
    let reopened: crate::view_cmds::ViewOptions = serde_json::from_value(saved).unwrap();
    assert!(!reopened.panels_hidden, "temporary hide never persists");
    gate.store(true, Ordering::SeqCst);
    let end = Instant::now() + Duration::from_secs(10);
    while h.state().session.has_jobs() {
        assert!(Instant::now() < end);
        h.run_steps(1);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(h.state().ui.view.panels_hidden);
}

#[test]
fn tab_binding_override_and_visible_menu_action_are_respected() {
    let mut h = harness(ThemeKind::ProMedium, Persona::Pixel, vec2(1440., 900.), 1.);
    h.state_mut().session.edit_prefs(|p| {
        p.shortcuts.insert("view.togglePanels".into(), "F8".into());
    });
    tab(&mut h);
    assert!(!h.state().ui.view.panels_hidden);
    if let Some(id) = h.ctx.memory(|m| m.focused()) {
        h.ctx.memory_mut(|m| m.surrender_focus(id));
    }
    h.key_press(Key::F8);
    h.run_steps(3);
    assert!(h.state().ui.view.panels_hidden);
    h.get_by_label("View").click();
    h.run_steps(3);
    // The View menu scrolls: an accessibility node can exist below its visible viewport.
    h.get_by_label("✔ Hide Panels F8").scroll_to_me();
    let mut previous = None;
    let mut reachable = false;
    for _ in 0..32 {
        h.run_steps(1);
        let row = h.get_by_label("✔ Hide Panels F8").rect();
        let viewport = crate::menu_nav::Nav::current(&h.ctx).views[0];
        if viewport.contains_rect(row) && previous == Some(row) {
            reachable = true;
            break;
        }
        previous = Some(row);
    }
    let row = h.get_by_label("✔ Hide Panels F8").rect();
    let viewport = crate::menu_nav::Nav::current(&h.ctx).views[0];
    assert!(reachable && viewport.contains_rect(row), "Hide Panels row {row:?} must be wholly visible in menu viewport {viewport:?}");
    assert!(h.ctx.content_rect().contains_rect(row));
    assert_eq!(h.ctx.layer_id_at(row.center()).map(|layer| layer.order), Some(egui::Order::Foreground), "the visible menu receives the click");
    h.get_by_label("✔ Hide Panels F8").click();
    h.run_steps(3);
    assert!(!h.state().ui.view.panels_hidden);
    h.key_press_modifiers(Modifiers::SHIFT, Key::Tab);
    h.run_steps(3);
    assert!(!h.state().ui.view.panels_hidden);
    assert_eq!(crate::shortcut_dispatch::Focus::of(&h.ctx), crate::shortcut_dispatch::Focus::Widget);
}
