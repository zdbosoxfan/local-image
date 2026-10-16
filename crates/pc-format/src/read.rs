//! Reading a whole file to open it: the read side of [`crate::atomic`].
//!
//! What it adds over `std::fs::read` for multi-gigabyte PSB files:
//!
//! - **Bounded reads.** Each read asks for at most [`READ_CHUNK`] bytes. `std` asks Windows for up
//!   to 4 GiB in one `ReadFile`, and some file systems (exFAT, network redirectors) reject a read
//!   that large with `ERROR_INVALID_PARAMETER` ("The parameter is incorrect", #375).
//! - **A clear out-of-memory error.** The buffer is reserved up front with `try_reserve_exact`,
//!   and a file larger than the memory available says how much it needed.
//!
//! On `wasm32` there is no file system and every call returns the `std` "unsupported" error.

use std::io::{self, Read};
use std::path::Path;

/// Largest single read: big enough that a 5 GB file takes under a hundred reads.
pub const READ_CHUNK: usize = 64 << 20;

/// Input bytes retained by one import.
pub const MAX_FILE_BYTES: u64 = li_fs::read::MAX_FILE_BYTES;

/// A successful whole-file save must remain readable on the next open or save.
pub fn admit_output_size(bytes: u64) -> io::Result<()> {
    if bytes > MAX_FILE_BYTES {
        Err(io::Error::new(io::ErrorKind::InvalidInput, "The saved file would exceed the app's reopen limit; the destination was not changed"))
    } else {
        Ok(())
    }
}

/// Reject network/device paths and non-regular files before opening.
pub fn validate_path(path: &Path) -> io::Result<()> {
    let text = path.to_string_lossy();
    if text.starts_with("\\\\") || text.starts_with("//") || text.starts_with("/dev/") || text.starts_with("/proc/") || text.starts_with("/sys/") {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "Network and device input paths are unavailable"));
    }
    let meta = std::fs::metadata(path)?;
    if !meta.is_file() || meta.len() > MAX_FILE_BYTES {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "Input must be a regular file within the file memory limit"));
    }
    let resolved = std::fs::canonicalize(path)?;
    if resolved != path && ["/dev", "/proc", "/sys"].iter().any(|root| resolved.starts_with(root)) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "Device input paths are unavailable"));
    }
    Ok(())
}

/// Read all of `path`, in reads of at most [`READ_CHUNK`] bytes.
pub fn read_file(path: &Path) -> io::Result<Vec<u8>> {
    read_file_with(path, READ_CHUNK)
}

/// [`read_file`] with a chosen read size (tests use small ones to exercise the loop).
pub fn read_file_with(path: &Path, chunk: usize) -> io::Result<Vec<u8>> {
    validate_path(path)?;
    li_fs::read::file_with_chunk(path, MAX_FILE_BYTES, chunk)
}

/// Read `reader` to its end in reads of at most [`READ_CHUNK`] bytes, for an already open file
/// (a capability-scoped handle, say). `size_hint` (the file's length) sizes the buffer up front.
pub fn read_all(reader: &mut impl Read, size_hint: u64) -> io::Result<Vec<u8>> {
    read_all_with(reader, size_hint, READ_CHUNK)
}

fn read_all_with(reader: &mut impl Read, size_hint: u64, chunk: usize) -> io::Result<Vec<u8>> {
    // The hint only sizes the buffer: a file that grows or shrinks while being read is still read
    // to its end.
    if size_hint > MAX_FILE_BYTES {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "Input exceeds the file memory limit"));
    }
    let mut buf = buffer_for(size_hint)?;
    let chunk = u64::try_from(chunk.max(1)).unwrap_or(u64::MAX);
    loop {
        let room = MAX_FILE_BYTES.saturating_sub(buf.len() as u64);
        if room == 0 {
            let mut probe = [0];
            if reader.read(&mut probe)? != 0 {
                return Err(io::Error::new(io::ErrorKind::InvalidInput, "Input exceeds the file memory limit"));
            }
            break;
        }
        if reader.take(chunk.min(room)).read_to_end(&mut buf)? == 0 {
            break;
        }
    }
    Ok(buf)
}

/// An empty buffer with room for `len` bytes, or an `OutOfMemory` error saying how much was needed.
pub fn buffer_for(len: u64) -> io::Result<Vec<u8>> {
    let mut buf = Vec::new();
    let reserved = usize::try_from(len).ok().and_then(|n| buf.try_reserve_exact(n).ok());
    match reserved {
        Some(()) => Ok(buf),
        None => Err(io::Error::new(io::ErrorKind::OutOfMemory, format!("not enough memory to read the whole file ({})", size_text(len)))),
    }
}

/// `len` bytes for people: "4.75 GB", "759 MB", "12 KB".
fn size_text(len: u64) -> String {
    const UNITS: [&str; 5] = ["bytes", "KB", "MB", "GB", "TB"];
    let mut v = len as f64;
    let mut unit = 0;
    while v >= 1000.0 && unit + 1 < UNITS.len() {
        v /= 1000.0;
        unit += 1;
    }
    match UNITS.get(unit) {
        Some(name) if unit > 0 => format!("{v:.2} {name}"),
        _ => format!("{len} bytes"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("photocraft-read-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("f.bin");
        std::fs::write(&p, bytes).unwrap();
        p
    }

    #[test]
    fn reads_whole_file_across_many_chunks() {
        let data: Vec<u8> = (0..100_003u32).map(|i| (i * 7 % 251) as u8).collect();
        let p = temp("chunks", &data);
        for chunk in [1, 7, 4096, 100_003, 1 << 20] {
            assert_eq!(read_file_with(&p, chunk).unwrap(), data, "chunk {chunk}");
        }
        assert_eq!(read_file(&p).unwrap(), data);
        assert_eq!(read_file_with(&p, 0).unwrap(), data, "a zero chunk still makes progress");
        // From an open reader, whatever the hint says.
        for hint in [0, 10, data.len() as u64, 1 << 20] {
            assert_eq!(read_all(&mut std::io::Cursor::new(&data), hint).unwrap(), data, "hint {hint}");
        }
    }

    #[test]
    fn empty_and_missing_files() {
        assert!(read_file(&temp("empty", b"")).unwrap().is_empty());
        let missing = std::env::temp_dir().join("photocraft-read-does-not-exist.psb");
        assert_eq!(read_file(&missing).unwrap_err().kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn a_file_too_large_for_memory_is_a_clear_error() {
        // More than any address space can hold.
        let e = buffer_for(u64::MAX).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::OutOfMemory);
        assert!(e.to_string().contains("18446744.07 TB"), "{e}");
        let e = buffer_for(1 << 62).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::OutOfMemory);
        assert_eq!(buffer_for(1024).unwrap().capacity(), 1024);
    }

    #[test]
    fn sizes_read_like_the_os_shows_them() {
        assert_eq!(size_text(512), "512 bytes");
        assert_eq!(size_text(4_750_000_000), "4.75 GB");
        assert_eq!(size_text(759_000_000), "759.00 MB");
    }
}

#[cfg(test)]
mod bounded_read_regressions {
    use super::*;
    #[test]
    fn saved_file_admission_matches_the_reader_boundary() {
        assert!(admit_output_size(MAX_FILE_BYTES).is_ok());
        assert!(admit_output_size(MAX_FILE_BYTES + 1).is_err());
    }
    #[test]
    fn devices_and_network_paths_are_rejected_before_open() {
        for path in ["/dev/zero", "/proc/self/mem", "//server/share", "\\\\server\\share"] {
            assert!(read_file(Path::new(path)).is_err());
        }
    }
    #[test]
    fn sparse_oversized_file_is_rejected_before_reservation() {
        let path = std::env::temp_dir().join(format!("li-input-budget-{}", std::process::id()));
        let file = std::fs::File::create(&path).unwrap();
        file.set_len(MAX_FILE_BYTES + 1).unwrap();
        assert!(read_file(&path).is_err());
        std::fs::remove_file(path).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn fifo_is_rejected_without_waiting_for_a_writer() {
        let path = std::env::temp_dir().join(format!("li-input-fifo-{}", std::process::id()));
        assert!(std::process::Command::new("mkfifo").arg(&path).status().unwrap().success());
        assert!(read_file(&path).is_err());
        std::fs::remove_file(path).unwrap();
    }
}
