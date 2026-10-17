//! Background-removal timing on the production `Segmenter::predict` path:
//! `cargo run --offline -p li-seg --example bg_timing -- <dir with rmbg-1.4.onnx and
//! inspyrenet-swinb-plus-ultra.onnx> <photo> [warm runs]`.
//!
//! Prints the model load time, the first (cold) run and the median of the warm runs for Fast
//! (RMBG-1.4) and Better (InSPyReNet). `BENCH_POOL=<n>` builds the rayon global pool with `n`
//! threads first (as the app does at start-up with the computed compute budget); without it rayon
//! picks one thread per logical CPU. `LOCAL_IMAGE_SEG_THREADS=1` measures tract on one thread.
use std::time::Instant;

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let dir = std::path::PathBuf::from(args.next().ok_or_else(|| anyhow::anyhow!("model folder required"))?);
    let photo = args.next().ok_or_else(|| anyhow::anyhow!("photo required"))?;
    let warm: usize = args.next().and_then(|v| v.parse().ok()).unwrap_or(3).max(1);
    if let Some(n) = std::env::var("BENCH_POOL").ok().and_then(|v| v.parse::<usize>().ok()) {
        rayon::ThreadPoolBuilder::new().num_threads(n).thread_name(|i| format!("li-compute-{i}")).build_global()?;
    }
    let img = image::open(&photo)?.to_rgba8();
    let (w, h) = (img.width() as usize, img.height() as usize);
    println!("photo {photo} {w}x{h}; rayon pool {} threads", rayon::current_num_threads());
    for id in ["rmbg-1.4", "inspyrenet"] {
        let spec = li_seg::spec(id).ok_or_else(|| anyhow::anyhow!("unknown model {id}"))?;
        let t = Instant::now();
        let seg = li_seg::Segmenter::load(spec, &dir.join(spec.file))?;
        let load = t.elapsed().as_secs_f64();
        let t = Instant::now();
        let first = seg.predict(img.as_raw(), w, h)?;
        let cold = t.elapsed().as_secs_f64();
        let mut runs: Vec<f64> = (0..warm)
            .map(|_| {
                let t = Instant::now();
                seg.predict(img.as_raw(), w, h)?;
                Ok(t.elapsed().as_secs_f64())
            })
            .collect::<anyhow::Result<_>>()?;
        runs.sort_by(f64::total_cmp);
        let median = runs[runs.len() / 2];
        let covered = first.map_or(0.0, |p| p.iter().filter(|v| **v >= 0.5).count() as f64 / (w * h) as f64);
        println!("{id:<11} load {load:6.2} s  cold {cold:6.2} s  warm median {median:6.2} s ({warm} runs)  subject {:.1} %", covered * 100.0);
    }
    Ok(())
}
