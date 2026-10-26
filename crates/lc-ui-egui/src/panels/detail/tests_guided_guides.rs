use std::time::Duration;

use serde_json::json;

use crate::headless::Headless;
use crate::{LightcraftApp, Services};

const T: Duration = Duration::from_secs(20);
const SETTLE: Duration = Duration::from_secs(120);

fn guided() -> Headless {
    let mut app = LightcraftApp::new(lightcraft_engine::Session::with_demo(), Services { png: None, ..Default::default() });
    app.ui.settings.gpu = false;
    let mut h = Headless::new(app, [1200.0, 800.0], 1.0);
    for (method, params) in [
        ("ui.set", json!({"view": "detail"})),
        ("engine.execute", json!({"command": "tool.guidedUpright"})),
        ("engine.execute", json!({"command": "geometry.guides", "params": {"guides": [[0.2, 0.3, 0.8, 0.3], [0.2, 0.7, 0.8, 0.7]]}})),
    ] {
        let r = h.request(method, params, T);
        assert_eq!(r["ok"], true, "{r}");
    }
    assert!(h.settle(SETTLE));
    h
}

fn guides_len(h: &Headless) -> usize {
    let id = h.app.session.active().expect("a photo");
    h.app.session.catalog.photo(id).expect("the photo").develop.geometry.guides.len()
}

fn guide_at(h: &Headless, i: usize) -> egui::Pos2 {
    guide_rect(h, i).center()
}

fn guide_rect(h: &Headless, i: usize) -> egui::Rect {
    h.app.widgets.iter().rev().find(|(w, _)| *w == format!("uprightGuide:{i}")).map(|(_, r)| *r).expect("the guide is on screen")
}

#[test]
fn a_click_near_a_guide_selects_it() {
    let mut h = guided();
    assert_eq!(guides_len(&h), 2);
    let id = h.app.session.active().expect("a photo");
    let r = h.request("ui.click", json!({"x": guide_at(&h, 0).x, "y": guide_at(&h, 0).y}), T);
    assert_eq!(r["ok"], true, "{r}");
    assert!(h.settle(SETTLE));
    assert_eq!(h.app.ui.selected_guide, Some((id.0, 0)), "the clicked guide is selected");
    assert_eq!(guides_len(&h), 2, "selecting removes nothing");
}

#[test]
fn delete_removes_the_selected_guide() {
    let mut h = guided();
    let r = h.request("ui.click", json!({"x": guide_at(&h, 1).x, "y": guide_at(&h, 1).y}), T);
    assert_eq!(r["ok"], true, "{r}");
    assert!(h.settle(SETTLE));
    assert_eq!(h.app.ui.selected_guide.map(|(_, i)| i), Some(1));
    let undo = h.app.session.undo.len();
    let r = h.request("ui.key", json!({"key": "Delete"}), T);
    assert_eq!(r["ok"], true, "{r}");
    assert!(h.settle(SETTLE));
    assert_eq!(guides_len(&h), 1, "one guide is gone");
    assert_eq!(h.app.ui.selected_guide, None, "and nothing is selected afterwards");
    let id = h.app.session.active().expect("a photo");
    let d = h.app.session.catalog.photo(id).expect("the photo");
    let (a, b) = d.develop.geometry.guides[0];
    assert!(
        (a.x - 0.2).abs() < 1e-6 && (a.y - 0.3).abs() < 1e-6 && (b.x - 0.8).abs() < 1e-6 && (b.y - 0.3).abs() < 1e-6,
        "the first guide survived: {a:?}–{b:?}"
    );
    assert_eq!(h.app.session.undo.len(), undo + 1, "one deletion edit");
    let r = h.request("engine.execute", json!({"command":"edit.undo"}), T);
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(guides_len(&h), 2, "undo restores the deleted guide");
}

#[test]
fn a_click_away_clears_the_selection() {
    let mut h = guided();
    let r = h.request("ui.click", json!({"x": guide_at(&h, 0).x, "y": guide_at(&h, 0).y}), T);
    assert_eq!(r["ok"], true, "{r}");
    assert!(h.settle(SETTLE));
    assert!(h.app.ui.selected_guide.is_some());
    let img = h.app.image_rect.expect("the photo on screen");
    let r = h.request("ui.click", json!({"x": img.center().x, "y": img.top() + 0.95 * img.height()}), T);
    assert_eq!(r["ok"], true, "{r}");
    assert!(h.settle(SETTLE));
    assert_eq!(h.app.ui.selected_guide, None);
    assert_eq!(guides_len(&h), 2, "and removes nothing");
}

#[test]
fn delete_with_no_selection_removes_nothing() {
    let mut h = guided();
    let undo = h.app.session.undo.len();
    let r = h.request("ui.key", json!({"key": "Delete"}), T);
    assert_eq!(r["ok"], true, "{r}");
    assert!(h.settle(SETTLE));
    assert_eq!(guides_len(&h), 2, "no guide is removed");
    assert_eq!(h.app.session.undo.len(), undo, "no edit either");
    assert!(h.app.session.active().is_some(), "the photo is still active");
    assert!(h.app.ui.dialog.is_none(), "Delete never opens the photo deletion dialog");
}

#[test]
fn drawing_a_new_guide_clears_the_selection() {
    let mut h = guided();
    let r = h.request("ui.click", json!({"x": guide_at(&h, 0).x, "y": guide_at(&h, 0).y}), T);
    assert_eq!(r["ok"], true, "{r}");
    assert!(h.settle(SETTLE));
    assert!(h.app.ui.selected_guide.is_some());
    let img = h.app.image_rect.expect("the photo on screen");
    let r = h.request(
        "ui.drag",
        json!({"x": img.left() + 0.3 * img.width(), "y": img.top() + 0.5 * img.height(), "toX": img.left() + 0.7 * img.width(), "toY": img.top() + 0.5 * img.height()}),
        T,
    );
    assert_eq!(r["ok"], true, "{r}");
    assert!(h.settle(SETTLE));
    assert_eq!(guides_len(&h), 3, "the drawn guide is added");
    assert_eq!(h.app.ui.selected_guide, None, "and the click's selection is gone");
}

#[test]
fn dragging_a_guide_endpoint_moves_it() {
    let mut h = guided();
    assert_eq!(guides_len(&h), 2);
    let id = h.app.session.active().expect("a photo");
    let img = h.app.image_rect.expect("the photo on screen");
    let before = guide_rect(&h, 0);
    let from = before.left_top();
    let to = egui::pos2(from.x + 24.0, from.y + 60.0);
    let norm = |p: egui::Pos2| (((p.x - img.left()) / img.width()) as f64, ((p.y - img.top()) / img.height()) as f64);
    let (fx, fy) = norm(from);
    let (tx, ty) = norm(to);
    let r = h.request(
        "ui.pointer",
        json!({"events": [{"kind": "down", "x": fx, "y": fy}, {"kind": "drag", "x": tx, "y": ty}, {"kind": "up", "x": tx, "y": ty}]}),
        T,
    );
    assert_eq!(r["ok"], true, "{r}");
    assert!(h.settle(SETTLE));
    assert_eq!(guides_len(&h), 2, "no guide is added");
    let d = h.app.session.catalog.photo(id).expect("the photo");
    let (a, b) = d.develop.geometry.guides[0];
    assert!((a.x - tx).abs() < 1e-3 && (a.y - ty).abs() < 1e-3, "the pressed end followed the pointer: {a:?} vs ({tx}, {ty})");
    assert!((b.x - 0.8).abs() < 1e-6 && (b.y - 0.3).abs() < 1e-6, "the far end stays: {b:?}");
    assert_eq!(h.app.ui.selected_guide, Some((id.0, 0)), "the moved guide is selected");
}

#[test]
fn clicking_an_endpoint_selects_it_without_changing_it() {
    let mut h = guided();
    let id = h.app.session.active().expect("a photo");
    let undo = h.app.session.undo.len();
    let img = h.app.image_rect.expect("the photo on screen");
    let at = guide_rect(&h, 1).left_top();
    let x = ((at.x - img.left()) / img.width()) as f64;
    let y = ((at.y - img.top()) / img.height()) as f64;
    let r = h.request("ui.pointer", json!({"events": [{"kind": "down", "x": x, "y": y}, {"kind": "up", "x": x, "y": y}]}), T);
    assert_eq!(r["ok"], true, "{r}");
    assert!(h.settle(SETTLE));
    assert_eq!(h.app.ui.selected_guide, Some((id.0, 1)), "the endpoint's guide is selected");
    assert_eq!(guides_len(&h), 2);
    let d = h.app.session.catalog.photo(id).expect("the photo");
    let (a, b) = d.develop.geometry.guides[1];
    assert!(
        (a.x - 0.2).abs() < 1e-6 && (a.y - 0.7).abs() < 1e-6 && (b.x - 0.8).abs() < 1e-6 && (b.y - 0.7).abs() < 1e-6,
        "the guide is untouched: {a:?}–{b:?}"
    );
    assert_eq!(h.app.session.undo.len(), undo, "a click is not an edit");
}

#[test]
fn a_new_guide_starts_at_the_press_point() {
    let mut h = guided();
    assert_eq!(guides_len(&h), 2);
    let img = h.app.image_rect.expect("the photo on screen");
    let p1 = egui::pos2(img.left() + 0.35 * img.width(), img.top() + 0.55 * img.height());
    let p2 = egui::pos2(img.left() + 0.6 * img.width(), img.top() + 0.55 * img.height());
    let r = h.request(
        "ui.pointer",
        json!({"events": [{"kind": "down", "x": 0.35, "y": 0.55}, {"kind": "drag", "x": 0.6, "y": 0.55}, {"kind": "up", "x": 0.6, "y": 0.55}]}),
        T,
    );
    assert_eq!(r["ok"], true, "{r}");
    assert!(h.settle(SETTLE));
    assert_eq!(guides_len(&h), 3, "the drawn guide is added");
    assert_eq!(h.app.ui.selected_guide, None, "drawing a new guide leaves nothing selected");
    let rect = guide_rect(&h, 2);
    assert!(rect.left_top().distance(p1) < 2.0, "the guide starts at the press point: {rect:?} vs {p1:?}");
    assert!(rect.right_top().distance(p2) < 2.0, "and ends at the release point: {rect:?} vs {p2:?}");
}

#[test]
fn a_drag_whose_press_and_release_share_one_frame_still_draws() {
    let mut h = guided();
    assert_eq!(guides_len(&h), 2);
    let undo = h.app.session.undo.len();
    let img = h.app.image_rect.expect("the photo on screen");
    let p1 = egui::pos2(img.left() + 0.35 * img.width(), img.top() + 0.55 * img.height());
    let p2 = egui::pos2(img.left() + 0.6 * img.width(), img.top() + 0.55 * img.height());
    h.events.push(egui::Event::PointerMoved(p1));
    h.events.push(egui::Event::PointerButton { pos: p1, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
    h.events.push(egui::Event::PointerMoved(p2));
    h.events.push(egui::Event::PointerButton { pos: p2, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE });
    assert!(h.settle(SETTLE));
    assert_eq!(guides_len(&h), 3, "the drag that shared one frame still draws its guide");
    assert_eq!(h.app.session.undo.len(), undo + 1, "one edit was written");
    let rect = guide_rect(&h, 2);
    assert!(rect.left_top().distance(p1) < 2.0, "the guide starts at the press point: {rect:?} vs {p1:?}");
    assert!(rect.right_top().distance(p2) < 2.0, "and ends at the release point: {rect:?} vs {p2:?}");
}

#[test]
fn a_right_click_on_a_guide_selects_nothing() {
    let mut h = guided();
    let r = h.request("ui.click", json!({"x": guide_at(&h, 0).x, "y": guide_at(&h, 0).y, "button": "right"}), T);
    assert_eq!(r["ok"], true, "{r}");
    assert!(h.settle(SETTLE));
    assert_eq!(h.app.ui.selected_guide, None, "the right button selects nothing");
    assert_eq!(guides_len(&h), 2, "and draws nothing");
}

#[test]
fn a_press_on_the_navigator_draws_no_guide() {
    let mut h = guided();
    let r = h.request("engine.execute", json!({"command": "view.zoom100"}), T);
    assert_eq!(r["ok"], true, "{r}");
    assert!(h.settle(SETTLE));
    let nav = h.app.widgets.iter().rev().find(|(w, _)| *w == "canvas:navigator").map(|(_, r)| *r).expect("navigator shown when zoomed");
    let img = h.app.image_rect.expect("the photo on screen");
    let c = nav.center();
    let (nx, ny) = ((c.x - img.left()) / img.width(), (c.y - img.top()) / img.height());
    let r = h.request(
        "ui.pointer",
        json!({"events": [{"kind": "down", "x": nx, "y": ny}, {"kind": "drag", "x": nx + 0.05, "y": ny}, {"kind": "up", "x": nx + 0.05, "y": ny}]}),
        T,
    );
    assert_eq!(r["ok"], true, "{r}");
    assert!(h.settle(SETTLE));
    assert_eq!(guides_len(&h), 2, "the press on the navigator drew no guide");
    assert_eq!(h.app.ui.selected_guide, None, "and selected nothing");
}

#[test]
fn a_press_on_a_menu_over_a_guide_selects_nothing() {
    let mut h = guided();
    let id = h.app.session.active().expect("a photo");
    let undo = h.app.session.undo.len();
    let line = guide_at(&h, 0);
    let r = h.request("ui.click", json!({"x": line.x - 30.0, "y": line.y - 30.0, "button": "right"}), T);
    assert_eq!(r["ok"], true, "{r}");
    assert!(h.settle(SETTLE));
    assert!(egui::Popup::is_any_open(&h.view.ctx), "the context menu is open over the photo");
    let img = h.app.image_rect.expect("the photo on screen");
    let (lx, ly) = ((line.x - img.left()) / img.width(), (line.y - img.top()) / img.height());
    let r = h.request("ui.pointer", json!({"events": [{"kind": "down", "x": lx, "y": ly}, {"kind": "up", "x": lx, "y": ly}]}), T);
    assert_eq!(r["ok"], true, "{r}");
    assert!(h.settle(SETTLE));
    assert_eq!(h.app.ui.selected_guide, None, "the press under the menu selects nothing");
    assert!(h.app.gesture.is_none(), "and starts no gesture");
    assert_eq!(guides_len(&h), 2, "and draws nothing");
    assert_eq!(h.app.session.undo.len(), undo, "no edit was written");
    let d = h.app.session.catalog.photo(id).expect("the photo");
    let (a, b) = d.develop.geometry.guides[0];
    assert!(
        (a.x - 0.2).abs() < 1e-6 && (a.y - 0.3).abs() < 1e-6 && (b.x - 0.8).abs() < 1e-6 && (b.y - 0.3).abs() < 1e-6,
        "the guide is untouched: {a:?}–{b:?}"
    );
}

#[test]
fn a_press_on_a_menu_over_an_endpoint_moves_nothing() {
    let mut h = guided();
    let id = h.app.session.active().expect("a photo");
    let undo = h.app.session.undo.len();
    let end = guide_rect(&h, 0).left_top();
    let r = h.request("ui.click", json!({"x": end.x - 30.0, "y": end.y - 30.0, "button": "right"}), T);
    assert_eq!(r["ok"], true, "{r}");
    assert!(h.settle(SETTLE));
    assert!(egui::Popup::is_any_open(&h.view.ctx), "the context menu is open over the photo");
    let img = h.app.image_rect.expect("the photo on screen");
    let (ex, ey) = ((end.x - img.left()) / img.width(), (end.y - img.top()) / img.height());
    let r = h.request(
        "ui.pointer",
        json!({"events": [{"kind": "down", "x": ex, "y": ey}, {"kind": "drag", "x": ex + 0.05, "y": ey + 0.08}, {"kind": "up", "x": ex + 0.05, "y": ey + 0.08}]}),
        T,
    );
    assert_eq!(r["ok"], true, "{r}");
    assert!(h.settle(SETTLE));
    assert_eq!(h.app.ui.selected_guide, None, "the endpoint's press under the menu selects nothing");
    assert!(h.app.gesture.is_none(), "and starts no gesture");
    assert_eq!(guides_len(&h), 2, "and draws nothing");
    assert_eq!(h.app.session.undo.len(), undo, "no edit was written");
    let d = h.app.session.catalog.photo(id).expect("the photo");
    let (a, b) = d.develop.geometry.guides[0];
    assert!(
        (a.x - 0.2).abs() < 1e-6 && (a.y - 0.3).abs() < 1e-6 && (b.x - 0.8).abs() < 1e-6 && (b.y - 0.3).abs() < 1e-6,
        "the endpoint under the menu did not move: {a:?}–{b:?}"
    );
}

#[test]
fn a_press_that_leaves_the_menu_in_one_frame_draws_nothing() {
    let mut h = guided();
    let undo = h.app.session.undo.len();
    let end = guide_rect(&h, 0).left_top();
    let r = h.request("ui.click", json!({"x": end.x - 30.0, "y": end.y - 30.0, "button": "right"}), T);
    assert_eq!(r["ok"], true, "{r}");
    assert!(h.settle(SETTLE));
    assert!(egui::Popup::is_any_open(&h.view.ctx), "the context menu is open over the photo");
    let img = h.app.image_rect.expect("the photo on screen");
    let menu = end - egui::vec2(29.0, 29.0);
    let bare = egui::pos2(img.center().x, img.top() + 0.95 * img.height());
    let rel = |p: egui::Pos2| (((p.x - img.left()) / img.width()) as f64, ((p.y - img.top()) / img.height()) as f64);
    let (mx, my) = rel(menu);
    let (bx, by) = rel(bare);
    let r = h.request(
        "ui.pointer",
        json!({"events": [{"kind": "down", "x": mx, "y": my}, {"kind": "drag", "x": bx, "y": by}, {"kind": "up", "x": bx, "y": by}]}),
        T,
    );
    assert_eq!(r["ok"], true, "{r}");
    assert!(h.settle(SETTLE));
    assert_eq!(guides_len(&h), 2, "the press that left the menu drew no guide");
    assert_eq!(h.app.ui.selected_guide, None, "and selected nothing");
    assert!(h.app.gesture.is_none(), "and leaves no gesture behind");
    assert_eq!(h.app.session.undo.len(), undo, "no edit was written");
}

#[test]
fn delete_ignores_a_stale_guide_index() {
    let mut h = guided();
    let id = h.app.session.active().unwrap();
    let undo = h.app.session.undo.len();
    h.app.ui.selected_guide = Some((id.0, 10));
    h.events.push(egui::Event::Key { key: egui::Key::Backspace, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE });
    h.step();
    assert_eq!(h.app.ui.selected_guide, None);
    assert_eq!(guides_len(&h), 2);
    assert_eq!(h.app.session.undo.len(), undo, "stale selection writes no edit");
    assert!(h.app.ui.dialog.is_none(), "Delete cannot reach photo deletion");
}

#[test]
fn changing_photo_or_tool_clears_the_guide_selection() {
    let mut h = guided();
    let id = h.app.session.active().unwrap();
    h.app.ui.selected_guide = Some((id.0, 0));
    let r = h.request("engine.execute", json!({"command":"library.next"}), T);
    assert_eq!(r["ok"], true, "{r}");
    assert_ne!(h.app.session.active(), Some(id));
    assert_eq!(h.app.ui.selected_guide, None);
    let next = h.app.session.active().unwrap();
    h.app.ui.selected_guide = Some((next.0, 0));
    let r = h.request("engine.execute", json!({"command":"panel.remove"}), T);
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(h.app.ui.selected_guide, None);
}

#[test]
fn guides_can_be_selected_and_deleted_by_keyboard() {
    let mut h = guided();
    let photo = h.app.session.active().unwrap();
    let target = egui::Id::new(("uprightGuide", photo.0, 1_usize));
    for _ in 0..256 {
        if h.view.ctx.memory(|memory| memory.focused()) == Some(target) {
            break;
        }
        let r = h.request("ui.key", json!({"key":"tab"}), T);
        assert_eq!(r["ok"], true, "{r}");
    }
    assert_eq!(h.view.ctx.memory(|memory| memory.focused()), Some(target));
    let r = h.request("ui.key", json!({"key":"space"}), T);
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(h.app.ui.selected_guide, Some((photo.0, 1)));
    let undo = h.app.session.undo.len();
    let r = h.request("ui.key", json!({"key":"Backspace"}), T);
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(guides_len(&h), 1);
    assert_eq!(h.app.session.undo.len(), undo + 1);
    assert!(h.app.ui.dialog.is_none());
    assert!(!h.app.session.catalog.photo(photo).unwrap().deleted);
}

#[test]
fn endpoint_drag_in_a_warped_frame_preserves_the_other_corrected_points() {
    let mut h = guided();
    h.app.run("develop.set", json!({"control":"geometry.vertical","value":20})).unwrap();
    assert!(h.settle(SETTLE));
    let id = h.app.session.active().unwrap();
    let photo = h.app.session.catalog.photo(id).unwrap();
    let settings = photo.develop.clone();
    let original = settings.geometry.guides.clone();
    let frame = super::Frame::new(photo.width as usize, photo.height as usize, &settings, false);
    let map = super::CanvasMap::new(&frame, h.app.image_rect.unwrap());
    let from = map.screen(frame.corrected_to_transformed(original[0].0));
    let to = from + egui::vec2(24.0, 35.0);
    let expected = frame.transformed_to_corrected(map.norm(to));
    assert!(expected.dist(map.norm(to)) > 0.001, "fixture exercises the corrected-coordinate conversion");
    let undo = h.app.session.undo.len();
    h.events.extend([
        egui::Event::PointerMoved(from),
        egui::Event::PointerButton { pos: from, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE },
    ]);
    h.step();
    h.events.push(egui::Event::PointerMoved(to));
    h.step();
    h.events.push(egui::Event::PointerButton { pos: to, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE });
    h.step();
    let after = h.app.session.develop_of(id).unwrap();
    assert!(after.geometry.guides[0].0.dist(expected) < 1e-3);
    assert_eq!(after.geometry.guides[0].1, original[0].1);
    assert_eq!(after.geometry.guides[1], original[1]);
    assert_eq!(h.app.session.undo.len(), undo + 1);
    h.app.run("edit.undo", json!({})).unwrap();
    assert_eq!(h.app.session.develop_of(id).unwrap().geometry.guides, original);
}

#[test]
fn undo_remains_available_while_a_guide_has_keyboard_focus() {
    let mut h = guided();
    let photo = h.app.session.active().unwrap();
    h.view.ctx.memory_mut(|memory| memory.request_focus(egui::Id::new(("uprightGuide", photo.0, 0_usize))));
    h.step();
    let r = h.request("ui.key", json!({"key":"space"}), T);
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(h.app.ui.selected_guide, Some((photo.0, 0)));
    let r = h.request("ui.key", json!({"key":"Delete"}), T);
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(guides_len(&h), 1);
    let r = h.request("ui.key", json!({"key":"z", "cmd":true}), T);
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(guides_len(&h), 2, "Undo restores the guide while its surviving slot retains focus");
}

#[test]
fn focused_guide_reserves_activation_keys_and_keeps_rebound_commands() {
    for key in ["Enter", "Space"] {
        let mut h = guided();
        let photo = h.app.session.active().unwrap();
        h.app.shortcut_overrides.insert("library::rate:4:0".into(), key.into());
        h.app.shortcut_overrides.insert("library::view.zoomToggle".into(), "F8".into());
        h.app.shortcut_overrides.insert("library::tool.done".into(), "F9".into());
        let guide = egui::Id::new(("uprightGuide", photo.0, 0_usize));
        h.view.ctx.memory_mut(|memory| memory.request_focus(guide));
        h.step();
        let r = h.request("ui.key", json!({"key":key}), T);
        assert_eq!(r["ok"], true, "{r}");
        assert_eq!(h.app.ui.selected_guide, Some((photo.0, 0)), "{key} activates the guide");
        assert_eq!(h.app.session.catalog.photo(photo).unwrap().rating, 0, "{key} cannot rate the photo");
        let r = h.request("ui.key", json!({"key":"F8"}), T);
        assert_eq!(r["ok"], true, "{r}");
        assert_ne!(h.app.ui.zoom, crate::state::Zoom::Fit, "rebound zoom remains available");
        h.app.run("view.zoomFit", json!({})).unwrap();
        assert!(h.settle(SETTLE));
        h.view.ctx.memory_mut(|memory| memory.request_focus(guide));
        h.step();
        let r = h.request("ui.key", json!({"key":"F9"}), T);
        assert_eq!(r["ok"], true, "{r}");
        assert_ne!(h.app.ui.tool, "guidedUpright", "rebound Done remains available");
    }
}
