//! File access behind the shell's Open Folder and the background-removal batch (File ›
//! Automate › Remove Backgrounds): folder listing in natural order, whole-file and size-limited
//! reads, output naming on disk, and the guard that keeps a batch from overwriting its own input
//! photos. Plain functions, so the UI does no file IO of its own.

use std::path::{Path, PathBuf};

/// Reads a whole file; the error is the IO error's message (for the status bar).
pub fn read_file(path: &Path) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|e| e.to_string())
}

/// The images in `dir` Local Image can open (readable extensions and `.pcraft`), in natural order.
/// An unreadable folder lists nothing.
pub fn folder_images(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.is_file()
                        && p.extension()
                            .and_then(|e| e.to_str())
                            .is_some_and(|e| e.eq_ignore_ascii_case("pcraft") || photocraft_codecs::is_readable_extension(e, true))
                })
                .collect()
        })
        .unwrap_or_default();
    files.sort_by(|a, b| natural_cmp(&a.to_string_lossy(), &b.to_string_lossy()));
    files
}

/// Natural sort: "img2" before "img10".
pub fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let (mut ai, mut bi) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (ai.peek().copied(), bi.peek().copied()) {
            (None, None) => return std::cmp::Ordering::Equal,
            (None, _) => return std::cmp::Ordering::Less,
            (_, None) => return std::cmp::Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let mut na = String::new();
                while let Some(c) = ai.peek().copied().filter(char::is_ascii_digit) {
                    na.push(c);
                    ai.next();
                }
                let mut nb = String::new();
                while let Some(c) = bi.peek().copied().filter(char::is_ascii_digit) {
                    nb.push(c);
                    bi.next();
                }
                let o = na
                    .trim_start_matches('0')
                    .len()
                    .cmp(&nb.trim_start_matches('0').len())
                    .then_with(|| na.trim_start_matches('0').cmp(nb.trim_start_matches('0')));
                if o != std::cmp::Ordering::Equal {
                    return o;
                }
            }
            (Some(x), Some(y)) => {
                let o = x.to_lowercase().cmp(y.to_lowercase());
                if o != std::cmp::Ordering::Equal {
                    return o;
                }
                ai.next();
                bi.next();
            }
        }
    }
}

/// Reads a batch input photo, refusing files larger than the import memory limit
/// ([`photocraft_format::read::MAX_FILE_BYTES`]) without reading past it.
pub fn read_input(input: &Path) -> anyhow::Result<Vec<u8>> {
    use std::io::Read;
    let limit = photocraft_format::read::MAX_FILE_BYTES;
    let mut bytes = Vec::new();
    std::fs::File::open(input)?.take(limit + 1).read_to_end(&mut bytes)?;
    anyhow::ensure!(bytes.len() as u64 <= limit, "Image exceeds the file memory limit");
    Ok(bytes)
}

/// Where a batch output named `name` goes in `dir` when that file may already exist: `conflict`
/// "overwrite" replaces it, "skip" gives `None`, anything else picks the first free
/// `stem-2.ext`, `stem-3.ext`, ….
pub fn batch_destination(dir: &Path, name: &str, conflict: &str) -> Option<PathBuf> {
    let dest = dir.join(name);
    if !dest.exists() || conflict == "overwrite" {
        return Some(dest);
    }
    if conflict == "skip" {
        return None;
    }
    let stem = dest.file_stem()?.to_string_lossy();
    let ext = dest.extension()?.to_string_lossy();
    (2..).map(|n| dir.join(format!("{stem}-{n}.{ext}"))).find(|p| !p.exists())
}

/// Creates the output folder `dir` and resolves the destination of `name` in it
/// ([`batch_destination`]; `None` = skip). Fails when that destination is one of the batch's
/// `inputs`, by path or by what it resolves to.
pub fn prepare_output(dir: &Path, name: &str, conflict: &str, inputs: &[PathBuf]) -> anyhow::Result<Option<PathBuf>> {
    std::fs::create_dir_all(dir)?;
    let Some(dest) = batch_destination(dir, name, conflict) else {
        return Ok(None);
    };
    // A template can target a different photo later in the same list.
    let canonical_dest = dest.exists().then(|| std::fs::canonicalize(&dest)).transpose()?;
    anyhow::ensure!(
        !inputs
            .iter()
            .any(|source| source == &dest || canonical_dest.as_ref().is_some_and(|target| std::fs::canonicalize(source).ok().as_ref() == Some(target))),
        "Output would overwrite an input photo"
    );
    Ok(Some(dest))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh, empty folder under the system temp dir, removed when dropped.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("pc-batch-io-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn natural_cmp_orders_numbers_by_value_and_ignores_case() {
        let mut v = vec!["img10.jpg", "img2.jpg", "IMG1.jpg", "a.png", "img02.jpg"];
        v.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(v, vec!["a.png", "IMG1.jpg", "img2.jpg", "img02.jpg", "img10.jpg"]);
    }

    #[test]
    fn folder_images_lists_readable_files_in_natural_order() {
        let tmp = TempDir::new("folder");
        for name in ["b10.png", "b2.JPG", "a.pcraft", "raw.CRW", "notes.txt", "noext"] {
            std::fs::write(tmp.0.join(name), []).unwrap();
        }
        std::fs::create_dir(tmp.0.join("sub.png")).unwrap();
        let names: Vec<String> = folder_images(&tmp.0).iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(names, ["a.pcraft", "b2.JPG", "b10.png", "raw.CRW"]);
        assert!(folder_images(&tmp.0.join("missing")).is_empty());
    }

    #[test]
    fn read_file_returns_bytes_or_the_io_message() {
        let tmp = TempDir::new("read");
        let path = tmp.0.join("a.bin");
        std::fs::write(&path, b"abc").unwrap();
        assert_eq!(read_file(&path).unwrap(), b"abc");
        let missing = tmp.0.join("missing.bin");
        assert_eq!(read_file(&missing).unwrap_err(), std::fs::read(&missing).unwrap_err().to_string());
    }

    #[test]
    fn read_input_reads_within_the_limit_and_reports_missing_files() {
        let tmp = TempDir::new("input");
        let path = tmp.0.join("photo.png");
        std::fs::write(&path, b"pixels").unwrap();
        assert_eq!(read_input(&path).unwrap(), b"pixels");
        assert!(read_input(&tmp.0.join("missing.png")).is_err());
    }

    #[test]
    fn batch_destination_overwrites_skips_or_numbers_existing_outputs() {
        let tmp = TempDir::new("dest");
        let free = batch_destination(&tmp.0, "cat.psd", "skip");
        assert_eq!(free, Some(tmp.0.join("cat.psd")), "a free name is used whatever the conflict rule");
        std::fs::write(tmp.0.join("cat.psd"), b"existing").unwrap();
        assert_eq!(batch_destination(&tmp.0, "cat.psd", "overwrite"), Some(tmp.0.join("cat.psd")));
        assert_eq!(batch_destination(&tmp.0, "cat.psd", "skip"), None);
        assert_eq!(batch_destination(&tmp.0, "cat.psd", "rename"), Some(tmp.0.join("cat-2.psd")));
        std::fs::write(tmp.0.join("cat-2.psd"), b"existing").unwrap();
        assert_eq!(batch_destination(&tmp.0, "cat.psd", "rename"), Some(tmp.0.join("cat-3.psd")));
    }

    #[test]
    fn prepare_output_creates_the_folder_and_never_targets_an_input() {
        let tmp = TempDir::new("prepare");
        let out = tmp.0.join("outputs");
        let inputs = vec![tmp.0.join("1.png"), tmp.0.join("2.png")];
        for input in &inputs {
            std::fs::write(input, b"original").unwrap();
        }
        assert_eq!(prepare_output(&out, "1.png", "overwrite", &inputs).unwrap(), Some(out.join("1.png")));
        assert!(out.is_dir());
        std::fs::write(out.join("1.png"), b"done").unwrap();
        assert_eq!(prepare_output(&out, "1.png", "skip", &inputs).unwrap(), None);
        // Writing next to the inputs: the same path, or another spelling of it, is refused.
        let err = prepare_output(&tmp.0, "2.png", "overwrite", &inputs).unwrap_err();
        assert_eq!(err.to_string(), "Output would overwrite an input photo");
        let respelled = vec![out.join("..").join("2.png")];
        assert!(prepare_output(&tmp.0, "2.png", "overwrite", &respelled).is_err());
        // Renaming around an existing input is fine.
        assert_eq!(prepare_output(&tmp.0, "2.png", "rename", &inputs).unwrap(), Some(tmp.0.join("2-2.png")));
    }
}
