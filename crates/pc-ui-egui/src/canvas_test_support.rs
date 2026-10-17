//! Shared kittest helpers for tests that drive the real document canvas and key handling.

use egui::{Modifiers, Pos2, vec2};
use egui_kittest::Harness;
use serde_json::json;

use crate::PhotocraftApp;
use crate::canvas::ViewXform;
use crate::state::Tool;

/// A 400 × 300 document in the bare canvas (no rulers), shortcuts handled, `tool` selected and the
/// view at 100 % and centred: one document pixel per point, so drags land on whole pixels.
/// `step_dt` overrides the harness frame time (e.g. 60 fps for double-clicks).
pub(crate) fn canvas_harness(tool: Tool, step_dt: Option<f32>) -> Harness<'static, PhotocraftApp> {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.run("file.new", json!({"width": 400, "height": 300})).unwrap();
    app.sync_views();
    app.ui.extras.rulers = false;
    app.ui.tool = tool;
    let mut builder = Harness::builder().with_size(vec2(1000.0, 700.0));
    if let Some(dt) = step_dt {
        builder = builder.with_step_dt(dt);
    }
    let mut h = builder.build_ui_state(
        |ui, app: &mut PhotocraftApp| {
            let ctx = ui.ctx().clone();
            if !ctx.fonts(|f| f.families().contains(&egui::FontFamily::Name("medium".into()))) {
                return;
            }
            crate::shortcuts::handle(app, &ctx);
            egui::CentralPanel::default().show(ui, |ui| crate::canvas::document_area(app, ui));
        },
        app,
    );
    PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::ALL[0]);
    h.run_steps(4);
    let v = &mut h.state_mut().ui.views[0];
    v.zoom = 1.0;
    v.center = [200.0, 150.0];
    v.fit_pending = false;
    h.run_steps(2);
    h
}

/// Document point `(x, y)` in screen points, through the first view's transform.
pub(crate) fn screen(h: &Harness<'static, PhotocraftApp>, x: f32, y: f32) -> Pos2 {
    let app = h.state();
    let v = &app.ui.views[0];
    let xf = ViewXform {
        rect: crate::rulers::content_rect(app, app.last_canvas_rect),
        zoom: v.zoom,
        center: v.center,
        flip: app.ui.view.flip_horizontal,
        rotation: v.rotation,
    };
    xf.to_screen(x, y)
}

/// The platform's modifiers for `Cmd` (⌘ on the Mac, Ctrl elsewhere), as egui-winit reports them.
pub(crate) fn platform(m: Modifiers) -> Modifiers {
    let mut out = Modifiers { alt: m.alt, shift: m.shift, ctrl: m.ctrl, ..Default::default() };
    if m.command {
        out.command = true;
        if cfg!(target_os = "macos") {
            out.mac_cmd = true;
        } else {
            out.ctrl = true;
        }
    }
    out
}
