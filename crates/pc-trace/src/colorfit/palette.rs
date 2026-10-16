//! Document-palette snapping for image tracing.
//!
//! Ported from vtracer @ 928ed0a6f654408e28fb741b6133d4c456bd0160.
// Copyright (c) 2024 TSANG, Hao Fung, vtracer contributors.
// SPDX-License-Identifier: MIT OR Apache-2.0
// See licenses/vtracer-LICENSE-MIT and -APACHE.
use crate::vc::Color;

use crate::ir::{Paint, Segmentation};

use super::ColorFitter;
use super::oklab::Oklab;

/// Snap every layer paint to the nearest color in a fixed palette, measured in
/// OKLab. An empty palette leaves paints untouched.
#[derive(Debug, Clone, Default)]
pub struct FixedPalette {
    pub colors: Vec<Color>,
}

impl FixedPalette {
    pub fn new(colors: Vec<Color>) -> Self {
        Self { colors }
    }

    /// The palette entry closest to `color` in OKLab.
    fn nearest(&self, color: &Color, lab: &[Oklab]) -> Color {
        let target = Oklab::from_color(color);
        let mut best = self.colors[0];
        let mut best_dist = f64::INFINITY;
        for (i, entry) in self.colors.iter().enumerate() {
            let dist = target.distance_squared(&lab[i]);
            if dist < best_dist {
                best_dist = dist;
                best = *entry;
            }
        }
        best
    }
}

impl ColorFitter for FixedPalette {
    fn fit(&self, seg: &mut Segmentation) {
        if self.colors.is_empty() {
            return;
        }
        let lab: Vec<Oklab> = self.colors.iter().map(Oklab::from_color).collect();
        for layer in &mut seg.layers {
            let snapped = self.nearest(&layer.paint.color(), &lab);
            layer.paint = Paint::Solid(snapped);
        }
    }
}
