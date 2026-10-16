//! Bounded regular-file reads shared by Library and Compositing.
use lightcraft_sysmem::jobs::Interrupt;
use std::io::{self, Read};
use std::path::Path;

/// The desktop's whole-file input contract. Writers that publish documents must
/// enforce the same ceiling before replacing a destination.
pub const MAX_FILE_BYTES: u64 = 512 << 20;
/// Apply through either native or capability-based open options before validating the handle.
#[cfg(unix)]
pub const REGULAR_READ_OPEN_FLAGS: i32 = libc::O_NONBLOCK;
const CHUNK: usize = 1 << 20;

fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, "Input must be a regular file within the file memory limit")
}
fn check(interrupt: &Interrupt<'_>) -> io::Result<()> {
    interrupt.check().map_err(|_| io::Error::new(io::ErrorKind::Interrupted, "File read cancelled"))
}

/// Validate both the requested path and the opened handle. Nonblocking open
/// prevents a path changed into a FIFO between those checks from hanging.
pub fn file(path: &Path, max: u64, interrupt: &Interrupt<'_>) -> io::Result<Vec<u8>> {
    file_impl(path, max, CHUNK, interrupt)
}

pub fn file_with_chunk(path: &Path, max: u64, chunk: usize) -> io::Result<Vec<u8>> {
    file_impl(path, max, chunk, &Interrupt::NONE)
}

fn file_impl(path: &Path, max: u64, chunk: usize, interrupt: &Interrupt<'_>) -> io::Result<Vec<u8>> {
    check(interrupt)?;
    let mut input = open_regular(path, max)?;
    let metadata = input.metadata()?;
    all(&mut input, metadata.len(), max, chunk, interrupt)
}

/// Open a validated regular-file handle for bounded streaming consumers such as
/// content hashing. The caller must still bound bytes if the file grows.
pub fn open_regular(path: &Path, max: u64) -> io::Result<std::fs::File> {
    let resolved = std::fs::canonicalize(path)?;
    if ["/dev", "/proc", "/sys"].iter().any(|root| resolved.starts_with(root)) {
        return Err(invalid());
    }
    let metadata = std::fs::metadata(&resolved)?;
    if !metadata.is_file() || metadata.len() > max {
        return Err(invalid());
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(REGULAR_READ_OPEN_FLAGS);
    }
    let input = options.open(&resolved)?;
    let metadata = input.metadata()?;
    if !metadata.is_file() || metadata.len() > max {
        return Err(invalid());
    }
    Ok(input)
}

/// Bound actual bytes read, including files that grow after opening. Allocation
/// failures are returned rather than relying on infallible Vec growth.
pub fn all(reader: &mut impl Read, hint: u64, max: u64, chunk: usize, interrupt: &Interrupt<'_>) -> io::Result<Vec<u8>> {
    check(interrupt)?;
    if hint > max {
        return Err(invalid());
    }
    let mut out = Vec::new();
    let capacity = usize::try_from(hint).map_err(|_| invalid())?;
    out.try_reserve_exact(capacity).map_err(|e| io::Error::new(io::ErrorKind::OutOfMemory, e))?;
    let mut buffer = vec![0u8; chunk.clamp(1, CHUNK)];
    loop {
        check(interrupt)?;
        let room = max.saturating_sub(out.len() as u64);
        let count = usize::try_from(room).unwrap_or(usize::MAX).min(buffer.len()).max(1);
        let n = reader.read(&mut buffer[..count])?;
        if n == 0 {
            return Ok(out);
        }
        if n as u64 > room {
            return Err(invalid());
        }
        out.try_reserve(n).map_err(|e| io::Error::new(io::ErrorKind::OutOfMemory, e))?;
        out.extend_from_slice(&buffer[..n]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_bytes_and_cancellation_are_bounded() {
        let bytes = [1u8; 17];
        assert!(all(&mut &bytes[..], 0, 16, 3, &Interrupt::NONE).is_err());
        assert_eq!(all(&mut &bytes[..], 17, 17, 3, &Interrupt::NONE).unwrap(), bytes);
        assert!(all(&mut &bytes[..], 18, 17, 3, &Interrupt::NONE).is_err());
        let cancelled = || true;
        assert_eq!(all(&mut &bytes[..], 0, 17, 3, &Interrupt::cancel_only(&cancelled)).unwrap_err().kind(), io::ErrorKind::Interrupted);
    }
    #[cfg(unix)]
    #[test]
    fn fifo_and_symlink_to_fifo_are_rejected_without_a_writer() {
        let root = std::env::temp_dir().join(format!("li-safe-read-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let fifo = root.join("input.jpg");
        assert!(std::process::Command::new("mkfifo").arg(&fifo).status().unwrap().success());
        let link = root.join("link.jpg");
        std::os::unix::fs::symlink(&fifo, &link).unwrap();
        for path in [&fifo, &link, Path::new("/dev/zero")] {
            assert!(file(path, 1024, &Interrupt::NONE).is_err());
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}
