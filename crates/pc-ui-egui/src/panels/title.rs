//! The title bar: brand mark, menus, document title, module switch and workspace controls.
use super::*;

// Title bar

/// The app's top bar: brand mark, menus, the document title and the workspace controls. With
/// [`PhotocraftApp::custom_titlebar`] (Windows and Linux) it is also the window's title bar, as in
/// Photoshop on Windows: the caption buttons take its right end (`titlebar`).
pub fn title_bar(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    let custom = app.custom_titlebar;
    let left = if cfg!(target_os = "macos") && app.integrated_titlebar { 78 } else { 10 };
    let right = if custom { 0 } else { 10 };
    let bar = egui::Panel::top("title_bar")
        .exact_size(if t.pro { 32.0 } else { 38.0 })
        .frame(egui::Frame::NONE.fill(t.chrome).inner_margin(egui::Margin { left, right, top: 0, bottom: 0 }))
        .show(ui, |ui| {
            let full = ui.max_rect();
            // Only the free gap between the menus and the right-hand controls drags the window: a
            // press on a menu title must open the menu, never move the window. The gap is last
            // frame's, as the menus are laid out after this.
            let span_id = ui.id().with("titledrag-span");
            let (gap_l, gap_r) = ui.ctx().data(|d| d.get_temp::<(f32, f32)>(span_id)).unwrap_or((full.right(), full.right()));
            let gap = egui::Rect::from_x_y_ranges(gap_l.max(full.left())..=gap_r.min(full.right()).max(gap_l), full.y_range());
            let drag = ui.interact(gap, ui.id().with("titledrag"), Sense::click_and_drag());
            if drag.drag_started() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
            if drag.double_clicked() {
                let max = ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false));
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Maximized(!max));
            }
            let title =
                app.session.active().map(|d| format!("{}{}", d.doc.name, if d.is_dirty() { "  •" } else { "" })).unwrap_or_else(|| "Local Image".into());
            // The menus and the right-hand controls are laid out first; the title is centred in
            // whatever room is left between them, shortened or dropped rather than drawn over them.
            let (mut menus_right, mut controls_left) = (full.left(), full.right());
            ui.horizontal_centered(|ui| {
                let side = if t.pro { 18.0 } else { 20.0 };
                let (mark, _) = ui.allocate_exact_size(vec2(side, side), Sense::hover());
                crate::brand::paint_mark(ui, mark);
                ui.add_space(6.0);
                // With the macOS menu bar the menus are at the top of the screen instead.
                let modules = if app.host_modes {
                    crate::Module::ALL
                        .iter()
                        .map(|module| {
                            let text = RichText::new(tl!(module.label())).font(egui::TextStyle::Button.resolve(ui.style()));
                            let galley = egui::WidgetText::from(text).into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, egui::TextStyle::Button);
                            (galley.size().x + 2.0 * ui.spacing().button_padding.x).max(2.0 * ui.spacing().interact_size.x + ui.spacing().button_padding.x)
                        })
                        .sum::<f32>()
                        + ui.spacing().item_spacing.x
                } else {
                    0.0
                };
                let caption = if custom { crate::titlebar::WIDTH + TITLE_GAP } else { 0.0 };
                let room = (ui.available_width() - modules - caption - WORKSPACE_MIN - TITLE_GAP).max(0.0);
                menus_right = if app.services.native_menu.is_some() { ui.cursor().left() } else { crate::menus::menu_bar_with_width(app, ui, room) };
                // The menu bar takes the whole row, so the right-hand group gets its own rect:
                // from the menus to the bar's end, or to the caption buttons.
                let right_edge = if custom { full.right() - crate::titlebar::WIDTH - 4.0 } else { full.right() };
                let group = egui::Rect::from_min_max(egui::pos2((menus_right + TITLE_GAP).min(right_edge), full.top()), egui::pos2(right_edge, full.bottom()));
                let mut group_ui = ui.new_child(egui::UiBuilder::new().max_rect(group).layout(egui::Layout::right_to_left(egui::Align::Center)));
                controls_left = {
                    let ui = &mut group_ui;
                    ui.spacing_mut().item_spacing.x = 6.0;
                    if app.host_modes
                        && let Some(m) = widgets::module_switcher(ui, app.current_module)
                    {
                        app.switch_module = Some(m);
                    }
                    let mut ws = app.ui.workspace.clone();
                    let opts = [
                        // Keys stay English (`apply_workspace` matches them); the dropdown
                        // shows the labels in the UI language.
                        ("Essentials".to_string(), tl!("Essentials")),
                        ("Photography".to_string(), tl!("Photography")),
                        ("Painting".to_string(), tl!("Painting")),
                        ("Graphic and Web".to_string(), tl!("Graphic and Web")),
                    ];
                    // A narrow bar drops what is also in a menu, Discord first (below), then
                    // the theme toggle (Preferences), then search (Edit › Search), and narrows
                    // the switcher (Window › Workspace) before anything runs over.
                    let room = ui.available_width();
                    let icons_shown: u8 = if room >= WORKSPACE_MIN + 2.0 * ICON_SLOT {
                        2
                    } else if room >= WORKSPACE_MIN + ICON_SLOT {
                        1
                    } else {
                        0
                    };
                    let ws_width = (room - f32::from(icons_shown) * ICON_SLOT - 6.0).clamp(WORKSPACE_MIN, 130.0);
                    if room >= WORKSPACE_MIN && widgets::dropdown(ui, "workspace", &mut ws, &opts, ws_width) {
                        app.ui.workspace = ws;
                        crate::menus::apply_workspace(app);
                    }
                    if icons_shown >= 1
                        && icons::button(ui, "search", 28.0, app.ui.palette_open, &crate::shortcuts::tip_label(app, "Search commands", "edit.search")).clicked()
                    {
                        app.ui.palette_open = !app.ui.palette_open;
                    }
                    let theme_icon = if t.dark() { "sun" } else { "moon" };
                    if icons_shown >= 2 && icons::button(ui, theme_icon, 28.0, false, tl!("Switch theme")).clicked() {
                        let next = app.ui.theme.next();
                        app.set_theme(ui.ctx(), next);
                    }
                    ui.min_rect().left()
                };
            });
            ui.ctx().data_mut(|d| d.insert_temp(span_id, (menus_right, controls_left)));
            let font = theme::medium(13.0);
            let galley = ui.painter().layout_no_wrap(title.clone(), font.clone(), t.text_dim);
            if let Some(x) = title_x(full.center().x, menus_right, controls_left, galley.size().x) {
                ui.painter().galley(egui::pos2(x, full.center().y - galley.size().y / 2.0), galley, t.text_dim);
            } else if controls_left - menus_right > 80.0 {
                // Too narrow for the whole name: show the start of it, elided, in the free gap.
                let avail = controls_left - menus_right - 2.0 * TITLE_GAP;
                let mut job = egui::text::LayoutJob::simple_singleline(title, font, t.text_dim);
                job.wrap = egui::text::TextWrapping::truncate_at_width(avail);
                let g = ui.painter().layout_job(job);
                ui.painter().galley(egui::pos2(menus_right + TITLE_GAP, full.center().y - g.size().y / 2.0), g, t.text_dim);
            }
        });
    if custom {
        crate::titlebar::caption_buttons(app, ui, bar.response.rect);
    }
}

/// local-image: the title bar over the hosted Library and Develop modules: the brand mark, `title`,
/// the module switch (`current` selected) and, with a custom title bar, the caption buttons. Close
/// asks the window to close (the host then checks every module for unsaved work). Returns the
/// module the user picked.
pub fn library_title_bar(app: &mut PhotocraftApp, ui: &mut egui::Ui, title: &str, current: crate::Module) -> Option<crate::Module> {
    library_title_bar_with_menus(app, ui, title, current, |ui, _| ui.cursor().left())
}

/// Hosted module menus share the title/caption/drag geometry; callback returns its right edge.
pub fn library_title_bar_with_menus(
    app: &mut PhotocraftApp,
    ui: &mut egui::Ui,
    title: &str,
    current: crate::Module,
    mut menus: impl FnMut(&mut egui::Ui, f32) -> f32,
) -> Option<crate::Module> {
    let t = Tokens::get(ui.ctx());
    let custom = app.custom_titlebar;
    let left = if cfg!(target_os = "macos") && app.integrated_titlebar { 78 } else { 10 };
    let mut go = None;
    let bar = egui::Panel::top("library_title_bar")
        .exact_size(if t.pro { 32.0 } else { 38.0 })
        .frame(egui::Frame::NONE.fill(t.chrome).inner_margin(egui::Margin { left, right: if custom { 0 } else { 10 }, top: 0, bottom: 0 }))
        .show(ui, |ui| {
            let full = ui.max_rect();
            let right_edge = if custom { full.right() - crate::titlebar::WIDTH - 4.0 } else { full.right() };
            let mut controls_left = right_edge;
            let mut menus_right = full.left() + 40.;
            ui.horizontal_centered(|ui| {
                let side = if t.pro { 18.0 } else { 20.0 };
                let (mark, _) = ui.allocate_exact_size(vec2(side, side), Sense::hover());
                crate::brand::paint_mark(ui, mark);
                let group = egui::Rect::from_min_max(egui::pos2(ui.cursor().left() + TITLE_GAP, full.top()), egui::pos2(right_edge, full.bottom()));
                let mut g = ui.new_child(egui::UiBuilder::new().max_rect(group).layout(egui::Layout::right_to_left(egui::Align::Center)));
                go = widgets::module_switcher(&mut g, current);
                // CPU mode after a GPU failure, visible in the Library and Develop too.
                crate::gpu_status::status_pill(app, &mut g);
                controls_left = g.min_rect().left();
                let room = (controls_left - ui.cursor().left() - TITLE_GAP).max(0.);
                menus_right = menus(ui, room);
            });
            // The free space drags the window; a double click maximizes.
            let gap = egui::Rect::from_x_y_ranges((menus_right + 4.0)..=(controls_left - 4.0).max(menus_right + 4.0), full.y_range());
            let drag = ui.interact(gap, ui.id().with("library-titledrag"), Sense::click_and_drag());
            if drag.drag_started() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
            if drag.double_clicked() {
                let max = ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false));
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Maximized(!max));
            }
            let font = theme::medium(13.0);
            let galley = ui.painter().layout_no_wrap(title.to_owned(), font, t.text_dim);
            if let Some(x) = title_x(full.center().x, menus_right, controls_left, galley.size().x) {
                ui.painter().galley(egui::pos2(x, full.center().y - galley.size().y / 2.0), galley, t.text_dim);
            }
        });
    if custom {
        crate::titlebar::library_caption_buttons(ui, bar.response.rect);
    }
    go
}

/// local-image: the window's edge resize zones (custom title bar) for a hosted mode.
pub fn host_resize_zones(app: &PhotocraftApp, ui: &mut egui::Ui) {
    if app.custom_titlebar {
        crate::titlebar::resize_zones(ui);
    }
}

/// Minimum space kept between the window title and the menus or controls beside it.
pub(super) const TITLE_GAP: f32 = 16.0;
/// The workspace switcher's narrowest width, and one 28 pt title-bar icon with its spacing.
pub(super) const WORKSPACE_MIN: f32 = 90.0;
pub(super) const ICON_SLOT: f32 = 34.0;

/// Left edge for a title `width` wide: centred on `center` when it fits between the menus
/// (ending at `menus_right`) and the right-hand controls (starting at `controls_left`), else
/// slid into that gap; None when the gap is too small for the whole title.
pub(super) fn title_x(center: f32, menus_right: f32, controls_left: f32, width: f32) -> Option<f32> {
    let (lo, hi) = (menus_right + TITLE_GAP, controls_left - TITLE_GAP - width);
    if !(lo.is_finite() && hi.is_finite() && width.is_finite()) || hi < lo {
        return None;
    }
    if !center.is_finite() {
        return Some(lo);
    }
    Some((center - width / 2.0).clamp(lo, hi))
}
