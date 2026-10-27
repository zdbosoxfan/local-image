use super::*;

#[test]
fn shared_matte_host_dispatch_contract() {
    use lightcraft_develop::{Mask, MaskComponent, MaskOp, MaskShape, SegMask};
    let mut mask = Mask {
        refine: 100.0,
        components: vec![MaskComponent {
            name: None,
            op: MaskOp::Add,
            invert: false,
            shape: MaskShape::Subject { seg: Some(SegMask::from_logits(4, &[12.0; 16])) },
        }],
        ..Default::default()
    };
    assert!(lightcraft_pipeline::masks::uses_subject_matting(&mask));
    mask.refine = 0.0;
    assert!(!lightcraft_pipeline::masks::uses_subject_matting(&mask));
    mask.refine = 100.0;
    mask.components[0].shape = MaskShape::DepthRange { lo: 0.2, hi: 0.8, feather: 0.1, seg: None };
    assert!(!lightcraft_pipeline::masks::uses_subject_matting(&mask));
    mask.components[0].shape = MaskShape::Full;
    assert!(!lightcraft_pipeline::masks::uses_subject_matting(&mask));
}

#[test]
fn capture_source_cache_reuses_upload_and_invalidates_parameters() {
    let gpu_device = crate::test_device().unwrap_or_else(|| panic!("real GPU required: {:?}", crate::unavailable_reason()));
    let gpu = &*gpu_device;
    let _scope = crate::ctx::RenderScope::new(gpu);
    let errors = crate::ctx::ErrorScopes::push(gpu);
    let mut cx = Cx::new(gpu);
    let stages = GpuStages::default();
    let src = Arc::new(lightcraft_scenes::demo_library()[3].render(73, 49));
    let p = CaptureParams { sigma: 0.8, ..Default::default() };
    let original = stages.source(&mut cx, &src, None, &SourceInfo::default());
    let sharp = stages.source(&mut cx, &src, Some(p), &SourceInfo::default());
    assert!(!Arc::ptr_eq(&original, &sharp));
    assert!(Arc::ptr_eq(&sharp, &stages.source(&mut cx, &src, Some(p), &SourceInfo::default())));
    assert!(Arc::ptr_eq(&original, &stages.source(&mut cx, &src, None, &SourceInfo::default())));
    assert!(Arc::ptr_eq(&sharp, &stages.source(&mut cx, &src, Some(p), &SourceInfo::default())), "off/on reuses the same capture result");
    assert_eq!(stages.bytes(), 73 * 49 * 3 * 4 * 2, "upload and sharpened source counted once");
    let changed = stages.source(&mut cx, &src, Some(CaptureParams { threshold: 0.8, ..p }), &SourceInfo::default());
    assert!(!Arc::ptr_eq(&sharp, &changed), "parameter change reruns capture");
    assert!(Arc::ptr_eq(&original, &stages.source(&mut cx, &src, None, &SourceInfo::default())), "capture change preserves the original upload");
    let new_src = Arc::new((*src).clone());
    let new_upload = stages.source(&mut cx, &new_src, None, &SourceInfo::default());
    assert!(!Arc::ptr_eq(&original, &new_upload), "decoder source identity invalidates upload");
    assert_eq!(stages.bytes(), 73 * 49 * 3 * 4);
    stages.clear();
    assert_eq!(stages.bytes(), 0);
    cx.flush();
    assert!(errors.pop().is_none());
    assert!(!crate::ctx::failed());
}

#[test]
fn coverage_mask_matches_per_pixel_decision() {
    use lightcraft_develop::{EmbeddedLens, EmbeddedWarp};
    let lens = EmbeddedLens {
        warp: Some(EmbeddedWarp {
            fisheye: false,
            planes: [[1.0, -0.03, 0.01, 0.0, 0.001, -0.002]; 3],
            center: lightcraft_geom::Point::new(0.52, 0.48),
            radius: 0.6,
        }),
        vignette: None,
        radial: None,
    };
    let mut s = DevelopSettings::default();
    s.optics.lens_profile = true;
    s.optics.distortion = -40.0;
    s.geometry.horizontal = -15.0;
    s.geometry.rotate = 3.0;
    for orientation in [Orientation::Normal, Orientation::Rotate270] {
        s.orientation = orientation;
        let f = lightcraft_pipeline::geometry::Frame::with_lens(900, 600, &s, true, Some(&lens));
        let wp = f.warp.as_ref().expect("warp");
        // widths that do and don't fill the last word; heights that do and don't fill the last band
        for (w, h) in [f.fit(700, 700), (333, 221), (64, 16), (1, 1)] {
            let o2t = f.out_to_oriented(w, h);
            let mask = coverage_mask(wp, &o2t, w, h);
            let words = w.div_ceil(32);
            assert_eq!(mask.len(), words * h);
            for y in 0..h {
                for x in 0..w {
                    let bit = mask[y * words + x / 32] >> (x % 32) & 1 == 1;
                    assert_eq!(bit, wp.covers(&o2t, x, y), "{orientation:?} {w}x{h}: pixel {x},{y}");
                }
                let pad = mask[y * words + words - 1] >> (w % 32);
                assert!(w.is_multiple_of(32) || pad == 0, "padding bits set");
            }
        }
    }
}

/// The perspective horizon in view: its denominator is exactly 0 on output row 75 (clamped to 1e-300) and
/// changes sign there, so the blocks around it are undecided (clamp, and division by a range that may hold
/// zero) and are decided per pixel, while blocks clear of it are filled at once.
#[test]
fn coverage_mask_with_the_horizon_in_view() {
    use lightcraft_geom::{Affine, Homography};
    let (w, h) = (900, 600);
    let mut wp = lightcraft_pipeline::optics::Warp::identity(w as f64, h as f64);
    // centred coordinates: v = (y − 300) / 450, denominator 2·v + m8, exactly 0 at y = 75.5
    let m8 = -((75.5 - 300.0) / 450.0 * 2.0);
    wp.persp_inv = Homography([1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 2.0, m8]);
    wp.persp = wp.persp_inv.inverse().expect("invertible");
    let o2t = Affine::IDENTITY;
    assert_eq!(wp.block_coverage(&o2t, 0, 32, 64, 80), None, "the block holding the horizon");
    assert_eq!(wp.block_coverage(&o2t, 0, 32, 0, 16), Some(false), "beyond the horizon");
    assert_eq!(wp.block_coverage(&o2t, 448, 480, 288, 304), Some(true), "the centre");
    let mask = coverage_mask(&wp, &o2t, w, h);
    let words = w.div_ceil(32);
    let mut covered = 0;
    for y in 0..h {
        for x in 0..w {
            let bit = mask[y * words + x / 32] >> (x % 32) & 1 == 1;
            assert_eq!(bit, wp.covers(&o2t, x, y), "pixel {x},{y}");
            covered += bit as usize;
        }
    }
    assert!(covered > 0 && covered < w * h, "{covered} px covered");
}

#[test]
fn orient_map_matches_image_oriented() {
    use Orientation::*;
    let img = RgbFrame::from_fn(5, 3, |x, y| [x as f32, y as f32, 0.0]);
    for o in [Normal, Rotate90, Rotate180, Rotate270, FlipH, Transverse, FlipV, Transpose] {
        let r = img.oriented(o);
        let m = orient_map(o, 5, 3);
        for y in 0..r.height {
            for x in 0..r.width {
                let (x, y) = (x as i32, y as i32);
                let (sx, sy) = (m[0] * x + m[1] * y + m[2], m[3] * x + m[4] * y + m[5]);
                assert_eq!(r.get(x as usize, y as usize), img.get(sx as usize, sy as usize), "{o:?} at {x},{y}");
            }
        }
    }
}
