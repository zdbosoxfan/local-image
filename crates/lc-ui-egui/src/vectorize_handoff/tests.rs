#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use crate::headless::HeadlessView;
use crate::{LightcraftApp, Services};
use egui::{Event, Modifiers, PointerButton, Pos2, Rect, Sense};
use lightcraft_catalog::{Op, Photo};
use serde_json::json;

/// The owned menu contribution is hosted on a photo response in Library's existing
/// offscreen driver.
struct Menu {
    app: LightcraftApp,
    view: HeadlessView,
    photo: Rect,
    button: Option<Rect>,
    frames: u64,
    target: PhotoId,
}
impl Menu {
    fn new() -> Self {
        let mut session = lightcraft_engine::Session::new();
        for id in 1..=3 {
            let photo =
                Photo::new(PhotoId(id), Source::File { path: format!("/library-fixture/{id}.png") }, &format!("{id}.png"), "PNG", 4, 3, "2026-10-10");
            session.catalog.apply(Op::AddPhoto { photo: Box::new(photo) }).unwrap();
        }
        session.execute("library.select", &json!({"ids":[1,3],"active":1})).unwrap();
        session.filter.person_ids = vec![17, 21];
        session.filter.people_everyone = true;
        let services = Services { open_with: Some(Box::new(|_, _| panic!("the vectorize action must stay in this window"))), ..Default::default() };
        let mut app = LightcraftApp::new(session, services);
        app.ui.people.filter_ids = vec![17, 21];
        app.ui.people.filter_everyone = true;
        app.ui.people.find_name = "Ada".into();
        Self { app, view: HeadlessView::new(), photo: Rect::NOTHING, button: None, frames: 0, target: PhotoId(2) }
    }
    fn frame(&mut self, events: Vec<Event>) {
        let raw = HeadlessView::raw_input(egui::vec2(600., 400.), 1., self.frames as f64 / 60., events);
        self.frames += 1;
        self.view.run(raw, |ui| {
            let photo = ui.add(egui::Label::new("Context photo 2").sense(Sense::click()));
            self.photo = photo.rect;
            photo.context_menu(|ui| {
                context_action(&self.app, ui, self.target);
            });
        });
        self.button = crate::widgets::take_registry(&self.view.ctx).into_iter().find(|(id, _)| id == WIDGET).map(|(_, rect)| rect);
    }
    fn click(&mut self, pos: Pos2, button: PointerButton) {
        self.frame(vec![Event::PointerMoved(pos), Event::PointerButton { pos, button, pressed: true, modifiers: Modifiers::NONE }]);
        self.frame(vec![Event::PointerButton { pos, button, pressed: false, modifiers: Modifiers::NONE }]);
        self.frame(vec![]);
    }
    fn open_menu(&mut self) {
        self.frame(vec![]);
        self.frame(vec![]);
        self.click(self.photo.center(), PointerButton::Secondary);
        self.frame(vec![]);
        assert!(self.button.is_some(), "right-click opens the photo context action");
    }
    fn snapshot(&self) -> serde_json::Value {
        json!({
            "selection":format!("{:?}",self.app.session.selection),
            "filter":self.app.session.filter,
            "source":format!("{:?}",self.app.session.source),
            "ui":serde_json::to_value(&self.app.ui).unwrap(),
            "people":format!("{:?}",self.app.ui.people),
            "photos":self.app.session.catalog.photos().collect::<Vec<_>>(),
            "revision":self.app.session.catalog.revision,
            "undo":self.app.session.undo.len(),"redo":self.app.session.redo.len()
        })
    }
}

#[test]
fn library_vectorize_headless_right_click_exact_photo_single_handoff_preserves_people_and_selection() {
    let mut h = Menu::new();
    let before = h.snapshot();
    h.open_menu();
    h.click(h.button.unwrap().center(), PointerButton::Primary);
    assert!(pending(&h.view.ctx));
    assert_eq!(h.snapshot(), before);
    // Reopening the menu cannot enqueue a second request while the first waits.
    h.open_menu();
    h.click(h.button.unwrap().center(), PointerButton::Primary);
    let request = take_request(&h.view.ctx).unwrap();
    assert_eq!(request.photo, PhotoId(2), "context target differs from active and selected photos");
    assert_eq!(request.path, "/library-fixture/2.png");
    request.check(&h.app).unwrap();
    assert!(take_request(&h.view.ctx).is_none());
    assert_eq!(h.snapshot(), before);
}

#[test]
fn library_vectorize_headless_unavailable_and_demo_photos_cannot_dispatch() {
    for unavailable in [true, false] {
        let mut h = Menu::new();
        if unavailable {
            h.app.services.open_with = None;
        } else {
            let demo = Photo::new(PhotoId(4), Source::Demo { scene: 1 }, "Demo", "Demo", 4, 3, "2026-10-10");
            h.app.session.catalog.apply(Op::AddPhoto { photo: Box::new(demo) }).unwrap();
            h.target = PhotoId(4);
        }
        let before = h.snapshot();
        h.open_menu();
        h.click(h.button.unwrap().center(), PointerButton::Primary);
        assert!(take_request(&h.view.ctx).is_none());
        assert_eq!(h.snapshot(), before);
    }
}

#[test]
fn library_vectorize_request_rejects_catalog_replacement_without_changing_state() {
    let mut h = Menu::new();
    h.open_menu();
    h.click(h.button.unwrap().center(), PointerButton::Primary);
    let request = take_request(&h.view.ctx).unwrap();
    h.app.session.execute("photo.rate", &json!({"ids":[2],"rating":5})).unwrap();
    let before = h.snapshot();
    assert!(request.check(&h.app).is_err());
    assert_eq!(h.snapshot(), before);
}
