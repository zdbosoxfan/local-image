//! Tool sets in the shell: the switcher at the top of the toolbar, Window › Tool Set, and the
//! Toolbar tab of Edit › Toolbar… (list of sets with New / Duplicate / Rename / Delete / Reset,
//! and a checkbox per tool for the selected set). The sets and their rules live in the engine
//! (`photocraft_engine::toolsets`, commands `toolset.*`); everything here goes through those
//! commands, so the dialog, the menu and automation stay in step.
//!
//! A set decides which tools the toolbar shows; a tool it leaves out still works by its shortcut,
//! and while it is the active tool the toolbar shows it in its usual place (see
//! [`crate::panels::visible_sections`]).

use egui::RichText;
use photocraft_engine::prefs::ToolbarCustomization;
use photocraft_engine::toolsets::ToolSet;
use serde_json::{Map, Value, json};

use crate::PhotocraftApp;
use crate::state::Tool;
use crate::theme::Tokens;
use li_commands::menu::MenuItem;

#[path = "vector_packages/persona.rs"]
pub mod persona;

/// The prefix of the Window › Tool Set item ids (`window.toolSet.<set id>`).
const MENU_PREFIX: &str = "window.toolSet.";

fn ordered_label(name: &str) -> &'static str {
    Tool::from_name(name)
        .map(Tool::label)
        .or_else(|| photocraft_engine::toolsets::VECTOR_TOOLS.iter().find(|t| t.id == name).map(|t| t.label))
        .unwrap_or("Tool")
}

/// A set's name in the UI language: the built-ins are translated, the user's own are theirs.
pub fn display_name(set: &ToolSet) -> String {
    if !ToolbarCustomization::is_builtin(&set.id) {
        return set.name.clone();
    }
    match set.id.as_str() {
        "allTools" | "vector.allTools" => tl!("All Tools"),
        "photographer" | "vector.photographer" => tl!("Photographer"),
        "essentials" | "vector.essentials" => tl!("Essentials"),
        "retouching" => tl!("Retouching"),
        "ai" => tl!("AI"),
        "vector.illustration" => tl!("Illustration"),
        "vector.layout" => tl!("Layout & Type"),
        _ => set.name.as_str(),
    }
    .to_string()
}

fn sets(app: &PhotocraftApp) -> (String, Vec<ToolSet>) {
    let c = &app.session.prefs().toolbar;
    (c.active().id, c.sets_for(c.persona))
}

/// The switcher button at the top of the toolbar: a menu of the sets and a way to edit them.
pub fn switcher(app: &mut PhotocraftApp, ui: &mut egui::Ui, size: f32) {
    let (active_id, all) = sets(app);
    let active = all.iter().find(|s| s.id == active_id).cloned().unwrap_or_default();
    let resp = ui.add_sized([ui.available_width().max(size), size], egui::Button::new(RichText::new(display_name(&active)).size(9.5)).truncate());
    let tip = format!("{}: {}", tl!("Tool Set"), display_name(&active));
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &tip));
    let resp = resp.on_hover_text(tip);
    let ctx = ui.ctx().clone();
    egui::Popup::menu(&resp).show(|ui| {
        ui.set_min_width(200.0);
        for set in &all {
            if ui.add(egui::Button::selectable(set.id == active_id, display_name(set))).clicked() {
                let _ = app.run("toolset.select", json!({"id": set.id}));
                ui.close();
            }
        }
        ui.separator();
        if ui.button(tl!("Edit Toolbar…")).clicked() {
            let _ = crate::menus::invoke(app, &ctx, "edit.toolbar", json!({}));
            ui.close();
        }
    });
}

/// Window › Tool Set › <each set>, then Edit Toolbar…: inserted after Window › Tools.
pub fn menu_items(app: &PhotocraftApp, items: &mut Vec<MenuItem>) {
    let active_persona = app.session.prefs().toolbar.persona;
    let mut personas = Vec::new();
    for (id, label, shortcut, family) in [
        ("view.persona.pixel", tl!("Pixel persona"), "Cmd+Shift+1", photocraft_engine::toolsets::Persona::Pixel),
        ("view.persona.vector", tl!("Vector persona"), "Cmd+Shift+2", photocraft_engine::toolsets::Persona::Vector),
    ] {
        personas.push(MenuItem {
            id: id.into(),
            label: label.into(),
            path: vec!["View".into(), "Persona".into()],
            shortcut: app.session.prefs().shortcut(id, Some(shortcut)).map(str::to_string),
            enabled: true,
            checked: Some(family == active_persona),
            color: None,
            tooltip: None,
        });
    }
    insert_section(items, "View", personas);
    let mut geometry = Vec::new();
    for op in persona::Geometry::ALL {
        let id = format!("layer.geometry.{}", op.op());
        let enabled = persona::GeometryRequest::capture(app, op, false).is_ok_and(|r| app.session.is_enabled_with(op.command(), &r.params()));
        geometry.push(MenuItem {
            id,
            label: tl!(op.label()).into(),
            path: vec!["Layer".into(), "Geometry".into()],
            shortcut: None,
            enabled,
            checked: None,
            color: None,
            tooltip: None,
        });
    }
    insert_section(items, "Layer", geometry);
    let Some(after) =
        items.iter().position(|i| i.id == "window.toggle.toolbar").or_else(|| items.iter().rposition(|i| i.path.first().is_some_and(|p| p == "Window")))
    else {
        return;
    };
    let (active, all) = sets(app);
    let path = vec!["Window".to_string(), "Tool Set".to_string()];
    let mut add: Vec<MenuItem> = all
        .iter()
        .map(|s| MenuItem {
            id: format!("{MENU_PREFIX}{}", s.id),
            label: display_name(s),
            path: path.clone(),
            shortcut: None,
            enabled: true,
            checked: Some(s.id == active),
            tooltip: None,
            color: None,
        })
        .collect();
    add.push(MenuItem { id: "---".into(), label: "---".into(), path: path.clone(), shortcut: None, enabled: false, checked: None, tooltip: None, color: None });
    add.push(MenuItem {
        id: "edit.toolbar".into(),
        label: tl!("Edit Toolbar…").to_string(),
        path,
        shortcut: None,
        enabled: true,
        checked: None,
        tooltip: None,
        color: None,
    });
    // The Edit menu has its own Toolbar… entry; this one only repeats it under Tool Set.
    for (k, it) in add.into_iter().enumerate() {
        items.insert(after + 1 + k, it);
    }
}

/// The shortcut editor uses contiguous top-level sections from the menu order.
fn insert_section(items: &mut Vec<MenuItem>, section: &str, add: Vec<MenuItem>) {
    let at = items.iter().rposition(|item| item.path.first().is_some_and(|p| p == section)).map_or(items.len(), |i| i + 1);
    items.splice(at..at, add);
}

pub fn handles(id: &str) -> bool {
    id.starts_with(MENU_PREFIX) || matches!(id, "view.persona.pixel" | "view.persona.vector") || id.starts_with("layer.geometry.")
}

/// Run a Window › Tool Set item.
pub fn run(app: &mut PhotocraftApp, id: &str) -> Option<Result<Value, String>> {
    if let Some(family) = id.strip_prefix("view.persona.") {
        return Some(app.run("persona.select", json!({"persona": family})));
    }
    if let Some(key) = id.strip_prefix("layer.geometry.") {
        let op = persona::Geometry::ALL.into_iter().find(|op| op.op() == key)?;
        return Some(persona::GeometryRequest::capture(app, op, false).and_then(|r| r.execute(app)));
    }
    let set = id.strip_prefix(MENU_PREFIX)?;
    Some(app.run("toolset.select", json!({"id": set})))
}

pub fn checked(app: &PhotocraftApp, id: &str) -> Option<bool> {
    if let Some(family) = id.strip_prefix("view.persona.") {
        return Some(match family {
            "pixel" => app.session.prefs().toolbar.persona == photocraft_engine::toolsets::Persona::Pixel,
            "vector" => app.session.prefs().toolbar.persona == photocraft_engine::toolsets::Persona::Vector,
            _ => return None,
        });
    }
    let set = id.strip_prefix(MENU_PREFIX)?;
    Some(app.session.prefs().toolbar.active().id == set)
}

/// The Toolbar tab of Edit › Toolbar…. `f` is the dialog's field map (the rename box).
pub fn tab(app: &mut PhotocraftApp, ui: &mut egui::Ui, f: &mut Map<String, Value>) {
    persona::switcher(app, ui);
    let t = Tokens::get(ui.ctx());
    let (active_id, all) = sets(app);
    let active = all.iter().find(|s| s.id == active_id).cloned().unwrap_or_default();
    let builtin = ToolbarCustomization::is_builtin(&active.id);
    let mut renaming = f.get("tsRenaming").and_then(Value::as_bool).unwrap_or(false);
    let mut rename_to = f.get("tsRename").and_then(Value::as_str).unwrap_or("").to_string();
    if f.get("tsRenameOwner").and_then(Value::as_str) != Some(active.id.as_str()) {
        renaming = false;
        rename_to.clear();
        f.insert("tsRenameOwner".into(), json!(active.id));
    }
    let mut error = f.get("tsError").and_then(Value::as_str).unwrap_or("").to_string();
    let mut act = |app: &mut PhotocraftApp, cmd: &str, p: Value| {
        error = app.run(cmd, p).err().unwrap_or_default();
        error.is_empty()
    };
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(200.0);
            ui.label(RichText::new(tl!("Tool Sets")).color(t.text_dim));
            egui::ScrollArea::vertical().max_height(300.0).id_salt("toolsets-list").show(ui, |ui| {
                for set in &all {
                    let name =
                        if ToolbarCustomization::is_builtin(&set.id) { format!("{} ({})", display_name(set), tl!("built-in")) } else { display_name(set) };
                    if ui.add(egui::Button::selectable(set.id == active.id, name)).clicked() && set.id != active.id {
                        act(app, "toolset.select", json!({"id": set.id}));
                        renaming = false;
                        rename_to.clear();
                    }
                }
            });
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                if ui.button(tl!("New")).clicked() {
                    // A new set starts as a copy of the one in use.
                    let name = app.session.prefs().toolbar.unique_name(tl!("My Tools"));
                    act(app, "toolset.save", json!({"from": active.id, "name": name}));
                    renaming = false;
                }
                if ui.button(tl!("Duplicate")).clicked() {
                    act(app, "toolset.duplicate", json!({"id": active.id}));
                    renaming = false;
                }
                if ui.add_enabled(!builtin, egui::Button::new(tl!("Rename"))).clicked() {
                    renaming = true;
                    rename_to = active.name.clone();
                }
                if ui.add_enabled(!builtin, egui::Button::new(tl!("Delete"))).clicked() {
                    act(app, "toolset.delete", json!({"id": active.id}));
                    renaming = false;
                }
            });
            if renaming && !builtin {
                ui.horizontal(|ui| {
                    let label = ui.label(tl!("Tool Set Name"));
                    ui.add(egui::TextEdit::singleline(&mut rename_to).desired_width(110.0).id_salt("toolset-rename")).labelled_by(label.id);
                    if ui.button(tl!("Save Name")).clicked() && act(app, "toolset.rename", json!({"id": active.id, "newName": rename_to})) {
                        renaming = false;
                    }
                    if ui.button(tl!("Cancel Rename")).clicked() {
                        renaming = false;
                    }
                });
            }
            if ui.button(tl!("Reset to All Tools")).clicked() {
                act(app, "toolset.reset", json!({}));
                renaming = false;
            }
        });
        ui.add_space(16.0);
        ui.vertical(|ui| {
            if builtin {
                ui.label(RichText::new(tl!("Built-in sets can't be changed. Save a copy to choose your own tools.")).color(t.text_dim));
                if ui.button(tl!("Save as New Set")).clicked() {
                    act(app, "toolset.duplicate", json!({"id": active.id}));
                }
            } else {
                ui.label(RichText::new(tl!("Tick the tools this set shows.")).color(t.text_dim));
            }
            egui::ScrollArea::vertical().max_height(340.0).id_salt("toolbar-scroll").show(ui, |ui| {
                egui::Grid::new("toolbar-grid").num_columns(3).spacing([16.0, 4.0]).show(ui, |ui| {
                    let mut tools: Vec<String> = active.tools.clone();
                    let mut changed = false;
                    let mut ordered: Vec<(String, &'static str)> = if active.persona == photocraft_engine::toolsets::Persona::Vector {
                        photocraft_engine::toolsets::VECTOR_TOOLS.iter().map(|tool| (tool.id.to_string(), tool.label)).collect()
                    } else {
                        Tool::ALL
                            .iter()
                            .filter(|tool| photocraft_engine::toolsets::supports(&format!("{tool:?}"), active.persona))
                            .map(|tool| (format!("{tool:?}"), tool.label()))
                            .collect()
                    };
                    ordered.sort_by_key(|(name, _)| active.tools.iter().position(|n| n == name).unwrap_or(usize::MAX));
                    let mut reorder = None;
                    for (name, label) in ordered {
                        let mut on = tools.contains(&name);
                        let was = on;
                        ui.add_enabled_ui(!builtin, |ui| crate::widgets::checkbox(ui, &mut on, tl!(label)));
                        if on != was && !builtin {
                            if on {
                                tools.push(name.clone());
                            } else {
                                tools.retain(|x| *x != name);
                            }
                            changed = true;
                        }
                        ui.label(Tool::from_name(&name).and_then(|tool| crate::tool_tips::shortcut(&app.session, tool)).unwrap_or_default());
                        ui.horizontal(|ui| {
                            let index = tools.iter().position(|n| *n == name);
                            for (delta, label) in [(-1isize, tl!("Up")), (1isize, tl!("Down"))] {
                                let enabled = !builtin && index.is_some_and(|i| (i as isize + delta) >= 0 && (i as isize + delta) < tools.len() as isize);
                                let response = ui.add_enabled(enabled, egui::Button::new(label).small());
                                response.widget_info(|| {
                                    egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, format!("{}: {}", tl!(ordered_label(&name)), label))
                                });
                                if response.clicked() {
                                    reorder = index.map(|i| (i, (i as isize + delta) as usize));
                                }
                            }
                        });
                        ui.end_row();
                    }
                    if let Some((a, b)) = reorder {
                        tools.swap(a, b);
                        changed = true;
                    }
                    if changed {
                        act(app, "toolset.save", json!({"id": active.id, "tools": tools}));
                    }
                });
            });
        });
    });
    if !error.is_empty() {
        ui.colored_label(t.danger, &error);
    }
    f.insert("tsRenaming".into(), json!(renaming));
    f.insert("tsRename".into(), json!(rename_to));
    f.insert("tsError".into(), json!(error));
}

#[cfg(test)]
#[path = "toolsets_ui_tests.rs"]
mod tests;
