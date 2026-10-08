//! local-image: the Contextual Task Bar, as in Photoshop: a small floating bar under what you just
//! made that offers the next step.
//!
//! * **After a selection** (any selection tool, Select Subject, or a pen path turned into a
//!   selection): **Remove** (AI Remove on the selection), **Generative Fill** (an inline prompt),
//!   **Invert**, **Mask** (a layer mask from the selection), **Deselect**, and *More* (Feather,
//!   Select and Mask, Content-Aware Fill, Hide Bar).
//! * **After a closed pen path** (no selection yet): **Make Selection**, **Remove** (makes the
//!   selection and removes it in one step), **Generative Fill**, **Mask**.
//!
//! The bar sits centred under the selection's (or path's) bounds, flips above them when there is
//! no room below, and stays inside the canvas. It hides while the pointer is down so it never
//! covers a drag, and Window › Contextual Task Bar turns it off.

use std::cell::RefCell;

use egui::{Color32, Sense, Stroke, vec2};
use serde_json::{Value, json};

use crate::PhotocraftApp;
use crate::canvas::ViewXform;
use crate::theme::Tokens;

pub const TOGGLE_ID: &str = "window.toggle.contextualTaskBar";

#[derive(Default)]
struct State {
    /// `(doc id, revision)` → the selection's bounds in document pixels.
    bounds: Option<((u64, u64), Option<photocraft_geom::Rect>)>,
    /// The Generative Fill prompt is open.
    fill_open: bool,
    fill_prompt: String,
    focus: bool,
}

thread_local! { static STATE: RefCell<State> = RefCell::new(State::default()); }

fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with(|s| f(&mut s.borrow_mut()))
}

pub fn enabled(app: &PhotocraftApp) -> bool {
    app.session.prefs().interface.contextual_task_bar
}

/// Window › Contextual Task Bar.
pub fn menu(app: &mut PhotocraftApp, id: &str) -> Option<Result<Value, String>> {
    (id == TOGGLE_ID).then(|| {
        let on = !enabled(app);
        app.session.prefs.edit(|p| p.interface.contextual_task_bar = on);
        Ok(json!({ "visible": on }))
    })
}

pub fn checked(app: &PhotocraftApp, id: &str) -> Option<bool> {
    (id == TOGGLE_ID).then(|| enabled(app))
}

/// What the bar is for right now.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Context {
    Selection,
    Path,
}

/// The selection's bounds (cached per document revision).
fn selection_bounds(app: &PhotocraftApp) -> Option<photocraft_geom::Rect> {
    let d = app.session.active()?;
    let sel = d.doc.selection.as_ref()?;
    let key = (d.doc.id.0, d.revision);
    with(|s| {
        if let Some((k, b)) = &s.bounds
            && *k == key
        {
            return *b;
        }
        let r = sel.content_bounds();
        let b = (!r.is_empty()).then_some(r);
        s.bounds = Some((key, b));
        b
    })
}

/// The bounds of the work path when it has a closed subpath.
fn path_bounds(app: &PhotocraftApp) -> Option<photocraft_geom::Rect> {
    let d = app.session.active()?;
    let path = d.doc.work_path.as_ref()?;
    let closed: Vec<_> = path.subpaths.iter().filter(|s| s.closed && s.knots.len() >= 3).collect();
    if closed.is_empty() {
        return None;
    }
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for k in closed.iter().flat_map(|s| s.knots.iter()) {
        for p in [k.anchor, k.in_ctrl, k.out_ctrl] {
            x0 = x0.min(p.x);
            y0 = y0.min(p.y);
            x1 = x1.max(p.x);
            y1 = y1.max(p.y);
        }
    }
    Some(photocraft_geom::Rect::new(x0.floor() as i32, y0.floor() as i32, x1.ceil() as i32, y1.ceil() as i32))
}

/// When the bar shows, and for what.
pub fn context(app: &PhotocraftApp) -> Option<(Context, photocraft_geom::Rect)> {
    if !enabled(app) {
        return None;
    }
    if let Some(r) = selection_bounds(app) {
        return Some((Context::Selection, r));
    }
    let pen = matches!(app.ui.tool, crate::Tool::Pen | crate::Tool::PathSelection | crate::Tool::DirectSelection);
    if pen && let Some(r) = path_bounds(app) {
        return Some((Context::Path, r));
    }
    None
}

fn report(app: &mut PhotocraftApp, r: Result<Value, String>) {
    if let Err(e) = r {
        app.ui.status = e;
        app.ui.status_error = true;
    }
}

/// A bar button: icon and label, quiet until hovered; `primary` gets the accent.
fn bar_button(ui: &mut egui::Ui, icon: &str, label: &str, primary: bool, enabled: bool, t: &Tokens) -> egui::Response {
    let ink = if !enabled {
        t.text_faint
    } else if primary {
        t.primary_text
    } else {
        t.text
    };
    let galley = ui.painter().layout_no_wrap(label.to_owned(), crate::theme::medium(12.0), ink);
    let w = galley.size().x + if icon.is_empty() { 16.0 } else { 34.0 };
    let (r, resp) = ui.allocate_exact_size(vec2(w, 26.0), if enabled { Sense::click() } else { Sense::hover() });
    let fill = match (primary, resp.hovered() && enabled) {
        (true, false) => t.primary_bg,
        (true, true) => t.accent,
        (false, true) => t.hover,
        (false, false) => Color32::TRANSPARENT,
    };
    ui.painter().rect_filled(r, t.radius_sm, fill);
    let mut x = r.left() + 8.0;
    if !icon.is_empty() {
        crate::icons::paint(ui, egui::Rect::from_min_size(egui::pos2(x, r.center().y - 8.0), vec2(16.0, 16.0)), icon, 14.0, ink);
        x += 22.0;
    }
    ui.painter().galley(egui::pos2(x, r.center().y - galley.size().y / 2.0), galley, ink);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, label));
    resp
}

fn divider(ui: &mut egui::Ui, t: &Tokens) {
    let (r, _) = ui.allocate_exact_size(vec2(1.0, 18.0), Sense::hover());
    ui.painter().vline(r.center().x, r.y_range(), Stroke::new(1.0, t.separator));
}

/// The AI Remove engine chosen on the AI Remove tool's options bar, and whether it is ready.
fn remove_ready(app: &PhotocraftApp) -> Result<(), String> {
    let (m, v) = crate::ai_ui::remove_engine(&app.ui.ai.remove_engine);
    crate::ai_ui::status().ready(m, v)
}

/// Draws the bar (call every frame after the canvas).
pub fn show(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let Some((what, bounds)) = context(app) else {
        with(|s| s.fill_open = false);
        return;
    };
    // Never over a drag, a text edit on the canvas, or a modal.
    if ctx.input(|i| i.pointer.any_down()) || app.session.active().is_none() || ctx.memory(|m| m.top_modal_layer().is_some()) {
        return;
    }
    if app.ui.text_edit.is_some() {
        return;
    }
    let Some(xf) = ViewXform::active(app) else { return };
    let canvas = xf.rect;
    let sel = xf.doc_rect(bounds);
    if !sel.intersects(canvas) {
        return;
    }
    let t = Tokens::get(ctx);
    let (fill_open, mut prompt, focus) = with(|s| (s.fill_open, s.fill_prompt.clone(), std::mem::take(&mut s.focus)));
    let id = egui::Id::new("li-context-bar");
    // Size from last frame (first frame: a guess), so the bar can be centred and kept in view.
    let size = ctx.memory(|m| m.area_rect(id)).map(|r| r.size()).unwrap_or(vec2(if fill_open { 460.0 } else { 420.0 }, 38.0));
    let gap = 12.0;
    let mut y = sel.bottom() + gap;
    if y + size.y > canvas.bottom() - 8.0 {
        y = (sel.top() - gap - size.y).max(canvas.top() + 8.0);
    }
    let x = (sel.center().x - size.x / 2.0).clamp(canvas.left() + 8.0, (canvas.right() - size.x - 8.0).max(canvas.left() + 8.0));
    let ready = remove_ready(app);
    // Tooltips name the user's own shortcuts (Edit › Keyboard Shortcuts), in the platform's notation.
    let with_key = |text: &str, command: &str| match crate::shortcuts::shortcut_label(app, command) {
        Some(k) => format!("{text}  ({k})"),
        None => text.to_owned(),
    };
    let make_selection_tip = with_key(tl!("Turn the path into a selection"), "path.toSelection");
    let invert_tip = with_key(tl!("Inverse"), "select.inverse");
    let deselect_tip = with_key(tl!("Deselect"), "select.deselect");
    let mut action: Option<&'static str> = None;
    egui::Area::new(id).order(egui::Order::Foreground).fixed_pos(egui::pos2(x, y)).constrain(false).show(ctx, |ui| {
        egui::Frame::new()
            .fill(t.card)
            .stroke(Stroke::new(1.0, t.card_border))
            .corner_radius(t.radius)
            .shadow(egui::Shadow { offset: [0, 3], blur: 12, spread: 0, color: t.shadow })
            .inner_margin(egui::Margin::symmetric(5, 5))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    if fill_open {
                        let (r, _) = ui.allocate_exact_size(vec2(22.0, 26.0), Sense::hover());
                        crate::icons::paint(ui, r, "sparkles", 14.0, t.accent);
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut prompt)
                                .hint_text(tl!("Describe what to generate (optional)"))
                                .desired_width(280.0)
                                .margin(egui::Margin::symmetric(6, 4)),
                        );
                        if focus {
                            resp.request_focus();
                        }
                        let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if bar_button(ui, "", tl!("Generate"), true, true, &t)
                            .on_hover_text(tl!("Generative Fill: regenerate the selection from your description"))
                            .clicked()
                            || enter
                        {
                            action = Some("fill.go");
                        }
                        if crate::icons::button(ui, "x", 26.0, false, tl!("Back")).clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                            action = Some("fill.close");
                        }
                        return;
                    }
                    let remove_tip = match &ready {
                        Ok(()) => tl!("Remove what's inside with AI (on its own layer)").to_owned(),
                        Err(why) => crate::i18n::fmt(tl!("AI Remove isn't ready: {why}"), &[("why", why)]),
                    };
                    if what == Context::Path {
                        if bar_button(ui, "square-dashed", tl!("Make Selection"), false, true, &t).on_hover_text(&make_selection_tip).clicked() {
                            action = Some("path.select");
                        }
                        divider(ui, &t);
                    }
                    if bar_button(ui, "eraser-magic", tl!("Remove"), true, ready.is_ok(), &t)
                        .on_hover_text(&remove_tip)
                        .on_disabled_hover_text(&remove_tip)
                        .clicked()
                    {
                        action = Some("remove");
                    }
                    if bar_button(ui, "sparkles", tl!("Generative Fill"), false, true, &t)
                        .on_hover_text(tl!("Regenerate the area from a description"))
                        .clicked()
                    {
                        action = Some("fill.open");
                    }
                    divider(ui, &t);
                    if what == Context::Selection && bar_button(ui, "squares-subtract", tl!("Invert"), false, true, &t).on_hover_text(&invert_tip).clicked() {
                        action = Some("invert");
                    }
                    if bar_button(ui, "layers", tl!("Mask"), false, true, &t).on_hover_text(tl!("Add a layer mask that shows only this area")).clicked() {
                        action = Some("mask");
                    }
                    if what == Context::Selection && crate::icons::button(ui, "x", 26.0, false, &deselect_tip).clicked() {
                        action = Some("deselect");
                    }
                    let more = crate::icons::button(ui, "ellipsis", 26.0, false, tl!("More"));
                    egui::Popup::menu(&more).show(|ui| {
                        ui.set_min_width(190.0);
                        if what == Context::Selection {
                            for (label, a) in [(tl!("Feather…"), "feather"), (tl!("Select and Mask…"), "selectAndMask"), (tl!("Content-Aware Fill…"), "caf")]
                            {
                                if ui.button(label).clicked() {
                                    action = Some(a);
                                    ui.close();
                                }
                            }
                            ui.separator();
                        }
                        if ui.button(tl!("Hide Contextual Task Bar")).clicked() {
                            action = Some("hide");
                            ui.close();
                        }
                    });
                });
            });
    });
    with(|s| s.fill_prompt = prompt.clone());
    let Some(a) = action else { return };
    // A path's actions start from its selection.
    let from_path = what == Context::Path && matches!(a, "path.select" | "remove" | "fill.go" | "mask");
    if from_path {
        let r = app.run("path.toSelection", json!({ "name": "work" }));
        if r.is_err() {
            report(app, r);
            return;
        }
    }
    let r = match a {
        "path.select" => Ok(Value::Null),
        "remove" => {
            let engine = app.ui.ai.remove_engine.clone();
            app.run("ai.remove", json!({ "engine": engine }))
        }
        "fill.open" => {
            with(|s| {
                s.fill_open = true;
                s.focus = true;
            });
            Ok(Value::Null)
        }
        "fill.close" => {
            with(|s| s.fill_open = false);
            Ok(Value::Null)
        }
        "fill.go" => {
            with(|s| s.fill_open = false);
            app.ui.ai.fill_prompt = prompt.clone();
            app.run("ai.generativeFill", json!({ "prompt": prompt }))
        }
        "invert" => app.run("select.inverse", json!({})),
        "mask" => crate::menus::invoke(app, ctx, "layer.layerMask.revealSelection", json!({})),
        "deselect" => app.run("select.deselect", json!({})),
        "feather" => crate::menus::invoke(app, ctx, "select.modify.feather", json!({})),
        "selectAndMask" => crate::menus::invoke(app, ctx, "select.selectAndMask", json!({})),
        "caf" => crate::menus::invoke(app, ctx, "edit.contentAwareFill", json!({})),
        "hide" => {
            app.session.prefs.edit(|p| p.interface.contextual_task_bar = false);
            app.ui.status = tl!("Contextual Task Bar hidden — Window › Contextual Task Bar shows it again.").into();
            app.ui.status_error = false;
            Ok(Value::Null)
        }
        _ => Ok(Value::Null),
    };
    report(app, r);
}

#[cfg(test)]
mod tests {
    use super::*;
    use photocraft_doc::{Color, ColorMode, Document, SampleType, Size};

    fn app() -> PhotocraftApp {
        let doc = Document::with_background("bar", Size::new(120, 90), ColorMode::Rgb, SampleType::U8, Color::WHITE);
        let mut s = photocraft_engine::Session::new();
        s.add_document(doc, None);
        PhotocraftApp::new(s, crate::Services::default())
    }

    /// A closed pen path offers the bar (with the Pen tools), its Make Selection turns into the
    /// selection bar, and Deselect or the Window toggle hides it.
    #[test]
    fn shows_after_a_closed_path_and_after_a_selection() {
        let mut app = app();
        assert_eq!(context(&app), None);
        let square = json!({"subpaths": [{"closed": true, "knots": [[10, 10], [60, 10], [60, 60], [10, 60]]}]});
        app.run("path.set", json!({ "path": square })).unwrap();
        app.ui.tool = crate::Tool::Move;
        assert_eq!(context(&app), None, "a path only offers the bar while a Pen tool is active");
        app.ui.tool = crate::Tool::Pen;
        let (what, r) = context(&app).unwrap();
        assert_eq!(what, Context::Path);
        assert_eq!((r.x0, r.y0, r.x1, r.y1), (10, 10, 60, 60));
        // An open path doesn't.
        app.run("path.set", json!({ "path": {"subpaths": [{"closed": false, "knots": [[10, 10], [60, 10], [60, 60]]}]} })).unwrap();
        assert_eq!(context(&app), None);
        app.run("path.set", json!({ "path": square })).unwrap();
        app.run("path.toSelection", json!({ "name": "work" })).unwrap();
        assert_eq!(context(&app).map(|c| c.0), Some(Context::Selection));
        let _ = menu(&mut app, TOGGLE_ID);
        assert_eq!(context(&app), None, "Window › Contextual Task Bar turns it off");
        assert_eq!(checked(&app, TOGGLE_ID), Some(false));
        let _ = menu(&mut app, TOGGLE_ID);
        app.run("select.deselect", json!({})).unwrap();
        assert_eq!(context(&app).map(|c| c.0), Some(Context::Path), "deselected, the path offers it again");
    }
}
