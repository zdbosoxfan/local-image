//! Opt-in command fuzz. External effects are denied, writes stay in disposable directories,
//! and each case runs in a child test process that is killed and reaped on timeout.
//! `cargo +1.98.1 test --offline -p photocraft-engine --test it panic_hunt -- --ignored --nocapture`

#[path = "../../../../tests/support/panic_hunt.rs"]
mod guard;

use photocraft_engine::{EngineError, Session, command_specs};
use serde_json::{Value, json};
use std::path::Path;

/// Audited against layer_menu_cmds::spawn, print_cmds::spool_pdf and ai_cmds. File dialogs
/// belong to the shell; registered engine commands take paths rather than opening dialogs.
const DENIED: &[(&str, &str)] = &[
    ("layer.smartObjects.revealInFinder", "opens the platform file manager"),
    ("file.print", "runs the system print spooler"),
    ("file.printOneCopy", "runs the system print spooler"),
    ("ai.remove", "submits work to ComfyUI"),
    ("ai.removeBackground", "submits work to ComfyUI"),
    ("ai.selectSubject", "submits work to ComfyUI"),
    ("ai.generativeFill", "submits work to ComfyUI"),
    ("ai.generateBackground", "submits work to ComfyUI"),
    ("ai.enhance", "submits work to ComfyUI"),
];

fn authorize(id: &str, _: &Value) -> photocraft_engine::Result<()> {
    if let Some((_, reason)) = DENIED.iter().find(|(denied, _)| *denied == id) {
        return Err(EngineError::Other(format!("panic_hunt denies {id}: {reason}")));
    }
    Ok(())
}

fn fresh(root: &Path) -> Session {
    let mut s = Session::new();
    // Also guard nested actions/scripts, not just the outer registry loop.
    s.authorize = Some(authorize);
    s.execute("file.new", json!({"width": 24, "height": 16})).unwrap();
    s.active_mut().unwrap().path = Some(root.join("document.pcraft").to_string_lossy().into_owned());
    s.execute("layer.new.layer", json!({})).unwrap();
    s.execute("select.rect", json!({"x": 1, "y": 1, "width": 6, "height": 5})).unwrap();
    // Session::new leaves preferences/presets in memory; no desktop store is attached.
    s
}

/// All registered writers' destination keys (including optional writers) are supplied here.
/// The child's cwd also contains relative/default names, and its TMPDIR contains spool files.
fn confined_params(id: &str, p: &Value, root: &Path) -> Value {
    let mut p = p.clone();
    let ext = match id {
        "file.export.artboardsToPdf" => Some("pdf"),
        "file.export.colorLookupTables" => Some("cube"),
        "file.export.pathsToIllustrator" => Some("ai"),
        "measurementLog.export" => Some("csv"),
        "pattern.export" => Some("pat"),
        "file.automate.createDroplet" => Some("pcdroplet"),
        "layer.smartObjects.exportContents" | "layer.smartObjects.convertToLinked" | "file.saveACopy" => Some("pcraft"),
        "layer.quickExportAsPng" | "layer.exportAs" | "file.export.quickExport" | "file.export.saveForWebLegacy" => Some("png"),
        _ => None,
    };
    if let Some(ext) = ext {
        p["path"] = json!(root.join(format!("output.{ext}")));
    }
    if matches!(
        id,
        "file.export.artboardsToFiles"
            | "file.export.layerCompsToFiles"
            | "file.export.layersToFiles"
            | "file.export.dataSetsAsFiles"
            | "file.export.renderVideo"
            | "file.generate.imageAssets"
            | "file.package"
            | "file.automate.batch"
            | "file.automate.runDroplet"
            | "file.scripts.imageProcessor"
            | "file.automate.createDroplet"
    ) {
        p["dir"] = json!(root.join("output"));
        p["output"] = json!(root.join("output"));
    }
    p
}

#[test]
#[ignore = "entry point for an isolated panic_hunt case"]
fn panic_hunt_child() {
    guard::child(|id, p, root| {
        assert!(authorize(id, p).is_ok(), "a denied command reached the worker");
        let mut s = fresh(root);
        if id == "fixture.save" {
            let r = s.execute("file.saveACopy", confined_params("file.saveACopy", p, root)).unwrap();
            let path = Path::new(r["path"].as_str().unwrap());
            assert!(path.starts_with(root));
            assert!(!std::fs::read(path).unwrap().is_empty());
            return;
        }
        let _ = s.execute(id, confined_params(id, p, root));
    });
}

#[test]
#[ignore = "slow full-registry fuzz; run with --ignored"]
fn no_command_panics_or_hangs_on_adversarial_params() {
    let mut bad = Vec::new();
    let mut cases = 0;
    for spec in command_specs() {
        if let Some((_, reason)) = DENIED.iter().find(|(id, _)| *id == spec.id) {
            eprintln!("skip {}: {reason}", spec.id);
            continue;
        }
        for p in guard::params() {
            cases += 1;
            if let Err(why) = guard::run("panic_hunt::panic_hunt_child", spec.id, &p) {
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
fn panic_hunt_denies_external_effects_in_nested_actions() {
    let mut s = fresh(Path::new("."));
    for &(id, _) in DENIED {
        assert!(command_specs().iter().any(|spec| spec.id == id), "stale deny-list entry: {id}");
        let e = s.execute(id, json!({})).unwrap_err();
        assert!(e.to_string().contains("panic_hunt denies"), "{e}");
        let r = s.execute("file.scripts.browse", json!({"steps": [[id, {}]]})).unwrap();
        assert_eq!(r["ok"], false, "nested {id} must be denied");
        assert!(r["results"][0]["error"].as_str().unwrap().contains("panic_hunt denies"));
    }
}

#[test]
fn panic_hunt_guard_detects_panics_and_reaps_timeouts() {
    guard::check_runner("panic_hunt::panic_hunt_child");
    guard::run("panic_hunt::panic_hunt_child", "fixture.save", &json!({})).unwrap();
}
