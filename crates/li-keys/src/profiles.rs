use crate::normalize_shortcut;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Immutable application bindings. Undo/redo route to the focused workspace.
pub const GLOBAL_SHORTCUTS: &[(&str, &str, &str)] = &[
    ("file.exit", "Quit", "Cmd+Q"),
    ("edit.preferences.general", "Preferences", "Cmd+K"),
    ("edit.undo", "Undo", "Cmd+Z"),
    ("edit.redo", "Redo", "Cmd+Shift+Z"),
    ("view.screenMode.cycle", "Screen mode", "F"),
    ("global.fullscreen", "Full screen", "Cmd+Shift+F"),
    ("host.library", "Library workspace", "Cmd+Alt+1"),
    ("host.develop", "Develop workspace", "Cmd+Alt+2"),
    ("host.editor", "Editor workspace", "Cmd+Alt+3"),
];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShortcutWorkspace {
    pub active: String,
    pub custom: BTreeMap<String, ShortcutProfile>,
}
impl Default for ShortcutWorkspace {
    fn default() -> Self {
        Self { active: "Photoshop".into(), custom: BTreeMap::new() }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShortcutProfile {
    pub parent: String,
    pub overrides: BTreeMap<String, String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShortcutProfiles {
    pub library: ShortcutWorkspace,
    pub editor: ShortcutWorkspace,
}
impl Default for ShortcutProfiles {
    fn default() -> Self {
        Self { library: ShortcutWorkspace { active: "Lightroom".into(), ..Default::default() }, editor: ShortcutWorkspace::default() }
    }
}
impl ShortcutWorkspace {
    pub fn parent(&self) -> &str {
        self.custom.get(&self.active).map_or(self.active.as_str(), |p| p.parent.as_str())
    }
    pub fn duplicate(&mut self, name: &str) -> std::result::Result<(), String> {
        if name.trim().is_empty() || self.custom.contains_key(name) || ["Lightroom", "Capture One", "Photoshop", "Affinity Photo"].contains(&name) {
            return Err("Choose a unique profile name".into());
        }
        let profile =
            ShortcutProfile { parent: self.parent().into(), overrides: self.custom.get(&self.active).map(|p| p.overrides.clone()).unwrap_or_default() };
        self.custom.insert(name.into(), profile);
        self.active = name.into();
        Ok(())
    }
    pub fn rename(&mut self, name: &str) -> std::result::Result<(), String> {
        if name == self.active {
            return Ok(());
        }
        if name.trim().is_empty() || self.custom.contains_key(name) || ["Lightroom", "Capture One", "Photoshop", "Affinity Photo"].contains(&name) {
            return Err("Choose a unique profile name".into());
        }
        let profile = self.custom.remove(&self.active).ok_or("Duplicate a built-in before editing it")?;
        self.custom.insert(name.into(), profile);
        self.active = name.into();
        Ok(())
    }
    /// The active profile, ready to edit. With a built-in active it first becomes a copy,
    /// "Custom (<built-in>)", like Photoshop's "Photoshop Defaults (modified)".
    pub fn editable(&mut self) -> &mut ShortcutProfile {
        if !self.custom.contains_key(&self.active) {
            let mut name = format!("Custom ({})", self.active);
            while self.custom.contains_key(&name) {
                name.push('2');
            }
            let parent = self.active.clone();
            self.custom.insert(name.clone(), ShortcutProfile { parent, overrides: BTreeMap::new() });
            self.active = name;
        }
        let parent = self.parent().to_string();
        self.custom.entry(self.active.clone()).or_insert_with(|| ShortcutProfile { parent, overrides: BTreeMap::new() })
    }
    pub fn delete_active(&mut self) {
        if let Some(profile) = self.custom.remove(&self.active) {
            self.active = profile.parent;
        }
    }
}
impl ShortcutProfiles {
    pub fn workspace(&self, library: bool) -> &ShortcutWorkspace {
        if library { &self.library } else { &self.editor }
    }
    pub fn workspace_mut(&mut self, library: bool) -> &mut ShortcutWorkspace {
        if library { &mut self.library } else { &mut self.editor }
    }
    /// The active profile's own bindings (empty while a built-in is active).
    pub fn overrides(&self, library: bool) -> &BTreeMap<String, String> {
        static EMPTY: std::sync::LazyLock<BTreeMap<String, String>> = std::sync::LazyLock::new(BTreeMap::new);
        let workspace = self.workspace(library);
        workspace.custom.get(&workspace.active).map_or(&*EMPTY, |profile| &profile.overrides)
    }
    /// Bind `id` in the active profile of its workspace (`library::` ids belong to Library):
    /// `Some("")` disables the default, `None` returns to it. Binding while a built-in is
    /// active first copies it ([`ShortcutWorkspace::editable`]).
    pub fn set_override(&mut self, id: &str, chord: Option<&str>) {
        let workspace = self.workspace_mut(id.starts_with("library::"));
        match chord {
            Some(chord) => {
                workspace.editable().overrides.insert(id.into(), chord.into());
            }
            None => {
                if let Some(profile) = workspace.custom.get_mut(&workspace.active) {
                    profile.overrides.remove(id);
                }
            }
        }
    }
    /// Remove every binding of the active profiles (built-ins have none).
    pub fn clear_overrides(&mut self) {
        for workspace in [&mut self.library, &mut self.editor] {
            if let Some(profile) = workspace.custom.get_mut(&workspace.active) {
                profile.overrides.clear();
            }
        }
    }
    pub fn validate(&self, previous: &Self) -> std::result::Result<(), String> {
        for library in [false, true] {
            let workspace = self.workspace(library);
            let builtins = if library { ["Lightroom", "Capture One"] } else { ["Photoshop", "Affinity Photo"] };
            if !builtins.contains(&workspace.active.as_str()) && !workspace.custom.contains_key(&workspace.active) {
                return Err("Unknown active shortcut profile".into());
            }
            for (name, profile) in &workspace.custom {
                if name.trim().is_empty() || builtins.contains(&name.as_str()) || !builtins.contains(&profile.parent.as_str()) {
                    return Err("Invalid shortcut profile name or parent".into());
                }
                for (id, chord) in &profile.overrides {
                    if id.is_empty() || id.starts_with("library::") != library {
                        return Err("Shortcut belongs to another workspace".into());
                    }
                    let old = previous.workspace(library).custom.get(name).and_then(|p| p.overrides.get(id));
                    if !chord.is_empty() && normalize_shortcut(chord).is_none() && old != Some(chord) {
                        return Err(format!("`{chord}` is not a valid shortcut"));
                    }
                }
            }
        }
        Ok(())
    }
}
