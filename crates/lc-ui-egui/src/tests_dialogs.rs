//! Modal application dialogs block background shortcuts while keeping their own keys
//! (ported from upstream LightCraft e1267a7).

use std::time::Duration;

use serde_json::json;

use crate::headless::Headless;
use crate::{LightcraftApp, Services};

const T: Duration = Duration::from_secs(20);

fn demo() -> Headless {
    let services = Services { png: None, ..Default::default() };
    let app = LightcraftApp::new(lightcraft_engine::Session::with_demo(), services);
    let mut h = Headless::new(app, [1400.0, 1000.0], 1.0);
    h.settle(Duration::from_secs(120));
    h
}

/// Rating, navigation and view keys pressed while a dialog is open do nothing to the photos behind
/// it; Esc closes the dialog only (it used to also fire Back to Grid); closing restores shortcuts.
#[test]
fn modal_dialogs_block_background_shortcuts_and_keep_escape() {
    let mut h = demo();
    let active = h.app.session.active().unwrap();
    let rating = h.app.session.catalog.photo(active).unwrap().rating;
    let flag = h.app.session.catalog.photo(active).unwrap().flag;
    let view = h.app.ui.view;
    for open in ["dialog.copySettings", "dialog.rename"] {
        h.app.run(open, json!({})).unwrap();
        h.step();
        assert!(h.app.ui.dialog.is_some(), "{open}");
        for key in ["5", "ArrowRight", "P"] {
            let r = h.request("ui.key", json!({"key": key}), T);
            assert_eq!(r["ok"], true, "{r}");
            h.step();
        }
        let photo = h.app.session.catalog.photo(active).unwrap();
        assert_eq!(photo.rating, rating, "{open}: the dialog blocks background ratings");
        assert_eq!(photo.flag, flag, "{open}: and flags");
        assert_eq!(h.app.session.active(), Some(active), "{open}: and navigation");
        let r = h.request("ui.key", json!({"key": "Escape"}), T);
        assert_eq!(r["ok"], true, "{r}");
        h.step();
        assert!(h.app.ui.dialog.is_none(), "{open}: Esc closes the dialog");
        assert_eq!(h.app.ui.view, view, "{open}: Esc only closes the dialog");
    }
    let r = h.request("ui.key", json!({"key": "5"}), T);
    assert_eq!(r["ok"], true, "{r}");
    h.step();
    assert_eq!(h.app.session.catalog.photo(active).unwrap().rating, 5, "closing restores shortcuts");
}
