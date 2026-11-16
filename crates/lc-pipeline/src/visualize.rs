//! Diagnostic views drawn over a finished render ([`Overlay`]), shared by the CPU and GPU renderers
//! (they run on the 8-bit result, after the histogram is taken).
//!
//! - **Point Color range:** the photo is rendered without that sample's own adjustment; pixels
//!   inside the sample's range keep their colour, the rest turn grey (weighted by the range).
//! - **Visualize Spots** (Remove tool): a high-pass of the luminance thresholded to black/white,
//!   so dust spots and specks stand out as white dots. The scale is relative to the image (the
//!   same spots show at any preview size); the threshold slider (0..100) raises sensitivity.
//! - **Mask overlay** (Masking): the evaluated alpha of one mask, drawn as a colour tint, a colour
//!   tint on a black-and-white image, the image on black / white, or the alpha as white on black
//!   ([`MaskView`]). Both renderers hand the same alpha plane (the one the render used) to [`apply`].
//! - **Alt/Option-drag previews** (Lightroom's temporary views while a slider is dragged with the
//!   modifier held; [`Overlay::for_control`] picks one per control): shadow / highlight clipping
//!   (white or black, with the clipped channels shown), greyscale (Sharpening Amount), the detail
//!   band sharpening amplifies (Radius / Detail) and the sharpening mask (Masking). The sharpening
//!   views read each renderer's own sharpening planes.

use std::borrow::Cow;

use li_color::perceptual::{lab_to_lch, oklab_from_2020};
use li_color::transfer::srgb_to_linear;
use li_color::{REC2020, SRGB};
use li_raster::{GrayFrame, Rgba8};
use lightcraft_develop::DevelopSettings;

use crate::colorops::PointK;
use crate::{Plan, for_rows};

/// A diagnostic view of a render.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Overlay {
    #[default]
    None,
    /// Point Color sample `i`: its range in colour, everything else grey.
    PointColorRange(u8),
    /// Skin Tone range: global, or the tools of a layer id.
    SkinToneRange(Option<u32>),
    /// Visualize Spots with a threshold 0..100 (higher = more sensitive).
    Spots(u8),
    /// The evaluated alpha of mask `id` (a [`lightcraft_develop::Mask`] id), drawn as `view` in
    /// `color` at `opacity` (0..100; the colour views only).
    Mask { id: u16, view: MaskView, color: [u8; 3], opacity: u8 },
    /// The tone equalizer's mask: each pixel's luminance zone as a grey level (−8 EV black …
    /// 0 EV white).
    ToneEqMask,
    /// Detail sharpening's Masking (the Alt-drag preview): white where sharpening applies,
    /// black where the mask holds it back.
    SharpenMask,
    /// Shadow clipping (Alt-drag on Blacks / Shadows): white, a channel at 0 drops out of the
    /// colour, so pixels clipped in every channel are black and partly clipped ones are coloured.
    ClipShadows,
    /// Highlight clipping (Alt-drag on Exposure / Highlights / Whites): black, a channel at 255
    /// shows in the colour, so pixels clipped in every channel are white.
    ClipHighlights,
    /// The photo in greyscale (Alt-drag on Sharpening Amount).
    Grayscale,
    /// The detail band sharpening amplifies, as grey ± detail (Alt-drag on Sharpening Radius /
    /// Detail).
    SharpenDetail,
}

/// How [`Overlay::Mask`] draws the mask.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MaskView {
    /// The photo with the mask tinted in the overlay colour.
    #[default]
    Color,
    /// A black-and-white photo with the mask tinted in the overlay colour.
    ColorOnBw,
    /// The photo where the mask is, black elsewhere.
    ImageOnBlack,
    /// The photo where the mask is, white elsewhere.
    ImageOnWhite,
    /// The mask itself: white on black.
    WhiteOnBlack,
}

impl MaskView {
    pub const ALL: [MaskView; 5] = [MaskView::Color, MaskView::ColorOnBw, MaskView::ImageOnBlack, MaskView::ImageOnWhite, MaskView::WhiteOnBlack];

    /// Stable name (control channel, settings).
    pub fn name(self) -> &'static str {
        match self {
            MaskView::Color => "color",
            MaskView::ColorOnBw => "colorOnBw",
            MaskView::ImageOnBlack => "imageOnBlack",
            MaskView::ImageOnWhite => "imageOnWhite",
            MaskView::WhiteOnBlack => "whiteOnBlack",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            MaskView::Color => "Color Overlay",
            MaskView::ColorOnBw => "Color Overlay on B&W",
            MaskView::ImageOnBlack => "Image on Black",
            MaskView::ImageOnWhite => "Image on White",
            MaskView::WhiteOnBlack => "White on Black",
        }
    }

    pub fn parse(s: &str) -> Option<MaskView> {
        MaskView::ALL.into_iter().find(|v| v.name() == s)
    }

    /// The next view (Shift+O cycles them).
    pub fn next(self) -> MaskView {
        let i = MaskView::ALL.iter().position(|v| *v == self).unwrap_or(0);
        MaskView::ALL[(i + 1) % MaskView::ALL.len()]
    }

    fn index(self) -> u64 {
        MaskView::ALL.iter().position(|v| *v == self).unwrap_or(0) as u64
    }
}

/// [`Overlay::Mask`]'s fields in 51 bits (exact in an `f64`): id 16, view 4, colour 24, opacity 7.
fn pack_mask(id: u16, view: MaskView, color: [u8; 3], opacity: u8) -> u64 {
    let rgb = (color[0] as u64) << 16 | (color[1] as u64) << 8 | color[2] as u64;
    id as u64 | view.index() << 16 | rgb << 20 | (opacity.min(100) as u64) << 44
}

fn unpack_mask(v: u64) -> Overlay {
    let rgb = (v >> 20) & 0xff_ffff;
    Overlay::Mask {
        id: (v & 0xffff) as u16,
        view: MaskView::ALL.get(((v >> 16) & 0xf) as usize).copied().unwrap_or_default(),
        color: [(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8],
        opacity: ((v >> 44) & 0x7f).min(100) as u8,
    }
}

impl Overlay {
    /// A plain (kind, value) pair, e.g. for sending a request to a web worker.
    pub fn to_parts(self) -> (u8, f64) {
        match self {
            Overlay::None => (0, 0.0),
            Overlay::PointColorRange(i) => (1, i as f64),
            Overlay::SkinToneRange(id) => (6, id.map_or(0.0, |id| id as f64 + 1.0)),
            Overlay::Spots(t) => (2, t as f64),
            Overlay::Mask { id, view, color, opacity } => (3, pack_mask(id, view, color, opacity) as f64),
            Overlay::ToneEqMask => (4, 0.0),
            Overlay::SharpenMask => (5, 0.0),
            Overlay::ClipShadows => (7, 0.0),
            Overlay::ClipHighlights => (8, 0.0),
            Overlay::Grayscale => (9, 0.0),
            Overlay::SharpenDetail => (10, 0.0),
        }
    }

    /// Inverse of [`Overlay::to_parts`] (unknown kinds: no overlay).
    pub fn from_parts(kind: u8, v: f64) -> Overlay {
        match kind {
            1 => Overlay::PointColorRange(v as u8),
            2 => Overlay::Spots(v.clamp(0.0, 100.0) as u8),
            3 if v.is_finite() && v >= 0.0 => unpack_mask(v as u64),
            4 => Overlay::ToneEqMask,
            5 => Overlay::SharpenMask,
            6 => Overlay::SkinToneRange(if v < 1.0 { None } else { Some((v - 1.0) as u32) }),
            7 => Overlay::ClipShadows,
            8 => Overlay::ClipHighlights,
            9 => Overlay::Grayscale,
            10 => Overlay::SharpenDetail,
            _ => Overlay::None,
        }
    }

    /// A stable value for render-cache keys (0 = no overlay).
    pub fn key(self) -> u64 {
        match self {
            Overlay::None => 0,
            Overlay::PointColorRange(i) => 0x1000 + i as u64,
            Overlay::Spots(t) => 0x2000 + t as u64,
            Overlay::Mask { id, view, color, opacity } => 3 << 60 | pack_mask(id, view, color, opacity),
            Overlay::ToneEqMask => 0x4000,
            Overlay::SharpenMask => 0x5000,
            Overlay::SkinToneRange(id) => 6 << 60 | id.map_or(0, |id| id as u64 + 1),
            Overlay::ClipShadows => 0x5001,
            Overlay::ClipHighlights => 0x5002,
            Overlay::Grayscale => 0x5003,
            Overlay::SharpenDetail => 0x5004,
        }
    }

    /// The temporary preview Lightroom shows while develop control `id` is dragged with Alt
    /// (Option on macOS) held, if it has one.
    pub fn for_control(id: &str) -> Option<Overlay> {
        match id {
            "light.exposure" | "light.highlights" | "light.whites" => Some(Overlay::ClipHighlights),
            "light.shadows" | "light.blacks" => Some(Overlay::ClipShadows),
            "detail.sharpenAmount" => Some(Overlay::Grayscale),
            "detail.sharpenRadius" | "detail.sharpenDetail" => Some(Overlay::SharpenDetail),
            "detail.sharpenMasking" => Some(Overlay::SharpenMask),
            _ => None,
        }
    }

    /// Whether the overlay draws from the render's sharpening planes.
    pub fn reads_detail(self) -> bool {
        matches!(self, Overlay::SharpenDetail | Overlay::SharpenMask)
    }

    /// The Skin Tone tool this overlay targets.
    pub fn skin(self, s: &DevelopSettings) -> Option<&lightcraft_develop::SkinTone> {
        match self {
            Overlay::SkinToneRange(None) => Some(&s.skin_tone),
            Overlay::SkinToneRange(Some(id)) => s.masks.iter().find(|m| m.id == id)?.tools.skin_tone.as_ref(),
            _ => None,
        }
    }

    /// The mask an [`Overlay::Mask`] shows (if it exists in `s` and has components).
    pub fn mask(self, s: &DevelopSettings) -> Option<&lightcraft_develop::Mask> {
        match self {
            Overlay::Mask { id, .. } => s.masks.iter().find(|m| m.id == id as u32 && !m.components.is_empty()),
            _ => None,
        }
    }
}

/// Settings changes an overlay needs before rendering (e.g. the visualized sample's adjustment
/// is left out, so the range is shown on the colours it selects).
pub fn adjust_settings(o: Overlay, s: &mut Cow<'_, DevelopSettings>) {
    if let Overlay::PointColorRange(i) = o
        && let Some(p) = s.point_colors.get(i as usize)
        && !p.is_neutral()
    {
        let p = &mut s.to_mut().point_colors[i as usize];
        (p.hue_shift, p.sat_shift, p.lum_shift, p.variance) = (0.0, 0.0, 0.0, 0.0);
    }
    if let Overlay::SkinToneRange(id) = o
        && o.skin(s).is_some_and(|skin| skin.uniformity != 0.0 || skin.lightness != 0.0)
    {
        let settings = s.to_mut();
        let skin = match id {
            None => Some(&mut settings.skin_tone),
            Some(id) => settings.masks.iter_mut().find(|m| m.id == id).and_then(|m| m.tools.skin_tone.as_mut()),
        };
        if let Some(skin) = skin {
            skin.uniformity = 0.0;
            skin.lightness = 0.0;
        }
    }
    // the sharpening views show even at Amount 0 (their planes are made only while sharpening)
    if o.reads_detail() && s.detail.sharpen_amount == 0.0 {
        s.to_mut().detail.sharpen_amount = 1.0;
    }
}

/// Primary tools have already run, but a range diagnostic still needs its selection reference.
pub fn remaining_for_overlay(o: Overlay, s: &DevelopSettings) -> DevelopSettings {
    let mut remaining = crate::primary::remaining(s);
    if let Some(skin) = o.skin(s).copied() {
        match o {
            Overlay::SkinToneRange(None) => remaining.skin_tone = skin,
            Overlay::SkinToneRange(Some(id)) => {
                if let Some(m) = remaining.masks.iter_mut().find(|m| m.id == id) {
                    m.tools.skin_tone = Some(skin);
                }
            }
            _ => {}
        }
    }
    remaining
}

/// Skin coverage on exposed scene-linear values, matching the picker's UCS22 reference.
pub fn skin_alpha(img: &li_raster::RgbFrame, skin: &lightcraft_develop::SkinTone, gain: f32) -> GrayFrame {
    let lw = crate::ucs::y_to_l_star(1.0);
    img.map(|rgb| {
        let xyz = crate::ucs::mul(&crate::ucs::mats().rgb_to_xyz, rgb.map(|v| v * gain));
        crate::primary::skin_weight(crate::ucs::xyy_to_jch(crate::ucs::xyz_to_xyy(xyz), lw), skin)
    })
}

/// [`Overlay::SharpenDetail`]: encoded grey per EV of detail (±0.4 EV spans black to white).
const DETAIL_GAIN: f32 = 320.0;

/// Draw overlay `o` over `img` (rendered with `plan`). `mask` is the plane the overlay shows, at
/// the image's size: the evaluated alpha of the mask an [`Overlay::Mask`] shows (see
/// [`Overlay::mask`]), or a grey-level or detail plane.
pub fn apply(img: &mut Rgba8, o: Overlay, plan: &Plan<'_>, mask: Option<&GrayFrame>) {
    match o {
        Overlay::None => {}
        Overlay::PointColorRange(i) => {
            if let Some(p) = plan.settings.point_colors.get(i as usize) {
                point_color_range(img, &PointK::new(p));
            }
        }
        Overlay::SkinToneRange(_) => {
            if let Some(a) = mask {
                selected_range(img, a);
            }
        }
        Overlay::Spots(t) => spots(img, t, plan.px_per_long),
        Overlay::Mask { view, color, opacity, .. } => {
            if let Some(a) = o.mask(&plan.settings).and_then(crate::masks::scalar_alpha) {
                mask_view_with(img, view, color, opacity, |_| a);
            } else if let Some(a) = mask.filter(|a| (a.width, a.height) == (img.width, img.height)) {
                mask_view(img, a, view, color, opacity);
            }
        }
        // `mask` holds the zones (see `crate::toneeq::preview_grey`) or the sharpening mask as
        // grey levels
        Overlay::ToneEqMask | Overlay::SharpenMask => {
            if let Some(a) = mask.filter(|a| (a.width, a.height) == (img.width, img.height)) {
                let w = img.width;
                for_rows(&mut img.data, w, |y, row| {
                    for (x, px) in row.iter_mut().enumerate() {
                        let v = (a.data[y * w + x].clamp(0.0, 1.0) * 255.0).round() as u8;
                        *px = [v, v, v, 255];
                    }
                });
            }
        }
        Overlay::ClipShadows => clipping(img, 0),
        Overlay::ClipHighlights => clipping(img, 255),
        Overlay::Grayscale => grayscale(img),
        // `mask` holds the detail band (EV, see `crate::detail::sharp_detail_plane`)
        Overlay::SharpenDetail => {
            if let Some(a) = mask.filter(|a| (a.width, a.height) == (img.width, img.height)) {
                let w = img.width;
                for_rows(&mut img.data, w, |y, row| {
                    for (x, px) in row.iter_mut().enumerate() {
                        let v = (128.0 + a.data[y * w + x] * DETAIL_GAIN).round().clamp(0.0, 255.0) as u8;
                        *px = [v, v, v, 255];
                    }
                });
            }
        }
    }
}

/// [`Overlay::ClipHighlights`] (`clipped` 255) or [`Overlay::ClipShadows`] (`clipped` 0): each
/// channel shows only whether it sits at `clipped`, on black (highlights) or white (shadows).
fn clipping(img: &mut Rgba8, clipped: u8) {
    let w = img.width;
    for_rows(&mut img.data, w, |_, row| {
        for px in row.iter_mut() {
            for c in &mut px[..3] {
                *c = if *c == clipped { clipped } else { 255 - clipped };
            }
            px[3] = 255;
        }
    });
}

fn grayscale(img: &mut Rgba8) {
    let w = img.width;
    for_rows(&mut img.data, w, |_, row| {
        for px in row.iter_mut() {
            let g = li_color::luminance_709([px[0] as f32, px[1] as f32, px[2] as f32]).round().clamp(0.0, 255.0) as u8;
            *px = [g, g, g, 255];
        }
    });
}

/// Draw mask alpha `a` over `img` as `view`.
pub fn mask_view(img: &mut Rgba8, a: &GrayFrame, view: MaskView, color: [u8; 3], opacity: u8) {
    mask_view_with(img, view, color, opacity, |i| a.data[i]);
}
// Scalar Full overlays use the same drawing without materialising an alpha plane.
fn mask_view_with(img: &mut Rgba8, view: MaskView, color: [u8; 3], opacity: u8, alpha: impl Fn(usize) -> f32 + Sync + Send) {
    let op = opacity.min(100) as f32 / 100.0;
    let col = color.map(|c| c as f32);
    let w = img.width;
    for_rows(&mut img.data, w, |y, row| {
        for (x, px) in row.iter_mut().enumerate() {
            let a = alpha(y * w + x).clamp(0.0, 1.0);
            let p = [px[0] as f32, px[1] as f32, px[2] as f32];
            let grey = li_color::luminance_709(p);
            let out: [f32; 3] = match view {
                MaskView::Color => std::array::from_fn(|c| p[c] + (col[c] - p[c]) * a * op),
                MaskView::ColorOnBw => std::array::from_fn(|c| grey + (col[c] - grey) * a * op),
                MaskView::ImageOnBlack => p.map(|v| v * a),
                MaskView::ImageOnWhite => p.map(|v| v * a + 255.0 * (1.0 - a)),
                MaskView::WhiteOnBlack => [255.0 * a; 3],
            };
            let o = out.map(|v| v.round().clamp(0.0, 255.0) as u8);
            *px = [o[0], o[1], o[2], 255];
        }
    });
}

/// Blur radius (Gaussian sigma) of the spot view, as a fraction of the long edge.
const SPOT_SIGMA: f64 = 0.006;

/// High-pass threshold (encoded luminance) for slider value `t` (0..100): 100 shows the faintest
/// specks, 0 only strong ones.
pub fn spot_threshold(t: u8) -> f32 {
    let k = 1.0 - t.min(100) as f32 / 100.0;
    0.01 + 0.2 * k * k
}

fn spots(img: &mut Rgba8, t: u8, ppl: f64) {
    let (w, h) = (img.width, img.height);
    if w == 0 || h == 0 {
        return;
    }
    let l = li_raster::GrayFrame::from_fn(w, h, |x, y| {
        let p = img.data[y * w + x];
        (li_color::luminance_709([p[0] as f32, p[1] as f32, p[2] as f32])) / 255.0
    });
    let b = l.gaussian((SPOT_SIGMA * ppl).max(0.8) as f32);
    let thr = spot_threshold(t);
    for (i, px) in img.data.iter_mut().enumerate() {
        let v = if (l.data[i] - b.data[i]).abs() > thr { 255 } else { 0 };
        *px = [v, v, v, 255];
    }
}

/// Keep selected pixels in colour, fade the rest to grey using the tool's feathered window.
fn selected_range(img: &mut Rgba8, alpha: &GrayFrame) {
    if (img.width, img.height) != (alpha.width, alpha.height) {
        return;
    }
    let w = img.width;
    for_rows(&mut img.data, w, |y, row| {
        for (x, px) in row.iter_mut().enumerate() {
            let a = alpha.data[y * w + x];
            let grey = li_color::luminance_709([px[0] as f32, px[1] as f32, px[2] as f32]);
            for value in &mut px[..3] {
                *value = (grey + (*value as f32 - grey) * a).round().clamp(0.0, 255.0) as u8;
            }
        }
    });
}

fn point_color_range(img: &mut Rgba8, k: &PointK) {
    let m = SRGB.to_space(&REC2020).to_f32();
    let lut: Vec<f32> = (0..256).map(|v| srgb_to_linear(v as f32 / 255.0)).collect();
    let w = img.width;
    for_rows(&mut img.data, w, |_, row| {
        for px in row.iter_mut() {
            let lin = [lut[px[0] as usize], lut[px[1] as usize], lut[px[2] as usize]];
            let c = std::array::from_fn(|r| m[r][0] * lin[0] + m[r][1] * lin[1] + m[r][2] * lin[2]);
            let [l, ch, h] = lab_to_lch(oklab_from_2020(c));
            let a = k.weight(l, ch, h);
            let grey = li_color::luminance_709([px[0] as f32, px[1] as f32, px[2] as f32]);
            for c in 0..3 {
                px[c] = (grey + (px[c] as f32 - grey) * a).round().clamp(0.0, 255.0) as u8;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RenderRequest, SourceInfo, render};
    use li_raster::RgbFrame;
    use lightcraft_develop::PointColor;

    #[test]
    fn spots_view_marks_specks_at_any_size() {
        // a smooth gradient with two small dark specks
        let src = RgbFrame::from_fn(400, 300, |x, y| {
            let d = |cx: f32, cy: f32| ((x as f32 - cx).powi(2) + (y as f32 - cy).powi(2)).sqrt();
            let base = 0.1 + 0.3 * x as f32 / 400.0;
            if d(100.0, 100.0) < 2.5 || d(300.0, 200.0) < 2.5 { [base * 0.5; 3] } else { [base; 3] }
        });
        let info = SourceInfo::default();
        let s = DevelopSettings::default();
        for size in [400, 200] {
            let req = RenderRequest { overlay: Overlay::Spots(50), ..RenderRequest::fit(size, size) };
            let v = render(&src, &info, &s, &req).image;
            let at = |x: f64, y: f64| v.data[(y * v.height as f64) as usize * v.width + (x * v.width as f64) as usize];
            assert_eq!(at(0.25, 1.0 / 3.0), [255, 255, 255, 255], "speck at size {size}");
            assert_eq!(at(0.75, 2.0 / 3.0), [255, 255, 255, 255]);
            assert_eq!(at(0.5, 0.5), [0, 0, 0, 255], "smooth gradient is black");
            let white = v.data.iter().filter(|p| p[0] == 255).count() as f64 / v.data.len() as f64;
            assert!(white < 0.01, "{white}");
        }
        assert!(spot_threshold(0) > spot_threshold(50) && spot_threshold(50) > spot_threshold(100));
        let (k, v) = Overlay::Spots(37).to_parts();
        assert_eq!(Overlay::from_parts(k, v), Overlay::Spots(37));
    }

    #[test]
    fn point_color_range_keeps_the_selected_colour_only() {
        // left half orange, right half blue
        let src = RgbFrame::from_fn(32, 16, |x, _| if x < 16 { [0.4, 0.15, 0.04] } else { [0.03, 0.06, 0.35] });
        let info = SourceInfo::default();
        let plain = render(&src, &info, &DevelopSettings::default(), &RenderRequest::fit(32, 16)).image;
        // sample the orange as it renders
        let px = plain.data[8 * 32 + 4];
        let lin = [px[0], px[1], px[2]].map(|v| srgb_to_linear(v as f32 / 255.0));
        let c = SRGB.to_space(&REC2020).apply_f32(lin);
        let [l, ch, h] = lab_to_lch(oklab_from_2020(c));
        let mut s = DevelopSettings::default();
        s.point_colors.push(PointColor { lum: l as f64, chroma: ch as f64, hue: (h as f64).to_degrees(), hue_shift: 80.0, ..Default::default() });
        let req = RenderRequest { overlay: Overlay::PointColorRange(0), ..RenderRequest::fit(32, 16) };
        let v = render(&src, &info, &s, &req).image;
        let (o, b) = (v.data[8 * 32 + 4], v.data[8 * 32 + 28]);
        // the orange stays orange (and unshifted: the overlay shows the selection, not the edit)
        assert!(o[0].abs_diff(px[0]) <= 2 && o[2].abs_diff(px[2]) <= 2, "{o:?} vs {px:?}");
        // the blue turns grey
        assert!(b[0].abs_diff(b[2]) <= 1, "{b:?}");
        // without the overlay the edit shows
        let e = render(&src, &info, &s, &RenderRequest::fit(32, 16)).image.data[8 * 32 + 4];
        assert!(e[1].abs_diff(px[1]) > 10, "{e:?} vs {px:?}");
    }

    #[test]
    fn mask_overlay_shows_the_evaluated_alpha() {
        use lightcraft_develop::{Mask, MaskComponent, MaskOp, MaskShape};
        use lightcraft_geom::Point;
        let src = RgbFrame::from_fn(80, 40, |x, _| if x < 40 { [0.18, 0.18, 0.18] } else { [0.05, 0.1, 0.3] });
        let info = SourceInfo::default();
        let shape = MaskShape::Radial { center: Point::new(0.25, 0.5), rx: 0.15, ry: 0.15, angle: 0.0, feather: 10.0, invert: false };
        let mut s = DevelopSettings::default();
        s.masks.push(Mask { id: 7, components: vec![MaskComponent { name: None, op: MaskOp::Add, invert: false, shape }], ..Default::default() });
        let plain = render(&src, &info, &s, &RenderRequest::fit(80, 40)).image;
        let shot = |s: &DevelopSettings, view: MaskView| {
            let o = Overlay::Mask { id: 7, view, color: [255, 0, 0], opacity: 100 };
            render(&src, &info, s, &RenderRequest { overlay: o, ..RenderRequest::fit(80, 40) }).image
        };
        let (inside, outside) = (20 * 80 + 20, 20 * 80 + 70);
        let c = shot(&s, MaskView::Color);
        assert_eq!(c.data[inside], [255, 0, 0, 255], "full tint inside the mask");
        assert_eq!(c.data[outside], plain.data[outside], "untouched outside");
        let bw = shot(&s, MaskView::WhiteOnBlack);
        assert_eq!((bw.data[inside], bw.data[outside]), ([255; 4], [0, 0, 0, 255]));
        let ib = shot(&s, MaskView::ImageOnBlack);
        assert_eq!((ib.data[inside], ib.data[outside]), (plain.data[inside], [0, 0, 0, 255]));
        let iw = shot(&s, MaskView::ImageOnWhite);
        assert_eq!(iw.data[outside], [255; 4]);
        let g = shot(&s, MaskView::ColorOnBw).data[outside];
        assert!(g[0] == g[1] && g[1] == g[2], "grey outside: {g:?}");
        // a hidden mask still shows (it's the one being edited)
        s.masks[0].visible = false;
        assert_eq!(shot(&s, MaskView::WhiteOnBlack).data[inside], [255; 4]);
        // an unknown mask draws nothing
        let o = Overlay::Mask { id: 9, view: MaskView::WhiteOnBlack, color: [0; 3], opacity: 50 };
        assert_eq!(render(&src, &info, &s, &RenderRequest { overlay: o, ..RenderRequest::fit(80, 40) }).image, plain);
        // parts round trip (web worker requests) and distinct cache keys
        let o = Overlay::Mask { id: 513, view: MaskView::ImageOnWhite, color: [12, 200, 255], opacity: 65 };
        let (k, v) = o.to_parts();
        assert_eq!(Overlay::from_parts(k, v), o);
        assert_ne!(o.key(), Overlay::Mask { id: 513, view: MaskView::Color, color: [12, 200, 255], opacity: 65 }.key());
        assert_eq!(MaskView::WhiteOnBlack.next(), MaskView::Color);
        assert_eq!(MaskView::parse("colorOnBw"), Some(MaskView::ColorOnBw));
    }

    #[test]
    fn clipping_views_show_the_clipped_channels() {
        let mut img = Rgba8 {
            width: 5,
            height: 1,
            data: vec![[0, 0, 0, 255], [255, 255, 255, 255], [255, 40, 0, 255], [0, 128, 200, 255], [120, 130, 140, 255]],
        };
        let mut hi = img.clone();
        clipping(&mut hi, 255);
        // black background, white where every channel clips, the clipped channels' colour otherwise
        assert_eq!(hi.data, vec![[0, 0, 0, 255], [255; 4], [255, 0, 0, 255], [0, 0, 0, 255], [0, 0, 0, 255]]);
        clipping(&mut img, 0);
        // white background, black where every channel clips, the unclipped channels' colour otherwise
        assert_eq!(img.data, vec![[0, 0, 0, 255], [255; 4], [255, 255, 0, 255], [0, 255, 255, 255], [255; 4]]);
    }

    #[test]
    fn clipping_previews_follow_the_rendered_clipping() {
        // left: deep shadow, middle: grey, right: bright highlight
        let src = RgbFrame::from_fn(60, 20, |x, _| match x / 20 {
            0 => [0.0; 3],
            1 => [0.18; 3],
            _ => [3.0; 3],
        });
        let info = SourceInfo::default();
        let mut s = DevelopSettings::default();
        s.light.blacks = -100.0;
        s.light.whites = 100.0;
        let req = |o| RenderRequest { overlay: o, ..RenderRequest::fit(60, 20) };
        let plain = render(&src, &info, &s, &req(Overlay::None));
        let hi = render(&src, &info, &s, &req(Overlay::ClipHighlights));
        let lo = render(&src, &info, &s, &req(Overlay::ClipShadows));
        let at = |i: &Rgba8, x: usize| i.data[10 * 60 + x];
        assert_eq!(at(&plain.image, 55), [255; 4], "the highlight clips");
        assert_eq!(at(&plain.image, 5), [0, 0, 0, 255], "the shadow clips");
        assert_eq!((at(&hi.image, 55), at(&hi.image, 30), at(&hi.image, 5)), ([255; 4], [0, 0, 0, 255], [0, 0, 0, 255]));
        assert_eq!((at(&lo.image, 5), at(&lo.image, 30), at(&lo.image, 55)), ([0, 0, 0, 255], [255; 4], [255; 4]));
        // a preview only: the histogram is the photo's
        assert_eq!(hi.histogram, plain.histogram);
        assert_eq!(render(&src, &info, &s, &req(Overlay::None)).image, plain.image);
    }

    #[test]
    fn sharpening_previews_show_the_mask_and_detail() {
        // a smooth ramp (left) and a hard edge (at x = 60), 2.4 EV: past Masking 100's 1.6 EV
        let src = RgbFrame::from_fn(80, 40, |x, _| if x < 60 { [0.05 + 0.002 * x as f32; 3] } else { [0.9; 3] });
        let info = SourceInfo::default();
        let mut s = DevelopSettings::default();
        s.detail.sharpen_amount = 60.0;
        let shot = |s: &DevelopSettings, o| render(&src, &info, s, &RenderRequest { overlay: o, ..RenderRequest::fit(80, 40) }).image;
        let (flat, edge) = (20 * 80 + 20, 20 * 80 + 60);
        // Masking 0 sharpens everything: an all-white mask
        let m = shot(&s, Overlay::SharpenMask);
        assert!(m.data.iter().all(|p| *p == [255; 4]));
        // Masking 100 keeps the edge, protects the ramp
        s.detail.sharpen_masking = 100.0;
        let m = shot(&s, Overlay::SharpenMask);
        assert_eq!((m.data[edge], m.data[flat]), ([255; 4], [0, 0, 0, 255]));
        // the same without sharpening on (the preview turns the planes on)
        s.detail.sharpen_amount = 0.0;
        assert_eq!(shot(&s, Overlay::SharpenMask), m);
        // detail: mid grey on the ramp, light / dark either side of the edge
        let d = shot(&s, Overlay::SharpenDetail);
        assert!(d.data[flat][0].abs_diff(128) <= 3, "{:?}", d.data[flat]);
        assert!(d.data[edge][0] > 200 && d.data[edge - 1][0] < 60, "{:?} {:?}", d.data[edge], d.data[edge - 1]);
        // greyscale
        let src = RgbFrame::from_fn(8, 8, |_, _| [0.4, 0.1, 0.05]);
        let g = render(&src, &info, &s, &RenderRequest { overlay: Overlay::Grayscale, ..RenderRequest::fit(8, 8) }).image;
        assert!(g.data.iter().all(|p| p[0] == p[1] && p[1] == p[2] && p[0] > 0));
    }

    #[test]
    fn alt_previews_map_to_controls_and_round_trip() {
        assert_eq!(Overlay::for_control("light.blacks"), Some(Overlay::ClipShadows));
        assert_eq!(Overlay::for_control("light.shadows"), Some(Overlay::ClipShadows));
        assert_eq!(Overlay::for_control("light.whites"), Some(Overlay::ClipHighlights));
        assert_eq!(Overlay::for_control("light.exposure"), Some(Overlay::ClipHighlights));
        assert_eq!(Overlay::for_control("light.highlights"), Some(Overlay::ClipHighlights));
        assert_eq!(Overlay::for_control("detail.sharpenAmount"), Some(Overlay::Grayscale));
        assert_eq!(Overlay::for_control("detail.sharpenDetail"), Some(Overlay::SharpenDetail));
        assert_eq!(Overlay::for_control("detail.sharpenMasking"), Some(Overlay::SharpenMask));
        assert_eq!(Overlay::for_control("light.contrast"), None);
        let all = [Overlay::ClipShadows, Overlay::ClipHighlights, Overlay::Grayscale, Overlay::SharpenDetail, Overlay::SharpenMask];
        for o in all {
            let (k, v) = o.to_parts();
            assert_eq!(Overlay::from_parts(k, v), o);
            assert_eq!(all.iter().filter(|p| p.key() == o.key()).count(), 1);
            assert_ne!(o.key(), Overlay::ToneEqMask.key());
            assert_ne!(o.key(), Overlay::SkinToneRange(None).key());
        }
    }
}

#[cfg(test)]
#[test]
fn develop_c_skin_range_uses_scene_reference_and_preserves_targeted_layer() {
    use crate::{RenderRequest, SourceInfo, render};
    use li_raster::RgbFrame;
    use lightcraft_develop::{LayerTools, Mask, MaskComponent, MaskShape, SkinTone};
    use lightcraft_geom::Point;
    let src = RgbFrame::from_fn(40, 20, |x, _| if x < 20 { [0.3, 0.12, 0.05] } else { [0.03, 0.07, 0.3] });
    let info = SourceInfo::default();
    let req = RenderRequest::fit(40, 20);
    let reference = crate::skin_reference_sample(&src, &info, &DevelopSettings::default(), &req, Point::new(0.2, 0.5)).unwrap();
    let mut s = DevelopSettings {
        skin_tone: SkinTone { reference: Some(reference), uniformity: 80.0, lightness: 30.0, ..Default::default() },
        ..Default::default()
    };
    let plain = render(&src, &info, &DevelopSettings::default(), &req).image;
    let overlay = Overlay::SkinToneRange(None);
    let view = render(&src, &info, &s, &RenderRequest { overlay, ..req.clone() }).image;
    assert_eq!(view.data[10 * 40 + 5], plain.data[10 * 40 + 5], "selected skin stays in colour without its own edit");
    let blue = view.data[10 * 40 + 35];
    assert_eq!(blue[0], blue[1]);
    assert_eq!(blue[1], blue[2]);
    let mask = Mask {
        id: 9,
        components: vec![MaskComponent { name: None, op: Default::default(), invert: false, shape: MaskShape::Full }],
        tools: LayerTools { skin_tone: Some(s.skin_tone), ..Default::default() },
        ..Default::default()
    };
    s.skin_tone = Default::default();
    s.masks.push(mask);
    assert_eq!(render(&src, &info, &s, &RenderRequest { overlay: Overlay::SkinToneRange(Some(9)), ..req }).image, view);
    for overlay in [Overlay::SkinToneRange(None), Overlay::SkinToneRange(Some(0)), Overlay::SkinToneRange(Some(u32::MAX))] {
        let (kind, value) = overlay.to_parts();
        assert_eq!(Overlay::from_parts(kind, value), overlay);
    }
    assert_ne!(Overlay::SkinToneRange(None).key(), Overlay::SkinToneRange(Some(0)).key());
}
