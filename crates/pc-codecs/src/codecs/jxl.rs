//! JPEG XL samples via the pure Rust `jxl-oxide` decoder. Enum-encoded files are rendered to
//! linear Rec.2020; ICC-encoded files retain their source encoding for the caller's CMS.

use crate::Format;
use crate::error::CodecError;
use crate::image::{ChannelLayout, ColorHint, DecodeWarning, Image, Metadata, SampleType};
use crate::options::Limits;
use jxl_oxide::color::{ColourSpace, EnumColourEncoding, Primaries, RenderingIntent, TransferFunction, WhitePoint};
use jxl_oxide::{AllocTracker, AuxBoxData, JxlImage};

const F: Format = Format::Jxl;

fn err(e: impl std::fmt::Display) -> CodecError {
    CodecError::malformed(F, e)
}

pub(crate) fn decode(bytes: &[u8], limits: &Limits) -> Result<Image, CodecError> {
    decode_inner(bytes, limits, false)
}

/// Library colour processing unassociates float samples after conversion to its linear space.
pub(crate) fn decode_for_color(bytes: &[u8], limits: &Limits) -> Result<Image, CodecError> {
    decode_inner(bytes, limits, true)
}

fn decode_inner(bytes: &[u8], limits: &Limits, keep_associated: bool) -> Result<Image, CodecError> {
    let mut image = JxlImage::builder().alloc_tracker(AllocTracker::with_limit(limits.alloc_usize())).read(std::io::Cursor::new(bytes)).map_err(err)?;
    // jxl-oxide reports oriented dimensions and renders in that orientation.
    let (w, h) = (image.width(), image.height());
    let header = image.image_header();
    let bits = header.metadata.bit_depth.bits_per_sample() as u8;
    let float = matches!(header.metadata.bit_depth, jxl_oxide::image::BitDepth::FloatSample { .. });
    let gray = header.metadata.grayscale();
    let associated = header.metadata.ec_info.iter().find_map(|channel| channel.alpha_associated()).unwrap_or(false);
    let layout = match image.pixel_format() {
        jxl_oxide::PixelFormat::Gray => ChannelLayout::Gray,
        jxl_oxide::PixelFormat::Graya => ChannelLayout::GrayA,
        jxl_oxide::PixelFormat::Rgb => ChannelLayout::Rgb,
        jxl_oxide::PixelFormat::Rgba => ChannelLayout::Rgba,
        jxl_oxide::PixelFormat::Cmyk => ChannelLayout::Cmyk,
        jxl_oxide::PixelFormat::Cmyka => ChannelLayout::CmykA,
    };
    limits.check(w, h, layout, SampleType::F32)?;
    let icc = image.original_icc().map(<[u8]>::to_vec);
    let color_hint = if icc.is_none() {
        image.request_color_encoding(EnumColourEncoding {
            colour_space: if gray { ColourSpace::Grey } else { ColourSpace::Rgb },
            white_point: WhitePoint::D65,
            primaries: Primaries::Bt2100,
            tf: TransferFunction::Linear,
            rendering_intent: RenderingIntent::Relative,
        });
        Some(ColorHint::linear_rec2020())
    } else {
        None
    };
    let mut meta = Metadata::default();
    if let Ok(AuxBoxData::Data(exif)) = image.aux_boxes().first_exif() {
        // Samples are already oriented. Normalizing the embedded tag prevents a second turn by
        // decode_with or by consumers of the original EXIF payload.
        meta.exif = Some(lightcraft_tiff::exif::upright(exif.payload()).into_owned());
    }
    if let AuxBoxData::Data(xml) = image.aux_boxes().first_xml() {
        let xmp = String::from_utf8_lossy(xml).trim_end_matches('\0').to_owned();
        meta.xmp = Some(lightcraft_meta::upright_xmp(&xmp).into_owned());
    }
    let render = image.render_frame(0).map_err(err)?;
    // The stream selects black and the primary alpha by channel type, instead of assuming the
    // first extra channel is alpha. It also composites spot colours as the decoder recommends.
    let mut stream = render.stream();
    let (fw, fh) = (stream.width(), stream.height());
    if stream.channels() as usize != layout.channels() {
        return Err(err("unexpected rendered channel layout"));
    }
    limits.check(fw, fh, layout, SampleType::F32)?;
    let len = (fw as usize).checked_mul(fh as usize).and_then(|n| n.checked_mul(layout.channels())).ok_or_else(|| err("pixel buffer overflow"))?;
    let mut values = Vec::new();
    values.try_reserve_exact(len).map_err(|e| CodecError::LimitExceeded(e.to_string()))?;
    values.resize(len, 0.0f32);
    if stream.write_to_buffer(&mut values) != len {
        return Err(err("short pixel buffer"));
    }
    if associated && !keep_associated {
        unassociate(&mut values, layout);
    }
    let mut result = Image::from_f32(fw, fh, layout, &values)?;
    result.associated_alpha = associated && keep_associated;
    result.icc = icc;
    result.meta = meta;
    result.color_hint = color_hint;
    result.source_bit_depth = Some(bits);
    result.source_float = Some(float);
    let frames = image.num_loaded_keyframes();
    if frames > 1 {
        result.warnings.push(DecodeWarning::MoreFrames { total: u32::try_from(frames).ok() });
    }
    if !image.is_loading_done() {
        result.warnings.push(DecodeWarning::Truncated { format: F });
    }
    Ok(result)
}

fn unassociate(values: &mut [f32], layout: ChannelLayout) {
    if !layout.has_alpha() {
        return;
    }
    let colors = layout.color_channels();
    for pixel in values.chunks_exact_mut(layout.channels()) {
        let alpha = pixel[colors];
        for value in &mut pixel[..colors] {
            *value = if alpha > 0.0 { *value / alpha } else { 0.0 };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn j06_jxl_associated_fixture_preserves_raw_and_exposes_straight_samples() {
        let bytes = include_bytes!("../../tests/fixtures/j06-jxl-conformance/alpha_premultiplied.jxl");
        let raw = decode_for_color(bytes, &Limits::default()).unwrap();
        let straight = decode(bytes, &Limits::default()).unwrap();
        assert!(raw.associated_alpha);
        assert!(!straight.associated_alpha);
        assert_eq!(raw.dimensions(), straight.dimensions());
        assert_eq!(raw.layout(), ChannelLayout::Rgba);
        assert_eq!(raw.source_bit_depth, Some(12));
        assert_eq!(raw.source_float, Some(false));
        let mut fractional_alpha = 0usize;
        for (raw, straight) in raw.to_f32_samples().unwrap().as_chunks::<4>().0.iter().zip(straight.to_f32_samples().unwrap().as_chunks::<4>().0.iter()) {
            assert_eq!(raw[3], straight[3]);
            if raw[3] > 0.0 {
                fractional_alpha += usize::from(raw[3] < 1.0);
                for channel in 0..3 {
                    assert!((raw[channel] / raw[3] - straight[channel]).abs() < 1e-6);
                }
            } else {
                assert_eq!(&straight[..3], &[0.0, 0.0, 0.0]);
            }
        }
        assert!(fractional_alpha > 0, "fixture must exercise association");
        assert!(raw.color_hint.as_ref().is_some_and(|hint| hint.already_linear));
    }

    #[test]
    fn j06_jxl_unassociates_float_hdr_without_clipping() {
        let mut samples = vec![0.5, 0.25, 1.5, 0.5, 0.3, 0.2, 0.1, 0.0];
        unassociate(&mut samples, ChannelLayout::Rgba);
        assert_eq!(samples, [1.0, 0.5, 3.0, 0.5, 0.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn j06_jxl_rejects_truncated_header_without_panicking() {
        for bytes in [&b"\xff\x0a"[..], &b"\0\0\0\x0cJXL \r\n\x87\n"[..]] {
            assert!(decode(bytes, &Limits::default()).is_err());
        }
    }
}
