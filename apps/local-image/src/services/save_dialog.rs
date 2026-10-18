//! Save dialog naming: the extension a chosen name gets, and Save As through the XDG desktop
//! portal on Linux (upstream PhotoCraft 26e00305, 58f855a4).
//!
//! The format follows the extension, so a name typed without one (common where file extensions
//! are hidden) gets the suggested file's. On Linux the GTK file chooser behind the portal keeps
//! the typed name when the user picks another file type, and rfd 0.17 reads only the file name
//! from the portal's answer, so `image.psd` with PNG picked was written as a PSD. The portal's
//! answer does say which type was picked (`current_filter`), so Save As asks the portal itself
//! ([`super::portal_save`]) and gives the name that type's extension ([`with_chosen_type`]). rfd
//! stays the fallback (zenity, or no portal); this can go once an rfd release reads the picked
//! type (PolyMeilex/rfd#341).

use std::path::Path;
#[cfg(any(target_os = "linux", test))]
use std::path::PathBuf;

/// `chosen` (what the save dialog answered) with `suggested`'s extension when it has none.
pub(super) fn with_suggested_extension(chosen: String, suggested: &str) -> String {
    let Some(ext) = Path::new(suggested).extension().filter(|e| !e.is_empty()) else { return chosen };
    if Path::new(&chosen).extension().is_some() {
        return chosen;
    }
    let mut name = std::ffi::OsString::from(chosen);
    name.push(".");
    name.push(ext.to_ascii_lowercase());
    name.to_string_lossy().into_owned()
}

/// One file type in the dialog: its label and extensions (`("PNG", ["png"])`).
#[cfg(any(target_os = "linux", test))]
pub(super) type Filter = (String, Vec<String>);

/// A Save dialog to show.
#[cfg(any(target_os = "linux", test))]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct SaveRequest {
    /// The name the dialog shows: see [`dialog_file_name`].
    pub name: Option<String>,
    /// The suggested file's name (`test.psd`): a name that comes back with its extension still
    /// takes the extension of the type picked ([`with_chosen_type`]).
    pub suggested: Option<String>,
    pub folder: Option<PathBuf>,
    pub filters: Vec<Filter>,
    /// The filter the dialog starts on.
    pub current: Option<usize>,
}

/// What the user chose: the path, as typed, and the label of the file type picked.
#[cfg(any(target_os = "linux", test))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Saved {
    pub path: PathBuf,
    pub filter: Option<String>,
}

/// Whether Local Image saves files with extension `ext` (lower case).
#[cfg(any(target_os = "linux", test))]
fn saved_extension(ext: &str) -> bool {
    super::SAVE_FILTERS.iter().any(|(_, exts)| exts.contains(&ext)) || photocraft_codecs::writable_extensions().contains(&ext)
}

/// `path` with the extension of the file type picked in the save dialog (`chosen`), where the
/// dialog left the name alone. Only a name that still has the extension the dialog opened with
/// (`opened_with`: `Untitled-1.psd` with PNG picked) or none gets the type's extension, as
/// PolyMeilex/rfd#341 does. An extension the user typed that Local Image saves (`art.png` with the
/// Photoshop type untouched) is theirs and stays, and so does one of the type's own (`.psb` for
/// Photoshop). One Local Image doesn't save (`scan.2024`) is part of the name, and the type's
/// extension is added after it.
#[cfg(any(target_os = "linux", test))]
pub(super) fn with_chosen_type(path: &Path, opened_with: Option<&str>, chosen: &[String]) -> PathBuf {
    let Some(first) = chosen.first() else { return path.to_path_buf() };
    let ext = path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
    let opened_ext = opened_with.and_then(|n| Path::new(n).extension()).and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
    match ext {
        Some(ext) if chosen.contains(&ext) => path.to_path_buf(),
        Some(ext) if opened_ext.as_ref() == Some(&ext) => path.with_extension(first),
        Some(ext) if saved_extension(&ext) => path.to_path_buf(),
        _ => {
            let mut name = path.as_os_str().to_owned();
            name.push(".");
            name.push(first);
            PathBuf::from(name)
        }
    }
}

/// The name the portal's Save dialog starts with: the stem, so the type picked supplies the
/// extension. The portal's dialog keeps the name as typed and, before returning, asks whether to
/// replace a file of that name, so `test.psd` with PNG picked asked to replace test.psd, which the
/// save leaves alone. A stem that still has a dot (`scan.jpg.psd`) or an upper-case extension
/// (the user's spelling) keeps the whole name.
#[cfg(any(target_os = "linux", test))]
pub(super) fn dialog_file_name(suggested: &str) -> Option<String> {
    let path = Path::new(suggested);
    let name = path.file_name()?.to_string_lossy().into_owned();
    let (Some(stem), Some(ext)) = (path.file_stem(), path.extension().and_then(|e| e.to_str())) else { return Some(name) };
    let bare = Path::new(stem).extension().is_none() && !ext.is_empty() && ext == ext.to_ascii_lowercase();
    Some(if bare { stem.to_string_lossy().into_owned() } else { name })
}

/// Save As through a dialog that reports the type picked (`show`, the portal's): the name gets
/// that type's extension. When that changes the name to one that already exists, the dialog opens
/// again under the new name, so the dialog asks before replacing it (it asked only about the
/// typed name). `Ok(None)` is Cancel; `Err` means the dialog couldn't be shown.
#[cfg(any(target_os = "linux", test))]
pub(super) fn save_as(mut show: impl FnMut(&SaveRequest) -> Result<Option<Saved>, String>, mut req: SaveRequest) -> Result<Option<PathBuf>, String> {
    loop {
        let Some(saved) = show(&req)? else { return Ok(None) };
        let picked = saved.filter.as_deref().and_then(|label| req.filters.iter().position(|(l, _)| l == label));
        // With one type offered there is nothing to pick, and a typed extension is the user's.
        let exts: &[String] = match picked {
            Some(i) if req.filters.len() > 1 => &req.filters[i].1,
            _ => &[],
        };
        let path = with_chosen_type(&saved.path, req.suggested.as_deref(), exts);
        if path == saved.path || !path.exists() {
            return Ok(Some(path));
        }
        req.name = path.file_name().map(|n| n.to_string_lossy().into_owned());
        req.suggested.clone_from(&req.name);
        req.folder = path.parent().map(Path::to_path_buf);
        req.current = picked;
    }
}

/// `file:///home/me/A%20b.psd` → `/home/me/A b.psd`; `None` for other schemes and hosts.
#[cfg(any(target_os = "linux", all(test, unix)))]
pub(super) fn file_uri_path(uri: &str) -> Option<PathBuf> {
    use std::os::unix::ffi::OsStringExt;
    let rest = uri.strip_prefix("file://")?;
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    if !rest.starts_with('/') {
        return None;
    }
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(b) = bytes.get(i + 1..i + 3).and_then(|h| std::str::from_utf8(h).ok()).and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(b);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    Some(PathBuf::from(std::ffi::OsString::from_vec(out)))
}

#[cfg(test)]
mod tests;
