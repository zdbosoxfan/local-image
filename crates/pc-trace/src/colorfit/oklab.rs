//! Oklab palette distance for image tracing.
//!
//! Ported from vtracer @ 928ed0a6f654408e28fb741b6133d4c456bd0160.
// Copyright (c) 2024 TSANG, Hao Fung, vtracer contributors.
// SPDX-License-Identifier: MIT OR Apache-2.0
// See licenses/vtracer-LICENSE-MIT and -APACHE.
//! Minimal sRGB → OKLab conversion for perceptual color distance.
//!
//! OKLab (Björn Ottosson, 2020) gives a Euclidean space where distance
//! approximates perceived color difference far better than raw RGB.

use crate::vc::Color;

/// A color in the OKLab space.
#[derive(Debug, Clone, Copy)]
pub struct Oklab {
    pub l: f64,
    pub a: f64,
    pub b: f64,
}

fn srgb_to_linear(c: u8) -> f64 {
    li_color::transfer::srgb_to_linear_f64(f64::from(c) / 255.0)
}

impl Oklab {
    pub fn from_color(color: &Color) -> Self {
        let rgb = [color.r, color.g, color.b].map(srgb_to_linear);
        let [l, a, b] = li_color::srgb_perceptual::linear_srgb_to_oklab_f64(rgb);
        Self { l, a, b }
    }

    /// Squared Euclidean distance (monotonic with distance; avoids the sqrt).
    pub fn distance_squared(&self, other: &Oklab) -> f64 {
        let dl = self.l - other.l;
        let da = self.a - other.a;
        let db = self.b - other.b;
        dl * dl + da * da + db * db
    }
}
