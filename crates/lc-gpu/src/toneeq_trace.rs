//! Scoped, test-only snapshots. Ordinary renders and the legacy environment trace
//! retain no snapshots. A dump does not change process environment or GPU state.
use std::cell::RefCell;

struct Snapshot {
    label: String,
    cpu: Vec<f32>,
    gpu: Vec<f32>,
    w: usize,
    nc: usize,
}
thread_local! {
    static SNAPSHOTS: RefCell<Option<Vec<Snapshot>>> = const { RefCell::new(None) };
}
pub(crate) struct Scope;
impl Scope {
    pub(crate) fn new() -> Self {
        SNAPSHOTS.with(|v| *v.borrow_mut() = Some(Vec::new()));
        Self
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        SNAPSHOTS.with(|v| *v.borrow_mut() = None);
    }
}
pub(crate) fn enabled() -> bool {
    SNAPSHOTS.with(|v| v.borrow().is_some())
}
pub(crate) fn record(label: &str, cpu: &[f32], gpu: &[f32], w: usize, nc: usize) {
    SNAPSHOTS.with(|v| {
        if let Some(snapshots) = &mut *v.borrow_mut() {
            snapshots.push(Snapshot { label: label.to_owned(), cpu: cpu.to_vec(), gpu: gpu.to_vec(), w, nc });
        }
    });
}
pub(crate) fn dump(w: usize, h: usize, points: &[(usize, usize)]) {
    SNAPSHOTS.with(|v| {
        let mut snapshots = v.borrow_mut();
        let Some(snapshots) = snapshots.as_mut() else { return };
        for &(x, y) in points {
            eprintln!("toneeq pixel ({x}, {y}) intermediate dump:");
            for s in snapshots.iter() {
                let sh = s.cpu.len() / (s.w * s.nc);
                // Lower corner of the reference's corner-aligned interpolation.
                let sx = ((x as f32 / w as f32 * s.w as f32) as usize).min(s.w - 1);
                let sy = ((y as f32 / h as f32 * sh as f32) as usize).min(sh - 1);
                for ch in 0..s.nc {
                    let i = (sy * s.w + sx) * s.nc + ch;
                    let (p, q) = (s.cpu[i], s.gpu[i]);
                    eprintln!(
                        "  {} {}x{} ({sx}, {sy}) ch{ch}: CPU={p:.9e} [{:08x}] GPU={q:.9e} [{:08x}]",
                        s.label,
                        s.w,
                        sh,
                        p.to_bits(),
                        q.to_bits()
                    );
                }
            }
        }
        snapshots.clear();
    });
}
