//! Photoshop uses the established command/menu/tool tables, sourced from Adobe's
//! published shortcuts: https://helpx.adobe.com/pdf/cs6/photoshop_reference.pdf
//! Current Adobe list: https://helpx.adobe.com/photoshop/web/get-set-up/preferences-and-settings/keyboard-shortcuts.html
//! Affinity is deliberately a supported subset: https://affinity.help/photo2/en-US.lproj/pages/Workspace/shortcuts.html
//! R remains Rotate View and AI Remove remains in J, as explicitly settled by the owner.
use photocraft_engine::prefs::Preferences;
#[path = "../../lc-ui-egui/src/keymaps.rs"]
pub mod library;
/// Faithful Affinity tool/file equivalents; unsupported Photoshop bindings are unassigned.
pub const AFFINITY: &[(&str, &str)] = &[
    ("file.new", "Cmd+N"),
    ("file.open", "Cmd+O"),
    ("file.save", "Cmd+S"),
    ("file.saveAs", "Cmd+Shift+S"),
    ("file.close", "Cmd+W"),
    ("file.print", "Cmd+P"),
    ("tools.select.Move", "V"),
    ("tools.select.Hand", "H"),
    ("tools.select.Eyedropper", "I"),
    ("tools.select.Crop", "C"),
    ("tools.select.MagicWand", "W"),
    ("tools.select.RectMarquee", "M"),
    ("tools.select.Lasso", "L"),
    ("tools.select.Gradient", "G"),
    ("tools.select.Brush", "B"),
    ("tools.select.Eraser", "E"),
    ("tools.select.Dodge", "O"),
    ("tools.select.CloneStamp", "K"),
    ("tools.select.SpotHealing", "J"),
    ("tools.select.Pen", "P"),
    ("tools.select.Zoom", "Z"),
    ("tools.select.Rectangle", "U"),
    ("tools.select.Type", "T"),
    ("tools.select.RotateView", "R"),
    ("edit.copy", "Cmd+C"),
    ("edit.cut", "Cmd+X"),
    ("edit.paste", "Cmd+V"),
    ("select.all", "Cmd+A"),
    ("select.deselect", "Cmd+D"),
];
pub fn default<'a>(prefs: &Preferences, id: &str, base: Option<&'a str>) -> Option<&'a str> {
    if prefs.shortcut_profiles.editor.parent() == "Affinity Photo" {
        // Shared hold keys retain their lifecycle and navigation semantics.
        if crate::hold_keys::is_temporary(id) {
            return base;
        }
        AFFINITY.iter().find(|(command, _)| *command == id).map(|(_, key)| *key)
    } else {
        base
    }
}
pub fn effective<'a>(prefs: &'a Preferences, id: &str, base: Option<&'a str>) -> Option<&'a str> {
    let chord = prefs.shortcut(id, default(prefs, id, base))?;
    if is_global(id) || global_conflict(chord).is_none() { Some(chord) } else { None }
}
pub fn is_global(id: &str) -> bool {
    let id = id.strip_prefix("library::").unwrap_or(id);
    photocraft_engine::prefs::GLOBAL_SHORTCUTS.iter().any(|(command, _, _)| *command == id)
        || matches!(id, "app.quit" | "view.enterFullScreen" | "view.fullScreenPreview" | "view.screenMode.cycle")
}
pub fn global_conflict(chord: &str) -> Option<&'static str> {
    let sc = crate::shortcuts::parse(chord)?;
    photocraft_engine::prefs::GLOBAL_SHORTCUTS
        .iter()
        .find(|(_, _, key)| crate::shortcuts::parse(key).is_some_and(|global| crate::shortcuts::key_matches(&global, sc.logical_key, sc.modifiers)))
        .map(|(_, label, _)| *label)
}
