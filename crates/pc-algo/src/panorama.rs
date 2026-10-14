//! Photo editor layer/layout compatibility adapters onto the shared merge backend.
use crate::transform::Homography;
use lightcraft_merge::editor::{geom, panorama as shared};
pub use shared::{
    AlignOptions, ImageFeatures, Layout, MopsImageFeatures, Motion, Photometric, PreparedImage, Projection, RoiImage, blend_levels, detect, detect_many,
    detect_mops_many, multiband, seam_labels,
};
fn to_shared(h: &Homography) -> geom::Homography {
    geom::Homography(h.0)
}
pub fn fit_motion(motion: Motion, src: &[[f64; 2]], dst: &[[f64; 2]]) -> Option<Homography> {
    shared::fit_motion(motion, src, dst).map(|h| Homography(h.0))
}
pub fn ransac_motion(motion: Motion, src: &[[f64; 2]], dst: &[[f64; 2]], iters: usize, threshold: f64, seed: u64) -> Option<(Homography, Vec<usize>)> {
    shared::ransac_motion(motion, src, dst, iters, threshold, seed).map(|(h, i)| (Homography(h.0), i))
}
pub fn focal_from_homography(h: &Homography, ca: [f64; 2], cb: [f64; 2]) -> Option<f64> {
    shared::focal_from_homography(&to_shared(h), ca, cb)
}
#[derive(Clone, Debug, PartialEq)]
pub struct Placement {
    pub projection: Projection,
    pub focal: f64,
    pub center: [f64; 2],
    pub k1: f64,
    pub h: Homography,
}
impl Placement {
    fn shared(&self) -> shared::Placement {
        shared::Placement { projection: self.projection, focal: self.focal, center: self.center, k1: self.k1, h: to_shared(&self.h) }
    }
    fn from_shared(p: shared::Placement) -> Self {
        Self { projection: p.projection, focal: p.focal, center: p.center, k1: p.k1, h: Homography(p.h.0) }
    }
    pub fn project(&self, x: f64, y: f64) -> [f64; 2] {
        self.shared().project(x, y)
    }
    pub fn unproject(&self, a: f64, b: f64) -> Option<(f64, f64)> {
        self.shared().unproject(a, b)
    }
    pub fn forward(&self, x: f64, y: f64) -> (f64, f64) {
        self.shared().forward(x, y)
    }
    pub fn inverse(&self, u: f64, v: f64) -> Option<(f64, f64)> {
        self.shared().inverse(u, v)
    }
    pub fn scaled(&self, k: f64) -> Self {
        Self::from_shared(self.shared().scaled(k))
    }
    pub fn translated(&self, dx: f64, dy: f64) -> Self {
        Self::from_shared(self.shared().translated(dx, dy))
    }
    pub fn bounds(&self, w: f64, h: f64) -> [f64; 4] {
        self.shared().bounds(w, h)
    }
}
#[derive(Clone, Debug)]
pub struct PairMatch {
    pub a: usize,
    pub b: usize,
    pub pts_a: Vec<[f64; 2]>,
    pub pts_b: Vec<[f64; 2]>,
    pub homography: Homography,
}
impl PairMatch {
    fn shared(&self) -> shared::PairMatch {
        shared::PairMatch { a: self.a, b: self.b, pts_a: self.pts_a.clone(), pts_b: self.pts_b.clone(), homography: to_shared(&self.homography) }
    }
    fn from_shared(p: shared::PairMatch) -> Self {
        Self { a: p.a, b: p.b, pts_a: p.pts_a, pts_b: p.pts_b, homography: Homography(p.homography.0) }
    }
}
pub fn match_pairs(feats: &[ImageFeatures]) -> Vec<PairMatch> {
    shared::match_pairs(feats).into_iter().map(PairMatch::from_shared).collect()
}
pub fn match_mops_pairs(feats: &[MopsImageFeatures]) -> Vec<PairMatch> {
    shared::match_mops_pairs(feats).into_iter().map(PairMatch::from_shared).collect()
}
#[derive(Clone, Debug)]
pub struct Alignment {
    pub layout: Layout,
    pub reference: usize,
    pub placements: Vec<Option<Placement>>,
    pub focal: f64,
    pub k1: f64,
    pub pairs: Vec<(usize, usize, usize)>,
    pub rms: f64,
}
impl Alignment {
    fn from_shared(a: shared::Alignment) -> Self {
        Self {
            layout: a.layout,
            reference: a.reference,
            placements: a.placements.into_iter().map(|p| p.map(Placement::from_shared)).collect(),
            focal: a.focal,
            k1: a.k1,
            pairs: a.pairs,
            rms: a.rms,
        }
    }
}
pub fn align(feats: &[ImageFeatures], matches: &[PairMatch], opts: &AlignOptions) -> Option<Alignment> {
    shared::align(feats, &matches.iter().map(PairMatch::shared).collect::<Vec<_>>(), opts).map(Alignment::from_shared)
}
pub fn align_mops(feats: &[MopsImageFeatures], matches: &[PairMatch], opts: &AlignOptions) -> Option<Alignment> {
    shared::align_mops(feats, &matches.iter().map(PairMatch::shared).collect::<Vec<_>>(), opts).map(Alignment::from_shared)
}
pub type Geometry<'a> = (&'a [Placement], &'a [(usize, usize)]);
pub fn photometric(imgs: &[RoiImage], geometry: Option<Geometry>, gain: bool, vignette: bool) -> Photometric {
    let placements = geometry.map(|(p, _)| p.iter().map(Placement::shared).collect::<Vec<_>>());
    let geometry = geometry.zip(placements.as_ref()).map(|((_, sizes), p)| (p.as_slice(), sizes));
    shared::photometric(imgs, geometry, gain, vignette)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::photo_util::Rng;

    /// Smooth random texture: blobs over a gradient (bounded detail so Harris finds corners).
    pub(crate) fn scene(w: usize, h: usize, seed: u64) -> Vec<f32> {
        let mut rng = Rng(seed);
        let mut img: Vec<f32> = (0..w * h).map(|i| 0.25 + 0.2 * ((i % w) as f32 / w as f32)).collect();
        for _ in 0..(w * h / 900).max(40) {
            let cx = (rng.next() % w as u64) as f64;
            let cy = (rng.next() % h as u64) as f64;
            let r = 3.0 + (rng.next() % 9) as f64;
            let v = (rng.next() % 1000) as f32 / 1000.0;
            let sq = rng.next().is_multiple_of(2);
            let (x0, x1) = ((cx - r).max(0.0) as usize, ((cx + r) as usize + 1).min(w));
            let (y0, y1) = ((cy - r).max(0.0) as usize, ((cy + r) as usize + 1).min(h));
            for y in y0..y1 {
                for x in x0..x1 {
                    let (dx, dy) = (x as f64 - cx, y as f64 - cy);
                    if if sq { dx.abs() < r && dy.abs() < r * 0.6 } else { dx.hypot(dy) < r } {
                        img[y * w + x] = v;
                    }
                }
            }
        }
        img
    }

    fn sample(img: &[f32], w: usize, h: usize, x: f64, y: f64) -> Option<f32> {
        if x < 0.0 || y < 0.0 || x > (w - 1) as f64 || y > (h - 1) as f64 {
            return None;
        }
        Some(crate::photo_util::bilinear(img, w, h, 1, 0, x as f32, y as f32))
    }

    /// Crops `tw × th` views of a scene through `map` (view px → scene px).
    fn view(scene: &[f32], sw: usize, sh: usize, tw: usize, th: usize, map: impl Fn(f64, f64) -> (f64, f64)) -> Vec<f32> {
        (0..tw * th)
            .map(|i| {
                let (x, y) = map((i % tw) as f64, (i / tw) as f64);
                sample(scene, sw, sh, x, y).unwrap_or(0.0)
            })
            .collect()
    }

    #[test]
    fn recovers_translations_and_homographies() {
        let (sw, sh) = (640, 300);
        let sc = scene(sw, sh, 3);
        let (tw, th) = (260, 220);
        // Three overlapping crops at known offsets.
        let offs = [(20.0, 30.0), (190.0, 40.0), (360.0, 25.0)];
        let imgs: Vec<Vec<f32>> = offs.iter().map(|&(ox, oy)| view(&sc, sw, sh, tw, th, |x, y| (x + ox, y + oy))).collect();
        let feats: Vec<ImageFeatures> = imgs.iter().map(|im| detect(tw, th, im, None, 700)).collect();
        let matches = match_pairs(&feats);
        assert!(matches.len() >= 2, "{}", matches.len());
        for layout in [Layout::Reposition, Layout::Collage, Layout::Perspective] {
            let a = align(&feats, &matches, &AlignOptions { layout, focal: None, reference: Some(1), geometric: false }).unwrap();
            assert_eq!(a.reference, 1);
            for (i, p) in a.placements.iter().enumerate() {
                let p = p.as_ref().unwrap();
                // A point at image px (50, 60) is scene (50+ox, 60+oy); relative to image 1.
                let (u, v) = p.forward(50.0, 60.0);
                let (eu, ev) = (50.0 + offs[i].0 - offs[1].0, 60.0 + offs[i].1 - offs[1].1);
                assert!((u - eu).abs() < 1.0 && (v - ev).abs() < 1.0, "{layout:?} image {i}: ({u},{v}) vs ({eu},{ev})");
            }
            assert!(a.rms < 1.0, "{layout:?} rms {}", a.rms);
        }
        // A projective view is recovered by the perspective layout.
        let hq = Homography([1.05, 0.03, 180.0, -0.02, 1.0, 35.0, 0.0001, -0.00005, 1.0]);
        let im2 = view(&sc, sw, sh, tw, th, |x, y| hq.apply(x, y));
        let feats2 = vec![feats[0].clone(), detect(tw, th, &im2, None, 700)];
        let m2 = match_pairs(&feats2);
        let a = align(&feats2, &m2, &AlignOptions { layout: Layout::Perspective, focal: None, reference: Some(0), geometric: false }).unwrap();
        let p = a.placements[1].as_ref().unwrap();
        // Points inside the overlap (extrapolation beyond it is loosely constrained).
        for (x, y) in [(20.0, 30.0), (70.0, 180.0), (40.0, 100.0)] {
            let (sx, sy) = hq.apply(x, y);
            let (u, v) = p.forward(x, y);
            assert!((u - (sx - 20.0)).abs() < 1.5 && (v - (sy - 30.0)).abs() < 1.5, "({u},{v}) vs ({},{})", sx - 20.0, sy - 30.0);
        }
    }

    #[test]
    fn rotating_camera_focal_and_cylindrical_layout() {
        // Views of a cylinder-mapped scene from a camera panning by 25° with f = 300 px.
        let f = 300.0f64;
        let (sw, sh) = (1400, 500);
        let sc = scene(sw, sh, 9);
        let (tw, th) = (320, 240);
        let pans = [-25f64, 0.0, 25.0];
        let imgs: Vec<Vec<f32>> = pans
            .iter()
            .map(|&deg| {
                view(&sc, sw, sh, tw, th, |x, y| {
                    let (dx, dy) = (x - tw as f64 / 2.0, y - th as f64 / 2.0);
                    let th_ = dx.atan2(f) + deg.to_radians();
                    let hh = dy / dx.hypot(f);
                    (sw as f64 / 2.0 + f * th_, sh as f64 / 2.0 + f * hh)
                })
            })
            .collect();
        let feats: Vec<ImageFeatures> = imgs.iter().map(|im| detect(tw, th, im, None, 700)).collect();
        let matches = match_pairs(&feats);
        assert_eq!(matches.iter().filter(|m| m.b == m.a + 1).count(), 2);
        let est: Vec<f64> = matches.iter().filter_map(|m| focal_from_homography(&m.homography, [160.0, 120.0], [160.0, 120.0])).collect();
        assert!(est.iter().any(|e| (e - f).abs() / f < 0.15), "focal estimates {est:?}");
        let a = align(&feats, &matches, &AlignOptions { layout: Layout::Cylindrical, focal: None, reference: Some(1), geometric: false }).unwrap();
        assert!((a.focal - f).abs() / f < 0.1, "focal {}", a.focal);
        let p = a.placements[2].as_ref().unwrap();
        // The centre of the right image sits 25° along the cylinder.
        let (u, _) = p.forward(160.0, 120.0);
        let expect = 160.0 + a.focal * 25f64.to_radians();
        assert!((u - expect).abs() < 4.0, "{u} vs {expect}");
        assert!(a.rms < 1.5, "rms {}", a.rms);
        // Round trip through the placement.
        let (u, v) = p.forward(40.0, 200.0);
        let (x, y) = p.inverse(u, v).unwrap();
        assert!((x - 40.0).abs() < 1e-6 && (y - 200.0).abs() < 1e-6);
    }

    #[test]
    fn placement_round_trips_all_projections() {
        for projection in [Projection::Plane, Projection::Cylinder, Projection::Sphere] {
            let p =
                Placement { projection, focal: 400.0, center: [200.0, 150.0], k1: 0.05, h: Homography([0.98, -0.1, 30.0, 0.1, 0.98, -12.0, 0.0, 0.0, 1.0]) };
            for (x, y) in [(0.0, 0.0), (390.0, 20.0), (123.0, 280.0)] {
                let (u, v) = p.forward(x, y);
                let (bx, by) = p.inverse(u, v).unwrap();
                assert!((bx - x).abs() < 1e-6 && (by - y).abs() < 1e-6, "{projection:?}");
            }
            let s = p.scaled(2.0);
            let (u, v) = p.forward(100.0, 50.0);
            let (u2, v2) = s.forward(200.0, 100.0);
            assert!((u2 - 2.0 * u).abs() < 1e-6 && (v2 - 2.0 * v).abs() < 1e-6);
        }
    }

    #[test]
    fn photometric_recovers_gain_and_vignette() {
        // Two half-overlapping views of a flat-ish scene: the second one 30 % brighter.
        let (w, h) = (200usize, 100usize);
        let base = |x: i32, y: i32| 0.3 + 0.2 * ((x as f32 * 0.05).sin() * (y as f32 * 0.07).cos()).abs();
        let mut imgs = Vec::new();
        let mut pls = Vec::new();
        for (k, (x0, g)) in [(0, 1.0f32), (100, 1.3f32)].into_iter().enumerate() {
            let mut px = Vec::new();
            for y in 0..h as i32 {
                for x in 0..w as i32 {
                    px.push(base(x + x0, y) * g);
                }
            }
            imgs.push(RoiImage { x0, y0: 0, w, h, ch: 1, px, alpha: vec![1.0; w * h] });
            pls.push(Placement {
                projection: Projection::Plane,
                focal: 200.0,
                center: [100.0, 50.0],
                k1: 0.0,
                h: Homography([1.0, 0.0, 100.0 + x0 as f64, 0.0, 1.0, 50.0, 0.0, 0.0, 1.0]),
            });
            let _ = k;
        }
        let ph = photometric(&imgs, Some((&pls, &[(w, h), (w, h)])), true, false);
        let ratio = ph.gains[1][0] / ph.gains[0][0];
        assert!((ratio - 1.3).abs() < 0.03, "{ph:?}");
        // With vignetting on, the flat scene (no falloff) gives a near-zero model.
        let pv = photometric(&imgs, Some((&pls, &[(w, h), (w, h)])), true, true);
        assert!(pv.vignette[0].abs() < 0.1, "{pv:?}");
    }

    #[test]
    fn seams_avoid_disagreement_and_multiband_is_seamless() {
        // Two images over a 120×40 strip, overlapping in 40..80; the second has a bright
        // "moving object" at x 50..58 that the first doesn't.
        let (w, h) = (120usize, 40usize);
        let mut luma = vec![vec![0.5f32; w * h], vec![0.5f32; w * h]];
        let cover: Vec<Vec<bool>> = vec![(0..w * h).map(|i| i % w < 80).collect(), (0..w * h).map(|i| i % w >= 40).collect()];
        for y in 0..h {
            for x in 50..58 {
                luma[1][y * w + x] = 1.0;
            }
        }
        let labels = seam_labels(w, h, &luma, &cover, &[0, 1]);
        // The object column belongs to image 0 (the seam passes to its right), or entirely to 1.
        for y in 0..h {
            let owner: Vec<i32> = (50..58).map(|x| labels[y * w + x]).collect();
            assert!(owner.iter().all(|o| *o == owner[0]), "row {y} split the object: {owner:?}");
        }
        assert_eq!(labels[5], 0);
        assert_eq!(labels[w - 1], 1);
        // Multi-band blend of two flat images at different levels joins smoothly.
        let mk = |x0: i32, v: f32| RoiImage { x0, y0: 0, w: 80, h, ch: 1, px: vec![v; 80 * h], alpha: vec![1.0; 80 * h] };
        let imgs = [mk(0, 0.3), mk(40, 0.7)];
        let wts: Vec<Vec<f32>> = (0..2)
            .map(|k| {
                (0..80 * h)
                    .map(|i| {
                        let x = (i % 80) as i32 + imgs[k].x0;
                        if (x < 60) == (k == 0) { 1.0 } else { 0.0 }
                    })
                    .collect()
            })
            .collect();
        let (out, cov) = multiband(w, h, &imgs, &wts, 4);
        let row: Vec<f32> = (0..w).map(|x| out[20 * w + x]).collect();
        assert!((row[2] - 0.3).abs() < 0.02 && (row[w - 3] - 0.7).abs() < 0.02, "{row:?}");
        let max_jump = row.windows(2).map(|p| (p[1] - p[0]).abs()).fold(0.0f32, f32::max);
        assert!(max_jump < 0.1, "max step {max_jump}");
        assert!(cov.iter().all(|c| *c == 1.0));
    }
}

#[cfg(test)]
mod j19_parity {
    use super::*;
    #[test]
    fn j19_layer_projection_scaling_semantics_parity() {
        for projection in [Projection::Plane, Projection::Cylinder, Projection::Sphere] {
            let p = Placement {
                projection,
                focal: 180.0,
                center: [100.0, 75.0],
                k1: -0.005,
                h: Homography([1.01, 0.02, 31.0, -0.02, 1.01, -19.0, 0.0001, -0.0001, 1.0]),
            };
            for (x, y) in [(20.0, 30.0), (100.0, 75.0), (180.0, 120.0)] {
                assert_eq!(p.forward(x, y), p.shared().forward(x, y));
                let actual = p.forward(x, y);
                let full = p.scaled(2.0).forward(x * 2.0, y * 2.0);
                assert!((full.0 - actual.0 * 2.0).hypot(full.1 - actual.1 * 2.0) < 1e-9);
                let moved = p.translated(4.0, -7.0).forward(x, y);
                assert!((moved.0 - actual.0 - 4.0).hypot(moved.1 - actual.1 + 7.0) < 1e-9);
            }
        }
    }
    #[test]
    fn j19_mops_native_coordinate_and_pair_parity() {
        let (w, h) = (192, 128);
        let luma: Vec<_> = (0..w * h).map(|i| (((i % w / 8) * 37 + (i / w / 8) * 23 + (i % w / 13) * (i / w / 11) * 17) % 127) as f32 / 127.0).collect();
        let prepared = vec![(w, h, luma.clone(), None), (w, h, luma.clone(), None)];
        let feats = detect_mops_many(&prepared, 100);
        assert_eq!(feats[0].features.desc, feats[1].features.desc);
        let native = lightcraft_merge::editor::features::mops(w, h, &luma, None, 100);
        assert_eq!(native.desc, feats[0].features.desc);
        for (native, editor) in native.points.iter().zip(&feats[0].features.points) {
            assert_eq!(native.x, editor.x);
            assert_eq!(native.y, editor.y)
        }
        let matched = match_mops_pairs(&feats);
        assert_eq!(matched.len(), 1);
        assert!(matched[0].pts_a.len() >= 10);
        let opts = AlignOptions { layout: Layout::Reposition, focal: None, reference: Some(0), geometric: false };
        let alignment = align_mops(&feats, &matched, &opts).unwrap();
        assert_eq!(alignment.reference, 0);
        assert_eq!(alignment.layout, Layout::Reposition);
        assert!(alignment.rms < 1e-6);
    }
}

#[cfg(test)]
mod j19_legacy_goldens {
    use super::*;
    #[test]
    fn j19_layer_gain_seam_multiband_matches_legacy_numeric_goldens() {
        // Goldens harvested from unchanged pre-J19 code, retained only under ignored target/.
        let (w, h) = (16, 12);
        let px: Vec<f32> = (0..w * h).map(|i| 0.15 + (i % w) as f32 * 0.02 + (i / w) as f32 * 0.003).collect();
        let imgs = [
            RoiImage { x0: 0, y0: 0, w, h, ch: 1, px: px.clone(), alpha: vec![1.0; w * h] },
            RoiImage { x0: 0, y0: 0, w, h, ch: 1, px: px.iter().map(|p| p * 1.8).collect(), alpha: vec![1.0; w * h] },
        ];
        let gains = photometric(&imgs, None, true, false);
        assert!((gains.gains[0][0] - 0.7454194).abs() < 1e-7);
        assert!((gains.gains[1][0] - 1.3415267).abs() < 1e-7);
        let lum = [imgs[0].px.clone(), imgs[1].px.clone()];
        let cov = [vec![true; w * h], vec![true; w * h]];
        assert_eq!(seam_labels(w, h, &lum, &cov, &[0, 1]), vec![0; w * h]);
        let weights = [(0..w * h).map(|i| if i % w < 8 { 1.0 } else { 0.0 }).collect(), (0..w * h).map(|i| if i % w >= 8 { 1.0 } else { 0.0 }).collect()];
        let (blended, coverage) = multiband(w, h, &imgs, &weights, 3);
        let golden = [0.15, 0.35500276, 0.42055085, 0.8039041, 0.83077806, 0.86298317];
        for (i, expected) in [0, 7, 8, 15, 95, 191].into_iter().zip(golden) {
            assert!((blended[i] - expected).abs() < 1e-6, "blend[{i}] {} != {expected}", blended[i]);
        }
        assert!((blended.iter().map(|v| *v as f64).sum::<f64>() - 86.70398907363415).abs() < 1e-5);
        assert_eq!(coverage, vec![1.0; w * h]);
    }
    #[test]
    fn j19_layer_seam_does_not_split_moving_foreground() {
        let (w, h) = (16, 12);
        let mut lum = vec![vec![0.5; w * h], vec![0.5; w * h]];
        for y in 0..h {
            for x in 6..9 {
                lum[1][y * w + x] = 1.0;
            }
        }
        let cov = vec![(0..w * h).map(|i| i % w < 12).collect(), (0..w * h).map(|i| i % w >= 4).collect()];
        let labels = seam_labels(w, h, &lum, &cov, &[0, 1]);
        for y in 0..h {
            assert_eq!(labels[y * w], 0);
            assert_eq!(labels[y * w + 15], 1);
            assert!(labels[y * w + 6..y * w + 9].iter().all(|v| *v == labels[y * w + 6]));
        }
    }
}

#[cfg(test)]
mod j19_blender_models {
    use super::*;
    #[test]
    fn j19_shared_blender_preserves_odd_canvas_and_non_rgb_channels() {
        let (w, h, ch) = (17, 13, 2);
        let px = (0..w * h).flat_map(|_| [0.3, 0.8]).collect();
        let image = RoiImage { x0: 0, y0: 0, w, h, ch, px, alpha: vec![1.0; w * h] };
        let (out, coverage) = multiband(w, h, &[image], &[vec![1.0; w * h]], 4);
        assert_eq!(out.len(), w * h * ch);
        assert_eq!(coverage, vec![1.0; w * h]);
        for p in out.as_chunks::<2>().0 {
            assert!((p[0] - 0.3).abs() < 1e-6);
            assert!((p[1] - 0.8).abs() < 1e-6);
        }
    }
    #[test]
    fn j19_editor_roi_hole_fill_prevents_dark_blend_edges() {
        let (w, h) = (31, 17);
        let imgs = [
            RoiImage { x0: -3, y0: -2, w: 18, h: 15, ch: 1, px: vec![0.4; 18 * 15], alpha: vec![1.0; 18 * 15] },
            RoiImage { x0: 9, y0: 2, w: 25, h: 20, ch: 1, px: vec![0.4; 25 * 20], alpha: vec![1.0; 25 * 20] },
        ];
        let weights = imgs.iter().map(|i| vec![1.0; i.w * i.h]).collect::<Vec<_>>();
        let (out, coverage) = multiband(w, h, &imgs, &weights, 4);
        for (i, p) in out.iter().enumerate() {
            if coverage[i] > 0.0 {
                assert!((*p - 0.4).abs() < 1e-6, "covered edge {i}: {p}");
            }
        }
        assert_eq!(coverage[(h - 1) * w], 0.0);
        assert_eq!(coverage[w - 1], 0.0);
    }
}
