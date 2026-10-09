//! Floating dock panels and magnetic docking, like Photoshop's: drag a tab (or a group by its
//! strip) more than [`TEAR`] points out of the dock and it becomes a floating panel under the
//! pointer, the drag carrying on seamlessly. While a floating panel (or a docked tab) is dragged
//! the drop target under the pointer is highlighted in the accent colour, within [`SNAP`]
//! points of it: a box on a tab strip (join that group, or another floating panel), a line
//! between two groups or along the dock's edge (a new group there). Letting go docks it.
//!
//! Floating panels are themed cards over the canvas with a grip bar (drag to move, × to close),
//! their tab strip and a resize corner; their edges snap to the window's and canvas' edges.
//! Lock Workspace keeps them (and the dock) where they are.

use egui::{CornerRadius, Id, LayerId, Order, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2, pos2, vec2};

use super::{Drag, GAP, Group, Place, Tab, active_tab, dock_area, drop_before, is_pro, last_rects, last_strips, select, sync, tab_body};
use crate::PhotocraftApp;
use crate::theme::Tokens;
use crate::widgets;

/// How far (points) past the dock's edge a dragged tab or group goes before it tears off.
pub const TEAR: f32 = 36.0;
/// How near (points) a drop zone the pointer must be for it to catch a dragged panel.
pub const SNAP: f32 = 24.0;
/// Floating panel edges within this many points of the window's or canvas' edges snap to them.
const EDGE_SNAP: f32 = 10.0;
/// The grip bar along a floating panel's top.
const GRIP_H: f32 = 14.0;
const MIN_SIZE: Vec2 = vec2(200.0, 140.0);

/// Where a dragged panel docks when let go.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Target {
    /// Into docked group `g`'s tab strip, at that tab position.
    Group(Group, usize),
    /// A new group in the dock column, drawn before the given group (last when `None`).
    NewGroup(Option<Group>),
    /// Into floating panel `id`'s tab strip, at that tab position.
    Floating(u32, usize),
}

/// A floating panel as drawn last frame (screen points), for drop targets, tests and automation.
#[derive(Clone, Debug, PartialEq)]
pub struct FloatRects {
    pub id: u32,
    /// The whole panel.
    pub rect: Rect,
    /// The grip bar (drag to move).
    pub grip: Rect,
    /// The close button.
    pub close: Rect,
    /// The tab strip.
    pub strip: Rect,
    /// The panel's tabs, in strip order.
    pub tab_ids: Vec<Tab>,
    /// `(tab index, rect)` of the tabs on the strip.
    pub tabs: Vec<(usize, Rect)>,
}

fn floats_id() -> Id {
    Id::new("dock-float-rects")
}

fn area_id(id: u32) -> Id {
    Id::new(("dock-float", id))
}

/// The floating panels drawn last frame.
pub fn last_floats(ctx: &egui::Context) -> Vec<FloatRects> {
    ctx.data(|d| d.get_temp::<Vec<FloatRects>>(floats_id())).unwrap_or_default()
}

/// Strip position for a drop at `x`: after every shown tab whose centre is left of it.
fn insert_at(tabs: &[(usize, Rect)], x: f32) -> usize {
    tabs.iter().filter(|(_, r)| x > r.center().x).map(|(i, _)| i + 1).max().unwrap_or(0)
}

/// The gap lines of the dock column: `(y, group drawn below it)`.
fn gaps(rects: &[(Group, Rect)], area: Rect) -> Vec<(f32, Option<Group>)> {
    let Some((first, r0)) = rects.first() else { return vec![(area.top() + 2.0, None)] };
    let mut out = vec![(r0.top() - GAP / 2.0, Some(*first))];
    for w in rects.windows(2) {
        if let [(_, a), (b, rb)] = w {
            out.push(((a.bottom() + rb.top()) / 2.0, Some(*b)));
        }
    }
    if let Some((_, last)) = rects.last() {
        out.push(((last.bottom() + GAP / 2.0).min(area.bottom()), None));
    }
    out
}

/// The drop target for a panel dragged to `p` (`exclude`: the floating panel being dragged).
pub fn target_at(ctx: &egui::Context, p: Pos2, exclude: Option<u32>) -> Option<Target> {
    // Floating panels are drawn over the dock: their strips first.
    for f in last_floats(ctx).iter().rev() {
        if Some(f.id) == exclude {
            continue;
        }
        if f.strip.union(f.grip).expand(SNAP / 2.0).contains(p) {
            return Some(Target::Floating(f.id, insert_at(&f.tabs, p.x)));
        }
        if f.rect.contains(p) {
            return None;
        }
    }
    let area = dock_area(ctx)?;
    for s in last_strips(ctx) {
        // The strip itself (and the dock's edge beside it); the gap above it makes a new group.
        let zone = Rect::from_min_max(pos2(s.strip.left() - SNAP, s.strip.top()), s.strip.right_bottom());
        if zone.contains(p) {
            return Some(Target::Group(s.group, insert_at(&s.tabs, p.x)));
        }
    }
    let zone = Rect::from_min_max(pos2(area.left() - SNAP, area.top() - SNAP), pos2(area.right(), area.bottom() + SNAP));
    if !zone.contains(p) {
        return None;
    }
    let rects = last_rects(ctx);
    let (y, before) = gaps(&rects, area).into_iter().min_by(|a, b| (a.0 - p.y).abs().total_cmp(&(b.0 - p.y).abs()))?;
    // Along the dock's edge the nearest gap catches it; inside the column only a gap near it.
    let at_edge = p.x < area.left() + SNAP / 2.0;
    (at_edge || rects.is_empty() || (y - p.y).abs() <= SNAP).then_some(Target::NewGroup(before))
}

/// Has a drag from the dock gone far enough out of it to tear off?
fn torn(ctx: &egui::Context, p: Pos2) -> bool {
    dock_area(ctx).is_none_or(|a| !a.expand(TEAR).contains(p))
}

/// Size of a panel torn off a dock column `area` wide from a group `h` tall.
pub(super) fn torn_size(area: Rect, h: f32) -> Vec2 {
    let w = if area.width().is_finite() { area.width() } else { 280.0 };
    let h = if h.is_finite() { h } else { 320.0 };
    vec2(w.clamp(220.0, 420.0), h.clamp(180.0, 560.0))
}

/// Float docked group `g` (all its tabs) at `pos`. Returns the new panel's id (0: nothing to float).
pub(super) fn float_group(app: &mut PhotocraftApp, g: Group, pos: Pos2, size: Vec2) -> u32 {
    let pro = is_pro(app);
    let tabs = app.ui.dock.group_tabs(g, pro);
    if tabs.is_empty() {
        return 0;
    }
    let active = active_tab(app, Place::Docked(g));
    let id = app.ui.dock.float(&tabs, active, pos, size, pro);
    if let Some(a) = active {
        select(app, a);
    }
    app.ui.dock.live.raise = Some(id);
    id
}

/// Dock `tabs` (showing `lead`) at `target`.
fn drop_tabs(app: &mut PhotocraftApp, tabs: &[Tab], lead: Tab, target: Target) {
    let pro = is_pro(app);
    // Strip positions count the dragged tabs where they are now; they leave first.
    let shift = |list: &[Tab], at: usize| at - list.iter().take(at).filter(|t| tabs.contains(t)).count();
    match target {
        Target::Group(g, at) => {
            let at = shift(&app.ui.dock.group_tabs(g, pro), at);
            app.ui.dock.insert(tabs, g, at, pro);
            g.set_shown(&mut app.ui.panels, true);
        }
        Target::NewGroup(before) => {
            let g = app.ui.dock.new_group(tabs, lead, before, pro);
            g.set_shown(&mut app.ui.panels, true);
        }
        Target::Floating(id, at) => {
            let at = app.ui.dock.floating(id).map_or(0, |f| shift(&f.tabs, at));
            app.ui.dock.merge_into_floating(tabs, id, at, pro);
            app.ui.dock.live.raise = Some(id);
        }
    }
    select(app, lead);
}

/// Keep a floating panel's grip bar reachable on `screen`.
fn keep_on_screen(r: Rect, screen: Rect) -> Rect {
    if !screen.is_positive() {
        return r;
    }
    let x = r.left().clamp((screen.left() - r.width() + 60.0).min(screen.left()), (screen.right() - 60.0).max(screen.left()));
    let y = r.top().clamp(screen.top(), (screen.bottom() - GRIP_H - 10.0).max(screen.top()));
    Rect::from_min_size(pos2(x, y), r.size())
}

/// Snap `r`'s edges to the nearest of `bounds`' edges within [`EDGE_SNAP`].
fn snap_offset(r: Rect, bounds: &[Rect]) -> Vec2 {
    let pick = |cands: &mut dyn Iterator<Item = f32>| cands.filter(|d| d.abs() < EDGE_SNAP).min_by(|a, b| a.abs().total_cmp(&b.abs())).unwrap_or(0.0);
    let good: Vec<Rect> = bounds.iter().copied().filter(|b| b.is_positive() && b.is_finite()).collect();
    let dx = pick(&mut good.iter().flat_map(|b| [b.left() - r.left(), b.right() - r.right()]));
    let dy = pick(&mut good.iter().flat_map(|b| [b.top() - r.top(), b.bottom() - r.bottom()]));
    vec2(dx, dy)
}

fn painter(ctx: &egui::Context) -> egui::Painter {
    ctx.layer_painter(LayerId::new(Order::Foreground, Id::new("dock-drop-zone")))
}

/// The highlighted drop zone for `target`.
fn paint_target(ctx: &egui::Context, target: Target, t: &Tokens) {
    let p = painter(ctx);
    let strip_box = |strip: Rect, tabs: &[(usize, Rect)], at: usize| {
        p.rect_filled(strip, 3.0, t.accent.gamma_multiply(0.22));
        p.rect_stroke(strip, 3.0, Stroke::new(2.0, t.accent), StrokeKind::Inside);
        let x = match at.checked_sub(1) {
            Some(prev) => tabs.iter().filter(|(i, _)| *i <= prev).map(|(_, r)| r.right()).fold(strip.left() + 3.0, f32::max),
            None => tabs.first().map_or(strip.left() + 3.0, |(_, r)| r.left() + 1.0),
        };
        p.line_segment([pos2(x, strip.top() + 3.0), pos2(x, strip.bottom() - 3.0)], Stroke::new(3.0, t.accent));
    };
    match target {
        Target::Group(g, at) => {
            if let Some(s) = last_strips(ctx).into_iter().find(|s| s.group == g) {
                strip_box(s.strip, &s.tabs, at);
            }
        }
        Target::Floating(id, at) => {
            if let Some(f) = last_floats(ctx).into_iter().find(|f| f.id == id) {
                strip_box(f.strip, &f.tabs, at);
            }
        }
        Target::NewGroup(before) => {
            let Some(area) = dock_area(ctx) else { return };
            let rects = last_rects(ctx);
            let y = gaps(&rects, area).into_iter().find(|(_, b)| *b == before).map_or(area.top() + 2.0, |(y, _)| y);
            let band = Rect::from_min_max(pos2(area.left(), y - 4.0), pos2(area.right(), y + 4.0));
            p.rect_filled(band, 2.0, t.accent.gamma_multiply(0.25));
            p.line_segment([pos2(area.left(), y), pos2(area.right(), y)], Stroke::new(3.0, t.accent));
        }
    }
}

/// The tab label following the pointer while a docked tab is dragged.
fn paint_ghost(ctx: &egui::Context, at: Pos2, label: &str, t: &Tokens) {
    let p = painter(ctx);
    let galley = p.layout_no_wrap(tl!(label).to_owned(), egui::FontId::proportional(11.5), t.text);
    let r = Rect::from_min_size(at + vec2(12.0, 10.0), galley.size() + vec2(16.0, 8.0));
    p.rect_filled(r, 3.0, t.card);
    p.rect_stroke(r, 3.0, Stroke::new(1.0, t.accent), StrokeKind::Inside);
    p.galley(r.min + vec2(8.0, 4.0), galley, t.text);
}

/// Move the drag in progress on (once per frame): tear off, follow the pointer, show the drop
/// zone, dock or reorder on release.
pub(super) fn drive(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let pass = ctx.cumulative_pass_nr();
    let last = app.ui.dock.live.driven.replace(pass);
    if last == Some(pass) {
        return;
    }
    let Some(drag) = app.ui.dock.live.drag else { return };
    // Frames went by without the dock or floating panels (full screen): drop the drag.
    if last.is_some_and(|l| l.saturating_add(1) < pass) || app.session.prefs().workspace_locked {
        app.ui.dock.live.drag = None;
        return;
    }
    let (pos, down) = ctx.input(|i| (i.pointer.latest_pos(), i.pointer.primary_down()));
    let Some(p) = pos else {
        if !down {
            app.ui.dock.live.drag = None;
        }
        return;
    };
    let t = Tokens::get(ctx);
    let pro = is_pro(app);
    ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
    match drag {
        Drag::Tab { tab, .. } => {
            if down && torn(ctx, p) {
                let area = dock_area(ctx).unwrap_or(Rect::NOTHING);
                let grab = vec2(36.0, GRIP_H + 12.0);
                let id = app.ui.dock.float(&[tab], Some(tab), p - grab, torn_size(area, 320.0), pro);
                select(app, tab);
                app.ui.dock.live.drag = Some(Drag::Floating { id, grab });
                app.ui.dock.live.raise = Some(id);
                ctx.request_repaint();
                return;
            }
            let target = target_at(ctx, p, None);
            if down {
                if let Some(tg) = target {
                    paint_target(ctx, tg, &t);
                }
                paint_ghost(ctx, p, tab.label(), &t);
            } else if let Some(tg) = target {
                drop_tabs(app, &[tab], tab, tg);
            }
        }
        Drag::Group { group, grab } => {
            if down && torn(ctx, p) {
                let area = dock_area(ctx).unwrap_or(Rect::NOTHING);
                let h = last_rects(ctx).into_iter().find(|(g, _)| *g == group).map_or(320.0, |(_, r)| r.height());
                let size = torn_size(area, h);
                let grab = vec2(grab.x.clamp(12.0, size.x - 12.0), GRIP_H + 12.0);
                let id = float_group(app, group, p - grab, size);
                app.ui.dock.live.drag = (id != 0).then_some(Drag::Floating { id, grab });
                ctx.request_repaint();
                return;
            }
            // Still in the dock: move the group up or down the column.
            let rects = last_rects(ctx);
            let order: Vec<Group> = rects.iter().map(|(g, _)| *g).collect();
            let before = drop_before(&order, &rects, group, p.y);
            if down {
                if let Some(area) = dock_area(ctx) {
                    let line_y = match before.and_then(|b| rects.iter().find(|(x, _)| *x == b)) {
                        Some((_, r)) => r.top() - GAP / 2.0,
                        None => rects.last().map_or(area.top(), |(_, r)| r.bottom() + GAP / 2.0),
                    };
                    painter(ctx).line_segment([pos2(area.left(), line_y), pos2(area.right(), line_y)], Stroke::new(3.0, t.accent));
                }
            } else if before != Some(group) {
                app.ui.dock.move_group(group, before);
            }
        }
        Drag::Floating { id, grab } => {
            let target = target_at(ctx, p, Some(id));
            if down {
                let screen = ctx.content_rect();
                let canvas = app.last_canvas_rect;
                if let Some(f) = app.ui.dock.floating_mut(id) {
                    let mut r = Rect::from_min_size(p - grab, f.rect().size());
                    if target.is_none() {
                        r = r.translate(snap_offset(r, &[screen, canvas]));
                    }
                    let r = keep_on_screen(r, screen);
                    f.pos = [r.left(), r.top()];
                }
                if let Some(tg) = target {
                    paint_target(ctx, tg, &t);
                }
            } else if let Some(tg) = target
                && let Some(f) = app.ui.dock.floating(id).cloned()
                && let Some(lead) = f.shown_tab()
            {
                drop_tabs(app, &f.tabs, lead, tg);
            }
        }
    }
    if down {
        ctx.request_repaint();
    } else {
        app.ui.dock.live.drag = None;
    }
}

/// What a floating panel reported this frame.
struct PanelOut {
    grip: egui::Response,
    close: egui::Response,
    card: widgets::CardResponse,
    resize: Option<egui::Response>,
    rects: FloatRects,
}

/// Draw the open floating panels (from the app's ui, over the canvas). `body` draws one tab's
/// content, as for the dock.
pub fn show_floating(app: &mut PhotocraftApp, ctx: &egui::Context, mut body: impl FnMut(&mut PhotocraftApp, &mut egui::Ui, Tab)) {
    sync(app);
    drive(app, ctx);
    let t = Tokens::get(ctx);
    let locked = app.session.prefs().workspace_locked;
    let screen = ctx.content_rect();
    let pro = is_pro(app);
    let ids: Vec<u32> = app.ui.dock.floating.iter().filter(|f| !f.hidden).map(|f| f.id).collect();
    let mut drawn: Vec<FloatRects> = Vec::with_capacity(ids.len());
    for id in ids {
        let Some(f) = app.ui.dock.floating(id).cloned() else { continue };
        let Some(active) = f.shown_tab() else { continue };
        let size = vec2(f.size[0], f.size[1]).max(MIN_SIZE).min(screen.size().max(MIN_SIZE));
        let rect = keep_on_screen(Rect::from_min_size(pos2(f.pos[0], f.pos[1]), size), screen);
        let tabs = f.tabs.clone();
        let before = tabs.iter().position(|x| *x == active).unwrap_or(0);
        let mut sel = before;
        let Some(out) = panel(app, ctx, id, rect, &tabs, &mut sel, locked, &t, &mut body) else { continue };
        if out.close.clicked()
            && let Some(f) = app.ui.dock.floating_mut(id)
        {
            f.hidden = true;
        }
        if sel != before
            && let Some(tab) = tabs.get(sel)
        {
            select(app, *tab);
        }
        let p = ctx.pointer_interact_pos().unwrap_or(rect.min);
        let press = ctx.input(|i| i.pointer.press_origin()).unwrap_or(p);
        if !locked && app.ui.dock.live.drag.is_none() {
            let tab_drag = out.card.tab_drag_started.and_then(|i| tabs.get(i).copied());
            if let Some(tab) = tab_drag.filter(|_| tabs.len() > 1) {
                // Pull the tab out of this panel into its own, under the pointer.
                let grab = vec2(36.0, GRIP_H + 12.0);
                let new = app.ui.dock.float(&[tab], Some(tab), p - grab, rect.size(), pro);
                select(app, tab);
                app.ui.dock.live.drag = Some(Drag::Floating { id: new, grab });
                app.ui.dock.live.raise = Some(new);
            } else if tab_drag.is_some() || out.grip.drag_started() || out.card.strip.drag_started() {
                app.ui.dock.live.drag = Some(Drag::Floating { id, grab: press - rect.min });
                app.ui.dock.live.raise = Some(id);
            }
        }
        if let Some(r) = &out.resize {
            if r.hovered() || r.dragged() {
                ctx.set_cursor_icon(egui::CursorIcon::ResizeNwSe);
            }
            if r.dragged()
                && let Some(f) = app.ui.dock.floating_mut(id)
            {
                let s = (rect.size() + r.drag_delta()).max(MIN_SIZE);
                f.size = [s.x, s.y];
                f.pos = [rect.left(), rect.top()];
            }
        }
        if !locked && (out.grip.hovered() || out.card.strip.hovered()) && app.ui.dock.live.drag.is_none() {
            ctx.set_cursor_icon(egui::CursorIcon::Grab);
        }
        egui::Popup::menu(&out.card.menu).show(|ui| {
            ui.set_min_width(170.0);
            if tabs.get(sel) == Some(&Tab::Layers) {
                crate::layer_row_ui::panel_menu(app, ui);
                ui.separator();
            }
            if ui.add_enabled(!locked, egui::Button::new(tl!("Dock Panel"))).clicked() {
                drop_tabs(app, &tabs, tabs.get(sel).copied().unwrap_or(active), Target::NewGroup(None));
                ui.close();
            }
            ui.separator();
            if ui.button(tl!("Close")).clicked() {
                if let Some(f) = app.ui.dock.floating_mut(id) {
                    f.hidden = true;
                }
                ui.close();
            }
        });
        drawn.push(out.rects);
    }
    if let Some(id) = app.ui.dock.live.raise.take() {
        ctx.move_to_top(LayerId::new(Order::Middle, area_id(id)));
    }
    ctx.data_mut(|d| d.insert_temp(floats_id(), drawn));
}

/// One floating panel: grip bar with close button, the tab group, a resize corner.
#[allow(clippy::too_many_arguments)]
fn panel(
    app: &mut PhotocraftApp,
    ctx: &egui::Context,
    id: u32,
    rect: Rect,
    tabs: &[Tab],
    sel: &mut usize,
    locked: bool,
    t: &Tokens,
    body: &mut impl FnMut(&mut PhotocraftApp, &mut egui::Ui, Tab),
) -> Option<PanelOut> {
    let aid = area_id(id);
    let labels: Vec<&str> = tabs.iter().map(|t| t.label()).collect();
    let frame = egui::Frame::NONE.fill(t.dock).stroke(Stroke::new(1.0, t.card_border)).corner_radius(CornerRadius::same(4)).shadow(egui::Shadow {
        offset: [0, 10],
        blur: 30,
        spread: 0,
        color: t.shadow,
    });
    egui::Area::new(aid)
        .order(Order::Middle)
        .fixed_pos(rect.min)
        .constrain(false)
        .show(ctx, |ui| {
            frame
                .show(ui, |ui| {
                    let (outer, _) = ui.allocate_exact_size(rect.size(), Sense::hover());
                    let grip = Rect::from_min_size(outer.min, vec2(outer.width(), GRIP_H));
                    let grip_resp = ui.interact(grip, aid.with("grip"), Sense::click_and_drag());
                    let dots = if grip_resp.hovered() { t.text_dim } else { t.text_faint };
                    for k in 0..5 {
                        ui.painter().circle_filled(pos2(grip.center().x - 8.0 + k as f32 * 4.0, grip.center().y), 1.0, dots);
                    }
                    let close = Rect::from_center_size(pos2(grip.right() - 9.0, grip.center().y), vec2(14.0, 14.0));
                    let close_resp = ui.interact(close, aid.with("close"), Sense::click());
                    close_resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tl!("Close panel")));
                    let c = if close_resp.hovered() { t.text } else { t.text_faint };
                    let x = close.shrink(4.0);
                    ui.painter().line_segment([x.left_top(), x.right_bottom()], Stroke::new(1.2, c));
                    ui.painter().line_segment([x.right_top(), x.left_bottom()], Stroke::new(1.2, c));
                    let close_resp = close_resp.on_hover_text(tl!("Close"));
                    let body_rect = Rect::from_min_max(pos2(outer.left() + 2.0, grip.bottom()), outer.max - vec2(2.0, 2.0));
                    let mut child = ui.new_child(egui::UiBuilder::new().id_salt(("dock-float-body", id)).max_rect(body_rect));
                    child.set_clip_rect(body_rect.intersect(ui.clip_rect()));
                    child.spacing_mut().item_spacing.y = if t.pro { 0.0 } else { 6.0 };
                    let card = widgets::card_ex(&mut child, "float", &labels, sel, false, |ui, i| {
                        if let Some(tab) = tabs.get(i).copied() {
                            tab_body(app, ui, tab, body);
                        }
                    });
                    let corner = Rect::from_min_size(outer.max - vec2(14.0, 14.0), vec2(14.0, 14.0));
                    let resize = (!locked).then(|| ui.interact(corner, aid.with("resize"), Sense::drag()));
                    if !locked {
                        for k in [4.0, 8.0] {
                            ui.painter().line_segment(
                                [pos2(corner.right() - k, corner.bottom() - 2.0), pos2(corner.right() - 2.0, corner.bottom() - k)],
                                Stroke::new(1.0, t.text_faint),
                            );
                        }
                    }
                    let strip = card.tabs.iter().fold(card.strip.rect.union(card.menu.rect), |r, (_, t)| r.union(*t));
                    let rects = FloatRects { id, rect: outer, grip, close, strip, tab_ids: tabs.to_vec(), tabs: card.tabs.clone() };
                    PanelOut { grip: grip_resp, close: close_resp, card, resize, rects }
                })
                .inner
        })
        .inner
        .into()
}
