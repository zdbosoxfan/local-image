//! Inspect raw registries before any UI layout deduplication can conceal a collision.
use photocraft_engine::{command_specs, prefs, toolsets, vector_toolbar_layout as layout};
use std::collections::BTreeMap;
use toolsets::Persona;
fn unique<'a>(items: impl IntoIterator<Item = &'a str>, context: &str) {
    let mut counts = BTreeMap::new();
    for item in items {
        *counts.entry(item.to_ascii_lowercase()).or_insert(0) += 1;
    }
    let duplicates: Vec<_> = counts.into_iter().filter(|(_, n)| *n > 1).collect();
    assert!(duplicates.is_empty(), "{context}: duplicate registrations {duplicates:?}");
}
#[test]
fn engine_command_ids_are_unique_before_dispatch() {
    let specs = command_specs();
    assert!(specs.len() > 100);
    unique(specs.iter().map(|s| s.id), "command_specs");
}
#[test]
fn pixel_tool_ids_and_each_builtin_layout_are_unique() {
    check_tools(Persona::Pixel);
}
#[test]
fn vector_tool_ids_and_each_builtin_layout_are_unique() {
    check_tools(Persona::Vector);
}
fn check_tools(persona: Persona) {
    let tools = toolsets::tools_for(persona);
    assert!(!tools.is_empty());
    unique(tools.iter().copied(), &format!("{persona:?}"));
    let sets = toolsets::builtins_for(persona);
    unique(sets.iter().map(|s| s.id.as_str()), "tool-set ids");
    for set in sets {
        unique(set.tools.iter().map(String::as_str), &set.id);
        for tool in &set.tools {
            assert!(toolsets::supports(tool, persona), "{} references unsupported {tool}", set.id);
        }
    }
}
#[test]
fn pixel_tool_shortcuts_have_one_owner_per_chord() {
    unique(prefs::TOOL_SHORTCUTS.iter().map(|(id, _, _)| *id).chain(prefs::TEMPORARY_TOOLS.iter().map(|(id, _, _)| *id)), "Pixel shortcut ids");
    unique(prefs::TOOL_SHORTCUTS.iter().filter_map(|(_, _, key)| *key).chain(prefs::TEMPORARY_TOOLS.iter().map(|(_, _, key)| *key)), "Pixel tool keys");
    for (id, _, _) in prefs::TOOL_SHORTCUTS {
        assert!(toolsets::supports(id.strip_prefix("tools.select.").unwrap(), Persona::Pixel), "{id}");
    }
}
#[test]
fn vector_illustrator_tool_shortcuts_have_one_owner_per_chord() {
    check_keys(layout::Preset::Illustrator);
}
#[test]
fn vector_affinity_tool_shortcuts_have_one_owner_per_chord() {
    check_keys(layout::Preset::Affinity);
}
fn check_keys(preset: layout::Preset) {
    let keys = layout::tool_bindings(preset);
    assert!(!keys.is_empty());
    unique(keys.iter().map(|b| b.key), "Vector tool keys");
    // The raw table includes future tools, so availability is checked by the actual dispatcher.
    let tools = toolsets::tools_for(Persona::Vector);
    for b in keys {
        if tools.contains(&b.target) {
            assert_eq!(layout::tool_for_key(Persona::Vector, preset, b.key, &tools), Some(b.target));
        }
    }
    unique(layout::ACCELERATORS.iter().chain(layout::PROXY_KEYS).map(|b| b.key), "Vector accelerator/proxy keys");
}
