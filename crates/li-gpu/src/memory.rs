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
    let bytes = drm_video_memory(std::path::Path::new("/sys/class/drm"), key.0, key.1);
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

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
