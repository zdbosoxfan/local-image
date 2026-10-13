//! Version-bump contract for the unmodified rawler dependency. Sensor hashes were independently
//! checked with LibRaw 0.22.2; the NEF/ARW rows lock our native decoders' existing output.
//! Fetch CC0 data with scripts/rawdecode_samples.py; the large sample tests are explicitly ignored.
use lightcraft_raw::{RawData, RawFormat, Rect, decode, probe_info};
use proptest::prelude::*;
use std::{path::PathBuf, time::Instant};

fn rows() -> impl Iterator<Item = Vec<&'static str>> {
    include_str!("../fixtures/rawler-contract.tsv").lines().skip(1).map(|line| line.split('\t').collect())
}
fn root() -> PathBuf {
    std::env::var_os("RAWLER_CONTRACT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/rawdecode"))
}
fn sample(row: &[&str]) -> Vec<u8> {
    std::fs::read(root().join(row[1])).unwrap_or_else(|e| panic!("CC0 sample {}: {e}; run python3 scripts/rawdecode_samples.py", row[1]))
}
fn fingerprint(data: &[u16]) -> u64 {
    data.iter().fold(0xcbf29ce484222325, |h, &v| (h ^ u64::from(v)).wrapping_mul(0x100000001b3))
}
fn floats(s: &str) -> Vec<f32> {
    s.split(',').map(|v| v.trim().parse().unwrap()).collect()
}
fn contract(extension: &str) {
    let mut count = 0;
    for row in rows().filter(|r| r[1].to_ascii_uppercase().ends_with(extension)) {
        let bytes = sample(&row);
        let start = Instant::now();
        let raw = decode(&bytes).unwrap_or_else(|e| panic!("{}: {e}", row[1]));
        eprintln!("{} {} sensor decode {:.1} ms", row[1], row[3], start.elapsed().as_secs_f64() * 1000.);
        assert_eq!(probe_info(&bytes).unwrap(), raw.info(), "{}: headers differ from full decode", row[1]);
        assert_eq!(raw.width, row[5].parse().unwrap());
        assert_eq!(raw.height, row[6].parse().unwrap());
        assert_eq!(raw.bits, row[7].parse().unwrap());
        assert_eq!(raw.cfa.as_ref().unwrap().name(), row[8]);
        let rect = |start: usize| {
            Rect::new(row[start].parse().unwrap(), row[start + 1].parse().unwrap(), row[start + 2].parse().unwrap(), row[start + 3].parse().unwrap())
        };
        assert_eq!(raw.active_area, rect(9));
        assert_eq!(raw.crop, rect(13));
        assert_eq!(raw.black.values, floats(row[17]));
        assert_eq!(raw.white, floats(row[18]));
        for (a, b) in raw.wb_multipliers.unwrap().iter().zip(floats(row[19])) {
            assert!((a - b).abs() < 1e-6);
        }
        assert_eq!(format!("{:?}", raw.orientation), row[20]);
        let RawData::U16(data) = &raw.data else { panic!("integer sensor") };
        assert_eq!(fingerprint(data), u64::from_str_radix(row[21], 16).unwrap(), "{}: sensor fingerprint", row[1]);
        assert!(raw.color.color_matrix.iter().any(Option::is_some));
        let transform = lightcraft_raw::color::camera_transform(&raw, lightcraft_raw::color::as_shot_white_xy(&raw));
        assert!(transform.wb.iter().all(|v| v.is_finite() && *v > 0.0));
        let developed = raw.develop_binned(12, 0.99).unwrap().unwrap();
        assert!(developed.data.iter().flatten().all(|v| v.is_finite()));
        assert!(developed.data.iter().flatten().any(|v| *v > 0.0));
        count += 1;
    }
    assert!(count > 0);
}
#[test]
#[ignore = "download CC0 files with scripts/rawdecode_samples.py"]
fn rawler_contract_cr3_cc0() {
    contract("CR3");
}
#[test]
#[ignore = "download CC0 files with scripts/rawdecode_samples.py"]
fn rawler_contract_raf_cc0() {
    contract("RAF");
}
#[test]
#[ignore = "download CC0 files with scripts/rawdecode_samples.py"]
fn rawler_contract_orf_cc0() {
    contract("ORF");
}

#[test]
#[ignore = "download CC0 files with scripts/rawdecode_samples.py"]
fn rawler_contract_native_baselines_and_decode_speed() {
    contract("NEF");
    contract("ARW");
    // Through the public decode path, in reproducible independent pools (no hardware-specific tuning).
    for threads in [1, 4] {
        let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build().unwrap();
        for id in ["4662", "4660", "6122", "1998", "898", "2414"] {
            let row = rows().find(|r| r[0] == id).unwrap();
            let bytes = sample(&row);
            let started = Instant::now();
            let raw = pool.install(|| decode(&bytes)).unwrap();
            let elapsed = started.elapsed().as_secs_f64();
            eprintln!(
                "{threads} workers {} {} {:.1} ms {:.1} MP/s",
                row[1],
                row[3],
                elapsed * 1000.,
                (raw.width * raw.height) as f64 / 1e6 / elapsed
            );
            let RawData::U16(data) = &raw.data else { panic!("integer sensor") };
            assert_eq!(fingerprint(data), u64::from_str_radix(row[21], 16).unwrap());
        }
    }
}

#[test]
#[ignore = "download CC0 files with scripts/rawdecode_samples.py"]
fn rawler_contract_truncations_mutated_containers_and_sensor_payloads() {
    for id in ["4660", "6122", "1998"] {
        let row = rows().find(|r| r[0] == id).unwrap();
        let bytes = sample(&row);
        for end in [0, 12, 108, 512, 4096, 16384, 32768, bytes.len() / 2, bytes.len() - 1] {
            let result = std::panic::catch_unwind(|| {
                let _ = probe_info(&bytes[..end.min(bytes.len())]);
                let _ = decode(&bytes[..end.min(bytes.len())]);
            });
            assert!(result.is_ok(), "{} truncation {end}", row[1]);
        }
        for at in [20, 84, 92, 100, 512, 1024, 4096, 16384] {
            let mut mutated = bytes.clone();
            mutated[at..at + 4].fill(0xff);
            assert!(
                std::panic::catch_unwind(|| {
                    let _ = probe_info(&mutated);
                })
                .is_ok(),
                "{} mutation {at}",
                row[1]
            );
        }
        // Enter the entropy decoder with bounded, valid container geometry and damaged sensor data.
        let mut mutated = bytes.clone();
        let start = mutated.len() / 2;
        mutated[start..start + 1024].fill(0xff);
        assert!(
            std::panic::catch_unwind(|| {
                let _ = decode(&mutated);
            })
            .is_ok(),
            "{} payload mutation",
            row[1]
        );
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 128, .. ProptestConfig::default() })]
    #[test]
    fn rawler_contract_random_recognized_containers_never_escape_panics(mut bytes in proptest::collection::vec(any::<u8>(),16..1024), kind in 0usize..3) {
        let magic: &[u8]=match kind {0=>b"\0\0\0\x18ftypcrx ",1=>b"FUJIFILMCCD-RAW",_=>b"IIRO\x08\0\0\0"};
        bytes[..magic.len()].copy_from_slice(magic);
        let _=probe_info(&bytes);
        let _=decode(&bytes);
    }
}
#[test]
fn rawler_contract_container_support_is_attempted_without_model_lists() {
    assert!(RawFormat::Cr3.is_supported());
    assert!(RawFormat::Orf.is_supported());
    assert!(RawFormat::OtherTiff.is_supported());
    // Malformed CR3 does not quietly claim preview-only support.
    assert!(matches!(decode(b"\0\0\0\x18ftypcrx \0\0\0\x01"), Err(lightcraft_raw::RawError::Corrupt(_))));
}
