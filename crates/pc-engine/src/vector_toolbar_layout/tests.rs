#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use std::collections::BTreeSet;
#[test]
fn toolbar46_presets_are_unique_and_switch_actual_lookup() {
    for preset in [Preset::Illustrator, Preset::Affinity] {
        let mut keys = BTreeSet::new();
        for b in tool_bindings(preset).iter().chain(PROXY_KEYS).chain(ACCELERATORS) {
            assert!(keys.insert(b.key), "duplicate {}", b.key);
        }
    }
    let registered = ["Scale", "ShapeBuilder", "VectorMagicWand", "Transparency"];
    assert_eq!(tool_for_key(Persona::Vector, Preset::Illustrator, "S", &registered), Some("Scale"));
    assert_eq!(tool_for_key(Persona::Vector, Preset::Affinity, "S", &registered), Some("ShapeBuilder"));
    assert_eq!(tool_for_key(Persona::Vector, Preset::Illustrator, "Y", &registered), Some("VectorMagicWand"));
    assert_eq!(tool_for_key(Persona::Vector, Preset::Affinity, "Y", &registered), Some("Transparency"));
    assert_eq!(tool_for_key(Persona::Pixel, Preset::Illustrator, "S", &registered), None);
    assert_eq!(tool_for_key(Persona::Vector, Preset::Illustrator, "P", &registered), None);
    assert_eq!(tool_for_key_after(Persona::Vector, Preset::Affinity, "M", &["Rectangle", "EllipseShape"], "Rectangle"), Some("EllipseShape"));
    assert_eq!(tool_for_key_after(Persona::Vector, Preset::Affinity, "M", &["Rectangle", "EllipseShape"], "EllipseShape"), Some("Rectangle"));
    assert_eq!(serde_json::from_str::<Settings>("{}").unwrap(), Settings::default());
    assert_eq!(serde_json::from_str::<Settings>(r#"{"preset":"affinity"}"#).unwrap().preset, Preset::Affinity);
}
#[test]
fn toolbar46_registered_tools_appear_once_with_stable_flyout_order() {
    let mut registered: Vec<_> = crate::toolsets::VECTOR_TOOLS.iter().map(|t| t.id).collect();
    registered.extend(["Curvature", "GroupSelection", "VectorLasso", "Scale", "FutureTool", "Pen"]);
    let g = groups(&registered, None);
    let actual: Vec<_> = g.iter().flat_map(|g| g.tools.iter().copied()).collect();
    let expected: BTreeSet<_> = registered.iter().copied().collect();
    assert_eq!(actual.len(), expected.len());
    assert_eq!(actual.iter().copied().collect::<BTreeSet<_>>(), expected);
    assert_eq!(g.iter().find(|g| g.slot == 1004).unwrap().tools, ["Pen", "Curvature", "AddAnchor", "DeleteAnchor", "AnchorPoint"]);
    assert_eq!(groups(&["Curvature"], None)[0].tools, ["Curvature"]);
    for set in builtins(&registered) {
        assert!(set.tools.iter().all(|id| registered.contains(&id.as_str())));
        assert_eq!(set.tools.len(), set.tools.iter().collect::<BTreeSet<_>>().len());
        let shown: Vec<_> = groups(&registered, Some(&set)).into_iter().flat_map(|g| g.tools).collect();
        assert_eq!(shown.iter().copied().collect::<BTreeSet<_>>(), set.tools.iter().map(String::as_str).collect());
    }
    let essentials = builtins(&registered).into_iter().find(|s| s.id == "vector.essentials").unwrap();
    assert!(essentials.tools.contains(&"Curvature".into()));
    assert!(!essentials.tools.contains(&"Scale".into()));
}
#[test]
fn toolbar46_pixel_accelerator_snapshot_and_vector_channel_conflicts() {
    let pixel: Vec<(String, String)> = serde_json::from_str(include_str!("pixel-accelerators.json")).unwrap();
    assert_eq!(command_bindings(Persona::Pixel, &pixel), pixel);
    let vector = command_bindings(Persona::Vector, &pixel);
    assert_eq!(vector.iter().filter(|(_, key)| key == "Cmd+D").map(|(id, _)| id.as_str()).collect::<Vec<_>>(), ["vector.transform.again"]);
    for id in [
        "select.deselect",
        "li.openFolder",
        "channel.target.composite",
        "channel.target.slot3",
        "channel.target.slot7",
        "channel.target.slot8",
        "tools.defaultColors",
        "tools.swapColors",
    ] {
        assert!(!vector.iter().any(|(actual, _)| actual == id), "left conflicting owner {id}");
    }
    assert!(vector.contains(&("edit.copy".into(), "Cmd+C".into())));
    // The registered raster table is independent of the layout's override table.
    for (id, key) in &pixel {
        if let Some(spec) = crate::commands::find(id)
            && let Some(sc) = spec.shortcut
        {
            assert_eq!(sc, key, "Pixel {id} changed");
        }
    }
}

#[test]
fn toolbar46_frozen_pixel_tool_keys_and_query_no_journal() {
    let expected: serde_json::Value = serde_json::from_str(include_str!("pixel-tool-keys.json")).unwrap();
    let actual: Vec<_> = crate::prefs::TOOL_SHORTCUTS.iter().map(|(id, _, key)| (*id, *key)).collect();
    assert_eq!(serde_json::json!(actual), expected);
    let mut s = crate::Session::new();
    let before = s.prefs_to_json();
    let spec = specs().remove(0);
    assert!(!spec.journal);
    let result = (spec.run)(&mut s, &serde_json::json!({"preset":"affinity"})).unwrap();
    assert_eq!(result["tools"].as_array().unwrap().iter().find(|v| v["key"] == "S").unwrap()["tool"], "ShapeBuilder");
    assert_eq!(s.prefs_to_json(), before);
}

#[test]
fn review_followup_vector_accelerators_preserve_custom_nonconflicting_chords() {
    let pixel = vec![("object.pasteBack".into(), "Shift+Cmd+V".into())];
    assert!(command_bindings(Persona::Vector, &pixel).contains(&pixel[0]));
}
