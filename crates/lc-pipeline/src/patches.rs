//! AI Remove patches: pixels generated for a removal, stored (not baked) and composited at the
//! retouching stage — after demosaic and lens correction, before white balance and tone — so
//! every later develop change applies to them like to the rest of the photo.
//!
//! A patch is a small linear RGBA raster in the space of the pipeline's source (linear Rec.2020,
//! as-shot white balance) placed on a rectangle in normalized *transformed* coordinates (the same
//! coordinates as spots and masks: the oriented source after lens correction and perspective), so
//! crops, straightening and flips keep it in place. Its alpha is the removal's coverage.
//!
//! The pipeline is pure, so patches come from a process-wide registry: a [`PatchSource`] (the
//! engine's content-addressed store beside the catalog) behind a small memory cache, looked up by
//! the patch's key. Keys are content addresses (the source photo, the stroke, the engine and the
//! seed), so a key always names the same pixels and the cache never needs invalidating.
//!
//! [`AiView`] is the reversible display encoding the generator works through: the AI engines see
//! (and return) 8-bit sRGB, and what they return is decoded back into the source's space.

use std::sync::{Arc, Mutex, RwLock};

use lightcraft_develop::DevelopSettings;
use lightcraft_geom::Point;
use lightcraft_raster::Rgb32f;

use crate::SourceInfo;
use crate::geometry::Frame;

/// A patch raster: linear RGB in the source's space and coverage alpha (0..1), row-major.
#[derive(Clone, Debug, PartialEq)]
pub struct PatchPixels {
    pub width: usize,
    pub height: usize,
    pub data: Vec<[f32; 4]>,
}

impl PatchPixels {
    pub fn bytes(&self) -> usize {
        self.data.len() * 16
    }

    /// Box-downscale by an integer `f` (premultiplied, so transparent pixels don't bleed).
    fn shrink(&self, f: usize) -> PatchPixels {
        let (w, h) = (self.width.div_ceil(f).max(1), self.height.div_ceil(f).max(1));
        let mut data = vec![[0.0f32; 4]; w * h];
        for (i, o) in data.iter_mut().enumerate() {
            let (x0, y0) = ((i % w) * f, (i / w) * f);
            let mut acc = [0.0f32; 4];
            let mut n = 0.0f32;
            for y in y0..(y0 + f).min(self.height) {
                for x in x0..(x0 + f).min(self.width) {
                    let p = self.data[y * self.width + x];
                    for c in 0..3 {
                        acc[c] += p[c] * p[3];
                    }
                    acc[3] += p[3];
                    n += 1.0;
                }
            }
            *o = if acc[3] > 1e-6 { [acc[0] / acc[3], acc[1] / acc[3], acc[2] / acc[3], acc[3] / n.max(1.0)] } else { [0.0; 4] };
        }
        PatchPixels { width: w, height: h, data }
    }

    /// Bilinear, premultiplied sample at continuous pixel coordinates (centres at +0.5); `None`
    /// outside the raster.
    fn sample(&self, x: f32, y: f32) -> Option<[f32; 4]> {
        if x < 0.0 || y < 0.0 || x > self.width as f32 || y > self.height as f32 {
            return None;
        }
        let (fx, fy) = (x - 0.5, y - 0.5);
        let (x0, y0) = (fx.floor(), fy.floor());
        let (tx, ty) = (fx - x0, fy - y0);
        let at =
            |x: isize, y: isize| self.data[y.clamp(0, self.height as isize - 1) as usize * self.width + x.clamp(0, self.width as isize - 1) as usize];
        let (x0, y0) = (x0 as isize, y0 as isize);
        let mut acc = [0.0f32; 4];
        for (p, wgt) in [
            (at(x0, y0), (1.0 - tx) * (1.0 - ty)),
            (at(x0 + 1, y0), tx * (1.0 - ty)),
            (at(x0, y0 + 1), (1.0 - tx) * ty),
            (at(x0 + 1, y0 + 1), tx * ty),
        ] {
            for c in 0..3 {
                acc[c] += p[c] * p[3] * wgt;
            }
            acc[3] += p[3] * wgt;
        }
        if acc[3] <= 1e-6 {
            return Some([0.0; 4]);
        }
        Some([acc[0] / acc[3], acc[1] / acc[3], acc[2] / acc[3], acc[3]])
    }
}

/// Where patches come from when the memory cache doesn't hold them.
pub trait PatchSource: Send + Sync {
    fn load(&self, key: &str) -> Option<Arc<PatchPixels>>;
}

/// Bytes of decoded patches kept in memory.
const CACHE_BYTES: usize = 128 << 20;

static SOURCE: RwLock<Option<Arc<dyn PatchSource>>> = RwLock::new(None);
static CACHE: Mutex<Vec<(String, Arc<PatchPixels>)>> = Mutex::new(Vec::new());

/// Install (or remove) the source patches are loaded from (the engine's store).
pub fn set_source(src: Option<Arc<dyn PatchSource>>) {
    *SOURCE.write().unwrap_or_else(|e| e.into_inner()) = src;
}

/// Keep `pixels` under `key` in the memory cache (most recent first; the oldest go beyond the
/// budget).
pub fn insert(key: &str, pixels: Arc<PatchPixels>) {
    let mut c = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    c.retain(|(k, _)| k != key);
    c.insert(0, (key.to_owned(), pixels));
    let mut total = 0;
    c.retain(|(_, p)| {
        total += p.bytes();
        total <= CACHE_BYTES || total == p.bytes()
    });
}

/// Drop `key` from the memory cache.
pub fn forget(key: &str) {
    CACHE.lock().unwrap_or_else(|e| e.into_inner()).retain(|(k, _)| k != key);
}

/// The patch stored under `key`: from memory, else from the source. `None` when it is nowhere.
pub fn lookup(key: &str) -> Option<Arc<PatchPixels>> {
    {
        let mut c = CACHE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(i) = c.iter().position(|(k, _)| k == key) {
            let e = c.remove(i);
            let p = e.1.clone();
            c.insert(0, e);
            return Some(p);
        }
    }
    let src = SOURCE.read().unwrap_or_else(|e| e.into_inner()).clone()?;
    let p = src.load(key)?;
    insert(key, p.clone());
    Some(p)
}

/// One patch to composite: its key, where it goes (`[x0, y0, x1, y1]`, normalized transformed
/// coordinates) and how strongly (0..1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement<'a> {
    pub key: &'a str,
    pub rect: [f64; 4],
    pub opacity: f32,
}

/// Composite `placements` into `img` (a render's resampled image, before white balance) framed
/// by `frame`. Patches that can't be found are skipped (the photo renders without them).
pub fn apply(img: &mut Rgb32f, placements: &[Placement<'_>], frame: &Frame) {
    for pl in placements {
        if pl.opacity <= 0.0 {
            continue;
        }
        let Some(p) = lookup(pl.key) else { continue };
        composite(img, &p, pl.rect, pl.opacity.min(1.0), frame);
    }
}

/// Composite one patch raster (see [`apply`]).
pub fn composite(img: &mut Rgb32f, patch: &PatchPixels, rect: [f64; 4], opacity: f32, frame: &Frame) {
    let (w, h) = (img.width, img.height);
    let [x0, y0, x1, y1] = rect;
    if patch.width == 0 || patch.height == 0 || x1 <= x0 || y1 <= y0 || w == 0 || h == 0 {
        return;
    }
    let to_out = frame.norm_to_out(w, h);
    let to_norm = frame.out_to_norm(w, h);
    let corners = [(x0, y0), (x1, y0), (x0, y1), (x1, y1)].map(|(x, y)| to_out.apply(Point::new(x, y)));
    let (mut bx0, mut by0, mut bx1, mut by1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for c in &corners {
        (bx0, by0, bx1, by1) = (bx0.min(c.x), by0.min(c.y), bx1.max(c.x), by1.max(c.y));
    }
    let (ox0, oy0) = (bx0.floor().max(0.0) as usize, by0.floor().max(0.0) as usize);
    let (ox1, oy1) = ((bx1.ceil().max(0.0) as usize).min(w), (by1.ceil().max(0.0) as usize).min(h));
    if ox1 <= ox0 || oy1 <= oy0 {
        return;
    }
    // minifying: average the patch down first so it doesn't alias
    let span = corners[0].dist(corners[1]).max(1e-9);
    let f = (patch.width as f64 / span).floor() as usize;
    let shrunk;
    let patch = if f >= 2 {
        shrunk = patch.shrink(f);
        &shrunk
    } else {
        patch
    };
    let (sx, sy) = (patch.width as f64 / (x1 - x0), patch.height as f64 / (y1 - y0));
    for y in oy0..oy1 {
        for x in ox0..ox1 {
            let n = to_norm.apply(Point::new(x as f64 + 0.5, y as f64 + 0.5));
            let Some(s) = patch.sample(((n.x - x0) * sx) as f32, ((n.y - y0) * sy) as f32) else { continue };
            let a = s[3] * opacity;
            if a <= 0.0 {
                continue;
            }
            let t = img.get(x, y);
            img.set(x, y, [0, 1, 2].map(|c| (t[c] + (s[c] - t[c]) * a).max(0.0)));
        }
    }
}

/// The transformed image the patches are made on: `src` resampled through the uncropped frame
/// (orientation, lens corrections, perspective) at its native size — what a render shows, before
/// cropping and white balance. Returns the frame too (its `norm_to_out` places strokes on it).
pub fn transformed(src: &Rgb32f, info: &SourceInfo, s: &DevelopSettings) -> (Frame, Rgb32f) {
    let frame = crate::frame_for(src, info, s, false);
    let (nw, nh) = frame.native_size();
    let (w, h) = ((nw.round() as usize).max(1), (nh.round() as usize).max(1));
    let img = frame.sample(src, w, h);
    (frame, img)
}

/// Linear Rec.2020 → linear Rec.709 (sRGB primaries).
const REC2020_TO_709: [[f32; 3]; 3] = [[1.660_491, -0.587_641, -0.072_850], [-0.124_550, 1.132_900, -0.008_349], [-0.018_151, -0.100_579, 1.118_730]];

fn mul(a: &[[f32; 3]; 3], b: &[[f32; 3]; 3]) -> [[f32; 3]; 3] {
    std::array::from_fn(|r| std::array::from_fn(|c| (0..3).map(|k| a[r][k] * b[k][c]).sum()))
}

fn apply3(m: &[[f32; 3]; 3], v: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|r| m[r][0] * v[0] + m[r][1] * v[1] + m[r][2] * v[2])
}

fn invert3(m: &[[f32; 3]; 3]) -> Option<[[f32; 3]; 3]> {
    let m = m.map(|r| r.map(f64::from));
    let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
    if det.abs() < 1e-12 {
        return None;
    }
    let c = |r0: usize, r1: usize, c0: usize, c1: usize| m[r0][c0] * m[r1][c1] - m[r0][c1] * m[r1][c0];
    let adj = [
        [c(1, 2, 1, 2), -c(0, 2, 1, 2), c(0, 1, 1, 2)],
        [-c(1, 2, 0, 2), c(0, 2, 0, 2), -c(0, 1, 0, 2)],
        [c(1, 2, 0, 1), -c(0, 2, 0, 1), c(0, 1, 0, 1)],
    ];
    Some(adj.map(|r| r.map(|v| (v / det) as f32)))
}

/// The reversible display encoding an AI engine works through: the photo's white balance, a
/// gain that puts the area's bright end (99th percentile) just below white, Rec.2020 → sRGB
/// primaries, the sRGB curve, 8 bits. [`AiView::decode`] undoes it exactly for anything below
/// white (the engines' output always is).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AiView {
    fwd: [[f32; 3]; 3],
    inv: [[f32; 3]; 3],
}

impl AiView {
    /// The view for `region` (pixels in the source's space) developed with `s`.
    pub fn new(region: &[[f32; 3]], info: &SourceInfo, s: &DevelopSettings) -> AiView {
        let wb = crate::local::wb_matrix_for(info, s).unwrap_or([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);
        let m = mul(&REC2020_TO_709, &wb);
        let mut lum: Vec<f32> = region.iter().map(|p| apply3(&m, *p).iter().fold(0.0f32, |a, v| a.max(*v))).filter(|v| v.is_finite()).collect();
        let k = if lum.is_empty() {
            1.0
        } else {
            let i = ((lum.len() - 1) as f32 * 0.99) as usize;
            let (_, p99, _) = lum.select_nth_unstable_by(i, f32::total_cmp);
            (0.92 / p99.max(1e-4)).clamp(1e-3, 1e4)
        };
        let fwd = m.map(|r| r.map(|v| v * k));
        let inv = invert3(&fwd).unwrap_or([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);
        AiView { fwd, inv }
    }

    pub fn encode(&self, v: [f32; 3]) -> [u8; 3] {
        apply3(&self.fwd, v).map(lightcraft_color::transfer::encode_srgb8)
    }

    pub fn decode(&self, c: [u8; 3]) -> [f32; 3] {
        apply3(&self.inv, c.map(lightcraft_color::transfer::decode_srgb8))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: usize, h: usize, v: [f32; 4]) -> PatchPixels {
        PatchPixels { width: w, height: h, data: vec![v; w * h] }
    }

    fn flat(w: usize, h: usize) -> Rgb32f {
        Rgb32f::from_fn(w, h, |_, _| [0.2, 0.2, 0.2])
    }

    #[test]
    fn a_patch_replaces_its_rectangle_only() {
        let mut img = flat(100, 80);
        let frame = Frame::new(100, 80, &DevelopSettings::default(), true);
        composite(&mut img, &solid(20, 16, [0.8, 0.5, 0.1, 1.0]), [0.4, 0.4, 0.6, 0.6], 1.0, &frame);
        assert_eq!(img.get(50, 40), [0.8, 0.5, 0.1]);
        assert_eq!(img.get(10, 10), [0.2, 0.2, 0.2]);
        assert_eq!(img.get(39, 40), [0.2, 0.2, 0.2]);
        assert_eq!(img.get(60, 40), [0.2, 0.2, 0.2]);
    }

    #[test]
    fn opacity_and_alpha_blend() {
        let frame = Frame::new(50, 50, &DevelopSettings::default(), true);
        let mut img = flat(50, 50);
        composite(&mut img, &solid(10, 10, [1.0, 1.0, 1.0, 1.0]), [0.0, 0.0, 1.0, 1.0], 0.5, &frame);
        assert!((img.get(25, 25)[0] - 0.6).abs() < 1e-5);
        let mut img = flat(50, 50);
        composite(&mut img, &solid(10, 10, [1.0, 1.0, 1.0, 0.25]), [0.0, 0.0, 1.0, 1.0], 1.0, &frame);
        assert!((img.get(25, 25)[0] - 0.4).abs() < 1e-5);
    }

    /// Crops and flips move the output, not the patch: it stays on the same part of the photo.
    #[test]
    fn crop_and_flip_keep_the_patch_on_the_photo() {
        let mut s = DevelopSettings::default();
        s.crop.geometry.rect = lightcraft_geom::Rect::new(0.5, 0.0, 1.0, 1.0);
        s.crop.flip_h = true;
        let frame = Frame::new(200, 100, &s, true);
        // the output (100 × 100) shows x 0.5..1 mirrored: normalized x 0.7..0.8 lands at 40..60
        let mut img = flat(100, 100);
        composite(&mut img, &solid(10, 10, [1.0, 0.0, 0.0, 1.0]), [0.7, 0.2, 0.8, 0.3], 1.0, &frame);
        assert_eq!(img.get(50, 25), [1.0, 0.0, 0.0]);
        assert_eq!(img.get(30, 25), [0.2, 0.2, 0.2]);
        assert_eq!(img.get(70, 25), [0.2, 0.2, 0.2]);
    }

    #[test]
    fn a_large_patch_on_a_small_preview_is_averaged() {
        // a fine checkerboard of 0 and 1 seen at a tenth of its size: grey, not aliased
        let p = PatchPixels {
            width: 200,
            height: 200,
            data: (0..200 * 200).map(|i| if (i % 200 + i / 200) % 2 == 0 { [1.0, 1.0, 1.0, 1.0] } else { [0.0, 0.0, 0.0, 1.0] }).collect(),
        };
        let frame = Frame::new(20, 20, &DevelopSettings::default(), true);
        let mut img = flat(20, 20);
        composite(&mut img, &p, [0.0, 0.0, 1.0, 1.0], 1.0, &frame);
        for y in 0..20 {
            for x in 0..20 {
                assert!((img.get(x, y)[0] - 0.5).abs() < 0.06, "{x},{y}: {:?}", img.get(x, y));
            }
        }
    }

    #[test]
    fn registry_finds_patches_and_skips_missing_ones() {
        struct One;
        impl PatchSource for One {
            fn load(&self, key: &str) -> Option<Arc<PatchPixels>> {
                (key == "patches-test-from-source").then(|| Arc::new(solid(4, 4, [0.0, 1.0, 0.0, 1.0])))
            }
        }
        let frame = Frame::new(40, 40, &DevelopSettings::default(), true);
        insert("patches-test-mem", Arc::new(solid(4, 4, [1.0, 0.0, 0.0, 1.0])));
        let mut img = flat(40, 40);
        let pl = |key| Placement { key, rect: [0.0, 0.0, 0.5, 0.5], opacity: 1.0 };
        apply(&mut img, &[pl("patches-test-mem"), pl("patches-test-missing")], &frame);
        assert_eq!(img.get(5, 5), [1.0, 0.0, 0.0]);
        assert_eq!(img.get(30, 30), [0.2, 0.2, 0.2]);
        // (other tests may install a source too; this one only needs its own key to resolve)
        set_source(Some(Arc::new(One)));
        assert!(lookup("patches-test-from-source").is_some());
        forget("patches-test-mem");
        assert!(lookup("patches-test-missing").is_none());
    }

    #[test]
    fn the_ai_view_round_trips() {
        let region: Vec<[f32; 3]> = (0..400).map(|i| [0.02 + i as f32 * 0.002, 0.05 + i as f32 * 0.0015, 0.01 + i as f32 * 0.001]).collect();
        let info = SourceInfo { raw: true, as_shot_temp: 5000.0, ..Default::default() };
        let mut s = DevelopSettings::default();
        s.wb = lightcraft_develop::WhiteBalance { mode: lightcraft_develop::WbMode::Custom, temp: 4300.0, tint: 8.0 };
        let v = AiView::new(&region, &info, &s);
        for p in region.iter().take(390) {
            let back = v.decode(v.encode(*p));
            for c in 0..3 {
                assert!((back[c] - p[c]).abs() < p[c] * 0.04 + 2e-3, "{p:?} → {back:?}");
            }
        }
        // the bright end sits just below white
        let top = v.encode(region[396]);
        assert!(top.iter().any(|c| *c > 200), "{top:?}");
    }

    #[test]
    fn the_transformed_image_is_the_uncropped_render_frame() {
        let src = Rgb32f::from_fn(60, 40, |x, y| [x as f32 / 60.0, y as f32 / 40.0, 0.5]);
        let mut s = DevelopSettings::default();
        s.crop.geometry.rect = lightcraft_geom::Rect::new(0.2, 0.2, 0.8, 0.8);
        let (frame, img) = transformed(&src, &SourceInfo::default(), &s);
        assert_eq!((img.width, img.height), (60, 40));
        assert_eq!(frame.out_to_norm(60, 40).apply(Point::new(0.0, 0.0)), Point::new(0.0, 0.0));
        assert_eq!(img.get(10, 10), src.get(10, 10));
    }
}
