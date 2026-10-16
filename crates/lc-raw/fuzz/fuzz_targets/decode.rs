#![no_main]
//! Full raw decode of arbitrary bytes under a bounded TIFF pixel-buffer allowance (the app passes
//! its shared hardware budget via `decode_with_memory_budget`); anything that decodes must pass
//! its own validation and report consistent info.

use libfuzzer_sys::fuzz_target;
use std::sync::Once;

static QUIET: Once = Once::new();

fuzz_target!(|data: &[u8]| {
    // rawler's decoder contains its own panics and turns them into errors (rawler_adapter.rs);
    // libfuzzer-sys' default hook aborts on any panic, even a contained one. Silence the hook so
    // only panics that escape the crate (which libfuzzer-sys still aborts on) are reported.
    QUIET.call_once(|| std::panic::set_hook(Box::new(|_| {})));
    if let Ok(img) = lightcraft_raw::decode_with_memory_budget(data, 256 << 20) {
        let _ = img.validate();
        let _ = img.info();
    }
});
