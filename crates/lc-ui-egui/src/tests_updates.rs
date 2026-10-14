//! The actual Library Help menu requests the host's shared update window.
use serde_json::json;
use std::time::Duration;

#[test]
fn release_update_library_help_menu_is_clicked_headlessly() {
    let mut app = crate::LightcraftApp::new(lightcraft_engine::Session::with_demo(), Default::default());
    app.ui.settings.gpu = false;
    let mut h = crate::headless::Headless::new(app, [1200.0, 800.0], 1.0);
    h.step();
    h.step();
    for id in ["menu:Help", "menuItem:app.checkUpdates"] {
        let result = h.request("ui.clickWidget", json!({"id": id}), Duration::from_secs(20));
        assert_eq!(result["ok"], true, "{id}: {result}");
        h.step();
    }
    assert!(h.app.open_shared_updates);
    assert!(h.app.ui.dialog.is_none(), "the host owns the shared update window");
}
