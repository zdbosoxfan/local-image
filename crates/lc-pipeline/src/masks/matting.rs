//! Subject matting kept between renders. The closed-form solve takes hundreds of milliseconds,
//! while what it depends on (the photo as sampled for this view and the model mask's coverage)
//! stays the same as Develop sliders are dragged: exposure, tone and colour act after it.

use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex, PoisonError};

use rayon::prelude::*;

/// Results kept (a loupe, its drafts, the filmstrip, an export), newest first, and the memory
/// they may take together.
const KEEP: usize = 4;
const KEEP_BYTES: usize = 256 << 20;

struct Results(Mutex<Vec<(u64, Arc<Vec<f32>>)>>);

impl Results {
    const fn new() -> Self {
        Results(Mutex::new(Vec::new()))
    }

    /// The result kept for `key`, or `solve`'s (then kept).
    fn get_or_solve(&self, key: u64, solve: impl FnOnce() -> Vec<f32>) -> Vec<f32> {
        let list = || self.0.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some((_, hit)) = list().iter().find(|(k, _)| *k == key) {
            return hit.to_vec();
        }
        let out = solve();
        let mut kept = list();
        kept.retain(|(k, _)| *k != key);
        kept.insert(0, (key, Arc::new(out.clone())));
        let mut bytes = 0;
        kept.retain(|(_, r)| {
            bytes += r.len() * size_of::<f32>();
            bytes <= KEEP_BYTES
        });
        kept.truncate(KEEP);
        out
    }
}

static RESULTS: Results = Results::new();

/// [`li_matte::refine::subject_alpha`] of `mask` over the `w × h` image `rgb` with `band`, reusing
/// the result of a render with the same inputs.
pub(super) fn subject_alpha(w: usize, h: usize, rgb: &[[f32; 3]], mask: &[f32], band: f32) -> Vec<f32> {
    RESULTS.get_or_solve(fingerprint(w, h, rgb, mask, band), || li_matte::refine::subject_alpha(&li_matte::RgbImage::borrowed(w, h, rgb), mask, band))
}

/// Identifies the inputs: for each row of the image and of the mask, every channel's sum and its
/// sum weighted by column (a value that changes or moves changes them), with the sizes and band.
fn fingerprint(w: usize, h: usize, rgb: &[[f32; 3]], mask: &[f32], band: f32) -> u64 {
    let image: Vec<[f64; 6]> = rgb
        .par_chunks(w.max(1))
        .map(|row| {
            let mut m = [0.0f64; 6];
            for (x, p) in row.iter().enumerate() {
                for c in 0..3 {
                    let v = f64::from(p[c]);
                    m[c] += v;
                    m[3 + c] += v * x as f64;
                }
            }
            m
        })
        .collect();
    let coverage: Vec<[f64; 2]> = mask
        .par_chunks(w.max(1))
        .map(|row| row.iter().enumerate().fold([0.0f64; 2], |m, (x, v)| [m[0] + f64::from(*v), m[1] + f64::from(*v) * x as f64]))
        .collect();
    let mut hasher = std::hash::DefaultHasher::new();
    (w, h, rgb.len(), mask.len(), band.to_bits()).hash(&mut hasher);
    for v in image.iter().flatten().chain(coverage.iter().flatten()) {
        v.to_bits().hash(&mut hasher);
    }
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use li_raster::RgbFrame;

    /// A bright sky over a darker roof with a soft model edge a few pixels off the roofline.
    fn scene() -> (RgbFrame, Vec<f32>) {
        let (w, h) = (96, 64);
        let img = RgbFrame::from_fn(w, h, |x, y| if y < 30 + x / 16 { [0.8, 0.85, 0.95] } else { [0.12, 0.1, 0.09] });
        let mask = (0..w * h).map(|i| (((34 + (i % w) / 16) as f32 - (i / w) as f32) / 4.0).clamp(0.0, 1.0)).collect();
        (img, mask)
    }

    #[test]
    fn matting_is_solved_once_for_the_same_inputs_and_again_when_they_change() {
        let (img, mask) = scene();
        let solved = std::cell::Cell::new(0);
        let results = Results::new();
        let solve = |img: &RgbFrame, mask: &[f32], band: f32| {
            results.get_or_solve(fingerprint(img.width, img.height, &img.data, mask, band), || {
                solved.set(solved.get() + 1);
                li_matte::refine::subject_alpha(&li_matte::RgbImage::borrowed(img.width, img.height, &img.data), mask, band)
            })
        };
        let first = solve(&img, &mask, 6.0);
        assert_eq!(solve(&img, &mask, 6.0), first, "a drag's next render reuses the solve");
        assert_eq!(solved.get(), 1);
        // a changed pixel, the photo flipped, another mask or band: solved again
        let mut changed = img.clone();
        changed.data[40 * 96 + 7][1] += 1e-4;
        let flipped = RgbFrame::from_fn(96, 64, |x, y| img.get(95 - x, y));
        let mut other = mask.clone();
        other[10] = 0.5;
        for (i, (img, mask, band)) in
            [(&changed, &mask, 6.0), (&flipped, &mask, 6.0), (&img, &other, 6.0), (&img, &mask, 7.0)].into_iter().enumerate()
        {
            let direct = li_matte::refine::subject_alpha(&li_matte::RgbImage::borrowed(img.width, img.height, &img.data), mask, band);
            assert_eq!(solve(img, mask, band), direct);
            assert_eq!(solved.get(), i + 2);
        }
        assert_eq!(results.0.lock().unwrap().len(), KEEP, "a few results are kept");
    }
}
