//! Editor checks of the shared `li_raster` histogram: float bins describe the supplied sample
//! domain; no colour conversion is performed.
use li_raster::histogram::{BINS, RgbHistogram, clipping};

#[test]
fn exact_bins_and_channel_totals() {
    let pixels: Vec<_> = (0..BINS).map(|i| [i as f32 / 255.0, (255 - i) as f32 / 255.0, 0.5, 0.25]).collect();
    let h = RgbHistogram::from_rgba(&pixels);
    assert_eq!(h.samples, 256);
    assert!(h.channels[0].iter().all(|&v| v == 1));
    assert!(h.channels[1].iter().all(|&v| v == 1));
    assert_eq!(h.channels[2][128], 256);
    for channel in h.channels {
        assert_eq!(channel.iter().sum::<u64>(), h.samples);
    }
}

#[test]
fn empty_transparent_invalid_and_hdr_are_explicit() {
    assert_eq!(RgbHistogram::from_rgba(&[]), RgbHistogram::default());
    let pixels = [[0.7, 0.9, 0.4, 0.0], [f32::NAN, 0.0, 0.0, 1.0], [0.0, f32::INFINITY, 0.0, 1.0], [0.0, 0.0, 0.0, f32::NAN], [-2.0, 4.0, 0.5, 1.0]];
    let original = pixels;
    let h = RgbHistogram::from_rgba(&pixels);
    assert_eq!((h.samples, h.transparent, h.invalid), (1, 1, 3));
    assert_eq!(h.underflow, [1, 0, 0]);
    assert_eq!(h.overflow, [0, 1, 0]);
    assert_eq!((h.channels[0][0], h.channels[1][255], h.channels[2][128]), (1, 1, 1));
    assert_eq!(pixels[4], original[4]);
    assert!(pixels[1][0].is_nan());
}

#[test]
fn clipping_indicators_use_exact_samples_not_edge_bin_rounding() {
    let h = RgbHistogram::from_rgba(&[[0.0001, 0.9999, 0.5, 1.0], [-0.2, 1.0, 0.0, 1.0], [0.0, 1.0, 0.0, 0.0]]);
    assert_eq!(h.shadows, [1, 0, 1]);
    assert_eq!(h.highlights, [0, 1, 0]);
    assert_eq!(h.channels[0][0], 2);
    assert_eq!(h.channels[1][255], 2);
}

#[test]
fn rgb_and_gray_at_all_depths_use_the_same_float_contract() {
    use photocraft_color::{ColorMode, PixelFormat, SampleType};
    use photocraft_geom::Rect;
    use photocraft_raster::Surface;
    for mode in [ColorMode::Rgb, ColorMode::Grayscale] {
        for sample in SampleType::ALL {
            let mut surface = Surface::new(PixelFormat::new(mode, sample, true));
            for (x, v) in [0.0, 0.25, 0.75, 1.0].into_iter().enumerate() {
                let px = if mode == ColorMode::Rgb { vec![v, v, v, 1.0] } else { vec![v, 1.0] };
                surface.write_pixel(x as i32, 0, &px);
            }
            let mut pixels = [[0.0; 4]; 4];
            surface.read_rgba_into(Rect::new(0, 0, 4, 1), &mut pixels);
            let h = RgbHistogram::from_rgba(&pixels);
            assert_eq!(h.samples, 4, "{mode:?}/{sample:?}");
            for c in &h.channels {
                for bin in [0, 64, 191, 255] {
                    assert_eq!(c[bin], 1, "{mode:?}/{sample:?}, bin {bin}");
                }
            }
        }
    }
}

#[test]
fn exact_clipping_endpoints_not_quantized_neighbours() {
    assert_eq!(clipping([0.0001, 0.9999, 0.5, 1.0]), (0, 0));
    assert_eq!(clipping([-0.1, 1.0, 0.0, 1.0]), (5, 2));
    assert_eq!(clipping([0.0, 1.0, 0.0, 0.0]), (0, 0));
    assert_eq!(clipping([0.0, f32::INFINITY, 0.0, 1.0]), (0, 0));
}

#[test]
fn exposure_of_an_eight_bit_gradient_reproduces_the_quantization_comb() {
    let mut pixels: Vec<_> = (0..256).map(|i| [i as f32 / 255.0, i as f32 / 255.0, i as f32 / 255.0, 1.0]).collect();
    assert!(RgbHistogram::from_rgba(&pixels).channels[0].iter().all(|&v| v == 1));
    // A known half-stop exposure of a quantized gradient, independent of any develop engine.
    for pixel in &mut pixels {
        for c in &mut pixel[..3] {
            *c = crate::photo_util::linear_to_srgb(crate::photo_util::srgb_to_linear(*c) * 2f32.sqrt());
        }
    }
    let h = RgbHistogram::from_rgba(&pixels);
    assert!(h.channels[0][10..230].iter().filter(|&&v| v == 0).count() > 10);
    assert_eq!(h.samples, 256);
    assert!(h.overflow[0] > 0);
}

#[test]
fn j01_shared_histogram_pc_adapter_luma_goldens() {
    let pixels = [[1.0, 0.0, 0.0, 1.0], [0.0, 1.0, 0.0, 1.0], [0.0, 0.0, 1.0, 1.0]];
    let h = RgbHistogram::from_rgba(&pixels);
    assert_eq!(h.samples, 3);
    assert_eq!(h.shadows, [2; 3]);
    assert_eq!(h.highlights, [1; 3]);
    let mut expected = [0; BINS];
    for bin in [54, 182, 18] {
        expected[bin] = 1;
    }
    assert_eq!(h.luma, expected);
    assert_eq!(h.channels[0][0], 2);
    assert_eq!(h.channels[0][255], 1);
}
