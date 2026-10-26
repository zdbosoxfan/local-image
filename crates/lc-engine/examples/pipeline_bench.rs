//! Library batch pipelines, end to end on real files: import (copy + verify), Build Previews
//! (standard size) and export (full-size JPEG) into a fresh library under `<work dir>`:
//! `cargo run -p lightcraft-engine --example pipeline_bench -- <work dir> <files…>`
//!
//! `PHASE=import|previews|export` runs one phase (run them as separate processes, in that order,
//! to get each phase's own peak RSS); without it all three run in turn. Each phase prints its wall
//! time, process CPU time and the process's peak RSS so far (`VmHWM`). The compute profile comes
//! from the hardware like the app's (`LOCAL_IMAGE_THREADS` overrides the pool size).
#![expect(clippy::allow_attributes_without_reason, clippy::let_underscore_must_use, reason = "baseline 2026-10-25: unreviewed")]

use std::path::Path;
use std::time::Instant;

use lightcraft_engine::Session;
use serde_json::json;

/// Process CPU time in ms (all threads). The only `unsafe` is this libc clock read, in a dev-only example.
#[allow(unsafe_code)]
fn cpu_ms() -> f64 {
    #[cfg(unix)]
    {
        let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 0 };
        // SAFETY: `clock_gettime` only writes into the timespec we pass.
        unsafe { libc::clock_gettime(libc::CLOCK_PROCESS_CPUTIME_ID, &mut ts) };
        ts.tv_sec as f64 * 1e3 + ts.tv_nsec as f64 / 1e6
    }
    #[cfg(not(unix))]
    {
        0.0
    }
}

/// Peak resident set size so far, in MB (Linux; 0 elsewhere).
fn peak_rss_mb() -> f64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| s.lines().find(|l| l.starts_with("VmHWM:")).and_then(|l| l.split_whitespace().nth(1)?.parse::<f64>().ok()))
        .map_or(0.0, |kb| kb / 1024.0)
}

fn load() -> String {
    std::fs::read_to_string("/proc/loadavg").map(|s| s.split_whitespace().take(3).collect::<Vec<_>>().join(" ")).unwrap_or_default()
}

fn timed(what: &str, f: impl FnOnce() -> String) {
    let before = load();
    let (t, c) = (Instant::now(), cpu_ms());
    let note = f();
    println!(
        "{what}: {:.2} s wall, {:.1} s cpu, peak RSS {:.0} MB; load {before} -> {}; {note}",
        t.elapsed().as_secs_f64(),
        (cpu_ms() - c) / 1e3,
        peak_rss_mb(),
        load()
    );
}

fn session(lib: &Path) -> Session {
    let mut s = Session::new().with_fs();
    if let Err(e) = s.open_library(lib, false) {
        eprintln!("open library {}: {e}", lib.display());
        std::process::exit(1);
    }
    s
}

fn main() {
    let profile = lightcraft_sysmem::init(lightcraft_sysmem::Overrides::default());
    println!("{profile}");
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some((work, files)) = args.split_first() else {
        eprintln!("usage: pipeline_bench <work dir> <files…>");
        std::process::exit(2);
    };
    let (work, phase) = (Path::new(work), std::env::var("PHASE").unwrap_or_default());
    let (lib, out) = (work.join("library"), work.join("exported"));
    let run = |p: &str| phase.is_empty() || phase == p;
    if run("import") {
        let _ = std::fs::remove_dir_all(&lib);
        let mut s = session(&lib);
        timed(&format!("import (copy) of {} files", files.len()), || {
            let r = s.execute("library.import", &json!({"paths": files, "mode": "copy"}));
            let _ = s.persist();
            match r {
                Ok(r) => format!("imported {}", r["imported"].as_array().map_or(0, Vec::len)),
                Err(e) => format!("error: {e}"),
            }
        });
    }
    if run("previews") {
        let mut s = session(&lib);
        let ids: Vec<u64> = s.catalog.photos().map(|p| p.id.0).collect();
        timed(&format!("build previews (standard) of {} photos", ids.len()), || {
            match s.execute("library.buildPreviews", &json!({"ids": ids, "wait": true})) {
                Ok(r) => format!("done {} failed {}", r["done"], r["failed"]),
                Err(e) => format!("error: {e}"),
            }
        });
    }
    if run("export") {
        let _ = std::fs::remove_dir_all(&out);
        let mut s = session(&lib);
        let ids: Vec<_> = s.catalog.photos().map(|p| p.id).collect();
        let o = lightcraft_engine::export::ExportOptions::from_json(&json!({"format": "jpeg"}));
        let to = lightcraft_engine::export::Destination { dir: out.to_string_lossy().to_string(), exact: None };
        timed(&format!("export (full-size JPEG) of {} photos", ids.len()), || {
            let items = match lightcraft_engine::export::prepare_batch(&mut s, &ids, &o) {
                Ok(items) => items,
                Err(e) => return format!("error: {e}"),
            };
            let r = lightcraft_engine::export::run_batch(
                items,
                &o,
                &to,
                &mut lightcraft_engine::export::write_file,
                &|p| Path::new(p).exists(),
                false,
                &mut |_, _| true,
            );
            match r {
                Ok(files) => format!(
                    "written {}, failed {}",
                    files.iter().filter(|f| f.get("path").is_some()).count(),
                    files.iter().filter(|f| f.get("error").is_some()).count()
                ),
                Err(e) => format!("error: {e}"),
            }
        });
    }
}
