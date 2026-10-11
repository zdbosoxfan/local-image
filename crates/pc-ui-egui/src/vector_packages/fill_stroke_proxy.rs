//! Vector paint proxy and compact flyout host. Original Local Image, GPL-3.0-or-later.
//! Uses 08's detached appearance transaction and 35's draw-mode widget. Pixel never calls it.
use egui::{Color32, Key, KeyboardShortcut, Modifiers, Sense, Ui};
use photocraft_doc::vector_design::appearance::{Appearance, AppearanceItem, FillItem, StrokeItem};
use photocraft_doc::{Color, Fill, LayerId, ShapeStroke};
use photocraft_engine::vector_toolbar_layout::{self as layout, Preset};
use photocraft_engine::{EngineError, Result, Session, vector_packages::appearance::Gesture};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActivePaint {
    #[default]
    Fill,
    Stroke,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Swap,
    Default,
    None,
    Colour(Color),
    Gradient,
}
#[derive(Default)]
pub struct State {
    pub generation: u64,
    pub active: ActivePaint,
    pub selected_stop: Option<(usize, usize)>,
    pub preview: Option<photocraft_doc::Document>,
    gesture: Option<Gesture>,
    held_button: Option<egui::Id>,
    targets: Vec<LayerId>,
    colour_open: bool,
    hex: String,
}
fn error(message: &str) -> EngineError {
    EngineError::Other(message.into())
}
impl State {
    pub fn cancel(&mut self) {
        self.gesture = None;
        self.held_button = None;
        self.generation = self.generation.wrapping_add(1);
        self.preview = None;
        self.targets.clear();
        self.colour_open = false;
    }
    pub fn synchronize(&mut self, s: &Session, ids: &[LayerId], enabled: bool) {
        if !enabled || self.gesture.as_ref().is_some_and(|g| !g.is_fresh(s) || ids != self.targets) {
            self.cancel();
        }
    }
    pub fn begin(&mut self, s: &Session, ids: &[LayerId]) -> Result<()> {
        self.cancel();
        if ids.is_empty() {
            return Err(error("Select shape layers"));
        }
        self.gesture = Some(Gesture::begin(s, ids)?);
        self.targets = ids.to_vec();
        Ok(())
    }
    /// Detached and cancellable. Render validation/budgets are delegated to package 08.
    pub fn update(&mut self, s: &Session, action: &Action) -> Result<()> {
        let mut gesture = self.gesture.clone().ok_or_else(|| error("Paint gesture has not begun"))?;
        if !gesture.is_fresh(s) {
            self.cancel();
            return Err(error("The document changed; try again"));
        }
        gesture.update(|a| {
            if let (Some((item, stop)), Action::Colour(color)) = (self.selected_stop, action) {
                let target = a.items.get_mut(item).ok_or_else(|| error("Paint item is absent"))?;
                let paint = match target {
                    AppearanceItem::Fill(fill) => &mut fill.paint,
                    AppearanceItem::Stroke(stroke) => &mut stroke.base.paint,
                    AppearanceItem::Mesh(_) => return Err(error("Select a gradient color stop")),
                };
                if let Fill::Gradient { stops, .. } = paint {
                    stops.get_mut(stop).ok_or_else(|| error("Gradient stop is absent"))?.1 = *color;
                } else {
                    return Err(error("Select a gradient color stop"));
                }
            } else {
                edit(a, self.active, action);
            }
            Ok(())
        })?;
        let preview = photocraft_engine::vector_packages::appearance::preview_document(s, gesture.request())?;
        self.gesture = Some(gesture);
        self.generation = self.generation.wrapping_add(1);
        self.preview = Some(preview);
        Ok(())
    }
    pub fn release(&mut self, s: &mut Session, ids: &[LayerId]) -> Result<()> {
        if ids != self.targets {
            self.cancel();
            return Err(error("The selection changed; try again"));
        }
        let gesture = self.gesture.take().ok_or_else(|| error("Paint gesture has not begun"))?;
        self.generation = self.generation.wrapping_add(1);
        self.preview = None;
        self.targets.clear();
        gesture.commit(s)
    }
    pub fn apply(&mut self, s: &mut Session, ids: &[LayerId], action: &Action) -> Result<()> {
        // A button captures on press. Keyboard commands capture at the key event instead.
        if self.gesture.is_none() {
            self.begin(s, ids)?;
        }
        let result = self.update(s, action).and_then(|()| self.release(s, ids));
        if result.is_err() {
            self.cancel();
        }
        result
    }
}
fn matches_kind(item: &AppearanceItem, active: ActivePaint) -> bool {
    matches!((item, active), (AppearanceItem::Fill(_) | AppearanceItem::Mesh(_), ActivePaint::Fill) | (AppearanceItem::Stroke(_), ActivePaint::Stroke))
}
fn visible(item: &AppearanceItem) -> bool {
    match item {
        AppearanceItem::Fill(f) => f.visible,
        AppearanceItem::Mesh(m) => m.visible,
        AppearanceItem::Stroke(s) => s.visible,
    }
}
fn paint(item: &AppearanceItem) -> Option<&Fill> {
    match item {
        AppearanceItem::Fill(f) => Some(&f.paint),
        AppearanceItem::Mesh(_) => None,
        AppearanceItem::Stroke(s) => Some(&s.base.paint),
    }
}
fn set_visible(item: &mut AppearanceItem, v: bool) {
    match item {
        AppearanceItem::Fill(f) => f.visible = v,
        AppearanceItem::Mesh(m) => m.visible = v,
        AppearanceItem::Stroke(s) => s.visible = v,
    }
}
fn put(a: &mut Appearance, active: ActivePaint, value: Option<Fill>) {
    // Hide rather than destroy stack items: None preserves brushes, widths and arrow metadata.
    if value.is_none() {
        for item in &mut a.items {
            if matches_kind(item, active) {
                set_visible(item, false);
            }
        }
        return;
    }
    let Some(value) = value else { return };
    let target = a.items.iter().rposition(|i| matches_kind(i, active) && visible(i)).or_else(|| a.items.iter().rposition(|i| matches_kind(i, active)));
    if let Some(item) = target.and_then(|index| a.items.get_mut(index)) {
        set_visible(item, true);
        match item {
            AppearanceItem::Fill(f) => f.paint = value,
            AppearanceItem::Mesh(m) => *item = AppearanceItem::Fill(FillItem { paint: value, opacity: m.opacity, blend: m.blend, visible: true }),
            AppearanceItem::Stroke(s) => {
                s.base.paint = value;
                if s.base.width == 0. {
                    s.base.width = 1.;
                }
            }
        }
    } else {
        a.items.push(match active {
            ActivePaint::Fill => AppearanceItem::Fill(FillItem::new(value)),
            ActivePaint::Stroke => {
                AppearanceItem::Stroke(StrokeItem { base: ShapeStroke { paint: value, width: 1., ..Default::default() }, ..Default::default() })
            }
        });
    }
}
fn current(a: &Appearance, active: ActivePaint) -> Option<Fill> {
    a.items.iter().rev().find(|i| matches_kind(i, active) && visible(i)).and_then(paint).cloned()
}
fn edit(a: &mut Appearance, active: ActivePaint, action: &Action) {
    match action {
        Action::Swap => {
            let fill = current(a, ActivePaint::Fill);
            let stroke = current(a, ActivePaint::Stroke);
            put(a, ActivePaint::Fill, stroke);
            put(a, ActivePaint::Stroke, fill);
        }
        Action::Default => {
            put(a, ActivePaint::Fill, None);
            put(a, ActivePaint::Stroke, None);
            put(a, ActivePaint::Fill, Some(Fill::Solid(Color::WHITE)));
            put(a, ActivePaint::Stroke, Some(Fill::Solid(Color::BLACK)));
        }
        Action::None => put(a, active, None),
        Action::Colour(c) => put(a, active, Some(Fill::Solid(*c))),
        Action::Gradient => {
            put(a, active, Some(Fill::gradient(vec![(0., Color::BLACK), (1., Color::WHITE)], 0., 1., photocraft_doc::GradientStyle::Linear, false)))
        }
    }
}
/// Parser extension is local: egui calls the US tilde key Backtick, with Shift held.
pub fn shortcut(s: &str) -> Option<KeyboardShortcut> {
    match s {
        "Shift+~" => Some(KeyboardShortcut::new(Modifiers::SHIFT, Key::Backtick)),
        "\\" => Some(KeyboardShortcut::new(Modifiers::NONE, Key::Backslash)),
        "/" => Some(KeyboardShortcut::new(Modifiers::NONE, Key::Slash)),
        _ => crate::shortcuts::parse(s),
    }
}
pub fn tool_key(ctx: &egui::Context, preset: Preset, registered: &[&str], current: &str) -> Option<String> {
    if crate::shortcut_dispatch::Focus::of(ctx) == crate::shortcut_dispatch::Focus::Text {
        return None;
    }
    for b in layout::tool_bindings(preset) {
        if let Some(target) = layout::tool_for_key_after(photocraft_engine::toolsets::Persona::Vector, preset, b.key, registered, current)
            && shortcut(b.key).is_some_and(|sc| crate::shortcuts::consume(ctx, &sc))
        {
            return Some(target.into());
        }
    }
    None
}
/// Call before shared D/X shortcuts. Guarded by persona and focus; never consumes text input.
pub fn keys(ctx: &egui::Context, state: &mut State, s: &mut Session, ids: &[LayerId], vector: bool) -> Result<bool> {
    state.synchronize(s, ids, vector);
    if !vector || crate::shortcut_dispatch::Focus::of(ctx) == crate::shortcut_dispatch::Focus::Text {
        return Ok(false);
    }
    for (key, action) in [("Shift+X", Some(Action::Swap)), ("X", None), ("D", Some(Action::Default)), ("/", Some(Action::None))] {
        if shortcut(key).is_some_and(|sc| crate::shortcuts::consume(ctx, &sc)) {
            if let Some(action) = action {
                state.apply(s, ids, &action)?;
            } else {
                state.cancel();
                state.active = if state.active == ActivePaint::Fill { ActivePaint::Stroke } else { ActivePaint::Fill };
            }
            return Ok(true);
        }
    }
    Ok(false)
}
fn compact_button(ui: &mut Ui, label: &str, glyph: &str, width: f32, enabled: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 20.), if enabled { Sense::click() } else { Sense::hover() });
    let visuals = ui.style().interact(&response);
    ui.painter().rect_filled(rect, 2., visuals.bg_fill);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        glyph,
        egui::FontId::proportional(14.),
        if enabled { visuals.text_color() } else { ui.visuals().weak_text_color() },
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, tl!(label)));
    response.on_hover_text(tl!(label))
}
fn button(ui: &mut Ui, label: &str, glyph: &str, width: f32, state: &mut State, s: &mut Session, ids: &[LayerId], action: Action) -> Result<()> {
    let response = compact_button(ui, label, glyph, width, !ids.is_empty());
    if (response.is_pointer_button_down_on() || response.clicked()) && ui.input(|i| i.pointer.any_pressed()) {
        state.begin(s, ids)?;
        state.held_button = Some(response.id);
        state.update(s, &action)?;
    }
    let released = state.held_button == Some(response.id) && response.contains_pointer() && ui.input(|i| i.pointer.primary_released());
    if response.clicked() || released {
        // Keyboard/accessibility activation is a fresh discrete gesture. A pointer release
        // with a cancelled capture must never silently retarget the new selection/document.
        if state.gesture.is_none() && !ui.input(|i| i.pointer.any_released()) {
            state.begin(s, ids)?;
        }
        if state.gesture.is_none() {
            return Err(error("The gesture was cancelled; try again"));
        }
        if state.preview.is_some() {
            state.release(s, ids)?;
        } else {
            state.apply(s, ids, &action)?;
        }
    }
    Ok(())
}
/// Place immediately below tools. Host owns the 35 State so drawing gestures share its mode.
pub fn widget(ui: &mut Ui, state: &mut State, s: &mut Session, ids: &[LayerId], draw: &mut crate::clip_compound::State) -> Result<()> {
    state.synchronize(s, ids, true);
    if ui.input(|i| !i.focused || i.key_pressed(Key::Escape)) {
        state.cancel();
    }
    let mut result = Ok(());
    let appearance = ids.first().and_then(|id| s.active()?.doc.layer(*id)).and_then(|layer| match &layer.content {
        photocraft_doc::LayerContent::Shape(shape) => {
            Some(layer.vector.appearance.clone().unwrap_or_else(|| photocraft_vector::appearance::legacy_appearance(shape)))
        }
        _ => None,
    });
    let (_, area) = ui.allocate_space(egui::vec2(46., 46.));
    let order = if state.active == ActivePaint::Fill { [ActivePaint::Stroke, ActivePaint::Fill] } else { [ActivePaint::Fill, ActivePaint::Stroke] };
    for active in order {
        let rect =
            egui::Rect::from_min_size(area.min + if active == ActivePaint::Fill { egui::vec2(0., 0.) } else { egui::vec2(14., 14.) }, egui::vec2(30., 30.));
        let label = if active == ActivePaint::Fill { "Active fill" } else { "Active stroke" };
        let hit = if state.active == active {
            rect
        } else if active == ActivePaint::Fill {
            egui::Rect::from_min_max(rect.min, egui::pos2(area.min.x + 14., rect.max.y))
        } else {
            egui::Rect::from_min_max(egui::pos2(area.min.x + 30., rect.min.y), rect.max)
        };
        let r = ui.interact(hit, ui.id().with(label), Sense::click());
        let mut clicked = r.clicked();
        if state.active != active {
            let strip = if active == ActivePaint::Fill {
                egui::Rect::from_min_max(egui::pos2(area.min.x + 14., rect.min.y), egui::pos2(rect.max.x, area.min.y + 14.))
            } else {
                egui::Rect::from_min_max(egui::pos2(rect.min.x, area.min.y + 30.), egui::pos2(area.min.x + 30., rect.max.y))
            };
            clicked |= ui.interact(strip, ui.id().with((label, "strip")), Sense::click()).clicked();
        }

        r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tl!(label)));
        let value = appearance.as_ref().and_then(|a| current(a, active));
        let colour = match &value {
            Some(Fill::Solid(c)) => Some(*c),
            Some(Fill::Gradient { stops, .. }) => stops.first().map(|(_, c)| *c),
            _ => None,
        };
        let colour = colour.map_or(Color32::TRANSPARENT, |c| {
            let rgb = c.to_rgb();
            Color32::from_rgba_unmultiplied((rgb[0] * 255.) as u8, (rgb[1] * 255.) as u8, (rgb[2] * 255.) as u8, (c.alpha * 255.) as u8)
        });
        ui.painter().rect_filled(rect, 1., ui.visuals().extreme_bg_color);
        ui.painter().rect(
            rect.shrink(2.),
            0.,
            if active == ActivePaint::Fill { colour } else { Color32::TRANSPARENT },
            egui::Stroke::new(if active == ActivePaint::Stroke { 5. } else { 1. }, colour),
            egui::StrokeKind::Inside,
        );
        ui.painter().rect_stroke(
            rect,
            1.,
            egui::Stroke::new(if state.active == active { 2. } else { 1. }, ui.visuals().text_color()),
            egui::StrokeKind::Inside,
        );
        if value.is_none() {
            ui.painter().line_segment([rect.left_bottom(), rect.right_top()], egui::Stroke::new(2., Color32::RED));
        }
        if clicked {
            state.cancel();
            state.active = active;
        }
        r.on_hover_text(tl!(label));
    }
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.;
        for (label, glyph, action) in [("Swap fill and stroke", "↔", Action::Swap), ("Default fill and stroke", "◩", Action::Default)] {
            if result.is_ok() {
                result = button(ui, label, glyph, 22., state, s, ids, action);
            }
        }
    });
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.;
        if compact_button(ui, "Colour", "■", 14., !ids.is_empty()).clicked() {
            result = state.begin(s, ids);
            if result.is_ok() {
                state.colour_open = true;
                state.hex = "000000".into();
            }
        }
        if result.is_ok() {
            result = button(ui, "Gradient", "▨", 14., state, s, ids, Action::Gradient);
        }
        if result.is_ok() {
            result = button(ui, "None", "/", 14., state, s, ids, Action::None);
        }
    });
    if state.colour_open {
        let mut open = true;
        egui::Window::new(tl!("Colour")).id(ui.id().with("paint-colour-editor")).open(&mut open).resizable(false).collapsible(false).show(ui.ctx(), |ui| {
            ui.label(tl!("Paint colour hex"));
            let response = ui.text_edit_singleline(&mut state.hex);
            response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, tl!("Paint colour hex")));
            if ui.button(tl!("Apply paint colour")).clicked() {
                result = (|| {
                    let text = state.hex.trim().trim_start_matches('#');
                    if text.len() != 6 {
                        return Err(error("Enter six hexadecimal digits"));
                    }
                    let value = u32::from_str_radix(text, 16).map_err(|_| error("Enter six hexadecimal digits"))?;
                    let colour = Color::rgb(((value >> 16) & 255) as f32 / 255., ((value >> 8) & 255) as f32 / 255., (value & 255) as f32 / 255.);
                    if state.gesture.is_none() {
                        return Err(error("The gesture was cancelled; try again"));
                    }
                    state.apply(s, ids, &Action::Colour(colour))?;
                    state.colour_open = false;
                    Ok(())
                })();
            }
            if ui.button(tl!("Cancel paint colour")).clicked() {
                state.cancel();
            }
        });
        if !open {
            state.cancel();
        }
    }
    let mode = match draw.settings.mode {
        photocraft_engine::vector_packages::clip_compound::DrawMode::Normal => "DrawNormal",
        photocraft_engine::vector_packages::clip_compound::DrawMode::Behind => "DrawBehind",
        photocraft_engine::vector_packages::clip_compound::DrawMode::Inside => "DrawInside",
    };
    let response = tool_button(ui, mode, 20.);
    egui::Popup::menu(&response).show(|ui| {
        crate::clip_compound::draw_mode_widget(ui, s, draw, |label| tl!(label).into());
    });
    if !state.colour_open && state.gesture.is_some() && ui.input(|i| !i.pointer.any_down()) {
        state.cancel();
    }
    result
}

#[derive(Default)]
pub struct FlyoutState {
    pub active: String,
    held: Option<(usize, f64)>,
    open: Option<usize>,
}
/// Small reusable host for 01's toolbar. Package registration is evaluated each frame.
pub fn flyouts(ui: &mut Ui, state: &mut FlyoutState, registered: &[&str], set: Option<&photocraft_engine::toolsets::ToolSet>) -> Option<String> {
    if ui.input(|i| !i.focused || i.key_pressed(Key::Escape)) {
        state.held = None;
        state.open = None;
    }
    let mut chosen = None;
    for group in layout::groups(registered, set) {
        let shown = group.tools.iter().find(|id| **id == state.active).unwrap_or(&group.tools[0]);
        let r = tool_button(ui, shown, 24.);
        if r.is_pointer_button_down_on() {
            let now = ui.input(|i| i.time);
            let (slot, start) = *state.held.get_or_insert((group.slot, now));
            if slot == group.slot && now - start >= 0.35 {
                state.open = Some(slot);
            }
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(40));
        }
        if r.clicked() && state.open != Some(group.slot) {
            chosen = Some((*shown).to_string());
        }
        let mut open = state.open == Some(group.slot);
        // Releasing the long-held anchor is part of opening the flyout, not an outside click.
        let closing = if state.held.is_some_and(|(slot, _)| slot == group.slot) && ui.input(|i| i.pointer.any_released()) {
            egui::PopupCloseBehavior::IgnoreClicks
        } else {
            egui::PopupCloseBehavior::CloseOnClickOutside
        };
        egui::Popup::from_response(&r).open_bool(&mut open).close_behavior(closing).show(|ui| {
            ui.horizontal(|ui| {
                for tool in group.tools {
                    if tool_button(ui, tool, 24.).clicked() {
                        chosen = Some(tool.to_string());
                    }
                }
            });
        });
        if !open && state.open == Some(group.slot) {
            state.open = None;
        }
    }
    if !ui.input(|i| i.pointer.any_down()) {
        state.held = None;
    }
    if let Some(tool) = &chosen {
        state.active = tool.clone();
        state.open = None;
        state.held = None;
    }
    chosen
}
#[cfg(test)]
#[path = "fill_stroke_proxy/tests.rs"]
mod tests;

pub use crate::vector_tool_icon_data as icons;

/// Shares egui_extras' existing SVG loader and egui's texture upload path. 00 appends the
/// ICONS bytes to colour_icons/mono_tool_icons as well, so menus and toolbar use one artwork.
pub fn tool_button(ui: &mut Ui, tool: &str, size: f32) -> egui::Response {
    egui_extras::install_image_loaders(ui.ctx());
    let Some(index) = icons::ICONS.iter().position(|(id, _, _, _)| *id == tool) else { return ui.button(tool) };
    let (_, name, _, _) = icons::ICONS[index];
    let coloured = crate::color_icons::enabled(ui.ctx());
    type IconBytes = (std::sync::Arc<[u8]>, std::sync::Arc<[u8]>);
    static BYTES: std::sync::OnceLock<Vec<IconBytes>> = std::sync::OnceLock::new();
    let bytes = BYTES.get_or_init(|| {
        icons::ICONS
            .iter()
            .map(|(_, _, colour, mono)| {
                (std::sync::Arc::from(*colour), std::sync::Arc::from(String::from_utf8_lossy(mono).replace("currentColor", "#ffffff").into_bytes()))
            })
            .collect()
    });
    let data = if coloured { bytes[index].0.clone() } else { bytes[index].1.clone() };
    let image = egui::Image::from_bytes(format!("bytes://vector46/{name}/{coloured}.svg"), data).fit_to_exact_size(egui::vec2(size, size)).tint(if coloured {
        Color32::WHITE
    } else {
        ui.visuals().text_color()
    });
    let r = ui.add(egui::Button::image(image));
    let label = icons::LABELS.iter().find(|(id, _)| *id == tool).map_or(tool, |(_, label)| *label);
    r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tl!(label)));
    r.on_hover_text(tl!(label))
}
