//! The window's own title bar on Windows and Linux, where the window has no OS decorations
//! ([`PhotocraftApp::custom_titlebar`]): the app's top bar (`panels::title_bar`) is the title bar,
//! as in Photoshop on Windows. The free gap between its menus and controls drags the window and a
//! double-click there maximizes it (`panels::title_bar`, on every platform); caption buttons at
//! the far right minimize, maximize/restore and close, and thin invisible zones along the window's
//! edges resize it.

use egui::{CursorIcon, Id, LayerId, Order, PointerButton, Rect, ResizeDirection, Sense, Stroke, Ui, ViewportCommand, pos2, vec2};
use serde_json::json;

use crate::PhotocraftApp;
use crate::theme::Tokens;

/// One caption button: as wide as the native Windows ones.
pub const BUTTON_WIDTH: f32 = 46.0;
/// The three caption buttons together.
pub const WIDTH: f32 = 3.0 * BUTTON_WIDTH;
/// Resize zone thickness along an edge, and the side of a corner zone.
const EDGE: f32 = 5.0;
const CORNER: f32 = 12.0;
const MODAL_DRAG_AREA: &str = "titlebar-modal-drag-area";

#[derive(Clone, Copy, PartialEq, Debug)]
enum Caption {
    Minimize,
    Maximize,
    Close,
}

/// Left to right.
const CAPTIONS: [Caption; 3] = [Caption::Minimize, Caption::Maximize, Caption::Close];

/// Caption button ids (stable, so tests and agents can find them).
fn caption_id(c: Caption) -> Id {
    Id::new(("titlebar-caption", c as u8))
}

/// The title bar's unused gap, saved by `panels::title_bar` for drags that begin while a modal
/// backdrop is on top of the normal title-bar widgets.
pub(crate) fn modal_drag_area_id() -> Id {
    Id::new(MODAL_DRAG_AREA)
}

/// Restores window dragging over the title bar's empty gap after a modal has blocked the normal
/// title-bar layer. The hit target stays limited to the same gap, so menus and caption controls
/// remain owned by their widgets.
pub(crate) fn modal_drag_overlay(ctx: &egui::Context) {
    let Some(area) = ctx.data(|d| d.get_temp::<Rect>(modal_drag_area_id())) else { return };
    if !area.is_positive() || egui::Popup::is_any_open(ctx) || !crate::dialogs::titlebar_drag_area_is_clear(ctx, area) {
        return;
    }
    egui::Area::new(Id::new("titlebar-modal-drag-overlay")).order(Order::Tooltip).fixed_pos(area.min).movable(false).sense(Sense::hover()).show(ctx, |ui| {
        let response = ui.allocate_rect(area, Sense::click_and_drag());
        if response.drag_started() {
            ctx.send_viewport_cmd(ViewportCommand::StartDrag);
        }
        if response.double_clicked() {
            toggle_maximize(ctx);
        }
    });
}

impl PhotocraftApp {
    /// Whether the title bar's free gap gets the drag overlay this frame: the app draws its title
    /// strip (`chrome`: not hidden by the screen mode) and a dialog's modal layer covers it. Not
    /// only with the custom title bar: on macOS the strip under the traffic lights is the window's
    /// title bar too, and an open dialog left it undraggable.
    pub(crate) fn titlebar_modal_drag(&self, chrome: bool) -> bool {
        chrome && !self.ui.dialogs.is_empty()
    }
}

fn maximized(ctx: &egui::Context) -> bool {
    ctx.input(|i| i.viewport().maximized.unwrap_or(false))
}

fn toggle_maximize(ctx: &egui::Context) {
    ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized(ctx)));
}

/// Minimize, Maximize/Restore and Close at the right end of `bar` (the title bar's outer rect),
/// full bar height, flush with the window's edge (even if the bar's contents run wider) so a
/// maximized window's corner hits Close. Close runs File › Exit, which asks about unsaved
/// documents first.
pub fn caption_buttons(app: &mut PhotocraftApp, ui: &mut Ui, bar: Rect) {
    if caption_row(ui, bar) {
        let ctx = ui.ctx().clone();
        let _ = crate::menus::invoke(app, &ctx, "file.exit", json!({}));
    }
}

/// local-image: the caption buttons over the hosted Library; Close asks the window to close.
pub fn library_caption_buttons(ui: &mut Ui, bar: Rect) {
    if caption_row(ui, bar) {
        ui.ctx().send_viewport_cmd(ViewportCommand::Close);
    }
}

/// The three caption buttons; true when Close was clicked (Minimize and Maximize act here).
fn caption_row(ui: &mut Ui, bar: Rect) -> bool {
    let t = Tokens::get(ui.ctx());
    let max = maximized(ui.ctx());
    let right = bar.right().min(ui.ctx().content_rect().right());
    let mut clicked = None;
    for (i, c) in CAPTIONS.into_iter().enumerate() {
        let r = Rect::from_min_size(pos2(right - WIDTH + i as f32 * BUTTON_WIDTH, bar.top()), vec2(BUTTON_WIDTH, bar.height()));
        let resp = ui.interact(r, caption_id(c), Sense::click());
        let close = c == Caption::Close;
        let fill = match (resp.is_pointer_button_down_on(), resp.hovered()) {
            (true, _) if close => t.caption_close.gamma_multiply(0.8),
            (_, true) if close => t.caption_close,
            (true, _) => t.pressed,
            (_, true) => t.hover,
            _ => egui::Color32::TRANSPARENT,
        };
        ui.painter().rect_filled(r, 0.0, fill);
        let ink = if close && resp.hovered() { t.caption_close_text } else { t.icon };
        paint_glyph(ui, c, max, r.center(), Stroke::new(1.0, ink));
        let tip = match c {
            Caption::Minimize => tl!("Minimize"),
            Caption::Maximize if max => tl!("Restore"),
            Caption::Maximize => tl!("Maximize"),
            Caption::Close => tl!("Close"),
        };
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tip));
        if resp.on_hover_text(tip).clicked() {
            clicked = Some(c);
        }
    }
    match clicked {
        Some(Caption::Minimize) => ui.ctx().send_viewport_cmd(ViewportCommand::Minimized(true)),
        Some(Caption::Maximize) => toggle_maximize(ui.ctx()),
        Some(Caption::Close) => return true,
        None => {}
    }
    false
}

/// The 10 pt line glyphs of the caption buttons.
fn paint_glyph(ui: &Ui, c: Caption, maximized: bool, at: egui::Pos2, s: Stroke) {
    let p = ui.painter();
    let g = Rect::from_center_size(at, vec2(10.0, 10.0));
    match c {
        Caption::Minimize => {
            p.line_segment([g.left_center(), g.right_center()], s);
        }
        // Restore: a front square with the corner of the one behind it.
        Caption::Maximize if maximized => {
            let front = Rect::from_min_max(g.min + vec2(0.0, 2.0), g.max - vec2(2.0, 0.0));
            p.rect_stroke(front, 0.0, s, egui::StrokeKind::Middle);
            p.line(vec![g.min + vec2(2.0, 2.0), g.min + vec2(2.0, 0.0), g.right_top(), g.max - vec2(0.0, 2.0), g.max - vec2(2.0, 2.0)], s);
        }
        Caption::Maximize => {
            p.rect_stroke(g, 0.0, s, egui::StrokeKind::Middle);
        }
        Caption::Close => {
            p.line_segment([g.left_top(), g.right_bottom()], s);
            p.line_segment([g.right_top(), g.left_bottom()], s);
        }
    }
}

/// The edge and corner zones of a window with content rect `w`: (zone, direction, cursor). Corners
/// come last so they win where they overlap the edges.
fn zones(w: Rect) -> [(Rect, ResizeDirection, CursorIcon); 8] {
    use ResizeDirection::*;
    let (e, c) = (EDGE, CORNER);
    [
        (Rect::from_min_max(w.left_top(), pos2(w.right(), w.top() + e)), North, CursorIcon::ResizeVertical),
        (Rect::from_min_max(pos2(w.left(), w.bottom() - e), w.right_bottom()), South, CursorIcon::ResizeVertical),
        (Rect::from_min_max(w.left_top(), pos2(w.left() + e, w.bottom())), West, CursorIcon::ResizeHorizontal),
        (Rect::from_min_max(pos2(w.right() - e, w.top()), w.right_bottom()), East, CursorIcon::ResizeHorizontal),
        (Rect::from_min_size(w.left_top(), vec2(c, c)), NorthWest, CursorIcon::ResizeNwSe),
        (Rect::from_min_size(w.right_top() - vec2(c, 0.0), vec2(c, c)), NorthEast, CursorIcon::ResizeNeSw),
        (Rect::from_min_size(w.left_bottom() - vec2(0.0, c), vec2(c, c)), SouthWest, CursorIcon::ResizeNeSw),
        (Rect::from_min_size(w.right_bottom() - vec2(c, c), vec2(c, c)), SouthEast, CursorIcon::ResizeNwSe),
    ]
}

/// Invisible resize zones along the window's edges, above everything else; a press in one starts
/// an OS resize in that direction. None while the window is maximized or full screen.
pub fn resize_zones(ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    if maximized(&ctx) || ctx.input(|i| i.viewport().fullscreen.unwrap_or(false)) {
        return;
    }
    let w = ctx.content_rect();
    let layer = LayerId::new(Order::Foreground, Id::new("titlebar-resize"));
    let zui = ui.new_child(egui::UiBuilder::new().layer_id(layer).max_rect(w));
    for (i, (r, dir, cursor)) in zones(w).into_iter().enumerate() {
        let resp = zui.interact(r, Id::new(("titlebar-resize", i)), Sense::drag()).on_hover_cursor(cursor);
        if resp.drag_started_by(PointerButton::Primary) {
            ctx.send_viewport_cmd(ViewportCommand::BeginResize(dir));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, Pos2, RawInput, ViewportId};
    use photocraft_engine::Session;

    /// A headless window `width` pt wide showing the title bar and the resize zones.
    struct Win {
        ctx: egui::Context,
        app: PhotocraftApp,
        width: f32,
        time: f64,
        maximized: bool,
    }

    impl Win {
        fn new(width: f32) -> Self {
            let ctx = egui::Context::default();
            PhotocraftApp::setup_context(&ctx, crate::theme::ThemeKind::ALL[0]);
            let mut app = PhotocraftApp::new(Session::new(), crate::Services::default());
            app.custom_titlebar = true;
            Self { ctx, app, width, time: 0.0, maximized: false }
        }

        fn frame(&mut self, events: Vec<Event>) -> egui::FullOutput {
            self.frame_with_modal(events, false)
        }

        fn frame_with_modal(&mut self, events: Vec<Event>, modal: bool) -> egui::FullOutput {
            self.time += 0.05;
            let mut input =
                RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(self.width, 600.0))), time: Some(self.time), events, ..Default::default() };
            if let Some(v) = input.viewports.get_mut(&ViewportId::ROOT) {
                v.maximized = Some(self.maximized);
            }
            let app = &mut self.app;
            let mut out = self.ctx.run_ui(input, |ui| {
                crate::panels::title_bar(app, ui);
                if modal {
                    egui::Modal::new(Id::new("titlebar-drag-test-modal")).backdrop_color(egui::Color32::TRANSPARENT).show(ui.ctx(), |ui| {
                        ui.label("Modal dialog");
                    });
                    modal_drag_overlay(ui.ctx());
                }
                resize_zones(ui);
            });
            out.textures_delta.clear();
            out
        }

        /// Frames for a pointer gesture: move to `path[0]`, press, move along `path`, release. The
        /// viewport commands the frames sent.
        fn gesture(&mut self, path: &[Pos2]) -> Vec<ViewportCommand> {
            self.gesture_with_modal(path, false)
        }

        fn gesture_with_modal(&mut self, path: &[Pos2], modal: bool) -> Vec<ViewportCommand> {
            let mut events = vec![vec![Event::PointerMoved(path[0])], vec![button(path[0], true)]];
            events.extend(path[1..].iter().map(|p| vec![Event::PointerMoved(*p)]));
            events.push(vec![button(*path.last().unwrap_or(&path[0]), false)]);
            events
                .into_iter()
                .flat_map(|e| self.frame_with_modal(e, modal).viewport_output.remove(&ViewportId::ROOT).map(|v| v.commands).unwrap_or_default())
                .collect()
        }

        fn click(&mut self, p: Pos2) -> Vec<ViewportCommand> {
            self.gesture(&[p])
        }

        fn caption(&self, c: Caption) -> Rect {
            self.ctx.read_response(caption_id(c)).map(|r| r.rect).unwrap_or(Rect::NOTHING)
        }

        /// The bar's clickable widgets other than its drag gap and the caption buttons.
        fn bar_widgets(&self) -> Vec<Rect> {
            let skip = CAPTIONS.map(caption_id);
            self.ctx.viewport(|v| {
                v.prev_pass
                    .widgets
                    .layers()
                    .filter(|(l, _)| l.order != Order::Foreground)
                    .flat_map(|(_, ws)| ws.iter())
                    .filter(|w| w.sense.senses_click() && !w.sense.senses_drag() && w.rect.top() < 30.0 && !skip.contains(&w.id))
                    .map(|w| w.rect)
                    .collect()
            })
        }
    }

    fn texts(out: &egui::FullOutput) -> Vec<(String, Rect)> {
        fn walk(s: &egui::Shape, v: &mut Vec<(String, Rect)>) {
            match s {
                egui::Shape::Text(t) => v.push((t.galley.text().to_string(), Rect::from_min_size(t.pos, t.galley.size()))),
                egui::Shape::Vec(s) => s.iter().for_each(|s| walk(s, v)),
                _ => {}
            }
        }
        let mut v = vec![];
        out.shapes.iter().for_each(|c| walk(&c.shape, &mut v));
        v
    }

    fn button(p: Pos2, pressed: bool) -> Event {
        Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() }
    }

    #[test]
    fn empty_bar_space_still_drags_with_a_modal_open() {
        let mut w = Win::new(1440.0);
        w.frame(vec![]);
        w.frame_with_modal(vec![], true);
        let empty = pos2(980.0, 16.0);
        let drag_area = w.ctx.data(|d| d.get_temp::<Rect>(modal_drag_area_id()));
        assert!(drag_area.is_some_and(|r| r.contains(empty)), "the test point must be in the current title-bar gap: {drag_area:?}");
        assert!(w.bar_widgets().iter().all(|r| !r.contains(empty)), "the probe point must be empty bar space");
        let cmds = w.gesture_with_modal(&[empty, empty + vec2(12.0, 4.0), empty + vec2(30.0, 8.0)], true);
        assert_eq!(cmds.iter().filter(|c| matches!(c, ViewportCommand::StartDrag)).count(), 1, "{cmds:?}");
    }

    #[test]
    fn modal_drag_overlay_does_not_cover_a_dialog_moved_into_the_gap() {
        let mut w = Win::new(1440.0);
        w.frame(vec![]);
        w.frame_with_modal(vec![], true);
        let area = w.ctx.data(|d| d.get_temp::<Rect>(modal_drag_area_id())).unwrap_or(Rect::NOTHING);
        assert!(area.contains(pos2(980.0, 16.0)), "the probe point must be in the gap: {area:?}");
        w.ctx.data_mut(|d| d.insert_temp(Id::new("pc-dialog-rects"), vec![area]));
        let cmds = w.gesture_with_modal(&[area.center(), area.center() + vec2(12.0, 4.0)], true);
        assert!(!cmds.iter().any(|c| matches!(c, ViewportCommand::StartDrag)), "{cmds:?}");
    }

    /// The drag overlay under a dialog doesn't depend on the custom title bar: on macOS (OS
    /// traffic lights over the app's own title strip) a dialog left the window undraggable.
    #[test]
    fn a_dialog_keeps_the_title_strip_draggable_with_either_title_bar() {
        for custom in [false, true] {
            let mut app = PhotocraftApp::new(Session::new(), crate::Services::default());
            app.custom_titlebar = custom;
            app.run("file.new", json!({"width": 8, "height": 8})).unwrap();
            assert!(!app.titlebar_modal_drag(true), "no dialog, no overlay");
            assert!(crate::filter_dialog::open(&mut app, "filter.blur.gaussianBlur").is_some());
            assert!(app.titlebar_modal_drag(true), "custom title bar: {custom}");
            assert!(!app.titlebar_modal_drag(false), "chrome hidden (full screen): no title strip");
        }
    }

    #[test]
    fn caption_buttons_take_the_right_end_and_nothing_overlaps_them() {
        for width in [760.0, 1100.0, 1440.0] {
            let mut w = Win::new(width);
            w.frame(vec![]);
            let out = w.frame(vec![]);
            let [min, max, close] = CAPTIONS.map(|c| w.caption(c));
            assert_eq!((close.right(), close.top()), (width, 0.0), "Close sits in the window's corner at {width}");
            assert!(close.height() >= 32.0 && [min, max, close].iter().all(|r| r.width() == BUTTON_WIDTH));
            assert_eq!((min.right(), max.right()), (max.left(), close.left()));
            let widgets = w.bar_widgets();
            assert!(widgets.len() >= 10, "menus, search, theme, workspace: {widgets:?}");
            for a in &widgets {
                assert!(a.right() <= min.left(), "{a:?} runs into the caption buttons at {width}");
            }
            // local-image: no community chat button (Local Image has none of its own).
            assert!(!texts(&out).iter().any(|(t, _)| t == "Discord"), "Discord at {width}");
        }
    }

    #[test]
    fn the_brand_mark_leads_the_bar() {
        let mut w = Win::new(1440.0);
        w.frame(vec![]);
        let out = w.frame(vec![]);
        let mark = crate::brand::mark_rect(&w.ctx).expect("the brand mark is painted");
        let file = texts(&out).into_iter().find(|(t, _)| t == "File").map(|(_, r)| r).expect("the File menu title");
        assert!(mark.right() < file.left(), "the mark sits left of the menus: {mark:?} {file:?}");
        assert!(mark.left() < 20.0 && mark.width() >= 16.0, "{mark:?}");
    }

    #[test]
    fn close_runs_file_exit_which_asks_about_unsaved_work() {
        let mut w = Win::new(1100.0);
        w.frame(vec![]);
        let close = w.caption(Caption::Close).center();
        let cmds = w.click(close);
        // File › Exit asks the window to close; the unsaved-changes guard (`discard_ui`) then
        // decides, so the caption button and the menu item behave the same.
        assert!(cmds.iter().any(|c| matches!(c, ViewportCommand::Close)), "{cmds:?}");
    }

    #[test]
    fn minimize_and_maximize_follow_the_window_state() {
        let mut w = Win::new(1100.0);
        w.frame(vec![]);
        let (min, max) = (w.caption(Caption::Minimize).center(), w.caption(Caption::Maximize).center());
        assert!(w.click(min).contains(&ViewportCommand::Minimized(true)));
        assert!(w.click(max).contains(&ViewportCommand::Maximized(true)));
        w.maximized = true;
        assert!(w.click(max).contains(&ViewportCommand::Maximized(false)), "Restore");
    }

    #[test]
    fn empty_bar_space_drags_and_double_click_maximizes() {
        let mut w = Win::new(1440.0);
        w.frame(vec![]);
        let empty = pos2(980.0, 16.0);
        assert!(w.bar_widgets().iter().all(|r| !r.contains(empty)), "the probe point must be empty bar space");
        let cmds = w.gesture(&[empty, empty + vec2(12.0, 4.0), empty + vec2(30.0, 8.0)]);
        assert_eq!(cmds.iter().filter(|c| matches!(c, ViewportCommand::StartDrag)).count(), 1, "{cmds:?}");
        let mut cmds = w.click(empty);
        cmds.extend(w.click(empty));
        assert!(cmds.contains(&ViewportCommand::Maximized(true)), "{cmds:?}");
        w.maximized = true;
        w.time += 1.0;
        let mut cmds = w.click(empty);
        cmds.extend(w.click(empty));
        assert!(cmds.contains(&ViewportCommand::Maximized(false)), "{cmds:?}");
        // Widgets keep their clicks: dragging from a menu title doesn't move the window.
        let file = texts(&w.frame(vec![])).into_iter().find(|(t, _)| t == "File").map(|(_, r)| r.center());
        let file = file.expect("the File menu title");
        let cmds = w.gesture(&[file, file + vec2(12.0, 4.0), file + vec2(30.0, 8.0)]);
        assert!(!cmds.iter().any(|c| matches!(c, ViewportCommand::StartDrag)), "{cmds:?}");
    }

    #[test]
    fn edge_zones_resize_unless_maximized() {
        use ResizeDirection::*;
        let mut w = Win::new(1000.0);
        w.frame(vec![]);
        for (p, dir) in [
            (pos2(1.0, 300.0), West),
            (pos2(998.0, 300.0), East),
            (pos2(500.0, 1.0), North),
            (pos2(500.0, 598.0), South),
            (pos2(2.0, 2.0), NorthWest),
            (pos2(997.0, 3.0), NorthEast),
            (pos2(3.0, 597.0), SouthWest),
            (pos2(998.0, 598.0), SouthEast),
        ] {
            let cmds = w.gesture(&[p, p + vec2(6.0, 6.0)]);
            assert_eq!(cmds, vec![ViewportCommand::BeginResize(dir)], "at {p:?}");
        }
        let out = w.frame(vec![Event::PointerMoved(pos2(1.0, 300.0))]);
        assert_eq!(out.platform_output.cursor_icon, CursorIcon::ResizeHorizontal);
        // Maximized: no zones, and the window's top-right corner is Close.
        w.maximized = true;
        let cmds = w.gesture(&[pos2(1.0, 300.0), pos2(8.0, 300.0)]);
        assert!(!cmds.iter().any(|c| matches!(c, ViewportCommand::BeginResize(_))), "{cmds:?}");
        assert!(w.click(pos2(999.5, 0.5)).iter().any(|c| matches!(c, ViewportCommand::Close)));
    }

    /// Drive the real Compositing UI with its host module switch, rather than an isolated bar.
    #[test]
    fn compositing_host_titlebar_drag_captions_and_resize_matrix() {
        use egui_kittest::kittest::Queryable;
        for size in [vec2(1800.0, 1000.0), vec2(1280.0, 800.0), vec2(2560.0, 1440.0)] {
            for theme in crate::theme::ThemeKind::ALL {
                let mut h = egui_kittest::Harness::builder().with_size(size).with_step_dt(1.0 / 60.0).build_eframe(move |cc| {
                    PhotocraftApp::setup_context(&cc.egui_ctx, theme);
                    let mut session = Session::new();
                    session.execute("prefs.set", json!({"values": {"interface.theme": theme.id()}})).unwrap();
                    let mut app = PhotocraftApp::new(session, crate::Services::default());
                    app.custom_titlebar = true;
                    app.host_modes = true;
                    app
                });
                h.run_steps(6);
                assert_eq!(crate::theme::Tokens::get(&h.ctx).kind, theme);
                let gap = h.ctx.viewport(|v| {
                    v.prev_pass
                        .widgets
                        .layers()
                        .flat_map(|(_, ws)| ws.iter())
                        .find(|w| w.rect.top() == 0.0 && w.sense.senses_click() && w.sense.senses_drag())
                        .expect("the title bar's drag response")
                        .rect
                });
                assert!(gap.width() > 100.0, "{theme:?}, {size:?}: {gap:?}");
                let captions = ["Minimize", "Maximize", "Close"].map(|label| h.get_by_label(label).rect());
                for (i, rect) in captions.into_iter().enumerate() {
                    assert_eq!(rect.top(), 0.0, "nothing sits above the title bar");
                    assert!(rect.is_positive());
                    assert_eq!(rect.right(), size.x - (2 - i) as f32 * BUTTON_WIDTH);
                    assert!(rect.left() >= gap.right());
                }
                let gesture = |h: &mut egui_kittest::Harness<'_, PhotocraftApp>, from, to| {
                    let mut commands = vec![];
                    for event in [Event::PointerMoved(from), button(from, true), Event::PointerMoved(to), button(to, false)] {
                        h.event(event);
                        h.run_steps(1);
                        commands.extend(h.output().viewport_output[&ViewportId::ROOT].commands.clone());
                    }
                    commands
                };
                for x in [gap.left() + 2.0, gap.center().x, gap.right() - 2.0] {
                    let p = pos2(x, gap.center().y);
                    let cmds = gesture(&mut h, p, p + vec2(20.0, 8.0));
                    assert_eq!(cmds.iter().filter(|c| **c == ViewportCommand::StartDrag).count(), 1, "{theme:?}, {size:?}: {cmds:?}");
                }
                let p = gap.center();
                let mut cmds = gesture(&mut h, p, p);
                cmds.extend(gesture(&mut h, p, p));
                assert!(cmds.contains(&ViewportCommand::Maximized(true)), "{cmds:?}");
                // Restore through the same double-click gesture.
                h.input_mut().viewports.get_mut(&ViewportId::ROOT).unwrap().maximized = Some(true);
                h.run_steps(60);
                let mut cmds = gesture(&mut h, p, p);
                cmds.extend(gesture(&mut h, p, p));
                assert!(cmds.contains(&ViewportCommand::Maximized(false)), "{cmds:?}");
                h.input_mut().viewports.get_mut(&ViewportId::ROOT).unwrap().maximized = Some(false);
                assert!(gesture(&mut h, captions[0].center(), captions[0].center()).contains(&ViewportCommand::Minimized(true)));
                assert!(gesture(&mut h, captions[1].center(), captions[1].center()).contains(&ViewportCommand::Maximized(true)));
                let p = pos2(gap.center().x, 1.0);
                let cmds = gesture(&mut h, p, p + vec2(20.0, 8.0));
                assert_eq!(cmds, [ViewportCommand::BeginResize(egui::ResizeDirection::North)]);
                // The menus and module buttons keep their gestures, including drag attempts.
                for label in ["File", "Develop"] {
                    let p = h.get_by_label(label).rect().center();
                    assert!(!gesture(&mut h, p, p + vec2(20.0, 8.0)).contains(&ViewportCommand::StartDrag));
                    h.key_press(egui::Key::Escape);
                    h.run_steps(2);
                }
                h.get_by_label("Library").click();
                h.run_steps(2);
                assert_eq!(h.state().switch_module, Some(crate::Module::Library));
                // Close remains reachable at the window's far right.
                assert!(gesture(&mut h, captions[2].center(), captions[2].center()).contains(&ViewportCommand::Close));
            }
        }
    }
}
