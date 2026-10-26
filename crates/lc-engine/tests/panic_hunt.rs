//! Library's opt-in full-registry command fuzz, sharing Compositing's isolated worker runner.
//! `cargo +1.98.1 test --offline -p lightcraft-engine --test panic_hunt -- --ignored --nocapture`
#![expect(clippy::let_underscore_must_use, reason = "baseline 2026-10-25: unreviewed")]

#[path = "../../../tests/support/panic_hunt.rs"]
mod guard;

use lightcraft_codecs::{EncodeImage, EncodeMeta, Samples, encode_png};
use lightcraft_engine::{Session, command_specs};
use serde_json::{Value, json};
use std::path::Path;

const DENIED: &[(&str, &str)] =
    &[("segment.model.download", "downloads the SAM model from the network"), ("library.devices", "scans the owner's mounted camera/card volumes")];

fn fresh(root: &Path) -> Session {
    let mut s = Session::new().with_fs();
    s.open_library(root.join("library"), false).unwrap();
    s.quick_seg_dir = Some(root.join("models"));
    s.segmenter.dir = Some(root.join("models"));
    let bytes = encode_png(&EncodeImage::new(24, 16, 3, Samples::U8(&[128; 24 * 16 * 3])), &EncodeMeta::default()).unwrap();
    let src = root.join("input.png");
    std::fs::write(&src, bytes).unwrap();
    let r = s.execute("library.import", &json!({"paths": [src]})).unwrap();
    s.execute("library.select", &json!({"ids": r["imported"]})).unwrap();
    s.execute("mask.add", &json!({"kind": "brush"})).unwrap();
    s
}

fn confined_params(id: &str, p: &Value, root: &Path) -> Value {
    let mut p = p.clone();
    match id {
        "preset.export" => p["path"] = json!(root.join("output/presets.lcpreset")),
        "library.smartPreviewsLocation" => p["path"] = json!(root.join("output/smart-previews")),
        "folder.move" => {
            p["path"] = json!(root.join("output"));
            p["into"] = json!(root.join("destination"));
        }
        "folder.rename" => p["path"] = json!(root.join("output")),
        "smartSort.export" => p["dir"] = json!(root.join("output")),
        "library.import" => p["destination"] = json!(root.join("output")),
        _ => {}
    }
    // Other writers (XMP, rename, save-over, previews, preferences) use the imported source
    // and the library, both created above under root. No owner's catalog or models are loaded.
    p
}

#[test]
#[ignore = "entry point for an isolated panic_hunt case"]
fn panic_hunt_child() {
    guard::child(|id, p, root| {
        assert!(!DENIED.iter().any(|(denied, _)| *denied == id), "a denied command reached the worker");
        let mut s = fresh(root);
        eprintln!("worker executing {id}");
        if id == "fixture.xmp" {
            let r = s.execute("photo.saveMetadataToFile", p).unwrap();
            let written = r["written"].as_array().unwrap();
            assert_eq!(written.len(), 1);
            let path = Path::new(written[0].as_str().unwrap());
            assert!(path.starts_with(root));
            assert!(!std::fs::read(path).unwrap().is_empty());
            return;
        }
        let _ = s.execute(id, &confined_params(id, p, root));
    });
}

#[test]
#[ignore = "slow full-registry fuzz; run with --ignored"]
fn panic_hunt_no_command_panics_or_hangs_on_adversarial_params() {
    let mut bad = Vec::new();
    let mut cases = 0;
    for spec in command_specs() {
        if let Some((_, reason)) = DENIED.iter().find(|(id, _)| *id == spec.id) {
            eprintln!("skip {}: {reason}", spec.id);
            continue;
        }
        for p in guard::params() {
            cases += 1;
            // These two commands initialize the device and compile all kernels on first use.
            // Every isolated child has an empty driver cache: the RTX 5090 diagnostic took
            // 2.7–3.7 s without contention, so a four-second deadline produced false positives.
            let r = if matches!(spec.id, "spot.findDust" | "app.gpu") {
                guard::run_timeout("panic_hunt_child", spec.id, &p, std::time::Duration::from_secs(15))
            } else {
                guard::run("panic_hunt_child", spec.id, &p)
            };
            if let Err(why) = r {
                eprintln!("{} <- {p}: {why}", spec.id);
                bad.push(format!("{} <- {p}: {why}", spec.id));
                break;
            }
        }
    }
    eprintln!("panic_hunt: {} commands, {} denied, {cases} cases", command_specs().len(), DENIED.len());
    assert!(bad.is_empty(), "{} commands panicked or hung:\n{}", bad.len(), bad.join("\n"));
}

#[test]
fn panic_hunt_deny_list_is_current() {
    for &(id, _) in DENIED {
        assert!(command_specs().iter().any(|spec| spec.id == id), "stale deny-list entry: {id}");
        let e = guard::run("panic_hunt_child", id, &json!({"acknowledged": true})).unwrap_err();
        assert!(e.contains("a denied command reached the worker"), "{e}");
    }
}

#[test]
fn panic_hunt_guard_detects_panics_and_reaps_timeouts() {
    guard::check_runner("panic_hunt_child");
    guard::run("panic_hunt_child", "fixture.xmp", &json!({})).unwrap();
}
