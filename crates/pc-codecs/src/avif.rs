//! Shared AVIF encoder (ravif/rav1e, pure Rust); 8/16-bit input, 10-bit output.
use crate::avif_input::{EncodeImage, EncodeMeta, Samples};
use crate::{CodecError, Format};
fn error(message: impl Into<String>) -> CodecError {
    CodecError::encode(Format::Avif, message.into())
}
/// Encode AVIF (sRGB, 4:4:4, 10-bit AV1) with `ravif` (rav1e). 8-bit samples are widened by the
/// encoder; 16-bit samples keep 10 bits of precision (BT.601 full-range YCbCr computed here).
/// `quality` 1..=100, `speed` 1 (slow) ..= 10. EXIF is embedded; ICC is not supported by the muxer
/// (the file is tagged sRGB via `nclx`). Returns [`CodecError`] for float samples, on wasm32 or
/// when built without the `avif` feature.
pub fn encode_avif(img: &EncodeImage, quality: u8, speed: u8, meta: &EncodeMeta) -> Result<Vec<u8>, CodecError> {
    img.validate()?;
    #[cfg(all(feature = "avif", not(target_arch = "wasm32")))]
    {
        let mut enc =
            ravif::Encoder::new().with_quality(quality.clamp(1, 100) as f32).with_alpha_quality(quality.clamp(1, 100) as f32).with_speed(speed.clamp(1, 10));
        if let Some(exif) = meta.exif {
            enc = enc.with_exif(exif.to_vec());
        }
        let s = match img.samples {
            Samples::U8(s) => s,
            Samples::U16(s) => return avif_10bit(&enc, img, s),
            Samples::F32(_) => return Err(error("AVIF encoder needs 8- or 16-bit samples")),
        };
        let n = img.width as usize * img.height as usize;
        let ch = img.channels as usize;
        let px: Vec<ravif::RGBA8> = s
            .chunks_exact(ch)
            .take(n)
            .map(|c| match ch {
                1 => ravif::RGBA8::new(c[0], c[0], c[0], 255),
                2 => ravif::RGBA8::new(c[0], c[0], c[0], c[1]),
                3 => ravif::RGBA8::new(c[0], c[1], c[2], 255),
                _ => ravif::RGBA8::new(c[0], c[1], c[2], c[3]),
            })
            .collect();
        let r = enc.encode_rgba(ravif::Img::new(&px[..], img.width as usize, img.height as usize)).map_err(|e| error(e.to_string()))?;
        let _ = img.has_alpha();
        Ok(r.avif_file)
    }
    #[cfg(not(all(feature = "avif", not(target_arch = "wasm32"))))]
    {
        let _ = (quality, speed, meta, img.has_alpha());
        Err(error("AVIF encoding is not available in this build"))
    }
}

/// 16-bit RGB(A) → 10-bit BT.601 full-range YCbCr planes (the matrix `ravif` uses for 8-bit input).
#[cfg(all(feature = "avif", not(target_arch = "wasm32")))]
fn avif_10bit(enc: &ravif::Encoder, img: &EncodeImage, s: &[u16]) -> Result<Vec<u8>, CodecError> {
    const K: [f32; 3] = li_color::convert::LUMA_601;
    let n = img.width as usize * img.height as usize;
    let ch = img.channels as usize;
    let rgb = |c: &[u16]| -> [f32; 3] {
        let v = |x: u16| x as f32 * (1023.0 / 65535.0);
        match ch {
            1 | 2 => [v(c[0]); 3],
            _ => [v(c[0]), v(c[1]), v(c[2])],
        }
    };
    let planes = s.chunks_exact(ch).take(n).map(|c| {
        let [r, g, b] = rgb(c);
        let y = li_color::convert::rgb_to_gray([r, g, b]);
        let cb = (b - y) * (0.5 / (1.0 - K[2])) + 512.0;
        let cr = (r - y) * (0.5 / (1.0 - K[0])) + 512.0;
        [y, cb, cr].map(|v| v.round().clamp(0.0, 1023.0) as u16)
    });
    let alpha_at = |c: &[u16]| if img.has_alpha() { c[ch - 1] } else { u16::MAX };
    let has_alpha = s.chunks_exact(ch).take(n).any(|c| alpha_at(c) != u16::MAX);
    let alpha = has_alpha.then(|| s.chunks_exact(ch).take(n).map(move |c| ((alpha_at(c) as u32 * 1023 + 32767) / 65535) as u16));
    let r = enc
        .encode_raw_planes_10_bit(img.width as usize, img.height as usize, planes, alpha, ravif::PixelRange::Full, ravif::MatrixCoefficients::BT601)
        .map_err(|e| error(e.to_string()))?;
    Ok(r.avif_file)
}

/// Encode the shared raster image using the requested metadata policy.
pub fn encode_image(src: &crate::Image, quality: u8, speed: u8, opts: &crate::EncodeOptions) -> Result<Vec<u8>, CodecError> {
    let layout = if src.layout().has_alpha() { crate::ChannelLayout::Rgba } else { crate::ChannelLayout::Rgb };
    let depth = if src.sample_type() == crate::SampleType::U16 { crate::SampleType::U16 } else { crate::SampleType::U8 };
    let img = src.convert(layout, depth);
    let deep;
    let samples = if depth == crate::SampleType::U16 {
        deep = img.data().as_chunks::<2>().0.iter().map(|b| u16::from_ne_bytes(*b)).collect::<Vec<_>>();
        Samples::U16(&deep)
    } else {
        Samples::U8(img.data())
    };
    let exif = opts.embed_metadata.then(|| img.meta.exif.as_deref().map(|e| crate::orientation::encoded_exif(e, opts.keep_orientation))).flatten();
    encode_avif(&EncodeImage::new(img.width(), img.height(), layout.channels() as u8, samples), quality, speed, &EncodeMeta { exif: exif.as_deref() })
}
