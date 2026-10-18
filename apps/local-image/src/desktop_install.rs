//! `local-image --install [--system]` and `local-image --uninstall` on Linux: copies the app into
//! `~/.local/bin` (or `/usr/local/bin` with `--system`), links `local-image-cli` to it (the same
//! executable runs the command line under that name), and adds the desktop entry and icons so
//! Local Image appears in the application menu with its file types.
//! Settings and the library are never touched.

use std::path::{Path, PathBuf};

const APP_ID: &str = "io.github.zdbosoxfan.LocalImage";
const DESKTOP: &str = include_str!("../io.github.zdbosoxfan.LocalImage.desktop");

macro_rules! icon {
    ($size:literal) => {
        ($size, include_bytes!(concat!("../../../assets/app-icon/hicolor/", $size, "/apps/io.github.zdbosoxfan.LocalImage.png")) as &[u8])
    };
}

const ICONS: [(&str, &[u8]); 8] =
    [icon!("16x16"), icon!("24x24"), icon!("32x32"), icon!("48x48"), icon!("64x64"), icon!("128x128"), icon!("256x256"), icon!("512x512")];

fn prefix(system: bool) -> Result<PathBuf, String> {
    if system {
        return Ok(PathBuf::from("/usr/local"));
    }
    std::env::var_os("HOME").filter(|h| !h.is_empty()).map(|h| PathBuf::from(h).join(".local")).ok_or_else(|| "HOME is not set".to_owned())
}

fn write(path: &Path, bytes: &[u8], exec: bool) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    // Write beside and rename, so replacing the running binary works.
    let staged = li_fs::stage_stream(path, li_fs::Policy::Durable, &mut |w| w.write_all(bytes)).map_err(|e| format!("{}: {e}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = if exec { 0o755 } else { 0o644 };
        staged.set_permissions(std::fs::Permissions::from_mode(mode)).map_err(|e| e.to_string())?;
    }
    let _ = exec;
    staged.commit().map_err(|e| format!("{}: {e}", path.display()))
}

/// `bin/local-image-cli` as a relative symlink to `local-image`, replacing whatever is there (an
/// older install shipped a separate CLI executable). Created beside and renamed into place.
fn link_cli(bin: &Path) -> Result<(), String> {
    let link = bin.join("local-image-cli");
    let staged = bin.join(format!(".local-image-cli.{}", std::process::id()));
    let _ = std::fs::remove_file(&staged);
    std::os::unix::fs::symlink("local-image", &staged).map_err(|e| format!("{}: {e}", staged.display()))?;
    std::fs::rename(&staged, &link).map_err(|e| {
        let _ = std::fs::remove_file(&staged);
        format!("{}: {e}", link.display())
    })
}

/// Installs from wherever this binary runs.
pub fn install(system: bool) -> Result<String, String> {
    let p = prefix(system)?;
    let me = std::env::current_exe().map_err(|e| e.to_string())?;
    let bin = p.join("bin");
    if me.parent() != Some(bin.as_path()) {
        write(&bin.join("local-image"), &std::fs::read(&me).map_err(|e| e.to_string())?, true)?;
    }
    link_cli(&bin)?;
    let exec = bin.join("local-image");
    let desktop = DESKTOP.replace("Exec=local-image %F", &format!("Exec={} %F", exec.display()));
    write(&p.join("share/applications").join(format!("{APP_ID}.desktop")), desktop.as_bytes(), false)?;
    for (size, png) in ICONS {
        write(&p.join("share/icons/hicolor").join(size).join("apps").join(format!("{APP_ID}.png")), png, false)?;
    }
    let mut msg = format!("Installed Local Image into {} (run: local-image)", p.display());
    let on_path = std::env::var_os("PATH").is_some_and(|path| std::env::split_paths(&path).any(|d| d == bin));
    if !on_path {
        msg.push_str(&format!("\nNote: {} is not on your PATH", bin.display()));
    }
    Ok(msg)
}

pub fn uninstall(system: bool) -> Result<String, String> {
    let p = prefix(system)?;
    for f in [p.join("bin/local-image"), p.join("bin/local-image-cli"), p.join("share/applications").join(format!("{APP_ID}.desktop"))] {
        let _ = std::fs::remove_file(f);
    }
    for (size, _) in ICONS {
        let _ = std::fs::remove_file(p.join("share/icons/hicolor").join(size).join("apps").join(format!("{APP_ID}.png")));
    }
    Ok(format!("Removed Local Image from {} (settings and the library are kept)", p.display()))
}

#[cfg(test)]
mod tests {
    #[test]
    fn j11_installer_keeps_complete_bytes_modes_and_cleans_failed_staging() {
        let dir = std::env::temp_dir().join(format!("j11-install-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("launcher");
        super::write(&path, b"original", true).unwrap();
        {
            let _fault = li_fs::fail_writes_after(2);
            assert!(super::write(&path, b"replacement", true).is_err());
        }
        assert_eq!(std::fs::read(&path).unwrap(), b"original");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        super::write(&path, b"replacement", false).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"replacement");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o644);
            super::write(&path, b"executable", true).unwrap();
            assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o755);
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn the_cli_is_installed_as_a_link_replacing_an_old_copy() {
        let dir = std::env::temp_dir().join(format!("install-cli-link-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("local-image-cli"), b"old separate executable").unwrap();
        super::link_cli(&dir).unwrap();
        super::link_cli(&dir).unwrap();
        assert_eq!(std::fs::read_link(dir.join("local-image-cli")).unwrap(), std::path::Path::new("local-image"));
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn the_desktop_entry_names_the_app_id_and_icons_are_pngs() {
        assert!(super::DESKTOP.contains("Icon=io.github.zdbosoxfan.LocalImage"));
        assert!(super::DESKTOP.contains("StartupWMClass=io.github.zdbosoxfan.LocalImage"));
        assert!(super::ICONS.iter().all(|(_, b)| b.starts_with(b"\x89PNG")));
    }
}
