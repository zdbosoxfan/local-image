//! Timing for the computational-photography algorithms on a ~24 MP (6000×4000) image:
//! Lens Correction, Adaptive Wide Angle, HDR merge + tone mapping, Crop and
//! Straighten detection, and a 4-image panorama registration + blend (2 MP sources).
//! `cargo run --release -p photocraft-algo --example bench_photo [case-substring]`
//! Bake-off mode on two small CC0 HDR samples: `cargo +1.98.1 run --offline -p photocraft-algo --example bench_photo --
//! bakeoff first.hdr second.hdr`
//! Both stacks compile the same bounded input synthesis and scoring protocol.
#[path = "../../lc-merge/examples/bakeoff/common.rs"]
mod common;
use std::time::Instant;

use lightcraft_merge::editor::hdr::{self as merge_hdr, MergeOptions};
use lightcraft_merge::editor::panorama::{self as merge_pano, AlignOptions, Layout, RoiImage};
use photocraft_algo::hdr::{self, ToneMethod};
use photocraft_algo::lens::{self, LensCorrection};
use photocraft_algo::panorama;
use photocraft_algo::tone::HdrToning;
use photocraft_algo::transform::Interp;
use photocraft_algo::wideangle::{self, Constraint, Orientation, WideAngle, WideModel};
use photocraft_color::PixelFormat;
use photocraft_geom::Rect;
use photocraft_raster::Surface;

fn pattern(w: usize, h: usize) -> Vec<[f32; 4]> {
    (0..w * h)
        .map(|i| {
            let (x, y) = (i % w, i / w);
            let t = ((x * 7 + y * 3) % 251) as f32 / 251.0;
            [(x % 256) as f32 / 255.0 * 0.6 + 0.2 * t, ((x * 3 + y) / 11 % 256) as f32 / 255.0, ((x ^ y) % 256) as f32 / 255.0, 1.0]
        })
        .collect()
}

fn time<T>(name: &str, only: &Option<String>, f: impl FnOnce() -> T) -> Option<T> {
    if only.as_ref().is_some_and(|o| !name.contains(o.as_str())) {
        return None;
    }
    let t0 = Instant::now();
    let r = f();
    println!("{name:<40} {:>9.1} ms", t0.elapsed().as_secs_f64() * 1000.0);
    Some(r)
}

fn main() -> common::Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|s| s == "bakeoff") {
        return common::run::<Photocraft>(&args[1..]);
    }
    let only = args.first().cloned();
    let (w, h) = (6000usize, 4000usize);
    let px = pattern(w, h);
    let r = Rect::new(0, 0, w as i32, h as i32);
    let mut s = Surface::new(PixelFormat::RGBA8);
    let bytes: Vec<u8> = px.iter().flat_map(|q| q.map(|v| (v * 255.0) as u8)).collect();
    s.write_interleaved(r, &bytes);

    time("lens correction (distortion, CA, vignette, persp)", &only, || {
        lens::correct(
            &s,
            r,
            &LensCorrection {
                profile: Some(lens::generic_profile(24.0)),
                distortion: 10.0,
                red_cyan: 20.0,
                vignette_amount: 30.0,
                vertical: -20.0,
                angle: 1.5,
                ..Default::default()
            },
        )
    });
    time("adaptive wide angle (fisheye, 2 constraints)", &only, || {
        let p = WideAngle {
            model: WideModel::Fisheye,
            focal_length: 12.0,
            constraints: vec![
                Constraint { a: [500.0, 600.0], b: [5500.0, 600.0], orientation: Orientation::Horizontal },
                Constraint { a: [700.0, 500.0], b: [700.0, 3500.0], orientation: Orientation::Vertical },
            ],
            ..Default::default()
        };
        wideangle::apply(&s, r, &p, Interp::Bicubic)
    });
    time("hdr: merge 3 exposures + local adaptation", &only, || {
        let shots: Vec<Vec<[f32; 4]>> =
            [0.5f32, 1.0, 2.0].iter().map(|k| px.iter().map(|q| [(q[0] * k).min(1.0), (q[1] * k).min(1.0), (q[2] * k).min(1.0), 1.0]).collect()).collect();
        let refs: Vec<&[[f32; 4]]> = shots.iter().map(Vec::as_slice).collect();
        let opts = MergeOptions { exposures: vec![0.5, 1.0, 2.0], remove_ghosts: true, ghost_base: None, response: None };
        let t0 = Instant::now();
        let mut m = merge_hdr::merge(w, h, &refs, &opts).unwrap();
        println!("  merge {:.1} ms", t0.elapsed().as_secs_f64() * 1000.0);
        hdr::tone_map(&mut m.px, w, h, &ToneMethod::LocalAdaptation(HdrToning { radius: 7.0, strength: 0.52, ..Default::default() }));
    });
    time("crop and straighten: detection", &only, || photocraft_algo::scancrop::find_photos(w, h, &px));
    time("panorama: 4 × 2 MP register + blend", &only, || {
        let (sw, sh) = (1700usize, 1200usize);
        let views: Vec<Vec<f32>> = (0..4)
            .map(|k| {
                (0..sw * sh)
                    .map(|i| {
                        let q = px[(i / sw + 300) * w + i % sw + 1100 * k];
                        0.3 * q[0] + 0.6 * q[1] + 0.1 * q[2]
                    })
                    .collect()
            })
            .collect();
        let prepared: Vec<merge_pano::PreparedImage> = views.iter().map(|v| (sw, sh, v.clone(), None)).collect();
        let t0 = Instant::now();
        let feats = merge_pano::detect_many(&prepared, 800);
        let matches = panorama::match_pairs(&feats);
        let al = panorama::align(&feats, &matches, &AlignOptions { layout: Layout::Reposition, focal: None, reference: None, geometric: false });
        println!(
            "  register {:.1} ms ({} pairs, placed {})",
            t0.elapsed().as_secs_f64() * 1000.0,
            matches.len(),
            al.as_ref().map_or(0, |a| a.placements.iter().flatten().count())
        );
        let rois: Vec<RoiImage> = views
            .iter()
            .enumerate()
            .map(|(k, v)| RoiImage { x0: 1100 * k as i32, y0: 0, w: sw, h: sh, ch: 1, px: v.clone(), alpha: vec![1.0; sw * sh] })
            .collect();
        let cw = 1100 * 3 + sw;
        let wts: Vec<Vec<f32>> = (0..4i32)
            .map(|k| {
                (0..sw * sh)
                    .map(|i| {
                        let x = (i % sw) as i32 + 1100 * k;
                        if (x / 1100).min(3) == k { 1.0 } else { 0.0 }
                    })
                    .collect()
            })
            .collect();
        let t1 = Instant::now();
        let _ = merge_pano::multiband(cw, sh, &rois, &wts, merge_pano::blend_levels(cw, sh));
        println!("  multiband {:.1} ms", t1.elapsed().as_secs_f64() * 1000.0);
    });
    Ok(())
}

struct Photocraft;
impl common::Backend for Photocraft {
    const NAME: &'static str = "pc_brief";
    fn register(pair: &common::Pair) -> common::Registration {
        use lightcraft_merge::editor::features;
        let t = Instant::now();
        let a = merge_pano::detect(pair.w, pair.h, &pair.a, None, common::MAX_FEATURES);
        let b = merge_pano::detect(pair.w, pair.h, &pair.b, None, common::MAX_FEATURES);
        let detect_ms = common::elapsed_ms(t);
        let t = Instant::now();
        let matches = features::match_descriptors(&a.desc, &b.desc, 0.8);
        let match_ms = common::elapsed_ms(t);
        let src: Vec<_> = matches.iter().map(|&(i, _)| a.points[i]).collect();
        let dst: Vec<_> = matches.iter().map(|&(_, j)| b.points[j]).collect();
        let t = Instant::now();
        let fit = features::ransac(features::Model::Homography, &src, &dst, common::THRESHOLD, common::SEED).filter(|(_, inliers)| inliers.len() >= 8);
        let fit_ms = common::elapsed_ms(t);
        common::Registration {
            features: [a.points.len(), b.points.len()],
            matches: src.into_iter().zip(dst).collect(),
            fit: fit.as_ref().map(|(h, _)| h.0),
            inliers: fit.map_or_else(Vec::new, |(_, i)| i),
            detect_ms,
            match_ms,
            fit_ms,
        }
    }
    fn merge(b: &common::Bracket, deghost: bool) -> common::Result<common::Merged> {
        // The editor merger expects non-linear sources. Test its actual sRGB8 input model
        // against the same scene radiance, rather than feeding it linear camera pixels.
        let encoded: Vec<Vec<[f32; 4]>> = b
            .frames
            .iter()
            .map(|rgb| {
                rgb.iter()
                    .map(|q| {
                        let c = q.map(srgb8);
                        [c[0], c[1], c[2], 1.0]
                    })
                    .collect()
            })
            .collect();
        let refs: Vec<_> = encoded.iter().map(Vec::as_slice).collect();
        let out = merge_hdr::merge(
            b.truth.w,
            b.truth.h,
            &refs,
            &MergeOptions { exposures: common::EXPOSURES.map(f64::from).to_vec(), remove_ghosts: deghost, ghost_base: Some(1), response: None },
        )?;
        Ok(common::Merged { rgb: out.px.iter().map(|q| [q[0], q[1], q[2]]).collect(), ghost_fraction: out.ghost_fraction })
    }
}

fn srgb8(v: f32) -> f32 {
    let s = if v <= 0.0031308 { 12.92 * v } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 };
    (s.clamp(0.0, 1.0) * 255.0).round() / 255.0
}

#[cfg(test)]
mod j10_tests {
    use super::*;
    #[test]
    fn j10_rendered_inputs_are_srgb8() {
        assert_eq!(srgb8(0.0), 0.0);
        assert_eq!(srgb8(1.0), 1.0);
        assert_eq!(srgb8(0.18), 118.0 / 255.0);
    }
}
