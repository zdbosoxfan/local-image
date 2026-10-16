#![no_main]
//! Fuzz the IPTC-IIM dataset stream parser: any input must return, never panic.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = lightcraft_meta::parse_iptc(data);
});
