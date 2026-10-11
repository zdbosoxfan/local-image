//! Background renders must be finishable before the process exits: NVIDIA's driver segfaults in
//! `libnvidia-glcore` (and the GPU raises Xid 13 "Illegal Instruction Encoding") when it is torn
//! down while a detached thread still renders. `quiesce` is the exit-time barrier.
//! Skips (passes with a note) when no GPU adapter exists.

use std::sync::Arc;

use lightcraft_develop::DevelopSettings;
use lightcraft_pipeline::{RenderRequest, SourceInfo};

fn render_once(src: &Arc<lightcraft_raster::Rgb32f>, w: usize, h: usize) {
    let mut s = DevelopSettings::default();
    s.light.exposure = 0.3;
    s.effects.clarity = 20.0;
    let _ = lightcraft_gpu::render(src, &SourceInfo::default(), &s, &RenderRequest::fit(w, h), None);
}

#[test]
fn quiesce_waits_for_background_renders() {
    if !lightcraft_gpu::available() {
        eprintln!("skipped: no GPU adapter ({:?})", lightcraft_gpu::unavailable_reason());
        return;
    }
    let (w, h) = (1200, 800);
    let src = Arc::new(lightcraft_scenes::demo_library()[0].render(w, h));
    assert!(lightcraft_gpu::quiesce(std::time::Duration::from_secs(30)), "idle: nothing to wait for");
    let done = Arc::new(std::sync::atomic::AtomicU32::new(0));
    let workers: Vec<_> = (0..3)
        .map(|_| {
            let (src, done) = (src.clone(), done.clone());
            std::thread::spawn(move || {
                for _ in 0..4 {
                    render_once(&src, w, h);
                }
                done.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            })
        })
        .collect();
    std::thread::sleep(std::time::Duration::from_millis(20));
    // whatever is running right now ends before quiesce returns, and the queue is empty
    assert!(lightcraft_gpu::quiesce(std::time::Duration::from_secs(120)));
    for t in workers {
        t.join().unwrap();
    }
    assert_eq!(done.load(std::sync::atomic::Ordering::SeqCst), 3);
}
