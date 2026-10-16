//! PSD/PSB merged pixels from the existing hardened `photocraft-psd` backend.
//! Adobe Photoshop File Formats: planar, big-endian samples; CMYK ink is inverted.
use crate::{ChannelLayout as L, CodecError, Format, Image, Limits, SampleType as S};
use photocraft_psd::{ColorMode, PsdFile};
const F: Format = Format::Psd;

pub(crate) fn decode(bytes: &[u8], limits: &Limits) -> Result<Image, CodecError> {
    // Validate sizes before the format parser allocates layers or composite buffers.
    let header = bytes.get(..26).ok_or_else(|| CodecError::malformed(F, "truncated header"))?;
    let height = u32::from_be_bytes(header[14..18].try_into().map_err(|_| CodecError::malformed(F, "height"))?);
    let width = u32::from_be_bytes(header[18..22].try_into().map_err(|_| CodecError::malformed(F, "width"))?);
    let channels = u16::from_be_bytes([header[12], header[13]]);
    let depth = u16::from_be_bytes([header[22], header[23]]);
    limits.check_bytes(width, height, u64::from(channels).saturating_mul(u64::from(depth.div_ceil(8))).saturating_mul(2))?;
    if bytes.len() as u64 > limits.max_alloc {
        return Err(CodecError::LimitExceeded("PSD input exceeds parser budget".into()));
    }
    let file = PsdFile::from_bytes(bytes).map_err(|e| CodecError::malformed(F, e))?;
    let h = &file.header;
    let (model, colors) = match h.color_mode {
        ColorMode::Grayscale | ColorMode::Duotone | ColorMode::Multichannel | ColorMode::Bitmap => (L::Gray, 1),
        ColorMode::Rgb => (L::Rgb, 3),
        ColorMode::Cmyk => (L::Cmyk, 4),
        ColorMode::Indexed => (L::Rgb, 1),
        _ => return Err(CodecError::unsupported(F, "PSD colour mode")),
    };
    if usize::from(h.channels) < colors {
        return Err(CodecError::malformed(F, "too few colour channels"));
    }
    let indexed = h.color_mode == ColorMode::Indexed;
    let alpha = file.merged_has_alpha() && !indexed;
    let layout = if alpha { model.with_alpha() } else { model };
    let sample = match depth {
        1 | 8 => S::U8,
        16 => S::U16,
        32 => S::F32,
        _ => return Err(CodecError::unsupported(F, "PSD sample depth")),
    };
    limits.check(width, height, layout, sample)?;
    let planes = file.image_data.decode(h).map_err(|e| CodecError::malformed(F, e))?;
    let n = width as usize * height as usize;
    let plane = h.row_bytes() * height as usize;
    let size =
        n.checked_mul(layout.channels()).and_then(|v| v.checked_mul(sample.bytes())).ok_or_else(|| CodecError::LimitExceeded("PSD sample size".into()))?;
    let mut data = crate::options::alloc_zeroed::<u8>(size)?;
    if indexed && (depth != 8 || file.color_mode_data.len() < 768) {
        return Err(CodecError::malformed(F, "invalid indexed palette"));
    }
    for i in 0..n {
        if indexed {
            let index = usize::from(planes[i]);
            data[i * 3..i * 3 + 3].copy_from_slice(&[file.color_mode_data[index], file.color_mode_data[256 + index], file.color_mode_data[512 + index]]);
            continue;
        }
        for c in 0..layout.channels() {
            let dst = (i * layout.channels() + c) * sample.bytes();
            let at = c * plane + i * sample.bytes();
            let invert = model == L::Cmyk && c < colors;
            match depth {
                1 => {
                    let row = i / width as usize;
                    let col = i % width as usize;
                    let bit = (planes[c * plane + row * h.row_bytes() + col / 8] >> (7 - col % 8)) & 1;
                    data[dst] = if bit == 0 { 255 } else { 0 };
                }
                8 => data[dst] = if invert { 255 - planes[at] } else { planes[at] },
                16 => {
                    let value = u16::from_be_bytes([planes[at], planes[at + 1]]);
                    data[dst..dst + 2].copy_from_slice(&if invert { 65535 - value } else { value }.to_ne_bytes());
                }
                _ => {
                    let value = f32::from_be_bytes([planes[at], planes[at + 1], planes[at + 2], planes[at + 3]]);
                    data[dst..dst + 4].copy_from_slice(&if invert { 1.0 - value } else { value }.to_ne_bytes());
                }
            }
        }
    }
    let mut image = Image::from_raw(width, height, layout, sample, data)?;
    image.icc = file.icc_profile().map(<[u8]>::to_vec);
    image.meta.exif = file.resource(1058).or_else(|| file.resource(1059)).map(|r| r.data.strip_prefix(b"Exif\0\0").unwrap_or(&r.data).to_vec());
    image.meta.xmp = file.resource(1060).map(|r| String::from_utf8_lossy(&r.data).trim_end_matches('\0').to_owned());
    image.source_bit_depth = Some(depth as u8);
    image.source_float = Some(depth == 32);
    Ok(image)
}

#[cfg(test)]
mod tests {
    use super::*;
    use photocraft_psd::{Compression, PixelData, PsdBuilder};

    #[test]
    fn shared_psd_composite_preserves_u16_float_and_zip() {
        for float in [false, true] {
            let mut builder = PsdBuilder::new(2, 1).depth(16);
            builder.composite(PixelData::Rgba16(vec![1, 13000, 65534, 32768, 23000, 2, 65535, 65535]));
            let mut file = PsdFile::from_bytes(&builder.to_bytes().unwrap()).unwrap();
            let planar = if float {
                file.header.depth = 32;
                // Planar big-endian RGBA, independently authored float reference.
                [-0.25f32, 0.0, 0.5, 1.0, 2.0, 0.25, 0.5, 1.0].into_iter().flat_map(f32::to_be_bytes).collect()
            } else {
                file.decode_merged().unwrap()
            };
            file.image_data = photocraft_psd::ImageData::encode(Compression::ZipPrediction, &planar, &file.header).unwrap();
            let image = decode(&file.to_bytes().unwrap(), &Limits::default()).unwrap();
            assert_eq!(image.layout(), L::Rgba);
            assert_eq!(image.dimensions(), (2, 1));
            if image.sample_type() == S::U16 {
                assert_eq!(image.to_u16_samples().unwrap(), [1, 13000, 65534, 32768, 23000, 2, 65535, 65535]);
            } else {
                assert_eq!(image.to_f32_samples().unwrap(), [-0.25, 0.5, 2.0, 0.5, 0.0, 1.0, 0.25, 1.0]);
            }
        }
    }
    #[test]
    fn shared_psd_limits_precede_pixel_allocation() {
        let mut builder = PsdBuilder::new(2, 1);
        builder.composite(PixelData::Rgba8(vec![255; 8]));
        let bytes = builder.to_bytes().unwrap();
        let limits = Limits { max_pixels: 1, ..Default::default() };
        assert!(matches!(decode(&bytes, &limits), Err(CodecError::LimitExceeded(_))));
    }
}
