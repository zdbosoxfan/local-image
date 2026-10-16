use super::*;
use crate::dock::Group;
use crate::theme::ThemeKind;
use egui::{Event, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;

struct Reset;
impl Drop for Reset {
    fn drop(&mut self) {
        crate::ai_ui::TEST_STATUS.with(|status| *status.borrow_mut() = None);
        rt(|runtime| *runtime = Runtime::default());
    }
}
fn fixture() -> (PhotocraftApp, Entry) {
    rt(|runtime| *runtime = Runtime::default());
    crate::ai_ui::TEST_STATUS.with(|status| {
        *status.borrow_mut() = Some(crate::ai_ui::EngineStatus {
            checked: true,
            connected: true,
            presets: catalog::presets().iter().map(|preset| (preset.id(), Ok(()))).collect(),
            ..Default::default()
        })
    });
    let services = crate::Services {
        import: Some(Box::new(|name, bytes| {
            photocraft_io::import(name, bytes).map(|result| (result.document, result.warnings)).map_err(|error| error.to_string())
        })),
        ..Default::default()
    };
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), services);
    app.session.execute("file.new", json!({"width":16,"height":16})).unwrap();
    app.ui.ai.generate.prompt = "a quiet garden".into();
    app.ui.ai.generate.advanced = true;
    app.ui.ai.generate.refine = true;
    app.ui.ai.generate.refine_model = ModelId::Klein4B.key().into();
    app.ui.ai.generate.refine_variant = catalog::default_variant(ModelId::Klein4B).into();
    crate::dock::reveal(&mut app, Group::Generate);
    let entry = library()
        .add(
            &RgbaImage::from_pixel(16, 16, image::Rgba([40, 90, 150, 255])),
            "sf07 completion",
            Some(json!({"command":"ai.generate","prompt":"fixture","model":ModelId::Klein4B.key(),"seed":42})),
            None,
        )
        .unwrap();
    let before = app.session.active().unwrap().doc.clone();
    let count = app.session.documents().len();
    on_generated(
        &mut app,
        &JobEvent {
            id: JobId(9_000_000),
            command: GENERATE_JOB.into(),
            label: "fixture".into(),
            document: None,
            outcome: JobOutcome::Done(serde_json::to_value(&entry).unwrap()),
        },
    );
    assert_eq!(app.session.documents().len(), count);
    assert!(Arc::ptr_eq(&before, &app.session.active().unwrap().doc), "Create completion must not replace/change the viewed canvas");
    assert_eq!(rt(|runtime| runtime.selected.clone()), Some(entry.id.clone()));
    rt(|runtime| runtime.capture_panel_start = true);
    (app, entry)
}
/// The shared CPU kittest renderer (`li-testkit`): no GPU or window.
fn cpu_renderer() -> li_testkit::SoftpaintRenderer {
    li_testkit::SoftpaintRenderer::with_clear_from(|ctx: &egui::Context| Tokens::get(ctx).canvas)
}

fn screenshot(h: &mut Harness<'_, PhotocraftApp>, name: &str, check_thumbnail: bool) {
    if check_thumbnail {
        let started = std::time::Instant::now();
        while !rt(|runtime| runtime.selected.as_ref().is_some_and(|id| runtime.gallery_thumbs.loaded(id))) {
            assert!(started.elapsed() < std::time::Duration::from_secs(10), "async Library thumbnail did not become ready");
            std::thread::sleep(std::time::Duration::from_millis(10));
            h.run_steps(1);
        }
    }
    let image = h.render().unwrap();
    if check_thumbnail {
        let center = visible(h, "thumbnail").rect.center() * h.ctx.pixels_per_point();
        let pixel = image.get_pixel(center.x as u32, center.y as u32).0;
        assert!(
            pixel.iter().zip([40u8, 90, 150, 255]).all(|(actual, want)| actual.abs_diff(want) <= 2),
            "fixture Library thumbnail was not painted: {pixel:?}"
        );
    }
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/generate-panel-ux");
    std::fs::create_dir_all(&dir).unwrap();
    image.save(dir.join(format!("{name}.png"))).unwrap();
}

fn harness(app: PhotocraftApp, size: egui::Vec2, theme: ThemeKind) -> Harness<'static, PhotocraftApp> {
    let mut harness = Harness::builder().with_size(size).with_step_dt(1. / 60.).renderer(cpu_renderer()).build_ui_state(
        |ui, app: &mut PhotocraftApp| {
            if !ui.ctx().fonts(|fonts| fonts.families().contains(&egui::FontFamily::Name("medium".into()))) {
                return;
            }
            crate::notices::panel(app, ui);
            crate::panels::right_dock(app, ui);
            egui::CentralPanel::default().show(ui, |_| {});
        },
        app,
    );
    PhotocraftApp::setup_context(&harness.ctx, theme);
    harness.state_mut().ui.theme = theme;
    harness.run_steps(5);
    harness
}
fn hit(h: &Harness<'_, PhotocraftApp>, name: &str) -> Hit {
    h.ctx
        .data(|data| data.get_temp::<Vec<Hit>>(egui::Id::new("generate-pinned-hits")))
        .unwrap()
        .into_iter()
        .find(|hit| hit.name == name)
        .unwrap_or_else(|| panic!("missing {name}"))
}
fn visible(h: &Harness<'_, PhotocraftApp>, name: &str) -> Hit {
    let hit = hit(h, name);
    let viewport = h.ctx.content_rect();
    let visible = hit.rect.intersect(hit.clip).intersect(viewport);
    assert!(hit.rect.width() > 4. && hit.rect.height() > 4., "empty hit: {hit:?}");
    assert!(
        visible.width() >= hit.rect.width() - 0.5 && visible.height() >= hit.rect.height() - 0.5,
        "paint/input clipped: {name} {hit:?} visible={visible:?}"
    );
    assert!(visible.contains(hit.rect.center()), "hit centre is clipped");
    hit
}
fn click(h: &mut Harness<'_, PhotocraftApp>, point: Pos2) {
    h.hover_at(point);
    h.step();
    h.event(Event::PointerButton { pos: point, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.step();
    h.event(Event::PointerButton { pos: point, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(3);
}
fn pending(app: &mut PhotocraftApp, own: bool) -> JobId {
    let command = if own { GENERATE_JOB } else { "ai.remove" };
    let job = app
        .session
        .start_job(
            command,
            json!({}),
            "SF07 controlled job",
            false,
            |ctx| {
                ctx.report(0.4, "Working on the fixture");
                let until = Instant::now() + Duration::from_secs(20);
                while !ctx.is_cancelled() && Instant::now() < until {
                    std::thread::sleep(Duration::from_millis(2));
                }
                ctx.check()?;
                Ok(())
            },
            |_, ()| Ok(Value::Null),
        )
        .unwrap();
    let photocraft_engine::jobs::Started::Job(id) = job else { panic!("background job") };
    if own {
        rt(|runtime| runtime.results.push_front(Tile::Pending { job: id.0, label: "Fixture generation".into(), edit: None, open: false }));
    }
    id
}

#[test]
fn sf07_pinned_actions_and_completion_are_clipped_visible_and_clickable() {
    let _lock = crate::ai_ui::AI_TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    for (size, height, theme) in [
        (vec2(1440., 900.), 200., ThemeKind::ProMedium),
        (vec2(1440., 900.), 360., ThemeKind::Studio),
        (vec2(760., 480.), 200., ThemeKind::ProMedium),
        (vec2(760., 480.), 360., ThemeKind::Studio),
    ] {
        let _reset = Reset;
        let (mut app, entry) = fixture();
        // User-expanded narrow dock: keep the other groups collapsed, rather than
        // replacing the real dock with an unbounded panel-sized test viewport.
        for group in [Group::Color, Group::Properties] {
            app.ui.dock.set_collapsed(group, true);
        }
        if height == 360. && size.y < 500. {
            app.ui.dock.set_collapsed(Group::Layers, true);
        }
        app.ui.dock.heights.insert(Group::Generate, height);
        let own = pending(&mut app, true);
        let unrelated = pending(&mut app, false);
        let original = app.session.active().unwrap().doc.clone();
        let original_count = app.session.documents().len();
        let mut h = harness(app, size, theme);
        for name in ["generate", "stop", "progress", "thumbnail", "open", "insert"] {
            visible(&h, name);
        }
        assert!(visible(&h, "generate").enabled);
        assert!(visible(&h, "insert").enabled);
        let theme_name = if theme == ThemeKind::ProMedium { "pro-medium" } else { "studio" };
        let shot_name = format!("completion-{}x{}-dock{}-{theme_name}", size.x as u32, size.y as u32, height as u32);
        screenshot(&mut h, &shot_name, true);
        let pinned = hit(&h, "generate").rect;
        let (_, settings) = h.ctx.data(|data| data.get_temp::<(egui::Rect, egui::Rect)>(egui::Id::new("generate-panel-layout"))).unwrap();
        h.hover_at(settings.center());
        h.event(Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: vec2(0., -1500.), phase: egui::TouchPhase::Move, modifiers: Modifiers::NONE });
        h.run_steps(4);
        assert_eq!(visible(&h, "generate").rect, pinned, "settings scrolling moved the action");
        screenshot(&mut h, &format!("{shot_name}-settings-scrolled"), true);
        let at = visible(&h, "generate").rect.center();
        click(&mut h, at);
        assert_eq!(rt(|runtime| runtime.panel_start_clicks), 1, "real clipped pointer click reaches Generate");
        assert!(Arc::ptr_eq(&original, &h.state().session.active().unwrap().doc));
        let at = visible(&h, "stop").rect.center();
        click(&mut h, at);
        assert!(!h.state().session.jobs().iter().any(|job| job.id == own));
        assert!(h.state().session.jobs().iter().any(|job| job.id == unrelated), "panel Stop cancelled another AI feature");
        let layers = h.state().session.active().unwrap().doc.layers.len();
        let at = visible(&h, "insert").rect.center();
        click(&mut h, at);
        assert_eq!(h.state().session.documents().len(), original_count);
        assert_eq!(h.state().session.active().unwrap().doc.layers.len(), layers + 1, "Insert is an explicit layer action");
        let at = visible(&h, "open").rect.center();
        click(&mut h, at);
        assert_eq!(h.state().session.documents().len(), original_count + 1, "Open is an explicit new-document action");
        h.state_mut().session.cancel_job(unrelated);
        h.state_mut().session.join_cancelled_jobs();
        library().delete(&[entry.id]).unwrap();
    }
}

#[test]
fn sf07_squeezed_dock_keeps_stop_progress_and_popup_access() {
    let _lock = crate::ai_ui::AI_TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    for theme in [ThemeKind::ProMedium, ThemeKind::Studio] {
        let _reset = Reset;
        let (mut app, entry) = fixture();
        app.ui.dock.heights.insert(Group::Generate, 80.);
        app.ui.ai.generate.count = 4; // Longer action label must not push Stop/progress out.
        crate::notices::post(&mut app, "Saved fixture", vec!["Encoder warning retained for details".into()], false, None);
        let own = pending(&mut app, true);
        let mut h = harness(app, vec2(760., 480.), theme);
        let panel = crate::dock::last_rects(&h.ctx).into_iter().find(|(group, _)| *group == Group::Generate).unwrap().1;
        assert!((panel.height() - 80.).abs() < 0.5, "actual squeezed group: {panel:?}");
        let notice = crate::notices::control_rect(&h.ctx, "details").unwrap();
        for name in ["generate", "stop", "progress", "settings", "results"] {
            let control = visible(&h, name);
            assert!(control.enabled);
            assert!(!control.rect.intersects(notice), "notice row overlaps {name}");
        }
        for label in ["Generate 4", "Stop", "View"] {
            let (text, button, clip) =
                h.ctx.data(|data| data.get_temp::<(egui::Rect, egui::Rect, egui::Rect)>(egui::Id::new(("generate-compact-label", label)))).unwrap();
            assert!(
                button.shrink(2.0).contains_rect(text) && clip.contains_rect(text),
                "full label must paint without truncation: {label} {text:?} {button:?} {clip:?}"
            );
        }
        assert!(
            (visible(&h, "progress").rect.center().y - visible(&h, "generate").rect.center().y).abs() < 3.0,
            "the actual squeezed content has no room for a second progress row"
        );
        let theme_name = if theme == ThemeKind::ProMedium { "pro-medium" } else { "studio" };
        screenshot(&mut h, &format!("squeezed-760x480-dock80-{theme_name}-progress"), false);
        let at = visible(&h, "generate").rect.center();
        click(&mut h, at);
        assert_eq!(rt(|runtime| runtime.panel_start_clicks), 1);
        let at = visible(&h, "settings").rect.center();
        click(&mut h, at);
        visible(&h, "settings-draft");
        h.key_press(egui::Key::Escape);
        h.run_steps(3);
        let at = visible(&h, "results").rect.center();
        click(&mut h, at);
        let at = visible(&h, "history-open").rect.center();
        visible(&h, "history-insert");
        screenshot(&mut h, &format!("squeezed-760x480-dock80-{theme_name}-results"), false);
        let documents = h.state().session.documents().len();
        click(&mut h, at);
        assert_eq!(h.state().session.documents().len(), documents + 1, "clipped Results popup Open works");
        h.key_press(egui::Key::Escape);
        h.run_steps(3);
        let at = visible(&h, "stop").rect.center();
        click(&mut h, at);
        assert!(!h.state().session.jobs().iter().any(|job| job.id == own));
        h.state_mut().session.join_cancelled_jobs();
        library().delete(&[entry.id]).unwrap();
    }
}

#[test]
fn sf07_completion_does_not_change_canvas_and_insert_requires_document() {
    let _lock = crate::ai_ui::AI_TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let _reset = Reset;
    let (mut app, entry) = fixture();
    app.session.close(0);
    for group in [Group::Color, Group::Properties, Group::Layers] {
        app.ui.dock.set_collapsed(group, true);
    }
    let mut h = harness(app, vec2(1440., 900.), ThemeKind::ProMedium);
    assert!(!visible(&h, "insert").enabled);
    assert!(visible(&h, "open").enabled);
    h.run_steps(4);
    assert!(h.state().session.documents().is_empty());
    let at = visible(&h, "open").rect.center();
    click(&mut h, at);
    assert_eq!(h.state().session.documents().len(), 1);
    library().delete(&[entry.id]).unwrap();
}
