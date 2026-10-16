//! Document-palette Swatches tab hook. Original GPL-3.0-or-later egui UI following
//! Affinity Global Colors and Illustrator Swatches. 00 routes returned events through
//! Session::execute and its existing file dialogs. Previews never mutate Session.
use egui::{Color32, Sense};
use photocraft_doc::{Color, Fill, GradientStyle};
use photocraft_engine::vector_packages::colours::{self, Access, Action, Capture, PaintTarget, Preview, Request, Swatch, SwatchBook, SwatchGroup, SwatchId};
use photocraft_engine::{EngineError, Result, Session};
use std::collections::BTreeSet;

#[derive(Clone, Debug)]
pub enum Event {
    Commit(Request),
    ImportAse(Capture),
    ExportAse(SwatchBook),
    Recolor,
}
pub struct State {
    pub generation: u64,
    pub selected: BTreeSet<SwatchId>,
    pub preview: Option<Preview>,
    editor: Option<Request>,
    preview_enabled: bool,
    group_name: String,
    pub error: Option<String>,
    pub tint_percent: f32,
}
impl Default for State {
    fn default() -> Self {
        Self {
            generation: 0,
            selected: BTreeSet::new(),
            preview: None,
            editor: None,
            preview_enabled: true,
            group_name: String::new(),
            error: None,
            tint_percent: 100.,
        }
    }
}
impl State {
    pub fn cancel(&mut self) {
        self.editor = None;
        self.generation = self.generation.wrapping_add(1);
        self.preview = None;
    }
    pub fn editing(&self) -> bool {
        self.editor.is_some()
    }
}
fn chip(ui: &mut egui::Ui, paint: &Fill) -> egui::Response {
    let (r, painter) = ui.allocate_painter(egui::vec2(28., 24.), Sense::click());
    match paint {
        Fill::Solid(c) => {
            painter.rect_filled(r.rect, 2., Color32::from_rgba_unmultiplied(c.to_rgba8()[0], c.to_rgba8()[1], c.to_rgba8()[2], c.to_rgba8()[3]));
        }
        Fill::Gradient { stops, .. } => {
            for i in 0..28 {
                let t = i as f32 / 27.;
                let c = stops.iter().min_by(|a, b| (a.0 - t).abs().total_cmp(&(b.0 - t).abs())).map_or(Color::BLACK, |s| s.1);
                let rect = egui::Rect::from_min_size(r.rect.min + egui::vec2(i as f32, 0.), egui::vec2(1., 24.));
                let [r, g, b, a] = c.to_rgba8();
                painter.rect_filled(rect, 0., Color32::from_rgba_unmultiplied(r, g, b, a));
            }
        }
        _ => {}
    }
    r
}
fn edited_color(mode: photocraft_doc::ColorMode, bytes: [u8; 4]) -> Color {
    use photocraft_color::{ColorMode, convert};
    let rgb = [bytes[0], bytes[1], bytes[2]].map(|v| f32::from(v) / 255.);
    let c = match mode {
        ColorMode::Cmyk => convert::rgb_to_cmyk(rgb),
        ColorMode::Lab => {
            let lab = li_color::convert::srgb_to_lab(rgb);
            [lab[0] / 100., (lab[1] + 128.) / 255., (lab[2] + 128.) / 255., 0.]
        }
        ColorMode::Grayscale => [li_color::convert::rgb_to_gray(rgb), 0., 0., 0.],
        _ => [rgb[0], rgb[1], rgb[2], 0.],
    };
    Color { mode, c, alpha: f32::from(bytes[3]) / 255. }
}
fn edit_color(ui: &mut egui::Ui, c: &mut Color) -> bool {
    let mut bytes = c.to_rgba8();
    if ui.color_edit_button_srgba_unmultiplied(&mut bytes).changed() {
        *c = edited_color(c.mode, bytes);
        true
    } else {
        false
    }
}
/// Call only for the Document palette tab in a vector document. `paint` is the current
/// fill/stroke proxy; `targets` captures its exact paint locations (including gradient stops).
pub fn panel(ui: &mut egui::Ui, s: &Session, access: Access, paint: &Fill, targets: &[PaintTarget], state: &mut State) -> Result<Option<Event>> {
    let capture = Capture::begin(s)?;
    let book = (access.read)(&s.active().ok_or(EngineError::NoDocument)?.doc)?;
    if state.editor.as_ref().is_some_and(|r| r.capture != capture) {
        state.cancel();
        state.error = Some(tl!("Document changed; start again").into());
    }
    state.selected.retain(|id| book.swatch(*id).is_some());
    let mut event = None;
    let mut action = None;
    ui.label(tl!("Document Palette"));
    ui.horizontal_wrapped(|ui| {
        if ui.button(tl!("Add Swatch")).clicked() {
            match book.next_id() {
                Ok(id) => {
                    action = Some(Action::Add {
                        swatch: Swatch {
                            id: SwatchId(id),
                            name: book.free_name(tl!("Swatch")),
                            paint: paint.clone(),
                            global: false,
                            spot: false,
                            group: None,
                            stops: Vec::new(),
                        },
                    })
                }
                Err(e) => state.error = Some(e),
            }
        }
        if ui.button(tl!("Add Gradient")).clicked() {
            match book.next_id() {
                Ok(id) => {
                    action = Some(Action::Add {
                        swatch: Swatch {
                            id: SwatchId(id),
                            name: book.free_name(tl!("Gradient")),
                            paint: Fill::gradient(vec![(0., Color::BLACK), (1., Color::WHITE)], 0., 100., GradientStyle::Linear, false),
                            global: false,
                            spot: false,
                            group: None,
                            stops: Vec::new(),
                        },
                    })
                }
                Err(e) => state.error = Some(e),
            }
        }
        if ui.button(tl!("Add Used Colours")).clicked() {
            action = Some(Action::AddUsed { global: false, group: None });
        }
        if ui.button(tl!("Import ASE…")).clicked() {
            event = Some(Event::ImportAse(capture));
        }
        if ui.button(tl!("Export ASE…")).clicked() {
            event = Some(Event::ExportAse(book.clone()));
        }
        if ui.button(tl!("Recolor Artwork…")).clicked() {
            event = Some(Event::Recolor);
        }
    });
    let ids: Vec<_> = state.selected.iter().copied().collect();
    ui.horizontal_wrapped(|ui| {
        ui.label(tl!("Tint (%)"));
        ui.add(egui::DragValue::new(&mut state.tint_percent).range(0.0..=100.0).speed(1.));
        if ui.add_enabled(!ids.is_empty(), egui::Button::new(tl!("Delete Swatch"))).clicked() {
            action = Some(Action::Delete { ids: ids.clone() });
        }
        if ui.add_enabled(ids.len() > 1, egui::Button::new(tl!("Merge Swatches"))).clicked() {
            action = Some(Action::Merge { into: ids[0], ids: ids[1..].to_vec() });
        }
        if ui.add_enabled(ids.len() == 1 && !targets.is_empty(), egui::Button::new(tl!("Apply Swatch"))).clicked()
            && let Some(w) = book.swatch(ids[0])
        {
            action = Some(if w.global {
                Action::Assign { targets: targets.to_vec(), swatch: w.id, tint: state.tint_percent }
            } else {
                Action::AssignPaint { targets: targets.to_vec(), swatch: w.id }
            });
        }
    });
    ui.horizontal(|ui| {
        ui.label(tl!("Group Name"));
        ui.text_edit_singleline(&mut state.group_name);
        if ui.add_enabled(!ids.is_empty() && !state.group_name.trim().is_empty(), egui::Button::new(tl!("Create Swatch Group"))).clicked() {
            match book.next_id() {
                Ok(id) => action = Some(Action::Group { group: SwatchGroup { id, name: book.free_name(&state.group_name) }, ids: ids.clone() }),
                Err(e) => state.error = Some(e),
            }
        }
    });
    let mut grouped = std::collections::HashMap::<Option<u64>, Vec<&Swatch>>::new();
    for swatch in &book.swatches {
        grouped.entry(swatch.group).or_default().push(swatch);
    }
    egui::ScrollArea::vertical().max_height(340.).show(ui, |ui| {
        for group in std::iter::once(None).chain(book.groups.iter().map(Some)) {
            if let Some(group) = group {
                ui.horizontal(|ui| {
                    ui.label(&group.name);
                    if ui.small_button(tl!("Ungroup")).clicked() {
                        action = Some(Action::Ungroup { id: group.id });
                    }
                    if ui.small_button(tl!("Delete Group")).clicked() {
                        action = Some(Action::DeleteGroup { id: group.id });
                    }
                });
            }
            for w in grouped.get(&group.map(|g| g.id)).into_iter().flatten() {
                ui.push_id(w.id.0, |ui| {
                    ui.horizontal(|ui| {
                        let r = chip(ui, &w.paint).on_hover_text(&w.name);
                        let mut selected = state.selected.contains(&w.id);
                        let label = ui.checkbox(&mut selected, &w.name);
                        if selected {
                            state.selected.insert(w.id);
                        } else {
                            state.selected.remove(&w.id);
                        }
                        if r.clicked() {
                            state.selected.clear();
                            state.selected.insert(w.id);
                        }
                        if w.global {
                            ui.label(tl!("Global"));
                        }
                        if w.spot {
                            ui.label(tl!("Spot"));
                        }
                        if r.double_clicked() || label.double_clicked() {
                            state.editor = Some(Request { capture, action: Action::Edit { swatch: (*w).clone() } });
                            state.preview_enabled = true;
                            state.generation = state.generation.wrapping_add(1);
                            state.preview = None;
                        }
                    });
                });
            }
        }
    });
    let escape = crate::shortcut_dispatch::Focus::of(ui.ctx()) == crate::shortcut_dispatch::Focus::None && ui.input(|i| i.key_pressed(egui::Key::Escape));
    let mut close = false;
    let mut changed = state.preview.is_none();
    if let Some(request) = &mut state.editor {
        egui::Window::new(tl!("Swatch Options")).id(egui::Id::new("document_swatch_options")).collapsible(false).show(ui.ctx(), |ui| {
            if let Action::Edit { swatch } = &mut request.action {
                ui.horizontal(|ui| {
                    ui.label(tl!("Name"));
                    changed |= ui.text_edit_singleline(&mut swatch.name).changed();
                });
                match &mut swatch.paint {
                    Fill::Solid(c) => {
                        changed |= edit_color(ui, c);
                        if c.mode == photocraft_doc::ColorMode::Rgb {
                            for (index, label) in [(0, tl!("Swatch Red")), (1, tl!("Swatch Green")), (2, tl!("Swatch Blue"))] {
                                ui.horizontal(|ui| {
                                    ui.label(label);
                                    let r = ui.add(egui::DragValue::new(&mut c.c[index]).range(0.0..=1.0).speed(0.01));
                                    changed |= r.changed();
                                    r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::DragValue, true, label));
                                });
                            }
                        }
                        changed |= ui.checkbox(&mut swatch.global, tl!("Global Colour")).changed();
                        if ui.checkbox(&mut swatch.spot, tl!("Spot Colour")).changed() {
                            changed = true;
                            if swatch.spot {
                                swatch.global = true;
                            }
                        }
                        if !swatch.global {
                            swatch.spot = false;
                        }
                    }
                    Fill::Gradient { stops, angle, scale, .. } => {
                        ui.label(tl!("Gradient Stops"));
                        for (i, (at, c)) in stops.iter_mut().enumerate() {
                            ui.push_id(i, |ui| {
                                ui.horizontal(|ui| {
                                    changed |= ui.add(egui::DragValue::new(at).range(0.0..=1.0)).changed();
                                    changed |= edit_color(ui, c);
                                    if c.mode == photocraft_doc::ColorMode::Rgb {
                                        for (index, label) in [(0, tl!("Swatch Red")), (1, tl!("Swatch Green")), (2, tl!("Swatch Blue"))] {
                                            ui.horizontal(|ui| {
                                                ui.label(label);
                                                let r = ui.add(egui::DragValue::new(&mut c.c[index]).range(0.0..=1.0).speed(0.01));
                                                changed |= r.changed();
                                                r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::DragValue, true, label));
                                            });
                                        }
                                    }
                                });
                            });
                        }
                        ui.horizontal(|ui| {
                            ui.label(tl!("Angle"));
                            changed |= ui.add(egui::DragValue::new(angle).range(-360.0..=360.0)).changed();
                        });
                        ui.horizontal(|ui| {
                            ui.label(tl!("Scale"));
                            changed |= ui.add(egui::DragValue::new(scale).range(0.01..=10000.0)).changed();
                        });
                    }
                    _ => {}
                }
            }
            changed |= ui.checkbox(&mut state.preview_enabled, tl!("Preview")).changed();
            ui.horizontal(|ui| {
                if ui.button(tl!("Apply")).clicked() {
                    event = Some(Event::Commit(request.clone()));
                    close = true;
                }
                if ui.button(tl!("Cancel")).clicked() {
                    close = true;
                }
            });
        });
    }
    if escape {
        close = true;
    }
    if close {
        state.cancel();
    } else if changed && let Some(r) = &state.editor {
        if state.preview_enabled {
            match colours::prepare(s, access, r) {
                Ok(p) => {
                    state.generation = state.generation.wrapping_add(1);
                    state.preview = Some(p);
                    state.error = None;
                }
                Err(e) => {
                    state.generation = state.generation.wrapping_add(1);
                    state.preview = None;
                    state.error = Some(e.to_string());
                }
            }
        } else {
            state.generation = state.generation.wrapping_add(1);
            state.preview = None;
        }
    }
    if let Some(e) = &state.error {
        ui.colored_label(Color32::RED, e);
    }
    if let Some(action) = action {
        event = Some(Event::Commit(Request { capture, action }));
    }
    Ok(event)
}
#[cfg(test)]
#[path = "swatches_doc/tests.rs"]
pub(crate) mod tests;

#[cfg(test)]
#[path = "swatches_doc/app_gpu.rs"]
mod app_gpu;
