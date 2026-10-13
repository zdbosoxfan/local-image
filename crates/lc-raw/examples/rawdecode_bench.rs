//! Exercise the public RAW path: `cargo run --offline -p lightcraft-raw --example rawdecode_bench -- FILE...`.
//! Reports complete sensor fingerprints and header/decode timings; development uses a small sensor bin.
use lightcraft_raw::RawData;
use std::time::Instant;
fn main() {
    for path in std::env::args().skip(1) {
        let bytes = std::fs::read(&path).expect("sample");
        let start = Instant::now();
        let info = lightcraft_raw::probe_info(&bytes);
        let header = start.elapsed();
        let start = Instant::now();
        match lightcraft_raw::decode(&bytes) {
            Ok(raw) => {
                let decode = start.elapsed();
                assert_eq!(raw.info(), info.expect("header"));
                let RawData::U16(data) = &raw.data else { panic!("integer sensor") };
                let hash = data.iter().fold(0xcbf29ce484222325u64, |h, v| (h ^ u64::from(*v)).wrapping_mul(0x100000001b3));
                let start = Instant::now();
                let developed = raw.develop_binned(12, 0.99).expect("bin").expect("CFA bin");
                println!(
                    "{path}: {}x{} bits={} CFA={:?} active={:?} crop={:?} black={:?} white={:?} WB={:?} orientation={:?} sensor={hash:016x} header={:.1}ms decode={:.1}ms bin={:.1}ms output={}x{}",
                    raw.width,
                    raw.height,
                    raw.bits,
                    raw.cfa.as_ref().map(|c| c.name()),
                    raw.active_area,
                    raw.crop,
                    raw.black.values,
                    raw.white,
                    raw.wb_multipliers,
                    raw.orientation,
                    header.as_secs_f64() * 1000.,
                    decode.as_secs_f64() * 1000.,
                    start.elapsed().as_secs_f64() * 1000.,
                    developed.width,
                    developed.height
                );
            }
            Err(err) => println!("{path}: {err}; header={:?}", info.err()),
        }
    }
}
