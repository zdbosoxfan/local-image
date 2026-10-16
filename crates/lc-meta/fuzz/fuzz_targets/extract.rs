#![no_main]
//! Whole-file metadata extraction: container walking (JPEG, PNG, WebP, TIFF-based, CR3) feeding the
//! EXIF, IPTC and XMP readers together.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = lightcraft_meta::extract(data);
    let _ = lightcraft_meta::embedded(data);
});
