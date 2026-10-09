use photocraft_cms::{Builtin, Intent, Profile, SampleKind, Transform, TransformOptions, black_point, cached};
use std::sync::Arc;

fn srgb() -> &'static Profile {
    Builtin::Srgb.profile()
}

fn cmyk() -> &'static Profile {
    Builtin::CoatedCmyk.profile()
}

fn assert_close(a: &[f32], b: &[f32], eps: f32) {
    assert_eq!(a.len(), b.len(), "slice lengths differ");
    for (i, (x, y)) in a.iter().zip(b.iter()).enumerate() {
        assert!((*x - *y).abs() <= eps, "mismatch at index {}: {} vs {}", i, x, y);
    }
}

#[test]
fn transform_reports_inputs_outputs_and_options() {
    let opts = TransformOptions { intent: Intent::Perceptual, bpc: false, precise_float: false };
    let t = Transform::with_options(srgb(), cmyk(), opts).unwrap();
    assert_eq!(t.inputs(), 3);
    assert_eq!(t.outputs(), 4);
    assert_eq!(t.options(), opts);
}

#[test]
fn cmyk_identity_inputs_outputs() {
    let t = Transform::new(cmyk(), cmyk(), Intent::RelativeColorimetric, true).unwrap();
    assert_eq!(t.inputs(), 4);
    assert_eq!(t.outputs(), 4);
}

#[test]
fn cmyk_to_srgb_inputs_outputs() {
    let t = Transform::new(cmyk(), srgb(), Intent::RelativeColorimetric, true).unwrap();
    assert_eq!(t.inputs(), 4);
    assert_eq!(t.outputs(), 3);
}

#[test]
fn srgb_identity_no_bpc_float_exact() {
    let opts = TransformOptions { intent: Intent::RelativeColorimetric, bpc: false, precise_float: true };
    let t = Transform::with_options(srgb(), srgb(), opts).unwrap();
    let input = [0.25f32, 0.5, 0.75];
    let mut out = [0.0f32; 3];
    t.eval(&input, &mut out);
    assert_close(&out, &input, 1e-5);
}

#[test]
fn srgb_identity_bpc_preserves_white() {
    let opts = TransformOptions { intent: Intent::RelativeColorimetric, bpc: true, precise_float: true };
    let t = Transform::with_options(srgb(), srgb(), opts).unwrap();
    let mut out = [0.0f32; 3];
    t.eval(&[1.0, 1.0, 1.0], &mut out);
    assert_close(&out, &[1.0, 1.0, 1.0], 1e-4);
}

#[test]
fn eval_fast_matches_eval_for_cmyk_transform() {
    let t = Transform::new(cmyk(), srgb(), Intent::RelativeColorimetric, true).unwrap();
    assert!(t.fast_grid().is_some());
    let samples = [
        [0.0f32, 0.0, 0.0, 0.0],
        [1.0, 1.0, 1.0, 1.0],
        [0.1, 0.2, 0.3, 0.4],
        [0.5, 0.5, 0.5, 0.5],
        [0.9, 0.0, 0.1, 0.0],
        [0.25, 0.25, 0.25, 0.25],
        [0.3, 0.1, 0.2, 0.5],
        [0.7, 0.6, 0.5, 0.4],
        [0.15, 0.75, 0.05, 0.95],
    ];
    let mut checked = 0;
    for input in samples.iter() {
        let mut exact = [0.0f32; 3];
        let mut fast = [0.0f32; 3];
        t.eval(input, &mut exact);
        t.eval_fast(input, &mut fast);
        // Only compare when exact output lies within the valid display range;
        // fast path clips out-of-gamut values as it is designed for integer buffers.
        if exact.iter().all(|v| (0.0f32..=1.0f32).contains(v)) {
            assert_close(&exact, &fast, 0.05);
            checked += 1;
        }
    }
    assert!(checked > 0, "no in-gamut sample was found; test would be vacuous");
}

#[test]
fn convert_f32_extra_copy_true() {
    let opts = TransformOptions { intent: Intent::RelativeColorimetric, bpc: false, precise_float: true };
    let t = Transform::with_options(srgb(), srgb(), opts).unwrap();
    let src = [0.1f32, 0.2, 0.3, 0.9];
    let mut dst = [0.0f32; 4];
    t.convert_f32(&src, 4, &mut dst, 4, true);
    assert_close(&dst[0..3], &src[0..3], 1e-5);
    assert_eq!(dst[3], src[3]);
}

#[test]
fn convert_f32_extra_copy_false() {
    let opts = TransformOptions { intent: Intent::RelativeColorimetric, bpc: false, precise_float: true };
    let t = Transform::with_options(srgb(), srgb(), opts).unwrap();
    let src = [0.1f32, 0.2, 0.3, 0.9];
    let mut dst = [0.5f32; 4];
    t.convert_f32(&src, 4, &mut dst, 4, false);
    assert_close(&dst[0..3], &src[0..3], 1e-5);
    assert_eq!(dst[3], 0.5);
}

#[test]
fn convert_u8_extra_copy_true() {
    let opts = TransformOptions { intent: Intent::RelativeColorimetric, bpc: false, precise_float: true };
    let t = Transform::with_options(srgb(), srgb(), opts).unwrap();
    let src = [64u8, 128, 192, 255];
    let mut dst = [0u8; 4];
    t.convert_u8(&src, 4, &mut dst, 4, true);
    assert_eq!(&dst[0..3], &src[0..3]);
    assert_eq!(dst[3], src[3]);
}

#[test]
fn convert_u8_extra_copy_false() {
    let opts = TransformOptions { intent: Intent::RelativeColorimetric, bpc: false, precise_float: true };
    let t = Transform::with_options(srgb(), srgb(), opts).unwrap();
    let src = [64u8, 128, 192, 255];
    let mut dst = [123u8; 4];
    t.convert_u8(&src, 4, &mut dst, 4, false);
    assert_eq!(&dst[0..3], &src[0..3]);
    assert_eq!(dst[3], 123);
}

#[test]
fn convert_u16_does_not_panic_with_valid_strides() {
    let t = Transform::new(srgb(), cmyk(), Intent::RelativeColorimetric, true).unwrap();
    let src = [0u16, 65535, 32768];
    let mut dst = [0u16; 4];
    t.convert_u16(&src, 3, &mut dst, 4, false);
    assert_eq!(dst.len(), 4);
}

#[test]
fn convert_u16_extra_copy() {
    let opts = TransformOptions { intent: Intent::RelativeColorimetric, bpc: false, precise_float: true };
    let t = Transform::with_options(srgb(), srgb(), opts).unwrap();
    let src = [16384u16, 32768, 49152, 65535];
    let mut dst = [0u16; 4];
    t.convert_u16(&src, 4, &mut dst, 4, true);
    assert_eq!(&dst[0..3], &src[0..3]);
    assert_eq!(dst[3], src[3]);
}

#[test]
fn apply_inplace_identity_srgb() {
    let opts = TransformOptions { intent: Intent::RelativeColorimetric, bpc: false, precise_float: true };
    let t = Transform::with_options(srgb(), srgb(), opts).unwrap();
    let src = [0.1f32, 0.2, 0.3, 0.4, 0.5, 0.6];
    let mut buf = src;
    t.apply(&mut buf, 3);
    assert_close(&buf, &src, 1e-5);
}

#[test]
fn fast_grid_available_for_cmyk_transform() {
    let t = Transform::new(cmyk(), srgb(), Intent::RelativeColorimetric, true).unwrap();
    let fg = t.fast_grid().expect("expected a grid for CMYK transform");
    assert_eq!(fg.clut.inputs, 4);
    assert_eq!(fg.clut.outputs, 3);
    assert!(fg.post.is_some());
}

#[test]
fn fast_grid_none_for_analytic_transform() {
    let opts = TransformOptions { intent: Intent::RelativeColorimetric, bpc: false, precise_float: true };
    let t = Transform::with_options(srgb(), srgb(), opts).unwrap();
    assert!(t.fast_grid().is_none());
}

#[test]
fn black_point_srgb_is_zero() {
    let bp = black_point(srgb(), Intent::RelativeColorimetric, true).unwrap();
    assert!(bp.abs() < 1e-6, "expected zero black point, got {}", bp);
}

#[test]
fn black_point_cmyk_is_positive() {
    let bp = black_point(cmyk(), Intent::RelativeColorimetric, true).unwrap();
    assert!(bp > 0.0, "expected positive black point, got {}", bp);
}

#[test]
fn cached_returns_same_transform() {
    let opts = TransformOptions { intent: Intent::Perceptual, bpc: true, precise_float: true };
    let a = cached(srgb(), cmyk(), opts).unwrap();
    let b = cached(srgb(), cmyk(), opts).unwrap();
    assert!(Arc::ptr_eq(&a, &b));
}

#[test]
fn cached_different_options_gives_distinct_transforms() {
    let opts1 = TransformOptions { intent: Intent::Perceptual, bpc: true, precise_float: true };
    let opts2 = TransformOptions { intent: Intent::Saturation, bpc: true, precise_float: true };
    let a = cached(srgb(), cmyk(), opts1).unwrap();
    let b = cached(srgb(), cmyk(), opts2).unwrap();
    assert!(!Arc::ptr_eq(&a, &b));
}

#[test]
fn proof_transform_basic() {
    let t = Transform::proof(srgb(), cmyk(), srgb(), Intent::Perceptual, true, false).unwrap();
    assert_eq!(t.inputs(), 3);
    assert_eq!(t.outputs(), 3);
    let mut out = [0.0f32; 3];
    t.eval(&[0.2, 0.4, 0.8], &mut out);
    assert!(out.iter().all(|v| v.is_finite()));

    let t2 = Transform::proof(srgb(), cmyk(), srgb(), Intent::Perceptual, true, true).unwrap();
    assert_eq!(t2.inputs(), 3);
    assert_eq!(t2.outputs(), 3);
}

#[test]
fn convert_bytes_many_u8() {
    let opts = TransformOptions { intent: Intent::RelativeColorimetric, bpc: false, precise_float: true };
    let t = Transform::with_options(srgb(), srgb(), opts).unwrap();
    let src1 = [10u8, 20, 30, 255];
    let mut dst1 = [0u8; 4];
    let src2 = [100u8, 150, 200, 128];
    let mut dst2 = [0u8; 4];
    let jobs = vec![(&src1[..], &mut dst1[..]), (&src2[..], &mut dst2[..])];
    t.convert_bytes_many(SampleKind::U8, jobs, 4, 4, true);
    assert_eq!(dst1, src1);
    assert_eq!(dst2, src2);
}

#[test]
fn convert_bytes_many_f32() {
    let opts = TransformOptions { intent: Intent::RelativeColorimetric, bpc: false, precise_float: true };
    let t = Transform::with_options(srgb(), srgb(), opts).unwrap();
    let src1 = [0.1f32, 0.2, 0.3, 0.9];
    let src2 = [0.5f32, 0.6, 0.7, 0.1];
    let src1_bytes: Vec<u8> = src1.iter().flat_map(|f| f.to_ne_bytes()).collect();
    let src2_bytes: Vec<u8> = src2.iter().flat_map(|f| f.to_ne_bytes()).collect();
    let mut dst1_bytes = vec![0u8; src1_bytes.len()];
    let mut dst2_bytes = vec![0u8; src2_bytes.len()];
    let jobs = vec![(&src1_bytes[..], &mut dst1_bytes[..]), (&src2_bytes[..], &mut dst2_bytes[..])];
    t.convert_bytes_many(SampleKind::F32, jobs, 4, 4, true);
    let dst1: Vec<f32> = dst1_bytes.as_chunks::<4>().0.iter().map(|b| f32::from_ne_bytes(*b)).collect();
    let dst2: Vec<f32> = dst2_bytes.as_chunks::<4>().0.iter().map(|b| f32::from_ne_bytes(*b)).collect();
    assert_close(&dst1, &src1, 1e-5);
    assert_close(&dst2, &src2, 1e-5);
}

#[test]
fn round_trip_srgb_cmyk_srgb() {
    let to_cmyk = Transform::new(srgb(), cmyk(), Intent::RelativeColorimetric, true).unwrap();
    let to_srgb = Transform::new(cmyk(), srgb(), Intent::RelativeColorimetric, true).unwrap();
    let input = [0.2f32, 0.5, 0.8];
    let mut cmyk_values = [0.0f32; 4];
    to_cmyk.eval(&input, &mut cmyk_values);
    let mut out = [0.0f32; 3];
    to_srgb.eval(&cmyk_values, &mut out);
    // Round trip through CMYK is lossy for out-of-gamut colours; use a generous tolerance.
    assert_close(&input, &out, 0.1);
}
