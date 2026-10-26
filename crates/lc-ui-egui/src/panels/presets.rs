//! The Presets column (opens to the left of the Edit panel): grouped presets with hover preview
//! (the loupe shows the photo with the preset while the pointer rests on it; nothing is
//! committed), optional live thumbnails, amount  create/favourite, import/export of preset
//! files.

use std::collections::BTreeMap;
use std::sync::Arc;

use egui::{Align2, Color32, Rect, Sense, pos2, vec2};
use lightcraft_develop::{ControlSpec, Preset, Section, Track};
use serde_json::json;

use crate::icons::{Icon, paint};
use crate::theme::Tokens;
use crate::widgets::{divider, icon_button, register};
use crate::{HoverPreview, LightcraftApp};

/// Long edge of preset thumbnails (px).
const THUMB_EDGE: usize = 128;

/// The preset amount slider may replace only the undo step it created.
#[derive(Clone)]
struct AppliedPreset {
    id: String,
    amount: f64,
    selected: Arc<[lightcraft_catalog::PhotoId]>,
    active: Option<lightcraft_catalog::PhotoId>,
    commits: u64,
    group: Option<u64>,
    settings: Arc<[(lightcraft_catalog::PhotoId, Arc<lightcraft_develop::DevelopSettings>)]>,
}

impl AppliedPreset {
    fn new(session: &lightcraft_engine::Session, id: String, amount: f64) -> Self {
        Self {
            id,
            amount,
            selected: session.selection.ids.clone().into(),
            active: session.active(),
            commits: session.commits(),
            group: session.undo_entry().and_then(|entry| entry.group),
            settings: session.selected().into_iter().filter_map(|photo| session.develop_of(photo).map(|settings| (photo, settings))).collect(),
        }
    }

    fn is_current(&self, session: &lightcraft_engine::Session) -> bool {
        self.selected.as_ref() == session.selection.ids.as_slice() && self.active == session.active() && !self.is_stale(session)
    }

    /// A later edit, undo or redo (on any photo) owns the undo step: the slider may never replace it.
    fn is_stale(&self, session: &lightcraft_engine::Session) -> bool {
        !((self.commits == session.commits()
            || (self.group.is_some()
                && session.undo_entry().and_then(|entry| entry.group) == self.group
                && self.settings.iter().all(|(photo, then)| session.develop_of(*photo).is_some_and(|now| same_applied_settings(then, &now)))))
            && session.interaction.is_none()
            && session.redo_entry().is_none()
            && session
                .undo_entry()
                .is_some_and(|entry| session.presets.iter().any(|preset| preset.id == self.id && entry.label == format!("Preset: {}", preset.name))))
    }
}

/// The preset each photo was given, with its settings right after: the panel marks it while the
/// photo still has exactly those settings, so it is still marked on coming back to the photo
/// (issue #2392) and drops once the photo is edited further.
#[derive(Clone, Default)]
struct AppliedByPhoto {
    generation: u64,
    photos: std::collections::HashMap<lightcraft_catalog::PhotoId, (String, Arc<lightcraft_develop::DevelopSettings>)>,
}

impl AppliedByPhoto {
    const ID: &'static str = "preset-applied-by-photo";

    /// Remember `preset` on the photos `preset.apply` just changed (the selection, else the active
    /// photo; an active photo outside the selection wasn't changed and isn't marked).
    fn remember(ctx: &egui::Context, session: &lightcraft_engine::Session, preset: &str) {
        ctx.data_mut(|data| {
            let map = data.get_temp_mut_or_default::<Self>(egui::Id::new(Self::ID));
            if map.generation != session.library_generation {
                map.photos.clear();
            }
            map.generation = session.library_generation;
            for photo in session.selected() {
                if let Some(settings) = session.develop_of(photo) {
                    map.photos.insert(photo, (preset.to_string(), settings));
                }
            }
        });
    }

    fn of(ctx: &egui::Context, session: &lightcraft_engine::Session, photo: Option<lightcraft_catalog::PhotoId>) -> Option<String> {
        let photo = photo?;
        ctx.data_mut(|data| {
            let map = data.get_temp_mut_or_default::<Self>(egui::Id::new(Self::ID));
            if map.generation != session.library_generation {
                return None;
            }
            let (preset, then) = map.photos.get_mut(&photo)?;
            let now = session.develop_of(photo)?;
            if Arc::ptr_eq(then, &now) || same_applied_settings(then, &now) {
                *then = now;
                Some(preset.clone())
            } else {
                None
            }
        })
    }
}

fn same_applied_settings(then: &lightcraft_develop::DevelopSettings, now: &lightcraft_develop::DevelopSettings) -> bool {
    if then == now {
        return true;
    }
    // Background analysis resolves these fields without replacing the applied preset.
    let mut resolved = then.clone();
    resolved.geometry.upright_transform = now.geometry.upright_transform;
    if resolved.wb.mode == lightcraft_develop::WbMode::Auto {
        resolved.wb.temp = now.wb.temp;
        resolved.wb.tint = now.wb.tint;
    }
    resolved == *now
}

pub fn show(app: &mut LightcraftApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    egui::Panel::right("presets_panel")
        .exact_size(t.panel_w)
        .resizable(false)
        .frame(egui::Frame::NONE.fill(t.chrome).stroke(egui::Stroke::new(1.0, t.divider)))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let (hr, _) = ui.allocate_exact_size(vec2(ui.available_width(), 46.0), Sense::hover());
            ui.painter().text(
                pos2(hr.left() + 24.0, hr.center().y + 2.0),
                Align2::LEFT_CENTER,
                li_i18n::library::t("Presets"),
                t.semibold(15.0),
                t.text,
            );
            let mut hdr = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(Rect::from_min_max(pos2(hr.right() - 80.0, hr.top()), hr.right_bottom()))
                    .layout(egui::Layout::right_to_left(egui::Align::Center)),
            );
            if icon_button(&mut hdr, "presetCreate", Icon::Plus, vec2(28.0, 28.0), false, app.session.active().is_some(), "Create Preset…").clicked()
            {
                app.ui.dialog = Some(crate::state::Dialog::create_preset());
            }
            let more = icon_button(&mut hdr, "presetMore", Icon::More, vec2(28.0, 28.0), false, true, "More preset options");
            egui::Popup::menu(&more).show(|ui| {
                let r = ui.checkbox(&mut app.ui.preset_thumbs, li_i18n::library::t("Show Thumbnails"));
                register(ui.ctx(), "presetMenu:thumbnails", r.rect);
                ui.separator();
                if ui.button(li_i18n::library::t("Import Presets…")).clicked() {
                    let _ = app.run("file.importPresets", json!({}));
                }
                let any_user = app.session.presets.iter().any(|p| !p.builtin);
                if ui.add_enabled(any_user, egui::Button::new(li_i18n::library::t("Export User Presets…"))).clicked() {
                    let _ = app.run("file.exportPresets", json!({}));
                }
            });
            divider(ui);
            // amount slider (applies to the last applied preset)
            let amt_id = egui::Id::new("preset-amount");
            let last: Option<AppliedPreset> = ui.data(|d| d.get_temp(amt_id));
            if let Some(last) = last.clone().filter(|last| last.is_current(&app.session)) {
                if last.commits != app.session.commits() {
                    ui.data_mut(|data| data.insert_temp(amt_id, AppliedPreset::new(&app.session, last.id.clone(), last.amount)));
                }
                let spec = ControlSpec {
                    id: "presetAmount",
                    label: "Amount",
                    section: Section::Profile,
                    min: 0.0,
                    max: 200.0,
                    default: 100.0,
                    step: 1.0,
                    decimals: 0,
                    track: Track::Plain,
                };
                let out =
                    crate::widgets::numeric_slider(ui, &spec, last.amount, true, None, crate::widgets::numeric_owner(app, ("preset", &last.id)));
                if let Some(v) = out.value {
                    match app.run("edit.undo", json!({})) {
                        Ok(_) => match app.run("preset.apply", json!({"id": &last.id, "amount": v})) {
                            Ok(_) => {
                                AppliedByPhoto::remember(ui.ctx(), &app.session, &last.id);
                                ui.data_mut(|d| d.insert_temp(amt_id, AppliedPreset::new(&app.session, last.id, v)));
                            }
                            Err(error) => {
                                app.toast_error(ui.ctx(), error);
                                if let Err(error) = app.run("edit.redo", json!({})) {
                                    app.toast_error(ui.ctx(), error);
                                }
                                ui.data_mut(|d| d.remove::<AppliedPreset>(amt_id));
                            }
                        },
                        Err(error) => app.toast_error(ui.ctx(), error),
                    }
                }
                divider(ui);
            } else if last.is_some_and(|last| last.is_stale(&app.session)) {
                ui.data_mut(|d| d.remove::<AppliedPreset>(amt_id));
            }
            let active = app.session.active();
            let marked = AppliedByPhoto::of(ui.ctx(), &app.session, active);
            let current = active.and_then(|id| app.session.develop_of(id)).map(|d| (*d).clone());
            let thumbs = app.ui.preset_thumbs && active.is_some();
            let row_h = if thumbs { 54.0 } else { 26.0 };
            let scroll = egui::ScrollArea::vertical().id_salt("presets-scroll").auto_shrink([false, false]).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let mut groups: BTreeMap<String, Vec<Preset>> = BTreeMap::new();
                for p in &app.session.presets {
                    groups.entry(p.group.clone()).or_default().push(p.clone());
                }
                for (g, items) in groups {
                    // A group holding stock presets is a stock group (shown in the UI language), even
                    // when user presets share its name; a group of user presets keeps its name.
                    let builtin = items.iter().any(|p| p.builtin);
                    let open_id = egui::Id::new(("preset-group", &g));
                    let open: bool = ui.data(|d| d.get_temp(open_id)).unwrap_or(true);
                    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
                    register(ui.ctx(), format!("presetGroup:{g}"), r);
                    paint(
                        ui.painter(),
                        Rect::from_center_size(pos2(r.left() + 22.0, r.center().y), vec2(12.0, 12.0)),
                        if open { Icon::ChevronDown } else { Icon::ChevronRight },
                        t.text_label,
                    );
                    ui.painter().text(
                        pos2(r.left() + 36.0, r.center().y),
                        Align2::LEFT_CENTER,
                        li_i18n::library::builtin_label(&g, builtin),
                        t.semibold(13.0),
                        t.text_label,
                    );
                    if resp.clicked() {
                        ui.data_mut(|d| d.insert_temp(open_id, !open));
                    }
                    resp.context_menu(|ui| {
                        if ui.button(li_i18n::library::t("Export Group…")).clicked() {
                            let _ = app.run("file.exportPresets", json!({"group": g}));
                        }
                    });
                    if !open {
                        continue;
                    }
                    for pr in items {
                        let (pid, name, fav) = (pr.id.clone(), li_i18n::library::builtin_label(&pr.name, pr.builtin).to_string(), pr.favorite);
                        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), row_h), Sense::click());
                        register(ui.ctx(), format!("preset:{pid}"), r);
                        let is_last = marked.as_deref() == Some(pid.as_str());
                        resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, is_last, &name));
                        if is_last {
                            register(ui.ctx(), "presetApplied", r);
                            ui.painter().rect_filled(r, 0.0, t.tool_active);
                        } else if resp.hovered() {
                            ui.painter().rect_filled(r, 0.0, t.hover.gamma_multiply(0.7));
                        }
                        let look = if thumbs || resp.hovered() { current.as_ref().map(|d| pr.apply(d, 1.0)) } else { None };
                        let mut text_x = r.left() + 40.0;
                        if thumbs && let (Some(id), Some(s)) = (active, look.as_ref()) {
                            let tr = Rect::from_min_size(pos2(r.left() + 36.0, r.top() + 5.0), vec2(66.0, row_h - 10.0));
                            ui.painter().rect_filled(tr, 2.0, t.canvas);
                            if let Some(job) = app.session.variant_job(id, s, THUMB_EDGE)
                                && let Some(tex) = app.renderer.variant(job)
                            {
                                ui.painter().image(tex.tex.id(), tr, cover_uv(tr, tex.size), Color32::WHITE);
                            }
                            text_x = tr.right() + 10.0;
                        }
                        ui.painter().text(pos2(text_x, r.center().y), Align2::LEFT_CENTER, &name, t.font(13.0), t.text_label);
                        if fav {
                            paint(
                                ui.painter(),
                                Rect::from_center_size(pos2(r.right() - 20.0, r.center().y), vec2(12.0, 12.0)),
                                Icon::StarFilled,
                                t.star,
                            );
                        }
                        // resting on a preset previews it in the loupe (no history entry)
                        if resp.hovered()
                            && let Some(s) = look
                        {
                            let label = li_i18n::tr_format!("Preset: {name}", name = name);
                            app.hover_preview = Some(HoverPreview { label: label.clone(), settings: s });
                            if let Some(id) = active {
                                ui.data_mut(|d| d.insert_temp(egui::Id::new("last-preset-hover"), (id, pr.clone(), label)));
                            }
                        }
                        if resp.clicked() {
                            match app.run("preset.apply", json!({"id": pid, "amount": 100})) {
                                Ok(_) => {
                                    AppliedByPhoto::remember(ui.ctx(), &app.session, &pid);
                                    ui.data_mut(|d| d.insert_temp(amt_id, AppliedPreset::new(&app.session, pid.clone(), 100.0)));
                                    ui.data_mut(|d| d.remove::<(lightcraft_catalog::PhotoId, Preset, String)>(egui::Id::new("last-preset-hover")));
                                    app.toast(ui.ctx(), li_i18n::tr_format!("Preset: {name}", name = name));
                                }
                                Err(error) => app.toast_error(ui.ctx(), error),
                            }
                        }
                        let (builtin, group) = (pr.builtin, pr.group.clone());
                        resp.context_menu(|ui| {
                            if ui.button(li_i18n::library::t(if fav { "Remove from Favorites" } else { "Add to Favorites" })).clicked() {
                                let _ = app.run("preset.favorite", json!({"id": pid}));
                            }
                            if builtin {
                                return;
                            }
                            ui.separator();
                            if ui
                                .add_enabled(app.session.active().is_some(), egui::Button::new(li_i18n::library::t("Update with Current Settings")))
                                .clicked()
                            {
                                match app.run("preset.update", json!({"id": pid})) {
                                    Ok(_) => app.toast(ui.ctx(), li_i18n::tr_format!("Updated “{name}”", name = name)),
                                    Err(e) => app.toast(ui.ctx(), e),
                                }
                            }
                            if ui.button(li_i18n::library::t("Rename…")).clicked() {
                                crate::panels::dialogs::prompt(
                                    app,
                                    "Rename Preset",
                                    "Preset name",
                                    &name,
                                    "preset.rename",
                                    json!({"id": pid}),
                                    "name",
                                );
                            }
                            ui.menu_button(li_i18n::library::t("Move to Group"), |ui| {
                                let mut groups: Vec<String> = app.session.presets.iter().filter(|p| !p.builtin).map(|p| p.group.clone()).collect();
                                groups.sort();
                                groups.dedup();
                                for g in groups.iter().filter(|g| **g != group) {
                                    if ui.button(g).clicked() {
                                        let _ = app.run("preset.move", json!({"id": pid, "group": g}));
                                    }
                                }
                                if ui.button(li_i18n::library::t("New Group…")).clicked() {
                                    crate::panels::dialogs::prompt(
                                        app,
                                        "Move Preset to New Group",
                                        "Group name",
                                        "",
                                        "preset.move",
                                        json!({"id": pid}),
                                        "group",
                                    );
                                }
                            });
                            ui.separator();
                            if ui.button(li_i18n::library::t("Delete Preset")).clicked() {
                                let _ = app.run("preset.delete", json!({"id": pid}));
                            }
                        });
                    }
                }
                ui.add_space(30.0);
            });
            let last_hover_id = egui::Id::new("last-preset-hover");
            let pointer_in_scroll = ui.ctx().input(|i| i.pointer.hover_pos()).is_some_and(|pos| scroll.inner_rect.contains(pos));
            if pointer_in_scroll {
                if app.hover_preview.is_none()
                    && let (Some(id), Some(curr)) = (active, current.as_ref())
                    && let Some((saved_id, saved_preset, saved_label)) =
                        ui.data(|d| d.get_temp::<(lightcraft_catalog::PhotoId, Preset, String)>(last_hover_id))
                    && saved_id == id
                {
                    app.hover_preview = Some(HoverPreview { label: saved_label, settings: saved_preset.apply(curr, 1.0) });
                }
            } else {
                ui.data_mut(|d| d.remove::<(lightcraft_catalog::PhotoId, Preset, String)>(last_hover_id));
            }
        });
}

/// The part of a `size` image that covers `r` (centre crop), as texture coordinates.
pub fn cover_uv(r: Rect, size: [usize; 2]) -> Rect {
    let a = size[0] as f32 / size[1].max(1) as f32;
    let ra = r.width() / r.height().max(1.0);
    if a > ra {
        let f = ra / a;
        Rect::from_min_max(pos2((1.0 - f) / 2.0, 0.0), pos2((1.0 + f) / 2.0, 1.0))
    } else {
        let f = a / ra;
        Rect::from_min_max(pos2(0.0, (1.0 - f) / 2.0), pos2(1.0, (1.0 + f) / 2.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amount_slider_does_not_undo_a_later_edit() {
        let mut session = lightcraft_engine::Session::with_demo();
        let pid = "lc.golden-hour";
        session.execute("preset.apply", &json!({"id": pid})).unwrap();
        let state = AppliedPreset::new(&session, pid.into(), 100.0);
        assert!(state.is_current(&session));

        session.execute("develop.set", &json!({"values": {"light.exposure": 0.75}})).unwrap();
        assert!(!state.is_current(&session), "a newer edit owns the undo step");
    }

    #[test]
    fn amount_slider_does_not_undo_on_another_photo() {
        let mut session = lightcraft_engine::Session::with_demo();
        let pid = "lc.golden-hour";
        session.execute("preset.apply", &json!({"id": pid})).unwrap();
        let state = AppliedPreset::new(&session, pid.into(), 100.0);
        let active = session.active();
        let other = session.visible().iter().copied().find(|id| Some(*id) != active).unwrap();
        session.execute("library.select", &json!({"ids": [other.0]})).unwrap();
        assert!(!state.is_current(&session), "the preset belongs to the earlier selection");
    }

    #[test]
    fn amount_and_mark_survive_the_presets_grouped_analysis() {
        let mut session = lightcraft_engine::Session::with_demo();
        let preset = "lc.golden-hour";
        session.execute("preset.apply", &json!({"id":preset})).unwrap();
        session.undo.last_mut().unwrap().group = Some(42);
        let state = AppliedPreset::new(&session, preset.into(), 100.0);
        let context = egui::Context::default();
        AppliedByPhoto::remember(&context, &session, preset);
        let photo = session.active().unwrap();
        let mut settings = (*session.develop_of(photo).unwrap()).clone();
        settings.geometry.upright_transform = Some(lightcraft_geom::Homography::IDENTITY);
        session
            .execute_grouped("develop.uprightAnalysed", 42, "Preset: Golden Hour", |session| {
                session.commit(
                    "Upright",
                    lightcraft_catalog::Op::SetDevelop { id: photo, settings: Arc::new(settings), label: "Upright".into(), edited: None },
                )?;
                Ok(json!({}))
            })
            .unwrap();
        assert!(state.is_current(&session));
        assert_eq!(AppliedByPhoto::of(&context, &session, Some(photo)).as_deref(), Some(preset));
        session.execute("develop.set", &json!({"control":"light.exposure","value":0.75})).unwrap();
        assert!(!state.is_current(&session));
        assert_eq!(AppliedByPhoto::of(&context, &session, Some(photo)), None);
    }
}
