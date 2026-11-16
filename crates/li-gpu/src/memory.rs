//! Dedicated adapter memory reported by the operating system.

/// Dedicated VRAM, when known. Integrated adapters continue using the host-memory budget.
/// `PHOTOCRAFT_VRAM_MB` simulates a discrete card, even on integrated hardware; zero means unknown.
pub fn video_memory(adapter: &wgpu::AdapterInfo) -> Option<u64> {
    if let Some(mb) = std::env::var("PHOTOCRAFT_VRAM_MB").ok().and_then(|s| s.trim().parse::<u64>().ok()) {
        return mb.checked_mul(1 << 20).filter(|bytes| *bytes > 0);
    }
    if adapter.device_type != wgpu::DeviceType::DiscreteGpu {
        return None;
    }
    type Seen = Vec<((u32, u32), Option<u64>)>;
    static SEEN: std::sync::Mutex<Seen> = std::sync::Mutex::new(Vec::new());
    let key = (adapter.vendor, adapter.device);
    let mut seen = SEEN.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some((_, bytes)) = seen.iter().find(|(id, _)| *id == key) {
        return *bytes;
    }
    #[cfg(target_os = "linux")]
    let bytes = drm_video_memory(std::path::Path::new("/sys/class/drm"), key.0, key.1).or_else(|| nvidia_video_memory(key.0, key.1));
    #[cfg(not(target_os = "linux"))]
    let bytes = None;
    seen.push((key, bytes));
    bytes
}

#[cfg(any(target_os = "linux", test))]
fn drm_video_memory(root: &std::path::Path, vendor: u32, device: u32) -> Option<u64> {
    let hex = |path| u32::from_str_radix(std::fs::read_to_string(path).ok()?.trim().trim_start_matches("0x"), 16).ok();
    std::fs::read_dir(root).ok()?.flatten().find_map(|card| {
        let dev = card.path().join("device");
        if hex(dev.join("vendor"))? != vendor || hex(dev.join("device"))? != device {
            return None;
        }
        std::fs::read_to_string(dev.join("mem_info_vram_total")).ok()?.trim().parse::<u64>().ok().filter(|b| *b > 0)
    })
}

/// PCI vendor id of NVIDIA, whose proprietary Linux driver writes no `mem_info_vram_total`.
#[cfg(any(target_os = "linux", test))]
const NVIDIA_VENDOR: u32 = 0x10de;

/// How long `nvidia-smi` gets to answer: it runs while the window opens, under the cache lock.
#[cfg(target_os = "linux")]
const NVIDIA_SMI_DEADLINE: std::time::Duration = std::time::Duration::from_secs(2);

/// NVIDIA's own memory size on Linux, from the `nvidia-smi` its driver installs. Absent, slower
/// than [`NVIDIA_SMI_DEADLINE`] or odd output all read as `None`.
#[cfg(target_os = "linux")]
fn nvidia_video_memory(vendor: u32, device: u32) -> Option<u64> {
    if vendor != NVIDIA_VENDOR {
        return None;
    }
    let mut cmd = std::process::Command::new("nvidia-smi");
    cmd.args(["--query-gpu=pci.device_id,memory.total", "--format=csv,noheader,nounits"]);
    let out = bounded_output(cmd, NVIDIA_SMI_DEADLINE)?;
    parse_nvidia_smi(&String::from_utf8_lossy(&out), vendor, device)
}

/// The first 64 KiB of `cmd`'s stdout when it exits successfully within `deadline`; `None` when
/// it can't start, fails or is still running by then. A late one is killed and reaped on a thread
/// of its own (a process stuck in the driver may not die at once).
#[cfg(any(target_os = "linux", test))]
fn bounded_output(mut cmd: std::process::Command, deadline: std::time::Duration) -> Option<Vec<u8>> {
    use std::io::Read;
    use std::process::{Child, Stdio};
    use std::time::{Duration, Instant};
    fn abandon(mut child: Child) {
        let _ = child.kill();
        let _ = std::thread::Builder::new().name("reap".into()).spawn(move || child.wait());
    }
    let end = Instant::now().checked_add(deadline)?;
    let mut child = cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok()?;
    let Some(stdout) = child.stdout.take() else {
        abandon(child);
        return None;
    };
    let (tx, rx) = std::sync::mpsc::channel();
    let reader = std::thread::Builder::new().name("child-stdout".into()).spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout.take(64 << 10).read_to_end(&mut buf);
        let _ = tx.send(buf);
    });
    if reader.is_err() {
        abandon(child);
        return None;
    }
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < end => std::thread::sleep(Duration::from_millis(10)),
            _ => {
                abandon(child);
                return None;
            }
        }
    };
    if !status.success() {
        return None;
    }
    // The child has exited, so its stdout is at EOF unless it handed the pipe to a process of its
    // own: bounded too, instead of joining the reader.
    rx.recv_timeout(end.saturating_duration_since(Instant::now()).max(Duration::from_millis(50))).ok()
}

/// `nvidia-smi --query-gpu=pci.device_id,memory.total --format=csv,noheader,nounits` prints one
/// `0xDDDDVVVV, size-in-MiB` line per card (device id the high half, vendor id the low). The
/// matching line's size in bytes, the smallest when several cards share the ids (they can't tell
/// which one wgpu picked).
#[cfg(any(target_os = "linux", test))]
fn parse_nvidia_smi(output: &str, vendor: u32, device: u32) -> Option<u64> {
    output
        .lines()
        .filter_map(|line| {
            let (id, mb) = line.split_once(',')?;
            let id = u32::from_str_radix(id.trim().trim_start_matches("0x").trim_start_matches("0X"), 16).ok()?;
            if id & 0xffff != vendor || id >> 16 != device {
                return None;
            }
            mb.trim().parse::<u64>().ok()?.checked_mul(1 << 20).filter(|b| *b > 0)
        })
        .min()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// Real output is `0x2C0510DE, 16303`. Cards sharing an id report the smaller memory; a bad
    /// or zero line is skipped.
    #[test]
    fn parses_nvidia_smis_report_of_the_cards_memory() {
        let two = "0x2C0510DE, 16303\n0x061C10DE, 24576\n";
        assert_eq!(parse_nvidia_smi(two, NVIDIA_VENDOR, 0x2c05), Some(16303 << 20));
        assert_eq!(parse_nvidia_smi(two, NVIDIA_VENDOR, 0x061c), Some(24576 << 20));
        assert_eq!(parse_nvidia_smi("0X2C0510DE,   16303  \n", NVIDIA_VENDOR, 0x2c05), Some(16303 << 20));
        assert_eq!(parse_nvidia_smi(two, NVIDIA_VENDOR, 0xdead), None);
        assert_eq!(parse_nvidia_smi(two, 0x1002, 0x2c05), None);
        let overflows = format!("0x2C0510DE, {}\n", u64::MAX);
        for bad in ["not csv at all", "0x2C0510DE 16303\n", "0x2C0510DE, lots\n", "", overflows.as_str()] {
            assert_eq!(parse_nvidia_smi(bad, NVIDIA_VENDOR, 0x2c05), None, "{bad:?}");
        }
        assert_eq!(parse_nvidia_smi("0x2C0510DE, 16303\n0x2C0510DE, 8192\n", NVIDIA_VENDOR, 0x2c05), Some(8192 << 20));
        assert_eq!(parse_nvidia_smi("0x2C0510DE, 0\n0x2C0510DE, lots\n0x2C0510DE, 16303\n", NVIDIA_VENDOR, 0x2c05), Some(16303 << 20));
    }

    /// The query runs while the window opens: one that never answers gives up at the deadline.
    #[cfg(unix)]
    #[test]
    fn a_child_that_never_answers_gives_up_at_the_deadline() {
        let mut slow = std::process::Command::new("sleep");
        slow.arg("10");
        let start = std::time::Instant::now();
        assert_eq!(bounded_output(slow, std::time::Duration::from_millis(200)), None);
        assert!(start.elapsed() < std::time::Duration::from_secs(2), "took {:?}", start.elapsed());
        let mut quick = std::process::Command::new("printf");
        quick.arg("0x2C0510DE, 16303\n");
        let out = bounded_output(quick, std::time::Duration::from_secs(10)).unwrap();
        assert_eq!(parse_nvidia_smi(&String::from_utf8_lossy(&out), NVIDIA_VENDOR, 0x2c05), Some(16303 << 20));
        assert_eq!(bounded_output(std::process::Command::new("false"), std::time::Duration::from_secs(10)), None);
        assert_eq!(bounded_output(std::process::Command::new("li-gpu-no-such-tool"), std::time::Duration::from_secs(1)), None);
    }

    #[test]
    fn drm_memory_matches_pci_ids_and_ignores_missing_or_malformed_values() {
        let root = std::env::temp_dir().join(format!("li-gpu-vram-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for (name, vendor, device, bytes) in [
            ("card0", "0x1002", "0x73bf", Some("17163091968\n")),
            ("card1", "0x8086", "0x9a49", None),
            ("card2", "0x10de", "0x2684", Some("garbage")),
            ("card3", "garbage", "0x73bf", Some("100")),
            ("card4", "0x1002", "0x1234", Some("0")),
        ] {
            let dev = root.join(name).join("device");
            std::fs::create_dir_all(&dev).unwrap();
            std::fs::write(dev.join("vendor"), vendor).unwrap();
            std::fs::write(dev.join("device"), device).unwrap();
            if let Some(bytes) = bytes {
                std::fs::write(dev.join("mem_info_vram_total"), bytes).unwrap();
            }
        }
        assert_eq!(drm_video_memory(&root, 0x1002, 0x73bf), Some(17_163_091_968));
        for (vendor, device) in [(0x8086, 0x9a49), (0x10de, 0x2684), (0xdead, 0xbeef), (0x1002, 0x1234)] {
            assert_eq!(drm_video_memory(&root, vendor, device), None);
        }
        assert_eq!(drm_video_memory(&root.join("missing"), 0x1002, 0x73bf), None);
        std::fs::remove_dir_all(root).unwrap();
    }
}
