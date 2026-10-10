//! Integration tests for the public blur and artistic filter APIs.
//!
//! Synthetic images only; no files, no network. All tests are fast and use
//! small surfaces.

use photocraft_algo::{FilterParams, GalleryEffect, GalleryFilter, RadialMethod, apply, apply_in, apply_tiled};
use photocraft_color::{ColorMode, PixelFormat, SampleType};
use photocraft_geom::Rect;
use photocraft_raster::Surface;

fn rgba_surface(w: i32, h: i32, sample: SampleType, data: &[f32]) -> Surface {
    let fmt = PixelFormat::new(ColorMode::Rgb, sample, true);
    let mut s = Surface::new(fmt);
    s.write_region(Rect::new(0, 0, w, h), data);
    s
}

fn read_region(s: &Surface, r: Rect) -> Vec<f32> {
    s.read_region(r)
}

fn bounds(w: i32, h: i32) -> Rect {
    Rect::new(0, 0, w, h)
}

fn gradient_rgba(w: i32, h: i32) -> Vec<f32> {
    let mut v = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let r = x as f32 / (w - 1).max(1) as f32;
            let g = y as f32 / (h - 1).max(1) as f32;
            let b = 0.5;
            v.extend_from_slice(&[r, g, b, 1.0]);
        }
    }
    v
}

fn assert_approx_eq(a: &[f32], b: &[f32], tol: f32) {
    assert_eq!(a.len(), b.len());
    for (x, y) in a.iter().zip(b) {
        assert!((x - y).abs() <= tol, "mismatch: {x} vs {y}");
    }
}

#[test]
fn gaussian_radius_zero_is_identity() {
    let (w, h) = (16, 12);
    let data = gradient_rgba(w, h);
    let s = rgba_surface(w, h, SampleType::F32, &data);
    let p = FilterParams::GaussianBlur { radius: 0.0 };
    let out = apply(&s, &p, bounds(w, h), bounds(w, h), None);
    assert_approx_eq(&read_region(&out, bounds(w, h)), &data, 0.0);
}

#[test]
fn box_radius_zero_is_identity() {
    let (w, h) = (16, 12);
    let data = gradient_rgba(w, h);
    let s = rgba_surface(w, h, SampleType::F32, &data);
    let p = FilterParams::BoxBlur { radius: 0.0 };
    let out = apply(&s, &p, bounds(w, h), bounds(w, h), None);
    assert_approx_eq(&read_region(&out, bounds(w, h)), &data, 0.0);
}

#[test]
fn motion_distance_zero_is_identity() {
    let (w, h) = (16, 12);
    let data = gradient_rgba(w, h);
    let s = rgba_surface(w, h, SampleType::F32, &data);
    let p = FilterParams::MotionBlur { angle: 30.0, distance: 0.0 };
    let out = apply(&s, &p, bounds(w, h), bounds(w, h), None);
    assert_approx_eq(&read_region(&out, bounds(w, h)), &data, 0.0);
}

#[test]
fn radial_amount_zero_is_identity() {
    let (w, h) = (16, 12);
    let data = gradient_rgba(w, h);
    let s = rgba_surface(w, h, SampleType::F32, &data);
    for method in [RadialMethod::Spin, RadialMethod::Zoom] {
        let p = FilterParams::RadialBlur { amount: 0.0, method, center_x: 0.5, center_y: 0.5 };
        let out = apply(&s, &p, bounds(w, h), bounds(w, h), None);
        assert_approx_eq(&read_region(&out, bounds(w, h)), &data, 0.0);
    }
}

#[test]
fn surface_radius_zero_is_identity() {
    let (w, h) = (16, 12);
    let data = gradient_rgba(w, h);
    let s = rgba_surface(w, h, SampleType::F32, &data);
    let p = FilterParams::SurfaceBlur { radius: 0.0, threshold: 20.0 };
    let out = apply(&s, &p, bounds(w, h), bounds(w, h), None);
    assert_approx_eq(&read_region(&out, bounds(w, h)), &data, 0.0);
}

#[test]
fn gaussian_constant_image_stays_constant() {
    let (w, h) = (15, 15);
    let c = [0.2, 0.3, 0.4, 1.0];
    let data: Vec<f32> = (0..w * h).flat_map(|_| c).collect();
    let s = rgba_surface(w, h, SampleType::F32, &data);
    let p = FilterParams::GaussianBlur { radius: 4.0 };
    let out = apply_in(&s, &p, bounds(w, h), bounds(w, h), None, bounds(w, h));
    let got = read_region(&out, bounds(w, h));
    for px in got.as_chunks::<4>().0 {
        assert_approx_eq(px, &c, 1e-6);
    }
}

#[test]
fn box_constant_image_stays_constant() {
    let (w, h) = (15, 15);
    let c = [0.6, 0.1, 0.9, 1.0];
    let data: Vec<f32> = (0..w * h).flat_map(|_| c).collect();
    let s = rgba_surface(w, h, SampleType::F32, &data);
    let p = FilterParams::BoxBlur { radius: 5.0 };
    let out = apply_in(&s, &p, bounds(w, h), bounds(w, h), None, bounds(w, h));
    let got = read_region(&out, bounds(w, h));
    for px in got.as_chunks::<4>().0 {
        assert_approx_eq(px, &c, 1e-6);
    }
}

#[test]
fn motion_constant_image_stays_constant() {
    let (w, h) = (15, 15);
    let c = [0.15, 0.65, 0.35, 1.0];
    let data: Vec<f32> = (0..w * h).flat_map(|_| c).collect();
    let s = rgba_surface(w, h, SampleType::F32, &data);
    let p = FilterParams::MotionBlur { angle: 45.0, distance: 12.0 };
    let out = apply_in(&s, &p, bounds(w, h), bounds(w, h), None, bounds(w, h));
    let got = read_region(&out, bounds(w, h));
    for px in got.as_chunks::<4>().0 {
        assert_approx_eq(px, &c, 1e-6);
    }
}

#[test]
fn radial_constant_image_stays_constant() {
    let (w, h) = (15, 15);
    let c = [0.75, 0.25, 0.55, 1.0];
    let data: Vec<f32> = (0..w * h).flat_map(|_| c).collect();
    let s = rgba_surface(w, h, SampleType::F32, &data);
    for method in [RadialMethod::Spin, RadialMethod::Zoom] {
        let p = FilterParams::RadialBlur { amount: 40.0, method, center_x: 0.5, center_y: 0.5 };
        let out = apply_in(&s, &p, bounds(w, h), bounds(w, h), None, bounds(w, h));
        let got = read_region(&out, bounds(w, h));
        for px in got.as_chunks::<4>().0 {
            assert_approx_eq(px, &c, 1e-6);
        }
    }
}

fn const_rgba(w: i32, h: i32, c: [f32; 4]) -> Surface {
    let data: Vec<f32> = (0..w * h).flat_map(|_| c).collect();
    rgba_surface(w, h, SampleType::F32, &data)
}

#[test]
fn radial_constant_image_any_centre_size_and_amount() {
    let c = [0.2, 0.6, 0.9, 1.0];
    for (w, h) in [(1, 1), (2, 3), (7, 5), (15, 15), (33, 20)] {
        let s = const_rgba(w, h, c);
        for method in [RadialMethod::Spin, RadialMethod::Zoom] {
            for (cx, cy) in [(0.5, 0.5), (0.0, 0.0), (1.0, 1.0), (0.1, 0.9), (-1.0, 0.5), (2.5, -0.7)] {
                for amount in [1.0, 40.0, 100.0] {
                    let p = FilterParams::RadialBlur { amount, method, center_x: cx, center_y: cy };
                    let out = apply_in(&s, &p, bounds(w, h), bounds(w, h), None, bounds(w, h));
                    for px in read_region(&out, bounds(w, h)).as_chunks::<4>().0 {
                        assert!(px.iter().all(|v| v.is_finite()));
                        assert_approx_eq(px, &c, 1e-5);
                    }
                    // The plain (no extent) entry point also repeats the bounds edge.
                    let out = apply(&s, &p, bounds(w, h), bounds(w, h), None);
                    for px in read_region(&out, bounds(w, h)).as_chunks::<4>().0 {
                        assert_approx_eq(px, &c, 1e-5);
                    }
                }
            }
        }
    }
}

#[test]
fn radial_keeps_interior_transparency() {
    // Left half opaque, right half fully transparent: the transparent half stays mostly
    // transparent far from the seam, and the opaque half stays opaque far from the seam.
    let (w, h) = (40, 20);
    let mut data = Vec::new();
    for _y in 0..h {
        for x in 0..w {
            data.extend_from_slice(&if x < w / 2 { [0.5, 0.5, 0.5, 1.0] } else { [0.0, 0.0, 0.0, 0.0] });
        }
    }
    let s = rgba_surface(w, h, SampleType::F32, &data);
    for method in [RadialMethod::Spin, RadialMethod::Zoom] {
        let p = FilterParams::RadialBlur { amount: 10.0, method, center_x: 0.5, center_y: 0.5 };
        let out = apply_in(&s, &p, bounds(w, h), bounds(w, h), None, bounds(w, h));
        let got = read_region(&out, bounds(w, h));
        assert!(got.iter().all(|v| v.is_finite()));
        let at = |x: i32, y: i32| got[((y * w + x) * 4) as usize + 3];
        assert!(at(30, 10) < 0.05, "transparent side picked up alpha: {}", at(30, 10));
        assert!(at(5, 10) > 0.95, "opaque side lost alpha: {}", at(5, 10));
        assert!(at(0, 0) > 0.9 && at(0, h - 1) > 0.9, "opaque corners faded");
    }
}

#[test]
fn motion_constant_image_stays_constant_at_edges() {
    let c = [0.3, 0.4, 0.5, 1.0];
    for (w, h) in [(1, 1), (3, 4), (15, 15)] {
        let s = const_rgba(w, h, c);
        for angle in [0.0, 45.0, 90.0, 135.0] {
            let p = FilterParams::MotionBlur { angle, distance: 20.0 };
            let out = apply_in(&s, &p, bounds(w, h), bounds(w, h), None, bounds(w, h));
            for px in read_region(&out, bounds(w, h)).as_chunks::<4>().0 {
                assert_approx_eq(px, &c, 1e-5);
            }
        }
    }
}

#[test]
fn surface_constant_image_stays_constant() {
    let (w, h) = (15, 15);
    let c = [0.45, 0.55, 0.65, 1.0];
    let data: Vec<f32> = (0..w * h).flat_map(|_| c).collect();
    let s = rgba_surface(w, h, SampleType::F32, &data);
    let p = FilterParams::SurfaceBlur { radius: 3.0, threshold: 50.0 };
    let out = apply(&s, &p, bounds(w, h), bounds(w, h), None);
    let got = read_region(&out, bounds(w, h));
    for px in got.as_chunks::<4>().0 {
        assert_approx_eq(px, &c, 1e-6);
    }
}

#[test]
fn gaussian_output_finite_and_in_range() {
    let (w, h) = (20, 16);
    let data = gradient_rgba(w, h);
    let s = rgba_surface(w, h, SampleType::F32, &data);
    let p = FilterParams::GaussianBlur { radius: 6.0 };
    let out = apply(&s, &p, bounds(w, h), bounds(w, h), None);
    let got = read_region(&out, bounds(w, h));
    assert!(got.iter().all(|v| v.is_finite()));
    assert!(got.iter().all(|v| (0.0..=1.0).contains(v)));
}

#[test]
fn box_output_finite_and_in_range() {
    let (w, h) = (20, 16);
    let data = gradient_rgba(w, h);
    let s = rgba_surface(w, h, SampleType::F32, &data);
    let p = FilterParams::BoxBlur { radius: 8.0 };
    let out = apply(&s, &p, bounds(w, h), bounds(w, h), None);
    let got = read_region(&out, bounds(w, h));
    assert!(got.iter().all(|v| v.is_finite()));
    assert!(got.iter().all(|v| (0.0..=1.0).contains(v)));
}

#[test]
fn motion_output_finite_and_in_range() {
    let (w, h) = (20, 16);
    let data = gradient_rgba(w, h);
    let s = rgba_surface(w, h, SampleType::F32, &data);
    let p = FilterParams::MotionBlur { angle: 0.0, distance: 10.0 };
    let out = apply(&s, &p, bounds(w, h), bounds(w, h), None);
    let got = read_region(&out, bounds(w, h));
    assert!(got.iter().all(|v| v.is_finite()));
    assert!(got.iter().all(|v| (0.0..=1.0).contains(v)));
}

#[test]
fn surface_output_finite_and_in_range() {
    let (w, h) = (20, 16);
    let data = gradient_rgba(w, h);
    let s = rgba_surface(w, h, SampleType::F32, &data);
    let p = FilterParams::SurfaceBlur { radius: 4.0, threshold: 40.0 };
    let out = apply(&s, &p, bounds(w, h), bounds(w, h), None);
    let got = read_region(&out, bounds(w, h));
    assert!(got.iter().all(|v| v.is_finite()));
    assert!(got.iter().all(|v| (0.0..=1.0).contains(v)));
}

#[test]
fn gaussian_symmetric_input_symmetric_output() {
    let (w, h) = (21, 13);
    let mut data = Vec::new();
    for _y in 0..h {
        for x in 0..w {
            let v = (x as f32 - 10.0).abs() / 10.0;
            data.extend_from_slice(&[v, 1.0 - v, 0.5, 1.0]);
        }
    }
    let s = rgba_surface(w, h, SampleType::F32, &data);
    let p = FilterParams::GaussianBlur { radius: 3.0 };
    let out = apply(&s, &p, bounds(w, h), bounds(w, h), None);
    let got = read_region(&out, bounds(w, h));
    for y in 0..h {
        for x in 0..w {
            let xm = w - 1 - x;
            let i = (y * w + x) as usize * 4;
            let j = (y * w + xm) as usize * 4;
            for c in 0..4 {
                assert!((got[i + c] - got[j + c]).abs() < 1e-6);
            }
        }
    }
}

#[test]
fn box_symmetric_input_symmetric_output() {
    let (w, h) = (21, 13);
    let mut data = Vec::new();
    for _y in 0..h {
        for x in 0..w {
            let v = (x as f32 - 10.0).abs() / 10.0;
            data.extend_from_slice(&[v, 1.0 - v, 0.5, 1.0]);
        }
    }
    let s = rgba_surface(w, h, SampleType::F32, &data);
    let p = FilterParams::BoxBlur { radius: 4.0 };
    let out = apply(&s, &p, bounds(w, h), bounds(w, h), None);
    let got = read_region(&out, bounds(w, h));
    for y in 0..h {
        for x in 0..w {
            let xm = w - 1 - x;
            let i = (y * w + x) as usize * 4;
            let j = (y * w + xm) as usize * 4;
            for c in 0..4 {
                assert!((got[i + c] - got[j + c]).abs() < 1e-6);
            }
        }
    }
}

#[test]
fn motion_symmetric_input_symmetric_output() {
    let (w, h) = (21, 13);
    let mut data = Vec::new();
    for _y in 0..h {
        for x in 0..w {
            let v = (x as f32 - 10.0).abs() / 10.0;
            data.extend_from_slice(&[v, 1.0 - v, 0.5, 1.0]);
        }
    }
    let s = rgba_surface(w, h, SampleType::F32, &data);
    let p = FilterParams::MotionBlur { angle: 0.0, distance: 7.0 };
    let out = apply(&s, &p, bounds(w, h), bounds(w, h), None);
    let got = read_region(&out, bounds(w, h));
    for y in 0..h {
        for x in 0..w {
            let xm = w - 1 - x;
            let i = (y * w + x) as usize * 4;
            let j = (y * w + xm) as usize * 4;
            for c in 0..4 {
                assert!((got[i + c] - got[j + c]).abs() < 1e-6);
            }
        }
    }
}

#[test]
fn radial_symmetric_input_symmetric_output() {
    let (w, h) = (21, 21);
    let mut data = Vec::new();
    let cx = (w - 1) as f32 / 2.0;
    let cy = (h - 1) as f32 / 2.0;
    for y in 0..h {
        for x in 0..w {
            let d = ((x as f32 - cx).powi(2) + (y as f32 - cy).powi(2)).sqrt() / 10.0;
            let v = d.min(1.0);
            data.extend_from_slice(&[v, 0.5, 1.0 - v, 1.0]);
        }
    }
    let s = rgba_surface(w, h, SampleType::F32, &data);
    let p = FilterParams::RadialBlur { amount: 30.0, method: RadialMethod::Spin, center_x: 0.5, center_y: 0.5 };
    let out = apply(&s, &p, bounds(w, h), bounds(w, h), None);
    let got = read_region(&out, bounds(w, h));
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) as usize * 4;
            // Rotational symmetry: compare pixel (x,y) with its 180° counterpart.
            let (xm, ym) = (w - 1 - x, h - 1 - y);
            let j = (ym * w + xm) as usize * 4;
            for c in 0..4 {
                assert!((got[i + c] - got[j + c]).abs() < 1e-5);
            }
        }
    }
}

#[test]
fn gaussian_tile_independence() {
    let (w, h) = (48, 32);
    let data = gradient_rgba(w, h);
    let s = rgba_surface(w, h, SampleType::F32, &data);
    let p = FilterParams::GaussianBlur { radius: 5.0 };
    let a = apply_tiled(&s, &p, bounds(w, h), bounds(w, h), None, 256, Some(bounds(w, h)));
    let b = apply_tiled(&s, &p, bounds(w, h), bounds(w, h), None, 16, Some(bounds(w, h)));
    assert_approx_eq(&read_region(&a, bounds(w, h)), &read_region(&b, bounds(w, h)), 1e-6);
}

#[test]
fn gaussian_alpha_zero_stays_zero() {
    let (w, h) = (16, 8);
    let data: Vec<f32> = (0..w * h)
        .flat_map(|i| {
            let x = (i % w) as f32 / (w - 1) as f32;
            [x, 0.5, 1.0 - x, 0.0]
        })
        .collect();
    let s = rgba_surface(w, h, SampleType::F32, &data);
    let p = FilterParams::GaussianBlur { radius: 3.0 };
    let out = apply(&s, &p, bounds(w, h), bounds(w, h), None);
    let got = read_region(&out, bounds(w, h));
    for px in got.as_chunks::<4>().0 {
        assert_approx_eq(px, &[0.0, 0.0, 0.0, 0.0], 1e-6);
    }
}

#[test]
fn gaussian_opaque_alpha_stays_one() {
    let (w, h) = (16, 8);
    let data = gradient_rgba(w, h);
    let s = rgba_surface(w, h, SampleType::F32, &data);
    let p = FilterParams::GaussianBlur { radius: 2.5 };
    let out = apply_in(&s, &p, bounds(w, h), bounds(w, h), None, bounds(w, h));
    let got = read_region(&out, bounds(w, h));
    for px in got.as_chunks::<4>().0 {
        assert!((px[3] - 1.0).abs() < 1e-6);
    }
}

#[test]
fn gaussian_radius_monotonic_difference() {
    let (w, h) = (32, 16);
    // A hard vertical edge at x = w/2.
    let data: Vec<f32> = (0..w * h)
        .flat_map(|i| {
            let x = (i % w) as f32;
            let v = if x < w as f32 / 2.0 { 0.0 } else { 1.0 };
            [v, v, v, 1.0]
        })
        .collect();
    let s = rgba_surface(w, h, SampleType::F32, &data);
    let orig = data.clone();
    let diff = |radius: f32| {
        let p = FilterParams::GaussianBlur { radius };
        let out = apply(&s, &p, bounds(w, h), bounds(w, h), None);
        let got = read_region(&out, bounds(w, h));
        got.iter().zip(&orig).map(|(a, b)| (a - b).powi(2)).sum::<f32>()
    };
    let d1 = diff(1.0);
    let d2 = diff(2.0);
    let d4 = diff(4.0);
    assert!(d1 < d2 && d2 < d4, "expected increasing differences: {d1}, {d2}, {d4}");
}

#[test]
fn gallery_empty_stack_is_identity() {
    let (w, h) = (16, 12);
    let data = gradient_rgba(w, h);
    let s = rgba_surface(w, h, SampleType::F32, &data);
    let p = FilterParams::FilterGallery { effects: vec![] };
    let out = apply(&s, &p, bounds(w, h), bounds(w, h), None);
    assert_approx_eq(&read_region(&out, bounds(w, h)), &data, 0.0);
}

#[test]
fn gallery_texturizer_relief_zero_is_identity() {
    let (w, h) = (16, 12);
    let data = gradient_rgba(w, h);
    let s = rgba_surface(w, h, SampleType::F32, &data);
    let mut effect = GalleryEffect::new(GalleryFilter::Texturizer);
    effect.set("relief", 0.0);
    let p = FilterParams::FilterGallery { effects: vec![effect] };
    let out = apply(&s, &p, bounds(w, h), bounds(w, h), None);
    assert_approx_eq(&read_region(&out, bounds(w, h)), &data, 1e-6);
}

#[test]
fn gallery_alpha_preserved() {
    let (w, h) = (16, 12);
    let mut data = gradient_rgba(w, h);
    // Vary alpha per pixel.
    for px in data.as_chunks_mut::<4>().0 {
        px[3] = 0.3 + 0.5 * ((px[0] * 10.0) as usize % 5) as f32 / 4.0;
    }
    let s = rgba_surface(w, h, SampleType::F32, &data);
    let mut effect = GalleryEffect::new(GalleryFilter::ColoredPencil);
    effect.set("pencilWidth", 5.0);
    let p = FilterParams::FilterGallery { effects: vec![effect] };
    let out = apply(&s, &p, bounds(w, h), bounds(w, h), None);
    let got = read_region(&out, bounds(w, h));
    for (i, px) in got.as_chunks::<4>().0.iter().enumerate() {
        let orig_alpha = data[i * 4 + 3];
        assert!((px[3] - orig_alpha).abs() < 1e-6, "alpha changed at {i}");
    }
}

#[test]
fn gallery_output_finite_and_in_range() {
    let (w, h) = (20, 16);
    let data = gradient_rgba(w, h);
    let s = rgba_surface(w, h, SampleType::F32, &data);
    for filter in [
        GalleryFilter::ColoredPencil,
        GalleryFilter::Cutout,
        GalleryFilter::NeonGlow,
        GalleryFilter::AccentedEdges,
        GalleryFilter::Glass,
        GalleryFilter::HalftonePattern,
    ] {
        let effect = GalleryEffect::new(filter);
        let p = FilterParams::FilterGallery { effects: vec![effect] };
        let out = apply(&s, &p, bounds(w, h), bounds(w, h), None);
        let got = read_region(&out, bounds(w, h));
        assert!(got.iter().all(|v| v.is_finite()), "{filter:?} produced non-finite");
        assert!(got.iter().all(|v| (0.0..=1.0).contains(v)), "{filter:?} out of range");
    }
}

#[test]
fn gallery_tile_independence() {
    let (w, h) = (48, 32);
    let data = gradient_rgba(w, h);
    let s = rgba_surface(w, h, SampleType::F32, &data);
    let effect = GalleryEffect::new(GalleryFilter::Cutout);
    let p = FilterParams::FilterGallery { effects: vec![effect] };
    let a = apply_tiled(&s, &p, bounds(w, h), bounds(w, h), None, 256, Some(bounds(w, h)));
    let b = apply_tiled(&s, &p, bounds(w, h), bounds(w, h), None, 16, Some(bounds(w, h)));
    assert_approx_eq(&read_region(&a, bounds(w, h)), &read_region(&b, bounds(w, h)), 1e-6);
}

#[test]
fn gallery_constant_color_stays_constant_for_zero_relief() {
    let (w, h) = (15, 15);
    let c = [0.35, 0.55, 0.75, 1.0];
    let data: Vec<f32> = (0..w * h).flat_map(|_| c).collect();
    let s = rgba_surface(w, h, SampleType::F32, &data);
    let mut effect = GalleryEffect::new(GalleryFilter::Texturizer);
    effect.set("relief", 0.0);
    let p = FilterParams::FilterGallery { effects: vec![effect] };
    let out = apply(&s, &p, bounds(w, h), bounds(w, h), None);
    let got = read_region(&out, bounds(w, h));
    for px in got.as_chunks::<4>().0 {
        assert_approx_eq(px, &c, 1e-6);
    }
}

#[test]
fn blur_small_sizes_never_panic() {
    let sizes = [(1, 1), (1, 5), (5, 1), (2, 2), (3, 3)];
    for (w, h) in sizes {
        let data = gradient_rgba(w, h);
        let s = rgba_surface(w, h, SampleType::F32, &data);
        for p in [
            FilterParams::GaussianBlur { radius: 2.0 },
            FilterParams::BoxBlur { radius: 2.0 },
            FilterParams::MotionBlur { angle: 30.0, distance: 3.0 },
            FilterParams::RadialBlur { amount: 20.0, method: RadialMethod::Spin, center_x: 0.5, center_y: 0.5 },
            FilterParams::SurfaceBlur { radius: 1.0, threshold: 20.0 },
        ] {
            let out = apply(&s, &p, bounds(w, h), bounds(w, h), None);
            let got = read_region(&out, bounds(w, h));
            assert_eq!(got.len(), (w * h * 4) as usize);
            assert!(got.iter().all(|v| v.is_finite()));
        }
    }
}

#[test]
fn gallery_small_sizes_never_panic() {
    let sizes = [(1, 1), (1, 5), (5, 1), (2, 2)];
    for (w, h) in sizes {
        let data = gradient_rgba(w, h);
        let s = rgba_surface(w, h, SampleType::F32, &data);
        let effect = GalleryEffect::new(GalleryFilter::Cutout);
        let p = FilterParams::FilterGallery { effects: vec![effect] };
        let out = apply(&s, &p, bounds(w, h), bounds(w, h), None);
        let got = read_region(&out, bounds(w, h));
        assert_eq!(got.len(), (w * h * 4) as usize);
        assert!(got.iter().all(|v| v.is_finite()));
    }
}
