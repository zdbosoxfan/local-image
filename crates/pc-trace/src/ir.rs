//! Stacked colour regions for image tracing.
//!
//! Ported from vtracer @ 928ed0a6f654408e28fb741b6133d4c456bd0160.
// Copyright (c) 2024 TSANG, Hao Fung, vtracer contributors.
// SPDX-License-Identifier: MIT OR Apache-2.0
// See licenses/vtracer-LICENSE-MIT and -APACHE.
use crate::vc::{BinaryImage, PointI32};

/// A region's pixel coverage: a local binary mask positioned on the canvas.
///
/// Foreground pixels are `true`. Holes (interior background) are already
/// punched out of the mask, so a mask is self-describing for tracing.
#[derive(Debug, Clone)]
pub struct RegionMask {
    /// Local coverage; `true` = inside the region.
    pub image: BinaryImage,
    /// Position of the mask's top-left corner in full-canvas coordinates.
    pub offset: PointI32,
}

impl RegionMask {
    pub fn new(image: BinaryImage, offset: PointI32) -> Self {
        Self { image, offset }
    }

    pub fn width(&self) -> usize {
        self.image.width
    }

    pub fn height(&self) -> usize {
        self.image.height
    }

    /// Number of foreground pixels.
    pub fn area(&self) -> usize {
        let mut count = 0;
        for y in 0..self.image.height {
            for x in 0..self.image.width {
                if self.image.get_pixel(x, y) {
                    count += 1;
                }
            }
        }
        count
    }
}

/// A single paint layer. Layers are painted bottom-to-top.
#[derive(Debug, Clone)]
pub struct Layer {
    /// Fill applied to the region. Starts as the cluster's mean color; a
    /// [`crate::colorfit::ColorFitter`] may rewrite it.
    pub paint: Paint,
    /// Pixel coverage of the region.
    pub mask: RegionMask,
}

/// Frontend output: ordered layers over a canvas, in paint order.
#[derive(Debug, Clone)]
pub struct Segmentation {
    pub width: u32,
    pub height: u32,
    /// Bottom-to-top paint order.
    pub layers: Vec<Layer>,
}

impl Segmentation {
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height, layers: Vec::new() }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Paint {
    Solid(crate::vc::Color),
}
impl Paint {
    pub fn color(&self) -> crate::vc::Color {
        let Self::Solid(c) = self;
        *c
    }
}

/// Fold repeated paints into one compound per colour without changing occlusion.
/// A colour's last occurrence defines its order. Earlier masks retain overdraw
/// only where the final owner will be painted above it; otherwise they are trimmed.
pub fn stack_colors(seg: Segmentation) -> Segmentation {
    use std::collections::BTreeMap;
    let (w, h) = (seg.width as usize, seg.height as usize);
    let mut last = BTreeMap::new();
    for (i, l) in seg.layers.iter().enumerate() {
        let c = l.paint.color();
        last.insert([c.r, c.g, c.b], i);
    }
    let mut ordered: Vec<_> = last.into_iter().collect();
    ordered.sort_by_key(|(_, i)| *i);
    let ids: BTreeMap<_, _> = ordered.iter().enumerate().map(|(i, (c, _))| (*c, i)).collect();
    let mut owner = vec![usize::MAX; w * h];
    for l in &seg.layers {
        let c = l.paint.color();
        let id = ids[&[c.r, c.g, c.b]];
        for y in 0..l.mask.height() {
            for x in 0..l.mask.width() {
                if l.mask.image.get_pixel(x, y) {
                    let i = (y + l.mask.offset.y as usize) * w + x + l.mask.offset.x as usize;
                    owner[i] = id;
                }
            }
        }
    }
    let mut px = vec![Vec::new(); ordered.len()];
    for l in seg.layers {
        let c = l.paint.color();
        let id = ids[&[c.r, c.g, c.b]];
        for y in 0..l.mask.height() {
            for x in 0..l.mask.width() {
                if l.mask.image.get_pixel(x, y) {
                    let i = (y + l.mask.offset.y as usize) * w + x + l.mask.offset.x as usize;
                    if owner[i] >= id {
                        px[id].push(i);
                    }
                }
            }
        }
    }
    let mut result = Segmentation::new(w as u32, h as u32);
    for ((color, _), mut pixels) in ordered.into_iter().zip(px) {
        pixels.sort_unstable();
        pixels.dedup();
        if pixels.is_empty() {
            continue;
        }
        let mut bounds = crate::vc::BoundingRect::default();
        for &i in &pixels {
            bounds.add_x_y((i % w) as i32, (i / w) as i32);
        }
        let mut image = BinaryImage::new_w_h(bounds.width() as usize, bounds.height() as usize);
        for i in pixels {
            image.set_pixel(i % w - bounds.left as usize, i / w - bounds.top as usize, true);
        }
        result.layers.push(Layer {
            paint: Paint::Solid(crate::vc::Color::new(color[0], color[1], color[2])),
            mask: RegionMask::new(image, PointI32 { x: bounds.left, y: bounds.top }),
        });
    }
    result
}
