use li_matte::matting::{EPS, RefineParams, box_mean, gaussian_blur, guided_filter_gray, morph};
use photocraft_algo::matting::{decontaminate, edge_width, guided_filter_color, refine_mask, region_surface, surface_reader};
use photocraft_algo::segment::{ImageSampler, RgbImage};
use photocraft_algo::selection::Region;
use photocraft_color::PixelFormat;
use photocraft_geom::Rect;
use photocraft_raster::Surface;

#[test]
fn refine_params_halo_and_spread_reflect_parameters() {
    let p = RefineParams { radius: 10.0, smart_radius: true, smooth: 50.0, feather: 6.0, contrast: 10.0, shift_edge: 20.0 };
    assert_eq!(p.halo(), 50);
    assert_eq!(p.spread(), 28);
}

#[test]
fn box_mean_constant_and_empty() {
    assert!(box_mean(&[], 0, 0, 3).is_empty());

    let src = vec![0.5f32; 5 * 4];
    let out = box_mean(&src, 5, 4, 2);
    assert!(out.iter().all(|v| (*v - 0.5).abs() < 1e-6));
}

#[test]
fn guided_filter_gray_constant_input_gives_constant_output() {
    let (w, h) = (7usize, 5usize);
    let guide: Vec<f32> = (0..w * h).map(|i| (i % 11) as f32 / 10.0).collect();
    let p = vec![0.3f32; w * h];
    let out = guided_filter_gray(&guide, &p, w, h, 2, EPS);
    assert!(out.iter().all(|v| (*v - 0.3).abs() < 1e-5));
}

#[test]
fn guided_filter_gray_empty_returns_empty() {
    assert!(guided_filter_gray(&[], &[], 0, 0, 1, EPS).is_empty());
}

#[test]
fn guided_filter_color_constant_input_gives_constant_output() {
    let (w, h) = (6usize, 4usize);
    let img = RgbImage::from_fn(w, h, |x, y| [x as f32 / w as f32, y as f32 / h as f32, 0.5]);
    let p = vec![0.7f32; w * h];
    let out = guided_filter_color(&img, &p, 2, EPS);
    assert!(out.iter().all(|v| (*v - 0.7).abs() < 1e-5));
}

#[test]
fn guided_filter_color_empty_returns_empty() {
    let img = RgbImage::from_fn(0, 0, |_, _| [0.0; 3]);
    assert!(guided_filter_color(&img, &[], 1, EPS).is_empty());
}

#[test]
fn gaussian_blur_small_sigma_is_copy_and_large_is_finite() {
    let (w, h) = (5usize, 4usize);
    let src: Vec<f32> = (0..w * h).map(|i| i as f32 / 20.0).collect();
    let small = gaussian_blur(&src, w, h, 0.05);
    assert_eq!(small, src);

    let large = gaussian_blur(&src, w, h, 2.0);
    assert_eq!(large.len(), src.len());
    assert!(large.iter().all(|v| v.is_finite()));
    let src_mean = src.iter().sum::<f32>() / src.len() as f32;
    let out_mean = large.iter().sum::<f32>() / large.len() as f32;
    assert!((src_mean - out_mean).abs() < 0.2);
}

#[test]
fn morph_dilation_and_erosion_3x3() {
    let (w, h) = (5usize, 5usize);
    let mut src = vec![0.0f32; w * h];
    src[2 * w + 2] = 1.0;
    let dil = morph(&src, w, h, 1, true);
    for y in 1..=3 {
        for x in 1..=3 {
            assert_eq!(dil[y * w + x], 1.0, "dilation ({}, {})", x, y);
        }
    }
    assert_eq!(dil[0], 0.0);

    let mut all_ones = vec![1.0f32; w * h];
    all_ones[2 * w + 2] = 0.0;
    let ero = morph(&all_ones, w, h, 1, false);
    for y in 1..=3 {
        for x in 1..=3 {
            assert_eq!(ero[y * w + x], 0.0, "erosion ({}, {})", x, y);
        }
    }
    assert_eq!(ero[0], 1.0);
}

#[test]
fn edge_width_constant_image_is_zero() {
    let (w, h) = (10usize, 6usize);
    let img = RgbImage::from_fn(w, h, |_, _| [0.5, 0.5, 0.5]);
    let widths = edge_width(&img, 2);
    assert!(widths.iter().all(|v| *v < 1e-6));
}

#[test]
fn refine_mask_empty_content_returns_none() {
    let img = RgbImage::from_fn(0, 0, |_, _| [0.0; 3]);
    let sampler = ImageSampler { img: &img, origin: (0, 0) };
    let mask = |_r: Rect| Vec::new();
    let canvas = Rect::new(0, 0, 10, 10);
    assert!(refine_mask(&sampler, &mask, Rect::EMPTY, canvas, &RefineParams::default()).is_none());
}

#[test]
fn region_surface_reader_roundtrip() {
    let (w, h) = (10usize, 5usize);
    let mask: Vec<u8> = (0..w * h).map(|i| (i % 256) as u8).collect();
    let reg = Region { bbox: Rect::new(0, 0, w as i32, h as i32), mask };
    let surface = region_surface(&reg);
    let reader = surface_reader(&surface);
    let read = reader(reg.bbox);
    for i in 0..(w * h) {
        let expected = reg.mask[i] as f32 / 255.0;
        assert!((read[i] - expected).abs() < 0.5 / 255.0, "pixel {i}");
    }
}

#[test]
fn decontaminate_amount_zero_is_identity() {
    let fmt = PixelFormat::RGBA8;
    let mut s = Surface::new(fmt);
    s.fill_rect(Rect::new(0, 0, 8, 8), &[0.2, 0.3, 0.4, 1.0]);
    s.fill_rect(Rect::new(2, 0, 6, 8), &[0.8, 0.7, 0.6, 1.0]);
    let mask: Vec<u8> = (0..8 * 8).map(|i| if (i % 8) < 4 { 200 } else { 50 }).collect();
    let alpha = Region { bbox: Rect::new(0, 0, 8, 8), mask };
    let d = decontaminate(&s, &alpha, 5.0, 0.0);
    for y in 0..8 {
        for x in 0..8 {
            assert_eq!(d.pixel(x, y), s.pixel(x, y), "pixel ({x},{y})");
        }
    }
}
