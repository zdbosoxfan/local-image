#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use egui::{Event, Modifiers, PointerButton};
use egui_kittest::{Harness, kittest::Queryable};
use photocraft_doc::{Color, ColorMode, Fill, Layer, LayerContent, Path, SampleType, ShapeLayer, Size, Subpath};
use serde_json::json;

struct Host {
    session: Session,
    state: State,
    rect: Rect,
    view: View,
    enabled: bool,
}
pub(super) fn fixture() -> Session {
    let mut doc = Document::new("Repeat UI", Size::new(512, 320), ColorMode::Rgb, SampleType::U8);
    let shape = ShapeLayer {
        path: Path::new(vec![Subpath::polygon(&[(30., 30.), (42., 30.), (42., 40.), (30., 40.)])]),
        fill: Some(Fill::Solid(Color::BLACK)),
        ..Default::default()
    };
    doc.layers.push(Layer::new("Source", LayerContent::Shape(shape)));
    let mut s = Session::new();
    s.add_document(doc, None);
    s
}
fn harness() -> Harness<'static, Host> {
    Harness::builder().with_size(egui::vec2(760., 640.)).with_max_steps(64).build_ui_state(
        |ui, h: &mut Host| {
            panel(ui, &mut h.session, &mut h.state, str::to_owned);
            h.rect = ui.allocate_exact_size(egui::vec2(700., 350.), egui::Sense::hover()).0;
            h.view.origin = h.rect.min + egui::vec2(20., 20.);
            canvas(ui, &mut h.session, &mut h.state, h.rect, h.view, h.enabled);
        },
        Host {
            session: fixture(),
            state: State { preferences: Preferences { rows: 2, cols: 3, ..Default::default() }, ..Default::default() },
            rect: Rect::NOTHING,
            view: View { origin: Pos2::ZERO, zoom: 1., flip: false, rotation: 0.0 },
            enabled: true,
        },
    )
}
fn pointer(h: &mut Harness<'_, Host>, point: Point, pressed: Option<bool>) {
    let pos = h.state().view.screen(point);
    h.event(Event::PointerMoved(pos));
    if let Some(pressed) = pressed {
        h.event(Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
    }
    h.step();
}
fn handle(h: &Harness<'_, Host>, which: Handle) -> Point {
    handles(&current(&h.state().session).unwrap(), target_bounds(&h.state().session).unwrap()).into_iter().find(|(kind, _)| *kind == which).unwrap().1
}
fn create_grid(h: &mut Harness<'_, Host>) {
    h.get_by_label("Grid Repeat").click();
    h.run_steps(2);
    assert!(h.state().state.error.is_none(), "{:?}", h.state().state.error);
}
#[test]
fn repeat19_kittest_grid_count_spacing_actual_drags_expand_and_undo() {
    let mut h = harness();
    let original = h.state().session.active().unwrap().doc.clone();
    create_grid(&mut h);
    let live = h.state().session.active().unwrap().doc.clone();
    let rev = h.state().session.active().unwrap().revision;
    let j = h.state().session.journal.len();
    let p = handle(&h, Handle::Cols);
    pointer(&mut h, p, Some(true));
    pointer(&mut h, Point::new(p.x + 24., p.y), None);
    assert!(h.state().state.captured());
    assert!(h.state().state.preview.is_some());
    assert_eq!(h.state().session.active().unwrap().doc, live);
    assert_eq!(h.state().session.active().unwrap().revision, rev);
    assert_eq!(h.state().session.journal.len(), j);
    pointer(&mut h, Point::new(p.x + 24., p.y), Some(false));
    h.run_steps(2);
    assert!(matches!(current(&h.state().session), Some(RepeatSpec::Grid { cols: 5, rows: 2, .. })));
    assert_eq!(h.state().session.journal.len(), j + 1);
    let p = handle(&h, Handle::SpacingX);
    pointer(&mut h, p, Some(true));
    pointer(&mut h, Point::new(p.x + 7., p.y), None);
    pointer(&mut h, Point::new(p.x + 7., p.y), Some(false));
    h.run_steps(2);
    let edited = h.state().session.active().unwrap().doc.clone();
    assert!(matches!(current(&h.state().session), Some(RepeatSpec::Grid { spacing: [15., 8.], .. })));
    let projected = engine::project_document(&edited).unwrap();
    let second = projected.layers[0].children().unwrap()[1].children().unwrap()[0].surface().unwrap();
    assert_eq!(second.content_bounds().x0, 57);
    h.get_by_label("Expand Repeat").click();
    h.run_steps(2);
    assert!(current(&h.state().session).is_none());
    assert_eq!(h.state().session.active().unwrap().doc.layers[0].children().unwrap().len(), 10);
    assert!(h.state_mut().session.undo());
    assert_eq!(h.state().session.active().unwrap().doc, edited);
    assert!(h.state_mut().session.undo());
    assert!(h.state_mut().session.undo());
    assert!(h.state_mut().session.undo());
    assert_eq!(h.state().session.active().unwrap().doc, original);
}
#[test]
fn repeat19_kittest_radial_count_and_mirror_center_real_drags() {
    let mut h = harness();
    h.get_by_label("Radial Repeat").click();
    h.run_steps(2);
    let p = handle(&h, Handle::Count);
    pointer(&mut h, p, Some(true));
    pointer(&mut h, Point::new(p.x + 36., p.y), None);
    pointer(&mut h, Point::new(p.x + 36., p.y), Some(false));
    h.run_steps(2);
    assert!(matches!(current(&h.state().session), Some(RepeatSpec::Radial { count: 11, .. })));
    h.get_by_label("Mirror Repeat").click();
    h.run_steps(2);
    let p = handle(&h, Handle::Center);
    pointer(&mut h, p, Some(true));
    pointer(&mut h, Point::new(p.x + 9., p.y + 5.), None);
    pointer(&mut h, Point::new(p.x + 9., p.y + 5.), Some(false));
    h.run_steps(2);
    let Some(RepeatSpec::Mirror { origin, .. }) = current(&h.state().session) else { panic!() };
    assert_eq!(origin, [59., 40.]);
}
#[test]
fn repeat19_kittest_numeric_typing_commits_once_on_enter() {
    let mut h = harness();
    create_grid(&mut h);
    let j = h.state().session.journal.len();
    h.get_by_role_and_label(egui::accesskit::Role::SpinButton, "Repeat rows").click();
    h.run_steps(2);
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.event(Event::Text("5".into()));
    h.step();
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert!(
        matches!(current(&h.state().session), Some(RepeatSpec::Grid { rows: 5, cols: 3, .. })),
        "{:?} {:?}",
        current(&h.state().session),
        h.state().state.error
    );
    assert_eq!(h.state().session.journal.len(), j + 1);
}
#[test]
fn repeat19_kittest_escape_focus_tool_document_revision_and_selection_cancel() {
    for cancel in 0..6 {
        let mut h = harness();
        create_grid(&mut h);
        let before = h.state().session.active().unwrap().doc.clone();
        let j = h.state().session.journal.len();
        let p = handle(&h, Handle::SpacingX);
        pointer(&mut h, p, Some(true));
        pointer(&mut h, Point::new(p.x + 4., p.y), None);
        assert!(h.state().state.preview.is_some());
        match cancel {
            0 => h.key_press(Key::Escape),
            1 => {
                h.event(Event::WindowFocused(false));
                h.step();
            }
            2 => h.state_mut().enabled = false,
            3 => {
                h.state_mut().session.add_document(Document::new("Other", Size::new(16, 16), ColorMode::Rgb, SampleType::U8), None);
            }
            4 => {
                let id = before.layers[0].children().unwrap()[0].id;
                h.state_mut()
                    .session
                    .execute(
                        "shape.applyPaths",
                        json!({"edits":[{"layer":id.0,"path":Path::new(vec![Subpath::polygon(&[(30.,30.),(46.,30.),(46.,40.),(30.,40.)])])}]}),
                    )
                    .unwrap();
            }
            _ => {
                let id = before.layers[0].children().unwrap()[0].id;
                h.state_mut().session.select_layer(id).unwrap();
            }
        }
        h.run_steps(2);
        assert!(!h.state().state.captured());
        assert!(h.state().state.preview.is_none());
        pointer(&mut h, Point::new(p.x + 4., p.y), Some(false));
        h.run_steps(2);
        assert_eq!(h.state().session.journal.len(), j + usize::from(cancel == 4));
        if cancel < 3 || cancel == 5 {
            assert_eq!(h.state().session.active().unwrap().doc, before);
        }
    }
}
#[test]
fn repeat19_ui_preferences_old_settings_and_bad_limits_are_safe() {
    let p: Preferences = serde_json::from_str("{}").unwrap();
    assert_eq!(p.radial_count, 8);
    assert_eq!(p.spacing, [8., 8.]);
    let mut h = harness();
    h.state_mut().state.preferences.rows = 1000;
    h.state_mut().state.preferences.cols = 1000;
    let before = h.state().session.active().unwrap().doc.clone();
    h.get_by_label("Grid Repeat").click();
    h.run_steps(2);
    assert!(h.state().state.error.is_some());
    assert_eq!(h.state().session.active().unwrap().doc, before);
    assert!(h.state().session.journal.is_empty());
}
#[test]
fn repeat19_display_cache_invalidates_after_source_edit_undo_and_new_arc() {
    let mut s = fixture();
    let id = s.active().unwrap().doc.layers[0].id;
    let mut p = Capture::new(&s, &[id]).unwrap().params();
    p["spec"] = serde_json::to_value(RepeatSpec::Grid { rows: 2, cols: 3, spacing: [8., 8.] }).unwrap();
    s.execute("vector.repeat.create", p).unwrap();
    let ctx = egui::Context::default();
    let source = s.active().unwrap().doc.clone();
    let first = app_hooks::display(&s, &ctx, source.clone(), 0).unwrap().0;
    let cached = app_hooks::display(&s, &ctx, source.clone(), 0).unwrap().0;
    assert!(Arc::ptr_eq(&first, &cached));
    s.execute("shape.applyPaths", json!({"edits":[{"layer":id.0,"path":Path::new(vec![Subpath::polygon(&[(30.,30.),(46.,30.),(46.,40.),(30.,40.)])])}]}))
        .unwrap();
    let next = app_hooks::display(&s, &ctx, s.active().unwrap().doc.clone(), 0).unwrap().0;
    assert!(!Arc::ptr_eq(&next, &first));
    assert_eq!(next.layers[0].children().unwrap()[1].children().unwrap()[0].surface().unwrap().content_bounds().x0, 54);
    assert!(s.undo());
    let restored = app_hooks::display(&s, &ctx, s.active().unwrap().doc.clone(), 0).unwrap().0;
    assert!(!Arc::ptr_eq(&restored, &next));
    assert_eq!(restored.layers[0].children().unwrap()[1].children().unwrap()[0].surface().unwrap().content_bounds().x0, 50);
    let distinct = app_hooks::display(&s, &ctx, Arc::new((*source).clone()), 0).unwrap().0;
    assert!(!Arc::ptr_eq(&distinct, &restored));
}
