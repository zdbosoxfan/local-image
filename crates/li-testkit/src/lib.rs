//! Local Image's one shared UI test toolkit.
//!
//! - [`softpaint`]: a deterministic CPU rasteriser of egui's tessellated output (the Library's
//!   headless screenshots, agents and CLI use it at runtime too).
//! - `kittest` feature: [`SoftpaintRenderer`], an `egui_kittest` renderer built on [`softpaint`],
//!   so the editor's and the Library's kittest screenshots render the same pixels on any machine,
//!   with no GPU or window.
//! - `gpu` feature: [`gpu`], the one strict real-GPU test prelude (Vulkan, or Metal on macOS;
//!   never a software adapter) shared by the GPU parity gates.
//! - [`constant_gray_jpeg`]: a tiny JPEG fixture of any declared size, for codec limit tests.
#![expect(clippy::allow_attributes_without_reason, reason = "baseline 2026-10-25: unreviewed")]
#![cfg_attr(all(test, feature = "kittest"), expect(clippy::let_underscore_must_use, reason = "baseline 2026-10-25: unreviewed"))]

pub mod softpaint;

#[cfg(feature = "kittest")]
mod kittest;
#[cfg(feature = "kittest")]
pub use kittest::SoftpaintRenderer;

#[cfg(feature = "gpu")]
pub mod gpu;

/// A valid baseline grayscale JPEG with constant 128 samples. Zero DC differences and
/// EOB each use a one-bit Huffman code, so large dimensions need no full-size test pixels.
pub fn constant_gray_jpeg(width: u16, height: u16) -> Vec<u8> {
    fn segment(jpeg: &mut Vec<u8>, marker: u8, payload: &[u8]) {
        jpeg.extend([0xff, marker]);
        jpeg.extend(((payload.len() + 2) as u16).to_be_bytes());
        jpeg.extend(payload);
    }
    let mut jpeg = vec![0xff, 0xd8];
    let mut quant = vec![0];
    quant.extend([1; 64]);
    segment(&mut jpeg, 0xdb, &quant);
    let mut frame = vec![8];
    frame.extend(height.to_be_bytes());
    frame.extend(width.to_be_bytes());
    frame.extend([1, 1, 0x11, 0]);
    segment(&mut jpeg, 0xc0, &frame);
    let mut huffman = Vec::new();
    for class in [0, 0x10] {
        huffman.extend([class, 1]);
        huffman.extend([0; 15]);
        huffman.push(0);
    }
    segment(&mut jpeg, 0xc4, &huffman);
    segment(&mut jpeg, 0xda, &[1, 1, 0, 0, 63, 0]);
    let bits = (width as usize).div_ceil(8) * (height as usize).div_ceil(8) * 2;
    jpeg.resize(jpeg.len() + bits.div_ceil(8), 0);
    if !bits.is_multiple_of(8) {
        *jpeg.last_mut().unwrap() = (1 << (8 - bits % 8)) - 1;
    }
    jpeg.extend([0xff, 0xd9]);
    jpeg
}

#[cfg(test)]
mod golden_tests;
