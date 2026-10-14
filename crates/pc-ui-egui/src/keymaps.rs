//! Compatibility adapters: preferences stay in the engine; profile policy lives in li-keys.
pub use li_keys::keymaps::{AFFINITY, is_global};
use photocraft_engine::prefs::Preferences;
/// Preserve the old cross-crate module path without compiling another crate's source.
pub mod library {
    pub use li_keys::keymaps::{CAPTURE_ONE, GLOBAL_CHORDS, library_default as default};
}
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
