//! One small CFA-domain quality comparison on CC0 radiance, not a benchmark suite.
//! Both paths receive identical Bayer captures and are scored against a noiseless,
//! single-demosaic Bayer reference. This isolates fusion order from CFA reconstruction.
#[allow(dead_code)]
#[path = "bakeoff/common.rs"]
mod common;

use lightcraft_merge::{Deghost, Frame, HdrOptions, merge_hdr, merge_raw_hdr, no_progress};
use lightcraft_raw::{Cfa, Method, RawData, RawImage};

fn mosaic(base: &RawImage, rgb: &[[f32; 3]], exposure: f32) -> common::Result<RawImage> {
    let cfa = Cfa::bayer("RGGB").ok_or("invalid Bayer pattern")?;
    let mut raw = base.clone();
    raw.cpp = 1;
    raw.cfa = Some(cfa.clone());
    raw.data = RawData::F32(rgb.iter().enumerate().map(|(i, p)| p[cfa.color_at(i % base.width, i / base.width) as usize]).collect());
    raw.metadata.exposure_time = Some(exposure as f64);
    raw.metadata.iso = Some(100);
    raw.metadata.f_number = Some(1.0);
    Ok(raw)
}

fn rmse(errors: impl Iterator<Item = f64>) -> Option<f64> {
    let (sum, n) = errors.fold((0.0, 0usize), |(s, n), e| (s + e * e, n + 1));
    (n > 0).then(|| (sum / n as f64).sqrt())
}

fn metrics(label: &str, image: &li_raster::RgbFrame, truth: &li_raster::RgbFrame, motion: &[bool], ms: f64) {
    let log_error = |a: f32, b: f32| (a.max(1e-6) as f64 / b.max(1e-6) as f64).log2();
    let pairs = || image.data.iter().zip(&truth.data).enumerate().filter(|(i, _)| !motion[*i]);
    let log = rmse(pairs().map(|(_, (a, b))| log_error(a[1], b[1])));
    let shadow = rmse(pairs().filter(|(_, (_, b))| b[1] < 0.02).map(|(_, (a, b))| (a[1] as f64 - b[1] as f64) / b[1].max(1e-6) as f64));
    let highlight = rmse(pairs().filter(|(_, (_, b))| b[1] > 0.8).map(|(_, (a, b))| log_error(a[1], b[1])));
    let edge = rmse(
        pairs()
            .filter(|(i, (_, b))| i % truth.width + 1 < truth.width && log_error(truth.data[i + 1][1], b[1]).abs() > 0.5)
            .map(|(_, (a, b))| log_error(a[1], b[1])),
    );
    let moving = rmse(image.data.iter().zip(&truth.data).enumerate().filter(|(i, _)| motion[*i]).map(|(_, (a, b))| log_error(a[1], b[1])));
    let dr = |img: &li_raster::RgbFrame| {
        let mut samples: Vec<_> = img.data.iter().map(|p| p[1].max(1e-6)).collect();
        samples.sort_by(f32::total_cmp);
        (samples[samples.len() * 999 / 1000] as f64 / samples[samples.len() / 100] as f64).log2()
    };
    println!(
        "{label},truth_dr={:.6},output_dr={:.6},log_rmse={log:?},shadow_relative_rmse={shadow:?},highlight_log_rmse={highlight:?},edge_log_rmse={edge:?},motion_log_rmse={moving:?},merge_ms={ms:.3}",
        dr(truth),
        dr(image)
    );
}

fn main() -> common::Result<()> {
    let path = std::env::args().nth(1).ok_or("pass one CC0 .hdr path under target/")?;
    let scene = common::load(std::path::Path::new(&path))?;
    for moving in [false, true] {
        let b = common::bracket(&scene, moving);
        let rgb = li_raster::RgbFrame::from_fn(b.truth.w, b.truth.h, |x, y| b.truth.rgb[y * b.truth.w + x]);
        let frame = Frame::from_linear_rec2020(rgb.clone(), Some(0.0));
        let bytes = lightcraft_merge::write_linear_dng(
            &rgb,
            &frame.color,
            frame.orientation,
            &frame.metadata,
            0.0,
            lightcraft_merge::DngSamples::Float,
            None,
        )?;
        let mut base = lightcraft_raw::decode(&bytes)?;
        // Restore scene units: the writer normalizes only its serialized output.
        base.color.baseline_exposure = 0.0;
        base.white = vec![1.0];
        let noiseless = mosaic(&base, &b.truth.rgb, 1.0)?;
        let truth = noiseless.develop(Method::Auto)?;
        let raws: Vec<_> = b.frames.iter().zip(common::EXPOSURES).map(|(f, e)| mosaic(&base, f, e)).collect::<common::Result<_>>()?;
        for deghost in [Deghost::None, Deghost::High] {
            if !moving && deghost == Deghost::High {
                continue;
            }
            let opts = HdrOptions { align: false, deghost };
            let start = std::time::Instant::now();
            let frames = raws
                .iter()
                .map(|raw| {
                    let mut f = Frame::from_linear_rec2020(raw.develop(Method::Auto)?, Some((raw.metadata.exposure_time.unwrap_or(1.0)).log2()));
                    f.raw = true;
                    f.clip = 1.0;
                    Ok(f)
                })
                .collect::<common::Result<Vec<_>>>()?;
            let old = merge_hdr(frames, &opts, &no_progress)?;
            metrics(&format!("demosaic_first,moving={moving},deghost={deghost:?}"), &old.radiance, &truth, &b.motion, common::elapsed_ms(start));
            let start = std::time::Instant::now();
            let new = merge_raw_hdr(&raws, &opts, &no_progress)?;
            metrics(&format!("raw_first,moving={moving},deghost={deghost:?}"), &new.radiance, &truth, &b.motion, common::elapsed_ms(start));
        }
    }
    Ok(())
}
