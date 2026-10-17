#![no_main]
//! Fuzz the EXIF reader (with and without the `Exif\0\0` header), and re-read anything it writes.

use libfuzzer_sys::fuzz_target;
use lightcraft_meta::{read_exif, strip_exif_header, try_read_exif, write_exif};

fuzz_target!(|data: &[u8]| {
    let _ = try_read_exif(data);
    let meta = read_exif(data);
    let _ = read_exif(strip_exif_header(data));
    // Whatever was read must serialise, and the output must parse again.
    let _ = read_exif(&write_exif(&meta));
});
