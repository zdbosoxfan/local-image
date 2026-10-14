//! High-quality separable resampling; shared kernels with tile-aware Surface adapters.
pub use li_raster::resample::Resample;
use photocraft_geom::{Rect, TILE_SIZE};
use photocraft_raster::Surface;
fn region(r: Rect) -> li_raster::Region {
    li_raster::Region::new(r.x0, r.y0, r.x1, r.y1)
}
fn rect(r: li_raster::Region) -> Rect {
    Rect::new(r.x0, r.y0, r.x1, r.y1)
}
struct Source<'a>(&'a Surface);
impl li_raster::resample::Source for Source<'_> {
    fn channels(&self) -> usize {
        self.0.channels()
    }
    fn alpha(&self) -> bool {
        self.0.format().alpha
    }
    fn default_pixel(&self) -> Vec<f32> {
        self.0.default_pixel()
    }
    fn content_bounds(&self) -> li_raster::Region {
        region(self.0.content_bounds())
    }
    fn tile_size(&self) -> i32 {
        TILE_SIZE
    }
    fn has_tiles_in(&self, r: li_raster::Region) -> bool {
        self.0.has_tiles_in(rect(r))
    }
    fn read_region_into(&self, r: li_raster::Region, out: &mut Vec<f32>) {
        self.0.read_region_into(rect(r), out);
    }
}
pub fn scaled_rect(r: Rect, sx: f64, sy: f64) -> Rect {
    rect(li_raster::resample::scaled_rect(region(r), sx, sy))
}
pub fn resize_surface(s: &Surface, sx: f64, sy: f64, filter: Resample) -> Surface {
    resize(s, sx, sy, filter, None)
}
pub fn resize_surface_in_canvas(s: &Surface, sx: f64, sy: f64, filter: Resample, canvas: Rect) -> Surface {
    let area = s.content_bounds().union(&canvas);
    resize(s, sx, sy, filter, (!area.is_empty()).then_some(area))
}
const READ_BUDGET: usize = 1 << 24;
fn resize(s: &Surface, sx: f64, sy: f64, filter: Resample, edge: Option<Rect>) -> Surface {
    resize_with_budget(s, sx, sy, filter, edge, READ_BUDGET)
}
fn resize_with_budget(s: &Surface, sx: f64, sy: f64, filter: Resample, edge: Option<Rect>, budget: usize) -> Surface {
    let mut out = Surface::with_default(s.format(), &s.default_pixel());
    if !sx.is_finite() || !sy.is_finite() || sx <= 0.0 || sy <= 0.0 {
        return out;
    }
    for (r, d) in li_raster::resample::resize_source(&Source(s), sx, sy, filter, edge.map(region), budget) {
        out.write_region(rect(r), &d);
    }
    let dst = scaled_rect(s.content_bounds(), sx, sy);
    if filter == Resample::PreserveDetails && (sx > 1.0 || sy > 1.0) && !dst.is_empty() {
        let p = crate::FilterParams::UnsharpMask { amount: 30.0, radius: 0.6 * sx.max(sy) as f32, threshold: 0.0 };
        let area = crate::output_area(&p, out.content_bounds(), dst, None);
        out = crate::apply(&out, &p, area, dst, None);
    }
    out.prune();
    out
}
/// Moves a surface by whole pixels (default pixel kept).
pub fn translate_surface(s: &Surface, dx: i32, dy: i32) -> Surface {
    let mut out = Surface::with_default(s.format(), &s.default_pixel());
    let r = s.content_bounds();
    if !r.is_empty() {
        out.write_interleaved(r.translate(dx, dy), &s.to_interleaved(r));
    }
    out
}

/// Keeps only the pixels inside `keep` (others become the default pixel).
pub fn crop_surface(s: &Surface, keep: Rect) -> Surface {
    let mut out = Surface::with_default(s.format(), &s.default_pixel());
    let r = s.content_bounds().intersect(&keep);
    if !r.is_empty() {
        out.write_interleaved(r, &s.to_interleaved(r));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use photocraft_color::{ColorMode, PixelFormat, SampleType};

    fn ramp(sample: SampleType) -> Surface {
        let mut s = Surface::new(PixelFormat::new(ColorMode::Rgb, sample, true));
        let r = Rect::new(0, 0, 64, 32);
        let v: Vec<f32> = (0..32).flat_map(|_| (0..64).flat_map(|x| [x as f32 / 63.0, 0.5, 1.0 - x as f32 / 63.0, 1.0])).collect();
        s.write_region(r, &v);
        s
    }

    #[test]
    fn identity_scale_is_lossless() {
        for f in [Resample::Nearest, Resample::Bilinear, Resample::Bicubic, Resample::Lanczos] {
            let s = ramp(SampleType::U16);
            let o = resize_surface(&s, 1.0, 1.0, f);
            let r = Rect::new(0, 0, 64, 32);
            let (a, b) = (s.read_region(r), o.read_region(r));
            let d = a.iter().zip(&b).map(|(x, y)| (x - y).abs()).fold(0.0f32, f32::max);
            assert!(d < 1e-4, "{f:?} {d}");
        }
    }

    #[test]
    fn half_and_double_sizes() {
        for f in [Resample::Nearest, Resample::Bilinear, Resample::Bicubic, Resample::Lanczos, Resample::PreserveDetails] {
            let s = ramp(SampleType::F32);
            let half = resize_surface(&s, 0.5, 0.5, f);
            assert_eq!(half.content_bounds(), Rect::new(0, 0, 32, 16), "{f:?}");
            let p = half.pixel(16, 8);
            assert!((p[1] - 0.5).abs() < 0.02 && (p[3] - 1.0).abs() < 1e-3, "{f:?} {p:?}");
            let dbl = resize_surface(&s, 2.0, 2.0, f);
            assert_eq!(dbl.content_bounds(), Rect::new(0, 0, 128, 64), "{f:?}");
            // Mid-gray channel and the ramp's monotonicity are preserved.
            let row: Vec<f32> = (8..120).map(|x| dbl.pixel(x, 30)[0]).collect();
            assert!(row.windows(2).all(|w| w[1] >= w[0] - 0.02), "{f:?}");
        }
    }

    #[test]
    fn downscale_averages_checkerboard() {
        let mut s = Surface::new(PixelFormat::RGBA8);
        for y in 0..64 {
            for x in 0..64 {
                let v = ((x + y) % 2) as f32;
                s.write_pixel(x, y, &[v, v, v, 1.0]);
            }
        }
        let o = resize_surface(&s, 0.25, 0.25, Resample::Bicubic);
        let p = o.pixel(8, 8);
        assert!((p[0] - 0.5).abs() < 0.05, "{p:?}");
    }

    #[test]
    fn transparent_edges_do_not_darken() {
        let mut s = Surface::new(PixelFormat::RGBA32F);
        s.fill_rect(Rect::new(10, 10, 20, 20), &[1.0, 0.0, 0.0, 1.0]);
        let o = resize_surface(&s, 1.7, 1.7, Resample::Lanczos);
        let edge = o.pixel(17, 25);
        assert!(edge[3] > 0.0 && (edge[0] - 1.0).abs() < 0.05 && edge[1].abs() < 0.05, "{edge:?}");
    }

    /// Image Size of a layer that fills the canvas: its edges stay opaque and keep their colour,
    /// up or down, at every depth; a layer away from the edges keeps its soft border.
    #[test]
    fn canvas_edges_repeat_instead_of_fading() {
        let canvas = Rect::new(0, 0, 30, 20);
        let colour = [0.2, 0.6, 1.0, 1.0];
        let filters = [Resample::Nearest, Resample::Bilinear, Resample::Bicubic, Resample::Lanczos, Resample::PreserveDetails];
        for sample in [SampleType::U8, SampleType::U16, SampleType::F32] {
            let fmt = PixelFormat::new(ColorMode::Rgb, sample, true);
            let mut s = Surface::new(fmt);
            s.fill_rect(canvas, &colour);
            for f in filters {
                for k in [2.0, 1.37, 0.5] {
                    let o = resize_surface_in_canvas(&s, k, k, f, canvas);
                    let dst = scaled_rect(canvas, k, k);
                    assert_eq!(o.content_bounds(), dst, "{sample:?} {f:?} {k}");
                    for (x, y) in [(0, 0), (dst.x1 - 1, 0), (0, dst.y1 - 1), (dst.x1 - 1, dst.y1 - 1), (dst.x1 / 2, 0), (0, dst.y1 / 2)] {
                        let p = o.pixel(x, y);
                        let off = p.iter().zip(colour).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
                        assert!(off < 0.01, "{sample:?} {f:?} {k} ({x},{y}): {p:?}");
                    }
                }
            }
            // Inside the canvas the default pixel is still what lies around a layer.
            let mut inner = Surface::new(fmt);
            inner.fill_rect(Rect::new(10, 5, 20, 15), &[1.0, 0.0, 0.0, 1.0]);
            let r = Rect::new(0, 0, 60, 40);
            let a = resize_surface_in_canvas(&inner, 1.7, 1.7, Resample::Lanczos, canvas);
            assert_eq!(a.read_region(r), resize_surface(&inner, 1.7, 1.7, Resample::Lanczos).read_region(r), "{sample:?}");
        }
    }

    /// Two tiles in opposite corners of a 100000 px square: the content bounds are huge but
    /// almost nothing is stored. Shrinking it to one pixel must neither allocate a window
    /// proportional to the scale (#713) nor read the empty middle.
    #[test]
    fn tiny_target_of_a_huge_sparse_source() {
        let mut s = Surface::new(PixelFormat::RGBA32F);
        s.fill_rect(Rect::new(0, 0, 256, 256), &[1.0, 1.0, 1.0, 1.0]);
        s.fill_rect(Rect::new(99_840, 99_840, 100_096, 100_096), &[1.0, 1.0, 1.0, 1.0]);
        let k = 1.0 / 100_096.0;
        let o = resize_surface(&s, k, k, Resample::Bicubic);
        assert_eq!(o.content_bounds(), Rect::new(0, 0, 1, 1));
        let p = o.pixel(0, 0);
        // Two 256² tiles out of 100096²: nearly transparent, and white where it isn't.
        assert!(p[3] >= 0.0 && p[3] < 1e-3 && p.iter().all(|v| v.is_finite()), "{p:?}");
        // Squashed to one row but kept wide: the whole height feeds one band of output rows,
        // which must not hold a full-height intermediate.
        let o = resize_surface(&s, 1000.0 / 100_096.0, k, Resample::Bicubic);
        let b = o.content_bounds();
        assert!(b.height() == 1 && (1000..=1001).contains(&b.width()), "{b:?}");
        assert!(o.pixel(0, 0)[3] > 0.0 && o.pixel(500, 0)[3] == 0.0, "{:?} {:?}", o.pixel(0, 0), o.pixel(500, 0));
    }

    /// Tile rows with nothing stored are not read; they count as rows of the default pixel,
    /// exactly as reading them would (and within rounding for a non-zero default).
    #[test]
    fn skipped_empty_rows_match_reading_them() {
        for (fmt, def, tol) in [(PixelFormat::RGBA32F, vec![0.0, 0.0, 0.0, 0.0], 0.0), (PixelFormat::GRAY8, vec![1.0], 1e-6)] {
            let mut sparse = Surface::with_default(fmt, &def);
            sparse.fill_rect(Rect::new(0, 0, 300, 40), &vec![0.5; def.len()]);
            sparse.fill_rect(Rect::new(100, 1500, 400, 1540), &vec![0.25; def.len()]);
            // The same pixels with the empty rows stored as tiles of the default pixel.
            let mut dense = sparse.clone();
            dense.fill_rect(Rect::new(0, 40, 400, 1500), &def);
            assert!(dense.tile_count() > sparse.tile_count());
            for f in [Resample::Bilinear, Resample::Bicubic, Resample::Lanczos] {
                for (kx, ky) in [(0.013, 0.013), (0.37, 0.37), (1.0, 0.01)] {
                    let (a, b) = (resize_surface(&sparse, kx, ky, f), resize_surface(&dense, kx, ky, f));
                    let r = a.content_bounds().union(&b.content_bounds());
                    let d = a.read_region(r).iter().zip(b.read_region(r)).map(|(x, y)| (x - y).abs()).fold(0.0f32, f32::max);
                    assert!(d <= tol, "{fmt:?} {f:?} {kx} {ky}: {d}");
                }
            }
        }
    }

    /// Reading the source in row chunks gives exactly what one read of the whole window gives.
    #[test]
    fn chunked_reads_match_a_single_read() {
        let mut s = ramp(SampleType::F32);
        s.fill_rect(Rect::new(600, 700, 640, 720), &[0.3, 0.9, 0.1, 0.5]);
        let mut mask = Surface::with_default(PixelFormat::GRAY8, &[1.0]);
        mask.fill_rect(Rect::new(0, 0, 40, 30), &[0.0]);
        mask.fill_rect(Rect::new(500, 900, 530, 940), &[0.25]);
        for src in [&s, &mask] {
            for f in [Resample::Nearest, Resample::Bilinear, Resample::Bicubic, Resample::Lanczos] {
                for k in [0.013, 0.37, 1.6] {
                    let whole = resize_with_budget(src, k, k, f, None, usize::MAX);
                    let chunked = resize_with_budget(src, k, k, f, None, 1);
                    let r = whole.content_bounds().union(&chunked.content_bounds());
                    assert_eq!(whole.read_region(r), chunked.read_region(r), "{f:?} {k}");
                }
            }
        }
    }

    #[test]
    fn masks_keep_default_pixel() {
        let fmt = PixelFormat::GRAY8;
        let mut m = Surface::with_default(fmt, &[1.0]);
        m.fill_rect(Rect::new(0, 0, 10, 10), &[0.0]);
        let o = resize_surface(&m, 2.0, 2.0, Resample::Bilinear);
        assert_eq!(o.default_pixel(), vec![1.0]);
        assert!(o.pixel(5, 5)[0] < 0.01);
    }

    #[test]
    fn translate_and_crop() {
        let s = ramp(SampleType::U8);
        let t = translate_surface(&s, 5, -3);
        assert_eq!(t.pixel(15, 7), s.pixel(10, 10));
        let c = crop_surface(&s, Rect::new(0, 0, 10, 10));
        assert_eq!(c.content_bounds(), Rect::new(0, 0, 10, 10));
    }
}

#[cfg(test)]
mod j01_golden {
    use super::*;
    #[test]
    fn j01_resample_adapter_byte_goldens() {
        for (f, expected) in [
            Resample::Nearest,
            Resample::Bilinear,
            Resample::Bicubic,
            Resample::Lanczos,
            Resample::PreserveDetails,
            Resample::BicubicSmoother,
            Resample::BicubicSharper,
        ]
        .into_iter()
        .zip([
            563068684180865877,
            5414249057966356400,
            5388069458813282607,
            11391420477039446530,
            16432942863708755085,
            9794917783319206443,
            10994775549111290735,
        ]) {
            let mut s = Surface::new(photocraft_color::PixelFormat::RGBA8);
            let r = Rect::new(-2, -1, 7, 6);
            let d: Vec<_> = (0..63)
                .flat_map(|i| [(i * 17 % 255) as f32 / 255.0, (i * 31 % 255) as f32 / 255.0, (i * 7 % 255) as f32 / 255.0, (i * 11 % 255) as f32 / 255.0])
                .collect();
            s.write_region(r, &d);
            let o = resize_surface_in_canvas(&s, 0.73, 1.37, f, r);
            let data = o.read_region(scaled_rect(r, 0.73, 1.37));
            let hash = data.iter().flat_map(|v| v.to_bits().to_le_bytes()).fold(14695981039346656037u64, |h, b| (h ^ u64::from(b)).wrapping_mul(1099511628211));
            assert_eq!(hash, expected, "{f:?}");
        }
    }
}
