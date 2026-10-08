//! `local-image --install [--system]` and `local-image --uninstall` on Linux: copies the app (and
//! `local-image-cli` beside it) into `~/.local/bin` (or `/usr/local/bin` with `--system`) and adds
//! the desktop entry and icons so Local Image appears in the application menu with its file types.
//! Settings and the library are never touched.

use std::path::{Path, PathBuf};

const APP_ID: &str = "io.github.zdbosoxfan.LocalImage";
const DESKTOP: &str = include_str!("../io.github.zdbosoxfan.LocalImage.desktop");

macro_rules! icon {
    ($size:literal) => {
        ($size, &include_bytes!(concat!("../../../assets/app-icon/hicolor/", $size, "/apps/io.github.zdbosoxfan.LocalImage.png"))[..])
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
    let tmp = path.with_extension("local-image-tmp");
    std::fs::write(&tmp, bytes).map_err(|e| format!("{}: {e}", tmp.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = if exec { 0o755 } else { 0o644 };
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(mode)).map_err(|e| e.to_string())?;
    }
    let _ = exec;
    std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Installs from wherever this binary runs.
pub fn install(system: bool) -> Result<String, String> {
    let p = prefix(system)?;
    let me = std::env::current_exe().map_err(|e| e.to_string())?;
    let bin = p.join("bin");
    if me.parent() != Some(bin.as_path()) {
        write(&bin.join("local-image"), &std::fs::read(&me).map_err(|e| e.to_string())?, true)?;
        let cli = me.with_file_name("local-image-cli");
        if cli.exists() {
            write(&bin.join("local-image-cli"), &std::fs::read(&cli).map_err(|e| e.to_string())?, true)?;
        }
    }
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
    fn the_desktop_entry_names_the_app_id_and_icons_are_pngs() {
        assert!(super::DESKTOP.contains("Icon=io.github.zdbosoxfan.LocalImage"));
        assert!(super::DESKTOP.contains("StartupWMClass=io.github.zdbosoxfan.LocalImage"));
        assert!(super::ICONS.iter().all(|(_, b)| b.starts_with(b"\x89PNG")));
    }
}
