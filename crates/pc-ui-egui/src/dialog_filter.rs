//! File-dialog filters that match extensions whatever their letter case.
//!
//! On Linux (and the BSDs), `rfd` turns each extension into a `*.ext` glob for the GTK dialog and
//! for the desktop portal (`xdg-desktop-portal` FileChooser), and both match globs
//! case-sensitively, so a filter of `nef` hid `DSC_0001.NEF` and `jpg` hid `IMG_0001.JPG`.
//! Windows and macOS ignore case. Every dialog filter in the app (File › Open, Save, Library ›
//! Add Photos, the preset and AI pickers) goes through [`FileDialogExt::add_filter_nocase`];
//! tests check that nothing calls rfd's own `add_filter`.
//!
//! [`file_dialog`] starts every dialog parented to the app's window.
//!
//! Ported from LightCraft `apps/lightcraft/src/dialog_filter.rs` (e2d35de, MIT OR Apache-2.0)
//! and PhotoCraft 9b4562f2 (the same fix for File › Open).

/// The patterns to hand to `rfd` for `exts`, which are plain extensions (a leading dot is
/// dropped).
///
/// Windows and macOS take the extension literally and ignore case: lower case, de-duplicated.
/// Linux (`linux`): `rfd` formats each entry as `*.{entry}`, which GTK 3 and the portal match as a
/// glob, so an entry may carry glob syntax. Each extension gives the lower-case form first (the one
/// a save dialog may append), the UPPER-case form (kept in case a portal backend does not do
/// `[...]` classes) and a bracket form such as `[nN][eE][fF]` that matches every mix (`.Nef`).
pub fn dialog_extensions_for(exts: &[&str], linux: bool) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut push = |s: String| {
        if !s.is_empty() && !out.contains(&s) {
            out.push(s);
        }
    };
    for ext in exts {
        let ext = ext.trim_start_matches('.');
        let lower = ext.to_lowercase();
        push(lower.clone());
        if !linux {
            continue;
        }
        push(ext.to_uppercase());
        // `[` `]` `*` `?` in an extension would already be glob syntax: leave those alone.
        if lower.chars().any(char::is_alphabetic) && !lower.contains(['[', ']', '*', '?', '\\']) {
            push(
                lower
                    .chars()
                    .map(|c| {
                        let up: String = c.to_uppercase().collect();
                        if up.chars().count() == 1 && up != c.to_string() { format!("[{c}{up}]") } else { c.to_string() }
                    })
                    .collect(),
            );
        }
    }
    out
}

/// Whether this build's dialogs match `*.ext` globs case-sensitively (Linux and the BSDs).
pub const CASE_SENSITIVE_GLOBS: bool = cfg!(all(unix, not(target_os = "macos")));

/// [`dialog_extensions_for`] for the platform this is built for.
pub fn dialog_extensions(exts: &[&str]) -> Vec<String> {
    dialog_extensions_for(exts, CASE_SENSITIVE_GLOBS)
}

/// The file dialogs' parent, set once the window exists ([`set_dialog_parent`]). Without a
/// parent the xdg-desktop-portal picker on Wayland is a stray top-level ("Failed to associate
/// portal window with parent window"): a tiling compositor puts it beside the app and a
/// fullscreen window hides it entirely, so the dialog looks like a hang. rfd exports the Wayland
/// surface through xdg_foreign itself; `FileDialog` is `Clone` and `Send`, so one parented
/// template serves every picker (upstream LightCraft 9c2bad1).
static DIALOG_PARENT: std::sync::OnceLock<rfd::FileDialog> = std::sync::OnceLock::new();

/// Make `template` (a `FileDialog` with only `set_parent` applied) the start of every
/// [`file_dialog`]. The app sets it on Linux and the BSDs only: on macOS a parented dialog
/// becomes a sheet and on Windows a modal, a behaviour change this does not make.
pub fn set_dialog_parent(template: rfd::FileDialog) {
    let _ = DIALOG_PARENT.set(template);
}

/// A file dialog owned by the app's window, or a plain one before the window exists (tests).
pub fn file_dialog() -> rfd::FileDialog {
    DIALOG_PARENT.get().cloned().unwrap_or_else(rfd::FileDialog::new)
}

static UNAVAILABLE: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// A file dialog came back empty because none can open on this system (on Linux: no desktop
/// portal and no zenity); `why` says what to install. The host shows it instead of taking the
/// empty answer for Cancel (upstream PhotoCraft b3993e89, #3354).
pub fn report_unavailable(why: String) {
    if let Ok(mut slot) = UNAVAILABLE.lock() {
        *slot = Some(why);
    }
}

/// The reason from [`report_unavailable`], once.
pub fn take_unavailable() -> Option<String> {
    UNAVAILABLE.lock().ok().and_then(|mut slot| slot.take())
}

/// `add_filter` that matches the extensions in any letter case.
pub trait FileDialogExt {
    fn add_filter_nocase(self, name: impl Into<String>, exts: &[&str]) -> Self;
}

impl FileDialogExt for rfd::FileDialog {
    fn add_filter_nocase(self, name: impl Into<String>, exts: &[&str]) -> Self {
        self.add_filter(name, &dialog_extensions(exts))
    }
}

/// Test helper: does the dialog glob `*.{entry}` match a file with extension `ext`? A tiny
/// matcher for the subset [`dialog_extensions_for`] produces (literals and `[..]` classes), used
/// by this crate's and the app's tests.
#[doc(hidden)]
pub fn glob_ext(entry: &str, ext: &str) -> bool {
    let mut pat = entry.chars();
    let mut s = ext.chars();
    while let Some(p) = pat.next() {
        let Some(c) = s.next() else { return false };
        if p == '[' {
            let mut hit = false;
            for q in pat.by_ref() {
                if q == ']' {
                    break;
                }
                hit |= q == c;
            }
            if !hit {
                return false;
            }
        } else if p != c {
            return false;
        }
    }
    s.next().is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dialog_filter_other_platforms_get_lower_case_once() {
        assert_eq!(dialog_extensions_for(&["NEF", "nef", ".Jpg", "jpg", ""], false), ["nef", "jpg"]);
    }

    #[test]
    fn dialog_filter_linux_gets_lower_upper_and_any_mix() {
        assert_eq!(dialog_extensions_for(&["nef"], true), ["nef", "NEF", "[nN][eE][fF]"]);
        // Digits have no case; a duplicate adds nothing.
        assert_eq!(dialog_extensions_for(&["mp4", "MP4"], true), ["mp4", "MP4", "[mM][pP]4"]);
        assert_eq!(dialog_extensions_for(&["7z"], true), ["7z", "7Z", "7[zZ]"]);
        assert_eq!(dialog_extensions_for(&["123"], true), ["123"]);
    }

    #[test]
    fn dialog_filter_linux_glob_syntax_is_not_wrapped_again() {
        assert_eq!(dialog_extensions_for(&["[nN]ef"], true), ["[nn]ef", "[NN]EF"]);
    }

    #[test]
    fn dialog_filter_linux_patterns_match_every_case_mix() {
        let pats = dialog_extensions_for(&["nef", "cr3", "jpg", "xmp"], true);
        let matches = |name: &str| pats.iter().any(|p| glob_ext(p, name));
        for ok in ["nef", "NEF", "Nef", "nEF", "CR3", "Cr3", "JPG", "Jpg", "jPg", "XMP", "xMp"] {
            assert!(matches(ok), "{ok}");
        }
        for bad in ["nex", "ne", "nefs", "jpeg", "cr2", "txt"] {
            assert!(!matches(bad), "{bad}");
        }
    }

    /// The editor's own dialogs (ONNX model, ComfyUI workflow, reference images) use the helper.
    #[test]
    fn dialog_filter_no_editor_dialog_bypasses_the_helper() {
        for (file, src) in [("ai_ui.rs", include_str!("ai_ui.rs")), ("generate_ui.rs", include_str!("generate_ui.rs"))] {
            assert!(!src.contains(".add_filter("), "{file} calls rfd's add_filter; use add_filter_nocase");
            assert!(src.contains(".add_filter_nocase("), "{file}");
            assert!(!src.contains(concat!("rfd::FileDialog::", "new()")), "{file} opens an unparented dialog; use dialog_filter::file_dialog");
        }
    }
}
