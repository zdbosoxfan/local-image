//! Compatibility adapter to the sole shared parallel JPEG encoder.
use crate::encode::ChromaSubsampling;
/// Encode independent restart bands, preserving the historical Library API.
pub fn encode(
    data: &[u8],
    width: usize,
    height: usize,
    channels: usize,
    quality: u8,
    sampling: ChromaSubsampling,
    segments: &[(u8, Vec<u8>)],
) -> Vec<u8> {
    let sampling = match sampling {
        ChromaSubsampling::S444 => photocraft_codecs::ChromaSubsampling::S444,
        ChromaSubsampling::S422 => photocraft_codecs::ChromaSubsampling::S422,
        ChromaSubsampling::S420 => photocraft_codecs::ChromaSubsampling::S420,
    };
    photocraft_codecs::jpeg_par::encode(data, width, height, channels, quality, sampling, segments)
}
