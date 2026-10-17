//! Numeric support for editor response recovery and bundle adjustment.
pub fn solve_dense(n: usize, a: Vec<f64>, b: Vec<f64>) -> Option<Vec<f64>> {
    crate::linalg::solve(a, b, n)
}
/// Accumulates weighted rows into normal equations `AᵀA x = Aᵀb` for a dense least-squares
/// problem with `n` unknowns.
pub struct Normal {
    pub n: usize,
    pub ata: Vec<f64>,
    pub atb: Vec<f64>,
}

impl Normal {
    pub fn new(n: usize) -> Self {
        Normal { n, ata: vec![0.0; n * n], atb: vec![0.0; n] }
    }
    /// Adds the sparse row `Σ coef·x[idx] = rhs` with weight `w` (applied squared).
    pub fn add(&mut self, row: &[(usize, f64)], rhs: f64, w: f64) {
        let w2 = w * w;
        for &(i, a) in row {
            self.atb[i] += w2 * a * rhs;
            for &(j, b) in row {
                self.ata[i * self.n + j] += w2 * a * b;
            }
        }
    }
    pub fn solve(self) -> Option<Vec<f64>> {
        solve_dense(self.n, self.ata, self.atb)
    }
}

/// Runs `f(y, row)` for every row of a `w × h × ch` buffer, in parallel on native targets.
pub(crate) fn par_rows<T: Send + Sync>(buf: &mut [T], w: usize, ch: usize, f: impl Fn(usize, &mut [T]) + Sync + Send) {
    li_raster::par_rows(buf, (w * ch).max(1), f);
}

pub(crate) use li_raster::par_map;

/// Deterministic 64-bit generator (SplitMix64).
pub(crate) struct Rng(pub u64);
impl Rng {
    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    pub fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

pub(super) use li_color::transfer::srgb_to_linear;
pub(super) fn gauss_blur_n(buf: &mut [f32], w: usize, h: usize, n: usize, sigma: f32) {
    li_raster::blur::gaussian_boxes_n(buf, w, h, n, sigma);
}
