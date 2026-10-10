//! local-image: the Contextual Task Bar, as in Photoshop: a small floating bar under the selected
//! item that offers the next step. What it offers follows the *item*, whatever tool made it:
//!
//! * **A selection** (any selection tool, Select Subject, a path made into a selection):
//!   **Remove** (AI Remove), **Generative Fill** (an inline prompt), **Content-Aware Fill**,
//!   **Invert**, **Mask**, **Deselect**, and *More* (Feather, Select and Mask, the Content-Aware
//!   Fill options, Hide Bar).
//! * **A closed pen path** (no selection yet): **Make Selection**, **Content-Aware Fill**,
//!   **AI Fill** and **Mask** (a vector mask from the path).
//! * **The selected layer**, by kind: a shape's Fill, Stroke and Radius with Edit Path and
//!   Rasterize; type's font, size and colour with Edit Text; Edit Contents for a Smart Object;
//!   Open in Develop for a Develop layer; Regenerate for an AI-generated layer; Select Subject and
//!   Remove Background for pixels.
//!
//! A selection or path takes precedence while it exists, like Photoshop. The bar sits centred
//! under the item's bounds, flips above them when there is no room below, and stays inside the
//! canvas. It hides while a press that began elsewhere (a drag on the canvas) is held, but never
//! while one of its own buttons or popups is pressed, so its clicks land. Window › Contextual
//! Task Bar turns it off.
//!
//! [`layer_options`] gives the options bar the same resolver: with the Move or Path Selection
//! tool and a shape or type layer selected, it shows that layer's options.

use std::cell::RefCell;

use egui::{Color32, Sense, Stroke, vec2};
use photocraft_doc::{LayerContent, LayerId};
use serde_json::{Value, json};

use crate::PhotocraftApp;
use crate::canvas::ViewXform;
use crate::theme::Tokens;

pub const TOGGLE_ID: &str = "window.toggle.contextualTaskBar";

#[derive(Default)]
struct State {
    /// `(doc id, revision, layer)` → the selection's (layer 0) or a layer's bounds in document pixels.
    bounds: Vec<((u64, u64, u64), Option<photocraft_geom::Rect>)>,
    /// The Generative Fill prompt is open.
    fill_open: bool,
    fill_prompt: String,
    focus: bool,
    /// `(doc id, layer)` last seen active, to reveal Properties when a shape or type layer is selected.
    last_active: Option<(u64, u64)>,
    /// Whether the current press began away from the bar. Keep this decision for a drag,
    /// even when edits change the item's bounds.
    pressed_elsewhere: Option<bool>,
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

/// What the bar is for right now: the selected item.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Context {
    Selection,
    Path,
    Shape(LayerId),
    Text(LayerId),
    Smart(LayerId),
    Develop(LayerId),
    /// A pixel layer an AI command made (it keeps its prompt and seed: `Layer::generation`).
    Generated(LayerId),
    Pixel(LayerId),
}

/// Cached bounds per document revision.
fn cached(app: &PhotocraftApp, layer: u64, f: impl FnOnce() -> photocraft_geom::Rect) -> Option<photocraft_geom::Rect> {
    let d = app.session.active()?;
    let key = (d.doc.id.0, d.revision, layer);
    if let Some(b) = with(|s| s.bounds.iter().find(|(k, _)| *k == key).map(|(_, b)| *b)) {
        return b;
    }
    let r = f();
    let b = (!r.is_empty()).then_some(r);
    with(|s| {
        // Only the current revision is worth keeping.
        s.bounds.retain(|((doc, rev, _), _)| *doc == key.0 && *rev == key.1);
        s.bounds.push((key, b));
    });
    b
}

/// The selection's bounds.
fn selection_bounds(app: &PhotocraftApp) -> Option<photocraft_geom::Rect> {
    let sel = app.session.active()?.doc.selection.as_ref()?;
    cached(app, 0, || sel.content_bounds())
}

/// The selected path, including saved paths and targeted vector masks. A shape retains its
/// shape context; an explicitly selected work/saved path takes precedence over it.
fn selected_path(app: &PhotocraftApp) -> Option<(String, photocraft_doc::vector::Path)> {
    let d = app.session.active()?;
    let layer = d.active_layer.and_then(|id| d.doc.layer(id));
    let named = |name: &str| {
        if name.eq_ignore_ascii_case("work") || name == "Work Path" {
            d.doc.work_path.clone().map(|p| ("work".into(), p))
        } else if name == "layer" {
            layer?.vector_mask.as_ref().map(|m| ("layer".into(), m.path.clone()))
        } else {
            d.doc.paths.iter().find(|p| p.name == name).map(|p| (name.to_owned(), p.path.clone()))
        }
    };
    if let Some(path) = app.ui.selected_path.as_deref().and_then(named) {
        return Some(path);
    }
    if app.ui.vector_mask_target {
        return named("layer");
    }
    if matches!(app.ui.tool, crate::Tool::Pen | crate::Tool::PathSelection | crate::Tool::DirectSelection)
        && !layer.is_some_and(|l| matches!(l.content, LayerContent::Shape(_)))
    {
        return named("work");
    }
    None
}

/// The bounds of the selected path when it has a closed subpath.
fn path_bounds(app: &PhotocraftApp) -> Option<photocraft_geom::Rect> {
    let (_, path) = selected_path(app)?;
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

/// The selected layer's context, from its kind (whatever tool is active).
pub fn layer_context(app: &PhotocraftApp) -> Option<Context> {
    let d = app.session.active()?;
    let id = d.active_layer?;
    let l = d.doc.layer(id)?;
    Some(match &l.content {
        LayerContent::Shape(_) => Context::Shape(id),
        LayerContent::Text(_) => Context::Text(id),
        LayerContent::Smart(_) if crate::develop_layer::is_develop_layer(l) => Context::Develop(id),
        LayerContent::Smart(_) => Context::Smart(id),
        LayerContent::Raster(_) if l.generation.is_some() => Context::Generated(id),
        // The Background shows the document (as the Properties panel does), not a layer bar.
        LayerContent::Raster(_) if !crate::doc_props_ui::is_background(&d.doc, l) => Context::Pixel(id),
        _ => return None,
    })
}

/// What the bar is for and where: a selection, then the selected path, then the selected layer.
/// `show` keeps the bar out of the way while a canvas stroke/drag is held.
pub fn resolve(app: &PhotocraftApp) -> Option<(Context, photocraft_geom::Rect)> {
    if let Some(r) = selection_bounds(app) {
        return Some((Context::Selection, r));
    }
    if let Some(r) = path_bounds(app) {
        return Some((Context::Path, r));
    }
    let what = layer_context(app)?;
    let d = app.session.active()?;
    let id = d.active_layer?;
    let layer = d.doc.layer(id)?;
    cached(app, id.0, || {
        if let Some(bounds) = layer.surface().map(|s| s.content_bounds()).filter(|r| !r.is_empty()) {
            return bounds;
        }
        // An unpainted shape still has editable geometry; missing previews and empty layers
        // still have useful actions. Their context must not depend on a raster cache.
        if let LayerContent::Shape(sh) = &layer.content
            && let Some((x0, y0, x1, y1)) = sh.path.control_bounds()
        {
            return photocraft_geom::Rect::new(
                x0.floor() as i32,
                y0.floor() as i32,
                (x1.ceil() as i32).max((x0.floor() as i32).saturating_add(1)),
                (y1.ceil() as i32).max((y0.floor() as i32).saturating_add(1)),
            );
        }
        d.doc.bounds()
    })
    .map(|r| (what, r))
}

/// When the bar shows, and for what.
pub fn context(app: &PhotocraftApp) -> Option<(Context, photocraft_geom::Rect)> {
    if !enabled(app) {
        return None;
    }
    resolve(app)
}

/// Whether [`layer_options`] has options for the selected layer.
pub fn has_layer_options(app: &PhotocraftApp) -> bool {
    matches!(layer_context(app), Some(Context::Shape(_) | Context::Text(_)))
}

/// The options bar for the selected layer's kind, bound to it (shape or type), for tools without
/// options of their own for it (Move, Path Selection). False when the layer is neither.
pub fn layer_options(app: &mut PhotocraftApp, ui: &mut egui::Ui) -> bool {
    match layer_context(app) {
        Some(Context::Shape(_)) => crate::vector_ui::shape_layer_bar(app, ui),
        Some(Context::Text(_)) => {
            crate::type_tool::options_bar(app, ui);
            true
        }
        _ => false,
    }
}

/// Photoshop brings Properties forward when a shape or type layer is selected (or a shape is
/// drawn): its controls are where that layer is edited.
fn reveal_properties(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let Some(d) = app.session.active() else { return };
    let now = d.active_layer.map(|id| (d.doc.id.0, id.0));
    if with(|s| s.last_active) == now {
        return;
    }
    // Expanding a dock group moves Layers rows. Allow the current click sequence to finish
    // first, so the second click of a thumbnail double-click lands on the same row.
    let since_click = f64::from(ctx.input(|i| i.pointer.time_since_last_click()));
    let delay = ctx.options(|o| o.input_options.max_double_click_delay);
    if since_click < delay {
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(delay - since_click));
        return;
    }
    with(|s| s.last_active = now);
    if matches!(layer_context(app), Some(Context::Shape(_) | Context::Text(_))) {
        crate::dock::reveal(app, crate::dock::Group::Properties);
        app.ui.dock_tabs.properties = 0;
    }
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

fn label(ui: &mut egui::Ui, s: &str, t: &Tokens) {
    ui.label(egui::RichText::new(s).color(t.text_dim).size(12.0));
}

/// The AI Remove engine chosen on the AI Remove tool's options bar, and whether it is ready.
fn remove_ready(app: &PhotocraftApp) -> Result<(), String> {
    let (m, v) = crate::ai_ui::remove_engine(&app.ui.ai.remove_engine);
    crate::ai_ui::status().ready(m, v)
}

/// The bar's id (its area).
fn bar_id() -> egui::Id {
    egui::Id::new("li-context-bar")
}

/// A press is held that began somewhere other than the bar or a popup above it (a drag on the
/// canvas, a panel): the bar keeps out of its way. A press on the bar itself must keep it drawn,
/// or egui drops the press when its widget vanishes and the button never clicks.
fn pressed_elsewhere(ctx: &egui::Context) -> bool {
    let Some(origin) = ctx.input(|i| if i.pointer.any_down() { i.pointer.press_origin() } else { None }) else {
        with(|s| s.pressed_elsewhere = None);
        return false;
    };
    with(|s| {
        *s.pressed_elsewhere.get_or_insert_with(|| {
            !ctx.memory(|m| m.area_rect(bar_id())).is_some_and(|r| r.contains(origin))
                && !ctx.layer_id_at(origin).is_some_and(|l| matches!(l.order, egui::Order::Foreground | egui::Order::Tooltip | egui::Order::Debug))
        })
    })
}

/// What a bar click asks for, applied after drawing.
enum Act {
    /// A named step (see [`show`]).
    Do(&'static str),
    /// `shape.edit` on the layer.
    Shape(Option<Value>),
}

/// Draws the bar (call every frame after the canvas).
pub fn show(app: &mut PhotocraftApp, ctx: &egui::Context) {
    if app.ai_remove.is_some() {
        return;
    }
    reveal_properties(app, ctx);
    let pressed_elsewhere = pressed_elsewhere(ctx);
    let Some((what, bounds)) = context(app) else {
        with(|s| s.fill_open = false);
        return;
    };
    // Never over a drag, a text edit on the canvas, or a modal.
    if pressed_elsewhere || app.session.active().is_none() || ctx.memory(|m| m.top_modal_layer().is_some()) {
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
    // The prompt belongs to a selection or path.
    let fill_open = with(|s| {
        s.fill_open &= matches!(what, Context::Selection | Context::Path);
        s.fill_open
    });
    let (mut prompt, focus) = with(|s| (s.fill_prompt.clone(), std::mem::take(&mut s.focus)));
    let id = bar_id();
    // Size from last frame (first frame: a guess), so the bar can be centred and kept in view.
    let size = ctx.memory(|m| m.area_rect(id)).map(|r| r.size()).unwrap_or(vec2(if fill_open { 460.0 } else { 420.0 }, 38.0));
    let gap = 12.0;
    let mut y = sel.bottom() + gap;
    if y + size.y > canvas.bottom() - 8.0 {
        y = (sel.top() - gap - size.y).max(canvas.top() + 8.0);
    }
    let x = (sel.center().x - size.x / 2.0).clamp(canvas.left() + 8.0, (canvas.right() - size.x - 8.0).max(canvas.left() + 8.0));
    let pos = if ctx.input(|i| i.pointer.any_down()) { ctx.memory(|m| m.area_rect(id)).map(|r| r.min).unwrap_or(egui::pos2(x, y)) } else { egui::pos2(x, y) };
    let ready = remove_ready(app);
    // Tooltips name the user's own shortcuts (Edit › Keyboard Shortcuts), in the platform's notation.
    let with_key = |text: &str, command: &str| match crate::shortcuts::shortcut_label(app, command) {
        Some(k) => format!("{text}  ({k})"),
        None => text.to_owned(),
    };
    let make_selection_tip = with_key(tl!("Turn the path into a selection"), "path.toSelection");
    let invert_tip = with_key(tl!("Inverse"), "select.inverse");
    let deselect_tip = with_key(tl!("Deselect"), "select.deselect");
    let generation = match what {
        Context::Generated(l) => app.session.active().and_then(|d| d.doc.layer(l)).and_then(|l| l.generation.clone()),
        _ => None,
    };
    let shape = match what {
        Context::Shape(l) => crate::vector_ui::active_shape(app).filter(|(id, _)| *id == l),
        _ => None,
    };
    let mut act: Option<Act> = None;
    egui::Area::new(id).order(egui::Order::Foreground).fixed_pos(pos).constrain(false).show(ctx, |ui| {
        egui::Frame::new()
            .fill(t.card)
            .stroke(Stroke::new(1.0, t.card_border))
            .corner_radius(t.radius)
            .shadow(egui::Shadow { offset: [0, 3], blur: 12, spread: 0, color: t.shadow })
            .inner_margin(egui::Margin::symmetric(5, 5))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    let mut go = |a: &'static str| act = Some(Act::Do(a));
                    if fill_open {
                        let (r, _) = ui.allocate_exact_size(vec2(22.0, 26.0), Sense::hover());
                        crate::icons::paint(ui, r, "sparkles", 14.0, t.accent);
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut prompt)
                                .hint_text(tl!("Describe what to generate, or leave empty to fill from the surroundings"))
                                .desired_width(300.0)
                                .margin(egui::Margin::symmetric(6, 4)),
                        );
                        if focus {
                            resp.request_focus();
                        }
                        let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if bar_button(ui, "", tl!("Generate"), true, true, &t)
                            .on_hover_text(tl!("Generative Fill: regenerate the area from your description (empty: from its surroundings)"))
                            .clicked()
                            || enter
                        {
                            go("fill.go");
                        }
                        let back = crate::icons::button(ui, "x", 26.0, false, tl!("Back"));
                        back.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tl!("Back")));
                        if back.clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                            go("fill.close");
                        }
                        return;
                    }
                    match what {
                        Context::Selection => {
                            let remove_tip = match &ready {
                                Ok(()) => tl!("Remove what's inside with AI (on its own layer)").to_owned(),
                                Err(why) => crate::i18n::fmt(tl!("AI Remove isn't ready: {why}"), &[("why", why)]),
                            };
                            if bar_button(ui, "eraser-magic", tl!("Remove"), true, ready.is_ok(), &t)
                                .on_hover_text(&remove_tip)
                                .on_disabled_hover_text(&remove_tip)
                                .clicked()
                            {
                                go("remove");
                            }
                            if bar_button(ui, "sparkles", tl!("Generative Fill"), false, true, &t)
                                .on_hover_text(tl!("Regenerate the area from a description"))
                                .clicked()
                            {
                                go("fill.open");
                            }
                            if bar_button(ui, "paint-bucket", tl!("Content-Aware Fill"), false, true, &t)
                                .on_hover_text(tl!("Fill the selection from the image around it"))
                                .clicked()
                            {
                                go("caf");
                            }
                            divider(ui, &t);
                            if bar_button(ui, "squares-subtract", tl!("Invert"), false, true, &t).on_hover_text(&invert_tip).clicked() {
                                go("invert");
                            }
                            if bar_button(ui, "layers", tl!("Mask"), false, true, &t).on_hover_text(tl!("Add a layer mask that shows only this area")).clicked()
                            {
                                go("mask");
                            }
                            let deselect = crate::icons::button(ui, "x", 26.0, false, &deselect_tip);
                            deselect.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tl!("Deselect")));
                            if deselect.clicked() {
                                go("deselect");
                            }
                        }
                        Context::Path => {
                            if bar_button(ui, "square-dashed", tl!("Make Selection"), true, true, &t).on_hover_text(&make_selection_tip).clicked() {
                                go("path.select");
                            }
                            divider(ui, &t);
                            if bar_button(ui, "paint-bucket", tl!("Content-Aware Fill"), false, true, &t)
                                .on_hover_text(tl!("Fill the path's area from the image around it"))
                                .clicked()
                            {
                                go("caf");
                            }
                            if bar_button(ui, "sparkles", tl!("AI Fill"), false, true, &t)
                                .on_hover_text(tl!("Generative Fill: regenerate the path's area from a description"))
                                .clicked()
                            {
                                go("fill.open");
                            }
                            if bar_button(ui, "layers", tl!("Mask"), false, true, &t).on_hover_text(tl!("Add a vector mask from the path")).clicked() {
                                go("mask");
                            }
                        }
                        Context::Shape(l) => {
                            if let Some((_, sh)) = &shape {
                                let mut edit = None;
                                label(ui, tl!("Fill"), &t);
                                edit = crate::vector_ui::fill_control(ui, sh, l).or(edit);
                                ui.add_space(6.0);
                                label(ui, tl!("Stroke"), &t);
                                edit = crate::vector_ui::stroke_color_control(ui, sh, l).or(edit);
                                edit = crate::vector_ui::stroke_width_control(ui, sh, l, 54.0).or(edit);
                                if matches!(sh.live, Some(photocraft_doc::vector::LiveShape::Rect { .. })) {
                                    ui.add_space(6.0);
                                    edit = crate::vector_ui::kind_controls(ui, sh, l, false).or(edit);
                                }
                                if edit.is_some() {
                                    act = Some(Act::Shape(edit));
                                }
                            }
                            divider(ui, &t);
                            if bar_button(ui, "pen-tool", tl!("Edit Path"), false, true, &t)
                                .on_hover_text(tl!("Edit the shape's anchors and handles"))
                                .clicked()
                            {
                                act = Some(Act::Do("editPath"));
                            }
                            if bar_button(ui, "image", tl!("Rasterize"), false, true, &t).on_hover_text(tl!("Turn the shape into pixels")).clicked() {
                                act = Some(Act::Do("rasterize"));
                            }
                        }
                        Context::Text(_) => {
                            crate::type_tool::quick_bar(app, ui);
                            divider(ui, &t);
                            if bar_button(ui, "text-cursor", tl!("Edit Text"), false, true, &t).clicked() {
                                go("editText");
                            }
                        }
                        Context::Smart(_) => {
                            if bar_button(ui, "package", tl!("Edit Contents"), true, true, &t).on_hover_text(tl!("Open the Smart Object's contents")).clicked()
                            {
                                go("editContents");
                            }
                        }
                        Context::Develop(_) => {
                            if bar_button(ui, "sliders-horizontal", tl!("Open in Develop"), true, true, &t)
                                .on_hover_text(tl!("Edit this photo's develop settings"))
                                .clicked()
                            {
                                go("develop");
                            }
                        }
                        Context::Generated(_) => {
                            let what = generation.as_ref().and_then(|g| g["prompt"].as_str()).filter(|p| !p.is_empty()).map_or_else(
                                || tl!("Generate this area again from its surroundings").to_owned(),
                                |p| crate::i18n::fmt(tl!("Generate “{prompt}” again with a new seed"), &[("prompt", p)]),
                            );
                            if bar_button(ui, "refresh-cw", tl!("Regenerate"), true, true, &t).on_hover_text(&what).clicked() {
                                go("regenerate");
                            }
                            if bar_button(ui, "sparkles", tl!("Variations"), false, true, &t)
                                .on_hover_text(tl!("Create another variation using this layer's prompt and a new seed"))
                                .clicked()
                            {
                                go("variations");
                            }
                            divider(ui, &t);
                            if bar_button(ui, "wand-sparkles", tl!("Select Subject"), false, true, &t).clicked() {
                                go("selectSubject");
                            }
                        }
                        Context::Pixel(_) => {
                            if bar_button(ui, "wand-sparkles", tl!("Select Subject"), false, true, &t)
                                .on_hover_text(tl!("Select the main subject with AI"))
                                .clicked()
                            {
                                go("selectSubject");
                            }
                            if bar_button(ui, "eraser-background", tl!("Remove Background"), false, crate::background_ui::can_remove(app), &t)
                                .on_hover_text(tl!("Remove the background using the chosen model and output"))
                                .clicked()
                            {
                                go("removeBackground");
                            }
                            let choices = crate::icons::button(ui, "chevron-down", 20.0, false, tl!("Background removal options"));
                            choices.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tl!("Background removal options")));
                            egui::Popup::menu(&choices).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
                                ui.set_min_width(240.0);
                                crate::background_ui::menu(app, ui);
                                crate::background_ui::readiness(app, ui);
                            });
                            if bar_button(ui, "move", tl!("Transform"), false, true, &t).on_hover_text(tl!("Free Transform")).clicked() {
                                go("transform");
                            }
                        }
                    }
                    if what == Context::Path {
                        return;
                    }
                    let more = crate::icons::button(ui, "ellipsis", 26.0, false, tl!("More"));
                    more.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tl!("More")));
                    egui::Popup::menu(&more).show(|ui| {
                        ui.set_min_width(190.0);
                        if what == Context::Selection {
                            for (label, a) in
                                [(tl!("Feather…"), "feather"), (tl!("Select and Mask…"), "selectAndMask"), (tl!("Content-Aware Fill…"), "cafDialog")]
                            {
                                if ui.button(label).clicked() {
                                    act = Some(Act::Do(a));
                                    ui.close();
                                }
                            }
                            ui.separator();
                        }
                        if ui.button(tl!("Hide Contextual Task Bar")).clicked() {
                            act = Some(Act::Do("hide"));
                            ui.close();
                        }
                    });
                });
            });
    });
    with(|s| s.fill_prompt = prompt.clone());
    match act {
        Some(Act::Do(a)) => run(app, ctx, what, a, prompt, generation),
        Some(Act::Shape(edit)) => {
            if let Context::Shape(l) = what {
                crate::vector_ui::apply_shape_edit(app, l, edit);
            }
        }
        None => {}
    }
}

/// Runs bar action `a` for `what`.
fn run(app: &mut PhotocraftApp, ctx: &egui::Context, what: Context, a: &str, prompt: String, generation: Option<Value>) {
    // A path's actions start from its selection (Mask tries a vector mask first).
    let from_path = what == Context::Path && matches!(a, "path.select" | "caf" | "fill.go");
    if from_path {
        let name = selected_path(app).map(|(name, _)| name).unwrap_or_else(|| "work".into());
        let r = app.run("path.toSelection", json!({ "name": name }));
        if r.is_err() {
            report(app, r);
            return;
        }
    }
    let layer = match what {
        Context::Shape(l) | Context::Text(l) | Context::Smart(l) | Context::Develop(l) | Context::Generated(l) | Context::Pixel(l) => Some(l.0),
        _ => None,
    };
    let r = match a {
        "path.select" => Ok(Value::Null),
        "remove" => {
            crate::ai_remove_ui::selection(app);
            Ok(Value::Null)
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
        // One click, Photoshop's defaults; the dialog (More › Content-Aware Fill…) has the options.
        "caf" => app.run("edit.contentAwareFill", json!({})),
        "cafDialog" => crate::menus::invoke(app, ctx, "edit.contentAwareFill", json!({})),
        "invert" => app.run("select.inverse", json!({})),
        "mask" if what == Context::Path => path_mask(app, ctx),
        "mask" => crate::menus::invoke(app, ctx, "layer.layerMask.revealSelection", json!({})),
        "deselect" => app.run("select.deselect", json!({})),
        "feather" => crate::menus::invoke(app, ctx, "select.modify.feather", json!({})),
        "selectAndMask" => crate::menus::invoke(app, ctx, "select.selectAndMask", json!({})),
        "editPath" => {
            app.ui.tool = crate::Tool::DirectSelection;
            app.ui.selected_path = Some("layer".into());
            Ok(Value::Null)
        }
        "rasterize" => app.run("layer.rasterize.shape", json!({ "layer": layer })),
        "editText" => crate::menus::invoke(app, ctx, "type.editText", json!({})),
        "editContents" => crate::menus::invoke(app, ctx, "layer.smartObjects.editContents", json!({})),
        "develop" => crate::menus::invoke(app, ctx, crate::develop_layer::DEVELOP_ID, json!({})),
        "regenerate" | "variations" => regenerate(app, ctx, layer, generation, a == "variations"),
        "selectSubject" => app.run("ai.selectSubject", json!({})),
        "removeBackground" => crate::background_ui::run(app, layer),
        "transform" => crate::menus::invoke(app, ctx, "edit.freeTransform", json!({})),
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

/// Mask from the work path: a vector mask on the active layer, as in Photoshop; where a layer
/// can't take one (a shape layer, the Background), a layer mask from the path's selection.
fn path_mask(app: &mut PhotocraftApp, ctx: &egui::Context) -> Result<Value, String> {
    let name = selected_path(app).map(|(name, _)| name).unwrap_or_else(|| "work".into());
    let vector = app
        .session
        .active()
        .and_then(|d| d.doc.layer(d.active_layer?).map(|l| !crate::doc_props_ui::is_background(&d.doc, l) && !matches!(l.content, LayerContent::Shape(_))));
    if vector == Some(true)
        && let Ok(v) = app.run("layer.vectorMask.currentPath", json!({ "name": name }))
    {
        return Ok(v);
    }
    app.run("path.toSelection", json!({ "name": name }))?;
    crate::menus::invoke(app, ctx, "layer.layerMask.revealSelection", json!({}))
}

/// Regenerate an AI layer with the prompt it was made from and a new seed: Generative Fill over
/// the layer's own pixels (or Generate Background again). The old result is hidden, not deleted.
fn regenerate(app: &mut PhotocraftApp, ctx: &egui::Context, layer: Option<u64>, generation: Option<Value>, variations: bool) -> Result<Value, String> {
    let (Some(layer), Some(g)) = (layer, generation) else { return Err(tl!("This layer has no generation settings to repeat").into()) };
    let mut p = json!({ "prompt": g["prompt"].as_str().unwrap_or(""), "replaceLayer": layer });
    if let Some(e) = g["engine"].as_str() {
        p["engine"] = json!(e);
    }
    match g["command"].as_str() {
        Some("ai.generativeFill") => {
            app.run("select.loadSelection", json!({ "channel": "transparency", "layer": layer }))?;
            app.run("ai.generativeFill", p)
        }
        Some("ai.generateBackground") => app.run("ai.generateBackground", p),
        Some("ai.generate") => crate::generate_ui::regenerate_layer(app, ctx, layer, &g, variations).map(|()| Value::Null),
        _ => Err(tl!("This layer has no generation settings to repeat").into()),
    }
}

#[cfg(test)]
#[path = "context_bar_tests.rs"]
mod tests;
