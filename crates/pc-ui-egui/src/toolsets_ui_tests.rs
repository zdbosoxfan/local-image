//! Tool sets in the shell: the switcher, Window › Tool Set, and the Edit › Toolbar… dialog.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use photocraft_engine::toolsets::{ALL_TOOLS, ToolSet, builtins};
use serde_json::json;

use super::*;

fn harness() -> Harness<'static, PhotocraftApp> {
    Harness::builder().with_size(egui::vec2(1100.0, 760.0)).with_max_steps(64).build_eframe(|cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default())
    })
}

fn active(h: &Harness<'_, PhotocraftApp>) -> ToolSet {
    h.state().session.prefs().toolbar.active()
}

fn open_dialog(h: &mut Harness<'_, PhotocraftApp>) {
    let ctx = h.ctx.clone();
    crate::menus::invoke(h.state_mut(), &ctx, "edit.toolbar", json!({})).unwrap();
    h.run_steps(4);
}

fn shown_tools(app: &PhotocraftApp) -> Vec<Tool> {
    crate::panels::visible_sections(app).iter().flatten().flat_map(|(_, s)| s.clone()).collect()
}

#[test]
fn the_tool_list_matches_the_tool_enum_and_every_set_parses() {
    let names: Vec<String> = Tool::ALL.iter().map(|t| format!("{t:?}")).collect();
    let mut listed: Vec<&str> = ALL_TOOLS.to_vec();
    listed.sort_unstable();
    let mut known: Vec<&str> = names.iter().map(String::as_str).collect();
    known.sort_unstable();
    assert_eq!(listed, known, "the engine's tool list is the shell's Tool enum");
    for set in builtins() {
        for n in &set.tools {
            assert_eq!(Tool::from_name(n).map(|t| format!("{t:?}")).as_deref(), Some(n.as_str()), "{} / {n}", set.id);
        }
    }
}

#[test]
fn all_tools_is_the_default_and_the_switcher_changes_the_toolbar() {
    let mut h = harness();
    h.run_steps(2);
    assert_eq!(active(&h).id, "allTools");
    assert_eq!(shown_tools(h.state()).len(), Tool::ALL.len());
    h.get_by_label("Tool Set: All Tools").click();
    h.run_steps(3);
    h.get_by_label("Retouching").click();
    h.run_steps(3);
    assert_eq!(active(&h).id, "retouching");
    let kept = shown_tools(h.state());
    assert!(kept.contains(&Tool::Healing) && !kept.contains(&Tool::Pen), "{kept:?}");
    h.get_by_label("Tool Set: Retouching");
    // A tool the set leaves out still works by its key, and the toolbar shows it while active.
    h.state_mut().ui.tool = Tool::Pen;
    assert!(shown_tools(h.state()).contains(&Tool::Pen));
}

#[test]
fn create_rename_and_delete_a_custom_set_in_the_dialog() {
    let mut h = harness();
    open_dialog(&mut h);
    h.get_by_label("New").click();
    h.run_steps(3);
    let made = active(&h);
    assert_eq!(made.name, "My Tools");
    assert_eq!(made.tools.len(), Tool::ALL.len(), "a new set starts as a copy of the one in use");
    // untick a tool: it leaves the set
    h.get_all_by_label("Sponge Tool").last().unwrap().click();
    h.run_steps(3);
    assert!(!active(&h).tools.iter().any(|t| t == "Sponge"));
    h.get_by_label("Rename").click();
    h.run_steps(2);
    let edit = h.get_by_role(egui::accesskit::Role::TextInput);
    edit.focus();
    h.run_steps(1);
    h.get_by_role(egui::accesskit::Role::TextInput).type_text("Faces");
    h.run_steps(2);
    h.get_by_label("OK").click();
    h.run_steps(3);
    assert!(active(&h).name.contains("Faces"), "{}", active(&h).name);
    h.get_by_label("Delete").click();
    h.run_steps(3);
    assert_eq!(active(&h).id, "allTools");
    assert!(h.state().session.prefs().toolbar.sets.is_empty());
}

#[test]
fn built_in_sets_cannot_be_deleted_or_edited() {
    let mut h = harness();
    open_dialog(&mut h);
    h.get_by_label("Delete").click();
    h.get_by_label("Rename").click();
    h.run_steps(3);
    assert_eq!(active(&h).id, "allTools");
    for id in ["allTools", "photographer", "essentials", "retouching", "ai"] {
        assert!(h.state_mut().run("toolset.delete", json!({"id": id})).is_err(), "{id}");
    }
    // "Save as New Set" copies the built-in into a custom one
    h.get_by_label("Save as New Set").click();
    h.run_steps(3);
    assert!(active(&h).name.starts_with("All Tools copy"), "{}", active(&h).name);
}

#[test]
fn sets_round_trip_through_the_saved_prefs_and_old_toolbars_migrate() {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.run("toolset.save", json!({"name": "Mine", "tools": ["Move", "Brush"]})).unwrap();
    let saved = app.session.prefs_to_json();
    let mut other = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    other.session.load_prefs_json(&saved).unwrap();
    assert_eq!(other.session.prefs().toolbar.active().name, "Mine");
    let mut old = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    old.session.load_prefs_json(r#"{"toolbar": {"hidden": ["Sponge"]}}"#).unwrap();
    let a = old.session.prefs().toolbar.active();
    assert_eq!(a.name, "My Tools");
    assert!(!a.tools.iter().any(|t| t == "Sponge"));
    let kept = shown_tools(&old);
    assert!(!kept.contains(&Tool::Sponge) && kept.contains(&Tool::Dodge));
}

#[test]
fn window_menu_lists_the_sets_and_marks_the_active_one() {
    let app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    let items = crate::menus::menu_items(&app);
    let sets: Vec<&MenuItem> = items.iter().filter(|i| i.path == ["Window", "Tool Set"] && i.id.starts_with("window.toolSet.")).collect();
    assert_eq!(sets.len(), 5);
    assert_eq!(sets.iter().filter(|i| i.checked == Some(true)).map(|i| i.id.as_str()).collect::<Vec<_>>(), ["window.toolSet.allTools"]);
}
