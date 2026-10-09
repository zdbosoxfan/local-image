//! Geometry tests, including adapter-independent checks of the packed WGSL math.
use super::*;
use lightcraft_geom::Affine;
use lightcraft_pipeline::geometry::Frame;
use lightcraft_pipeline::lensdb::{Distortion, LensCorrection, Tca};
use lightcraft_pipeline::optics::Warp;

const ORIENTATIONS: [Orientation; 8] = [
    Orientation::Normal,
    Orientation::Rotate90,
    Orientation::Rotate180,
    Orientation::Rotate270,
    Orientation::FlipH,
    Orientation::FlipV,
    Orientation::Transpose,
    Orientation::Transverse,
];

fn profiles() -> [LensCorrection; 3] {
    let c = LensCorrection {
        diag_norm: 2.0,
        center: [0.23, -0.19],
        distortion: Distortion::Poly3 { k1: -0.3 },
        tca: Tca::Linear { kr: 1.015, kb: 0.985 },
        vignetting: Some([-0.35, 0.08, -0.01]),
    };
    [
        c,
        LensCorrection {
            distortion: Distortion::Poly5 { k1: 0.25, k2: 0.08 },
            tca: Tca::Poly3 { red: [1.015, 0.006, 0.004], blue: [0.985, 0.0, -0.004] },
            ..c
        },
        LensCorrection { distortion: Distortion::Ptlens { a: 0.12, b: 0.16, c: 0.05 }, tca: Tca::None, ..c },
    ]
}

fn settings(orientation: Orientation, strength: f64) -> DevelopSettings {
    let mut s = DevelopSettings { orientation, ..Default::default() };
    s.lens_db.enabled = true;
    s.lens_db.distortion = strength;
    s.lens_db.tca = strength;
    s.lens_db.vignetting = strength;
    s
}

// Transcription of database_warp and the PA part of warp_gain, reading actual
// host parameters. This checks packing, f64 strength scaling, and f32 error
// independently of an adapter; device tests still check the real shader.
fn database_f32(p: &[u32], x: f32, y: f32, ch: usize) -> (f32, f32) {
    let f = |i: usize| f32::from_bits(p[i]);
    if p[65] == 0 && (p[69] == 0 || ch == 1) {
        return (x, y);
    }
    let (mut u, mut v) = ((x - f(81)) * f(80), (y - f(82)) * f(80));
    let r2 = u * u + v * v;
    let factor = match p[65] {
        1 => r2 * f(66) + 1.0,
        2 => r2 * f(66) + r2 * r2 * f(67) + 1.0,
        3 => r2 * r2.max(0.0).sqrt() * f(66) + r2 * f(67) + r2.max(0.0).sqrt() * f(68) + 1.0,
        _ => 1.0,
    };
    u *= factor;
    v *= factor;
    if p[69] != 0 && ch != 1 {
        let o = if ch == 0 { 70 } else { 73 };
        let mut scale = f(o);
        if p[69] == 2 {
            let r2 = u * u + v * v;
            scale = if f(o + 1) == 0.0 { r2 * f(o + 2) + f(o) } else { r2 * f(o + 2) + r2.max(0.0).sqrt() * f(o + 1) + f(o) };
            scale = (scale - 1.0) * f(83) + 1.0;
        }
        u *= scale;
        v *= scale;
    }
    (u / f(80) + f(81), v / f(80) + f(82))
}

fn gain_f32(p: &[u32], x: f32, y: f32) -> f32 {
    if p[76] == 0 {
        return 1.0;
    }
    let f = |i: usize| f32::from_bits(p[i]);
    let (u, v) = ((x - f(81)) * f(80), (y - f(82)) * f(80));
    let r2 = u * u + v * v;
    let g = 1.0 + f(77) * r2 + f(78) * r2 * r2 + f(79) * r2 * r2 * r2;
    if g > 1e-6 { (1.0 / g).powf(f(84)) } else { 1.0 }
}

#[test]
fn packed_lens_models_track_f64_reference_in_f32() {
    let mut max_position = 0.0f64;
    let mut max_gain_relative = 0.0f64;
    for (w, h) in [(641, 427), (6000, 4000), (1, 1)] {
        for orientation in ORIENTATIONS {
            for c in profiles() {
                for strength in [0.0, 50.0, 100.0, 200.0] {
                    let s = settings(orientation, strength);
                    let (ow, oh) = if orientation.swaps_axes() { (h, w) } else { (w, h) };
                    let mut wp = Warp::identity(ow as f64, oh as f64);
                    wp.set_lensdb(&c, orientation, w as f64, h as f64, &s);
                    let p = warp_params(&wp, (ow, oh), (ow, oh), &Affine::IDENTITY, 1.0, 1.0);
                    let m = wp.lensdb.expect("profile");
                    for (x, y) in [(0.5, 0.5), (ow as f64 - 0.5, oh as f64 - 0.5), (ow as f64 * 0.23, oh as f64 * 0.81), (m.cx, m.cy)] {
                        // Positions handed to the shader are already f32.
                        let (x, y) = (x as f32, y as f32);
                        for ch in 0..3 {
                            let a = database_f32(&p, x, y, ch);
                            let b = m.to_source(x as f64, y as f64, ch, strength / 100.0, strength / 100.0);
                            let err = (a.0 as f64 - b.0).abs().max((a.1 as f64 - b.1).abs());
                            max_position = max_position.max(err);
                            assert!(err < 0.006, "{orientation:?} {c:?} {strength}% {ch}: {a:?} vs {b:?} error {err}");
                            let gain = gain_f32(&p, a.0, a.1) as f64;
                            let reference = m.gain(a.0 as f64, a.1 as f64, strength / 100.0);
                            let rel = (gain - reference).abs() / reference;
                            max_gain_relative = max_gain_relative.max(rel);
                            assert!(rel < 2e-5, "PA gain {gain} vs {reference}");
                        }
                    }
                }
            }
        }
    }
    eprintln!("packed f32 lens models: max position {max_position:.6} px, relative gain {max_gain_relative:.8}");
}

#[test]
fn zero_strength_and_identity_models_disable_database_branches() {
    for c in profiles() {
        let s = settings(Orientation::Normal, 0.0);
        let mut wp = Warp::identity(641.0, 427.0);
        wp.set_lensdb(&c, s.orientation, 641.0, 427.0, &s);
        // An unrelated manual correction can still require the warp kernel.
        wp.k1 = 0.03;
        let p = warp_params(&wp, (641, 427), (641, 427), &Affine::IDENTITY, 1.0, 1.0);
        assert_eq!((p[65], p[69], p[76]), (0, 0, 0));
        assert_eq!(database_f32(&p, 1.25, 400.5, 0), (1.25, 400.5));
        assert_eq!(gain_f32(&p, 1.25, 400.5), 1.0);
    }
    let c = LensCorrection {
        distortion: Distortion::Poly5 { k1: 0.0, k2: 0.0 },
        tca: Tca::Linear { kr: 1.0, kb: 1.0 },
        vignetting: Some([0.0; 3]),
        ..profiles()[0]
    };
    let mut wp = Warp::identity(641.0, 427.0);
    // Explicitly attach an identity map too, to check packing independently of set_lensdb.
    wp.lensdb = Some(c.on(wp.w, wp.h));
    wp.lensdb_dist = 2.0;
    wp.lensdb_tca = 2.0;
    wp.lensdb_vig = 2.0;
    let p = warp_params(&wp, (641, 427), (641, 427), &Affine::IDENTITY, 1.0, 1.0);
    assert_eq!((p[65], p[69], p[76], p[63], p[64]), (0, 0, 0, 0, 0));
    // PA's nonpositive/near-zero denominator guard.
    for k in [-8.0, -0.9999995] {
        wp.lensdb = Some(LensCorrection { diag_norm: 1.0, vignetting: Some([k, 0.0, 0.0]), ..c }.on(wp.w, wp.h));
        let p = warp_params(&wp, (641, 427), (641, 427), &Affine::IDENTITY, 1.0, 1.0);
        let x = f32::from_bits(p[81]) + 1.0 / f32::from_bits(p[80]);
        assert_eq!(gain_f32(&p, x, f32::from_bits(p[82])), 1.0);
    }
}

#[test]
fn lens_database_coverage_matches_every_f64_pixel() {
    for orientation in ORIENTATIONS {
        for c in profiles() {
            let mut s = settings(orientation, 200.0);
            s.optics.distortion = -25.0;
            s.optics.ca_red = 100.0;
            s.geometry.horizontal = -15.0;
            s.geometry.vertical = 12.0;
            s.crop.geometry.angle = -4.5;
            s.crop.flip_h = true;
            let f = Frame::with_lenses(321, 211, &s, true, None, Some(&c));
            let wp = f.warp.as_ref().expect("warp");
            for (w, h) in [(173, 117), (64, 16), (1, 1)] {
                let a = f.out_to_oriented(w, h);
                let mask = coverage_mask(wp, &a, w, h);
                let words = w.div_ceil(32);
                for y in 0..h {
                    for x in 0..w {
                        let covered = mask[y * words + x / 32] & (1 << (x % 32)) != 0;
                        assert_eq!(covered, wp.covers(&a, x, y), "{orientation:?} {c:?} {x},{y}");
                    }
                    if !w.is_multiple_of(32) {
                        assert_eq!(mask[y * words + words - 1] >> (w % 32), 0);
                    }
                }
            }
        }
    }
}

#[test]
fn lens_database_gpu_uses_f64_coverage_at_rounding_boundary() {
    // Both source positions round to the same f32 x, but only one is covered.
    let mut xs = Vec::new();
    let mut cases = Vec::new();
    for (edge, covered) in [(-0.50000001, false), (-0.49999999, true)] {
        let mut wp = Warp::identity(32.0, 24.0);
        let mut m = profiles()[0].on(wp.w, wp.h);
        m.cx = 16.0;
        m.cy = 0.5;
        m.ns = 1.0;
        let u = 0.5 - m.cx;
        m.c.distortion = Distortion::Poly3 { k1: ((edge - m.cx) / u - 1.0) / (u * u) };
        m.c.tca = Tca::None;
        m.c.vignetting = None;
        wp.lensdb = Some(m);
        wp.lensdb_dist = 1.0;
        assert_eq!(wp.covers(&Affine::IDENTITY, 0, 0), covered);
        let p = warp_params(&wp, (32, 24), (1, 1), &Affine::IDENTITY, 1.0, 1.0);
        xs.push(database_f32(&p, 0.5, 0.5, 1).0);
        cases.push((wp, p, covered));
    }
    assert_eq!(xs[0], xs[1]);
    let Some(gpu) = crate::test_device() else {
        eprintln!("skipped: no GPU adapter (host rounding-boundary checks passed)");
        return;
    };
    let _scope = crate::ctx::RenderScope::new(gpu);
    let errors = crate::ctx::ErrorScopes::push(gpu);
    let src = gpu.upload(&[0.12f32; 32 * 24 * 3]);
    let mut cx = Cx::new(gpu);
    for (wp, p, covered) in cases {
        let mask = gpu.upload(&coverage_mask(&wp, &Affine::IDENTITY, 1, 1));
        let dst = gpu.buffer(3);
        cx.run("sample_warp", &p, &[Some(&src), Some(&dst), Some(&mask)], [1, 1, 1]);
        let v = cx.read::<f32>(&dst, 3);
        assert_eq!(v, if covered { vec![0.12; 3] } else { lightcraft_pipeline::geometry::BLANK.to_vec() });
    }
    assert!(errors.pop().is_none());
    assert!(!crate::ctx::failed());
}

#[test]
fn lens_database_renders_reuse_uploaded_source() {
    let Some(gpu) = crate::test_device() else {
        eprintln!("skipped: no GPU adapter");
        return;
    };
    let _scope = crate::ctx::RenderScope::new(gpu);
    let errors = crate::ctx::ErrorScopes::push(gpu);
    let src = Arc::new(lightcraft_scenes::demo_library()[2].render(321, 211));
    let mut info = SourceInfo { lens_db: Some(profiles()[0]), ..Default::default() };
    let req = RenderRequest::fit(321, 321);
    let stages = GpuStages::default();
    let mut uploaded = None;
    for (i, strength) in [100.0, 0.0, 200.0, 100.0].into_iter().enumerate() {
        info.lens_db = Some(profiles()[i % 3]);
        let s = settings(Orientation::Normal, strength);
        assert!(render(gpu, &src, &info, &s, &req, Some(&stages), None, lightcraft_pipeline::primary::DEFAULT_HS_METHOD).is_some());
        let source = stages.source.lock().unwrap_or_else(|e| e.into_inner());
        let b = source.as_ref().expect("lens frames upload the source rather than CPU resampling").uploaded.clone();
        if let Some(previous) = &uploaded {
            assert!(Arc::ptr_eq(previous, &b), "profile/strength changes retain the upload");
        }
        uploaded = Some(b);
        assert!(source.as_ref().is_some_and(|s| s.capture.is_none()));
        drop(source);
        if strength == 0.0 {
            let entry = stages.lock();
            assert!(
                Arc::ptr_eq(&entry.last().expect("cached frame").sampled, uploaded.as_ref().expect("upload")),
                "zero strength reuses the source buffer directly"
            );
        }
    }
    assert!(errors.pop().is_none());
    assert!(!crate::ctx::failed());
}

/// Compare the actual old geometry path to the new one at 24 MP, with GPU work
/// synchronized and no finish/readback in either timing. Full renders are also
/// covered by toolset::bench_toolset_24mp's lens rows.
#[test]
#[ignore = "24 MP before/after benchmark: run with --release --ignored"]
fn bench_lensdb_geometry_24mp() {
    let Some(gpu) = crate::test_device() else {
        eprintln!("skipped: no GPU adapter");
        return;
    };
    let _scope = crate::ctx::RenderScope::new(gpu);
    let errors = crate::ctx::ErrorScopes::push(gpu);
    let (w, h) = (6000, 4000);
    let src = Arc::new(lightcraft_scenes::demo_library()[3].render(w, h));
    let info = SourceInfo { lens_db: Some(profiles()[0]), ..Default::default() };
    let req = RenderRequest::fit(w, h);
    let mut cx = Cx::new(gpu);
    let uploaded = Arc::new(gpu.upload(rgb_words(&src)));
    for strength in [0.0, 100.0, 200.0] {
        let s = settings(Orientation::Normal, strength);
        let plan = lightcraft_pipeline::plan(&src, &info, &s, &req);
        let mut best = [f64::MAX; 3];
        // First pass warms kernels/pool; minimum of three timed repeats.
        for rep in 0..4 {
            for (path, b) in best.iter_mut().enumerate() {
                cx.sync();
                let t = std::time::Instant::now();
                let out = match path {
                    0 => {
                        let image = plan.frame.sample(&src, w, h);
                        Arc::new(gpu.upload(rgb_words(&image)))
                    }
                    1 => {
                        let b = Arc::new(gpu.upload(rgb_words(&src)));
                        sample(&mut cx, &src, b, &plan)
                    }
                    _ => sample(&mut cx, &src, uploaded.clone(), &plan),
                };
                cx.sync();
                if rep > 0 {
                    *b = b.min(t.elapsed().as_secs_f64() * 1e3);
                }
                std::hint::black_box(out);
            }
        }
        eprintln!(
            "24 MP lens {strength:3}%: before CPU geometry + upload {:.1} ms; after GPU upload + warp {:.1} ms; after GPU reused upload + warp {:.1} ms",
            best[0], best[1], best[2]
        );
    }
    assert!(errors.pop().is_none());
    assert!(!crate::ctx::failed());
}
