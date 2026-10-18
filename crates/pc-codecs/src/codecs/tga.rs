//! Targa RLE encoding with packets confined to a single bottom-up scanline.

use crate::{
    Format,
    error::CodecError,
    image::{ChannelLayout, Image, SampleType},
};

pub(crate) fn encode(img: &Image) -> Result<Vec<u8>, CodecError> {
    let (w, h) = img.dimensions();
    let (Ok(w), Ok(h)) = (u16::try_from(w), u16::try_from(h)) else {
        return Err(CodecError::encode(Format::Tga, "dimensions exceed 65535x65535"));
    };
    if img.sample_type() != SampleType::U8 {
        return Err(CodecError::encode(Format::Tga, "Targa requires 8-bit samples"));
    }
    let (bpp, kind, alpha): (usize, u8, u8) = match img.layout() {
        ChannelLayout::Gray => (1, 11, 0),
        ChannelLayout::GrayA => (2, 11, 8),
        ChannelLayout::Rgb => (3, 10, 0),
        ChannelLayout::Rgba => (4, 10, 8),
        _ => return Err(CodecError::encode(Format::Tga, "unsupported Targa layout")),
    };
    let row = usize::from(w).checked_mul(bpp).ok_or_else(|| CodecError::InvalidImage("row size overflow".into()))?;
    if row.checked_mul(usize::from(h)) != Some(img.data().len()) || row == 0 {
        return Err(CodecError::InvalidImage("buffer size mismatch".into()));
    }
    let mut out = vec![0, 0, kind, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    out.extend_from_slice(&w.to_le_bytes());
    out.extend_from_slice(&h.to_le_bytes());
    out.extend_from_slice(&[(bpp * 8) as u8, alpha]);
    for source in img.data().rchunks_exact(row) {
        let mut pixels = source.to_vec();
        if bpp >= 3 {
            for pixel in pixels.chunks_exact_mut(bpp) {
                pixel.swap(0, 2);
            }
        }
        let pixel = |i: usize| &pixels[i * bpp..(i + 1) * bpp];
        let mut i = 0;
        while i < usize::from(w) {
            let mut run = 1;
            while run < 128 && i + run < usize::from(w) && pixel(i) == pixel(i + run) {
                run += 1;
            }
            if run > 1 {
                out.push(0x80 | (run - 1) as u8);
                out.extend_from_slice(pixel(i));
                i += run;
            } else {
                let start = i;
                i += 1;
                while i < usize::from(w) && i - start < 128 && !(i + 1 < usize::from(w) && pixel(i) == pixel(i + 1)) {
                    i += 1;
                }
                out.push((i - start - 1) as u8);
                out.extend_from_slice(&pixels[start * bpp..i * bpp]);
            }
        }
    }
    out.extend_from_slice(b"\0\0\0\0\0\0\0\0TRUEVISION-XFILE.\0");
    Ok(out)
}
