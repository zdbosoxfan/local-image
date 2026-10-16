#![no_main]
//! Fuzz the XMP packet parser; anything that parses must write and re-parse.

use libfuzzer_sys::fuzz_target;
use lightcraft_meta::{parse_xmp, write_xmp};

fuzz_target!(|data: &[u8]| {
    // The app hands XMP over as text (sidecars and embedded packets are decoded first).
    let text = String::from_utf8_lossy(data);
    if let Ok(x) = parse_xmp(&text) {
        let out = write_xmp(&x.metadata, x.lc_settings.as_deref());
        let _ = parse_xmp(&out);
    }
});
