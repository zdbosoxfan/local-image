//! Shared tail of every decoder: interpret device samples (with ICC / container hints), linearize,
//! split alpha, and optionally downscale in linear light.

use crate::icc::{self, IccColorModel, IccKind};
use crate::space::{NamedSpace, SourceSpace, SpaceOrigin, Trc};
use crate::{DecodeOptions, Decoded, Error, Format, Result};
use li_raster::resample::Filter;
use li_raster::{GrayFrame, RgbFrame, par_rows};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Model {
    Gray,
    Rgb,
    /// Subtractive CMYK, 0 = no ink.
    Cmyk,
}

impl Model {
    pub fn channels(self) -> usize {
        match self {
            Model::Gray => 1,
            Model::Rgb => 3,
            Model::Cmyk => 4,
        }
    }
}

pub(crate) enum Buf {
    U8(Vec<u8>),
    U16(Vec<u16>),
    F32(Vec<f32>),
}

impl Buf {
    fn len(&self) -> usize {
        match self {
            Buf::U8(v) => v.len(),
            Buf::U16(v) => v.len(),
            Buf::F32(v) => v.len(),
        }
    }
    #[inline]
    fn norm(&self, i: usize) -> f32 {
        match self {
            Buf::U8(v) => v[i] as f32 / 255.0,
            Buf::U16(v) => v[i] as f32 / 65535.0,
            Buf::F32(v) => v[i],
        }
    }
}

/// Interleaved device samples as produced by a format decoder.
pub(crate) struct Raw {
    pub width: usize,
    pub height: usize,
    pub model: Model,
    /// One extra (last) channel of alpha.
    pub alpha: bool,
    /// Alpha is premultiplied into the colour channels.
    pub premultiplied: bool,
    pub buf: Buf,
    pub bit_depth: u8,
}

impl Raw {
    fn associated_integer(&self) -> bool {
        self.alpha && self.premultiplied && !matches!(self.buf, Buf::F32(_))
    }

    /// Integers store associated device codes, not associated linear-light RGB.
    /// Keep the division fractional; quantizing back to an integer loses precision.
    fn device_sample(&self, pixel: usize, channel: usize, channels: usize) -> f32 {
        let value = self.buf.norm(pixel * channels + channel);
        if self.associated_integer() {
            let alpha = self.buf.norm(pixel * channels + channels - 1);
            if alpha > 0.0 { value / alpha } else { 0.0 }
        } else {
            value
        }
    }
}

/// Metadata and colour hints gathered from the container.
#[derive(Default)]
pub(crate) struct Meta {
    pub icc: Option<Vec<u8>>,
    pub exif: Option<Vec<u8>>,
    pub xmp: Option<String>,
    /// Orientation from the container itself (e.g. TIFF tag); EXIF blob is consulted otherwise.
    pub orientation: Option<u16>,
    /// Container-level colour description (PNG cHRM/gAMA/sRGB/cICP, JXL enum encoding, …).
    pub hint: Option<SourceSpace>,
    /// `image` already went through a CMS/transform producing this space (skip interpretation).
    pub already_linear: Option<SourceSpace>,
}

pub(crate) fn shared_limits(opts: &DecodeOptions) -> photocraft_codecs::Limits {
    photocraft_codecs::Limits {
        max_pixels: opts.max_pixels,
        max_width: u32::MAX,
        max_height: u32::MAX,
        // Preserve explicit source-pixel overrides, including large 16-bit captures,
        // while sharing the desktop photo decoder's fixed allocation ceiling.
        max_alloc: opts.max_pixels.saturating_mul(32).clamp(photocraft_codecs::Limits::default().max_alloc, photocraft_codecs::DESKTOP_PHOTO_BYTES),
    }
}

pub(crate) fn shared_error(format: Format, bytes: &[u8], error: photocraft_codecs::CodecError) -> Error {
    match error {
        photocraft_codecs::CodecError::UnknownFormat => Error::UnknownFormat,
        photocraft_codecs::CodecError::Unsupported { .. } => {
            Error::Unsupported(format, "unsupported by shared decoder (format, encoding, or build feature)")
        }
        error @ photocraft_codecs::CodecError::LimitExceeded(_) => {
            if let Some(f) = format.raster()
                && let Ok((w, h)) = photocraft_codecs::dimensions(f, bytes)
            {
                return Error::TooLarge(u64::from(w), u64::from(h));
            }
            Error::Malformed(format, error.to_string())
        }
        error => Error::Malformed(format, error.to_string()),
    }
}

/// Preserve device precision until ICC/TRC interpretation; no RGBA8 detour.
pub(crate) fn from_shared(format: Format, mut image: photocraft_codecs::Image, source_dims: (u32, u32), opts: &DecodeOptions) -> Result<Decoded> {
    use photocraft_codecs::{ChannelLayout as L, SampleType as S};
    let layout = image.layout();
    let sample = image.sample_type();
    let hint = image.color_hint.as_ref().map(|h| SourceSpace {
        named: h.named,
        to_xyz_d50: h.to_xyz_d50,
        trc: Some(h.trc.clone()),
        origin: SpaceOrigin::Container,
    });
    let meta = Meta {
        icc: image.icc.take(),
        exif: image.meta.exif.take(),
        xmp: image.meta.xmp.take(),
        orientation: Some(image.orientation),
        already_linear: if image.color_hint.as_ref().is_some_and(|h| h.already_linear) { hint.clone() } else { None },
        hint,
    };
    let source_float = image.source_float.unwrap_or(sample.is_float());
    let raw = Raw {
        width: image.width() as usize,
        height: image.height() as usize,
        model: match layout {
            L::Gray | L::GrayA => Model::Gray,
            L::Cmyk | L::CmykA => Model::Cmyk,
            _ => Model::Rgb,
        },
        alpha: layout.has_alpha(),
        premultiplied: image.associated_alpha,
        bit_depth: image.source_bit_depth.unwrap_or((sample.bytes() * 8) as u8),
        buf: match sample {
            S::U8 => Buf::U8(image.into_data()),
            S::U16 => Buf::U16(image.to_u16_samples().unwrap_or_default()),
            S::F16 => Buf::F32(image.to_f16_samples().unwrap_or_default().into_iter().map(|v| v.to_f32()).collect()),
            S::F32 => Buf::F32(image.to_f32_samples().unwrap_or_default()),
        },
    };
    let mut decoded = finish(format, raw, meta, source_dims, opts)?;
    decoded.float = source_float;
    Ok(decoded)
}

pub(crate) fn finish(format: Format, raw: Raw, meta: Meta, source_dims: (u32, u32), opts: &DecodeOptions) -> Result<Decoded> {
    let n = raw.width * raw.height;
    let ch = raw.model.channels() + raw.alpha as usize;
    if n == 0 || raw.buf.len() < n * ch {
        return Err(Error::Malformed(format, "sample buffer too short".into()));
    }
    let float = matches!(raw.buf, Buf::F32(_));

    // Alpha plane (straight).
    let alpha = raw.alpha.then(|| {
        let mut a = GrayFrame::new(raw.width, raw.height);
        for (i, o) in a.data.iter_mut().enumerate() {
            *o = raw.buf.norm(i * ch + ch - 1).clamp(0.0, 1.0);
        }
        a
    });

    let parsed_icc = meta.icc.as_deref().and_then(icc::parse);
    let (image, space) = if let Some(space) = meta.already_linear.clone() {
        (to_rgb_passthrough(&raw, ch), space)
    } else if raw.model == Model::Cmyk {
        cmyk(&raw, ch, meta.icc.as_deref(), parsed_icc.as_ref())
    } else {
        rgb_or_gray(&raw, ch, &meta, parsed_icc.as_ref(), float)
    };

    let mut image = image;
    if raw.premultiplied
        && let Some(a) = &alpha
    {
        for (p, &av) in image.data.iter_mut().zip(&a.data) {
            if av <= 0.0 {
                // Associated colour is undefined at zero coverage, including a
                // subtractive CMS whose zero device values would otherwise be white.
                *p = [0.0; 3];
            } else if float {
                // Float input retains its existing linear/HDR interpretation.
                *p = p.map(|c| c / av);
            }
        }
    }

    let summary = meta.exif.as_deref().map(lightcraft_tiff::exif::summarize).unwrap_or_default();
    let orientation = meta.orientation.or(summary.orientation).filter(|o| (1..=8).contains(o)).unwrap_or(1);

    let (image, alpha) = match opts.max_size {
        Some((mw, mh)) if (image.width as u32 > mw || image.height as u32 > mh) && mw > 0 && mh > 0 => {
            let factor = (image.width as f64 / mw as f64).max(image.height as f64 / mh as f64);
            let f = if factor >= 2.0 { Filter::Box } else { Filter::Mitchell };
            let img = image.fit(mw as usize, mh as usize, f);
            let alpha = alpha.map(|a| a.fit(mw as usize, mh as usize, f));
            (img, alpha)
        }
        _ => (image, alpha),
    };

    Ok(Decoded {
        format,
        width: image.width as u32,
        height: image.height as u32,
        image,
        has_alpha: alpha.is_some(),
        alpha,
        space,
        bit_depth: raw.bit_depth,
        float,
        grayscale: raw.model == Model::Gray,
        cmyk: raw.model == Model::Cmyk,
        orientation,
        exif: meta.exif,
        xmp: meta.xmp,
        icc: meta.icc,
        source_width: source_dims.0,
        source_height: source_dims.1,
    })
}

fn to_rgb_passthrough(raw: &Raw, ch: usize) -> RgbFrame {
    let gray = raw.model == Model::Gray;
    let mut img = RgbFrame::new(raw.width, raw.height);
    par_rows(&mut img.data, raw.width, |y, row| {
        for (x, o) in row.iter_mut().enumerate() {
            let p = y * raw.width + x;
            *o = if gray {
                let v = raw.device_sample(p, 0, ch);
                [v, v, v]
            } else {
                [raw.device_sample(p, 0, ch), raw.device_sample(p, 1, ch), raw.device_sample(p, 2, ch)]
            };
        }
    });
    img
}

/// Per-channel linearization via LUTs (8/16-bit) or direct evaluation (float).
fn linearize(raw: &Raw, ch: usize, trc: &[Trc; 3]) -> RgbFrame {
    let gray = raw.model == Model::Gray;
    let mut img = RgbFrame::new(raw.width, raw.height);
    let w = raw.width;
    if raw.associated_integer() {
        par_rows(&mut img.data, w, |y, row| {
            for (x, out) in row.iter_mut().enumerate() {
                let pixel = y * w + x;
                *out = if gray {
                    [trc[1].to_linear(raw.device_sample(pixel, 0, ch)); 3]
                } else {
                    std::array::from_fn(|c| trc[c].to_linear(raw.device_sample(pixel, c, ch)))
                };
            }
        });
        return img;
    }
    match &raw.buf {
        Buf::U8(v) => {
            let luts: Vec<Vec<f32>> = trc.iter().map(|t| t.lut8()).collect();
            par_rows(&mut img.data, w, |y, row| {
                for (x, o) in row.iter_mut().enumerate() {
                    let i = (y * w + x) * ch;
                    *o = if gray {
                        let l = luts[1][v[i] as usize];
                        [l, l, l]
                    } else {
                        [luts[0][v[i] as usize], luts[1][v[i + 1] as usize], luts[2][v[i + 2] as usize]]
                    };
                }
            });
        }
        Buf::U16(v) => {
            let same = trc[0] == trc[1] && trc[1] == trc[2];
            let l0 = trc[1].lut16();
            let (l1, l2) = if same || gray { (None, None) } else { (Some(trc[0].lut16()), Some(trc[2].lut16())) };
            let lr = l1.as_ref().unwrap_or(&l0);
            let lb = l2.as_ref().unwrap_or(&l0);
            par_rows(&mut img.data, w, |y, row| {
                for (x, o) in row.iter_mut().enumerate() {
                    let i = (y * w + x) * ch;
                    *o = if gray {
                        let l = l0[v[i] as usize];
                        [l, l, l]
                    } else {
                        [lr[v[i] as usize], l0[v[i + 1] as usize], lb[v[i + 2] as usize]]
                    };
                }
            });
        }
        Buf::F32(v) => {
            let lin = trc.iter().all(|t| t.is_linear());
            par_rows(&mut img.data, w, |y, row| {
                for (x, o) in row.iter_mut().enumerate() {
                    let i = (y * w + x) * ch;
                    let s = |c: usize| {
                        let val = v[i + c];
                        if val.is_finite() { val } else { 0.0 }
                    };
                    *o = if gray {
                        let l = if lin { s(0) } else { trc[1].to_linear(s(0)) };
                        [l, l, l]
                    } else if lin {
                        [s(0), s(1), s(2)]
                    } else {
                        [trc[0].to_linear(s(0)), trc[1].to_linear(s(1)), trc[2].to_linear(s(2))]
                    };
                }
            });
        }
    }
    img
}

fn device_samples(raw: &Raw, ch: usize, n_color: usize) -> Vec<f32> {
    let n = raw.width * raw.height;
    let mut out = Vec::with_capacity(n * n_color);
    for p in 0..n {
        for c in 0..n_color {
            out.push(raw.device_sample(p, c, ch).clamp(0.0, 1.0));
        }
    }
    out
}

fn from_vec(w: usize, h: usize, data: Vec<[f32; 3]>) -> RgbFrame {
    RgbFrame { width: w, height: h, data }
}

fn rgb_or_gray(raw: &Raw, ch: usize, meta: &Meta, info: Option<&icc::IccInfo>, float: bool) -> (RgbFrame, SourceSpace) {
    let gray = raw.model == Model::Gray;
    if let (Some(bytes), Some(info)) = (meta.icc.as_deref(), info) {
        match (&info.kind, gray) {
            (IccKind::MatrixTrc { to_xyz_d50, trc }, false) => {
                let space = SourceSpace { named: info.named, to_xyz_d50: *to_xyz_d50, trc: Some(trc.clone()), origin: SpaceOrigin::IccMatrixTrc };
                return (linearize(raw, ch, trc), space);
            }
            (IccKind::MatrixTrc { to_xyz_d50, trc }, true) => {
                // Gray data with an RGB profile: neutral axis only needs the green curve.
                let t = [trc[1].clone(), trc[1].clone(), trc[1].clone()];
                let space = SourceSpace { named: info.named, to_xyz_d50: *to_xyz_d50, trc: Some(t.clone()), origin: SpaceOrigin::IccMatrixTrc };
                return (linearize(raw, ch, &t), space);
            }
            (IccKind::Gray { trc }, true) => {
                let t = [trc.clone(), trc.clone(), trc.clone()];
                let mut space = SourceSpace::named(NamedSpace::Srgb, SpaceOrigin::IccMatrixTrc);
                space.trc = Some(t.clone());
                return (linearize(raw, ch, &t), space);
            }
            (IccKind::NeedsCms, _) => {
                let wanted = if gray { IccColorModel::Gray } else { IccColorModel::Rgb };
                if info.model == wanted {
                    let nc = raw.model.channels();
                    let samples = device_samples(raw, ch, nc);
                    if let Some(px) = icc::cms_to_linear_rec2020(bytes, &samples, nc) {
                        let mut s = SourceSpace::named(NamedSpace::Rec2020, SpaceOrigin::IccCms);
                        s.trc = None;
                        return (from_vec(raw.width, raw.height, px), s);
                    }
                }
            }
            _ => {}
        }
    }
    if meta.icc.is_some() {
        // Present but unusable (malformed, mismatched model, CMS failure): assume sRGB.
        let s = SourceSpace::named(NamedSpace::Srgb, SpaceOrigin::IccUnsupported);
        let t = NamedSpace::Srgb.trc();
        return (linearize(raw, ch, &[t.clone(), t.clone(), t]), s);
    }
    if let Some(h) = &meta.hint {
        let t = h.trc.clone().unwrap_or([Trc::Srgb, Trc::Srgb, Trc::Srgb]);
        return (linearize(raw, ch, &t), h.clone());
    }
    let adobe = meta.exif.as_deref().map(lightcraft_tiff::exif::summarize).is_some_and(|s| s.adobe_rgb_hint);
    let mut s = if adobe {
        SourceSpace::named(NamedSpace::AdobeRgb, SpaceOrigin::Container)
    } else {
        SourceSpace::named(NamedSpace::Srgb, SpaceOrigin::Untagged)
    };
    if float {
        // Untagged float data is conventionally scene-linear.
        s.trc = Some([Trc::Linear, Trc::Linear, Trc::Linear]);
    }
    let t = s.trc.clone().unwrap_or([Trc::Srgb, Trc::Srgb, Trc::Srgb]);
    (linearize(raw, ch, &t), s)
}

fn cmyk(raw: &Raw, ch: usize, icc_bytes: Option<&[u8]>, info: Option<&icc::IccInfo>) -> (RgbFrame, SourceSpace) {
    let samples = device_samples(raw, ch, 4);
    if let (Some(bytes), Some(info)) = (icc_bytes, info)
        && info.model == IccColorModel::Cmyk
        && let Some(px) = icc::cms_to_linear_rec2020(bytes, &samples, 4)
    {
        let mut s = SourceSpace::named(NamedSpace::Rec2020, SpaceOrigin::IccCms);
        s.trc = None;
        return (from_vec(raw.width, raw.height, px), s);
    }
    // Naive: RGB = (1 - C)(1 - K) … treated as sRGB-encoded.
    let px = samples.as_chunks::<4>().0.iter().map(|c| li_color::convert::cmyk_to_rgb_naive(*c).map(li_color::transfer::srgb_to_linear)).collect();
    let origin = if icc_bytes.is_some() { SpaceOrigin::IccUnsupported } else { SpaceOrigin::Naive };
    (from_vec(raw.width, raw.height, px), SourceSpace::named(NamedSpace::Srgb, origin))
}

#[cfg(test)]
mod alpha_tests {
    use super::*;

    fn input(model: Model, buf: Buf, associated: bool) -> Raw {
        Raw { width: buf.len() / (model.channels() + 1), height: 1, model, alpha: true, premultiplied: associated, buf, bit_depth: 32 }
    }
    #[test]
    fn j6_limits_preserve_explicit_large_capture_budget_without_allocating_pixels() {
        use photocraft_codecs::{ChannelLayout, SampleType};
        let options = DecodeOptions { max_pixels: 100_000_000, ..Default::default() };
        let limits = shared_limits(&options);
        assert!(limits.check(10_000, 10_000, ChannelLayout::Rgb, SampleType::U16).is_ok());
        assert!(limits.check(10_001, 10_000, ChannelLayout::Rgb, SampleType::U16).is_err());
        assert_eq!(limits.max_width, u32::MAX);
        assert_eq!(limits.max_alloc, photocraft_codecs::DESKTOP_PHOTO_BYTES);
    }

    #[test]
    fn desktop_photo_facades_share_pixel_and_allocation_limits() {
        use photocraft_codecs::{ChannelLayout, Limits, SampleType};
        let desktop = Limits::desktop_photo();
        let limits = shared_limits(&DecodeOptions { max_pixels: photocraft_codecs::DESKTOP_PHOTO_PIXELS, ..Default::default() });
        assert_eq!(limits.max_pixels, desktop.max_pixels);
        assert_eq!(limits.max_alloc, desktop.max_alloc);
        assert!(limits.check(16_384, 8_192, ChannelLayout::Rgba, SampleType::F32).is_ok());
        assert!(limits.check(16_384, 8_192, ChannelLayout::CmykA, SampleType::F32).is_err());
        assert!(limits.check(16_384, 8_193, ChannelLayout::Gray, SampleType::U8).is_err());
        assert_eq!(shared_limits(&DecodeOptions::default()).max_pixels, Limits::default().max_pixels);
    }

    #[test]
    fn associated_integer_samples_reach_cms_unassociated() {
        let rgb = input(Model::Rgb, Buf::U8(vec![32, 64, 96, 128]), true);
        assert_eq!(device_samples(&rgb, 4, 3), [0.25, 0.5, 0.75]);
        // Force the actual CMS dispatch using a known profile, then compare with
        // independently unassociated input sent directly to that transform.
        let profile = icc::write_named(NamedSpace::DisplayP3);
        let mut info = icc::parse(&profile).unwrap();
        info.kind = IccKind::NeedsCms;
        let expected = icc::cms_to_linear_rec2020(&profile, &[0.25, 0.5, 0.75], 3).unwrap();
        let meta = Meta { icc: Some(profile), ..Default::default() };
        let (actual, space) = rgb_or_gray(&rgb, 4, &meta, Some(&info), false);
        assert_eq!(space.origin, SpaceOrigin::IccCms);
        for (a, b) in actual.data[0].iter().zip(expected[0]) {
            assert!((*a - b).abs() < 1e-6);
        }

        let cmyk = input(Model::Cmyk, Buf::U16(vec![8192, 16384, 24576, 8192, 32768, 65535, 65535, 65535, 65535, 0]), true);
        assert_eq!(device_samples(&cmyk, 5, 4), [0.25, 0.5, 0.75, 0.25, 0., 0., 0., 0.]);
        let decoded = finish(Format::Tiff, cmyk, Meta::default(), (2, 1), &DecodeOptions::default()).unwrap();
        let expected = [0.75 * 0.75, 0.5 * 0.75, 0.25 * 0.75].map(li_color::transfer::srgb_to_linear);
        for (actual, want) in decoded.image.data[0].iter().zip(expected) {
            assert!((*actual - want).abs() < 1e-6);
        }
        assert_eq!(decoded.image.data[1], [0.; 3]);
    }
    #[test]
    fn associated_float_retains_linear_hdr_and_explicit_transfer_semantics() {
        for associated in [false, true] {
            let raw = input(Model::Rgb, Buf::F32(vec![-0.25, 1., 2., 0.5, 0.25, 0.5, 0.75, 0.]), associated);
            let decoded = finish(Format::Tiff, raw, Meta::default(), (2, 1), &DecodeOptions::default()).unwrap();
            assert!(decoded.float);
            assert_eq!(decoded.image.data[0], if associated { [-0.5, 2., 4.] } else { [-0.25, 1., 2.] });
            assert_eq!(decoded.image.data[1], if associated { [0.; 3] } else { [0.25, 0.5, 0.75] });
            assert_eq!(decoded.alpha.unwrap().data, [0.5, 0.]);
        }
        let raw = input(Model::Rgb, Buf::F32(vec![0.25, 0.25, 0.25, 0.5]), true);
        let meta = Meta { icc: Some(icc::write_named(NamedSpace::Srgb)), ..Default::default() };
        let decoded = finish(Format::Tiff, raw, meta, (1, 1), &DecodeOptions::default()).unwrap();
        assert!((decoded.image.data[0][0] - Trc::Srgb.to_linear(0.25) / 0.5).abs() < 1e-5);
        let raw = input(Model::Rgb, Buf::U8(vec![32, 64, 96, 128]), true);
        let meta = Meta { already_linear: Some(SourceSpace::named(NamedSpace::Srgb, SpaceOrigin::Untagged)), ..Default::default() };
        let decoded = finish(Format::Tiff, raw, meta, (1, 1), &DecodeOptions::default()).unwrap();
        assert!(!decoded.float);
        assert_eq!(decoded.image.data[0], [0.25, 0.5, 0.75]);
    }
}
