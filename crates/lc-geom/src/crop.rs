//! The crop model.
//!
//! The image (w × h pixels after orientation) is rotated by `angle` about its centre. The crop is an
//! axis-aligned rectangle in that rotated frame, stored *normalized* to the image size
//! (`x / w`, `y / h`), so it is independent of preview resolution. The crop must stay inside the
//! rotated image (Lightroom's "Constrain to image").

use serde::{Deserialize, Serialize};

use crate::{Affine, Point, Rect, Vec2};

/// The smallest crop side, as a fraction of the photo's shorter side: what a drag of the crop
/// tool can make, and the least `crop.set` takes (below it an export comes out 1×1).
pub const MIN_CROP_FRACTION: f64 = 0.02;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CropGeometry {
    /// Normalized crop rectangle in the rotated frame (0..1 when angle = 0).
    pub rect: Rect,
    /// Straighten angle in degrees, clockwise positive, −45..45.
    pub angle: f64,
}

impl Default for CropGeometry {
    fn default() -> Self {
        Self { rect: Rect::UNIT, angle: 0.0 }
    }
}

impl CropGeometry {
    pub fn is_identity(&self) -> bool {
        self.angle == 0.0 && self.rect == Rect::UNIT
    }

    /// Crop rectangle in pixels of the rotated frame for an image of `w × h`.
    pub fn rect_px(&self, w: f64, h: f64) -> Rect {
        self.rect.scale(w, h)
    }

    /// Transform from *output* (cropped) pixel coordinates to *source* (oriented image) pixel
    /// coordinates, for an output of `out_w × out_h` pixels rendered from a `w × h` source.
    pub fn output_to_source(&self, w: f64, h: f64, out_w: f64, out_h: f64) -> Affine {
        let r = self.rect_px(w, h);
        // output -> crop rect in rotated frame
        let to_rot = Affine::translate(Vec2::new(r.x0, r.y0)) * Affine::scale(r.width() / out_w, r.height() / out_h);
        // rotated frame -> source: inverse rotation about the image centre
        let c = Point::new(w / 2.0, h / 2.0);
        Affine::rotate_about(-self.angle.to_radians(), c) * to_rot
    }

    /// The crop on whole pixels of a `w × h` image when it is unrotated, as Lightroom crops: a
    /// full-size render then copies pixels instead of interpolating them at the crop's sub-pixel
    /// phase. Position and size round separately, so moving a crop never changes its size. A
    /// rotated crop is returned as is.
    pub fn snapped(&self, w: f64, h: f64) -> CropGeometry {
        if self.angle != 0.0 || !(w >= 1.0 && h >= 1.0) {
            return *self;
        }
        let axis = |a: f64, b: f64, n: f64| {
            if !(a.is_finite() && b.is_finite()) {
                return (a, b);
            }
            let len = ((b - a) * n).round().clamp(0.0, n);
            let lo = (a * n).round().clamp(0.0, n - len);
            (lo / n, (lo + len) / n)
        };
        let (x0, x1) = axis(self.rect.x0, self.rect.x1, w);
        let (y0, y1) = axis(self.rect.y0, self.rect.y1, h);
        CropGeometry { rect: Rect::new(x0, y0, x1, y1), angle: self.angle }
    }

    /// Whether every crop corner lies within the rotated image.
    pub fn is_within_image(&self, w: f64, h: f64) -> bool {
        let r = self.rect_px(w, h);
        let back = Affine::rotate_about(-self.angle.to_radians(), Point::new(w / 2.0, h / 2.0));
        r.corners().iter().all(|p| {
            let q = back.apply(*p);
            q.x >= -1e-6 * w && q.y >= -1e-6 * h && q.x <= w * (1.0 + 1e-6) && q.y <= h * (1.0 + 1e-6)
        })
    }

    /// Shrink the crop (about its centre, keeping aspect) and then shift it until it fits inside the
    /// rotated image.
    pub fn constrained(&self, w: f64, h: f64) -> CropGeometry {
        if self.is_within_image(w, h) {
            return *self;
        }
        let r = self.rect_px(w, h);
        // Try moving towards the image centre first.
        let c = Point::new(w / 2.0, h / 2.0);
        let mut best = *self;
        for i in 1..=32 {
            let t = i as f64 / 32.0;
            let nc = r.center().lerp(c, t);
            let cand = CropGeometry { rect: Rect::from_center(nc, r.width(), r.height()).scale(1.0 / w, 1.0 / h), angle: self.angle };
            if cand.is_within_image(w, h) {
                return cand;
            }
            best = cand;
        }
        // Then shrink around the centre.
        let s = max_inscribed_scale(w, h, self.angle, r.width(), r.height());
        let nr = Rect::from_center(c, r.width() * s, r.height() * s);
        let _ = best;
        CropGeometry { rect: nr.scale(1.0 / w, 1.0 / h), angle: self.angle }
    }
}

/// Largest scale `s` such that a centred `cw·s × ch·s` rectangle fits inside a `w × h` image rotated
/// by `angle_deg` about its centre.
pub fn max_inscribed_scale(w: f64, h: f64, angle_deg: f64, cw: f64, ch: f64) -> f64 {
    let a = angle_deg.to_radians().abs();
    let (s, c) = a.sin_cos();
    // Half extents of the crop must satisfy, in the image frame (rotated back):
    // |x| c + |y| s <= w/2 and |x| s + |y| c <= h/2 for the crop corner (x, y) = (cw/2, ch/2)·scale.
    let (hx, hy) = (cw / 2.0, ch / 2.0);
    let s1 = (w / 2.0) / (hx * c + hy * s);
    let s2 = (h / 2.0) / (hx * s + hy * c);
    s1.min(s2)
}

/// The biggest crop of the image's own aspect (or `aspect` if given) that fits at `angle_deg`.
pub fn crop_fit_angle(w: f64, h: f64, angle_deg: f64, aspect: Option<f64>) -> CropGeometry {
    let a = aspect.unwrap_or(w / h);
    let base = Rect::new(0.0, 0.0, w, h).fit_aspect(a);
    let s = max_inscribed_scale(w, h, angle_deg, base.width(), base.height()).min(1.0);
    let r = Rect::from_center(Point::new(w / 2.0, h / 2.0), base.width() * s, base.height() * s);
    CropGeometry { rect: r.scale(1.0 / w, 1.0 / h), angle: angle_deg }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_maps_pixels() {
        let c = CropGeometry::default();
        let t = c.output_to_source(400.0, 300.0, 400.0, 300.0);
        let p = t.apply(Point::new(10.0, 20.0));
        assert!((p.x - 10.0).abs() < 1e-9 && (p.y - 20.0).abs() < 1e-9);
    }

    #[test]
    fn unrotated_crops_snap_to_whole_pixels() {
        let snap = |r: Rect| CropGeometry { rect: r, angle: 0.0 }.snapped(2000.0, 1000.0).rect_px(2000.0, 1000.0);
        let r = snap(Rect::new(0.4 / 2000.0, 0.2004, 1500.4 / 2000.0, 0.8));
        for (v, want) in [(r.x0, 0.0), (r.x1, 1500.0), (r.y0, 200.0), (r.y1, 800.0)] {
            assert!((v - want).abs() < 1e-9, "{r:?}");
        }
        // moving the crop by a fraction of a pixel never changes its size
        for dx in [0.0, 0.3, 0.5, 0.7, 1.49] {
            let r = snap(Rect::new((10.0 + dx) / 2000.0, 0.0, (1510.4 + dx) / 2000.0, 1.0));
            assert!((r.width() - 1500.0).abs() < 1e-9 && (r.x0 - r.x0.round()).abs() < 1e-9, "{dx}: {r:?}");
        }
        // a crop hanging over the edge stays inside, the whole frame is untouched, a rotated crop is kept
        let r = snap(Rect::new(0.9, -0.0004, 1.0002, 0.5));
        assert!(r.x1 <= 2000.0 + 1e-9 && r.y0 >= 0.0 && (r.width() - 200.0).abs() < 1e-9, "{r:?}");
        assert_eq!(CropGeometry::default().snapped(2000.0, 1000.0), CropGeometry::default());
        let rotated = CropGeometry { rect: Rect::new(0.1003, 0.1, 0.9, 0.9), angle: 2.0 };
        assert_eq!(rotated.snapped(2000.0, 1000.0), rotated);
    }

    #[test]
    fn rotated_fit_is_inside() {
        for ang in [-45.0, -10.0, 0.0, 3.5, 30.0, 45.0] {
            let c = crop_fit_angle(600.0, 400.0, ang, None);
            assert!(c.is_within_image(600.0, 400.0), "{ang}");
            // and maximal: 1% bigger does not fit (except angle 0 where it equals the image)
            let big = CropGeometry { rect: Rect::from_center(c.rect.center(), c.rect.width() * 1.01, c.rect.height() * 1.01), angle: ang };
            assert!(!big.is_within_image(600.0, 400.0), "{ang}");
        }
    }

    #[test]
    fn constrain_pulls_crop_inside() {
        let c = CropGeometry { rect: Rect::new(0.5, 0.5, 1.4, 1.2), angle: 12.0 };
        let k = c.constrained(300.0, 200.0);
        assert!(k.is_within_image(300.0, 200.0));
    }

    #[test]
    fn half_crop_maps_to_centre_quadrant() {
        let c = CropGeometry { rect: Rect::new(0.25, 0.25, 0.75, 0.75), angle: 0.0 };
        let t = c.output_to_source(100.0, 100.0, 50.0, 50.0);
        let p = t.apply(Point::new(0.0, 0.0));
        assert!((p.x - 25.0).abs() < 1e-9 && (p.y - 25.0).abs() < 1e-9);
    }
}
