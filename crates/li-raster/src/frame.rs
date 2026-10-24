//! One dense, row-major pixel frame for every float pipeline region and AI boundary.
//!
//! Tiled copy-on-write documents stay in `pc-raster::Surface`; this is the *dense* family only.
//! Samples are normalized (`0..=1` for encoded colour); quantization to 8 bits happens only at
//! wires that need it (PNG for ComfyUI, 8-bit previews), through [`quantize_u8`].

use lightcraft_geom::Orientation;

use crate::blur::GaussianPolicy;
use crate::par_rows;
use crate::resample::{Filter, Pixel, antiring, antiring_taper, half_pixels, resize_antiring, resize_pixels};

/// Widest kernel of [`Frame::sample_cubic`]: minification beyond this is a prefilter's job.
pub const CUBIC_MAX_STRETCH: f32 = 4.0;
/// Most taps per axis of [`Frame::sample_cubic`] (support ±2 at [`CUBIC_MAX_STRETCH`]).
const CUBIC_TAPS: usize = 17;

/// One axis of [`Frame::sample_cubic`] at continuous coordinate `x` (pixel centres at +0.5) over `n`
/// pixels.
struct CubicAxis {
    /// First source index and tap count.
    j0: isize,
    n: usize,
    /// Normalized Catmull-Rom weights.
    w: [f32; CUBIC_TAPS],
    /// How far each tap counts toward the ringing clamp's range ([`crate::resample::antiring_taper`]).
    taper: [f32; CUBIC_TAPS],
}

#[inline]
fn cubic_axis(x: f32, n: usize, stretch: f32) -> CubicAxis {
    let s = if stretch.is_finite() { stretch.clamp(1.0, CUBIC_MAX_STRETCH) } else { 1.0 };
    // A coordinate held past the edge reads the edge, as in `sample_bilinear`.
    let f = x.clamp(-1.0, n as f32 + 1.0) - 0.5;
    let j0 = (f - 2.0 * s).floor() + 1.0;
    let count = (((f + 2.0 * s).ceil() - 1.0 - j0) as usize + 1).min(CUBIC_TAPS);
    let mut a = CubicAxis { j0: j0 as isize, n: count, w: [0.0; CUBIC_TAPS], taper: [0.0; CUBIC_TAPS] };
    let mut sum = 0.0f32;
    for k in 0..count {
        a.w[k] = Filter::CatmullRom.eval((j0 + k as f32 - f) / s);
        sum += a.w[k];
    }
    let mut w_max = 0.0f32;
    for k in 0..count {
        a.w[k] /= sum;
        w_max = w_max.max(a.w[k]);
    }
    for k in 0..count {
        a.taper[k] = antiring_taper(a.w[k], w_max);
    }
    a
}

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

/// Float RGB (alpha already composited): normalized encoded colour at AI/preview wires, linear
/// scene-referred colour in Develop.
pub type RgbFrame = Frame<[f32; 3]>;
/// Straight (unpremultiplied) RGBA, normalized.
pub type RgbaFrame = Frame<[f32; 4]>;
/// One float channel (masks, mattes, luminance).
pub type GrayFrame = Frame<f32>;
/// 8-bit display-encoded RGBA (previews, thumbnails, histograms).
pub type Rgba8 = Frame<[u8; 4]>;

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
    /// [`Self::crop`] in place: rows move to the front of the same buffer (no second frame).
    pub fn into_crop(mut self, x0: usize, y0: usize, w: usize, h: usize) -> Self {
        let x0 = x0.min(self.width);
        let y0 = y0.min(self.height);
        let w = w.min(self.width - x0);
        let h = h.min(self.height - y0);
        for (i, y) in (y0..y0 + h).enumerate() {
            let src = y * self.width + x0;
            self.data.copy_within(src..src + w, i * w);
        }
        self.data.truncate(w * h);
        Self { width: w, height: h, data: self.data }
    }
    /// Rotate 90° clockwise.
    pub fn rotate_cw(&self) -> Self {
        let (w, h) = (self.width, self.height);
        Self::from_fn(h, w, |x, y| self.get(y, h - 1 - x))
    }
    pub fn rotate_180(&self) -> Self {
        let mut d = self.data.clone();
        d.reverse();
        Self { width: self.width, height: self.height, data: d }
    }
    pub fn flip_h(&self) -> Self {
        let mut out = self.clone();
        for y in 0..self.height {
            out.row_mut(y).reverse();
        }
        out
    }
    /// [`Self::oriented`] without a copy when `o` is the identity.
    pub fn into_oriented(self, o: Orientation) -> Self {
        if o == Orientation::Normal { self } else { self.oriented(o) }
    }
    /// Apply an EXIF orientation (stored → displayed).
    pub fn oriented(&self, o: Orientation) -> Self {
        let (flip, turns) = o.to_parts();
        let mut img = if flip { self.flip_h() } else { self.clone() };
        match turns {
            1 => img = img.rotate_cw(),
            2 => img = img.rotate_180(),
            3 => img = img.rotate_180().rotate_cw(),
            _ => {}
        }
        img
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
    /// Fit within `max_w × max_h` preserving aspect (never upscales).
    pub fn fit(&self, max_w: usize, max_h: usize, filter: Filter) -> Self {
        let s = (max_w as f64 / self.width as f64).min(max_h as f64 / self.height as f64).min(1.0);
        let w = ((self.width as f64 * s).round() as usize).max(1);
        let h = ((self.height as f64 * s).round() as usize).max(1);
        self.resize(w, h, filter)
    }
    /// 2× box reduction.
    pub fn half(&self) -> Self {
        Self { width: self.width.div_ceil(2).max(1), height: self.height.div_ceil(2).max(1), data: half_pixels(&self.data, self.width, self.height) }
    }
    /// The shared default Gaussian: exact at sigma <= 4, three running-sum boxes above it.
    /// Small sigmas produce a real blur (#706).
    pub fn gaussian(&self, sigma: f32) -> Self {
        Self { width: self.width, height: self.height, data: crate::blur::gaussian(&self.data, self.width, self.height, sigma) }
    }
    /// Gaussian with a pinned numerical policy (the default [`Self::gaussian`] retains the
    /// sampled small-radius and premultiplied-alpha contracts).
    pub fn gaussian_with_policy(&self, sigma: f32, policy: GaussianPolicy) -> Self {
        Self { width: self.width, height: self.height, data: crate::blur::gaussian_with_policy(&self.data, self.width, self.height, sigma, policy) }
    }
    /// Variance-corrected fine Gaussian, retaining the Develop GPU twin's tap order.
    pub fn gaussian_fine(&self, sigma: f32) -> Self {
        Self { width: self.width, height: self.height, data: crate::blur::gaussian_fine(&self.data, self.width, self.height, sigma) }
    }
}

impl Frame<f32> {
    pub fn clamp01(&mut self) {
        self.map_in_place(|v| v.clamp(0.0, 1.0));
    }
    /// Square minimum with replicated borders.
    pub fn min_filter(&self, r: usize) -> Self {
        Self { width: self.width, height: self.height, data: crate::blur::extrema_filter(&self.data, self.width, self.height, r, false) }
    }
    /// Square maximum with replicated borders.
    pub fn max_filter(&self, r: usize) -> Self {
        Self { width: self.width, height: self.height, data: crate::blur::extrema_filter(&self.data, self.width, self.height, r, true) }
    }
}

impl Frame<[f32; 3]> {
    pub fn clamp01(&mut self) {
        self.map_in_place(|p| p.map(|v| v.clamp(0.0, 1.0)));
    }
    /// Separable resample of scene-linear light through precomputed taps (`wx` per output column,
    /// `wy` per output row), with ringing suppressed: [`crate::resample::resize_antiring`].
    pub fn resample_antiring(&self, wx: &[(usize, Vec<f32>)], wy: &[(usize, Vec<f32>)], slack: f32) -> Self {
        Self { width: wx.len(), height: wy.len(), data: resize_antiring(&self.data, self.width, self.height, wx, wy, slack) }
    }
    /// Bilinear sample at continuous pixel coordinates (pixel centres at +0.5).
    #[inline]
    pub fn sample_bilinear(&self, x: f32, y: f32) -> [f32; 3] {
        // a pixel past the edge reads the edge pixel, so a coordinate held a pixel outside gives
        // the same sample; an infinite one (a crop of no size's inverse mapping) then reads the
        // edge too, instead of a NaN weight and a neighbour index past isize::MAX
        let fx = x.clamp(-1.0, self.width as f32 + 1.0) - 0.5;
        let fy = y.clamp(-1.0, self.height as f32 + 1.0) - 0.5;
        let x0 = fx.floor();
        let y0 = fy.floor();
        let tx = fx - x0;
        let ty = fy - y0;
        let (x0, y0) = (x0 as isize, y0 as isize);
        let a = self.get_clamped(x0, y0);
        let b = self.get_clamped(x0 + 1, y0);
        let c = self.get_clamped(x0, y0 + 1);
        let d = self.get_clamped(x0 + 1, y0 + 1);
        std::array::from_fn(|i| {
            let top = a[i] + (b[i] - a[i]) * tx;
            let bot = c[i] + (d[i] - c[i]) * tx;
            top + (bot - top) * ty
        })
    }
    /// Catmull-Rom sample at continuous pixel coordinates (pixel centres at +0.5, clamped edges) for
    /// scene-linear light. `stretch` widens the kernel per axis (1 to [`CUBIC_MAX_STRETCH`]) where
    /// one output pixel spans that many source pixels, so a minifying map does not alias. Ringing at
    /// high-contrast edges is suppressed ([`crate::resample::antiring`] with `slack`) to the range
    /// of the taps in the kernel's positive lobe: the 2×2 nearest at stretch 1.
    pub fn sample_cubic(&self, x: f32, y: f32, stretch: [f32; 2], slack: f32) -> [f32; 3] {
        let (ax, ay) = (cubic_axis(x, self.width, stretch[0]), cubic_axis(y, self.height, stretch[1]));
        let at = |kx: usize, ky: usize| self.get_clamped(ax.j0 + kx as isize, ay.j0 + ky as isize);
        let (mut acc, mut lobe, mut lobe_w) = ([0.0f32; 3], [0.0f32; 3], 0.0f32);
        for ky in 0..ay.n {
            let (mut row, mut lobe_row, mut lobe_row_w) = ([0.0f32; 3], [0.0f32; 3], 0.0f32);
            for kx in 0..ax.n {
                let (p, c) = (at(kx, ky), ax.w[kx] * ax.taper[kx]);
                for ch in 0..3 {
                    row[ch] += p[ch] * ax.w[kx];
                    lobe_row[ch] += p[ch] * c;
                }
                lobe_row_w += c;
            }
            let c = ay.w[ky] * ay.taper[ky];
            for ch in 0..3 {
                acc[ch] += row[ch] * ay.w[ky];
                lobe[ch] += lobe_row[ch] * c;
            }
            lobe_w += lobe_row_w * c;
        }
        if lobe_w <= 0.0 {
            return acc;
        }
        let mean = lobe.map(|v| v / lobe_w);
        let mut core = [(0.0f32, 0.0f32); 3];
        for ky in 0..ay.n {
            for kx in 0..ax.n {
                let t = ax.taper[kx] * ay.taper[ky];
                if t > 0.0 {
                    let p = at(kx, ky);
                    for ch in 0..3 {
                        let d = (p[ch] - mean[ch]) * t;
                        core[ch] = (core[ch].0.min(d), core[ch].1.max(d));
                    }
                }
            }
        }
        std::array::from_fn(|ch| antiring(acc[ch], (mean[ch] + core[ch].0, mean[ch] + core[ch].1), slack))
    }
    /// Rec.2020 luminance of linear colour.
    pub fn luminance(&self) -> Frame<f32> {
        self.map(li_color::luminance_2020)
    }
    /// Encode to 8-bit sRGB from linear sRGB values (no gamut conversion).
    pub fn to_srgb8(&self) -> Rgba8 {
        use li_color::transfer::encode_srgb8;
        self.map(|p| [encode_srgb8(p[0]), encode_srgb8(p[1]), encode_srgb8(p[2]), 255])
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

impl Rgba8 {
    /// Raw bytes (RGBA, row-major).
    pub fn as_bytes(&self) -> Vec<u8> {
        self.data.iter().flat_map(|p| *p).collect()
    }
    pub fn from_bytes(width: usize, height: usize, bytes: &[u8]) -> Option<Self> {
        if bytes.len() != width * height * 4 {
            return None;
        }
        Some(Self { width, height, data: bytes.as_chunks::<4>().0.iter().map(|c| [c[0], c[1], c[2], c[3]]).collect() })
    }
    /// Linear Rec.709/sRGB primaries float frame from 8-bit sRGB.
    pub fn to_linear(&self) -> Frame<[f32; 3]> {
        use li_color::transfer::decode_srgb8;
        self.map(|p| [decode_srgb8(p[0]), decode_srgb8(p[1]), decode_srgb8(p[2])])
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
    fn frame_edges_match_the_editor_policy() {
        let f = Frame::from_fn(3, 2, |x, y| (y * 3 + x) as f32);
        assert_eq!(f.get_edge(-1, 0, Edge::Transparent), 0.0);
        assert_eq!(f.get_edge(-1, 5, Edge::Repeat), 3.0);
        assert_eq!(f.get_edge(-1, -1, Edge::Wrap), 5.0);
        assert_eq!(f.get_edge(4, 1, Edge::Wrap), 4.0);
        assert_eq!(Edge::Repeat.resolve(0, 0, 0, 0, 0, 0), None);
    }

    #[test]
    fn frame_u8_u16_round_trips_are_exact() {
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
    fn frame_resize_crop_paste_and_over() {
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

    // Formerly `lightcraft-raster`'s `Image` tests (that type folded into `Frame`).
    use lightcraft_geom::Orientation;

    #[test]
    fn crop_and_rotate() {
        let img = Frame::<u32>::from_fn(4, 3, |x, y| (y * 10 + x) as u32);
        let c = img.crop(1, 1, 2, 2);
        assert_eq!(c.data, vec![11, 12, 21, 22]);
        // the in-place variants equal the copying ones
        for (x, y, w, h) in [(1, 1, 2, 2), (0, 0, 4, 3), (2, 0, 9, 9), (0, 2, 4, 1), (3, 2, 1, 1)] {
            assert_eq!(img.clone().into_crop(x, y, w, h), img.crop(x, y, w, h));
        }
        for v in 1..=8 {
            let o = Orientation::from_exif(v);
            assert_eq!(img.clone().into_oriented(o), img.oriented(o));
        }
        let mut m = img.clone();
        m.map_in_place(|p| p * 2 + 1);
        assert_eq!(m, img.map(|p| p * 2 + 1));
        let r = img.rotate_cw();
        assert_eq!((r.width, r.height), (3, 4));
        // top-left of rotated = bottom-left of original
        assert_eq!(r.get(0, 0), 20);
        assert_eq!(r.get(2, 0), 0);
        assert_eq!(img.rotate_cw().rotate_cw().rotate_cw().rotate_cw(), img);
        assert_eq!(img.rotate_cw().rotate_cw(), img.rotate_180());
    }

    #[test]
    fn orientation_matches_geom_mapping() {
        let img = Frame::<u32>::from_fn(5, 3, |x, y| (y * 10 + x) as u32);
        for v in 1..=8 {
            let o = Orientation::from_exif(v);
            let out = img.oriented(o);
            for (sx, sy) in [(0usize, 0usize), (4, 0), (2, 1), (0, 2)] {
                let (dx, dy) = o.map(sx as f64 + 0.5, sy as f64 + 0.5, 5.0, 3.0);
                assert_eq!(out.get(dx as usize, dy as usize), img.get(sx, sy), "orientation {v}");
            }
        }
    }

    #[test]
    fn bilinear_at_centres_is_exact() {
        let img = RgbFrame::from_fn(3, 3, |x, y| [x as f32, y as f32, 1.0]);
        assert_eq!(img.sample_bilinear(1.5, 2.5), [1.0, 2.0, 1.0]);
        let m = img.sample_bilinear(1.0, 1.5);
        assert!((m[0] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn bilinear_samples_at_infinite_coordinates_read_the_edge() {
        let img = RgbFrame::from_fn(3, 2, |x, y| [x as f32, y as f32, 1.0]);
        assert_eq!(img.sample_bilinear(f32::INFINITY, f32::INFINITY), [2.0, 1.0, 1.0]);
        assert_eq!(img.sample_bilinear(f32::MAX, 0.0), [2.0, 0.0, 1.0]);
        assert_eq!(img.sample_bilinear(f32::NEG_INFINITY, f32::NEG_INFINITY), [0.0, 0.0, 1.0]);
        assert_eq!(img.sample_bilinear(1.5, 0.5), [1.0, 0.0, 1.0], "an ordinary sample is as before");
    }

    #[test]
    fn srgb8_roundtrip() {
        let img = Rgba8::from_fn(16, 16, |x, y| [(x * 16) as u8, (y * 16) as u8, 7, 255]);
        assert_eq!(img.to_linear().to_srgb8(), img);
        assert_eq!(Rgba8::from_bytes(16, 16, &img.as_bytes()), Some(img));
    }

    #[test]
    fn fine_gaussian_keeps_variance_and_mass_at_small_sigma() {
        for sigma in [0.3f32, 0.5, 0.8, 1.2, 1.9] {
            let mut imp = Frame::<f32>::new(41, 1);
            imp.set(20, 0, 1.0);
            let b = imp.gaussian_fine(sigma);
            let sum: f32 = b.data.iter().sum();
            let var: f32 = b.data.iter().enumerate().map(|(i, v)| v * (i as f32 - 20.0).powi(2)).sum();
            assert!((sum - 1.0).abs() < 1e-5, "{sigma}: {sum}");
            assert!((var.sqrt() - sigma).abs() / sigma < 0.05, "{sigma}: {}", var.sqrt());
        }
        // Both shared policies now blur at 0.5 px (#706).
        let mut imp = Frame::<f32>::new(9, 9);
        imp.set(4, 4, 1.0);
        assert!(imp.gaussian(0.5).get(4, 4) < 1.0);
        assert!(imp.gaussian_fine(0.5).get(4, 4) < 0.6);
    }

    #[test]
    fn min_filter_takes_the_window_minimum() {
        let img = Frame::<f32>::from_fn(20, 10, |x, y| ((x * 3 + y * 7) % 11) as f32);
        let m = img.min_filter(2);
        for y in 0..10usize {
            for x in 0..20usize {
                let mut want = f32::INFINITY;
                for yy in y.saturating_sub(2)..=(y + 2).min(9) {
                    for xx in x.saturating_sub(2)..=(x + 2).min(19) {
                        want = want.min(img.get(xx, yy));
                    }
                }
                assert_eq!(m.get(x, y), want);
            }
        }
        let image = Frame::from_fn(3, 2, |x, y| (x + 3 * y) as f32);
        assert_eq!(image.min_filter(usize::MAX), Frame::filled(3, 2, 0.));
    }

    #[test]
    fn gaussian_preserves_constant_mass_and_variance() {
        let img = Frame::<f32>::filled(50, 30, 0.7);
        let b = img.gaussian(4.0);
        assert!(b.data.iter().all(|v| (v - 0.7).abs() < 1e-5));
        let mut imp = Frame::<f32>::new(101, 101);
        imp.set(50, 50, 1.0);
        let b = imp.gaussian(5.0);
        let sum: f32 = b.data.iter().sum();
        assert!((sum - 1.0).abs() < 1e-3, "{sum}");
        // peak moved to the centre and symmetric
        assert!((b.get(45, 50) - b.get(55, 50)).abs() < 1e-6);
        assert!((b.get(50, 45) - b.get(50, 55)).abs() < 1e-6);
        let mut imp = Frame::<f32>::new(201, 1);
        imp.set(100, 0, 1.0);
        let sigma = 8.0;
        let b = imp.gaussian(sigma);
        let var: f32 = b.data.iter().enumerate().map(|(i, v)| v * ((i as f32 - 100.0).powi(2))).sum();
        assert!((var.sqrt() - sigma).abs() / sigma < 0.1, "{}", var.sqrt());
    }

    #[test]
    fn d2_gaussian_small_sigma_and_alpha() {
        let img = Frame::from_fn(3, 1, |x, _| if x == 0 { [1.0, 0.0, 0.0, 1.0] } else { [0.0, 0.0, 1.0, 0.0] });
        let actual = img.gaussian(0.5);
        assert!(actual.get(0, 0)[3] < 1.0);
        assert!(actual.get(1, 0)[3] > 0.0);
        assert_eq!(actual.get(1, 0)[0], 1.0);
        assert_eq!(actual.get(1, 0)[2], 0.0);
        assert_eq!(actual.data, crate::blur::gaussian(&img.data, 3, 1, 0.5));
    }

    #[test]
    fn resize_keeps_constants_and_mean() {
        let img = Frame::<f32>::filled(37, 21, 0.42);
        for f in [Filter::Box, Filter::Bilinear, Filter::Mitchell, Filter::Lanczos3] {
            for (w, h) in [(10, 7), (80, 40), (37, 21), (1, 1)] {
                let r = img.resize(w, h, f);
                assert!(r.data.iter().all(|v| (v - 0.42).abs() < 1e-5), "{f:?} {w}x{h}");
            }
        }
        let img = Frame::<f32>::from_fn(64, 64, |x, y| ((x * 7 + y * 3) % 11) as f32 / 10.0);
        let mean = img.data.iter().sum::<f32>() / img.len() as f32;
        let r = img.resize(16, 16, Filter::Lanczos3);
        let m2 = r.data.iter().sum::<f32>() / r.len() as f32;
        assert!((mean - m2).abs() < 0.01);
        let hh = img.half();
        assert_eq!((hh.width, hh.height), (32, 32));
        let m3 = hh.data.iter().sum::<f32>() / hh.len() as f32;
        assert!((mean - m3).abs() < 1e-5);
    }

    #[test]
    fn fit_keeps_aspect() {
        let img = Frame::<f32>::new(4000, 3000);
        let r = img.fit(400, 400, Filter::Box);
        assert_eq!((r.width, r.height), (400, 300));
        let small = Frame::<f32>::new(10, 5).fit(400, 400, Filter::Box);
        assert_eq!((small.width, small.height), (10, 5));
    }

    #[test]
    fn resample_adapter_byte_goldens() {
        for (f, expected) in [Filter::Box, Filter::Bilinear, Filter::Mitchell, Filter::Lanczos3].into_iter().zip([
            16937065032499866148,
            13728202162992833757,
            1960567902461757162,
            495902503799777730,
        ]) {
            let s = Frame::<[f32; 3]>::from_fn(9, 7, |x, y| {
                [((x + y * 9) * 17 % 255) as f32 / 255.0, ((x + y * 9) * 31 % 255) as f32 / 255.0, ((x + y * 9) * 7 % 255) as f32 / 255.0]
            });
            let o = s.resize(7, 10, f);
            let data: Vec<_> = o.data.into_iter().flatten().collect();
            let hash = data.iter().flat_map(|v| v.to_bits().to_le_bytes()).fold(14695981039346656037u64, |h, b| (h ^ u64::from(b)).wrapping_mul(1099511628211));
            assert_eq!(hash, expected, "{f:?}");
        }
    }

    #[test]
    fn final_dense_alpha_half_nearest_and_empty() {
        let img = Frame::from_fn(3, 1, |x, _| if x == 0 { [1.0, 0.2, 0.0, 1.0] } else { [0.0, 0.0, 1.0, 0.0] });
        for got in [img.resize(7, 1, Filter::Bilinear), img.half()] {
            for p in got.data {
                if p[3] > 1e-7 {
                    assert!((p[0] - 1.0).abs() < 1e-6 && (p[1] - 0.2).abs() < 1e-6 && p[2] == 0.0);
                }
            }
        }
        let ramp = Frame::<f32>::from_fn(9, 3, |x, y| (x + y * 9) as f32);
        let got = ramp.resize(3, 1, Filter::Nearest);
        assert_eq!(crate::resample::weights(9, 3, Filter::Nearest), vec![(1, vec![1.0]), (4, vec![1.0]), (7, vec![1.0])]);
        assert_eq!(got.data, vec![10.0, 13.0, 16.0]);
        assert_eq!(Frame::<f32>::new(0, 3).half(), Frame::<f32>::new(1, 2));
        assert_eq!(Frame::<f32>::new(0, 3).resize(2, 1, Filter::Lanczos3), Frame::<f32>::new(2, 1));
    }
}
