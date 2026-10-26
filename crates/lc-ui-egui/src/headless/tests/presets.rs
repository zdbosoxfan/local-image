use super::*;

#[test]
fn applied_preset_is_marked_again_on_returning_to_the_photo() {
    let mut h = demo([1400.0, 900.0]);
    let t = Duration::from_secs(10);
    h.request("ui.set", json!({"view": "detail"}), t);
    h.request("engine.execute", json!({"command": "panel.presets"}), t);
    h.settle(SETTLE);
    let a = h.app.session.active().unwrap();
    let b = h.app.session.visible_cloned().into_iter().find(|id| *id != a).unwrap();
    let has = |h: &Headless, w: &str| h.app.widgets.iter().any(|(id, _)| id == w);
    let marked = |h: &Headless| {
        let r = h.app.widgets.iter().find(|(id, _)| id == "presetApplied").map(|(_, r)| *r)?;
        h.app.widgets.iter().find(|(id, wr)| id.starts_with("preset:") && *wr == r).map(|(id, _)| id.clone())
    };
    let r = h.request("ui.clickWidget", json!({"id": "preset:lc.warm-glow"}), t);
    assert_eq!(r["ok"], true, "{r}");
    h.settle(SETTLE);
    assert_eq!(marked(&h).as_deref(), Some("preset:lc.warm-glow"));
    assert!(has(&h, "slider:presetAmount"));
    for (photo, want) in [(b, None), (a, Some("preset:lc.warm-glow"))] {
        h.request("engine.execute", json!({"command": "library.select", "params": {"ids": [photo.0]}}), t);
        h.settle(SETTLE);
        h.step();
        assert_eq!(marked(&h).as_deref(), want, "on photo {photo:?}");
        assert_eq!(has(&h, "slider:presetAmount"), want.is_some(), "the Amount slider follows the mark");
    }
    h.request("engine.execute", json!({"command": "develop.set", "params": {"control": "light.exposure", "value": 0.5}}), t);
    h.settle(SETTLE);
    h.step();
    assert_eq!(marked(&h), None, "an edit after the preset drops the mark");
}

/// A preset applied while the active photo lies outside the selection changes only the
/// selection, so the active photo isn't marked as having it.
#[test]
fn applied_preset_is_not_marked_on_an_active_photo_outside_the_selection() {
    let mut h = demo([1400.0, 900.0]);
    let t = Duration::from_secs(10);
    h.request("ui.set", json!({"view": "detail"}), t);
    h.request("engine.execute", json!({"command": "panel.presets"}), t);
    h.settle(SETTLE);
    let a = h.app.session.active().unwrap();
    let b = h.app.session.visible_cloned().into_iter().find(|id| *id != a).unwrap();
    h.app.session.selection.ids = vec![b];
    h.app.session.selection.active = Some(a);
    let before = h.app.session.develop_of(a).unwrap();
    let r = h.request("ui.clickWidget", json!({"id": "preset:lc.warm-glow"}), t);
    assert_eq!(r["ok"], true, "{r}");
    h.settle(SETTLE);
    h.step();
    assert_eq!(h.app.session.active(), Some(a));
    assert!(*h.app.session.develop_of(a).unwrap() == *before, "the active photo wasn't changed");
    assert!(!h.app.widgets.iter().any(|(id, _)| id == "presetApplied"), "nor marked");
    h.request("engine.execute", json!({"command": "library.select", "params": {"ids": [b.0]}}), t);
    h.settle(SETTLE);
    h.step();
    assert!(h.app.widgets.iter().any(|(id, _)| id == "presetApplied"), "the selected photo is");
}

#[test]
fn preset_amount_disappears_after_an_unrelated_edit() {
    let mut h = demo([1400.0, 900.0]);
    let t = Duration::from_secs(10);
    h.request("ui.set", json!({"view": "detail"}), t);
    h.request("engine.execute", json!({"command": "panel.presets"}), t);
    h.settle(SETTLE);
    assert_eq!(h.request("ui.clickWidget", json!({"id": "preset:lc.warm-glow"}), t)["ok"], true);
    h.step();
    assert!(h.app.widgets.iter().any(|(id, _)| id == "slider:presetAmount"));
    h.request("engine.execute", json!({"command": "develop.set", "params": {"control": "light.exposure", "value": 0.5}}), t);
    h.step();
    assert!(!h.app.widgets.iter().any(|(id, _)| id == "slider:presetAmount"), "a later edit owns undo");
}
