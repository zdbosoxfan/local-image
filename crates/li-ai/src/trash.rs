//! Moving files to the desktop Trash, in pure Rust, following the freedesktop.org Trash
//! specification 1.0 (Linux and the BSDs): the file goes to `files/` of the home trash
//! (`$XDG_DATA_HOME/Trash`) when it is on the same filesystem, else to `$topdir/.Trash-$uid` of
//! its own filesystem, with a `.trashinfo` record in `info/` so file managers can show and
//! restore it. Nothing is copied: a rename either moves the file or fails.
//!
//! Other platforms have no Trash here ([`available`] is false); callers delete permanently after
//! saying so.

#[path = "trash_timezone.rs"]
mod timezone;

use std::io;
use std::path::{Path, PathBuf};

/// Whether [`move_to_trash`] works on this platform.
pub fn available() -> bool {
    cfg!(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))
}

/// Moves `path` (a file) to the Trash; returns where it went.
pub fn move_to_trash(path: &Path) -> io::Result<PathBuf> {
    if !available() {
        return Err(io::Error::new(io::ErrorKind::Unsupported, "this system has no Trash Local Image can use"));
    }
    let path = std::path::absolute(path)?;
    let meta = std::fs::symlink_metadata(&path)?;
    if meta.is_dir() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "only files are moved to the Trash"));
    }
    let home_trash = home_trash().ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no home folder"))?;
    match trash_into(&path, &home_trash) {
        Ok(p) => Ok(p),
        // Another filesystem: its own top-level trash (renames can't cross filesystems).
        Err(e) if cross_device(&e) => {
            let top = topdir_trash(&path)?;
            trash_into(&path, &top)
        }
        Err(e) => Err(e),
    }
}

/// `$XDG_DATA_HOME/Trash`, by default `~/.local/share/Trash`.
pub fn home_trash() -> Option<PathBuf> {
    let data = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).filter(|p| p.is_absolute()).or_else(|| dirs::home_dir().map(|h| h.join(".local/share")))?;
    Some(data.join("Trash"))
}

fn cross_device(e: &io::Error) -> bool {
    // EXDEV is 18 on Linux and the BSDs.
    e.kind() == io::ErrorKind::CrossesDevices || e.raw_os_error() == Some(18)
}

#[cfg(unix)]
fn topdir_trash(path: &Path) -> io::Result<PathBuf> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt};
    let top = mount_top(path)?;
    // Our user id, as the owner of our own process entry (no libc needed).
    let uid = std::fs::metadata("/proc/self")
        .map(|m| m.uid())
        .or_else(|_| dirs::home_dir().map_or(Err(io::ErrorKind::NotFound.into()), std::fs::metadata).map(|m| m.uid()))?;
    let dir = top.join(format!(".Trash-{uid}"));
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&dir)?;
    Ok(dir)
}
#[cfg(unix)]
fn mount_top(path: &Path) -> io::Result<PathBuf> {
    use std::os::unix::fs::MetadataExt;
    let dev = std::fs::symlink_metadata(path)?.dev();
    // The mount point: the highest ancestor still on the file's device.
    let mut top = path.parent().map(Path::to_path_buf).unwrap_or_default();
    while let Some(up) = top.parent() {
        match std::fs::metadata(up) {
            Ok(m) if m.dev() == dev => top = up.to_path_buf(),
            _ => break,
        }
    }
    Ok(top)
}

#[cfg(not(unix))]
fn topdir_trash(_path: &Path) -> io::Result<PathBuf> {
    Err(io::ErrorKind::Unsupported.into())
}

/// Records and moves `path` into `trash` (`files/` and `info/`); on failure the record is
/// removed again and the file stays where it was.
fn trash_into(path: &Path, trash: &Path) -> io::Result<PathBuf> {
    let (files, info) = (trash.join("files"), trash.join("info"));
    create_private_dir(&files)?;
    create_private_dir(&info)?;
    let name = path.file_name().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no file name"))?.to_os_string();
    let record = format!("[Trash Info]\nPath={}\nDeletionDate={}\n", encode_path(path), deletion_date(std::time::SystemTime::now()));
    for n in 1..10_000u32 {
        let mut stem = name.clone();
        if n != 1 {
            stem.push(format!(".{n}"));
        }
        let mut info_name = stem.clone();
        info_name.push(".trashinfo");
        let info_path = info.join(info_name);
        // Creating the record first (exclusively) reserves the name.
        let mut f = match std::fs::OpenOptions::new().write(true).create_new(true).open(&info_path) {
            Ok(f) => f,
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        };
        let dest = files.join(&stem);
        if std::fs::symlink_metadata(&dest).is_ok() {
            let _ = std::fs::remove_file(&info_path);
            continue;
        }
        let written = io::Write::write_all(&mut f, record.as_bytes()).and_then(|()| f.sync_all());
        drop(f);
        let moved = written.and_then(|()| std::fs::rename(path, &dest));
        return match moved {
            Ok(()) => Ok(dest),
            Err(e) => {
                let _ = std::fs::remove_file(&info_path);
                Err(e)
            }
        };
    }
    Err(io::Error::new(io::ErrorKind::AlreadyExists, "the Trash has too many files with this name"))
}

fn create_private_dir(dir: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new().recursive(true).mode(0o700).create(dir)
    }
    #[cfg(not(unix))]
    {
        std::fs::create_dir_all(dir)
    }
}

/// The path as the spec wants it: absolute, percent-encoded like a URL path (`/` kept).
pub fn encode_path(path: &Path) -> String {
    let mut out = String::new();
    #[cfg(unix)]
    let bytes = {
        use std::os::unix::ffi::OsStrExt;
        path.as_os_str().as_bytes()
    };
    #[cfg(not(unix))]
    let text = path.to_string_lossy();
    #[cfg(not(unix))]
    let bytes = text.as_bytes();
    for &b in bytes {
        if b.is_ascii_alphanumeric() || b"-_.~/!$&'()*+,;=:@".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// `YYYY-MM-DDThh:mm:ss` in the filesystem user's local time, as Trash 1.0 requires.
pub fn deletion_date(t: std::time::SystemTime) -> String {
    let seconds = match t.duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d.as_secs().min(i64::MAX as u64) as i64,
        Err(_) => 0,
    };
    format_date(seconds.saturating_add(i64::from(timezone::local_offset(seconds))))
}
fn format_date(seconds: i64) -> String {
    let (days, rem) = (seconds.div_euclid(86_400), seconds.rem_euclid(86_400));
    let (y, m, d) = timezone::civil(days);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}", rem / 3600, rem % 3600 / 60, rem % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_and_paths_are_formatted_for_the_spec() {
        assert_eq!(format_date(1_760_000_000), "2025-10-09T08:53:20");
        assert_eq!(format_date(0), "1970-01-01T00:00:00");
        assert_eq!(encode_path(Path::new("/a b/ü.onnx")), "/a%20b/%C3%BC.onnx");
    }

    /// Into a trash folder of our own (never the user's): the file moves, the record names it,
    /// and a second file of the same name gets its own slot.
    #[test]
    fn files_move_into_the_trash_with_a_record() {
        if !available() {
            return;
        }
        let root = std::env::temp_dir().join(format!("li-trash-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&root).unwrap();
        let trash = root.join("Trash");
        for round in 1..=2 {
            let f = root.join("model.safetensors");
            std::fs::write(&f, b"weights").unwrap();
            let dest = trash_into(&f, &trash).unwrap();
            assert!(!f.exists() && dest.exists(), "round {round}");
            let stem = dest.file_name().unwrap().to_string_lossy().to_string();
            assert_eq!(stem, if round == 1 { "model.safetensors" } else { "model.safetensors.2" });
            let info = std::fs::read_to_string(trash.join("info").join(format!("{stem}.trashinfo"))).unwrap();
            assert!(info.starts_with("[Trash Info]\nPath=/") && info.contains("model.safetensors\nDeletionDate="), "{info}");
        }
        // a file that can't be moved leaves no record behind
        assert!(trash_into(&root.join("missing.bin"), &trash).is_err());
        assert!(!trash.join("info").join("missing.bin.trashinfo").exists());
        let _ = std::fs::remove_dir_all(root);
    }
}

#[cfg(all(test, unix))]
#[test]
fn ds_finding_trash_preserves_non_utf8_names_and_symlinks() {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target").join(format!("ds-trash-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let name = std::ffi::OsString::from_vec(b"model-\xff.onnx".to_vec());
    let file = root.join(&name);
    std::fs::write(&file, b"model").unwrap();
    let trash = root.join("Trash");
    let dest = trash_into(&file, &trash).unwrap();
    assert_eq!(dest.file_name().unwrap().as_bytes(), name.as_bytes());
    let mut info = name.clone();
    info.push(".trashinfo");
    let record = std::fs::read_to_string(trash.join("info").join(info)).unwrap();
    assert!(record.contains("model-%FF.onnx"), "{record}");
    let link = root.join("missing-link");
    std::os::unix::fs::symlink("no-such-target", &link).unwrap();
    assert_eq!(mount_top(&link).unwrap(), mount_top(&dest).unwrap());
    let moved = trash_into(&link, &trash).unwrap();
    assert_eq!(std::fs::read_link(moved).unwrap(), Path::new("no-such-target"));
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(test)]
#[test]
fn ds_finding_deletion_date_uses_process_local_timezone() {
    const CHILD: &str = "LI_DS_TRASH_TZ_CHILD";
    let t = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_760_000_000);
    if let Ok(expected) = std::env::var(CHILD) {
        assert_eq!(deletion_date(t), expected);
        return;
    }
    for (zone, expected) in [("UTC0", "2025-10-09T08:53:20"), ("EST5EDT,M3.2.0,M11.1.0", "2025-10-09T04:53:20"), ("JST-9", "2025-10-09T17:53:20")] {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "trash::ds_finding_deletion_date_uses_process_local_timezone"])
            .env("TZ", zone)
            .env(CHILD, expected)
            .output()
            .unwrap();
        assert!(status.status.success(), "{zone}: {}", String::from_utf8_lossy(&status.stderr));
    }
}
