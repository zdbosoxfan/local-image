//! local-image: the Perspective Crop tool (C group), as in Photoshop. Drag a frame, then drag its
//! four corners onto the edges of something photographed at an angle (a page, a painting, a
//! sign); Enter (or ✓) straightens that area into the new canvas, Esc (or ✕) cancels. A grid inside
//! the frame shows how the perspective will be corrected.

use std::cell::RefCell;

use egui::{Color32, Modifiers, Pos2, Stroke, vec2};
use serde_json::json;

use crate::PhotocraftApp;
use crate::canvas::{ToolEvent, ViewXform};
use crate::state::Tool;

/// Corner handles grab within this many screen pixels.
const HANDLE_PX: f64 = 9.0;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Drag {
    Draw([f64; 2]),
    Corner(usize),
}

#[derive(Default)]
struct State {
    /// Corners clockwise from top-left (document pixels).
    quad: Option<[[f64; 2]; 4]>,
    drag: Option<Drag>,
    /// The document the frame belongs to.
    doc: Option<u64>,
}

thread_local! { static STATE: RefCell<State> = RefCell::new(State::default()); }

fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with(|s| f(&mut s.borrow_mut()))
}

fn doc_id(app: &PhotocraftApp) -> Option<u64> {
    app.session.active().map(|st| st.doc.id.0)
}

/// The frame for the active document, if one is drawn.
pub fn quad(app: &PhotocraftApp) -> Option<[[f64; 2]; 4]> {
    let id = doc_id(app);
    with(|s| s.quad.filter(|_| s.doc == id))
}

fn rect_quad(a: [f64; 2], b: [f64; 2]) -> [[f64; 2]; 4] {
    let (x0, x1) = (a[0].min(b[0]), a[0].max(b[0]));
    let (y0, y1) = (a[1].min(b[1]), a[1].max(b[1]));
    [[x0, y0], [x1, y0], [x1, y1], [x0, y1]]
}

/// Pointer input. Returns true when the event was the tool's.
pub fn pointer(app: &mut PhotocraftApp, ev: ToolEvent, _mods: Modifiers) -> bool {
    if app.ui.tool != Tool::PerspectiveCrop {
        with(|s| s.drag = None);
        return false;
    }
    let p = match ev {
        ToolEvent::Down { x, y, .. } | ToolEvent::Move { x, y, .. } | ToolEvent::Up { x, y } => [x, y],
    };
    if app.session.active().is_none() || !p[0].is_finite() || !p[1].is_finite() {
        return true;
    }
    let tol = HANDLE_PX / (app.current_zoom() as f64).max(0.01);
    let id = doc_id(app);
    with(|s| {
        if s.doc != id {
            *s = State { doc: id, ..State::default() };
        }
        match ev {
            ToolEvent::Down { .. } => {
                let corner = s.quad.and_then(|q| (0..4).find(|&i| (q[i][0] - p[0]).hypot(q[i][1] - p[1]) <= tol));
                s.drag = Some(match corner {
                    Some(i) => Drag::Corner(i),
                    None => Drag::Draw(p),
                });
            }
            ToolEvent::Move { .. } | ToolEvent::Up { .. } => {
                match s.drag {
                    Some(Drag::Draw(a)) if (a[0] - p[0]).abs() > 0.5 || (a[1] - p[1]).abs() > 0.5 => s.quad = Some(rect_quad(a, p)),
                    Some(Drag::Corner(i)) => {
                        if let Some(q) = s.quad.as_mut() {
                            q[i] = p;
                        }
                    }
                    _ => {}
                }
                if matches!(ev, ToolEvent::Up { .. }) {
                    // A click without a drag keeps the frame; a tiny one is dropped.
                    if let (Some(Drag::Draw(_)), Some(q)) = (s.drag, s.quad)
                        && ((q[1][0] - q[0][0]).abs() < 2.0 || (q[3][1] - q[0][1]).abs() < 2.0)
                    {
                        s.quad = None;
                    }
                    s.drag = None;
                }
            }
        }
    });
    true
}

/// Straightens the framed area into the new canvas.
pub fn commit(app: &mut PhotocraftApp) {
    let Some(q) = quad(app) else { return };
    match app.run("image.perspectiveCrop", json!({ "quad": q })) {
        Ok(v) => {
            app.ui.status = format!("Perspective Crop: {} × {} px", v["width"], v["height"]);
            app.ui.status_error = false;
            cancel();
        }
        Err(e) => {
            app.ui.status = e;
            app.ui.status_error = true;
        }
    }
}

pub fn cancel() {
    with(|s| {
        s.quad = None;
        s.drag = None;
    });
}

/// Enter commits and Esc cancels while the tool has a frame. True when a key was taken.
pub fn keys(app: &mut PhotocraftApp, ctx: &egui::Context) -> bool {
    if app.ui.tool != Tool::PerspectiveCrop || quad(app).is_none() {
        return false;
    }
    if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, egui::Key::Enter)) {
        commit(app);
        true
    } else if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, egui::Key::Escape)) {
        cancel();
        true
    } else {
        false
    }
}

/// The frame, its perspective grid and corner handles.
pub fn draw(app: &PhotocraftApp, painter: &egui::Painter, xf: &ViewXform) {
    if app.ui.tool != Tool::PerspectiveCrop {
        return;
    }
    let Some(q) = quad(app) else { return };
    let s: Vec<Pos2> = q.iter().map(|c| xf.to_screen(c[0] as f32, c[1] as f32)).collect();
    // The image outside the frame dims, like the Crop tool's.
    let outline = [s[0], s[1], s[2], s[3]];
    painter.add(egui::Shape::convex_polygon(outline.to_vec(), Color32::TRANSPARENT, Stroke::new(3.0, Color32::from_black_alpha(140))));
    painter.add(egui::Shape::closed_line(outline.to_vec(), Stroke::new(1.0, Color32::WHITE)));
    // Grid: lines between matching points on opposite edges (the perspective's thirds).
    let lerp = |a: Pos2, b: Pos2, t: f32| a + (b - a) * t;
    let thin = Stroke::new(1.0, Color32::from_white_alpha(110));
    for t in [1.0 / 3.0, 2.0 / 3.0] {
        painter.line_segment([lerp(s[0], s[1], t), lerp(s[3], s[2], t)], thin);
        painter.line_segment([lerp(s[0], s[3], t), lerp(s[1], s[2], t)], thin);
    }
    for c in &s {
        let r = egui::Rect::from_center_size(*c, vec2(8.0, 8.0));
        painter.rect_filled(r, 0.0, Color32::WHITE);
        painter.rect_stroke(r, 0.0, Stroke::new(1.0, Color32::BLACK), egui::StrokeKind::Outside);
    }
}

/// Options bar: what to do, the size it will produce, and the commit and cancel buttons.
pub fn options_bar(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let t = crate::theme::Tokens::get(ui.ctx());
    match quad(app) {
        None => {
            ui.label(egui::RichText::new(tl!("Drag a frame, then drag its corners onto the edges of the object to straighten")).color(t.text_dim));
        }
        Some(q) => {
            let (w, h) = photocraft_engine::perspective_crop_cmds::output_size(&q);
            ui.label(egui::RichText::new(format!("{w} × {h} px")).color(t.text_dim));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if crate::icons::button(
                    ui,
                    "check",
                    26.0,
                    false,
                    &crate::i18n::fmt(tl!("Commit current crop operation  ({key})"), &[("key", &crate::shortcuts::pretty("Enter"))]),
                )
                .clicked()
                {
                    commit(app);
                }
                if crate::icons::button(ui, "ban", 26.0, false, tl!("Cancel current crop operation  (Esc)")).clicked() {
                    cancel();
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::tool_event;
    use photocraft_doc::{Color, ColorMode, Document, SampleType, Size};

    #[test]
    fn draw_adjust_and_commit() {
        let doc = Document::with_background("persp", Size::new(200, 160), ColorMode::Rgb, SampleType::U8, Color::WHITE);
        let mut s = photocraft_engine::Session::new();
        s.add_document(doc, None);
        let mut app = PhotocraftApp::new(s, crate::Services::default());
        app.ui.tool = Tool::PerspectiveCrop;
        app.ui.extras.snap = false;
        let ev = |app: &mut PhotocraftApp, e: ToolEvent| tool_event(app, e, Modifiers::NONE);
        ev(&mut app, ToolEvent::Down { x: 40.0, y: 30.0, pressure: 1.0 });
        ev(&mut app, ToolEvent::Move { x: 160.0, y: 130.0, pressure: 1.0 });
        ev(&mut app, ToolEvent::Up { x: 160.0, y: 130.0 });
        assert_eq!(quad(&app), Some([[40.0, 30.0], [160.0, 30.0], [160.0, 130.0], [40.0, 130.0]]));
        // Pull the top-right corner down: a perspective frame.
        ev(&mut app, ToolEvent::Down { x: 160.0, y: 30.0, pressure: 1.0 });
        ev(&mut app, ToolEvent::Move { x: 150.0, y: 50.0, pressure: 1.0 });
        ev(&mut app, ToolEvent::Up { x: 150.0, y: 50.0 });
        assert_eq!(quad(&app).unwrap()[1], [150.0, 50.0]);
        commit(&mut app);
        let d = &app.session.active().unwrap().doc;
        let (w, h) = photocraft_engine::perspective_crop_cmds::output_size(&[[40.0, 30.0], [150.0, 50.0], [160.0, 130.0], [40.0, 130.0]]);
        assert_eq!((d.size.width, d.size.height), (w, h));
        assert!(quad(&app).is_none());
    }
}
