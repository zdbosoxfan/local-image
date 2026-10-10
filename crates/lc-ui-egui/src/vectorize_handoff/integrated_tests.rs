//! Package 00 registers this after inserting the production grid/menu hook.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use crate::headless::Headless;
use crate::{LightcraftApp, Services};
use lightcraft_catalog::{Op, Photo};
use serde_json::json;
use std::time::Duration;

#[test]
#[ignore = "package 00 must insert the production context-menu hook and preserve selection"]
fn library_vectorize_integrated_headless_grid_context_click_preserves_multiselection_and_people_filters() {
    let mut session = lightcraft_engine::Session::new();
    for id in 1..=3 {
        let mut p =
            Photo::new(PhotoId(id), Source::File { path: format!("/library-fixture/{id}.png") }, &format!("{id}.png"), "PNG", 4, 3, "2026-10-10");
        p.meta.person_ids = vec![17, 21];
        session.catalog.apply(Op::AddPhoto { photo: Box::new(p) }).unwrap();
    }
    session.execute("library.select", &json!({"ids":[1,3],"active":1})).unwrap();
    session.filter.text = ".png".into();
    session.filter.person_ids = vec![17, 21];
    session.filter.people_everyone = true;
    let app = LightcraftApp::new(session, Services { open_with: Some(Box::new(|_, _| panic!("no external window"))), ..Default::default() });
    let mut h = Headless::new(app, [1400., 900.], 1.);
    let timeout = Duration::from_secs(30);
    h.app.ui.view = crate::state::ViewMode::SquareGrid;
    h.app.ui.people.filter_ids = vec![17, 21];
    h.app.ui.people.filter_everyone = true;
    h.app.ui.people.find_name = "Ada".into();
    assert!(h.settle(timeout));
    let selection = h.app.session.selection.clone();
    let filter = serde_json::to_value(&h.app.session.filter).unwrap();
    let people = (h.app.ui.people.filter_ids.clone(), h.app.ui.people.filter_everyone, h.app.ui.people.find_name.clone());
    let context = h.app.widgets.iter().find(|(id, _)| id == "thumb:2").expect("context photo visible").1.center();
    assert_eq!(h.request("ui.click", json!({"x":context.x,"y":context.y,"button":"right"}), timeout)["ok"], true);
    assert_eq!(h.app.session.selection, selection, "opening a context menu must not discard selection");
    assert_eq!(h.request("ui.clickWidget", json!({"id":WIDGET}), timeout)["ok"], true);
    let request = take_request(&h.view.ctx).unwrap();
    assert_eq!(request.photo, PhotoId(2));
    assert_eq!(request.path, "/library-fixture/2.png");
    assert!(take_request(&h.view.ctx).is_none());
    assert_eq!(h.app.session.selection, selection);
    assert_eq!(serde_json::to_value(&h.app.session.filter).unwrap(), filter);
    assert_eq!((h.app.ui.people.filter_ids.clone(), h.app.ui.people.filter_everyone, h.app.ui.people.find_name.clone()), people);
    assert!(h.settle(timeout));
}
