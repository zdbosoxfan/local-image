//! Mask evaluation: each mask becomes an alpha plane at output resolution.
//!
//! Shapes are defined in normalized oriented-image coordinates; sizes are in "long-edge units".
//! Components combine with Add (max), Subtract (a·(1−c)) and Intersect (a·c).

use li_raster::{GrayFrame, RgbFrame};
use lightcraft_develop::{BrushStroke, LocalAdjustments, Mask, MaskOp, MaskShape, SegMask};
use lightcraft_geom::Point;

use crate::for_rows;
use crate::geometry::Frame;

#[derive(Clone)]
pub struct Evaluated {
    /// The mask's id ([`Mask::id`]).
    pub id: u32,
    pub alpha: Alpha,
    pub adjust: LocalAdjustments,
}

/// Full-only masks keep one scalar; other masks keep their spatial alpha plane.
#[derive(Clone)]
pub enum Alpha {
    Scalar(f32),
    Spatial(GrayFrame),
}
impl Alpha {
    #[inline]
    pub fn at(&self, i: usize) -> f32 {
        match self {
            Self::Scalar(a) => *a,
            Self::Spatial(p) => p.data[i],
        }
    }
    pub fn get(&self, x: usize, y: usize) -> f32 {
        match self {
            Self::Scalar(a) => *a,
            Self::Spatial(p) => p.get(x, y),
        }
    }
    pub fn plane(&self) -> Option<&GrayFrame> {
        match self {
            Self::Spatial(p) => Some(p),
            Self::Scalar(_) => None,
        }
    }
}
/// Constant coverage after component/layer inversion, amount and opacity. Refining a constant
/// cannot change its edges, so it also needs no plane.
pub fn scalar_alpha(m: &Mask) -> Option<f32> {
    let [c] = m.components.as_slice() else { return None };
    if !matches!(c.shape, MaskShape::Full) {
        return None;
    }
    let a = if c.op == MaskOp::Intersect || c.invert { 0.0 } else { 1.0 };
    Some((if m.invert { 1.0 - a } else { a }) * mask_scale(m))
}

#[inline]
fn smooth(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The visible masks with components, evaluated in order.
pub fn evaluate(masks: &[Mask], frame: &Frame, w: usize, h: usize, img: &RgbFrame, log_l: &GrayFrame, ev: f32) -> Vec<Evaluated> {
    masks
        .iter()
        .filter(|m| m.visible && !m.components.is_empty())
        .map(|m| Evaluated {
            id: m.id,
            alpha: scalar_alpha(m).map(Alpha::Scalar).unwrap_or_else(|| Alpha::Spatial(evaluate_one(m, frame, w, h, img, log_l, ev))),
            adjust: m.adjust,
        })
        .collect()
}

/// Range colours measured after this photo's tone map, in perceptual OkLab.
#[allow(clippy::too_many_arguments)]
pub fn evaluate_with_tone(
    masks: &[Mask],
    frame: &Frame,
    w: usize,
    h: usize,
    img: &RgbFrame,
    log_l: &GrayFrame,
    ev: f32,
    tone: &crate::ToneMap,
) -> Vec<Evaluated> {
    masks
        .iter()
        .filter(|m| m.visible && !m.components.is_empty())
        .map(|m| Evaluated {
            id: m.id,
            alpha: scalar_alpha(m)
                .map(Alpha::Scalar)
                .unwrap_or_else(|| Alpha::Spatial(evaluate_one_tone(m, frame, w, h, img, log_l, ev, Some(tone)))),
            adjust: m.adjust,
        })
        .collect()
}

/// The alpha plane of mask `m` (whether visible or not): its components combined, inverted and
/// scaled by its amount and opacity ([`mask_scale`]).
pub fn evaluate_one(m: &Mask, frame: &Frame, w: usize, h: usize, img: &RgbFrame, log_l: &GrayFrame, ev: f32) -> GrayFrame {
    evaluate_one_tone(m, frame, w, h, img, log_l, ev, None)
}
#[allow(clippy::too_many_arguments)]
pub fn evaluate_one_tone(
    m: &Mask,
    frame: &Frame,
    w: usize,
    h: usize,
    img: &RgbFrame,
    log_l: &GrayFrame,
    ev: f32,
    tone: Option<&crate::ToneMap>,
) -> GrayFrame {
    if let Some(a) = scalar_alpha(m) {
        return GrayFrame::from_fn(w, h, |_, _| a);
    }
    let mut alpha = GrayFrame::new(w, h);
    let mut first = true;
    for comp in &m.components {
        let mut c = shape_alpha_tone(&comp.shape, frame, w, h, img, log_l, ev, tone);
        if comp.invert {
            c.data.iter_mut().for_each(|v| *v = 1.0 - *v);
        }
        if first && comp.op != MaskOp::Intersect {
            alpha = c;
            first = false;
            continue;
        }
        for (a, c) in alpha.data.iter_mut().zip(&c.data) {
            *a = match comp.op {
                MaskOp::Add => a.max(*c),
                MaskOp::Subtract => *a * (1.0 - c),
                MaskOp::Intersect => *a * c,
            };
        }
        first = false;
    }
    if m.refine > 0.0 {
        // Refine Edges: the mask's edges follow the photo's (window up to ~4 % of the long edge,
        // the width of a soft brush edge or gradient)
        let k = (m.refine / 100.0).clamp(0.0, 1.0) as f32;
        let sigma = refine_sigma(frame, w, k);
        let refined = if uses_subject_matting(m) {
            GrayFrame {
                width: w,
                height: h,
                data: li_matte::refine::subject_alpha(&li_matte::RgbImage::borrowed(w, h, &img.data), &alpha.data, sigma),
            }
        } else {
            guided_cross(log_l, &alpha, sigma, 0.02)
        };
        for (a, r) in alpha.data.iter_mut().zip(&refined.data) {
            *a += (r - *a) * k.sqrt();
        }
    }
    if m.invert {
        alpha.data.iter_mut().for_each(|v| *v = 1.0 - *v);
    }
    let amt = mask_scale(m);
    if (amt - 1.0).abs() > 1e-6 {
        alpha.data.iter_mut().for_each(|v| *v *= amt);
    }
    alpha
}

/// Refine's radius is measured against the uncropped oriented image, like brush
/// and feather sizes. Cropping at the same source pixel scale cannot change it.
pub fn refine_sigma(frame: &Frame, out_w: usize, amount: f32) -> f32 {
    (0.04 * frame.px_per_long(out_w) as f32 * amount).max(1.0)
}

/// Whether this mask requests image-guided subject matting. Used by the GPU planner
/// to route AI boundary work through the same CPU solver as previews/exports.
/// Depth/gradient/brush and mixed-component masks keep their continuous guided semantics.
pub fn uses_subject_matting(m: &Mask) -> bool {
    if m.refine <= 0.0 || !m.refine.is_finite() {
        return false;
    }
    let [c] = m.components.as_slice() else { return false };
    match &c.shape {
        MaskShape::Subject { seg } | MaskShape::Sky { seg } | MaskShape::Background { seg } => seg.is_some(),
        MaskShape::Object { seg, detail, .. } | MaskShape::Prompt { seg, detail, .. } => seg.is_some() || !detail.is_empty(),
        _ => false,
    }
}

/// What a mask's alpha is scaled by: its amount (0..200 %) times its layer opacity (0..100 %).
pub fn mask_scale(m: &Mask) -> f32 {
    // (× exactly 1.0 at full opacity: masks written before layers scale as they always did)
    ((m.adjust.amount / 100.0) * (m.opacity.clamp(0.0, 100.0) / 100.0)) as f32
}

/// Per-pixel positions in long-edge units for output pixel centres.
fn for_each_pos(frame: &Frame, w: usize, h: usize, out: &mut GrayFrame, f: impl Fn(Point, usize) -> f32 + Sync + Send) {
    let m = frame.out_to_norm(w, h);
    let (ow, oh, l) = (frame.ow, frame.oh, frame.ow.max(frame.oh));
    for_rows(&mut out.data, w, |y, row| {
        for (x, v) in row.iter_mut().enumerate() {
            let n = m.apply(Point::new(x as f64 + 0.5, y as f64 + 0.5));
            *v = f(Point::new(n.x * ow / l, n.y * oh / l), y * w + x);
        }
    });
}

pub fn shape_alpha(shape: &MaskShape, frame: &Frame, w: usize, h: usize, img: &RgbFrame, log_l: &GrayFrame, ev: f32) -> GrayFrame {
    shape_alpha_tone(shape, frame, w, h, img, log_l, ev, None)
}
#[allow(clippy::too_many_arguments)]
pub fn shape_alpha_tone(
    shape: &MaskShape,
    frame: &Frame,
    w: usize,
    h: usize,
    img: &RgbFrame,
    log_l: &GrayFrame,
    ev: f32,
    tone: Option<&crate::ToneMap>,
) -> GrayFrame {
    let mut out = GrayFrame::new(w, h);
    let to_long = |p: Point| frame.norm_to_long(p);
    // `img`/`log_l` are before exposure: range masks select on the exposed values.
    let gain = ev.exp2();
    match shape {
        MaskShape::Full => out.data.fill(1.0),
        MaskShape::Linear { start, end } => {
            let (a, b) = (to_long(*start), to_long(*end));
            let d = b - a;
            let len2 = d.dot(d).max(1e-12);
            for_each_pos(frame, w, h, &mut out, |p, _| {
                let t = ((p - a).dot(d) / len2) as f32;
                1.0 - smooth(0.0, 1.0, t)
            });
        }
        MaskShape::Radial { center, rx, ry, angle, feather, invert } => {
            let c = to_long(*center);
            let (s, co) = (-angle.to_radians()).sin_cos();
            let f = (*feather / 100.0).clamp(0.0, 1.0) as f32;
            let (rx, ry) = (rx.max(1e-6), ry.max(1e-6));
            let inv = *invert;
            for_each_pos(frame, w, h, &mut out, |p, _| {
                let d = p - c;
                let (x, y) = (d.x * co - d.y * s, d.x * s + d.y * co);
                let r = ((x / rx).powi(2) + (y / ry).powi(2)).sqrt() as f32;
                let a = 1.0 - smooth(1.0 - f.max(0.01), 1.0, r);
                if inv { 1.0 - a } else { a }
            });
        }
        MaskShape::Brush { strokes } => rasterize_brush(strokes, frame, w, h, img, log_l, &mut out),
        MaskShape::LuminanceRange { lo, hi, lo_feather, hi_feather } => {
            let (lo, hi, lf, hf) = (*lo as f32, *hi as f32, (*lo_feather as f32).max(1e-3), (*hi_feather as f32).max(1e-3));
            for (v, l) in out.data.iter_mut().zip(&log_l.data) {
                // map log luminance to a 0..1 perceptual scale (−8..+4 EV)
                let y = ((l + ev + 8.0) / 12.0).clamp(0.0, 1.0);
                *v = smooth(lo - lf, lo, y) * (1.0 - smooth(hi, hi + hf, y));
            }
        }
        MaskShape::ColorRange { samples, refine } => {
            let tol = 0.04 + 0.16 * (*refine as f32 / 100.0);
            let samples: Vec<[f32; 3]> = samples.iter().map(|s| [s[0] as f32, s[1] as f32, s[2] as f32]).collect();
            for (v, c) in out.data.iter_mut().zip(&img.data) {
                let lab = li_color::perceptual::oklab_from_2020(
                    tone.map_or_else(|| tonemap_for_select(c.map(|v| v * gain)), |t| crate::tone2::tone_px(t, &t.method(), c.map(|v| v * gain))),
                );
                let d = samples
                    .iter()
                    .map(|s| ((lab[1] - s[1]).powi(2) + (lab[2] - s[2]).powi(2) + 0.25 * (lab[0] - s[0]).powi(2)).sqrt())
                    .fold(f32::MAX, f32::min);
                *v = 1.0 - smooth(tol * 0.5, tol, d);
            }
        }
        // local-image: the quick segmentation model's result, when the mask was added with one.
        MaskShape::Subject { seg: Some(seg) } | MaskShape::Sky { seg: Some(seg) } | MaskShape::Background { seg: Some(seg) } => {
            sample_seg(Some(seg), &[], 1.0, frame, w, h, &mut out);
        }
        MaskShape::Sky { .. } => {
            // Classical sky heuristic until the segmenter lands (M12): bright, smooth, blue-ish or
            // unsaturated, and connected to the top of the frame.
            let m = frame.out_to_norm(w, h);
            for (i, v) in out.data.iter_mut().enumerate() {
                let (x, y) = (i % w, i / w);
                let n = m.apply(Point::new(x as f64 + 0.5, y as f64 + 0.5));
                let c = img.data[i].map(|v| v * gain);
                let l = log_l.data[i] + ev;
                let blue = (c[2] - c[0]).max(0.0) / (c[2] + 1e-4);
                let top = 1.0 - smooth(0.25, 0.7, n.y as f32);
                *v = top * smooth(-3.5, -1.0, l) * (0.4 + 0.6 * smooth(0.0, 0.3, blue).max(smooth(0.0, 1.5, l)));
            }
            smooth_plane(&mut out, 0.01 * frame_px(frame, w));
        }
        MaskShape::Object { seg, detail, edge, .. } | MaskShape::Prompt { seg, detail, edge, .. } if seg.is_some() || !detail.is_empty() => {
            // Edge: below 0 a steeper transition (up to 8× the logits), above 0 feathered (up to
            // 2 % of the long edge, so previews and exports match)
            let gain = li_matte::refine::edge_gain(*edge);
            sample_seg(seg.as_ref(), detail, gain, frame, w, h, &mut out);
            if *edge > 0.0 {
                out.data = li_matte::refine::feather_edge(&out.data, w, h, *edge, frame_px(frame, w));
            }
        }
        // not computed yet (no clicks, or a build without the model): nothing selected
        MaskShape::Object { hint, .. } if hint.is_empty() => {}
        MaskShape::Prompt { .. } => {}
        MaskShape::Subject { .. } | MaskShape::Object { .. } | MaskShape::People { .. } => {
            // Saliency heuristic: centre-weighted local contrast (replaced by the segmenter in M12).
            let m = frame.out_to_norm(w, h);
            let blur = crate::blur::gaussian(log_l, 0.03 * frame_px(frame, w));
            for (i, v) in out.data.iter_mut().enumerate() {
                let (x, y) = (i % w, i / w);
                let n = m.apply(Point::new(x as f64 + 0.5, y as f64 + 0.5));
                let d = (((n.x - 0.5) / 0.35).powi(2) + ((n.y - 0.55) / 0.4).powi(2)).sqrt() as f32;
                let contrast = (log_l.data[i] - blur.data[i]).abs();
                *v = (1.0 - smooth(0.6, 1.0, d)) * (0.5 + 0.5 * smooth(0.05, 0.6, contrast));
            }
            smooth_plane(&mut out, 0.015 * frame_px(frame, w));
        }
        MaskShape::Background { .. } => {
            let mut s = shape_alpha(&MaskShape::Subject { seg: None }, frame, w, h, img, log_l, ev);
            s.data.iter_mut().for_each(|v| *v = 1.0 - *v);
            out = s;
        }
        MaskShape::DepthRange { lo, hi, feather, seg } => {
            // the stored distance map, then a feathered band of it
            if seg.is_some() {
                sample_seg(seg.as_ref(), &[], 1.0, frame, w, h, &mut out);
                let (lo, hi) = (lo.min(*hi) as f32, lo.max(*hi) as f32);
                let f = (*feather as f32).max(1e-3);
                for v in &mut out.data {
                    let d = *v;
                    *v = smooth(lo - f, lo, d) * (1.0 - smooth(hi, hi + f, d));
                }
            }
        }
        MaskShape::Landscape { .. } => {}
    }
    out
}

/// A decoded [`SegMask`]: its logits and the normalized rectangle they cover.
struct SegGrid {
    logits: Vec<f32>,
    side: usize,
    r: [f64; 4],
}

impl SegGrid {
    fn new(seg: &SegMask) -> Option<SegGrid> {
        Some(SegGrid { logits: seg.logits()?, side: seg.side as usize, r: seg.bounds()? })
    }

    fn contains(&self, n: Point) -> bool {
        n.x >= self.r[0] && n.x <= self.r[2] && n.y >= self.r[1] && n.y <= self.r[3]
    }

    /// The bilinearly sampled logit at normalized image point `n` (cell centres at
    /// `(i + 0.5) / side` of the rectangle).
    fn logit(&self, n: Point) -> f32 {
        let side = self.side;
        let at = |x: usize, y: usize| self.logits.get(y.min(side - 1) * side + x.min(side - 1)).copied().unwrap_or(-16.0);
        let u = (n.x - self.r[0]) / (self.r[2] - self.r[0]);
        let v = (n.y - self.r[1]) / (self.r[3] - self.r[1]);
        let (fx, fy) = ((u * side as f64 - 0.5) as f32, (v * side as f64 - 0.5) as f32);
        let (fx, fy) = (fx.clamp(0.0, (side - 1) as f32), fy.clamp(0.0, (side - 1) as f32));
        let (x0, y0) = (fx.floor() as usize, fy.floor() as usize);
        let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
        let top = at(x0, y0) * (1.0 - tx) + at(x0 + 1, y0) * tx;
        let bot = at(x0, y0 + 1) * (1.0 - tx) + at(x0 + 1, y0 + 1) * tx;
        top * (1.0 - ty) + bot * ty
    }
}

/// Stored model segmentations at each output pixel: the zoomed-in `detail` passes inside
/// their rectangles (the highest of them where they overlap), the whole-photo `seg`
/// elsewhere; the logit (times `gain`: > 1 is a harder edge) through a sigmoid. Damaged
/// data selects nothing.
fn sample_seg(seg: Option<&SegMask>, detail: &[SegMask], gain: f32, frame: &Frame, w: usize, h: usize, out: &mut GrayFrame) {
    let coarse = seg.and_then(SegGrid::new);
    // (capped: a hostile document can hold many)
    let patches: Vec<SegGrid> = detail.iter().take(lightcraft_develop::segmask::MAX_DETAIL).filter_map(SegGrid::new).collect();
    if coarse.is_none() && patches.is_empty() {
        return;
    }
    let m = frame.out_to_norm(w, h);
    for_rows(&mut out.data, w, |y, row| {
        for (x, v) in row.iter_mut().enumerate() {
            let n = m.apply(Point::new(x as f64 + 0.5, y as f64 + 0.5));
            let fine = patches.iter().filter(|p| p.contains(n)).map(|p| p.logit(n)).reduce(f32::max);
            let l = fine.or_else(|| coarse.as_ref().map(|c| c.logit(n))).unwrap_or(-16.0);
            *v = 1.0 / (1.0 + (-l * gain).exp());
        }
    });
}

fn frame_px(frame: &Frame, w: usize) -> f32 {
    frame.px_per_long(w) as f32
}

fn smooth_plane(p: &mut GrayFrame, sigma: f32) {
    *p = crate::blur::gaussian(p, sigma.max(0.5));
}

/// A rough display mapping for colour picking (so samples taken on screen match).
pub(crate) fn tonemap_for_select(c: [f32; 3]) -> [f32; 3] {
    c.map(|v| v / (1.0 + v))
}

/// A brush stroke resolved to output pixels: dab centres (spacing r/4 along the path), radius,
/// hard-core radius, flow and density (0..1), and whether it erases.
pub struct BrushDabs {
    pub dabs: Vec<Point>,
    pub r: f64,
    pub hard: f64,
    pub flow: f32,
    pub density: f32,
    pub erase: bool,
    /// Auto Mask: paint only pixels like the one under each dab, then snap to edges.
    pub auto: bool,
}

/// The dabs of stroke `s` in a `w × h` output.
pub fn brush_dabs(s: &BrushStroke, frame: &Frame, w: usize, h: usize) -> BrushDabs {
    let to_out = frame.norm_to_out(w, h);
    let ppl = frame.px_per_long(w);
    let r = (s.size * ppl).max(0.5);
    let hard = r * (1.0 - (s.feather / 100.0).clamp(0.0, 1.0));
    // Densify the path so dabs overlap (spacing r/4).
    let mut dabs: Vec<Point> = Vec::new();
    for (i, p) in s.points.iter().enumerate() {
        let q = to_out.apply(*p);
        if i > 0 {
            let prev = to_out.apply(s.points[i - 1]);
            let dist = prev.dist(q);
            let n = (dist / (r / 4.0).max(0.5)).ceil() as usize;
            for k in 1..n {
                dabs.push(prev.lerp(q, k as f64 / n as f64));
            }
        }
        dabs.push(q);
    }
    BrushDabs {
        dabs,
        r,
        hard,
        flow: (s.flow / 100.0).clamp(0.0, 1.0) as f32,
        density: (s.density / 100.0).clamp(0.0, 1.0) as f32,
        erase: s.erase,
        auto: s.auto_mask,
    }
}

/// Auto Mask tolerances: a pixel takes a dab's paint fully up to half of these from the colour
/// under the dab centre, none beyond them (log2 luminance, chromaticity `rgb / Y`).
pub const AUTO_TOL_EV: f32 = 0.5;
pub const AUTO_TOL_CHROMA: f32 = 0.25;

/// Chromaticity `rgb / Y` (as colour noise reduction uses it).
#[inline]
pub fn chromaticity(c: [f32; 3]) -> [f32; 3] {
    let y = li_color::luminance_2020(c).max(1e-6);
    [c[0] / y, c[1] / y, c[2] / y]
}

/// Auto Mask: how much a pixel (log luminance `l`, chromaticity `ch`) is like the reference.
#[inline]
pub fn auto_similarity(l: f32, ch: [f32; 3], rl: f32, rch: [f32; 3]) -> f32 {
    let dl = (l - rl) / AUTO_TOL_EV;
    let dc = ((ch[0] - rch[0]).powi(2) + (ch[1] - rch[1]).powi(2) + (ch[2] - rch[2]).powi(2)).sqrt() / AUTO_TOL_CHROMA;
    1.0 - smooth(0.5, 1.0, (dl * dl + dc * dc).sqrt())
}

/// The output pixel a dab at `d` samples its reference colour from.
#[inline]
pub fn dab_pixel(d: Point, w: usize, h: usize) -> usize {
    let x = (d.x.floor().max(0.0) as usize).min(w - 1);
    let y = (d.y.floor().max(0.0) as usize).min(h - 1);
    y * w + x
}

/// Guided-filter refinement of an Auto Mask stroke of radius `r` px: (sigma px, epsilon EV²). The
/// stroke keeps the larger of its own and the refined alpha.
pub fn auto_refine(r: f64) -> (f32, f32) {
    ((0.25 * r).max(1.0) as f32, 0.02)
}

/// Guided filter of `p` steered by `guide` (He et al.), clamped to 0..1: `p`'s edges snap to the
/// guide's.
pub fn guided_cross(guide: &GrayFrame, p: &GrayFrame, sigma: f32, eps: f32) -> GrayFrame {
    GrayFrame {
        width: p.width,
        height: p.height,
        data: li_matte::matting::guided_filter_gaussian(&guide.data, &p.data, p.width, p.height, sigma, eps),
    }
}

/// Stamp brush strokes into `out` (max-combine; erase strokes subtract). Auto Mask strokes weight
/// each dab by [`auto_similarity`] to the pixel under its centre and are refined with
/// [`guided_cross`] on log luminance.
fn rasterize_brush(strokes: &[BrushStroke], frame: &Frame, w: usize, h: usize, img: &RgbFrame, log_l: &GrayFrame, out: &mut GrayFrame) {
    if w == 0 || h == 0 {
        return;
    }
    for s in strokes {
        let BrushDabs { dabs, r, hard, flow, density: dens, erase, auto } = brush_dabs(s, frame, w, h);
        let mut stroke_alpha = GrayFrame::new(w, h);
        for d in &dabs {
            let refc = auto.then(|| {
                let j = dab_pixel(*d, w, h);
                (log_l.data[j], chromaticity(img.data[j]))
            });
            let (x0, x1) = (((d.x - r).floor().max(0.0)) as usize, ((d.x + r).ceil().max(0.0) as usize).min(w));
            let (y0, y1) = (((d.y - r).floor().max(0.0)) as usize, ((d.y + r).ceil().max(0.0) as usize).min(h));
            for y in y0..y1 {
                for x in x0..x1 {
                    let dd = Point::new(x as f64 + 0.5, y as f64 + 0.5).dist(*d);
                    if dd > r {
                        continue;
                    }
                    let mut a = if dd <= hard { 1.0 } else { 1.0 - smooth(hard as f32, r as f32, dd as f32) };
                    let i = y * w + x;
                    if let Some((rl, rch)) = refc {
                        a *= auto_similarity(log_l.data[i], chromaticity(img.data[i]), rl, rch);
                    }
                    let v = &mut stroke_alpha.data[i];
                    // flow accumulates within a stroke up to density
                    *v = (*v + a * flow * (1.0 - *v)).min(dens);
                }
            }
        }
        if auto {
            let (sigma, eps) = auto_refine(r);
            let q = guided_cross(log_l, &stroke_alpha, sigma, eps);
            // the refinement fills gaps along edges; it never takes paint away
            stroke_alpha = stroke_alpha.zip_map(&q, f32::max);
        }
        for (o, a) in out.data.iter_mut().zip(&stroke_alpha.data) {
            *o = if erase { *o * (1.0 - a) } else { o.max(*a) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lightcraft_develop::{DevelopSettings, MaskComponent};

    fn frame(w: usize, h: usize) -> Frame {
        Frame::new(w, h, &DevelopSettings::default(), true)
    }

    #[test]
    fn ai_color_edge_quality_and_pipeline_timing() {
        for (w, h) in [(96, 64), (384, 256)] {
            let scale = w as f32 / 96.0;
            let truth = GrayFrame::from_fn(w, h, |x, y| {
                let d = ((x as f32 - 48.0 * scale).powi(2) + (y as f32 - 32.0 * scale).powi(2)).sqrt();
                ((23.0 * scale - d) / (6.0 * scale)).clamp(0.0, 1.0)
            });
            // Equal Rec.2020 luminance, different color: grayscale cannot recover this edge.
            let fg = [0.9, 0.2, 0.1];
            let bg = [0.1, 0.2 + 0.8 * 0.2627 / 0.6780, 0.1];
            let img = RgbFrame::from_fn(w, h, |x, y| {
                let a = truth.get(x, y);
                [0, 1, 2].map(|c| a * fg[c] + (1.0 - a) * bg[c])
            });
            let log_l = img.map(crate::local::log_lum);
            let logits: Vec<f32> = (0..w * w)
                .map(|i| {
                    let x = i % w;
                    let y = ((i / w) as f32 * h as f32 / w as f32).floor() as usize;
                    if truth.get(x, y.min(h - 1)) >= 0.5 { 12.0 } else { -12.0 }
                })
                .collect();
            let mask = Mask {
                components: vec![MaskComponent {
                    name: None,
                    op: MaskOp::Add,
                    invert: false,
                    shape: MaskShape::Subject { seg: Some(SegMask::from_logits(w, &logits)) },
                }],
                refine: 100.0,
                ..Default::default()
            };
            let frame = frame(w, h);
            let raw = evaluate_one(&Mask { refine: 0.0, ..mask.clone() }, &frame, w, h, &img, &log_l, 0.0);
            let t = std::time::Instant::now();
            let old_raw = evaluate_one(&Mask { refine: 0.0, ..mask.clone() }, &frame, w, h, &img, &log_l, 0.0);
            let old = guided_cross(&log_l, &old_raw, 0.04 * w as f32, 0.02);
            let old_ms = t.elapsed().as_secs_f64() * 1000.0;
            let t = std::time::Instant::now();
            let after = evaluate_one(&mask, &frame, w, h, &img, &log_l, 0.0);
            let new_ms = t.elapsed().as_secs_f64() * 1000.0;
            let err = |p: &GrayFrame| p.data.iter().zip(&truth.data).map(|(a, b)| (a - b).abs()).sum::<f32>() / (w * h) as f32;
            let (before, after_err) = (err(&old), err(&after));
            println!(
                "app mask path {w}x{h}: old MAE={before:.8}, shared CF MAE={after_err:.8}; old evaluate+guided={old_ms:.3} ms, new evaluate_one={new_ms:.3} ms"
            );
            assert!(after_err < before * 0.6, "{after_err} vs {before}");
            assert_eq!(after.get(0, 0), raw.get(0, 0));
            assert_eq!(after.get(w / 2, h / 2), raw.get(w / 2, h / 2));
            let inverted = evaluate_one(&Mask { invert: true, ..mask }, &frame, w, h, &img, &log_l, 0.0);
            assert!(inverted.data.iter().zip(&after.data).all(|(a, b)| (a + b - 1.0).abs() < 1e-6));
        }
    }

    #[test]
    fn develop_edge_crop_detail_and_legacy_settings() {
        let mut settings = DevelopSettings::default();
        settings.crop.geometry.rect = lightcraft_geom::Rect::from_center(Point::new(0.75, 0.5), 0.5, 1.0);
        let (w, h) = (80, 40);
        let frame = Frame::new(w, h, &settings, true);
        let img = RgbFrame::new(w, h);
        let log_l = GrayFrame::new(w, h);
        let coarse = SegMask::from_logits(8, &[-12.0; 64]);
        let patch = SegMask::from_logits_in(8, &[12.0; 64], [0.5, 0.0, 0.75, 1.0]);
        let shape = MaskShape::Prompt { text: "test".into(), seg: Some(coarse), detail: vec![patch], edge: 100.0 };
        let a = shape_alpha(&shape, &frame, w, h, &img, &log_l, 0.0);
        assert!(a.get(4, 20) > 0.9 && a.get(75, 20) < 0.1);
        for edge in [-77.3f64, -0.004, 0.0, 13.7, 100.0] {
            let e = (edge / 100.0).clamp(-1.0, 1.0) as f32;
            let gain = if e < 0.0 { 1.0 - 7.0 * e } else { 1.0 };
            assert_eq!(li_matte::refine::edge_gain(edge), gain);
        }
        let mut no_edge = shape.clone();
        if let MaskShape::Prompt { edge, .. } = &mut no_edge {
            *edge = 0.0;
        }
        let b = shape_alpha(&no_edge, &frame, w, h, &img, &log_l, 0.0);
        let expected = li_matte::refine::feather_edge(&b.data, w, h, 100.0, frame_px(&frame, w));
        assert_eq!(a.data, expected);
        let old: Mask = serde_json::from_str(r#"{"id":7,"components":[],"refine":30.0}"#).unwrap();
        assert_eq!(old.refine, 30.0);
        assert_eq!(old.opacity, 100.0);
        assert!(!uses_subject_matting(&old));
        let depth = Mask {
            components: vec![MaskComponent {
                name: None,
                op: MaskOp::Add,
                invert: false,
                shape: MaskShape::DepthRange { lo: 0.3, hi: 0.7, feather: 0.1, seg: None },
            }],
            ..Default::default()
        };
        assert!(!uses_subject_matting(&depth));
    }

    #[test]
    fn develop_guided_golden() {
        let (w, h) = (39, 23);
        let mask = GrayFrame::from_fn(w, h, |x, _| if x < 19 { 1.0 } else { 0.0 });
        // Freeze guide words so native log2 implementations cannot change the input.
        // The output was checked bit-for-bit against pre-li-matte Develop on Apple Silicon.
        let bytes = include_bytes!("../tests/fixtures/guided-log-guide.f32le");
        assert_eq!(bytes.len(), w * h * 4);
        let guide = GrayFrame { width: w, height: h, data: bytes.as_chunks::<4>().0.iter().map(|v| f32::from_le_bytes(*v)).collect() };
        let q = guided_cross(&guide, &mask, 2.7, 0.02);
        let hash = q.data.iter().fold(0xcbf29ce484222325u64, |h, v| (h ^ u64::from(v.to_bits())).wrapping_mul(0x100000001b3));
        assert_eq!(hash, 9236099489760829479);
        // Keep the original Linux-native input and pre-migration snapshot as well.
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        {
            let guide = GrayFrame::from_fn(w, h, |x, y| (0.03 + (x * 7 + y * 11) as f32 / 400.0).log2());
            let q = guided_cross(&guide, &mask, 2.7, 0.02);
            let hash = q.data.iter().fold(0xcbf29ce484222325u64, |h, v| (h ^ u64::from(v.to_bits())).wrapping_mul(0x100000001b3));
            assert_eq!(hash, 7702663797854729420);
        }
    }

    #[test]
    fn linear_gradient_ramps() {
        let f = frame(100, 50);
        let img = RgbFrame::new(100, 50);
        let l = GrayFrame::new(100, 50);
        let a = shape_alpha(&MaskShape::Linear { start: Point::new(0.0, 0.0), end: Point::new(1.0, 0.0) }, &f, 100, 50, &img, &l, 0.0);
        assert!(a.get(1, 25) > 0.99 && a.get(98, 25) < 0.01);
        assert!((a.get(50, 25) - 0.5).abs() < 0.05);
    }

    #[test]
    fn stored_segmentations_render_at_any_size_and_follow_the_crop() {
        // the left half selected, on a 16² logit grid
        let side = 16;
        let logits: Vec<f32> = (0..side * side).map(|i| if i % side < side / 2 { 12.0 } else { -12.0 }).collect();
        let seg = SegMask::from_logits(side, &logits);
        for shape in [
            MaskShape::Object { hint: vec![Point::new(0.25, 0.5)], exclude: vec![], seg: Some(seg.clone()), detail: vec![], edge: 0.0 },
            MaskShape::Prompt { text: "left".into(), seg: Some(seg.clone()), detail: vec![], edge: 0.0 },
        ] {
            for (w, h) in [(40, 20), (400, 200)] {
                let a = shape_alpha(&shape, &frame(w, h), w, h, &RgbFrame::new(w, h), &GrayFrame::new(w, h), 0.0);
                assert!(a.get(1, h / 2) > 0.99 && a.get(w - 2, h / 2) < 0.01, "{w}×{h}");
                if w >= 400 {
                    let soft = (0..w).filter(|x| (0.05..0.95).contains(&a.get(*x, h / 2))).count();
                    assert!((2..30).contains(&soft), "an anti-aliased edge, not a ramp: {soft} px");
                }
            }
        }
        // cropped to the right half: nothing left selected
        let mut d = DevelopSettings::default();
        d.crop.geometry.rect = lightcraft_geom::Rect::from_center(Point::new(0.75, 0.5), 0.5, 1.0);
        let f = Frame::new(40, 20, &d, true);
        let shape = MaskShape::Prompt { text: "left".into(), seg: Some(seg), detail: vec![], edge: 0.0 };
        let a = shape_alpha(&shape, &f, 20, 20, &RgbFrame::new(20, 20), &GrayFrame::new(20, 20), 0.0);
        assert!(a.data.iter().all(|v| *v < 0.05));
    }

    #[test]
    fn detail_patches_replace_the_coarse_mask_inside_their_rectangle() {
        // coarse: nothing; a patch over the right half selects its own left half (x 0.5..0.75)
        let coarse = SegMask::from_logits(4, &[-12.0; 16]);
        let side = 16;
        let l: Vec<f32> = (0..side * side).map(|i| if i % side < side / 2 { 12.0 } else { -12.0 }).collect();
        let patch = SegMask::from_logits_in(side, &l, [0.5, 0.0, 1.0, 1.0]);
        let shape = MaskShape::Object { hint: vec![Point::new(0.6, 0.5)], exclude: vec![], seg: Some(coarse), detail: vec![patch], edge: 0.0 };
        let (w, h) = (200, 100);
        let a = shape_alpha(&shape, &frame(w, h), w, h, &RgbFrame::new(w, h), &GrayFrame::new(w, h), 0.0);
        assert!(a.get(20, 50) < 0.01, "outside the patch: the coarse mask");
        assert!(a.get(120, 50) > 0.99, "inside the patch, its selected half");
        assert!(a.get(190, 50) < 0.01, "inside the patch, its unselected half");
    }

    #[test]
    fn a_hostile_number_of_detail_patches_is_capped() {
        // built in memory (a deserialized document is capped already): only the first
        // MAX_DETAIL are decoded and sampled, so this needs 4 × 4 MB, not 20 000 × 4 MB
        let side = 1024;
        let l: Vec<f32> = vec![12.0; side * side];
        let patch = SegMask::from_logits_in(side, &l, [0.0, 0.0, 0.5, 1.0]);
        let shape = MaskShape::Object { hint: vec![Point::new(0.2, 0.5)], exclude: vec![], seg: None, detail: vec![patch; 20_000], edge: 0.0 };
        let (w, h) = (64, 32);
        let t = std::time::Instant::now();
        let a = shape_alpha(&shape, &frame(w, h), w, h, &RgbFrame::new(w, h), &GrayFrame::new(w, h), 0.0);
        assert!(a.get(5, 16) > 0.99 && a.get(60, 16) < 0.01);
        assert!(t.elapsed() < std::time::Duration::from_secs(20), "{:?}", t.elapsed());
    }

    #[test]
    fn the_edge_setting_hardens_or_feathers_the_transition() {
        let side = 16;
        let l: Vec<f32> = (0..side * side).map(|i| ((i % side) as f32 - 7.5) * -0.6).collect(); // a gentle ramp
        let seg = SegMask::from_logits(side, &l);
        let (w, h) = (400, 40);
        let soft_px = |edge: f64| {
            let shape = MaskShape::Prompt { text: "x".into(), seg: Some(seg.clone()), detail: vec![], edge };
            let a = shape_alpha(&shape, &frame(w, h), w, h, &RgbFrame::new(w, h), &GrayFrame::new(w, h), 0.0);
            assert!(a.get(2, h / 2) > 0.9 && a.get(w - 3, h / 2) < 0.1, "still the same selection at {edge}");
            (0..w).filter(|x| (0.1..0.9).contains(&a.get(*x, h / 2))).count()
        };
        let (hard, plain, soft) = (soft_px(-100.0), soft_px(0.0), soft_px(100.0));
        assert!(hard < plain && plain < soft, "transition widths {hard} < {plain} < {soft}");
    }

    #[test]
    fn ai_shapes_without_a_segmentation_select_nothing() {
        let f = frame(30, 30);
        let (img, l) = (RgbFrame::new(30, 30), GrayFrame::new(30, 30));
        for shape in [
            MaskShape::Object { hint: vec![], exclude: vec![], seg: None, detail: vec![], edge: 0.0 },
            MaskShape::Prompt { text: "sky".into(), seg: None, detail: vec![], edge: 0.0 },
            MaskShape::Prompt { text: "sky".into(), seg: Some(SegMask { side: 4, data: "damaged!".into(), rect: None }), detail: vec![], edge: 0.0 },
        ] {
            assert!(shape_alpha(&shape, &f, 30, 30, &img, &l, 0.0).data.iter().all(|v| *v == 0.0));
        }
    }

    #[test]
    fn radial_inside_and_invert() {
        let f = frame(100, 100);
        let img = RgbFrame::new(100, 100);
        let l = GrayFrame::new(100, 100);
        let shape = MaskShape::Radial { center: Point::new(0.5, 0.5), rx: 0.2, ry: 0.2, angle: 0.0, feather: 20.0, invert: false };
        let a = shape_alpha(&shape, &f, 100, 100, &img, &l, 0.0);
        assert!(a.get(50, 50) > 0.99 && a.get(5, 5) < 0.01);
        let m = Mask { components: vec![MaskComponent { name: None, op: MaskOp::Add, invert: true, shape }], ..Default::default() };
        let e = evaluate(&[m], &f, 100, 100, &img, &l, 0.0);
        assert!(e[0].alpha.get(50, 50) < 0.01);
    }

    #[test]
    fn brush_and_subtract() {
        let f = frame(200, 100);
        let img = RgbFrame::new(200, 100);
        let l = GrayFrame::new(200, 100);
        let stroke = BrushStroke { points: vec![Point::new(0.1, 0.5), Point::new(0.9, 0.5)], size: 0.05, ..Default::default() };
        let erase = BrushStroke { points: vec![Point::new(0.5, 0.5)], size: 0.05, erase: true, feather: 0.0, ..Default::default() };
        let m = Mask {
            components: vec![MaskComponent { name: None, op: MaskOp::Add, invert: false, shape: MaskShape::Brush { strokes: vec![stroke, erase] } }],
            ..Default::default()
        };
        let e = evaluate(&[m], &f, 200, 100, &img, &l, 0.0);
        let a = &e[0].alpha;
        assert!(a.get(40, 50) > 0.9, "{}", a.get(40, 50));
        assert!(a.get(100, 50) < 0.05, "erased centre {}", a.get(100, 50));
        assert!(a.get(40, 5) < 0.01);
    }

    #[test]
    fn auto_mask_stops_at_edges() {
        // dark left half, bright right half; a stroke along the boundary, centred on the dark side
        let (w, h) = (200, 100);
        let f = frame(w, h);
        let img = RgbFrame::from_fn(w, h, |x, _| if x < 100 { [0.03; 3] } else { [0.6, 0.5, 0.4] });
        let l = img.map(crate::local::log_lum);
        let stroke = |auto_mask| BrushStroke {
            points: vec![Point::new(0.47, 0.2), Point::new(0.47, 0.8)],
            size: 0.08,
            feather: 0.0,
            auto_mask,
            ..Default::default()
        };
        let comp = |auto| MaskShape::Brush { strokes: vec![stroke(auto)] };
        let plain = shape_alpha(&comp(false), &f, w, h, &img, &l, 0.0);
        let auto = shape_alpha(&comp(true), &f, w, h, &img, &l, 0.0);
        // without Auto Mask the brush spills over the edge; with it, it stays on the dark side
        assert!(plain.get(105, 50) > 0.9, "{}", plain.get(105, 50));
        assert!(auto.get(105, 50) < 0.05, "spill {}", auto.get(105, 50));
        assert!(auto.get(110, 50) < 0.02);
        assert!(auto.get(90, 50) > 0.9, "painted side {}", auto.get(90, 50));
        assert!(auto.get(99, 50) > 0.8, "up to the edge {}", auto.get(99, 50));
        // on a flat area Auto Mask paints like the plain brush
        let flat = RgbFrame::from_fn(w, h, |_, _| [0.2; 3]);
        let fl = flat.map(crate::local::log_lum);
        let a = shape_alpha(&comp(true), &f, w, h, &flat, &fl, 0.0);
        let b = shape_alpha(&comp(false), &f, w, h, &flat, &fl, 0.0);
        assert!((a.get(94, 50) - b.get(94, 50)).abs() < 0.02, "{} vs {}", a.get(94, 50), b.get(94, 50));
    }

    /// Refine Edges: a soft mask over a hard edge in the photo snaps to that edge.
    #[test]
    fn refine_edges_follows_the_photo() {
        let (w, h) = (120usize, 80usize);
        // left half dark, right half bright
        let img = RgbFrame::from_fn(w, h, |x, _| if x < 60 { [0.05; 3] } else { [0.6; 3] });
        let log_l = GrayFrame::from_fn(w, h, |x, _| if x < 60 { (0.05f32).log2() } else { (0.6f32).log2() });
        let f = Frame::new(w, h, &Default::default(), true);
        let soft = Mask {
            components: vec![MaskComponent {
                name: None,
                op: MaskOp::Add,
                invert: false,
                shape: MaskShape::Linear { start: Point::new(0.75, 0.5), end: Point::new(0.25, 0.5) },
            }],
            ..Default::default()
        };
        let refined = Mask { refine: 100.0, ..soft.clone() };
        let step = |m: &Mask| {
            let a = evaluate_one(m, &f, w, h, &img, &log_l, 0.0);
            a.data[40 * w + 63] - a.data[40 * w + 56]
        };
        let (plain, sharp) = (step(&soft), step(&refined));
        assert!(sharp > plain * 1.5 && sharp > 0.1, "the edge in the mask follows the photo's: {plain} → {sharp}");
    }
}

#[cfg(test)]
mod crop_refine_regression {
    use super::*;
    use lightcraft_develop::{DevelopSettings, MaskComponent};
    use lightcraft_geom::Rect;

    #[test]
    fn refine_coverage_is_invariant_under_crop_at_the_same_pixel_scale() {
        let mask = Mask {
            refine: 100.,
            components: vec![MaskComponent {
                name: None,
                op: Default::default(),
                invert: false,
                shape: MaskShape::Linear { start: Point::new(0.499999, 0.5), end: Point::new(0.500001, 0.5) },
            }],
            ..Default::default()
        };
        let full = Frame::new(256, 256, &DevelopSettings::default(), true);
        let mut s = DevelopSettings::default();
        s.crop.geometry.rect = Rect::new(0.25, 0.25, 0.75, 0.75);
        let crop = Frame::new(256, 256, &s, true);
        let a = evaluate_one(&mask, &full, 256, 256, &RgbFrame::filled(256, 256, [0.18; 3]), &GrayFrame::new(256, 256), 0.);
        let b = evaluate_one(&mask, &crop, 128, 128, &RgbFrame::filled(128, 128, [0.18; 3]), &GrayFrame::new(128, 128), 0.);
        assert_eq!(full.px_per_long(256), crop.px_per_long(128));
        for x in 96..160 {
            assert!((a.get(x, 128) - b.get(x - 64, 64)).abs() < 2e-5, "coverage moved at source x={x}");
        }
    }
}
