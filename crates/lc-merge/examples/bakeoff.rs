//! Fixed-CC0 feature/match quality and a small HDR quality sample.
//! Run (offline, dev profile; timing is diagnostic, not an interactive speed claim):
//! cargo +1.98.1 run --offline -p lightcraft-merge --example bakeoff --
//!   <a.hdr> [<b.hdr> ...]
#![forbid(unsafe_code)]

#[path = "bakeoff/common.rs"]
mod common;

use std::time::Instant;

use common::{Backend, Bracket, Merged, Pair, Registration, Result};
use li_raster::{GrayFrame, RgbFrame};
use lightcraft_geom::{Homography, Point};
use lightcraft_merge::{Deghost, Frame, HdrOptions, features, merge_hdr, no_progress, ransac};

struct Lightcraft;
impl Backend for Lightcraft {
    const NAME: &'static str = "lc_mops";
    fn register(pair: &Pair) -> Registration {
        let t = Instant::now();
        let make = |v: &[f32]| GrayFrame::from_fn(pair.w, pair.h, |x, y| v[y * pair.w + x]);
        let a = features::detect(&make(&pair.a), common::MAX_FEATURES);
        let b = features::detect(&make(&pair.b), common::MAX_FEATURES);
        let detect_ms = common::elapsed_ms(t);
        let t = Instant::now();
        let matches = features::match_features(&a, &b, 0.8);
        let match_ms = common::elapsed_ms(t);
        let src: Vec<_> = matches.iter().map(|&(i, _)| Point::new(a.points[i].x as f64, a.points[i].y as f64)).collect();
        let dst: Vec<_> = matches.iter().map(|&(_, j)| Point::new(b.points[j].x as f64, b.points[j].y as f64)).collect();
        let t = Instant::now();
        let fit = ransac::ransac(ransac::Model::Homography, &src, &dst, common::THRESHOLD, 8, common::SEED);
        let fit_ms = common::elapsed_ms(t);
        // LC uses +0.5 pixel centres; the protocol and PC use integer pixel centres.
        let to_centres = Homography([1.0, 0.0, 0.5, 0.0, 1.0, 0.5, 0.0, 0.0, 1.0]);
        let to_indices = Homography([1.0, 0.0, -0.5, 0.0, 1.0, -0.5, 0.0, 0.0, 1.0]);
        Registration {
            features: [a.len(), b.len()],
            matches: src.iter().zip(&dst).map(|(a, b)| ([a.x - 0.5, a.y - 0.5], [b.x - 0.5, b.y - 0.5])).collect(),
            fit: fit.as_ref().map(|f| to_indices.mul(&f.h).mul(&to_centres).0),
            inliers: fit.map_or_else(Vec::new, |f| f.inliers),
            detect_ms,
            match_ms,
            fit_ms,
        }
    }
    fn merge(b: &Bracket, deghost: bool) -> Result<Merged> {
        let frames: Vec<_> = b
            .frames
            .iter()
            .zip(common::EXPOSURES)
            .map(|(rgb, e)| {
                let image = RgbFrame::from_fn(b.truth.w, b.truth.h, |x, y| rgb[y * b.truth.w + x]);
                let mut f = Frame::from_linear_rec2020(image, Some(e.log2() as f64));
                // Simulated linear sensor data with an explicit white level. This does not exercise
                // the camera mosaic decoder or establish raw-domain production parity.
                f.raw = true;
                f.clip = 1.0;
                f
            })
            .collect();
        let out = merge_hdr(frames, &HdrOptions { align: false, deghost: if deghost { Deghost::High } else { Deghost::None } }, &no_progress)?;
        let ghost_fraction =
            if out.ghost.data.is_empty() { 0.0 } else { out.ghost.data.iter().filter(|v| **v > 0.5).count() as f64 / out.ghost.data.len() as f64 };
        Ok(Merged { rgb: out.radiance.data, ghost_fraction })
    }
}

fn main() -> Result<()> {
    common::run::<Lightcraft>(&std::env::args().skip(1).collect::<Vec<_>>())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn j10_float_dng_keeps_hdr_precision() -> Result<()> {
        use lightcraft_merge::{DngSamples, write_linear_dng};
        let image = RgbFrame::from_fn(16, 8, |x, y| [2f32.powf(x as f32 - 10.0), y as f32 * 0.1234567, 0.000012345]);
        let f = Frame::from_linear_rec2020(image.clone(), Some(0.0));
        let bytes = write_linear_dng(&image, &f.color, f.orientation, &f.metadata, 0.0, DngSamples::Float)?;
        let raw = lightcraft_raw::decode(&bytes)?;
        assert_eq!((raw.width, raw.height, raw.cpp, raw.bits), (16, 8, 3, 32));
        assert!(raw.cfa.is_none());
        let lightcraft_raw::RawData::F32(data) = raw.data else {
            return Err("float DNG did not decode as float samples".into());
        };
        let gain = 2f64.powf(raw.color.baseline_exposure);
        for (a, b) in data.iter().zip(image.data.iter().flatten()) {
            assert!((*a as f64 * gain - *b as f64).abs() <= (*b as f64).abs() * 2e-6 + 1e-12, "stored={a} source={b} gain={gain}");
        }
        Ok(())
    }

    #[test]
    fn j10_lc_half_pixel_conjugation() {
        let h = Homography([1.1, -0.2, 7.0, 0.2, 1.1, -4.0, 0.0001, -0.0002, 1.0]);
        let to_centres = Homography([1.0, 0.0, 0.5, 0.0, 1.0, 0.5, 0.0, 0.0, 1.0]);
        let to_indices = Homography([1.0, 0.0, -0.5, 0.0, 1.0, -0.5, 0.0, 0.0, 1.0]);
        let canonical = to_indices.mul(&h).mul(&to_centres);
        let a = h.apply(Point::new(20.5, 40.5));
        let b = common::apply(&canonical.0, [20.0, 40.0]);
        assert!((a.x - 0.5 - b[0]).abs() < 1e-10);
        assert!((a.y - 0.5 - b[1]).abs() < 1e-10);
    }
}
