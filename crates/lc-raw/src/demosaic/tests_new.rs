use super::*;
fn random(w: usize, h: usize, cfa: Cfa) -> Normalized {
    Normalized { width: w, height: h, cpp: 1, data: (0..w * h).map(crate::test_vectors::noise).collect(), cfa: Some(cfa) }
}
#[test]
fn pinned_darktable_markesteijn_all_36_phases() {
    for dy in 0..6 {
        for dx in 0..6 {
            let n = random(257, 251, Cfa::xtrans().shifted(dx, dy));
            let out = demosaic(&n, Method::Markesteijn3);
            let mut samples = Vec::new();
            for y in (20..231).step_by(7) {
                for x in (20..237).step_by(7) {
                    samples.extend(out.get(x, y));
                }
            }
            crate::test_vectors::compare(&format!("markesteijn/{dx}-{dy}.f32"), &samples, 2e-6);
        }
    }
}
#[test]
fn pinned_lmmse_and_dcb_all_bayer_phases() {
    for pat in ["RGGB", "BGGR", "GRBG", "GBRG"] {
        let n = random(128, 128, Cfa::bayer(pat).unwrap());
        for (method, name) in [(Method::Lmmse, "lmmse"), (Method::Dcb, "dcb")] {
            let out = demosaic(&n, method);
            let mut samples = Vec::new();
            for y in (24..104).step_by(3) {
                for x in (24..104).step_by(3) {
                    samples.extend(out.get(x, y));
                }
            }
            crate::test_vectors::compare(&format!("{name}/{pat}.f32"), &samples, 3e-6);
        }
    }
}
#[test]
fn xtrans_quality_beats_directional_and_dual_zero_is_exact() {
    let mut gains = 0.0;
    for kind in 0..3 {
        let truth = crate::quality_support::scene(kind, 256, 256);
        let n = mosaic_from_rgb(&truth, &Cfa::xtrans());
        let old = demosaic(&n, Method::XtransDirectional);
        let high = demosaic(&n, Method::Markesteijn3);
        let (p, q) = (psnr(&truth, &old, 20), psnr(&truth, &high, 20));
        eprintln!("xtrans,{kind},old={p:.6},markesteijn={q:.6}");
        gains += q - p;
        // Achromatic frequency charts specifically expose X-Trans maze/zipper artifacts.
        if kind < 2 {
            assert!(q > p + 10.0, "{kind}: {q} <= {p}+10");
        }
        let dual = demosaic_with(&n, Method::DualMarkesteijnVng, &DemosaicOptions { dual_threshold: 0.0 });
        assert_eq!(high.data, dual.data);
        let dual = demosaic(&n, Method::DualMarkesteijnVng);
        assert!(dual.data.iter().flatten().all(|v| v.is_finite()));
    }
    assert!(gains / 3.0 > 8.0, "mean CPSNR gain {gains}/3");
}
#[test]
fn new_methods_thread_determinism_and_degenerate_edges() {
    let one = rayon::ThreadPoolBuilder::new().num_threads(1).build().unwrap();
    let four = rayon::ThreadPoolBuilder::new().num_threads(4).build().unwrap();
    for cfa in [Cfa::bayer("GBRG").unwrap(), Cfa::xtrans().shifted(3, 5)] {
        let n = random(451, 401, cfa);
        for method in [Method::Lmmse, Method::Dcb, Method::Markesteijn3, Method::DualMarkesteijnVng] {
            let a = one.install(|| demosaic(&n, method));
            let b = four.install(|| demosaic(&n, method));
            assert_eq!(a.data, b.data, "{method:?}");
        }
    }
    for (w, h) in [(1, 1), (2, 3), (35, 37), (36, 36), (89, 89), (177, 178), (222, 223), (223, 222)] {
        for method in [Method::Markesteijn3, Method::DualMarkesteijnVng] {
            let out = demosaic(&random(w, h, Cfa::xtrans()), method);
            assert_eq!(out.data.len(), w * h);
            assert!(out.data.iter().flatten().all(|v| v.is_finite()));
        }
    }
}
#[test]
fn xtrans_constant_colour_and_sample_preservation_all_phases() {
    let img = RgbFrame::filled(67, 71, [0.3, 0.55, 0.8]);
    for dy in 0..6 {
        for dx in 0..6 {
            let cfa = Cfa::xtrans().shifted(dx, dy);
            let n = mosaic_from_rgb(&img, &cfa);
            for method in [Method::Markesteijn3, Method::DualMarkesteijnVng] {
                let out = demosaic(&n, method);
                assert!(out.data.iter().all(|p| p.iter().zip(img.data[0]).all(|(a, b)| (a - b).abs() < 2e-6)));
            }
            let n = random(67, 71, cfa);
            let out = demosaic(&n, Method::Markesteijn3);
            for y in 0..n.height {
                for x in 0..n.width {
                    let c = n.cfa.as_ref().unwrap().color_at(x, y) as usize;
                    assert!((out.get(x, y)[c] - n.data[y * n.width + x]).abs() < 2e-7);
                }
            }
        }
    }
}

#[test]
fn automatic_noise_selection_and_overrides() {
    let truth = RgbFrame::from_fn(384, 384, |x, y| [0.3 + 0.0002 * x as f32, 0.4 + 0.0001 * y as f32, 0.2]);
    for pat in ["RGGB", "BGGR", "GRBG", "GBRG"] {
        let mut n = mosaic_from_rgb(&truth, &Cfa::bayer(pat).unwrap());
        assert_eq!(automatic_method(&n), Method::Rcd);
        let mut seed = 123u32;
        for v in &mut n.data {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let a = ((seed >> 8) as f32 + 0.5) / 16777216.0;
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let b = ((seed >> 8) as f32 + 0.5) / 16777216.0;
            *v += 0.01 * (-2.0 * a.ln()).sqrt() * (std::f32::consts::TAU * b).cos();
        }
        let sigma = noise_sigma(&n);
        assert!((0.0085..0.011).contains(&sigma), "{pat}: {sigma}");
        assert_eq!(automatic_method(&n), Method::DualRcdVng);
        assert_eq!(demosaic(&n, Method::Auto).data, demosaic(&n, Method::DualRcdVng).data);
        let one = rayon::ThreadPoolBuilder::new().num_threads(1).build().unwrap();
        assert_eq!(one.install(|| noise_sigma(&n)), sigma);
    }
}
