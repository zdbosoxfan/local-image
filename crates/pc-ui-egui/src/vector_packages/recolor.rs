//! Recolor Artwork dialog hook. Original GPL-3.0-or-later egui UI; the host supplies the menu
//! and canvas preview router. Rows use egui typed drag/drop, all commits return a captured
//! Plan for one journaled command. Closing, Escape or stale captures discard detached previews.
use photocraft_doc::{Color, LayerId};
use photocraft_engine::vector_packages::{
    colours::{Access, Preview},
    recolor::{self, Method, Plan},
};
use photocraft_engine::{EngineError, Result, Session};

#[derive(Default)]
pub struct State {
    pub generation: u64,
    pub plan: Option<Plan>,
    pub preview: Option<Preview>,
    pub error: Option<String>,
    count: usize,
    preview_enabled: bool,
    save_group: bool,
    group_name: String,
    dirty: bool,
    hex: Vec<String>,
}
impl State {
    pub fn open(&mut self, s: &Session, access: Access, ids: &[LayerId]) -> Result<()> {
        let plan = recolor::begin(s, access, ids)?;
        self.count = plan.colors.len();
        self.hex = plan.rows.iter().map(|r| hex(r.to)).collect();
        self.plan = Some(plan);
        self.generation = self.generation.wrapping_add(1);
        self.preview = None;
        self.preview_enabled = true;
        self.save_group = false;
        self.group_name = tl!("Recolored Colours").into();
        self.error = None;
        self.dirty = true;
        Ok(())
    }
    pub fn cancel(&mut self) {
        self.plan = None;
        self.generation = self.generation.wrapping_add(1);
        self.preview = None;
        self.dirty = false;
    }
}
fn hex(c: Color) -> String {
    c.to_rgba8()[..3].iter().map(|v| format!("{v:02X}")).collect()
}
fn color_button(ui: &mut egui::Ui, color: &mut Color) -> bool {
    let mut bytes = color.to_rgba8();
    if ui.color_edit_button_srgba_unmultiplied(&mut bytes).changed() {
        *color = Color::rgba(bytes[0] as f32 / 255., bytes[1] as f32 / 255., bytes[2] as f32 / 255., bytes[3] as f32 / 255.);
        true
    } else {
        false
    }
}
/// Returns Some(plan) only on Apply. The host executes recolor.apply and clears preview.
pub fn dialog(ctx: &egui::Context, s: &Session, access: Access, state: &mut State) -> Result<Option<Plan>> {
    if state.plan.is_none() {
        return Ok(None);
    }
    if state.plan.as_ref().is_some_and(|p| p.capture.check(s).is_err()) {
        state.cancel();
        state.error = Some(tl!("Document changed; start again").into());
        return Err(EngineError::Other(tl!("Document changed; start again").into()));
    }
    let escape = crate::shortcut_dispatch::Focus::of(ctx) == crate::shortcut_dispatch::Focus::None && ctx.input(|i| i.key_pressed(egui::Key::Escape));
    let mut result = None;
    let mut cancel = false;
    let mut move_color = None;
    let mut error = None;
    if let Some(plan) = &mut state.plan {
        egui::Window::new(tl!("Recolor Artwork")).id(egui::Id::new("recolor_artwork")).collapsible(false).default_width(540.).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(tl!("Colour Groups"));
                ui.add(egui::DragValue::new(&mut state.count).range(1..=plan.colors.len()));
                if ui.button(tl!("Reduce Colours")).clicked() {
                    match plan.reduce(state.count, plan.seed) {
                        Ok(()) => {
                            state.dirty = true;
                            state.hex = plan.rows.iter().map(|r| hex(r.to)).collect();
                        }
                        Err(e) => error = Some(e.to_string()),
                    }
                }
            });
            ui.horizontal(|ui| {
                for (m, label) in
                    [(Method::Exact, tl!("Exact")), (Method::PreserveTints, tl!("Preserve Tints")), (Method::PreserveLightness, tl!("Preserve Lightness"))]
                {
                    state.dirty |= ui.radio_value(&mut plan.method, m, label).changed();
                }
            });
            egui::ScrollArea::vertical().max_height(300.).show(ui, |ui| {
                for row in 0..plan.rows.len() {
                    let frame = egui::Frame::group(ui.style());
                    let (inner, dropped) = ui.dnd_drop_zone::<usize, _>(frame, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(format!("{} {}", tl!("Row"), row + 1));
                            for &i in &plan.rows[row].from {
                                let [r, g, b, a] = plan.colors[i].color.to_rgba8();
                                ui.dnd_drag_source(egui::Id::new(("recolor_source", i)), i, |ui| {
                                    ui.add(egui::Button::new(format!("{} {}", tl!("Colour"), i + 1)).fill(egui::Color32::from_rgba_unmultiplied(r, g, b, a)));
                                });
                            }
                            ui.label(tl!("Assign"));
                            if color_button(ui, &mut plan.rows[row].to) {
                                state.dirty = true;
                                state.hex[row] = hex(plan.rows[row].to);
                            }
                            let value = &mut state.hex[row];
                            let response = ui.add(egui::TextEdit::singleline(value).desired_width(70.).id(egui::Id::new(("recolor_hex", row))));
                            state.dirty |= response.changed();
                            if response.changed()
                                && value.len() == 6
                                && value.bytes().all(|b| b.is_ascii_hexdigit())
                                && let Ok(value) = u32::from_str_radix(value, 16)
                            {
                                let alpha = plan.rows[row].to.alpha;
                                plan.rows[row].to =
                                    Color::rgba(((value >> 16) & 255) as f32 / 255., ((value >> 8) & 255) as f32 / 255., (value & 255) as f32 / 255., alpha);
                                state.dirty = true;
                            }
                            response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, format!("{} {}", tl!("Assign Row"), row + 1)));
                        });
                    });
                    let _ = inner;
                    if let Some(color) = dropped {
                        move_color = Some((*color, row));
                    }
                }
            });
            ui.horizontal(|ui| {
                if ui.button(tl!("Randomize Order")).clicked() {
                    plan.seed = plan.seed.wrapping_add(1);
                    plan.randomize_order(plan.seed);
                    state.hex = plan.rows.iter().map(|r| hex(r.to)).collect();
                    state.dirty = true;
                }
                if ui.button(tl!("Randomize Saturation/Brightness")).clicked() {
                    plan.seed = plan.seed.wrapping_add(1);
                    plan.randomize_saturation_brightness(plan.seed);
                    state.hex = plan.rows.iter().map(|r| hex(r.to)).collect();
                    state.dirty = true;
                }
            });
            state.dirty |= ui.checkbox(&mut state.preview_enabled, tl!("Preview")).changed();
            state.dirty |= ui.checkbox(&mut state.save_group, tl!("Save as Swatch Group")).changed();
            if state.save_group {
                state.dirty |= ui.text_edit_singleline(&mut state.group_name).changed();
            }
            for warning in &plan.warnings {
                ui.label(&warning.message);
            }
            if let Some(e) = &state.error {
                ui.colored_label(egui::Color32::RED, e);
            }
            let valid_hex = state.hex.iter().all(|v| v.len() == 6 && v.bytes().all(|b| b.is_ascii_hexdigit()));
            if !valid_hex {
                ui.colored_label(egui::Color32::RED, tl!("Enter six hexadecimal digits"));
            }
            let valid_group = !state.save_group || photocraft_doc::vector_design::swatches::validate_name(&state.group_name).is_ok();
            ui.horizontal(|ui| {
                if ui.add_enabled(valid_hex && valid_group, egui::Button::new(tl!("Apply"))).clicked() {
                    plan.save_group = state.save_group.then(|| state.group_name.clone());
                    result = Some(plan.clone());
                    cancel = true;
                }
                if ui.button(tl!("Cancel")).clicked() {
                    cancel = true;
                }
            });
        });
        if let Some((color, row)) = move_color {
            match plan.move_color(color, row) {
                Ok(()) => {
                    state.dirty = true;
                    state.hex = plan.rows.iter().map(|r| hex(r.to)).collect();
                }
                Err(e) => error = Some(e.to_string()),
            }
        }
        plan.save_group = state.save_group.then(|| state.group_name.clone());
    }
    if escape {
        cancel = true;
        result = None;
    }
    if let Some(e) = error {
        state.error = Some(e);
        state.generation = state.generation.wrapping_add(1);
        state.preview = None;
    }
    if cancel {
        state.cancel();
    } else if state.dirty {
        state.generation = state.generation.wrapping_add(1);
        state.preview = None;
        if state.preview_enabled
            && state.hex.iter().all(|v| v.len() == 6 && v.bytes().all(|b| b.is_ascii_hexdigit()))
            && (!state.save_group || photocraft_doc::vector_design::swatches::validate_name(&state.group_name).is_ok())
            && let Some(plan) = &state.plan
        {
            match recolor::prepare(s, access, plan) {
                Ok(p) => {
                    state.generation = state.generation.wrapping_add(1);
                    state.preview = Some(p);
                    state.error = None
                }
                Err(e) => state.error = Some(e.to_string()),
            }
        }
        state.dirty = false;
    }
    Ok(result)
}
#[cfg(test)]
#[path = "recolor/tests.rs"]
mod tests;
