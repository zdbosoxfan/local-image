//! PhotoCraft 652b9726 (MIT/Apache-2.0), safe XWayland startup detection.
/// `BTN_TOOL_PEN` (linux/input-event-codes.h): the key a pen tablet's stylus, a pen display's or
/// a pen-enabled touchscreen's reports (libinput treats such a device as a tablet tool).
const BTN_TOOL_PEN: u32 = 0x140;

/// Is a pen among the input devices `/proc/bus/input/devices` lists? Each device's `B: KEY=` line
/// is its key bitmap as hex words of the kernel's `long`, most significant first.
pub fn has_pen(devices: &str) -> bool {
    let bits = usize::BITS;
    let word = usize::try_from(BTN_TOOL_PEN / bits).unwrap_or(usize::MAX);
    devices.lines().filter_map(|l| l.strip_prefix("B: KEY=")).any(|keys| {
        keys.split_whitespace().rev().nth(word).and_then(|w| u64::from_str_radix(w, 16).ok()).is_some_and(|w| (w >> (BTN_TOOL_PEN % bits)) & 1 == 1)
    })
}

/// Auto detects attached pens; On explicitly uses compatibility; Off keeps native Wayland.
/// An explicit native opt-out always wins. Probes run only when needed.
pub fn compatibility_mode(mode: &str, wayland: bool, native: bool, display: bool, pen: impl FnOnce() -> bool, libs: impl FnOnce() -> bool) -> bool {
    wayland && !native && mode != "off" && display && (mode == "on" || pen()) && libs()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pen_bitmap_and_compatibility_modes() {
        assert!(has_pen("B: KEY=1c03 0 0 0 0 0"));
        assert!(!has_pen("B: KEY=e520 10000 0 0 0 0"));
        assert!(!has_pen("B: KEY=zz 0 0 0 0 0"));
        assert!(compatibility_mode("auto", true, false, true, || true, || true));
        assert!(!compatibility_mode("auto", true, false, true, || false, || true));
        assert!(compatibility_mode("on", true, false, true, || panic!("unneeded"), || true));
        assert!(!compatibility_mode("off", true, false, true, || panic!("unneeded"), || true));
        assert!(!compatibility_mode("on", true, true, true, || panic!("unneeded"), || true));
        assert!(!compatibility_mode("on", true, false, false, || panic!("unneeded"), || true));
        assert!(!compatibility_mode("on", true, false, true, || true, || false));
    }
}
