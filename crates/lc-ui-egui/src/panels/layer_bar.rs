//! Develop layer selection. This bar only edits settings; rendering remains on preview workers.
use crate::{
    LightcraftApp,
    icons::Icon,
    theme::Tokens,
    widgets::{icon_button, register, text_button},
};
use egui::{Align2, Rect, Sense, pos2, vec2};
use lightcraft_catalog::PhotoId;
use lightcraft_develop::{ControlSpec, Section, Track};
use serde_json::json;

pub const OPACITY: ControlSpec = ControlSpec {
    id: "opacity",
    label: "Opacity",
    section: Section::Light,
    min: 0.0,
    max: 100.0,
    default: 100.0,
    step: 1.0,
    decimals: 0,
    track: Track::Plain,
};

pub fn select_background(app: &mut LightcraftApp) {
    app.session.active_mask = None;
    app.ui.tool.clear();
    let _ = app.session.end_interaction();
    app.ui.pending_mask_shape = None;
    app.ui.pending_mask_origin = None;
    app.ui.renaming_mask = None;
}

pub fn global_only(app: &mut LightcraftApp, ui: &mut egui::Ui, key: &str, label: &str) {
    let t = Tokens::get(ui.ctx());
    egui::Frame::NONE.inner_margin(egui::Margin::symmetric(24, 6)).show(ui, |ui| {
        ui.add_enabled(false, egui::Label::new(egui::RichText::new(label).font(t.semibold(13.0))));
        ui.label(egui::RichText::new("This tool applies to the whole image.").size(11.0).color(t.text_label));
        let r = ui.link("Select Background");
        register(ui.ctx(), format!("layerBackground:{key}"), r.rect);
        if r.clicked() {
            select_background(app);
        }
    });
}

pub fn new_menu(app: &mut LightcraftApp, ui: &mut egui::Ui) {
    for (kind, label) in [
        ("full", "New Filled Layer"),
        ("empty", "New Empty Layer"),
        ("brush", "Brush"),
        ("linear", "Linear Gradient"),
        ("radial", "Radial Gradient"),
        ("colorRange", "Color Range"),
        ("luminanceRange", "Luminance Range"),
        ("object", "AI Object"),
        ("prompt", "Describe…"),
        ("subject", "AI Subject"),
        ("sky", "AI Sky"),
        ("background", "AI Background"),
        ("depth", "Depth Range"),
    ] {
        if text_button(ui, &format!("maskNew:{kind}"), label, false).clicked() {
            app.ui.layer_bar_collapsed = false;
            app.ui.mask_overlay = true;
            if matches!(kind, "linear" | "radial") {
                let _ = app.run("develop.beginInteraction", json!({"label":label}));
            }
            app.ui.tool.clear();
            if kind == "object" {
                super::masking::start_object(app, ui.ctx(), "new");
            } else if kind == "prompt" {
                super::masking::start_describe(app, "new");
            } else if app.run("mask.add", json!({"kind":kind})).is_ok() {
                if matches!(kind, "brush" | "linear" | "radial" | "colorRange") {
                    app.ui.tool = kind.into();
                }
                if matches!(kind, "linear" | "radial") {
                    app.ui.pending_mask_shape = app.session.active_mask.map(|id| (id, 0));
                }
                app.ui.layer_components_open = kind == "empty";
                app.ui.layer_bar_collapsed = false;
                app.ui.mask_overlay = true;
            }
            ui.close();
        }
    }
}

fn menu(app: &mut LightcraftApp, ui: &mut egui::Ui, m: &lightcraft_develop::Mask) {
    for (label, cmd) in [("Invert", "mask.invert"), ("Duplicate", "mask.duplicate"), ("Delete", "mask.delete"), ("Reset", "mask.resetTools")] {
        if text_button(ui, &format!("layerMenu:{label}"), label, false).clicked() {
            let _ = app.run(cmd, json!({"id":m.id}));
            ui.close();
        }
    }
    if text_button(ui, "layerMenu:Rename", "Rename…", false).clicked() {
        app.ui.renaming_mask = Some((m.id, m.name.clone()));
        ui.close();
    }
}

pub fn show(app: &mut LightcraftApp, ui: &mut egui::Ui, id: PhotoId) {
    app.session.resolve_active_mask();
    if app.ui.pending_mask_shape.is_some_and(|(mid, _)| app.session.active_mask != Some(mid)) {
        app.ui.pending_mask_shape = None;
        app.ui.pending_mask_origin = None;
    }
    let d = app.session.develop_of(id).unwrap_or_default();
    let t = Tokens::get(ui.ctx());
    let frame = egui::Frame::NONE.inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| {
        ui.horizontal(|ui| {
            if !d.masks.is_empty() && text_button(ui, "layerCollapse", if app.ui.layer_bar_collapsed { "▸" } else { "▾" }, false).clicked() {
                app.ui.layer_bar_collapsed = !app.ui.layer_bar_collapsed;
            }
            let name = if app.ui.layer_bar_collapsed {
                d.masks.iter().find(|m| Some(m.id) == app.session.active_mask).map(|m| m.name.as_str()).unwrap_or("Background")
            } else {
                "Background"
            };
            if text_button(ui, "layerBackground", name, app.session.active_mask.is_none()).clicked() {
                app.ui.layer_bar_focus = true;
                select_background(app);
            }
            let plus = text_button(ui, "layerNew", "+", false).on_hover_text("New layer");
            egui::Popup::menu(&plus).show(|ui| new_menu(app, ui));
        });
        if !app.ui.layer_bar_collapsed {
            for (index, m) in d.masks.iter().enumerate() {
                let selected = app.session.active_mask == Some(m.id);
                ui.push_id((id, m.id), |ui| {
                    let bg = if selected { t.tool_active } else { t.chrome };
                    egui::Frame::NONE.fill(bg).inner_margin(4).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 3.0;
                            if icon_button(
                                ui,
                                &format!("maskVisible:{}", m.id),
                                if m.visible { Icon::Eye } else { Icon::EyeOff },
                                vec2(22.0, 24.0),
                                false,
                                true,
                                "Show/hide layer",
                            )
                            .clicked()
                            {
                                let _ = app.run("mask.visible", json!({"id":m.id}));
                            }
                            let room = (ui.available_width() - 84.0).max(30.0);
                            if let Some((_, name)) = app.ui.renaming_mask.as_mut().filter(|(id, _)| *id == m.id) {
                                let r = ui.add_sized(vec2(room, 24.0), egui::TextEdit::singleline(name).id_salt("rename"));
                                register(ui.ctx(), format!("maskRename:{}", m.id), r.rect);
                                if !app.host_modal && !r.has_focus() && !r.lost_focus() {
                                    r.request_focus();
                                }
                                let cancel = ui.input(|i| i.key_pressed(egui::Key::Escape));
                                if cancel || r.lost_focus() {
                                    let name = name.trim().to_string();
                                    app.ui.renaming_mask = None;
                                    if !cancel && !name.is_empty() && name != m.name {
                                        let _ = app.run("mask.rename", json!({"id":m.id,"name":name}));
                                    }
                                }
                            } else {
                                let label = if m.components.is_empty() { format!("{} (no mask)", m.name) } else { m.name.clone() };
                                let r = ui
                                    .add_sized(vec2(room, 24.0), egui::Label::new(label).truncate().sense(Sense::click_and_drag()))
                                    .on_hover_text(&m.name);
                                register(ui.ctx(), format!("mask:{}", m.id), r.rect);
                                if r.clicked() {
                                    app.ui.layer_bar_focus = true;
                                    let _ = app.session.end_interaction();
                                    let _ = app.run("mask.select", json!({"id":m.id}));
                                    app.ui.tool.clear();
                                    app.ui.pending_mask_shape = None;
                                    r.request_focus();
                                }
                                if r.double_clicked() || (r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                                    app.ui.renaming_mask = Some((m.id, m.name.clone()));
                                }
                                if r.drag_started() {
                                    app.ui.dragging_layer = Some(m.id);
                                }
                                if let Some(mid) = app.ui.dragging_layer
                                    && mid != m.id
                                    && r.contains_pointer()
                                    && ui.input(|i| i.pointer.any_released())
                                {
                                    app.ui.dragging_layer = None;
                                    if let Some(old) = d.masks.iter().position(|m| m.id == mid) {
                                        let _ = app.run("mask.move", json!({"id":mid,"delta":index as i64-old as i64}));
                                    }
                                }
                                if r.has_focus() {
                                    let delta = ui.input(|i| {
                                        if i.key_pressed(egui::Key::ArrowDown) {
                                            1
                                        } else if i.key_pressed(egui::Key::ArrowUp) {
                                            -1
                                        } else {
                                            0
                                        }
                                    });
                                    if delta != 0 {
                                        let next = (index as i32 + delta).clamp(-1, d.masks.len() as i32 - 1);
                                        if next < 0 {
                                            select_background(app);
                                        } else {
                                            let _ = app.run("mask.select", json!({"id":d.masks[next as usize].id}));
                                        }
                                    }
                                }
                                r.context_menu(|ui| menu(app, ui, m));
                            }
                            let mut opacity = m.opacity;
                            let r = ui.add_sized(
                                vec2(48.0, 24.0),
                                egui::DragValue::new(&mut opacity).range(0.0..=100.0).speed(1.0).suffix("%").max_decimals(0),
                            );
                            register(ui.ctx(), format!("layerOpacity:{}", m.id), r.rect);
                            if r.drag_started() {
                                let _ = app.run("develop.beginInteraction", json!({"label":"Opacity","mask":m.id}));
                            }
                            if r.changed() {
                                let _ = app.run("mask.setOpacity", json!({"id":m.id,"value":opacity}));
                            }
                            if r.drag_stopped() {
                                let _ = app.run("develop.endInteraction", json!({}));
                            }
                            let more = text_button(ui, &format!("layerMenu:{}", m.id), "⋮", false);
                            egui::Popup::menu(&more).show(|ui| menu(app, ui, m));
                        });
                    });
                    if selected {
                        let plus = text_button(ui, &format!("maskPlus:{}", m.id), "+", false).on_hover_text("Edit mask components");
                        if app.ui.layer_components_open {
                            egui::Popup::open_id(ui.ctx(), egui::Popup::default_response_id(&plus));
                            app.ui.layer_components_open = false;
                        }
                        egui::Popup::menu(&plus).show(|ui| {
                            for op in ["add", "subtract", "intersect"] {
                                let response = ui.menu_button(
                                    match op {
                                        "add" => "Add",
                                        "subtract" => "Subtract",
                                        _ => "Intersect",
                                    },
                                    |ui| super::masking::component_menu(app, ui, op),
                                );
                                register(ui.ctx(), format!("layerOp:{op}"), response.response.rect);
                            }
                            if text_button(ui, "layerIntersectLuma", "Intersect with Luminance Range", false).clicked() {
                                let _ = app.run("mask.addComponent", json!({"kind":"luminanceRange","op":"intersect"}));
                                ui.close();
                            }
                            if text_button(ui, "layerEditMask", "Edit Mask…", false).clicked() {
                                app.ui.right = crate::state::RightPanel::Masking;
                                ui.close();
                            }
                        });
                        super::masking::describe_field(app, ui, false);
                    }
                });
            }
        }
        if let Some(mid) = app.ui.deleting_layer {
            ui.label("Delete this layer?");
            ui.horizontal(|ui| {
                if text_button(ui, "layerDeleteConfirm", "Delete", false).clicked() {
                    let _ = app.run("mask.delete", json!({"id":mid}));
                    app.ui.deleting_layer = None;
                }
                if text_button(ui, "layerDeleteCancel", "Cancel", false).clicked() {
                    app.ui.deleting_layer = None;
                }
            });
        }
        super::masking::describe_field(app, ui, true);
        if app.session.active_mask.is_some() {
            let m = d.masks.iter().find(|m| Some(m.id) == app.session.active_mask);
            if let Some(m) = m {
                let r = ui.checkbox(&mut app.ui.layer_used_only, "Only used tools");
                register(ui.ctx(), "layerUsedOnly", r.rect);
                r.on_hover_text("Show only tools this layer uses");
                let out =
                    crate::widgets::numeric_slider(ui, &OPACITY, m.opacity, true, None, crate::widgets::numeric_owner(app, ("layer-opacity", m.id)));
                super::edit::apply_slider_out(app, &OPACITY, out, |app, v| app.run("mask.setOpacity", json!({"id":m.id,"value":v})));
            }
            let overlay = text_button(ui, "maskOverlay", "Overlay", app.ui.mask_overlay);
            if overlay.clicked() {
                app.ui.mask_overlay = !app.ui.mask_overlay;
            }
            egui::Popup::menu(&overlay).show(|ui| super::masking::overlay_options(app, ui));
        }
        if ui.input(|i| i.pointer.any_released()) {
            app.ui.dragging_layer = None;
        }
    });
    register(ui.ctx(), "layerBar", frame.response.rect);
    if app.ui.tool == "brush" {
        super::masking::brush_settings(app, ui);
    }
}

pub fn canvas_pill(app: &LightcraftApp, ui: &mut egui::Ui, canvas: Rect) {
    if let Some(name) = app
        .session
        .active()
        .and_then(|id| app.session.develop_of(id))
        .and_then(|d| d.masks.iter().find(|m| Some(m.id) == app.session.active_mask).map(|m| m.name.clone()))
    {
        let t = Tokens::get(ui.ctx());
        let galley = ui.painter().layout_no_wrap(name, t.font(12.0), t.text);
        let r = Rect::from_min_size(canvas.min + vec2(12.0, 12.0), vec2((galley.size().x + 20.0).min(canvas.width() - 24.0), 26.0));
        ui.painter().rect_filled(r, 13.0, t.chrome);
        ui.painter().with_clip_rect(r.shrink(5.0)).text(
            pos2(r.left() + 10.0, r.center().y),
            Align2::LEFT_CENTER,
            galley.job.text.clone(),
            t.font(12.0),
            t.text,
        );
        register(ui.ctx(), "activeLayerPill", r);
    }
}

/// Layer-list navigation owns these keys while the list has focus.
pub fn keyboard(app: &mut LightcraftApp, ctx: &egui::Context) {
    if !app.ui.layer_bar_focus || app.ui.right != crate::state::RightPanel::Edit || app.ui.renaming_mask.is_some() || app.ui.dialog.is_some() {
        return;
    }
    if ctx.input(|i| i.pointer.any_pressed())
        && ctx.pointer_interact_pos().is_some_and(|p| !app.widgets.iter().any(|(id, r)| id == "layerBar" && r.contains(p)))
    {
        app.ui.layer_bar_focus = false;
        return;
    }
    let Some(d) = app.session.active().and_then(|id| app.session.develop_of(id)) else {
        return;
    };
    let current = d.masks.iter().position(|m| Some(m.id) == app.session.active_mask).map(|i| i as i32).unwrap_or(-1);
    let delta = ctx.input_mut(|i| {
        if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) {
            1
        } else if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) {
            -1
        } else {
            0
        }
    });
    if delta != 0 {
        let next = (current + delta).clamp(-1, d.masks.len() as i32 - 1);
        let _ = app.session.end_interaction();
        app.ui.tool.clear();
        app.ui.pending_mask_shape = None;
        app.ui.pending_mask_origin = None;
        app.session.active_mask = if next < 0 { None } else { Some(d.masks[next as usize].id) };
    }
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter))
        && let Some(m) = d.masks.iter().find(|m| Some(m.id) == app.session.active_mask)
    {
        app.ui.renaming_mask = Some((m.id, m.name.clone()));
    }
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Delete)) {
        app.ui.deleting_layer = app.session.active_mask;
    }
}
