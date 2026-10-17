#![no_main]
//! Format sniffing and the headers-only import path (`probe_info`) on arbitrary bytes.

use libfuzzer_sys::fuzz_target;
use std::sync::Once;

static QUIET: Once = Once::new();

fuzz_target!(|data: &[u8]| {
    // See decode.rs: contained rawler panics must not abort the fuzzer.
    QUIET.call_once(|| std::panic::set_hook(Box::new(|_| {})));
    if lightcraft_raw::probe(data).is_some() {
        let _ = lightcraft_raw::probe_info(data);
    }
});
