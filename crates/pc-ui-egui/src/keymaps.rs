//! The editor's effective shortcuts: the engine's preferences over li-keys' profile tables.
use li_keys::keymaps::is_global;
use photocraft_engine::prefs::Preferences;
pub fn default<'a>(prefs: &Preferences, id: &str, base: Option<&'a str>) -> Option<&'a str> {
    li_keys::keymaps::editor_default(prefs.shortcut_profiles.editor.parent(), id, crate::hold_keys::is_temporary(id), base)
}
pub fn effective<'a>(prefs: &'a Preferences, id: &str, base: Option<&'a str>) -> Option<&'a str> {
    let chord = prefs.shortcut(id, default(prefs, id, base))?;
    if is_global(id) || global_conflict(chord).is_none() { Some(chord) } else { None }
}
pub fn global_conflict(chord: &str) -> Option<&'static str> {
    li_keys::global_conflict(chord).map(|(_, label)| label)
}
