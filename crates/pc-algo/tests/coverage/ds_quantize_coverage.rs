//! Integration tests for the public `quantize` API of `photocraft-algo`.
//!
//! These tests pin real behaviour: palette construction, dithering,
//! bitmap conversion, boundary values, edge cases, determinism and
//! error paths, using only synthetic in-memory data.

use photocraft_algo::quantize::{
    BitmapMethod, Dither, Forced, HalftoneShape, PaletteKind, bayer8, build_palette, mac_palette, quantize, to_bitmap, uniform_palette, web_palette,
    windows_palette,
};

/// Small helper: an `w×h` gradient with varying red, green, fixed blue and opaque alpha.
fn gradient(w: usize, h: usize) -> Vec<[f32; 4]> {
    (0..w * h).map(|i| [(i % w) as f32 / (w - 1).max(1) as f32, (i / w) as f32 / (h - 1).max(1) as f32, 0.3, 1.0]).collect()
}

#[test]
fn fixed_palettes_have_expected_sizes_and_unique_entries() {
    assert_eq!(web_palette().len(), 216);
    assert_eq!(mac_palette().len(), 256);
    assert_eq!(windows_palette().len(), 256);
    assert_eq!(uniform_palette(27).len(), 27);

    let mut mac = mac_palette();
    mac.sort_unstable();
    mac.dedup();
    assert_eq!(mac.len(), 256, "mac palette entries must be unique");
}

#[test]
fn uniform_palette_uses_cube_root_grid() {
    // 2³ = 8 colours: only 0 and 255 per channel.
    let pal = uniform_palette(8);
    assert_eq!(pal.len(), 8);
    assert!(pal.iter().all(|&c| c.iter().all(|&v| v == 0 || v == 255)));

    // 4³ = 64 colours: values 0, 85, 170, 255.
    let pal = uniform_palette(64);
    assert_eq!(pal.len(), 64);
    let allowed = [0u8, 85, 170, 255];
    assert!(pal.iter().all(|&c| c.iter().all(|v| allowed.contains(v))));
}

#[test]
fn exact_palette_succeeds_for_few_colors_and_fails_above_256() {
    let px = vec![[1.0, 0.0, 0.0, 1.0], [0.0, 0.0, 1.0, 1.0], [1.0, 0.0, 0.0, 1.0]];
    let pal = build_palette(&px, PaletteKind::Exact, 256, Forced::None).unwrap();
    assert_eq!(pal.len(), 2);
    assert!(pal.contains(&[255, 0, 0]));
    assert!(pal.contains(&[0, 0, 255]));

    // More than 256 unique colours must return Err, not panic.
    let large = gradient(64, 64);
    assert!(build_palette(&large, PaletteKind::Exact, 256, Forced::None).is_err());
}

#[test]
fn build_palette_includes_forced_colors_and_respects_count() {
    let px = gradient(40, 40);
    let pal = build_palette(&px, PaletteKind::Adaptive, 16, Forced::BlackWhite).unwrap();
    assert!(pal.len() <= 16, "palette length {} exceeds request", pal.len());
    assert!(pal.len() >= 8, "palette length {} too small", pal.len());
    assert_eq!(&pal[..2], &[[0, 0, 0], [255, 255, 255]]);

    let pal = build_palette(&px, PaletteKind::Perceptual, 8, Forced::Primaries).unwrap();
    assert_eq!(pal.len(), 8);
    assert_eq!(&pal[..8], &[[0, 0, 0], [255, 255, 255], [255, 0, 0], [0, 255, 0], [0, 0, 255], [0, 255, 255], [255, 0, 255], [255, 255, 0]]);
}

#[test]
fn build_palette_handles_empty_input_and_tiny_sizes() {
    // Empty input: no histogram entries, but a palette must be returned.
    let pal = build_palette(&[], PaletteKind::Adaptive, 256, Forced::None).unwrap();
    assert!(!pal.is_empty(), "empty input should produce a fallback palette");

    // Single pixel.
    let px = vec![[0.5, 0.5, 0.5, 1.0]];
    let pal = build_palette(&px, PaletteKind::Exact, 256, Forced::None).unwrap();
    assert_eq!(pal.len(), 1);
    assert_eq!(pal[0], [128, 128, 128]); // (0.5*255).round() = 128
}

#[test]
fn build_palette_all_computed_kinds_return_finite_rgb() {
    let px = gradient(50, 50);
    for kind in [PaletteKind::Adaptive, PaletteKind::Perceptual, PaletteKind::Selective, PaletteKind::Uniform] {
        let pal = build_palette(&px, kind, 32, Forced::None).unwrap();
        assert!(pal.len() <= 32, "{kind:?} pal len {}", pal.len());
        assert!(pal.len() >= 2, "{kind:?} pal len {}", pal.len());
    }
}

#[test]
fn quantize_maps_pixels_to_palette_and_returns_valid_indices() {
    let pal: Vec<[u8; 3]> = vec![[0, 0, 0], [255, 255, 255], [255, 0, 0]];
    let px: Vec<[f32; 4]> = (0..100)
        .map(|i| {
            let v = i as f32 / 99.0;
            [v, v, v, 1.0]
        })
        .collect();

    for dither in [Dither::None, Dither::Diffusion, Dither::Pattern, Dither::Noise] {
        let mut input = px.clone();
        let idx = quantize(&mut input, 10, &pal, dither, 1.0, None);
        assert_eq!(idx.len(), 100);
        assert!(idx.iter().all(|&i| (i as usize) < pal.len()));
        for (p, &i) in input.iter().zip(idx.iter()) {
            let expected = pal[i as usize].map(|v| f32::from(v) / 255.0);
            // Opaque pixels become exactly the palette colour.
            assert_eq!(p[0], expected[0], "dither {dither:?}");
            assert_eq!(p[1], expected[1], "dither {dither:?}");
            assert_eq!(p[2], expected[2], "dither {dither:?}");
            assert_eq!(p[3], 1.0, "dither {dither:?}");
        }
    }
}

#[test]
fn quantize_transparent_index_sets_alpha_zero() {
    let pal = vec![[0, 0, 0], [255, 255, 255]];
    let mut px = vec![[1.0, 1.0, 1.0, 0.2], [0.2, 0.2, 0.2, 0.9]];
    let idx = quantize(&mut px, 2, &pal, Dither::None, 1.0, Some(1));
    assert_eq!(idx[0], 1);
    assert_eq!(px[0][3], 0.0);
    assert_eq!(px[0][0], 0.0);
    // Opaque pixels become fully opaque when a transparent index is used.
    assert_eq!(idx[1], 0); // dark -> black
    assert_eq!(px[1][3], 1.0);
}

#[test]
fn quantize_handles_empty_and_zero_width() {
    let pal = vec![[0, 0, 0]];
    let mut empty: Vec<[f32; 4]> = Vec::new();
    let idx = quantize(&mut empty, 0, &pal, Dither::None, 1.0, None);
    assert!(idx.is_empty());

    let mut empty2: Vec<[f32; 4]> = Vec::new();
    let idx = quantize(&mut empty2, 5, &pal, Dither::Diffusion, 1.0, None);
    assert!(idx.is_empty());

    // Non-empty input but zero width: no rows processed, but no panic.
    let mut px = vec![[0.5, 0.5, 0.5, 1.0]; 4];
    let idx = quantize(&mut px, 0, &pal, Dither::Diffusion, 1.0, None);
    assert_eq!(idx.len(), 4);
}

#[test]
fn quantize_handles_nan_and_inf_values() {
    let pal = vec![[0, 0, 0], [255, 255, 255]];
    let px = vec![[f32::NAN, 0.5, 0.5, 1.0], [0.5, f32::INFINITY, 0.5, 1.0], [0.5, 0.5, f32::NEG_INFINITY, 1.0]];
    for dither in [Dither::None, Dither::Diffusion, Dither::Pattern, Dither::Noise] {
        let mut input = px.clone();
        let idx = quantize(&mut input, 3, &pal, dither, 1.0, None);
        assert_eq!(idx.len(), 3);
        assert!(idx.iter().all(|&i| (i as usize) < pal.len()));
        assert!(input.iter().all(|p| p.iter().all(|v| v.is_finite())));
    }
}

#[test]
fn quantize_is_deterministic() {
    let pal = vec![[0, 0, 0], [255, 255, 255], [255, 0, 0]];
    let mut px1: Vec<[f32; 4]> = gradient(32, 8);
    let mut px2 = px1.clone();
    let idx1 = quantize(&mut px1, 32, &pal, Dither::Diffusion, 1.0, None);
    let idx2 = quantize(&mut px2, 32, &pal, Dither::Diffusion, 1.0, None);
    assert_eq!(idx1, idx2);
    assert_eq!(px1, px2);
}

#[test]
fn bayer8_is_periodic_and_in_range() {
    for y in 0..16 {
        for x in 0..16 {
            let v = bayer8(x, y);
            assert!(v > -0.5 && v < 0.5);
            assert_eq!(v, bayer8(x + 8, y));
            assert_eq!(v, bayer8(x, y + 8));
        }
    }
}

#[test]
fn to_bitmap_threshold_is_strict_binary() {
    let mut gray = vec![0.0, 0.499, 0.5, 0.9, 1.0];
    to_bitmap(&mut gray, 5, BitmapMethod::Threshold);
    assert_eq!(gray, vec![0.0, 0.0, 1.0, 1.0, 1.0]);
}

#[test]
fn to_bitmap_pattern_and_diffusion_preserve_mean_tone() {
    let mean_input = 0.25f32;
    let size = 64;
    let mut gray = vec![mean_input; size * size];
    to_bitmap(&mut gray, size, BitmapMethod::Pattern);
    assert!(gray.iter().all(|&v| v == 0.0 || v == 1.0));
    let mean = gray.iter().sum::<f32>() / gray.len() as f32;
    assert!((mean - mean_input).abs() < 0.15, "pattern mean {mean}");

    let mut gray = vec![mean_input; size * size];
    to_bitmap(&mut gray, size, BitmapMethod::Diffusion);
    assert!(gray.iter().all(|&v| v == 0.0 || v == 1.0));
    let mean = gray.iter().sum::<f32>() / gray.len() as f32;
    assert!((mean - mean_input).abs() < 0.15, "diffusion mean {mean}");
}

#[test]
fn to_bitmap_halftone_is_binary_and_preserves_tone() {
    let mean_input = 0.6f32;
    let size = 64;
    let mut gray = vec![mean_input; size * size];
    to_bitmap(&mut gray, size, BitmapMethod::Halftone { cell: 8.0, angle: 45.0, shape: HalftoneShape::Round });
    assert!(gray.iter().all(|&v| v == 0.0 || v == 1.0));
    let mean = gray.iter().sum::<f32>() / gray.len() as f32;
    assert!((mean - mean_input).abs() < 0.2, "halftone mean {mean}");
}

#[test]
fn to_bitmap_handles_empty_and_zero_width() {
    let mut empty: Vec<f32> = Vec::new();
    to_bitmap(&mut empty, 0, BitmapMethod::Threshold);
    assert!(empty.is_empty());

    let mut nonempty = vec![0.5f32; 4];
    to_bitmap(&mut nonempty, 0, BitmapMethod::Threshold);
    assert_eq!(nonempty.len(), 4);
}

#[test]
fn to_bitmap_handles_nan_and_inf() {
    let mut gray = vec![f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 0.5];
    to_bitmap(&mut gray, 4, BitmapMethod::Threshold);
    assert!(gray.iter().all(|&v| v == 0.0 || v == 1.0));

    let mut gray = vec![f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 0.5];
    to_bitmap(&mut gray, 4, BitmapMethod::Pattern);
    assert!(gray.iter().all(|&v| v == 0.0 || v == 1.0));

    let mut gray = vec![f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 0.5];
    to_bitmap(&mut gray, 4, BitmapMethod::Diffusion);
    assert!(gray.iter().all(|&v| v == 0.0 || v == 1.0));
}

#[test]
fn palette_kind_from_id_round_trips() {
    let cases = [
        ("exact", PaletteKind::Exact),
        ("systemMac", PaletteKind::SystemMac),
        ("systemWindows", PaletteKind::SystemWindows),
        ("web", PaletteKind::Web),
        ("uniform", PaletteKind::Uniform),
        ("perceptual", PaletteKind::Perceptual),
        ("selective", PaletteKind::Selective),
        ("adaptive", PaletteKind::Adaptive),
    ];
    for (id, kind) in cases {
        assert_eq!(PaletteKind::from_id(id), Some(kind));
        assert_eq!(PaletteKind::from_id("bogus"), None);
    }
}

#[test]
fn halftone_shape_from_id_round_trips() {
    assert_eq!(HalftoneShape::from_id("round"), HalftoneShape::Round);
    assert_eq!(HalftoneShape::from_id("ellipse"), HalftoneShape::Ellipse);
    assert_eq!(HalftoneShape::from_id("line"), HalftoneShape::Line);
    assert_eq!(HalftoneShape::from_id("square"), HalftoneShape::Square);
    assert_eq!(HalftoneShape::from_id("diamond"), HalftoneShape::Diamond);
    assert_eq!(HalftoneShape::from_id("cross"), HalftoneShape::Cross);
    // Unknown falls back to Round.
    assert_eq!(HalftoneShape::from_id("bogus"), HalftoneShape::Round);
}
