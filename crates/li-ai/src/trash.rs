//! Moving files to the desktop Trash, in pure Rust, following the freedesktop.org Trash
//! specification 1.0 (Linux and the BSDs): the file goes to `files/` of the home trash
//! (`$XDG_DATA_HOME/Trash`) when it is on the same filesystem, else to `$topdir/.Trash-$uid` of
//! its own filesystem, with a `.trashinfo` record in `info/` so file managers can show and
//! restore it. Nothing is copied: a rename either moves the file or fails.
//!
//! Other platforms have no Trash here ([`available`] is false); callers delete permanently after
//! saying so.

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
    let dev = std::fs::metadata(path)?.dev();
    // The mount point: the highest ancestor still on the file's device.
    let mut top = path.parent().map(Path::to_path_buf).unwrap_or_default();
    while let Some(up) = top.parent() {
        match std::fs::metadata(up) {
            Ok(m) if m.dev() == dev => top = up.to_path_buf(),
            _ => break,
        }
    }
    // Our user id, as the owner of our own process entry (no libc needed).
    let uid = std::fs::metadata("/proc/self")
        .map(|m| m.uid())
        .or_else(|_| dirs::home_dir().map_or(Err(io::ErrorKind::NotFound.into()), std::fs::metadata).map(|m| m.uid()))?;
    let dir = top.join(format!(".Trash-{uid}"));
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&dir)?;
    Ok(dir)
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
    let name = path.file_name().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no file name"))?.to_string_lossy().to_string();
    let record = format!("[Trash Info]\nPath={}\nDeletionDate={}\n", encode_path(path), deletion_date(std::time::SystemTime::now()));
    for n in 1..10_000u32 {
        let stem = if n == 1 { name.clone() } else { format!("{name}.{n}") };
        let info_path = info.join(format!("{stem}.trashinfo"));
        // Creating the record first (exclusively) reserves the name.
        let mut f = match std::fs::OpenOptions::new().write(true).create_new(true).open(&info_path) {
            Ok(f) => f,
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        };
        let dest = files.join(&stem);
        if dest.exists() {
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
    for b in path.to_string_lossy().bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~/!$&'()*+,;=:@".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// `YYYY-MM-DDThh:mm:ss` (the spec asks for local time; without a time-zone database this is
/// UTC, which file managers show close enough and restore by path anyway).
pub fn deletion_date(t: std::time::SystemTime) -> String {
    let secs = t.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64;
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}", rem / 3600, rem % 3600 / 60, rem % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_and_paths_are_formatted_for_the_spec() {
        let t = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_760_000_000);
        assert_eq!(deletion_date(t), "2025-10-09T08:53:20");
        assert_eq!(deletion_date(std::time::UNIX_EPOCH), "1970-01-01T00:00:00");
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
