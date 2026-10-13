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
pub const GLOBAL_CHORDS: &[&str] = &["Cmd+Q", "Cmd+K", "Cmd+Z", "Cmd+Shift+Z", "Cmd+Alt+1", "Cmd+Alt+2", "Cmd+Alt+3", "Cmd+Shift+F", "F"];
pub fn default<'a>(parent: &str, id: &str, edit: bool, base: Option<&'a str>) -> Option<&'a str> {
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
