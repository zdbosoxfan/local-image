//! Document tabs (simple and pro strips), the tab context menu and the Home screen.
use super::*;

pub(super) fn documents(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    app.drop_canvas_rect = None;
    app.tab_strip = None;
    retain_gpu_documents(app);
    crate::transform_tool::track_steps(app, ui.ctx());
    let n = app.session.documents().len();
    // Files opening in the background (#210) have tabs before they have documents.
    let opening = !app.jobs.opens.is_empty();
    if !opening && app.ui.chrome.shows_home(n, app.session.prefs().general.auto_show_home_screen) {
        start_screen(app, ui);
        return;
    }
    if n == 0 && !opening {
        // Auto show the Home Screen is off: an empty workspace, like Photoshop.
        paint_dots(ui, ui.available_rect_before_wrap());
        return;
    }
    if !app.ui.view.hides_tabs() || opening {
        app.tab_strip = Some(tabs(app, ui));
        drop_slot_line(app, ui);
    }
    if let Some(job) = app.jobs.focus.or_else(|| (n == 0).then(|| app.jobs.opens.last().map(|o| o.job)).flatten()) {
        crate::jobs_ui::open_card(app, ui, job);
        return;
    }
    let Some(idx) = app.session.active_index() else { return };
    let rect = ui.available_rect_before_wrap();
    app.last_canvas_rect = rect;
    app.drop_canvas_rect = Some(rect);
    // Window › Arrange: tiled / n-up layouts show several documents side by side; the active one
    // takes input, a click elsewhere activates that document.
    if let Some(shown) = arranged_cells(app, rect) {
        let t = crate::theme::Tokens::get(ui.ctx());
        for (d, cell) in shown {
            let cell = cell.shrink(1.0);
            let view = app.ui.views[d].clone();
            let v = canvas_view(app, ui, d, cell, view, d == idx);
            if d != idx {
                app.ui.views[d] = v;
                if ui.input(|i| i.pointer.primary_pressed() && i.pointer.interact_pos().is_some_and(|p| cell.contains(p))) {
                    app.session.set_active(d);
                }
            }
            let stroke = if d == idx { Stroke::new(1.5, t.accent) } else { Stroke::new(1.0, Color32::from_black_alpha(160)) };
            ui.painter().rect_stroke(cell, 0, stroke, egui::StrokeKind::Inside);
        }
        return;
    }
    let view = app.ui.views[idx].clone();
    canvas_view(app, ui, idx, rect, view, true);
}

/// The tab strip; returns where its document tabs are.
pub(super) fn tabs(app: &mut PhotocraftApp, ui: &mut egui::Ui) -> TabStrip {
    let t = crate::theme::Tokens::get(ui.ctx());
    if t.pro {
        return pro_tabs(app, ui);
    }
    let active = app.session.active_index();
    let mut activate = None;
    let mut close = None;
    let mut tab_action = None;
    let tab_count = app.session.documents().len();
    let (mut focus_open, mut cancel_open) = (None, None);
    let focused_open = app.jobs.focus.is_some();
    // Layers dragged over a tab show its document (`layer_transfer`).
    let dragging = crate::layer_transfer::pointer_if_armed(app, ui.ctx());
    let mut drag_over = None;
    let mut doc_tabs = Vec::with_capacity(tab_count);
    let frame = egui::Frame::NONE.fill(t.canvas).inner_margin(egui::Margin { left: 8, right: 8, top: 6, bottom: 4 }).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for (i, st) in app.session.documents().iter().enumerate() {
                let sel = Some(i) == active && !focused_open;
                let name = format!("{}{}", st.doc.name, if st.is_dirty() { " *" } else { "" });
                let meta = format!("{}/{}", mode_label(&st.doc), st.doc.depth.bits());
                let name_g = ui.painter().layout_no_wrap(name, crate::theme::medium(12.5), t.text);
                let meta_g = ui.painter().layout_no_wrap(meta, egui::FontId::proportional(10.5), t.text_faint);
                let w = name_g.size().x + meta_g.size().x + 44.0;
                let (r, resp) = ui.allocate_exact_size(egui::vec2(w, 26.0), Sense::click());
                resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, sel, &st.doc.name));
                doc_tabs.push(r);
                if sel {
                    ui.painter().rect_filled(r, t.radius_sm, t.card);
                    ui.painter().rect_stroke(r, t.radius_sm, Stroke::new(1.0, t.card_border), egui::StrokeKind::Inside);
                } else if resp.hovered() {
                    ui.painter().rect_filled(r, t.radius_sm, t.hover.gamma_multiply(0.5));
                }
                if dragging.is_some_and(|p| r.contains(p)) {
                    drag_over = Some(i);
                    ui.painter().rect_stroke(r, t.radius_sm, Stroke::new(1.5, t.accent), egui::StrokeKind::Inside);
                }
                let color = if sel { t.text } else { t.text_dim };
                let ny = r.center().y - name_g.size().y / 2.0;
                ui.painter().galley_with_override_text_color(egui::pos2(r.left() + 10.0, ny), name_g.clone(), color);
                ui.painter().galley(egui::pos2(r.left() + 16.0 + name_g.size().x, r.center().y - meta_g.size().y / 2.0), meta_g, t.text_faint);
                let xr = Rect::from_center_size(egui::pos2(r.right() - 12.0, r.center().y), egui::vec2(16.0, 16.0));
                let xresp = ui.interact(xr, ui.id().with(("tabx", i)), Sense::click());
                if xresp.hovered() {
                    ui.painter().rect_filled(xr, 4.0, t.hover);
                }
                crate::icons::paint(ui, xr, "x", 11.0, if xresp.hovered() { t.text } else { t.text_faint });
                if xresp.clicked() {
                    close = Some(i);
                } else if resp.clicked() {
                    activate = Some(i);
                }
                resp.context_menu(|ui| {
                    tab_action = tab_context_menu(ui, i, tab_count);
                });
            }
            // Files opening in the background: a tab with a progress underline; × cancels.
            for (job, name, frac) in crate::jobs_ui::open_tabs(app) {
                let sel = app.jobs.focus == Some(job);
                let name_g = ui.painter().layout_no_wrap(name, crate::theme::medium(12.5), t.text);
                let meta_g = ui.painter().layout_no_wrap(format!("{:.0}%", frac * 100.0), egui::FontId::proportional(10.5), t.text_faint);
                let w = name_g.size().x + meta_g.size().x + 44.0;
                let (r, resp) = ui.allocate_exact_size(egui::vec2(w, 26.0), Sense::click());
                if sel {
                    ui.painter().rect_filled(r, t.radius_sm, t.card);
                    ui.painter().rect_stroke(r, t.radius_sm, Stroke::new(1.0, t.card_border), egui::StrokeKind::Inside);
                } else if resp.hovered() {
                    ui.painter().rect_filled(r, t.radius_sm, t.hover.gamma_multiply(0.5));
                }
                let ny = r.center().y - name_g.size().y / 2.0;
                ui.painter().galley_with_override_text_color(egui::pos2(r.left() + 10.0, ny), name_g.clone(), if sel { t.text } else { t.text_dim });
                ui.painter().galley(egui::pos2(r.left() + 16.0 + name_g.size().x, r.center().y - meta_g.size().y / 2.0), meta_g, t.text_faint);
                crate::jobs_ui::tab_underline(ui, r, frac, &t);
                let xr = Rect::from_center_size(egui::pos2(r.right() - 12.0, r.center().y), egui::vec2(16.0, 16.0));
                let xresp = ui.interact(xr, ui.id().with(("tabjobx", job.0)), Sense::click());
                if xresp.hovered() {
                    ui.painter().rect_filled(xr, 4.0, t.hover);
                }
                crate::icons::paint(ui, xr, "x", 11.0, if xresp.hovered() { t.text } else { t.text_faint });
                if xresp.on_hover_text(tl!("Cancel opening")).clicked() {
                    cancel_open = Some(job);
                } else if resp.clicked() {
                    focus_open = Some(job);
                }
            }
        });
    });
    open_tab_clicks(app, activate, focus_open, cancel_open);
    if let Some(i) = drag_over {
        crate::layer_transfer::over_tab(app, ui.ctx(), i);
    }
    if let Some(i) = close {
        let _ = crate::menus::invoke(app, ui.ctx(), "file.close", json!({"document": i}));
    }
    if let Some((id, params)) = tab_action
        && let Err(e) = crate::menus::invoke(app, ui.ctx(), id, params)
    {
        app.ui.status = e;
        app.ui.status_error = true;
    }
    TabStrip { rect: frame.response.rect, tabs: doc_tabs }
}

/// All tab actions go through the same guarded File commands as the menu bar, including the
/// unsaved-changes prompt. The clicked tab is explicit even when another document is active.
pub(super) fn tab_context_items(index: usize, count: usize) -> [(&'static str, &'static str, serde_json::Value, bool); 3] {
    [
        ("Close", "file.close", json!({"document": index}), true),
        ("Close Others", "file.closeOthers", json!({"document": index}), count > 1),
        ("Close All", "file.closeAll", json!({}), true),
    ]
}

pub(super) fn tab_context_menu(ui: &mut egui::Ui, index: usize, count: usize) -> Option<(&'static str, serde_json::Value)> {
    crate::widgets::menu_scroll(ui, |ui| {
        ui.set_min_width(170.0);
        for (label, id, params, enabled) in tab_context_items(index, count) {
            if ui.add_enabled(enabled, egui::Button::new(tl!(label))).clicked() {
                ui.close();
                return Some((id, params));
            }
        }
        None
    })
}

/// Apply tab-strip clicks: a document tab shows that document, an opening tab its progress, and
/// an opening tab's × cancels the open (closing the half-open tab).
pub(super) fn open_tab_clicks(
    app: &mut PhotocraftApp,
    activate: Option<usize>,
    focus_open: Option<photocraft_engine::jobs::JobId>,
    cancel_open: Option<photocraft_engine::jobs::JobId>,
) {
    if let Some(i) = activate {
        app.session.set_active(i);
        app.jobs.focus = None;
    }
    if let Some(job) = focus_open {
        app.jobs.focus = Some(job);
    }
    if let Some(job) = cancel_open {
        crate::jobs_ui::cancel(app, job);
    }
}

/// Photoshop document tabs: "name @ 33.3% (RGB/8)" on a dark strip; active tab matches panels.
pub(super) fn pro_tabs(app: &mut PhotocraftApp, ui: &mut egui::Ui) -> TabStrip {
    let t = crate::theme::Tokens::get(ui.ctx());
    let active = app.session.active_index().filter(|_| app.jobs.focus.is_none());
    let (mut activate, mut close) = (None, None);
    let mut tab_action = None;
    let tab_count = app.session.documents().len();
    let dragging = crate::layer_transfer::pointer_if_armed(app, ui.ctx());
    let mut drag_over = None;
    let mac = ui.ctx().os() == egui::os::OperatingSystem::Mac;
    let (strip, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 26.0), Sense::hover());
    ui.painter().rect_filled(strip, 0.0, t.tab_strip);
    let mut x = strip.left();
    let mut doc_tabs = Vec::with_capacity(tab_count);
    for (i, st) in app.session.documents().iter().enumerate() {
        let zoom = app.ui.views.get(i).map_or(100.0, |v| v.zoom * 100.0);
        // Photoshop: "name @ 50% (Layer 1, RGB/8)", "(Layer 1, Layer Mask/8)" when the mask is targeted;
        // the Background layer's name is omitted.
        let active_layer = st.active_layer.and_then(|id| st.doc.layer(id)).filter(|l| !(l.name == "Background" && l.locks.transparency));
        let is_active_doc = app.session.active_index() == Some(i);
        let mask = is_active_doc && app.ui.mask_target && active_layer.is_some_and(|l| l.mask.is_some());
        let model = if mask { tl!("Layer Mask").to_string() } else { mode_label(&st.doc).to_string() };
        let layer = active_layer.map(|l| format!("{}, ", l.name)).unwrap_or_default();
        let title = format!("{} @ {}% ({layer}{model}/{}){}", st.doc.name, fmt_zoom(zoom), st.doc.depth.bits(), if st.is_dirty() { "*" } else { "" });
        let g = ui.painter().layout_no_wrap(title, egui::FontId::proportional(11.5), t.text);
        let r = Rect::from_min_size(egui::pos2(x, strip.top()), egui::vec2(g.size().x + 42.0, strip.height()));
        doc_tabs.push(r);
        let resp = ui.interact(r, ui.id().with(("ptab", i)), Sense::click());
        let sel = Some(i) == active;
        if sel {
            ui.painter().rect_filled(r, 0.0, t.chrome);
        } else if resp.hovered() {
            ui.painter().rect_filled(r, 0.0, t.hover.gamma_multiply(0.35));
        }
        if dragging.is_some_and(|p| r.contains(p)) {
            drag_over = Some(i);
            ui.painter().rect_stroke(r, 0.0, Stroke::new(1.5, t.accent), egui::StrokeKind::Inside);
        }
        ui.painter().line_segment([r.right_top(), r.right_bottom()], Stroke::new(1.0, t.separator));
        let (xr, at) = pro_tab_layout(r, g.size().y, mac);
        let xresp = ui.interact(xr, ui.id().with(("ptabx", i)), Sense::click());
        // Painted: name the tab and its × for accessibility (and so tests and agents can find them).
        resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, sel, g.text()));
        xresp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tl!("Close")));
        crate::icons::paint(ui, xr, "x", 10.0, if xresp.hovered() { t.text } else { t.text_faint });
        ui.painter().galley_with_override_text_color(at, g, if sel { t.text } else { t.text_faint });
        if xresp.clicked() {
            close = Some(i);
        } else if resp.clicked() {
            activate = Some(i);
        }
        resp.context_menu(|ui| {
            tab_action = tab_context_menu(ui, i, tab_count);
        });
        x = r.right();
    }
    // Files opening in the background (#210): "name (Opening… 45%)" with a progress underline.
    let (mut focus_open, mut cancel_open) = (None, None);
    for (job, name, frac) in crate::jobs_ui::open_tabs(app) {
        let title = format!("{name} ({} {:.0}%)", tl!("Opening…"), frac * 100.0);
        let g = ui.painter().layout_no_wrap(title, egui::FontId::proportional(11.5), t.text);
        let r = Rect::from_min_size(egui::pos2(x, strip.top()), egui::vec2(g.size().x + 42.0, strip.height()));
        let resp = ui.interact(r, ui.id().with(("ptabjob", job.0)), Sense::click());
        let sel = app.jobs.focus == Some(job);
        if sel {
            ui.painter().rect_filled(r, 0.0, t.chrome);
        } else if resp.hovered() {
            ui.painter().rect_filled(r, 0.0, t.hover.gamma_multiply(0.35));
        }
        ui.painter().line_segment([r.right_top(), r.right_bottom()], Stroke::new(1.0, t.separator));
        crate::jobs_ui::tab_underline(ui, r, frac, &t);
        let (xr, at) = pro_tab_layout(r, g.size().y, mac);
        let xresp = ui.interact(xr, ui.id().with(("ptabjobx", job.0)), Sense::click());
        crate::icons::paint(ui, xr, "x", 10.0, if xresp.hovered() { t.text } else { t.text_faint });
        ui.painter().galley_with_override_text_color(at, g, if sel { t.text } else { t.text_faint });
        if xresp.on_hover_text(tl!("Cancel opening")).clicked() {
            cancel_open = Some(job);
        } else if resp.clicked() {
            focus_open = Some(job);
        }
        x = r.right();
    }
    open_tab_clicks(app, activate, focus_open, cancel_open);
    if let Some(i) = drag_over {
        crate::layer_transfer::over_tab(app, ui.ctx(), i);
    }
    if let Some(i) = close {
        let _ = crate::menus::invoke(app, ui.ctx(), "file.close", json!({"document": i}));
    }
    if let Some((id, params)) = tab_action
        && let Err(e) = crate::menus::invoke(app, ui.ctx(), id, params)
    {
        app.ui.status = e;
        app.ui.status_error = true;
    }
    TabStrip { rect: strip, tabs: doc_tabs }
}

/// Where the document tabs were drawn (opening files' tabs aren't slots): a file dropped on the
/// strip opens at the slot under the pointer.
#[derive(Clone, Debug, PartialEq)]
pub struct TabStrip {
    pub rect: Rect,
    /// The document tabs, left to right.
    pub tabs: Vec<Rect>,
}

impl TabStrip {
    /// The tab position a drop at `x` opens at: before the first tab whose middle is right of it.
    pub fn slot(&self, x: f32) -> usize {
        self.tabs.iter().filter(|r| r.center().x < x).count()
    }
}

/// While files are dragged over the tab strip, an insertion line where they would open.
pub(super) fn drop_slot_line(app: &mut PhotocraftApp, ui: &egui::Ui) {
    if ui.input(|i| i.raw.hovered_files.is_empty()) {
        return;
    }
    let at = app.services.cursor_pos.as_mut().and_then(|f| f(ui.ctx()));
    let crate::file_open::DropTarget::Tabs(slot) = app.drop_target(ui.ctx(), at) else { return };
    let Some(tabs) = app.tab_strip.as_ref().map(|s| &s.tabs) else { return };
    let Some((r, after)) = tabs.get(slot).map(|r| (*r, false)).or_else(|| tabs.last().map(|r| (*r, true))) else { return };
    crate::widgets::drop_line(ui, r, after, true, &crate::theme::Tokens::get(ui.ctx()));
}

/// A Photoshop-style tab's close button and where its title starts. Photoshop puts the × after the
/// title on Windows (and so Linux) and before it on macOS (#619).
pub(super) fn pro_tab_layout(tab: Rect, title_height: f32, mac: bool) -> (Rect, egui::Pos2) {
    let (x, title) = if mac { (tab.left() + 13.0, tab.left() + 26.0) } else { (tab.right() - 13.0, tab.left() + 16.0) };
    (Rect::from_center_size(egui::pos2(x, tab.center().y), egui::vec2(14.0, 14.0)), egui::pos2(title, tab.center().y - title_height / 2.0))
}

/// Photoshop-style zoom label: "33.3", "100", "12.5".
pub fn fmt_zoom(pct: f32) -> String {
    if (pct - pct.round()).abs() < 0.05 { format!("{}", pct.round() as i64) } else { format!("{pct:.1}") }
}

/// Repeating dot-grid texture for the canvas surround.
pub(super) fn dots(ctx: &egui::Context, t: &crate::theme::Tokens) -> Option<egui::TextureId> {
    if t.bevel {
        return None;
    }
    let key = egui::Id::new(("canvas-dots", format!("{:?}", t.kind)));
    if let Some(tex) = ctx.data(|d| d.get_temp::<egui::TextureHandle>(key)) {
        return Some(tex.id());
    }
    let n = 22usize;
    let mut px = vec![Color32::TRANSPARENT; n * n];
    for (dx, dy, a) in [(0usize, 0usize, 255u8), (1, 0, 110), (0, 1, 110), (1, 1, 60)] {
        let c = t.canvas_dot;
        px[(n / 2 + dy) * n + n / 2 + dx] = Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a);
    }
    let opts = TextureOptions {
        magnification: egui::TextureFilter::Linear,
        minification: egui::TextureFilter::Linear,
        wrap_mode: egui::TextureWrapMode::Repeat,
        mipmap_mode: None,
    };
    let tex = ctx.load_texture("canvas-dots", egui::ColorImage::new([n, n], px), opts);
    let id = tex.id();
    ctx.data_mut(|d| d.insert_temp(key, tex));
    Some(id)
}

pub(super) fn paint_dots(ui: &egui::Ui, rect: Rect) {
    let t = crate::theme::Tokens::get(ui.ctx());
    if let Some(id) = dots(ui.ctx(), &t) {
        let uv = Rect::from_min_max(Pos2::ZERO, pos2(rect.width() / 22.0, rect.height() / 22.0));
        ui.painter_at(rect).image(id, rect, uv, Color32::WHITE);
    }
}

pub fn mode_label(doc: &Document) -> &'static str {
    match doc.mode {
        photocraft_doc::ColorMode::Rgb => "RGB",
        photocraft_doc::ColorMode::Grayscale => "Gray",
        photocraft_doc::ColorMode::Cmyk => "CMYK",
        photocraft_doc::ColorMode::Lab => "Lab",
        photocraft_doc::ColorMode::Indexed => "Indexed",
        photocraft_doc::ColorMode::Bitmap => "Bitmap",
        photocraft_doc::ColorMode::Duotone => "Duotone",
        photocraft_doc::ColorMode::Multichannel => "Multichannel",
    }
}

pub(super) fn start_screen(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let t = crate::theme::Tokens::get(ui.ctx());
    let area = ui.available_rect_before_wrap();
    paint_dots(ui, area);
    // File › Open Recent, newest first (on the web there are no paths to reopen).
    let recent: Vec<String> = if cfg!(target_arch = "wasm32") { Vec::new() } else { app.ui.recent_files.iter().take(HOME_RECENT).cloned().collect() };
    let recent_h = if recent.is_empty() { 0.0 } else { 34.0 + recent.len() as f32 * HOME_RECENT_ROW };
    let card = Rect::from_center_size(area.center(), egui::vec2(520.0, 380.0 + recent_h));
    let new_label = crate::shortcuts::command_label(app, "New document…", "file.new");
    let open_label = crate::shortcuts::command_label(app, "Open…", "file.open");
    ui.scope_builder(egui::UiBuilder::new().max_rect(card), |ui| {
        ui.vertical_centered(|ui| {
            ui.horizontal(|ui| {
                let title = ui.painter().layout_no_wrap(tl!("Local Image").into(), crate::theme::semibold(38.0), t.text);
                let by = ui.painter().layout_no_wrap("open source".into(), egui::FontId::proportional(13.0), t.text_faint);
                let total = title.size().x + by.size().x + 10.0;
                ui.add_space(((card.width() - total) / 2.0).max(0.0));
                let (r, _) = ui.allocate_exact_size(title.size(), Sense::hover());
                ui.painter().galley(r.min, title, t.text);
                let (r2, _) = ui.allocate_exact_size(egui::vec2(by.size().x + 10.0, r.height()), Sense::hover());
                ui.painter().galley(egui::pos2(r2.left() + 10.0, r.bottom() - by.size().y - 8.0), by, t.text_faint);
            });
            ui.add_space(6.0);
            ui.label(egui::RichText::new(tl!("Edit photos, catalogue them, and create with local AI.")).color(t.text_dim).size(14.0));
            ui.add_space(22.0);
            let bw = 160.0;
            ui.horizontal(|ui| {
                ui.add_space(((card.width() - 3.0 * bw - 2.0 * 10.0) / 2.0).max(0.0));
                ui.spacing_mut().item_spacing.x = 10.0;
                if crate::widgets::primary_button(ui, &new_label, bw).clicked() {
                    let fields = crate::new_doc_ui::fields(app);
                    app.ui.open_dialog(crate::state::DialogKind::NewDocument, fields);
                }
                if crate::widgets::secondary_button(ui, &open_label, bw).clicked() {
                    app.open_dialog_file();
                }
                if crate::widgets::secondary_button(ui, tl!("Open Folder…"), bw).clicked() {
                    let ctx = ui.ctx().clone();
                    if let Err(e) = crate::menus::invoke(app, &ctx, "li.openFolder", serde_json::json!({})) {
                        app.ui.status = e;
                        app.ui.status_error = true;
                    }
                }
            });
            ui.add_space(20.0);
            // Create from a description: fills Generate's prompt and runs it.
            ui.horizontal(|ui| {
                let w = 3.0 * bw + 2.0 * 10.0;
                ui.add_space(((card.width() - w) / 2.0).max(0.0));
                ui.spacing_mut().item_spacing.x = 8.0;
                let prompt = &mut app.ui.ai.generate.prompt;
                let resp = ui.add(
                    egui::TextEdit::singleline(prompt)
                        .hint_text(tl!("Describe an image to create…"))
                        .desired_width(w - 108.0)
                        .margin(egui::Margin::symmetric(8, 6)),
                );
                let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                let go = crate::widgets::primary_button(ui, tl!("Generate"), 100.0).clicked() || enter;
                if go && !app.ui.ai.generate.prompt.trim().is_empty() {
                    let ctx = ui.ctx().clone();
                    crate::generate_ui::generate_from_home(app, &ctx);
                }
            });
            let st = crate::ai_ui::status();
            if st.checked && !st.any_ready() {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let msg = if st.connected { tl!("No AI model is installed yet.") } else { tl!("The AI engine isn't running.") };
                    let g = ui.painter().layout_no_wrap(msg.into(), egui::FontId::proportional(12.5), t.text_faint);
                    ui.add_space(((card.width() - g.size().x - 90.0) / 2.0).max(0.0));
                    ui.label(egui::RichText::new(msg).color(t.text_faint));
                    let label = if st.connected { tl!("Get Models…") } else { tl!("Set Up AI…") };
                    if ui.link(label).clicked() {
                        if st.connected {
                            crate::model_browser::open();
                        } else {
                            crate::ai_ui::open_local_ai(app);
                        }
                    }
                });
            }
            ui.add_space(20.0);
            ui.horizontal(|ui| {
                let msg = start_screen_drop_hint(app.services.is_wayland);
                let g = ui.painter().layout_no_wrap(msg.into(), egui::FontId::proportional(12.5), t.text_faint);
                ui.add_space(((card.width() - g.size().x - 24.0) / 2.0).max(0.0));
                let (r, _) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), Sense::hover());
                crate::icons::paint(ui, r, "image", 15.0, t.text_faint);
                ui.label(egui::RichText::new(msg).color(t.text_faint));
            });
            if !recent.is_empty() {
                ui.add_space(22.0);
                home_recent(app, ui, &recent);
            }
            ui.add_space(26.0);
            crate::links::link_row(app, ui);
        });
    });
}

pub(super) fn start_screen_drop_hint(is_wayland: bool) -> &'static str {
    if is_wayland { tl!("Use File › Open to open an image.") } else { tl!("Drop an image or PSD anywhere to open it.") }
}

/// Recent files listed on the Home screen.
pub(super) const HOME_RECENT: usize = 6;
/// Height of one Home-screen recent-file row.
pub(super) const HOME_RECENT_ROW: f32 = 30.0;

/// The Home screen's "Recent" list: file name, its folder on the right, click to open.
pub(super) fn home_recent(app: &mut PhotocraftApp, ui: &mut egui::Ui, recent: &[String]) {
    let t = crate::theme::Tokens::get(ui.ctx());
    let width = 380.0;
    ui.horizontal(|ui| {
        // Line the heading up with the file icons.
        ui.add_space(((ui.available_width() - width) / 2.0).max(0.0) + 8.0);
        ui.label(egui::RichText::new(tl!("Recent")).font(crate::theme::semibold(12.5)).color(t.text_dim));
    });
    ui.add_space(4.0);
    let mut open = None;
    for path in recent {
        let name = crate::file_open::display_name(path);
        let folder = std::path::Path::new(path).parent().map(|d| d.to_string_lossy().into_owned()).unwrap_or_default();
        let (row, resp) = ui.allocate_exact_size(egui::vec2(width, HOME_RECENT_ROW), Sense::click());
        if resp.hovered() {
            ui.painter().rect_filled(row, t.radius_sm, t.hover);
        }
        let icon = Rect::from_center_size(egui::pos2(row.left() + 16.0, row.center().y), egui::vec2(16.0, 16.0));
        crate::icons::paint(ui, icon, "file", 14.0, t.text_dim);
        let x = row.left() + 32.0;
        let name_g = crate::tab_strip::elided(ui, &name, egui::FontId::proportional(12.5), t.text, 170.0);
        let name_w = name_g.size().x;
        ui.painter().galley(egui::pos2(x, row.center().y - name_g.size().y / 2.0), name_g, t.text);
        let folder_max = (row.right() - 10.0 - (x + name_w + 14.0)).max(0.0);
        if folder_max > 20.0 && !folder.is_empty() {
            let fg = crate::tab_strip::elided(ui, &folder, egui::FontId::proportional(11.5), t.text_faint, folder_max);
            ui.painter().galley(egui::pos2(row.right() - 10.0 - fg.size().x, row.center().y - fg.size().y / 2.0), fg, t.text_faint);
        }
        if resp.on_hover_text(path).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            open = Some(path.clone());
        }
    }
    if let Some(path) = open
        && let Err(e) = app.open_path(&path)
    {
        app.open_failed(&crate::file_open::display_name(&path), &e);
    }
}
