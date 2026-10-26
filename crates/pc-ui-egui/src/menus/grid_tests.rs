use super::*;
#[test]
fn showing_grid_restores_hidden_extras() {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.ui.view.extras = false;
    app.ui.extras.grid = false;
    let ctx = egui::Context::default();
    invoke(&mut app, &ctx, "view.show.grid", json!({})).unwrap();
    assert!(app.ui.view.extras && app.ui.extras.grid);
    invoke(&mut app, &ctx, "view.show.grid", json!({})).unwrap();
    assert!(!app.ui.extras.grid && app.ui.view.extras);
    app.ui.extras.grid = true;
    app.ui.view.extras = false;
    invoke(&mut app, &ctx, "view.show.grid", json!({})).unwrap();
    assert!(!app.ui.extras.grid && !app.ui.view.extras);
}
