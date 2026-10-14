//! Borrowed AVIF samples and metadata shared by both front ends.
use crate::{CodecError, Format};
/// Interleaved sample data. Integer samples are display-encoded (0..=MAX); float samples are
/// written as-is.
#[derive(Clone, Copy, Debug)]
pub enum Samples<'a> {
    U8(&'a [u8]),
    U16(&'a [u16]),
    F32(&'a [f32]),
}

/// An image to encode: `channels` = 1 (gray), 2 (gray + alpha), 3 (RGB) or 4 (RGBA), straight alpha.
#[derive(Clone, Copy, Debug)]
pub struct EncodeImage<'a> {
    pub width: u32,
    pub height: u32,
    pub channels: u8,
    pub samples: Samples<'a>,
}

impl<'a> EncodeImage<'a> {
    pub fn new(width: u32, height: u32, channels: u8, samples: Samples<'a>) -> Self {
        EncodeImage { width, height, channels, samples }
    }

    pub(crate) fn validate(&self) -> Result<(), CodecError> {
        if !(1..=4).contains(&self.channels) {
            return Err(CodecError::encode(Format::Avif, format!("unsupported channel count {}", self.channels)));
        }
        if self.width == 0 || self.height == 0 {
            return Err(CodecError::encode(Format::Avif, "zero dimension"));
        }
        let need = self.width as u64 * self.height as u64 * self.channels as u64;
        let have = match self.samples {
            Samples::U8(s) => s.len(),
            Samples::U16(s) => s.len(),
            Samples::F32(s) => s.len(),
        } as u64;
        if have < need {
            return Err(CodecError::encode(Format::Avif, format!("sample buffer too short: {have} < {need}")));
        }
        Ok(())
    }

    pub(crate) fn has_alpha(&self) -> bool {
        self.channels == 2 || self.channels == 4
    }
}

/// Metadata to embed.
#[derive(Clone, Copy, Debug, Default)]
pub struct EncodeMeta<'a> {
    /// TIFF-structured EXIF (no `Exif\0\0` prefix). AVIF's muxer supplies nclx, not ICC.
    pub exif: Option<&'a [u8]>,
}
