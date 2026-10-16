//! Supported Lightroom/Capture One bindings. Existing Lightroom command/alias catalogs
//! remain the base table; this is the sourced profile delta, not a parallel registry.
//! Adobe: https://helpx.adobe.com/lightroom-classic/desktop/introduction-to-lightroom-classic/keyboard-shortcuts.html
//! Capture One: https://www.captureone.com/blog/discover-the-shortcuts-of-the-adjustments-layers-tool
//! Current L gradient: https://support.captureone.com/hc/en-us/articles/360003399778
//! Radial T: https://support.captureone.com/hc/en-us/articles/360002627458
//! B/E/G/M use the published adjustment-layer context; G is the older gradient alias.
pub const CAPTURE_ONE: &[(&str, &str)] = &[
    ("tool.brush", "B"),
    ("tool.erase", "E"),
    ("tool.linear", "G"),
    ("tool.radial", "T"),
    ("view.maskOverlay", "M"),
    // Disable Lightroom's competing overlay/gradient defaults in this context.
    ("view.lightsOut", ""),
];
pub const GLOBAL_CHORDS: &[&str] = &[
    crate::GLOBAL_SHORTCUTS[0].2,
    crate::GLOBAL_SHORTCUTS[1].2,
    crate::GLOBAL_SHORTCUTS[2].2,
    crate::GLOBAL_SHORTCUTS[3].2,
    crate::GLOBAL_SHORTCUTS[6].2,
    crate::GLOBAL_SHORTCUTS[7].2,
    crate::GLOBAL_SHORTCUTS[8].2,
    crate::GLOBAL_SHORTCUTS[5].2,
    crate::GLOBAL_SHORTCUTS[4].2,
];
pub fn library_default<'a>(parent: &str, id: &str, edit: bool, base: Option<&'a str>) -> Option<&'a str> {
    let id = id.strip_prefix("library::").unwrap_or(id);
    if parent == "Capture One" && edit {
        if id == "alias/tool.linear/L" {
            return Some("L");
        }
        if matches!(id, "view.detail" | "view.gridToggle" | "alias/album.toggleTarget/B") {
            return None;
        }
        if let Some((_, key)) = CAPTURE_ONE.iter().find(|(command, _)| *command == id) {
            return (!key.is_empty()).then_some(*key);
        }
    }
    base
}

/// Affinity subset source: https://affinity.help/photo2/en-US.lproj/pages/Workspace/shortcuts.html
/// Photoshop/Lightroom bases remain the existing command/menu catalogs.
/// Owner decisions retain R = Rotate View and J = Healing / AI Remove.
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

/// Affinity keeps the shared temporary-tool lifecycle; unsupported commands are unassigned.
pub fn editor_default<'a>(parent: &str, id: &str, temporary: bool, base: Option<&'a str>) -> Option<&'a str> {
    if parent == "Affinity Photo" && !temporary { AFFINITY.iter().find(|(command, _)| *command == id).map(|(_, key)| *key) } else { base }
}
pub fn is_global(id: &str) -> bool {
    let id = id.strip_prefix("library::").unwrap_or(id);
    crate::GLOBAL_SHORTCUTS.iter().any(|(command, _, _)| *command == id)
        || matches!(id, "app.quit" | "view.enterFullScreen" | "view.fullScreenPreview" | "view.screenMode.cycle")
}
