//! Output framing: user orientation, lens corrections and perspective ([`Warp`]), crop + straighten,
//! flips — and the mapping between output pixels and normalized coordinates (used by masks, spots and
//! on-canvas tools).
//!
//! Normalized coordinates (masks, spots, crop) refer to the *transformed* image: the oriented source after
//! lens correction and perspective, at the source's size. Everything is sampled in one resample.

use li_raster::resample::{ANTIRING_SLACK, Filter, offset_weights, weights};
use li_raster::{RgbFrame, par_rows};
use lightcraft_develop::{DevelopSettings, EmbeddedLens};
use lightcraft_geom::{Affine, CropGeometry, Homography, Orientation, Point, Rect};

use crate::optics::{Warp, reorient_lens};

/// Linear value of areas outside the source after a warp (a light neutral, like an empty canvas).
pub const BLANK: [f32; 3] = [0.55, 0.55, 0.55];

/// The geometric frame of a render.
#[derive(Clone, Debug)]
pub struct Frame {
    /// Source (EXIF-oriented) dimensions in pixels.
    pub src_w: usize,
    pub src_h: usize,
    /// User orientation (Rotate Left/Right, Flip) on top of the source.
    pub orient: Orientation,
    /// Oriented dimensions.
    pub ow: f64,
    pub oh: f64,
    pub crop: CropGeometry,
    pub flip_h: bool,
    pub flip_v: bool,
    /// Lens corrections / perspective (None = identity).
    pub warp: Option<Warp>,
}

impl Frame {
    pub fn new(src_w: usize, src_h: usize, s: &DevelopSettings, apply_crop: bool) -> Frame {
        Frame::with_lens(src_w, src_h, s, apply_crop, None)
    }

    /// Like [`Frame::new`], with the source's embedded lens corrections (relative to the EXIF-oriented source).
    pub fn with_lens(src_w: usize, src_h: usize, s: &DevelopSettings, apply_crop: bool, lens: Option<&EmbeddedLens>) -> Frame {
        Frame::with_lenses(src_w, src_h, s, apply_crop, lens, None)
    }

    /// Embedded data takes precedence over the lens database when the master switch is on.
    pub fn with_lenses(
        src_w: usize,
        src_h: usize,
        s: &DevelopSettings,
        apply_crop: bool,
        lens: Option<&EmbeddedLens>,
        lensdb: Option<&crate::lensdb::LensCorrection>,
    ) -> Frame {
        Self::with_lcp(src_w, src_h, s, apply_crop, lens, lensdb, None)
    }

    /// Frame with one automatic source: embedded data, lensfun, then user LCP.
    #[allow(clippy::too_many_arguments)]
    pub fn with_lcp(
        src_w: usize,
        src_h: usize,
        s: &DevelopSettings,
        apply_crop: bool,
        lens: Option<&EmbeddedLens>,
        lensdb: Option<&crate::lensdb::LensCorrection>,
        lcp: Option<&crate::lcp::LcpCorrection>,
    ) -> Frame {
        let orient = s.orientation;
        let (ow, oh) = if orient.swaps_axes() { (src_h as f64, src_w as f64) } else { (src_w as f64, src_h as f64) };
        let crop = if apply_crop { s.crop.geometry.snapped(ow, oh) } else { CropGeometry::default() };
        let lens = lens.map(|l| reorient_lens(l, orient, src_w as f64, src_h as f64));
        let mut warp = Warp::from_settings(ow, oh, s, lens.as_ref());
        if let Some(c) = lensdb {
            warp.set_lensdb(c, orient, src_w as f64, src_h as f64, s);
        }
        if let Some(c) = lcp {
            warp.set_lcp(c, orient, src_w as f64, src_h as f64, s);
        }
        let persp = perspective(s, ow, oh);
        if persp != Homography::IDENTITY
            && let Some(inv) = persp.inverse()
        {
            warp.persp = persp;
            warp.persp_inv = inv;
        }
        let mut f = Frame {
            src_w,
            src_h,
            orient,
            ow,
            oh,
            crop,
            flip_h: apply_crop && s.crop.flip_h,
            flip_v: apply_crop && s.crop.flip_v,
            warp: (!warp.is_identity()).then_some(warp),
        };
        if apply_crop && s.geometry.constrain_crop {
            f.constrain_to_image();
        }
        f
    }

    /// The green-plane geometry for scalar sensor metadata. Shading, channel
    /// mixing and CA must not alter clip confidence or other scalar facts.
    pub fn for_metadata(&self) -> Frame {
        let mut frame = self.clone();
        if let Some(warp) = frame.warp.as_mut() {
            warp.camera_basis = None;
            warp.ca = [0.0; 3];
            warp.vig_stops = 0.0;
            warp.lens_vig = 0.0;
            warp.lensdb_vig = 0.0;
            warp.lensdb_tca = 0.0;
            warp.lcp_vig = 0.0;
            if let Some(lcp) = warp.lcp.as_mut() {
                lcp.correction.chromatic = lcp.correction.chromatic.map(|m| [m[1]; 3]);
            }
            if let Some(lens) = warp.lens.as_mut() {
                if let Some(w) = lens.warp.as_mut() {
                    w.planes = [w.planes[1]; 3];
                }
                if let Some(r) = lens.radial.as_mut() {
                    r.scales = [r.scales[1]; 3];
                }
            }
        }
        frame
    }

    /// "Constrain Crop": shrink the crop about its centre until it contains no area outside the warped source.
    fn constrain_to_image(&mut self) {
        let Some(wp) = self.warp.as_ref().filter(|w| w.moves_pixels()) else { return };
        let a = self.crop.output_to_source(self.ow, self.oh, 1.0, 1.0);
        let (w, h) = (self.ow, self.oh);
        let fits = |t: f64| {
            (0..=48).all(|i| {
                let u = i as f64 / 48.0;
                [(u, 0.0), (u, 1.0), (0.0, u), (1.0, u)].iter().all(|&(x, y)| {
                    let p = Point::new(0.5 + t * (x - 0.5), 0.5 + t * (y - 0.5));
                    let s = wp.to_source(a.apply(p), 1);
                    s.x >= -1e-6 * w && s.y >= -1e-6 * h && s.x <= w * (1.0 + 1e-6) && s.y <= h * (1.0 + 1e-6)
                })
            })
        };
        if fits(1.0) {
            return;
        }
        let (mut lo, mut hi) = (0.0, 1.0);
        for _ in 0..30 {
            let m = (lo + hi) / 2.0;
            if fits(m) { lo = m } else { hi = m }
        }
        let t = lo.max(0.02);
        let r = self.crop.rect;
        self.crop.rect = Rect::from_center(r.center(), r.width() * t, r.height() * t);
    }

    /// Whether the GPU's geometry kernel can sample this frame, including lens database models.
    pub fn gpu_samplable(&self) -> bool {
        self.warp.as_ref().is_none_or(|w| w.lcp.is_none())
    }

    /// Add automatically estimated lateral CA (`[α_R, α_B]`, see [`crate::optics::estimate_lateral_ca`]).
    pub fn add_lateral_ca(&mut self, ca: [f64; 2]) {
        if ca == [0.0; 2] {
            return;
        }
        let w = self.warp.get_or_insert_with(|| Warp::identity(self.ow, self.oh));
        w.ca[0] += ca[0];
        w.ca[2] += ca[1];
    }

    /// Map a point of the transformed image (normalized) to the lens-corrected, pre-perspective image (normalized).
    pub fn transformed_to_corrected(&self, p: Point) -> Point {
        match &self.warp {
            Some(w) => {
                let q = w.to_corrected(Point::new(p.x * self.ow, p.y * self.oh));
                Point::new(q.x / self.ow, q.y / self.oh)
            }
            None => p,
        }
    }

    /// Inverse of [`Frame::transformed_to_corrected`].
    pub fn corrected_to_transformed(&self, p: Point) -> Point {
        match &self.warp {
            Some(w) => {
                let q = w.from_corrected(Point::new(p.x * self.ow, p.y * self.oh));
                Point::new(q.x / self.ow, q.y / self.oh)
            }
            None => p,
        }
    }

    /// Aspect ratio (w/h) of the output. A crop narrower than a source pixel (or of no size, from
    /// an agent or a sidecar) counts each side as one pixel, so the aspect stays finite: 0/0 made
    /// it NaN, and the loupe laid the photo out at a NaN size.
    pub fn aspect(&self) -> f64 {
        let r = self.crop.rect_px(self.ow, self.oh);
        r.width().max(1.0) / r.height().max(1.0)
    }

    /// Size of the (cropped) output at the source's own resolution, in pixels.
    pub fn native_size(&self) -> (f64, f64) {
        let r = self.crop.rect_px(self.ow, self.oh);
        (r.width(), r.height())
    }

    pub fn fit(&self, max_w: usize, max_h: usize) -> (usize, usize) {
        let a = self.aspect();
        let (mw, mh) = (max_w.max(1) as f64, max_h.max(1) as f64);
        let (w, h) = if mw / mh > a { (mh * a, mh) } else { (mw, mw / a) };
        ((w.round() as usize).max(1), (h.round() as usize).max(1))
    }

    /// Output pixels per unit of the oriented image's long edge.
    pub fn px_per_long(&self, out_w: usize) -> f64 {
        let r = self.crop.rect_px(self.ow, self.oh);
        out_w as f64 / r.width() * self.ow.max(self.oh)
    }

    /// Affine from output pixel coordinates to oriented-image pixel coordinates.
    pub fn out_to_oriented(&self, out_w: usize, out_h: usize) -> Affine {
        let (w, h) = (out_w as f64, out_h as f64);
        let mut flip = Affine::IDENTITY;
        if self.flip_h {
            flip = Affine([-1.0, 0.0, 0.0, 1.0, w, 0.0]) * flip;
        }
        if self.flip_v {
            flip = Affine([1.0, 0.0, 0.0, -1.0, 0.0, h]) * flip;
        }
        self.crop.output_to_source(self.ow, self.oh, w, h) * flip
    }

    /// Map an output pixel to normalized oriented-image coordinates (0..1).
    pub fn out_to_norm(&self, out_w: usize, out_h: usize) -> Affine {
        Affine::scale(1.0 / self.ow, 1.0 / self.oh) * self.out_to_oriented(out_w, out_h)
    }

    /// Normalized oriented coords → output pixel (inverse of `out_to_norm`).
    pub fn norm_to_out(&self, out_w: usize, out_h: usize) -> Affine {
        self.out_to_norm(out_w, out_h).inverse().unwrap_or(Affine::IDENTITY)
    }

    /// Convert normalized oriented coordinates to "long-edge units" (isotropic), used by mask shapes.
    pub fn norm_to_long(&self, p: Point) -> Point {
        let l = self.ow.max(self.oh);
        Point::new(p.x * self.ow / l, p.y * self.oh / l)
    }

    /// How [`Frame::sample`] resamples a (EXIF-oriented) source of `src_w × src_h` into `w × h`.
    pub fn sample_plan(&self, src_w: usize, src_h: usize, w: usize, h: usize) -> SamplePlan {
        let (ow, oh) = if self.orient.swaps_axes() { (src_h, src_w) } else { (src_w, src_h) };
        let crop_px = self.crop.rect_px(self.ow, self.oh);
        let k = crop_px.width() / w as f64;
        // Not an enlargement: the output is at most the half pixel its size rounds by larger.
        let slack = if crop_px.width() + 0.5 >= w as f64 && crop_px.height() + 0.5 >= h as f64 { ANTIRING_SLACK } else { 0.0 };
        let o2o = self.out_to_oriented(w, h);
        // An axis-aligned mapping (no straighten, warp or perspective) is separable.
        if self.warp.is_none() && o2o.0[1] == 0.0 && o2o.0[2] == 0.0 {
            let mode = if self.crop.is_identity() && !self.flip_h && !self.flip_v && (ow, oh) == (w, h) {
                SampleMode::Copy
            } else {
                let filter = if k.max(crop_px.height() / h as f64) > PREFILTER_K { Filter::Mitchell } else { Filter::CatmullRom };
                SampleMode::Separable { x: [o2o.0[0], o2o.0[4]], y: [o2o.0[3], o2o.0[5]], filter }
            };
            return SamplePlan { ow, oh, prefilter: None, sx: 1.0, sy: 1.0, slack, mode };
        }
        let prefilter = (k > PREFILTER_K).then(|| (((ow as f64 / k).round() as usize).max(1), ((oh as f64 / k).round() as usize).max(1)));
        let (bw, bh) = prefilter.unwrap_or((ow, oh));
        let (sx, sy) = (bw as f64 / ow as f64, bh as f64 / oh as f64);
        let mode = if self.warp.is_some() { SampleMode::Warp(o2o) } else { SampleMode::Affine(Affine::scale(sx, sy) * o2o) };
        SamplePlan { ow, oh, prefilter, sx, sy, slack, mode }
    }

    /// Sample the source into a `w × h` output buffer: orientation, crop, rotation, flips — one
    /// resample. An unrotated, unwarped frame is one separable pass straight from the source
    /// ([`SampleMode::Separable`]); otherwise a Catmull-Rom resample
    /// ([`RgbFrame::sample_cubic`], widened where the map minifies) from a pre-filtered
    /// (area-downscaled) copy, so minification never aliases. All suppress ringing: a cubic of
    /// scene-linear light would otherwise outline every bright window or lamp in black.
    pub fn sample(&self, src: &RgbFrame, w: usize, h: usize) -> RgbFrame {
        let oriented = if self.orient == Orientation::Normal { None } else { Some(src.oriented(self.orient)) };
        let o = oriented.as_ref().unwrap_or(src);
        let plan = self.sample_plan(src.width, src.height, w, h);
        let base = match plan.prefilter {
            Some((nw, nh)) => {
                std::borrow::Cow::Owned(o.resample_antiring(&weights(o.width, nw, Filter::Mitchell), &weights(o.height, nh, Filter::Mitchell), 0.0))
            }
            None => std::borrow::Cow::Borrowed(o),
        };
        let affine = |xf: Affine| {
            let stretch = affine_stretch(&xf);
            let mut out = RgbFrame::new(w, h);
            par_rows(&mut out.data, w, |y, row| {
                for (x, px) in row.iter_mut().enumerate() {
                    let p = xf.apply(Point::new(x as f64 + 0.5, y as f64 + 0.5));
                    *px = base.sample_cubic(p.x as f32, p.y as f32, stretch, plan.slack);
                }
            });
            out
        };
        match (&plan.mode, self.warp.as_ref()) {
            (SampleMode::Copy, _) => base.into_owned(),
            (SampleMode::Separable { x, y, filter }, _) => {
                let (wx, wy) = separable_taps(&plan, *x, *y, *filter, w, h);
                o.resample_antiring(&wx, &wy, plan.slack)
            }
            (SampleMode::Warp(o2t), Some(warp)) => sample_warped(&base, &plan, warp, *o2t, w, h),
            // `sample_plan` plans a warp only when there is one; without it the same mapping is affine
            (SampleMode::Warp(o2t), None) => affine(Affine::scale(plan.sx, plan.sy) * *o2t),
            (SampleMode::Affine(xf), _) => affine(*xf),
        }
    }

    /// The vignetting compensation [`Frame::sample`] applies to a `w × h` output, per pixel (1 on
    /// blank canvas), when the warp has one.
    pub fn vignette_gain(&self, w: usize, h: usize) -> Option<li_raster::GrayFrame> {
        let wp = self.warp.as_ref().filter(|wp| wp.has_gain())?;
        let o2t = self.out_to_oriented(w, h);
        let mut out = li_raster::GrayFrame::new(w, h);
        par_rows(&mut out.data, w, |y, row| {
            for (x, k) in row.iter_mut().enumerate() {
                *k = wp.frame(&o2t, x, y).map(|(_, g)| wp.gain(g)).filter(|k| k.is_finite() && *k > 0.0).unwrap_or(1.0);
            }
        });
        Some(out)
    }

    /// The crop rectangle as a quad in normalized oriented coordinates (for overlays).
    pub fn crop_quad_norm(&self) -> [Point; 4] {
        let r = self.crop.rect_px(self.ow, self.oh);
        let back = Affine::rotate_about(-self.crop.angle.to_radians(), Point::new(self.ow / 2.0, self.oh / 2.0));
        r.corners().map(|p| {
            let q = back.apply(p);
            Point::new(q.x / self.ow, q.y / self.oh)
        })
    }
}

/// How a render resamples its source (see [`Frame::sample_plan`]).
#[derive(Clone, Debug)]
pub struct SamplePlan {
    /// Oriented source size.
    pub ow: usize,
    pub oh: usize,
    /// Size of the Mitchell prefilter (area downscale with ringing suppressed, before the cubic
    /// resample), if any.
    pub prefilter: Option<(usize, usize)>,
    /// Oriented-source px → prefiltered px.
    pub sx: f64,
    pub sy: f64,
    /// The final resample's anti-ringing slack: [`ANTIRING_SLACK`], none when it enlarges.
    pub slack: f32,
    pub mode: SampleMode,
}

#[derive(Clone, Debug)]
pub enum SampleMode {
    /// The oriented source is the output.
    Copy,
    /// One separable resample of the oriented source (crop, scale and flips without rotation or
    /// warp): output column `i` reads around source position `x[0]·(i + 0.5) + x[1]`, rows
    /// likewise ([`separable_taps`]). Catmull-Rom up to a [`PREFILTER_K`] reduction (an exact copy
    /// at unit scale on whole pixels), Mitchell beyond.
    Separable { x: [f64; 2], y: [f64; 2], filter: Filter },
    /// Catmull-Rom through this affine (output px → prefiltered px).
    Affine(Affine),
    /// Through the warp; the affine maps output px → transformed px.
    Warp(Affine),
}

/// Reduction (crop px per output px) above which a rotated or warped render resamples a Mitchell
/// prefiltered copy, and an unrotated one uses Mitchell instead of Catmull-Rom.
pub const PREFILTER_K: f64 = 1.25;

/// The taps (column, row) of a [`SampleMode::Separable`] resample into `w × h`.
pub fn separable_taps(
    plan: &SamplePlan,
    x: [f64; 2],
    y: [f64; 2],
    filter: Filter,
    w: usize,
    h: usize,
) -> (Vec<(usize, Vec<f32>)>, Vec<(usize, Vec<f32>)>) {
    (offset_weights(plan.ow, w, x[0], x[1], filter), offset_weights(plan.oh, h, y[0], y[1], filter))
}

/// The perspective homography of the settings (Upright, then the manual Transform sliders), lens-corrected →
/// transformed, in centred coordinates.
pub fn perspective(s: &DevelopSettings, ow: f64, oh: f64) -> Homography {
    if !s.section_enabled("geometry") {
        return Homography::IDENTITY;
    }
    crate::transform::manual(&s.geometry).mul(&crate::upright::homography(&s.geometry, ow, oh))
}

/// The kernel widening ([`RgbFrame::sample_cubic`]'s `stretch`) of a resample through `xf` (output px →
/// base px): how many base pixels one output pixel spans along each base axis.
pub fn affine_stretch(xf: &Affine) -> [f32; 2] {
    let a = xf.0;
    [a[0].hypot(a[2]), a[1].hypot(a[3])].map(|v| if v.is_finite() { v as f32 } else { 1.0 })
}

/// Output pixels per side of a block of [`warp_stretch`].
pub const STRETCH_BLOCK: usize = 16;

/// The kernel widening of a resample through the warp ([`RgbFrame::sample_cubic`]'s `stretch`) for each
/// block of [`STRETCH_BLOCK`]² output pixels, row-major: how many base pixels one output pixel spans along
/// each base axis, from the green plane's map at the block centre. Where the warp minifies (Upright
/// squeezes the foreground of a steeply tilted shot by 1.5× or more) the kernel widens instead of
/// aliasing. Both renderers read this one table.
pub fn warp_stretch(wp: &Warp, o2t: &Affine, sx: f64, sy: f64, w: usize, h: usize) -> Vec<[f32; 2]> {
    let (bw, bh) = (w.div_ceil(STRETCH_BLOCK), h.div_ceil(STRETCH_BLOCK));
    let map = |x: f64, y: f64| {
        let p = wp.to_source(o2t.apply(Point::new(x, y)), 1);
        Point::new(p.x * sx, p.y * sy)
    };
    let centre = |b: usize, n: usize| (b * STRETCH_BLOCK + (b * STRETCH_BLOCK + STRETCH_BLOCK).min(n)) as f64 / 2.0;
    (0..bw * bh)
        .map(|i| {
            let (cx, cy) = (centre(i % bw, w), centre(i / bw, h));
            let (dx, dy) = (map(cx + 0.5, cy) - map(cx - 0.5, cy), map(cx, cy + 0.5) - map(cx, cy - 0.5));
            [dx.x.hypot(dy.x), dx.y.hypot(dy.y)].map(|v| if v.is_finite() { v as f32 } else { 1.0 })
        })
        .collect()
}

/// One resample through the warp: `o2t` maps output px → transformed px, the warp maps those to source px
/// (scaled by the plan's `sx, sy` into `base`).
fn sample_warped(base: &RgbFrame, plan: &SamplePlan, wp: &Warp, o2t: Affine, w: usize, h: usize) -> RgbFrame {
    let (sx, sy, slack) = (plan.sx, plan.sy, plan.slack);
    let per_channel = wp.per_channel();
    let gain = wp.has_gain();
    let stretch = warp_stretch(wp, &o2t, sx, sy, w, h);
    let blocks = w.div_ceil(STRETCH_BLOCK);
    let mut out = RgbFrame::new(w, h);
    par_rows(&mut out.data, w, |y, row| {
        for (x, px) in row.iter_mut().enumerate() {
            let Some((c, g)) = wp.frame(&o2t, x, y) else {
                *px = BLANK;
                continue;
            };
            let st = stretch[(y / STRETCH_BLOCK) * blocks + x / STRETCH_BLOCK];
            let mut v = base.sample_cubic((g.x * sx) as f32, (g.y * sy) as f32, st, slack);
            if per_channel {
                if let Some([undo, forward]) = wp.camera_basis {
                    // Colour mixing and channel-dependent resampling do not
                    // commute. Reconstruct each camera plane at its own source
                    // position before applying the forward colour transform.
                    v = undo.apply_f32(v);
                    for ch in [0usize, 2] {
                        let q = wp.corrected_to_source(c, ch);
                        let rgb = base.sample_cubic((q.x * sx) as f32, (q.y * sy) as f32, st, slack);
                        v[ch] = undo.apply_f32(rgb)[ch];
                    }
                    v = forward.apply_f32(v);
                } else {
                    for ch in [0usize, 2] {
                        let q = wp.corrected_to_source(c, ch);
                        v[ch] = base.sample_cubic((q.x * sx) as f32, (q.y * sy) as f32, st, slack)[ch];
                    }
                }
            }
            if gain {
                let k = wp.gain(g);
                v = v.map(|c| c * k);
            }
            *px = v;
        }
    });
    out
}

/// Whole-image rectangle in normalized coordinates.
pub const FULL: Rect = Rect::UNIT;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_frame_is_identity() {
        let s = DevelopSettings::default();
        let f = Frame::new(300, 200, &s, true);
        assert_eq!(f.fit(3000, 3000), (3000, 2000));
        let m = f.out_to_norm(300, 200);
        let p = m.apply(Point::new(150.0, 100.0));
        assert!((p.x - 0.5).abs() < 1e-9 && (p.y - 0.5).abs() < 1e-9);
        assert!((f.px_per_long(300) - 300.0).abs() < 1e-9);
    }

    #[test]
    fn lens_database_zero_strength_and_identity_use_the_fast_sample_plan() {
        use crate::lensdb::{Distortion, LensCorrection, Tca};
        let mut s = DevelopSettings::default();
        s.lens_db.enabled = true;
        s.optics.lens_profile = true;
        let active = LensCorrection {
            diag_norm: 2.0,
            distortion: Distortion::Poly3 { k1: -0.3 },
            tca: Tca::Linear { kr: 1.02, kb: 0.98 },
            vignetting: Some([-0.4, 0.1, 0.0]),
            ..Default::default()
        };
        let identities = [
            LensCorrection::default(),
            LensCorrection { distortion: Distortion::Poly3 { k1: 0.0 }, tca: Tca::Linear { kr: 1.0, kb: 1.0 }, vignetting: Some([0.0; 3]), ..active },
            LensCorrection {
                distortion: Distortion::Poly5 { k1: 0.0, k2: 0.0 },
                tca: Tca::Poly3 { red: [1.0, 0.0, 0.0], blue: [1.0, 0.0, 0.0] },
                vignetting: None,
                ..active
            },
            LensCorrection { distortion: Distortion::Ptlens { a: 0.0, b: 0.0, c: 0.0 }, tca: Tca::None, vignetting: None, ..active },
        ];
        for c in identities {
            assert!(c.is_identity());
            let f = Frame::with_lenses(301, 199, &s, true, None, Some(&c));
            assert!(f.gpu_samplable() && f.warp.is_none());
            assert!(matches!(f.sample_plan(301, 199, 301, 199).mode, SampleMode::Copy));
        }
        s.lens_db.distortion = 0.0;
        s.lens_db.tca = 0.0;
        s.lens_db.vignetting = 0.0;
        for o in [
            Orientation::Normal,
            Orientation::Rotate90,
            Orientation::Rotate180,
            Orientation::Rotate270,
            Orientation::FlipH,
            Orientation::FlipV,
            Orientation::Transpose,
            Orientation::Transverse,
        ] {
            s.orientation = o;
            let f = Frame::with_lenses(301, 199, &s, true, None, Some(&active));
            assert!(f.gpu_samplable() && f.warp.is_none());
            assert!(matches!(f.sample_plan(301, 199, f.ow as usize, f.oh as usize).mode, SampleMode::Copy));
            s.crop.geometry.angle = 3.0;
            let cropped = Frame::with_lenses(301, 199, &s, true, None, Some(&active));
            assert!(cropped.warp.is_none());
            assert!(matches!(cropped.sample_plan(301, 199, 150, 100).mode, SampleMode::Affine(_)));
            s.crop.geometry.angle = 0.0;
        }
        s.lens_db.distortion = 100.0;
        let f = Frame::with_lenses(301, 199, &s, true, None, Some(&active));
        assert!(f.gpu_samplable());
        assert!(matches!(f.sample_plan(301, 199, 301, 199).mode, SampleMode::Warp(_)));
    }

    #[test]
    fn rotation_swaps_aspect() {
        let s = DevelopSettings { orientation: Orientation::Rotate90, ..Default::default() };
        let f = Frame::new(300, 200, &s, true);
        assert_eq!(f.fit(1000, 1000), (667, 1000));
    }

    #[test]
    fn crop_changes_aspect_and_mapping() {
        let mut s = DevelopSettings::default();
        s.crop.geometry.rect = Rect::new(0.5, 0.0, 1.0, 1.0);
        let f = Frame::new(400, 200, &s, true);
        assert_eq!(f.fit(1000, 1000), (1000, 1000));
        let p = f.out_to_norm(100, 100).apply(Point::new(0.0, 0.0));
        assert!((p.x - 0.5).abs() < 1e-9 && p.y.abs() < 1e-9);
        let back = f.norm_to_out(100, 100).apply(Point::new(0.75, 0.5));
        assert!((back.x - 50.0).abs() < 1e-6 && (back.y - 50.0).abs() < 1e-6);
    }

    /// A crop of no size (or under a pixel wide) gave a NaN or infinite aspect; each side now
    /// counts as at least one source pixel.
    #[test]
    fn degenerate_crops_have_a_finite_aspect() {
        let mut s = DevelopSettings::default();
        for (rect, want) in [
            (Rect::new(0.5, 0.5, 0.5, 0.5), 1.0),
            (Rect::new(0.0, 0.0, 1e-9, 1.0), 1.0 / 200.0),
            (Rect::new(0.0, 0.5, 1.0, 0.5), 400.0),
            (Rect::new(0.25, 0.0, 0.75, 1.0), 1.0),
        ] {
            s.crop.geometry.rect = rect;
            let f = Frame::new(400, 200, &s, true);
            assert!((f.aspect() - want).abs() < 1e-9, "{rect:?}: {}", f.aspect());
            let (w, h) = f.fit(1000, 1000);
            assert!((1..=1000).contains(&w) && (1..=1000).contains(&h), "{rect:?}: {w}×{h}");
        }
    }

    /// A window eight stops over the wall beside it, rendered reduced (a fit view, a web export):
    /// the scene-linear prefilter used to ring the wall next to the window below zero, a black
    /// outline after tone mapping. Unrotated and straightened frames alike now stay within the
    /// scene's range, give or take the anti-ringing slack.
    #[test]
    fn reduced_renders_do_not_outline_bright_windows_in_black() {
        let (wall, window) = (0.18f32, 0.18 * 256.0);
        for edge in [600, 601, 602] {
            let src = RgbFrame::from_fn(1200, 400, |x, _| if x < edge { [wall; 3] } else { [window; 3] });
            for angle in [0.0, 2.0] {
                let mut s = DevelopSettings::default();
                s.crop.geometry.angle = angle;
                let f = Frame::new(1200, 400, &s, true);
                for (w, h) in [(400, 133), (533, 178)] {
                    let out = f.sample(&src, w, h);
                    for (i, p) in out.data.iter().enumerate() {
                        assert!(
                            p.iter().all(|v| *v >= wall * (1.0 - ANTIRING_SLACK - 1e-4) && *v <= window * (1.0 + ANTIRING_SLACK + 1e-4)),
                            "edge {edge}, angle {angle}, {w}×{h}, pixel {i}: {p:?}"
                        );
                    }
                }
            }
        }
    }

    /// Deterministic white noise around mid grey.
    fn noise(w: usize, h: usize) -> RgbFrame {
        RgbFrame::from_fn(w, h, |x, y| {
            let mut v = (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (y as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
            v ^= v >> 29;
            v = v.wrapping_mul(0xBF58_476D_1CE4_E5B9);
            v ^= v >> 32;
            std::array::from_fn(|c| 0.1 + 0.3 * ((v >> (16 * c)) & 0xffff) as f32 / 65535.0)
        })
    }

    /// Mean squared difference of horizontal neighbours over the variance: the share of energy
    /// at the finest scale.
    fn fine_energy(img: &RgbFrame) -> f64 {
        let v: Vec<f64> = img.data.iter().map(|p| f64::from(p[1])).collect();
        let mean = v.iter().sum::<f64>() / v.len() as f64;
        let var = v.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / v.len() as f64;
        let d: f64 = img.data.chunks(img.width).flat_map(|r| r.windows(2).map(|p| f64::from(p[1][1] - p[0][1]).powi(2))).sum();
        d / (img.height * (img.width - 1)) as f64 / var
    }

    /// A full-size render of an unrotated crop copies whole source pixels (the crop snaps to the
    /// pixel grid, as in Lightroom); it was interpolated at the crop's sub-pixel phase, softening
    /// every cropped export. Flipped crops copy mirrored pixels.
    #[test]
    fn full_size_crops_copy_whole_source_pixels() {
        let src = noise(2000, 1000);
        for flip in [false, true] {
            let mut s = DevelopSettings::default();
            s.crop.geometry.rect = Rect::new(0.5 / 2000.0, 0.1003, 1500.5 / 2000.0, 0.9);
            s.crop.flip_h = flip;
            let f = Frame::new(2000, 1000, &s, true);
            let (nw, nh) = f.native_size();
            let (w, h) = (nw.round() as usize, nh.round() as usize);
            assert_eq!((w, h), (1500, 800));
            let r = f.crop.rect_px(2000.0, 1000.0);
            let (x0, y0) = (r.x0.round() as usize, r.y0.round() as usize);
            let out = f.sample(&src, w, h);
            for y in 0..h {
                for x in 0..w {
                    let want = src.get(x0 + if flip { w - 1 - x } else { x }, y0 + y);
                    let got = out.get(x, y);
                    assert!(got.iter().zip(want).all(|(a, b)| (a - b).abs() < 1e-6), "flip {flip}, {x},{y}: {got:?} vs {want:?}");
                }
            }
        }
    }

    /// A crop no longer costs sharpness in a reduced render: it went through the prefilter and then
    /// a second, bilinear resample at the crop's phase; now it is the same single pass as an
    /// uncropped render.
    #[test]
    fn crops_keep_reduced_renders_as_sharp_as_uncropped() {
        let src = noise(2400, 1600);
        let s = DevelopSettings::default();
        let whole = fine_energy(&Frame::new(2400, 1600, &s, true).sample(&src, 1000, 667));
        let mut s = s;
        s.crop.geometry.rect = Rect::new(0.37 / 2400.0, 0.21 / 1600.0, 2398.37 / 2400.0, 1598.21 / 1600.0);
        let cropped = fine_energy(&Frame::new(2400, 1600, &s, true).sample(&src, 1000, 667));
        assert!((cropped / whole - 1.0).abs() < 0.03, "cropped {cropped:.4} vs uncropped {whole:.4}");
    }

    /// A reduction by up to 1.25× (no prefilter) used two bilinear taps per pixel: stripes just
    /// under the source's Nyquist folded back at 41 % of their contrast. The widened Catmull-Rom
    /// band-limits the reduction.
    #[test]
    fn small_reductions_band_limit_fine_stripes() {
        let (fq, k) = (0.49f64, 1.25f64);
        let src = RgbFrame::from_fn(2500, 10, |x, _| [0.2 + 0.1 * (std::f64::consts::TAU * fq * (x as f64 + 0.5) + 0.3).cos() as f32; 3]);
        let f = Frame::new(2500, 10, &DevelopSettings::default(), true);
        let (w, h) = (2000, 8);
        assert!(matches!(f.sample_plan(2500, 10, w, h).mode, SampleMode::Separable { filter: Filter::CatmullRom, .. }));
        let out = f.sample(&src, w, h);
        let alias = (fq * k - (fq * k).round()).abs();
        let row: Vec<f64> = (0..w).map(|x| f64::from(out.get(x, 3)[0]) - 0.2).collect();
        let hann = |i: usize| 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / (w - 1) as f64).cos();
        let (mut re, mut im, mut norm) = (0.0, 0.0, 0.0);
        for (i, v) in row.iter().enumerate() {
            let a = std::f64::consts::TAU * alias * (i as f64 + 0.5);
            re += hann(i) * v * a.cos();
            im += hann(i) * v * a.sin();
            norm += hann(i);
        }
        let amplitude = 2.0 * re.hypot(im) / norm / 0.1;
        assert!(amplitude < 0.3, "aliased stripes at {:.0} % of their contrast", amplitude * 100.0);
    }

    /// Amplitude of `out` (a grating around 0.2) relative to `amplitude` at the phase
    /// `phase(source position)`, by a lock-in fit in windows of 32×32 output pixels.
    fn grating_response(f: &Frame, out: &RgbFrame, amplitude: f64, phase: impl Fn(Point) -> f64) -> Vec<f64> {
        let o2o = f.out_to_oriented(out.width, out.height);
        let mut res = Vec::new();
        for wy in (0..out.height - 31).step_by(32) {
            for wx in (0..out.width - 31).step_by(32) {
                let (mut c, mut s, mut n) = (0.0, 0.0, 0.0);
                for y in wy..wy + 32 {
                    for x in wx..wx + 32 {
                        let ph = phase(o2o.apply(Point::new(x as f64 + 0.5, y as f64 + 0.5)));
                        let v = f64::from(out.get(x, y)[1]) - 0.2;
                        c += v * ph.cos();
                        s += v * ph.sin();
                        n += 1.0;
                    }
                }
                res.push(2.0 * c.hypot(s) / n / amplitude);
            }
        }
        res
    }

    /// A straightened full-size render keeps fine texture everywhere: one bilinear tap passed a
    /// 4-pixel grating at 71 % where the sample phase is half a pixel, in bands across the frame.
    #[test]
    fn straightened_renders_keep_fine_texture() {
        let (w, h) = (640, 480);
        for (freq, offset) in [(0.25, 0.0), (0.2, 1.3)] {
            let phase = move |p: Point| std::f64::consts::TAU * freq * p.x + offset;
            let src = RgbFrame::from_fn(w, h, |x, y| [(0.2 + 0.04 * phase(Point::new(x as f64 + 0.5, y as f64 + 0.5)).cos()) as f32; 3]);
            let mut s = DevelopSettings::default();
            s.crop.geometry = lightcraft_geom::crop_fit_angle(w as f64, h as f64, 0.7, None);
            let f = Frame::new(w, h, &s, true);
            let (nw, nh) = f.native_size();
            let out = f.sample(&src, nw.round() as usize, nh.round() as usize);
            let r = grating_response(&f, &out, 0.04, phase);
            let (min, mean) = (r.iter().copied().fold(f64::MAX, f64::min), r.iter().sum::<f64>() / r.len() as f64);
            assert!(min >= 0.85 && mean >= 0.92, "{freq} cy/px: response min {min:.3}, mean {mean:.3}");
        }
    }

    /// A window 10 stops over the wall, straightened: the cubic rings neither side beyond the
    /// anti-ringing slack.
    #[test]
    fn straightened_hdr_edges_do_not_ring() {
        let src = RgbFrame::from_fn(400, 300, |x, _| if x < 200 { [0.01; 3] } else { [10.0; 3] });
        let mut s = DevelopSettings::default();
        s.crop.geometry = lightcraft_geom::crop_fit_angle(400.0, 300.0, 0.7, None);
        let f = Frame::new(400, 300, &s, true);
        let (nw, nh) = f.native_size();
        for p in f.sample(&src, nw.round() as usize, nh.round() as usize).data {
            let slack = ANTIRING_SLACK + 1e-4;
            assert!(p.iter().all(|v| (0.01 * (1.0 - slack)..=10.0 * (1.0 + slack)).contains(v)), "{p:?}");
        }
    }

    /// Where a perspective correction squeezes the image (the foreground of a steeply tilted shot),
    /// the kernel widens: fine stripes there aliased to coarse false stripes at about half their
    /// contrast through bilinear taps.
    #[test]
    fn warps_widen_the_kernel_where_they_minify() {
        let (w, h) = (1200, 800);
        let fq = 0.45;
        let src = RgbFrame::from_fn(w, h, |x, _| [(0.2 + 0.05 * (std::f64::consts::TAU * fq * (x as f64 + 0.5)).cos()) as f32; 3]);
        let mut s = DevelopSettings::default();
        s.geometry.vertical = 100.0;
        s.geometry.scale = 85.0;
        let f = Frame::new(w, h, &s, true);
        let wp = f.warp.as_ref().expect("a perspective warp");
        let o2t = f.out_to_oriented(w, h);
        let stretch = warp_stretch(wp, &o2t, 1.0, 1.0, w, h);
        let blocks = w.div_ceil(STRETCH_BLOCK);
        let out = f.sample(&src, w, h);
        let covered = |y: usize| (0..w).filter(|&x| out.get(x, y) != BLANK).count();
        // the most minified row band with enough of the photo in it
        let (row, local) = (0..h.div_ceil(STRETCH_BLOCK))
            .map(|by| (by * STRETCH_BLOCK + STRETCH_BLOCK / 2, stretch[by * blocks + blocks / 2][0]))
            .filter(|(y, _)| *y < h && covered(*y) > w / 2)
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .expect("rows");
        assert!(local > 1.4, "the keystone minifies the {row} row by {local}");
        // the numeric Jacobian agrees with the table
        let at = |x: f64| wp.to_source(o2t.apply(Point::new(x, row as f64 + 0.5)), 1).x;
        assert!(((at(w as f64 / 2.0 + 0.5) - at(w as f64 / 2.0 - 0.5)) as f32 - local).abs() < 0.05);
        let xs: Vec<usize> = (0..w).filter(|&x| out.get(x, row) != BLANK).collect();
        let (x0, n) = (xs[xs.len() / 6], xs.len() * 2 / 3);
        let alias = (fq * f64::from(local) - (fq * f64::from(local)).round()).abs();
        let hann = |i: usize| 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / (n - 1) as f64).cos();
        let (mut re, mut im, mut norm) = (0.0, 0.0, 0.0);
        for i in 0..n {
            let v = f64::from(out.get(x0 + i, row)[1]) - 0.2;
            let a = std::f64::consts::TAU * alias * i as f64;
            re += hann(i) * v * a.cos();
            im += hann(i) * v * a.sin();
            norm += hann(i);
        }
        let amplitude = 2.0 * re.hypot(im) / norm / 0.05;
        assert!(amplitude < 0.2, "stripes alias at {:.0} % of their contrast where the warp minifies {local:.2}×", amplitude * 100.0);
    }

    #[test]
    fn flip_mirrors() {
        let mut s = DevelopSettings::default();
        s.crop.flip_h = true;
        let src = RgbFrame::from_fn(4, 1, |x, _| [x as f32, 0.0, 0.0]);
        let f = Frame::new(4, 1, &s, true);
        let out = f.sample(&src, 4, 1);
        assert!((out.get(0, 0)[0] - 3.0).abs() < 1e-5);
        assert!((out.get(3, 0)[0] - 0.0).abs() < 1e-5);
    }
}
