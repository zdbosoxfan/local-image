#![no_main]
//! SVG/SVGZ import of arbitrary bytes with the app's default `ImportOptions` (the limits Open,
//! Place and Paste use). The recognizer and the bounded source decoder run on the same input.

use libfuzzer_sys::fuzz_target;
use std::sync::Once;

static QUIET: Once = Once::new();

fuzz_target!(|data: &[u8]| {
    // `import` wraps usvg/conversion in catch_unwind and reports a panic as `ImportError`;
    // libfuzzer-sys' default hook aborts even on a contained panic. Silence the hook so only panics
    // that escape that boundary (which libfuzzer-sys still aborts on) are reported.
    QUIET.call_once(|| std::panic::set_hook(Box::new(|_| {})));
    let options = Default::default();
    let _ = photocraft_svg::import::recognizes(data);
    let _ = photocraft_svg::import::source(data, &options);
    let _ = photocraft_svg::import(data, &options);
});
