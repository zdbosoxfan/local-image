//! What switching from the Library to Compositing costs on a photo (opus-scan): the module switch
//! runs `develop.openPhoto` (read + develop the original at full size, a new document whose
//! Background is a Develop layer) and, coming back from Develop with new settings,
//! `develop.syncPhoto` (re-develops the layer). Dispatch is measured separately from completion.
//!
//! ```sh
//! cargo run --release -p photocraft-engine --example bench_develop_open -- <photo.ARW|.jpg|…> [--reps 3] [--typical]
//! ```
//!
//! `--typical` opens with the edits `lightcraft-engine`'s `render_bench` calls typical (exposure,
//! highlights/shadows, clarity, texture, dehaze, noise reduction, sharpening) instead of none.

use std::time::Instant;

use photocraft_engine::{Session, jobs::Started};
use serde_json::json;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let Some(path) = args.get(1).filter(|a| !a.starts_with("--")).cloned() else {
        eprintln!("usage: bench_develop_open <photo> [--reps N] [--typical] [--inline]");
        std::process::exit(2);
    };
    let reps: usize = args.iter().position(|a| a == "--reps").and_then(|i| args.get(i + 1)).and_then(|v| v.parse().ok()).unwrap_or(3).max(1);
    let base = if args.iter().any(|a| a == "--typical") {
        json!({ "light": { "exposure": 0.3, "highlights": -40.0, "shadows": 30.0 },
                "effects": { "clarity": 15.0, "texture": 10.0, "dehaze": 10.0 },
                "detail": { "nr_luminance": 30.0, "nr_color": 25.0, "sharpen_amount": 40.0 } })
    } else {
        json!({})
    };
    let mut changed = base.clone();
    changed["light"]["exposure"] = json!(0.5);
    for rep in 0..reps {
        let mut s = Session::new();
        let t = Instant::now();
        let r = if args.iter().any(|a| a == "--inline") {
            s.execute("develop.openPhoto", json!({ "path": path, "photo": 1, "settings": base, "name": "photo" })).map(Started::Done)
        } else {
            s.start("develop.openPhoto", json!({ "path": path, "photo": 1, "settings": base, "name": "photo" }))
        };
        let open_dispatch = t.elapsed().as_secs_f64() * 1000.0;
        let r = r.and_then(|r| match r {
            Started::Done(v) => Ok(v),
            Started::Job(id) => s.wait_job(id),
        });
        let open = t.elapsed().as_secs_f64() * 1000.0;
        if let Err(e) = r {
            eprintln!("develop.openPhoto: {e}");
            std::process::exit(1);
        }
        let size = s.active().map(|d| (d.doc.size.width, d.doc.size.height)).unwrap_or_default();
        let t = Instant::now();
        let r = if args.iter().any(|a| a == "--inline") {
            s.execute("develop.syncPhoto", json!({ "photo": 1, "settings": changed })).map(Started::Done)
        } else {
            s.start("develop.syncPhoto", json!({ "photo": 1, "settings": changed }))
        };
        let sync_dispatch = t.elapsed().as_secs_f64() * 1000.0;
        let r = r.and_then(|r| match r {
            Started::Done(v) => Ok(v),
            Started::Job(id) => s.wait_job(id),
        });
        let sync = t.elapsed().as_secs_f64() * 1000.0;
        if let Err(e) = r {
            eprintln!("develop.syncPhoto: {e}");
            std::process::exit(1);
        }
        println!(
            "run {rep}: {}×{}: develop.openPhoto dispatch {open_dispatch:.3} ms / complete {open:.0} ms, develop.syncPhoto (exposure) dispatch {sync_dispatch:.3} ms / complete {sync:.0} ms",
            size.0, size.1
        );
    }
}
