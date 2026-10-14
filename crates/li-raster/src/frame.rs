//! One dense, row-major pixel frame for every float pipeline region and AI boundary.
//!
//! Tiled copy-on-write documents stay in `pc-raster::Surface`; this is the *dense* family only.
//! Samples are normalized (`0..=1` for encoded colour); quantization to 8 bits happens only at
//! wires that need it (PNG for ComfyUI, 8-bit previews), through [`quantize_u8`].

use crate::par_rows;
use crate::resample::{Filter, Pixel, half_pixels, resize_pixels};

/// How samples outside a frame (or an area of it) are read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    /// Zero (transparent).
    Transparent,
    /// Clamp to the nearest edge pixel.
    Repeat,
    /// Wrap around.
    Wrap,
}

impl Edge {
    /// Coordinate inside `[x0, x0+w) × [y0, y0+h)` that `(x, y)` reads, or `None` for
    /// [`Edge::Transparent`] outside it (and for an empty area).
    #[inline]
    pub fn resolve(self, x: i64, y: i64, x0: i64, y0: i64, w: i64, h: i64) -> Option<(i64, i64)> {
        if w <= 0 || h <= 0 {
            return None;
        }
        if x >= x0 && x < x0 + w && y >= y0 && y < y0 + h {
            return Some((x, y));
        }
        match self {
            Edge::Transparent => None,
            Edge::Repeat => Some((x.clamp(x0, x0 + w - 1), y.clamp(y0, y0 + h - 1))),
            Edge::Wrap => Some((x0 + (x - x0).rem_euclid(w), y0 + (y - y0).rem_euclid(h))),
        }
    }
}

/// Dense row-major pixels.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Frame<T> {
    pub width: usize,
    pub height: usize,
    pub data: Vec<T>,
}

/// Encoded RGB, normalized (alpha already composited).
pub type RgbFrame = Frame<[f32; 3]>;
/// Straight (unpremultiplied) RGBA, normalized.
pub type RgbaFrame = Frame<[f32; 4]>;
/// One normalized channel (masks, mattes, luminance).
pub type GrayFrame = Frame<f32>;

/// Round a normalized sample to 8 bits (NaN → 0). The only quantizer for frame wires.
#[inline]
pub fn quantize_u8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Round a normalized sample to 16 bits (NaN → 0).
#[inline]
pub fn quantize_u16(v: f32) -> u16 {
    (v.clamp(0.0, 1.0) * 65535.0).round() as u16
}

impl<T: Copy + Default> Frame<T> {
    pub fn new(width: usize, height: usize) -> Self {
        Self { width, height, data: vec![T::default(); width * height] }
    }
    pub fn filled(width: usize, height: usize, v: T) -> Self {
        Self { width, height, data: vec![v; width * height] }
    }
    /// `None` when `data` does not hold exactly `width × height` pixels.
    pub fn from_vec(width: usize, height: usize, data: Vec<T>) -> Option<Self> {
        (width.checked_mul(height) == Some(data.len())).then_some(Self { width, height, data })
    }
    pub fn from_fn(width: usize, height: usize, f: impl Fn(usize, usize) -> T) -> Self {
        let mut data = Vec::with_capacity(width * height);
        for y in 0..height {
            for x in 0..width {
                data.push(f(x, y));
            }
        }
        Self { width, height, data }
    }
    #[inline]
    pub fn dimensions(&self) -> (usize, usize) {
        (self.width, self.height)
    }
    pub fn len(&self) -> usize {
        self.data.len()
    }
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
    #[inline]
    pub fn get(&self, x: usize, y: usize) -> T {
        self.data[y * self.width + x]
    }
    #[inline]
    pub fn set(&mut self, x: usize, y: usize, v: T) {
        self.data[y * self.width + x] = v;
    }
    /// Clamped access (edge extension). The frame must not be empty.
    #[inline]
    pub fn get_clamped(&self, x: isize, y: isize) -> T {
        let x = x.clamp(0, self.width as isize - 1) as usize;
        let y = y.clamp(0, self.height as isize - 1) as usize;
        self.data[y * self.width + x]
    }
    /// Access with an edge policy; [`Edge::Transparent`] reads `T::default()` outside.
    #[inline]
    pub fn get_edge(&self, x: i64, y: i64, edge: Edge) -> T {
        match edge.resolve(x, y, 0, 0, self.width as i64, self.height as i64) {
            Some((x, y)) => self.data[y as usize * self.width + x as usize],
            None => T::default(),
        }
    }
    pub fn row(&self, y: usize) -> &[T] {
        &self.data[y * self.width..(y + 1) * self.width]
    }
    pub fn row_mut(&mut self, y: usize) -> &mut [T] {
        &mut self.data[y * self.width..(y + 1) * self.width]
    }
    /// Per-pixel conversion (row-parallel).
    pub fn map<U: Copy + Default + Send>(&self, f: impl Fn(T) -> U + Sync + Send) -> Frame<U>
    where
        T: Sync,
    {
        let mut out = Frame::<U>::new(self.width, self.height);
        let w = self.width;
        par_rows(&mut out.data, w, |y, row| {
            for (o, s) in row.iter_mut().zip(&self.data[y * w..(y + 1) * w]) {
                *o = f(*s);
            }
        });
        out
    }
    /// [`Self::map`] into the same buffer.
    pub fn map_in_place(&mut self, f: impl Fn(T) -> T + Sync + Send)
    where
        T: Send,
    {
        let w = self.width;
        par_rows(&mut self.data, w, |_, row| row.iter_mut().for_each(|p| *p = f(*p)));
    }
    /// Per-pixel combination of two frames of the same size.
    pub fn zip_map<U: Copy + Default + Sync, V: Copy + Default + Send>(&self, other: &Frame<U>, f: impl Fn(T, U) -> V + Sync + Send) -> Frame<V>
    where
        T: Sync,
    {
        assert_eq!(self.dimensions(), other.dimensions(), "zip_map: size mismatch");
        let mut out = Frame::<V>::new(self.width, self.height);
        let w = self.width;
        par_rows(&mut out.data, w, |y, row| {
            let (a, b) = (&self.data[y * w..(y + 1) * w], &other.data[y * w..(y + 1) * w]);
            for ((o, s), t) in row.iter_mut().zip(a).zip(b) {
                *o = f(*s, *t);
            }
        });
        out
    }
    /// Crop to `[x0, x0+w) × [y0, y0+h)`, clamped to the frame.
    pub fn crop(&self, x0: usize, y0: usize, w: usize, h: usize) -> Self {
        let x0 = x0.min(self.width);
        let y0 = y0.min(self.height);
        let w = w.min(self.width - x0);
        let h = h.min(self.height - y0);
        let mut data = Vec::with_capacity(w * h);
        for y in y0..y0 + h {
            data.extend_from_slice(&self.data[y * self.width + x0..y * self.width + x0 + w]);
        }
        Self { width: w, height: h, data }
    }
    /// Copy `src` into this frame with its top-left at `(x0, y0)`; parts outside are skipped.
    pub fn paste(&mut self, src: &Frame<T>, x0: usize, y0: usize) {
        let w = src.width.min(self.width.saturating_sub(x0));
        for y in 0..src.height.min(self.height.saturating_sub(y0)) {
            let d = (y0 + y) * self.width + x0;
            self.data[d..d + w].copy_from_slice(&src.data[y * src.width..y * src.width + w]);
        }
    }
}

impl<T: Pixel> Frame<T> {
    /// Separable resize on the shared kernels (alpha interpolated premultiplied).
    pub fn resize(&self, width: usize, height: usize, filter: Filter) -> Self {
        let (width, height) = (width.max(1), height.max(1));
        Self { width, height, data: resize_pixels(&self.data, self.width, self.height, width, height, filter) }
    }
    /// 2× box reduction.
    pub fn half(&self) -> Self {
        Self { width: self.width.div_ceil(2).max(1), height: self.height.div_ceil(2).max(1), data: half_pixels(&self.data, self.width, self.height) }
    }
    /// The shared default Gaussian.
    pub fn gaussian(&self, sigma: f32) -> Self {
        Self { width: self.width, height: self.height, data: crate::blur::gaussian(&self.data, self.width, self.height, sigma) }
    }
}

impl Frame<f32> {
    pub fn clamp01(&mut self) {
        self.map_in_place(|v| v.clamp(0.0, 1.0));
    }
}

impl Frame<[f32; 3]> {
    pub fn clamp01(&mut self) {
        self.map_in_place(|p| p.map(|v| v.clamp(0.0, 1.0)));
    }
    /// Opaque RGBA.
    pub fn to_rgba(&self) -> Frame<[f32; 4]> {
        self.map(|p| [p[0], p[1], p[2], 1.0])
    }
}

impl Frame<[f32; 4]> {
    pub fn clamp01(&mut self) {
        self.map_in_place(|p| p.map(|v| v.clamp(0.0, 1.0)));
    }
    /// Straight → premultiplied colour.
    pub fn premultiply(&mut self) {
        self.map_in_place(Pixel::premultiplied);
    }
    /// Premultiplied → straight colour (fully transparent pixels become zero).
    pub fn unpremultiply(&mut self) {
        self.map_in_place(Pixel::straight);
    }
    /// Composite straight colour over an opaque background.
    pub fn over(&self, bg: [f32; 3]) -> Frame<[f32; 3]> {
        self.map(|p| {
            let a = p[3].clamp(0.0, 1.0);
            [0, 1, 2].map(|k| p[k].clamp(0.0, 1.0) * a + bg[k] * (1.0 - a))
        })
    }
    /// Colour channels only (alpha dropped).
    pub fn rgb(&self) -> Frame<[f32; 3]> {
        self.map(|p| [p[0], p[1], p[2]])
    }
    pub fn alpha(&self) -> Frame<f32> {
        self.map(|p| p[3])
    }
}

/// `image` crate adapters: exact `u8 / 255` and `u16 / 65535` on the way in, one rounding on
/// the way out.
mod image_io {
    use super::{Frame, quantize_u8, quantize_u16};
    use image::{DynamicImage, GrayImage, ImageBuffer, Luma, Rgb, RgbImage, Rgba, RgbaImage};

    // Division (not a reciprocal multiply): bit-identical to the previous `v as f32 / 255.0`.
    const U8: f32 = 255.0;
    const U16: f32 = 65535.0;

    fn dims(w: u32, h: u32) -> (usize, usize) {
        (w as usize, h as usize)
    }

    impl Frame<f32> {
        pub fn from_gray8(img: &GrayImage) -> Self {
            let (w, h) = dims(img.width(), img.height());
            Self { width: w, height: h, data: img.as_raw().iter().map(|v| *v as f32 / U8).collect() }
        }
        pub fn to_gray8(&self) -> GrayImage {
            GrayImage::from_raw(self.width as u32, self.height as u32, self.data.iter().map(|v| quantize_u8(*v)).collect()).unwrap_or_default()
        }
    }

    impl Frame<[f32; 3]> {
        pub fn from_rgb8(img: &RgbImage) -> Self {
            let (w, h) = dims(img.width(), img.height());
            Self { width: w, height: h, data: img.as_raw().as_chunks::<3>().0.iter().map(|p| p.map(|v| v as f32 / U8)).collect() }
        }
        pub fn to_rgb8(&self) -> RgbImage {
            RgbImage::from_raw(self.width as u32, self.height as u32, self.data.iter().flat_map(|p| p.map(quantize_u8)).collect()).unwrap_or_default()
        }
        pub fn to_rgb16(&self) -> ImageBuffer<Rgb<u16>, Vec<u16>> {
            ImageBuffer::from_raw(self.width as u32, self.height as u32, self.data.iter().flat_map(|p| p.map(quantize_u16)).collect()).unwrap_or_default()
        }
    }

    impl Frame<[f32; 4]> {
        pub fn from_rgba8(img: &RgbaImage) -> Self {
            let (w, h) = dims(img.width(), img.height());
            Self { width: w, height: h, data: img.as_raw().as_chunks::<4>().0.iter().map(|p| p.map(|v| v as f32 / U8)).collect() }
        }
        pub fn to_rgba8(&self) -> RgbaImage {
            RgbaImage::from_raw(self.width as u32, self.height as u32, self.data.iter().flat_map(|p| p.map(quantize_u8)).collect()).unwrap_or_default()
        }
        pub fn to_rgba16(&self) -> ImageBuffer<Rgba<u16>, Vec<u16>> {
            ImageBuffer::from_raw(self.width as u32, self.height as u32, self.data.iter().flat_map(|p| p.map(quantize_u16)).collect()).unwrap_or_default()
        }
        /// Any decoded image at its own precision: 8-bit, 16-bit and float sources keep every
        /// bit (grey expands to RGB; missing alpha is opaque).
        pub fn from_dynamic(img: &DynamicImage) -> Self {
            let (w, h) = dims(img.width(), img.height());
            let data: Vec<[f32; 4]> = match img {
                DynamicImage::ImageRgba8(i) => return Self::from_rgba8(i),
                DynamicImage::ImageRgb8(i) => i.pixels().map(|Rgb(p)| [p[0] as f32 / U8, p[1] as f32 / U8, p[2] as f32 / U8, 1.0]).collect(),
                DynamicImage::ImageLuma8(i) => i.pixels().map(|Luma([v])| [*v as f32 / U8, *v as f32 / U8, *v as f32 / U8, 1.0]).collect(),
                DynamicImage::ImageLumaA8(i) => i.pixels().map(|p| [p[0] as f32 / U8, p[0] as f32 / U8, p[0] as f32 / U8, p[1] as f32 / U8]).collect(),
                DynamicImage::ImageRgba16(i) => i.pixels().map(|Rgba(p)| p.map(|v| v as f32 / U16)).collect(),
                DynamicImage::ImageRgb16(i) => i.pixels().map(|Rgb(p)| [p[0] as f32 / U16, p[1] as f32 / U16, p[2] as f32 / U16, 1.0]).collect(),
                DynamicImage::ImageLuma16(i) => i.pixels().map(|Luma([v])| [*v as f32 / U16, *v as f32 / U16, *v as f32 / U16, 1.0]).collect(),
                DynamicImage::ImageLumaA16(i) => i.pixels().map(|p| [p[0] as f32 / U16, p[0] as f32 / U16, p[0] as f32 / U16, p[1] as f32 / U16]).collect(),
                DynamicImage::ImageRgba32F(i) => i.pixels().map(|Rgba(p)| *p).collect(),
                DynamicImage::ImageRgb32F(i) => i.pixels().map(|Rgb(p)| [p[0], p[1], p[2], 1.0]).collect(),
                other => other.to_rgba32f().pixels().map(|Rgba(p)| *p).collect(),
            };
            Self { width: w, height: h, data }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn j28_frame_edges_match_the_editor_policy() {
        let f = Frame::from_fn(3, 2, |x, y| (y * 3 + x) as f32);
        assert_eq!(f.get_edge(-1, 0, Edge::Transparent), 0.0);
        assert_eq!(f.get_edge(-1, 5, Edge::Repeat), 3.0);
        assert_eq!(f.get_edge(-1, -1, Edge::Wrap), 5.0);
        assert_eq!(f.get_edge(4, 1, Edge::Wrap), 4.0);
        assert_eq!(Edge::Repeat.resolve(0, 0, 0, 0, 0, 0), None);
    }

    #[test]
    fn j28_frame_u8_u16_round_trips_are_exact() {
        let rgba = image::RgbaImage::from_fn(17, 5, |x, y| image::Rgba([(x * 15) as u8, (y * 50) as u8, (x * y) as u8, 255 - x as u8]));
        assert_eq!(Frame::from_rgba8(&rgba).to_rgba8(), rgba);
        let rgb = image::DynamicImage::ImageRgba8(rgba.clone()).to_rgb8();
        assert_eq!(Frame::from_rgb8(&rgb).to_rgb8(), rgb);
        let gray = image::GrayImage::from_fn(9, 4, |x, y| image::Luma([(x * 28 + y) as u8]));
        assert_eq!(Frame::from_gray8(&gray).to_gray8(), gray);
        let deep =
            image::ImageBuffer::<image::Rgba<u16>, Vec<u16>>::from_fn(7, 3, |x, y| image::Rgba([x as u16 * 9000 + 1, y as u16 * 20000 + 3, 65535, 32769]));
        let f = Frame::from_dynamic(&image::DynamicImage::ImageRgba16(deep.clone()));
        assert_eq!(f.to_rgba16(), deep, "16-bit sources keep every bit");
        assert_eq!(quantize_u8(f32::NAN), 0);
    }

    #[test]
    fn j28_frame_resize_crop_paste_and_over() {
        let f = Frame::from_fn(8, 6, |x, y| [x as f32 / 7.0, y as f32 / 5.0, 0.5, 1.0]);
        let r = f.resize(16, 12, Filter::Lanczos3);
        assert_eq!(r.dimensions(), (16, 12));
        let mut c = Frame::filled(4, 4, [0.0f32; 4]);
        c.paste(&f.crop(2, 1, 3, 3), 1, 1);
        assert_eq!(c.get(1, 1), f.get(2, 1));
        assert_eq!(c.get(3, 3), f.get(4, 3));
        let half = Frame::filled(1, 1, [0.2f32, 0.4, 0.6, 0.5]).over([1.0; 3]);
        assert!(half.get(0, 0).iter().zip([0.6, 0.7, 0.8]).all(|(a, b)| (a - b).abs() < 1e-6));
    }
}
