//! Why a file dialog may never appear on Linux (upstream PhotoCraft b3993e89, #3354).
//!
//! `rfd` talks to the XDG file-chooser portal through `libdbus-1`, which it loads at runtime, and
//! falls back to `zenity`. When neither works (a minimal window-manager session without a portal
//! backend) it logs the failure and answers "nothing picked", which the app took for Cancel: the
//! menu item did nothing and said nothing. [`start`] looks for the pieces once, in the background,
//! and [`known_problem`] returns what to install when a dialog came back empty and none of them
//! is there.
//!
//! The probe only ever explains an empty answer; it never stops a dialog from opening. Anything
//! it can't tell counts as present, so a working system never sees the message.
//!
//! Upstream asks the session bus for the portal's `FileChooser` interface (zbus); Local Image has
//! no D-Bus dependency of its own, so it reads the installed portal backends' `.portal` files
//! instead (`<XDG_DATA_DIRS>/xdg-desktop-portal/portals/*.portal`). No portals folder at all (a
//! Flatpak sandbox, an unusual prefix) is "can't tell".
#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// What the probe found. `None` means it couldn't tell, which counts as present.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Findings {
    /// `libdbus-1.so.3` is in the linker cache (`rfd` loads it for the portal).
    pub libdbus: Option<bool>,
    /// An installed portal backend offers a file chooser.
    pub file_chooser_portal: Option<bool>,
    /// `zenity` is on `PATH` (`rfd`'s fallback).
    pub zenity: bool,
}

/// The message for `findings`, or `None` when some file dialog should work.
pub fn problem(findings: Findings) -> Option<String> {
    let portal = findings.libdbus != Some(false) && findings.file_chooser_portal != Some(false);
    if portal || findings.zenity {
        return None;
    }
    let cause = if findings.libdbus == Some(false) { "libdbus is missing" } else { "no desktop portal offers a file chooser" };
    Some(format!(
        "No file dialog can open: {cause}. Install xdg-desktop-portal with a backend (xdg-desktop-portal-kde, -gtk or -gnome) and libdbus, or zenity. Meanwhile, drop a file onto the window or paste a copied image."
    ))
}

static RESULT: OnceLock<Option<String>> = OnceLock::new();

/// The problem found by [`start`], once it has finished.
pub fn known_problem() -> Option<String> {
    RESULT.get().cloned().flatten()
}

/// `picked` from a file dialog; when it's empty and no dialog backend exists, the reason is
/// handed to the UI (the host shows it) instead of passing for Cancel.
pub fn explain_empty<T>(picked: Option<T>) -> Option<T> {
    if picked.is_none()
        && let Some(why) = known_problem()
    {
        photocraft_ui_egui::dialog_filter::report_unavailable(why);
    }
    picked
}

/// Whether `program` is an executable file in one of the `path` directories.
fn on_path(program: &str, path: &std::ffi::OsStr) -> bool {
    std::env::split_paths(path).any(|dir| {
        let Ok(meta) = dir.join(program).metadata() else { return false };
        if !meta.is_file() {
            return false;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            meta.permissions().mode() & 0o111 != 0
        }
        #[cfg(not(unix))]
        {
            true
        }
    })
}

/// The portal backend folders under `xdg_data_dirs` (the XDG default when unset or empty).
fn portal_dirs(xdg_data_dirs: Option<&str>) -> Vec<PathBuf> {
    let dirs = xdg_data_dirs.filter(|d| !d.is_empty()).unwrap_or("/usr/local/share:/usr/share");
    dirs.split(':').filter(|d| !d.is_empty()).map(|d| Path::new(d).join("xdg-desktop-portal").join("portals")).collect()
}

/// Whether a backend in `dirs` declares the FileChooser interface; `None` when none of the folders
/// exists (nothing to judge by).
fn file_chooser_portal_in(dirs: &[PathBuf]) -> Option<bool> {
    let mut seen = false;
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(dir) else { continue };
        seen = true;
        for e in entries.flatten() {
            let path = e.path();
            if path.extension().is_some_and(|x| x == "portal")
                && std::fs::read_to_string(&path).is_ok_and(|text| text.contains("org.freedesktop.impl.portal.FileChooser"))
            {
                return Some(true);
            }
        }
    }
    seen.then_some(false)
}

/// Look for the file dialog backends on a helper thread (`ldconfig` can take a moment).
#[cfg(target_os = "linux")]
pub fn start() {
    let spawned = std::thread::Builder::new().name("file dialog probe".into()).spawn(|| {
        let findings = Findings {
            libdbus: crate::linux_libs::cached("libdbus-1.so.3"),
            file_chooser_portal: file_chooser_portal_in(&portal_dirs(std::env::var("XDG_DATA_DIRS").ok().as_deref())),
            zenity: std::env::var_os("PATH").is_some_and(|path| on_path("zenity", &path)),
        };
        log::info!("file dialog backends: {findings:?}");
        let _ = RESULT.set(problem(findings));
    });
    if let Err(e) = spawned {
        log::warn!("couldn't start the file dialog probe: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(libdbus: Option<bool>, file_chooser_portal: Option<bool>, zenity: bool) -> Findings {
        Findings { libdbus, file_chooser_portal, zenity }
    }

    #[test]
    fn file_dialog_probe_a_working_backend_is_not_reported() {
        assert_eq!(problem(found(Some(true), Some(true), false)), None, "portal");
        assert_eq!(problem(found(Some(false), Some(false), true)), None, "zenity alone");
        assert_eq!(problem(found(None, None, false)), None, "unknown counts as present");
        assert_eq!(problem(found(Some(true), None, false)), None, "portal folder not found");
    }

    #[test]
    fn file_dialog_probe_nothing_to_show_a_dialog_with_is_explained() {
        let no_portal = problem(found(Some(true), Some(false), false)).unwrap();
        assert!(no_portal.contains("no desktop portal offers a file chooser") && no_portal.contains("xdg-desktop-portal"), "{no_portal}");
        let no_dbus = problem(found(Some(false), Some(true), false)).unwrap();
        assert!(no_dbus.contains("libdbus is missing"), "{no_dbus}");
    }

    #[test]
    // TEST-CHANGED F001: a regular Unix file is unavailable until its executable bits are set.
    fn file_dialog_probe_finds_programs_and_portal_backends() {
        let dir = std::env::temp_dir().join(format!("local-image-probe-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("zenity"), b"").unwrap();
        let path = std::env::join_paths([Path::new("/nonexistent"), dir.as_path()]).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert!(!on_path("zenity", &path));
            std::fs::set_permissions(dir.join("zenity"), std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        assert!(on_path("zenity", &path));
        assert!(!on_path("kdialog", &path));
        // Portal backends: none installed, one without a file chooser, then KDE's.
        let data = dir.join("share");
        let dirs = portal_dirs(Some(&format!("/nonexistent:{}", data.display())));
        assert_eq!(dirs[1], data.join("xdg-desktop-portal/portals"));
        assert_eq!(file_chooser_portal_in(&dirs), None, "no portals folder: can't tell");
        std::fs::create_dir_all(&dirs[1]).unwrap();
        std::fs::write(dirs[1].join("wlr.portal"), "[portal]\nInterfaces=org.freedesktop.impl.portal.Screenshot;\n").unwrap();
        assert_eq!(file_chooser_portal_in(&dirs), Some(false));
        std::fs::write(dirs[1].join("kde.portal"), "[portal]\nInterfaces=org.freedesktop.impl.portal.FileChooser;org.freedesktop.impl.portal.Print;\n")
            .unwrap();
        assert_eq!(file_chooser_portal_in(&dirs), Some(true));
        assert_eq!(portal_dirs(None)[1], Path::new("/usr/share/xdg-desktop-portal/portals"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
