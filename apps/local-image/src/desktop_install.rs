//! From the unpacked folder, run `./local-image --install`: it copies the app to `~/.local/bin`,
//! links `local-image-cli` to it and adds the menu entry. Use `sudo ./local-image --install --system`
//! for `/usr/local`, `local-image --uninstall` to remove a user install, or
//! `sudo /usr/local/bin/local-image --uninstall --system` to remove a system install.
//! Settings and the library are never touched.

use std::path::{Path, PathBuf};

const APP_ID: &str = "io.github.zdbosoxfan.LocalImage";
const DESKTOP: &str = include_str!("../io.github.zdbosoxfan.LocalImage.desktop");
const MIME: &str = include_str!("../io.github.zdbosoxfan.LocalImage.xml");

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
    write_stream(path, exec, &mut |writer| writer.write_all(bytes))
}

fn write_stream(path: &Path, exec: bool, fill: &mut dyn FnMut(&mut dyn std::io::Write) -> std::io::Result<()>) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    // Write beside and rename, so replacing the running binary works.
    let staged = li_fs::stage_stream(path, li_fs::Policy::Durable, fill).map_err(|e| format!("{}: {e}", path.display()))?;
    use std::os::unix::fs::PermissionsExt;
    let mode = if exec { 0o755 } else { 0o644 };
    staged.set_permissions(std::fs::Permissions::from_mode(mode)).map_err(|e| format!("{}: {e}", path.display()))?;
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
    install_into(&p, &me)
}

fn install_into(p: &Path, exe: &Path) -> Result<String, String> {
    let bin = p.join("bin");
    let canonical_bin = match std::fs::canonicalize(&bin) {
        Ok(path) => path,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => bin.clone(),
        Err(e) => return Err(format!("{}: {e}", bin.display())),
    };
    let exec = bin.join("local-image");
    let destination_is_symlink = match std::fs::symlink_metadata(&exec) {
        Ok(metadata) => metadata.file_type().is_symlink(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        Err(e) => return Err(format!("{}: {e}", exec.display())),
    };
    let already_installed = exe.parent() == Some(canonical_bin.as_path()) && !destination_is_symlink;
    if !already_installed {
        let mut source = std::fs::File::open(exe).map_err(|e| format!("{}: {e}", exe.display()))?;
        if destination_is_symlink {
            std::fs::remove_file(&exec).map_err(|e| format!("{}: {e}", exec.display()))?;
        }
        write_stream(&exec, true, &mut |writer| std::io::copy(&mut source, writer).map(|_| ()))?;
    }
    link_cli(&bin)?;
    // Desktop string escapes are decoded before Exec's argument quoting and field codes.
    let mut quoted_exec = String::new();
    for character in exec.to_string_lossy().chars() {
        match character {
            '\\' => quoted_exec.push_str(r"\\\\"),
            '"' => quoted_exec.push_str(r#"\\""#),
            '`' => quoted_exec.push_str(r"\\`"),
            '$' => quoted_exec.push_str(r"\\$"),
            '%' => quoted_exec.push_str("%%"),
            _ => quoted_exec.push(character),
        }
    }
    let try_exec = exec.to_string_lossy().replace('\\', r"\\");
    let desktop = DESKTOP.replace("Exec=local-image %F", &format!("Exec=\"{quoted_exec}\" %F\nTryExec={try_exec}"));
    write(&p.join("share/applications").join(format!("{APP_ID}.desktop")), desktop.as_bytes(), false)?;
    write(&p.join("share/mime/packages").join(format!("{APP_ID}.xml")), MIME.as_bytes(), false)?;
    for (size, png) in ICONS {
        write(&p.join("share/icons/hicolor").join(size).join("apps").join(format!("{APP_ID}.png")), png, false)?;
    }
    refresh_caches(p);
    let mut msg = if already_installed {
        format!(
            "Already running from {}; desktop entry and file types refreshed, binary not replaced.\nTo upgrade, run ./local-image --install from the new version's unpacked folder (sudo ./local-image --install --system for /usr/local).",
            bin.display()
        )
    } else {
        format!("Installed Local Image into {} (run: local-image)", p.display())
    };
    let on_path = std::env::var_os("PATH").is_some_and(|path| std::env::split_paths(&path).any(|d| d == bin));
    if !on_path {
        msg.push_str(&format!("\nNote: {} is not on your PATH", bin.display()));
    }
    Ok(msg)
}

pub fn uninstall(system: bool) -> Result<String, String> {
    uninstall_from(&prefix(system)?)
}

fn uninstall_from(p: &Path) -> Result<String, String> {
    let mut removed = false;
    let mut failures = Vec::new();
    let files = [
        p.join("bin/local-image"),
        p.join("bin/local-image-cli"),
        p.join("share/applications").join(format!("{APP_ID}.desktop")),
        p.join("share/mime/packages").join(format!("{APP_ID}.xml")),
    ];
    for path in files.into_iter().chain(ICONS.iter().map(|(size, _)| p.join("share/icons/hicolor").join(size).join("apps").join(format!("{APP_ID}.png")))) {
        match std::fs::remove_file(&path) {
            Ok(()) => removed = true,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => failures.push(format!("{}: {e}", path.display())),
        }
    }
    refresh_caches(p);
    if !failures.is_empty() {
        return Err(format!("Could not completely remove Local Image:\n{}", failures.join("\n")));
    }
    if !removed {
        return Ok(format!("No Local Image install found in {} (--system selects /usr/local)", p.display()));
    }
    Ok(format!("Removed Local Image from {} (settings and the library are kept)", p.display()))
}

fn refresh_caches(prefix: &Path) {
    let commands: [(&str, &[&str], PathBuf); 3] = [
        ("update-mime-database", &[], prefix.join("share/mime")),
        ("update-desktop-database", &["-q"], prefix.join("share/applications")),
        ("gtk-update-icon-cache", &["-q", "-t", "-f"], prefix.join("share/icons/hicolor")),
    ];
    for (program, options, directory) in commands {
        // Creating a cache in a previously uncached theme can hide other apps' later icons.
        if program == "gtk-update-icon-cache" && !directory.join("icon-theme.cache").exists() {
            continue;
        }
        // Desktop caches are best effort: absent or failing tools must not undo file installation.
        let _ = std::process::Command::new(program).args(options).arg(directory).output();
    }
}

#[cfg(test)]
mod tests {
    fn test_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("install-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn installed_binary_reports_how_to_upgrade() {
        let dir = test_dir("skip-copy");
        let exe = dir.join("bin/local-image");
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::write(&exe, b"installed executable").unwrap();
        let message = super::install_into(&dir, &exe).unwrap();
        assert!(message.contains("Already running from"), "{message}");
        assert!(message.contains("binary not replaced"), "{message}");
        assert!(message.contains("./local-image --install"), "{message}");
        assert_eq!(std::fs::read(&exe).unwrap(), b"installed executable");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn installed_binary_recognizes_a_symlinked_prefix() {
        let dir = test_dir("canonical-prefix");
        let real = dir.join("real");
        let alias = dir.join("alias");
        std::fs::create_dir_all(real.join("bin")).unwrap();
        std::os::unix::fs::symlink(&real, &alias).unwrap();
        let exe = real.join("bin/local-image");
        std::fs::write(&exe, b"installed executable").unwrap();
        let message = super::install_into(&alias, &exe.canonicalize().unwrap()).unwrap();
        assert!(message.contains("Already running from"), "{message}");
        assert!(message.contains("binary not replaced"), "{message}");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn uninstall_reports_an_absent_install() {
        let dir = test_dir("absent");
        let message = super::uninstall_from(&dir).unwrap();
        assert!(message.contains(&format!("No Local Image install found in {}", dir.display())), "{message}");
        assert!(message.contains("--system"), "{message}");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn uninstall_collects_errors_and_still_removes_other_files() {
        let dir = test_dir("remove-errors");
        let blocked = [dir.join("bin/local-image"), dir.join("share/applications").join(format!("{}.desktop", super::APP_ID))];
        for path in &blocked {
            std::fs::create_dir_all(path).unwrap();
        }
        let removable = [
            dir.join("bin/local-image-cli"),
            dir.join("share/icons/hicolor/512x512/apps").join(format!("{}.png", super::APP_ID)),
            dir.join("share/mime/packages").join(format!("{}.xml", super::APP_ID)),
        ];
        for path in &removable {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, b"installed file").unwrap();
        }
        let error = super::uninstall_from(&dir).unwrap_err();
        for path in blocked {
            assert!(error.contains(&path.display().to_string()), "{error}");
            assert!(path.is_dir());
        }
        for path in removable {
            assert!(!path.exists(), "{} was not removed", path.display());
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn install_replaces_live_and_dangling_binary_symlinks() {
        let dir = test_dir("binary-symlinks");
        let source = dir.join("download");
        let other = dir.join("other-executable");
        std::fs::write(&source, b"new executable").unwrap();
        std::fs::write(&other, b"unrelated executable").unwrap();
        for (name, target) in [("live", other.clone()), ("dangling", dir.join("missing"))] {
            let prefix = dir.join(name);
            let installed = prefix.join("bin/local-image");
            std::fs::create_dir_all(installed.parent().unwrap()).unwrap();
            std::os::unix::fs::symlink(&target, &installed).unwrap();
            super::install_into(&prefix, &source).unwrap();
            assert!(!std::fs::symlink_metadata(&installed).unwrap().file_type().is_symlink());
            assert_eq!(std::fs::read(&installed).unwrap(), b"new executable");
            assert_eq!(std::fs::read(&other).unwrap(), b"unrelated executable");
            assert!(!dir.join("missing").exists());
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn install_replaces_a_binary_symlink_within_the_same_bin_directory() {
        let dir = test_dir("same-bin-symlink");
        let source = dir.join("bin/local-image-versioned");
        let installed = dir.join("bin/local-image");
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        std::fs::write(&source, b"versioned executable").unwrap();
        std::os::unix::fs::symlink("local-image-versioned", &installed).unwrap();
        let message = super::install_into(&dir, &source).unwrap();
        assert!(!std::fs::symlink_metadata(&installed).unwrap().file_type().is_symlink());
        assert_eq!(std::fs::read(&installed).unwrap(), b"versioned executable");
        assert_eq!(std::fs::read(&source).unwrap(), b"versioned executable");
        assert!(message.starts_with("Installed Local Image"), "{message}");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn installed_desktop_escapes_exec_and_keeps_tryexec_a_plain_path() {
        assert!(super::DESKTOP.lines().any(|line| line == "Exec=local-image %F"));
        let dir = test_dir("desktop-path");
        let prefix = dir.join(r#"space "$\%`"#);
        let source = dir.join("download");
        std::fs::write(&source, b"executable").unwrap();
        super::install_into(&prefix, &source).unwrap();
        let desktop = std::fs::read_to_string(prefix.join("share/applications").join(format!("{}.desktop", super::APP_ID))).unwrap();
        let exec = format!(r#"Exec="{}/space \\"\\$\\\\%%\\`/bin/local-image" %F"#, dir.display());
        let try_exec = format!(r#"TryExec={}/space "$\\%`/bin/local-image"#, dir.display());
        assert_eq!(desktop.lines().find(|line| line.starts_with("Exec=")), Some(exec.as_str()));
        assert_eq!(desktop.lines().find(|line| line.starts_with("TryExec=")), Some(try_exec.as_str()));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn install_and_uninstall_register_native_documents() {
        let dir = test_dir("native-mime");
        let source = dir.join("download");
        let prefix = dir.join("prefix");
        std::fs::write(&source, b"new executable").unwrap();
        super::install_into(&prefix, &source).unwrap();
        let installed = prefix.join("bin/local-image");
        assert_eq!(std::fs::read(&installed).unwrap(), b"new executable");
        let mime = prefix.join("share/mime/packages").join(format!("{}.xml", super::APP_ID));
        let xml = std::fs::read_to_string(&mime).unwrap();
        assert!(xml.contains(r#"<mime-info xmlns="http://www.freedesktop.org/standards/shared-mime-info">"#));
        assert!(xml.contains(r#"<mime-type type="application/x-local-image-document">"#));
        assert!(xml.contains(r#"<sub-class-of type="application/zip"/>"#));
        assert!(xml.contains(r#"<glob pattern="*.pcraft"/>"#));
        let message = super::uninstall_from(&prefix).unwrap();
        assert!(message.starts_with("Removed Local Image"), "{message}");
        assert!(!installed.exists());
        assert!(!mime.exists());
        assert!(source.exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn desktop_mime_types_track_every_open_extension() {
        // Glob mappings from shared-mime-info's freedesktop.org.xml, plus our native document.
        // Extensions without a standard type stay explicit so newly opened formats need review.
        let mappings = [
            ("pcraft", Some("application/x-local-image-document")),
            ("psdt", None),
            ("svg", Some("image/svg+xml")),
            ("svgz", Some("image/svg+xml-compressed")),
            ("pdf", Some("application/pdf")),
            ("abr", None),
            ("grd", None),
            ("png", Some("image/png")),
            ("apng", Some("image/apng")),
            ("jpg", Some("image/jpeg")),
            ("jpeg", Some("image/jpeg")),
            ("jpe", Some("image/jpeg")),
            ("jfif", Some("image/jpeg")),
            ("tif", Some("image/tiff")),
            ("tiff", Some("image/tiff")),
            ("webp", Some("image/webp")),
            ("gif", Some("image/gif")),
            ("bmp", Some("image/bmp")),
            ("dib", Some("image/bmp")),
            ("tga", Some("image/x-tga")),
            ("icb", Some("image/x-tga")),
            ("vda", Some("image/x-tga")),
            ("vst", Some("image/x-tga")),
            ("ico", Some("image/vnd.microsoft.icon")),
            ("pnm", Some("image/x-portable-anymap")),
            ("pbm", Some("image/x-portable-bitmap")),
            ("pgm", Some("image/x-portable-graymap")),
            ("ppm", Some("image/x-portable-pixmap")),
            ("pam", None),
            ("pfm", None),
            ("qoi", Some("image/qoi")),
            ("exr", Some("image/x-exr")),
            ("hdr", None),
            ("heic", Some("image/heif")),
            ("heif", Some("image/heif")),
            ("hif", Some("image/heif")),
            ("jxl", Some("image/jxl")),
            ("psd", Some("image/vnd.adobe.photoshop")),
            ("psb", None),
            ("dng", Some("image/x-adobe-dng")),
            ("cr2", Some("image/x-canon-cr2")),
            ("cr3", Some("image/x-canon-cr3")),
            ("nef", Some("image/x-nikon-nef")),
            ("nrw", Some("image/x-nikon-nrw")),
            ("arw", Some("image/x-sony-arw")),
            ("arq", None),
            ("raf", Some("image/x-fuji-raf")),
            ("orf", Some("image/x-olympus-orf")),
            ("rw2", Some("image/x-panasonic-rw2")),
            ("rwl", None),
            ("raw", Some("image/x-panasonic-rw")),
            ("pef", Some("image/x-pentax-pef")),
            ("srw", None),
            ("x3f", Some("image/x-sigma-x3f")),
            ("3fr", None),
            ("iiq", None),
            ("erf", None),
            ("mos", None),
            ("srf", Some("image/x-sony-srf")),
            ("sr2", Some("image/x-sony-sr2")),
            ("kdc", Some("image/x-kodak-kdc")),
            ("dcr", Some("image/x-kodak-dcr")),
            ("mef", None),
            ("mrw", Some("image/x-minolta-mrw")),
            ("fff", None),
            ("crw", Some("image/x-canon-crw")),
        ];
        let extensions = crate::services::open_extensions();
        let types: std::collections::BTreeSet<_> =
            super::DESKTOP.lines().find_map(|line| line.strip_prefix("MimeType=")).unwrap().split(';').filter(|mime| !mime.is_empty()).collect();
        for extension in &extensions {
            let (_, mime) = mappings.iter().find(|(known, _)| known == extension).unwrap_or_else(|| panic!("unmapped open extension: {extension}"));
            if let Some(mime) = mime {
                assert!(types.contains(mime), "missing MIME type {mime} for {extension}");
            }
        }
        for mime in types {
            // Official Linux packages enable HEIF; feature-free developer builds do not.
            if !cfg!(feature = "heif") && mime == "image/heif" {
                continue;
            }
            assert!(mappings.iter().any(|(ext, mapped)| *mapped == Some(mime) && extensions.contains(ext)), "unreadable MIME type: {mime}");
        }
    }

    #[test]
    fn installer_keeps_complete_bytes_modes_and_cleans_failed_staging() {
        let dir = std::env::temp_dir().join(format!("install-{}", std::process::id()));
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
