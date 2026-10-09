use crate::common;
use common::*;
use image::RgbImage;
use pc_trace::{Fitter, Image, Params, Preset, trace};
use photocraft_testkit::vector as metrics;
use proptest::prelude::*;
#[test]
fn binary_counters_and_one_color() -> TestResult {
    let case = case(4, 128, Variant::Clean)?;
    let mut p = Params::for_preset(Preset::BlackWhite);
    p.speckle = 0;
    let out = trace(Image { width: 128, height: 128, rgba: case.input.as_raw() }, &p)?;
    assert_eq!(out.layers.len(), 1);
    let cov = photocraft_vector::path_coverage(&out.layers[0].path, photocraft_geom::Rect::new(0, 0, 128, 128));
    assert!(cov[64 * 128 + 64] < 0.01, "text counter filled");
    assert!(cov[64 * 128 + 25] > 0.99);
    assert!(out.nodes < 100);
    Ok(())
}
#[test]
fn alpha_ignore_singleton_and_budget() -> TestResult {
    for preset in [Preset::Logo, Preset::Photo, Preset::PixelArt] {
        let mut p = Params::for_preset(preset);
        p.speckle = 0;
        let transparent = trace(Image { width: 1, height: 1, rgba: &[23, 89, 5, 0] }, &p)?;
        assert!(transparent.layers.is_empty());
        let single = trace(Image { width: 1, height: 1, rgba: &[23, 89, 5, 255] }, &p)?;
        assert_eq!(single.layers.len(), 1);
        assert!(metrics::is_finite(&single.layers[0].path));
    }
    let p = Params { ignore_color: Some([255; 3]), ..Default::default() };
    assert!(trace(Image { width: 1, height: 1, rgba: &[255; 4] }, &p)?.layers.is_empty());
    assert!(matches!(trace(Image { width: 16000, height: 16000, rgba: &[] }, &Params::default()), Err(pc_trace::Error::PixelBudget(_))));
    assert!(trace(Image { width: 1, height: 1, rgba: &[] }, &Params::default()).is_err());
    Ok(())
}
#[test]
fn invalid_params_adapter_and_cancel() -> TestResult {
    for invalid in [f64::NAN, f64::INFINITY, -1.] {
        let p = Params { detail: invalid, ..Default::default() };
        assert!(trace(Image { width: 1, height: 1, rgba: &[255; 4] }, &p).is_err());
    }
    let mut bez = kurbo::BezPath::new();
    bez.move_to((0., 0.));
    bez.curve_to((1., 2.), (3., 4.), (0., 0.));
    bez.close_path();
    let p = pc_trace::adapter::bezpath_to_path(&bez)?;
    assert_eq!(p.subpaths[0].knots.len(), 2);
    assert_eq!(p.subpaths[0].knots[0].in_ctrl.x, 1.5);
    bez.move_to((f64::NAN, 0.));
    assert!(pc_trace::adapter::bezpath_to_path(&bez).is_err());
    let cancel = std::sync::atomic::AtomicBool::new(true);
    assert!(matches!(
        pc_trace::trace_cancellable(Image { width: 1, height: 1, rgba: &[255; 4] }, &Params::default(), &cancel),
        Err(pc_trace::Error::Cancelled)
    ));
    Ok(())
}
#[test]
fn fixed_palette_is_bounded_and_unique() -> TestResult {
    let c = case(25, 128, Variant::Jpeg)?;
    let p = Params { palette: vec![[255; 3], [0; 3], [200, 35, 55], [235, 160, 20]], ..Default::default() };
    let out = trace(Image { width: 128, height: 128, rgba: c.input.as_raw() }, &p)?;
    let colors: std::collections::BTreeSet<_> = out.layers.iter().map(|l| l.color).collect();
    assert_eq!(colors.len(), out.layers.len());
    assert!(out.layers.len() <= p.palette.len());
    for l in &out.layers {
        assert!(p.palette.contains(&[l.color[0], l.color[1], l.color[2]]));
    }
    Ok(())
}
#[test]
fn presets_serde_defaults_and_exact_pixels() -> TestResult {
    let p: Params = serde_json::from_str("{}")?;
    assert_eq!(p.fitter, Fitter::Potrace);
    assert_eq!(Params::for_preset(Preset::Photo).fitter, Fitter::Spline);
    let rgba = [0, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 0, 0, 0, 255];
    let p = Params::for_preset(Preset::PixelArt);
    let out = trace(Image { width: 2, height: 2, rgba: &rgba }, &p)?;
    assert_eq!(render(&out).as_raw(), &[0, 0, 0, 255, 255, 255, 255, 255, 255, 0, 0, 0]);
    Ok(())
}
proptest! {
 #![proptest_config(ProptestConfig::with_cases(256))]
 #[test]
 fn binary_outputs_are_finite_closed_and_simple((w,h,values) in (1usize..13,1usize..13).prop_flat_map(|(w,h)|(Just(w),Just(h),prop::collection::vec(any::<bool>(),w*h)))) {
  let rgba:Vec<u8>=values.into_iter().flat_map(|b|if b{[0,0,0,255]}else{[255;4]}).collect();
  let p=Params{speckle:0,..Params::for_preset(Preset::BlackWhite)};
  let out=trace(Image{width:w as u32,height:h as u32,rgba:&rgba},&p).map_err(|e|TestCaseError::fail(e.to_string()))?;
  for l in &out.layers {prop_assert!(metrics::is_finite(&l.path));prop_assert!(l.path.subpaths.iter().all(|s|s.closed));prop_assert!(metrics::self_intersections(&l.path).is_empty(),"intersections: {:?}",metrics::self_intersections(&l.path));}
 }
 #[test]
 fn deterministic_color_and_thread_counts((w,h,values) in (1usize..9,1usize..9).prop_flat_map(|(w,h)|(Just(w),Just(h),prop::collection::vec(0u8..4,w*h)))) {
  let rgba:Vec<u8>=values.into_iter().flat_map(|v|[[0,0,0,255],[255;4],[200,20,40,255],[10,60,190,0]][v as usize]).collect();let p=Params{speckle:0,max_colors:3,..Default::default()};let image=Image{width:w as u32,height:h as u32,rgba:&rgba};let a=trace(image,&p).map_err(|e|TestCaseError::fail(e.to_string()))?;let b=trace(image,&p).map_err(|e|TestCaseError::fail(e.to_string()))?;prop_assert_eq!(serde_json::to_vec(&a).map_err(|e|TestCaseError::fail(e.to_string()))?,serde_json::to_vec(&b).map_err(|e|TestCaseError::fail(e.to_string()))?);
  for l in &a.layers {prop_assert!(metrics::is_finite(&l.path));prop_assert!(metrics::self_intersections(&l.path).is_empty());}
 }
}
#[test]
fn adaptive_scan_and_worker_threads() -> TestResult {
    let c = case(3, 128, Variant::Scan)?;
    let mut p = Params::for_preset(Preset::BlackWhite);
    p.adaptive = true;
    p.adaptive_window = 64;
    let out = trace(Image { width: 128, height: 128, rgba: c.input.as_raw() }, &p)?;
    assert!(!out.layers.is_empty());
    let expected = serde_json::to_vec(&out)?;
    std::thread::scope(|scope| -> TestResult {
        let workers: Vec<_> = (0..3).map(|_| scope.spawn(|| trace(Image { width: 128, height: 128, rgba: c.input.as_raw() }, &p))).collect();
        for w in workers {
            let out = w.join().map_err(|_| "worker panicked")??;
            assert_eq!(serde_json::to_vec(&out)?, expected);
        }
        Ok(())
    })?;
    Ok(())
}
#[test]
fn photo_watershed_regions_and_skinny_inputs() -> TestResult {
    for (w, h) in [(32, 32), (65, 1), (1, 65)] {
        let rgba: Vec<u8> =
            (0..w * h).flat_map(|i| if if w == 1 { i < h / 2 } else { i % w < w / 2 } { [10, 30, 200, 255] } else { [210, 30, 40, 255] }).collect();
        let p = Params { speckle: 0, ..Params::for_preset(Preset::Photo) };
        let out = trace(Image { width: w, height: h, rgba: &rgba }, &p)?;
        assert!(out.layers.len() >= 2, "watershed discarded a contrasting region");
        let result = render(&out);
        let reference = RgbImage::from_raw(w, h, rgba.as_chunks::<4>().0.iter().flat_map(|p| p[..3].iter().copied()).collect()).ok_or("raster")?;
        assert!(metrics::fidelity(&reference, &result)?.fidelity > 0.99);
        for l in &out.layers {
            assert!(metrics::self_intersections(&l.path).is_empty());
        }
    }
    Ok(())
}
#[test]
fn tiny_alpha_holes_survive_keying_and_cancellation_during_clustering() -> TestResult {
    let mut rgba = vec![255; 16 * 16 * 4];
    rgba[(8 * 16 + 8) * 4 + 3] = 0;
    let p = Params { speckle: 0, ..Default::default() };
    let out = trace(Image { width: 16, height: 16, rgba: &rgba }, &p)?;
    assert_eq!(out.layers.len(), 1);
    let coverage = photocraft_vector::path_coverage(&out.layers[0].path, photocraft_geom::Rect::new(0, 0, 16, 16));
    assert!(coverage[8 * 16 + 8] < 0.5);
    let calls = std::cell::Cell::new(0);
    let result = pc_trace::trace_with_cancel(Image { width: 16, height: 16, rgba: &rgba }, &p, || {
        let n = calls.get();
        calls.set(n + 1);
        n >= 2
    });
    assert!(matches!(result, Err(pc_trace::Error::Cancelled)));
    Ok(())
}
