//! Headless shell contract: persona tables feed unique UI tool identities and handlers.
use photocraft_engine::toolsets::{self, Persona};
use std::collections::BTreeSet;
#[test]
fn ui_tool_registry_ids_resolve_once_in_each_persona() {
    for persona in [Persona::Pixel, Persona::Vector] {
        let mut seen = BTreeSet::new();
        for id in toolsets::tools_for(persona) {
            assert!(seen.insert(id.to_ascii_lowercase()), "{persona:?}: duplicate UI id {id}");
            assert!(photocraft_ui_egui::shared_tools::handler(id, persona).is_some(), "{persona:?}: missing shell handler {id}");
        }
    }
}
