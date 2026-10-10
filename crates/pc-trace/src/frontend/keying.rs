//! Deterministic alpha/background keying for image tracing.
//!
//! Ported from vtracer @ 928ed0a6f654408e28fb741b6133d4c456bd0160.
// Copyright (c) 2024 TSANG, Hao Fung, vtracer contributors.
// SPDX-License-Identifier: MIT OR Apache-2.0
// See licenses/vtracer-LICENSE-MIT and -APACHE.
//! Transparency keying, ported from the 0.6.x `converter.rs`.
//!
//! When an image has substantial transparency, fully-transparent pixels are
//! recolored to an unused "key" color so the clustering runner can treat them
//! as a discardable background. The random key search of 0.6.x is replaced by a
//! deterministic sweep so results are reproducible and `no_std`/wasm-friendly.

use crate::vc::{Color, ColorImage};

use crate::error::Error;

/// Find a color not present in the image, to be used as the key. Tries the
/// primary/secondary colors first, then does a deterministic sweep of the RGB
/// cube. Returns [`Error::NoKeyColor`] only if all 24-bit RGB colors are used.
pub fn find_unused_color(img: &ColorImage) -> Result<Color, Error> {
    // One bit per RGB value: bounded 2 MiB, regardless of image size.
    let mut occupied = vec![0u64; (1 << 24) / 64];
    for p in img.pixels.as_chunks::<4>().0 {
        let i = (usize::from(p[0]) << 16) | (usize::from(p[1]) << 8) | usize::from(p[2]);
        occupied[i / 64] |= 1u64 << (i % 64);
    }
    let exists = |c: Color| {
        let i = (usize::from(c.r) << 16) | (usize::from(c.g) << 8) | usize::from(c.b);
        occupied[i / 64] & (1u64 << (i % 64)) != 0
    };
    let specials =
        [Color::new(255, 0, 0), Color::new(0, 255, 0), Color::new(0, 0, 255), Color::new(255, 255, 0), Color::new(0, 255, 255), Color::new(255, 0, 255)];
    for &c in specials.iter() {
        if !exists(c) {
            return Ok(c);
        }
    }

    // Deterministic sweep: step by a value coprime-ish with 256 to spread out.
    const STEP: u16 = 37;
    let mut r = 0u16;
    while r < 256 {
        let mut g = 0u16;
        while g < 256 {
            let mut b = 0u16;
            while b < 256 {
                let c = Color::new(r as u8, g as u8, b as u8);
                if !exists(c) {
                    return Ok(c);
                }
                b += STEP;
            }
            g += STEP;
        }
        r += STEP;
    }

    for (word, &bits) in occupied.iter().enumerate() {
        if bits != u64::MAX {
            let i = word * 64 + (!bits).trailing_zeros() as usize;
            return Ok(Color::new((i >> 16) as u8, (i >> 8) as u8, i as u8));
        }
    }
    Err(Error::NoKeyColor)
}

/// Recolor every fully-transparent pixel to `key`, in place.
pub fn apply_key(img: &mut ColorImage, key: Color) {
    for y in 0..img.height {
        for x in 0..img.width {
            if img.get_pixel(x, y).a == 0 {
                img.set_pixel(x, y, &key);
            }
        }
    }
}

#[cfg(test)]
mod ds_tests {
    use super::*;
    #[test]
    fn ds_finding_key_search_covers_more_than_initial_candidates() {
        let mut colors = vec![[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255], [255, 255, 0, 255], [0, 255, 255, 255], [255, 0, 255, 255]];
        for r in (0..256).step_by(37) {
            for g in (0..256).step_by(37) {
                for b in (0..256).step_by(37) {
                    colors.push([r as u8, g as u8, b as u8, 255]);
                }
            }
        }
        let img = ColorImage { width: colors.len(), height: 1, pixels: colors.iter().flatten().copied().collect() };
        let key = find_unused_color(&img).unwrap();
        assert!(!colors.iter().any(|c| c[..3] == [key.r, key.g, key.b]));
        assert_eq!([key.r, key.g, key.b], [0, 0, 1]);
    }
}
