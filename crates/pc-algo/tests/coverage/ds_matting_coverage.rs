use photocraft_algo::matting::{
    EPS, RefineParams, box_mean, decontaminate, edge_width, gaussian_blur, guided_filter_color, guided_filter_gray, masked_copy, morph, refine_buffer,
    refine_mask, region_reader, region_surface, surface_reader,
};
use photocraft_algo::segment::{ImageSampler, RgbImage};
use photocraft_algo::selection::Region;
use photocraft_color::{PixelFormat, SampleType};
use photocraft_geom::Rect;
use photocraft_raster::Surface;

fn blurred_edge(w: usize, h: usize, sigma: f32) -> (RgbImage, Vec<f32>) {
    let (a, b) = ([0.9f32, 0.8, 0.2], [0.1f32, 0.2, 0.6]);
    let erf = |x: f32| {
        let t = 1.0 / (1.0 + 0.327_591_1 * x.abs());
        let y = 1.0 - (((((1.061_405_4 * t - 1.453_152_1) * t) + 1.421_413_7) * t - 0.284_496_74) * t + 0.254_829_6) * t * (-x * x).exp();
        if x >= 0.0 { y } else { -y }
    };
    let alpha: Vec<f32> = (0..w).map(|x| 0.5 * (1.0 - erf((x as f32 + 0.5 - 40.0) / (sigma * std::f32::consts::SQRT_2)))).collect();
    let img = RgbImage::from_fn(w, h, |x, _| [0, 1, 2].map(|c| a[c] * alpha[x] + b[c] * (1.0 - alpha[x])));
    (img, alpha)
}

#[test]
fn default_refine_params_are_noop() {
    let p = RefineParams::default();
    assert_eq!(p.radius, 0.0);
    assert!(!p.smart_radius);
    assert_eq!(p.smooth, 0.0);
    assert_eq!(p.feather, 0.0);
    assert_eq!(p.contrast, 0.0);
    assert_eq!(p.shift_edge, 0.0);
    assert_eq!(p.halo(), 2);
    assert_eq!(p.spread(), 1);

    let m = vec![0.0f32, 0.25, 0.5, 0.75, 1.0];
    let out = refine_buffer(None, &m, 5, 1, &p);
    assert_eq!(out, m);
}

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
fn box_mean_matches_naive_clipping() {
    let (w, h) = (13usize, 9usize);
    let src: Vec<f32> = (0..w * h).map(|i| ((i * 7) % 11) as f32).collect();
    let fast = box_mean(&src, w, h, 2);
    for y in 0..h {
        for x in 0..w {
            let (mut s, mut n) = (0.0, 0.0);
            for yy in y.saturating_sub(2)..(y + 3).min(h) {
                for xx in x.saturating_sub(2)..(x + 3).min(w) {
                    s += src[yy * w + xx];
                    n += 1.0;
                }
            }
            assert!((fast[y * w + x] - s / n).abs() < 1e-4, "({x},{y})");
        }
    }
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
fn refine_buffer_feather_contrast_shift() {
    let (w, h) = (40usize, 5usize);
    let m: Vec<f32> = (0..w * h).map(|i| if (i % w) < 20 { 1.0 } else { 0.0 }).collect();

    let f = refine_buffer(None, &m, w, h, &RefineParams { feather: 4.0, ..Default::default() });
    let row = &f[2 * w..3 * w];
    assert!(row[19] > 0.5 && row[19] < 0.85);
    assert!(row[21] > 0.05 && row[21] < 0.5);

    let c = refine_buffer(None, &f, w, h, &RefineParams { contrast: 100.0, ..Default::default() });
    assert_eq!(c[2 * w + 19], 1.0);
    assert_eq!(c[2 * w + 21], 0.0);

    let grown = refine_buffer(None, &m, w, h, &RefineParams { shift_edge: 100.0, ..Default::default() });
    assert_eq!(grown[2 * w + 23], 1.0);
    assert_eq!(grown[2 * w + 24], 0.0);

    let shrunk = refine_buffer(None, &m, w, h, &RefineParams { shift_edge: -50.0, ..Default::default() });
    assert_eq!(shrunk[2 * w + 17], 1.0);
    assert_eq!(shrunk[2 * w + 18], 0.0);

    let sm = refine_buffer(None, &m, w, h, &RefineParams { smooth: 50.0, ..Default::default() });
    assert_eq!(sm[2 * w + 5], 1.0);
    assert_eq!(sm[2 * w + 35], 0.0);
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
fn refine_mask_tiles_match_single_buffer() {
    let (w, h) = (160usize, 80usize);
    let (img, _) = blurred_edge(w, h, 2.0);
    let sampler = ImageSampler { img: &img, origin: (0, 0) };

    let mut mask = vec![0u8; w * h];
    for y in 0..h {
        for x in 0..w {
            let (dx, dy) = (x as f32 - 30.0, y as f32 - 40.0);
            mask[y * w + x] = if dx * dx * 0.05 + dy * dy < 30.0 * 30.0 || x < 30 { 255 } else { 0 };
        }
    }
    let reg = Region { bbox: Rect::new(0, 0, w as i32, h as i32), mask };
    let p = RefineParams { radius: 5.0, feather: 1.0, contrast: 5.0, ..Default::default() };
    let out = refine_mask(&sampler, &region_reader(&reg), reg.bbox, reg.bbox, &p).unwrap();

    let whole: Vec<f32> = reg.mask.iter().map(|v| *v as f32 / 255.0).collect();
    let full = refine_buffer(Some(&img), &whole, w, h, &p);

    let mut maxd = 0.0f32;
    for y in 0..h {
        for x in 0..w {
            maxd = maxd.max((out.at(x as i32, y as i32) - full[y * w + x]).abs());
        }
    }
    assert!(maxd <= 1.5 / 255.0, "max diff {maxd}");
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

#[test]
fn masked_copy_gray_adds_alpha_and_multiplies() {
    let mut g = Surface::new(PixelFormat::GRAY8);
    g.fill_rect(Rect::new(0, 0, 4, 4), &[0.5]);

    let mask = vec![255, 0, 128, 255, 255, 255, 0, 0, 0, 255, 255, 0, 255, 255, 255, 255];
    let a = Region { bbox: Rect::new(0, 0, 4, 4), mask };
    let c = masked_copy(&g, &a);

    assert_eq!(c.format(), PixelFormat::GRAYA8);

    let p = c.pixel(1, 1);
    assert!((p[0] - 0.5).abs() < 0.01 && p[1] == 1.0);

    let p2 = c.pixel(2, 0);
    assert!((p2[1] - 128.0 / 255.0).abs() < 0.01);

    assert_eq!(c.pixel(3, 1)[1], 0.0);
}

#[test]
fn decontaminate_u8_and_f32() {
    for sample in [SampleType::U8, SampleType::F32] {
        let fmt = PixelFormat::RGBA8.with_sample(sample);
        let mut s = Surface::new(fmt);
        s.fill_rect(Rect::new(0, 0, 6, 4), &[1.0, 0.0, 0.0, 1.0]);
        s.fill_rect(Rect::new(6, 0, 12, 4), &[0.0, 0.0, 1.0, 1.0]);
        s.fill_rect(Rect::new(5, 0, 7, 4), &[0.5, 0.0, 0.5, 1.0]);

        let mask: Vec<u8> = (0..12 * 4)
            .map(|i| {
                let x = i % 12;
                if x < 5 {
                    255
                } else if x == 5 {
                    128
                } else {
                    0
                }
            })
            .collect();
        let a = Region { bbox: Rect::new(0, 0, 12, 4), mask };
        let d = decontaminate(&s, &a, 3.0, 100.0);

        let p = d.pixel(5, 2);
        assert!(p[0] > 0.85 && p[2] < 0.15, "{:?}: {:?}", sample, p);
        assert_eq!(d.pixel(9, 2), s.pixel(9, 2));
    }
}
