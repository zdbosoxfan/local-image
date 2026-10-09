//! Burst / near-duplicate stacking for Smart Sort (spec item 16). Pure logic, no UI.
//!
//! Photos are ordered by capture time; a photo joins the current stack when it is within the
//! strictness' time window of the PREVIOUS photo in the stack and its CLIP embedding has cosine
//! similarity at or above the threshold with that previous photo (chaining). Photos without a
//! parseable capture time or without an embedding are singletons.
//!
//! Constants (pinned by tests): Strict = 2 s and cos >= 0.97, Normal = 5 s and cos >= 0.93,
//! Loose = 15 s and cos >= 0.88.
//!
//! The best shot of a stack is the sharpest (variance of the Laplacian, see [`sharpness`]), ties
//! going to the highest rating, then the earliest capture, then the lowest id. Writing library
//! stacks is not done here (the dialog uses the existing `stack.group` command).

use lightcraft_catalog::{Catalog, PhotoId, stacks::iso_seconds};
use lightcraft_raster::Rgba8;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BurstStrictness {
    Strict,
    #[default]
    Normal,
    Loose,
}

impl BurstStrictness {
    /// Maximum capture-time gap, in seconds, between consecutive photos of a stack.
    pub fn window_secs(self) -> i64 {
        match self {
            BurstStrictness::Strict => 2,
            BurstStrictness::Normal => 5,
            BurstStrictness::Loose => 15,
        }
    }

    /// Minimum cosine similarity between consecutive photos of a stack.
    pub fn min_cosine(self) -> f32 {
        match self {
            BurstStrictness::Strict => 0.97,
            BurstStrictness::Normal => 0.93,
            BurstStrictness::Loose => 0.88,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BurstExport {
    /// Export only the best shot of each burst.
    #[default]
    BestOnly,
    /// Export every photo.
    All,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BurstSettings {
    pub enabled: bool,
    pub strictness: BurstStrictness,
    pub export: BurstExport,
    pub also_stack_in_library: bool,
}

/// One stack of near-identical photos (`photos.len() == 1` for a singleton).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Burst {
    /// Members in capture order (ties by id).
    pub photos: Vec<PhotoId>,
    pub best: PhotoId,
}

/// Longest side of the grayscale analysis image used by [`sharpness`].
const ANALYSIS_SIDE: usize = 256;

/// Variance of the 4-neighbour Laplacian of a small grayscale version of `img` (box-downsampled
/// to at most 256 px on the long side). Higher is sharper; 0 for flat or tiny images.
pub fn sharpness(img: &Rgba8) -> f32 {
    let (w, h) = (img.width, img.height);
    if w < 3 || h < 3 || img.data.len() < w * h {
        return 0.0;
    }
    let step = w.max(h).div_ceil(ANALYSIS_SIDE).max(1);
    let (gw, gh) = (w / step, h / step);
    if gw < 3 || gh < 3 {
        return 0.0;
    }
    let mut gray = vec![0f32; gw * gh];
    for gy in 0..gh {
        for gx in 0..gw {
            let mut sum = 0f32;
            for y in gy * step..(gy + 1) * step {
                for x in gx * step..(gx + 1) * step {
                    let [r, g, b, _] = img.data[y * w + x];
                    sum += 0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32;
                }
            }
            gray[gy * gw + gx] = sum / (step * step) as f32;
        }
    }
    let (mut sum, mut sum2, mut n) = (0f64, 0f64, 0f64);
    for y in 1..gh - 1 {
        for x in 1..gw - 1 {
            let c = gray[y * gw + x];
            let l = gray[y * gw + x - 1] + gray[y * gw + x + 1] + gray[(y - 1) * gw + x] + gray[(y + 1) * gw + x] - 4.0 * c;
            sum += l as f64;
            sum2 += (l as f64) * (l as f64);
            n += 1.0;
        }
    }
    let mean = sum / n;
    ((sum2 / n) - mean * mean).max(0.0) as f32
}

fn cosine(a: &[f32], b: &[f32]) -> Option<f32> {
    if a.len() != b.len() || a.is_empty() {
        return None;
    }
    let (mut dot, mut na, mut nb) = (0f64, 0f64, 0f64);
    for (x, y) in a.iter().zip(b) {
        dot += (*x as f64) * (*y as f64);
        na += (*x as f64) * (*x as f64);
        nb += (*y as f64) * (*y as f64);
    }
    if na <= 0.0 || nb <= 0.0 {
        return None;
    }
    Some((dot / (na.sqrt() * nb.sqrt())) as f32)
}

/// Group `ids` into bursts. Deterministic and independent of input order; every distinct input id
/// appears in exactly one [`Burst`] (unknown ids become singletons). Bursts are ordered by their
/// first capture time (untimed photos last), then by id.
pub fn group_bursts(
    cat: &Catalog,
    ids: &[PhotoId],
    embedding: &dyn Fn(PhotoId) -> Option<Vec<f32>>,
    sharpness: &HashMap<PhotoId, f32>,
    s: BurstStrictness,
) -> Vec<Burst> {
    let mut seen = HashSet::new();
    let mut timed: Vec<(i64, PhotoId, Vec<f32>)> = Vec::new();
    let mut untimed: Vec<PhotoId> = Vec::new();
    for &id in ids {
        if !seen.insert(id) {
            continue;
        }
        let t = cat.photo(id).and_then(|p| iso_seconds(p.captured.as_deref()?));
        match (t, embedding(id)) {
            (Some(t), Some(e)) => timed.push((t, id, e)),
            _ => untimed.push(id),
        }
    }
    timed.sort_by_key(|(t, id, _)| (*t, *id));
    untimed.sort();

    let mut runs: Vec<Vec<(i64, PhotoId)>> = Vec::new();
    let mut prev: Option<(i64, &[f32])> = None;
    for (t, id, emb) in &timed {
        let joins = match prev {
            Some((pt, pe)) => *t - pt <= s.window_secs() && cosine(pe, emb).is_some_and(|c| c >= s.min_cosine()),
            None => false,
        };
        match runs.last_mut() {
            Some(r) if joins => r.push((*t, *id)),
            _ => runs.push(vec![(*t, *id)]),
        }
        prev = Some((*t, emb));
    }

    let mut out: Vec<(i64, Burst)> = runs
        .into_iter()
        .map(|r| {
            let best = pick_best(cat, &r, sharpness);
            (r[0].0, Burst { photos: r.iter().map(|(_, id)| *id).collect(), best })
        })
        .collect();
    out.extend(untimed.into_iter().map(|id| (i64::MAX, Burst { photos: vec![id], best: id })));
    out.sort_by_key(|(t, b)| (*t, b.photos[0]));
    out.into_iter().map(|(_, b)| b).collect()
}

fn pick_best(cat: &Catalog, run: &[(i64, PhotoId)], sharp: &HashMap<PhotoId, f32>) -> PhotoId {
    // Higher sharpness, then higher rating, then earlier capture, then lower id.
    let better = |a: &(i64, PhotoId), b: &(i64, PhotoId)| {
        let sh = |id: PhotoId| sharp.get(&id).copied().filter(|v| v.is_finite()).unwrap_or(f32::NEG_INFINITY);
        let rating = |id: PhotoId| cat.photo(id).map_or(0, |p| p.rating);
        sh(a.1).total_cmp(&sh(b.1)).then(rating(a.1).cmp(&rating(b.1))).then(b.0.cmp(&a.0)).then(b.1.cmp(&a.1)).is_gt()
    };
    let mut best = run[0];
    for c in &run[1..] {
        if better(c, &best) {
            best = *c;
        }
    }
    best.1
}

/// The photos to export: the best of each burst, or every photo, in burst order.
pub fn export_ids(bursts: &[Burst], mode: BurstExport) -> Vec<PhotoId> {
    match mode {
        BurstExport::BestOnly => bursts.iter().map(|b| b.best).collect(),
        BurstExport::All => bursts.iter().flat_map(|b| b.photos.iter().copied()).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lightcraft_catalog::{Op, Photo, Source};

    /// Catalog with one photo per `(captured, rating)` entry.
    fn cat(specs: &[(Option<&str>, u8)]) -> (Catalog, Vec<PhotoId>) {
        let mut c = Catalog::new();
        let mut ids = Vec::new();
        for (i, (t, r)) in specs.iter().enumerate() {
            let id = c.alloc_photo_id();
            let mut p = Photo::new(id, Source::Demo { scene: 1 }, &format!("{i}.jpg"), "JPEG", 60, 40, "2026-01-01T00:00:00");
            p.captured = t.map(str::to_string);
            p.rating = *r;
            c.apply(Op::AddPhoto { photo: Box::new(p) }).unwrap();
            ids.push(id);
        }
        (c, ids)
    }

    /// Unit vector at angle `deg` from the x axis (cosine to angle 0 is cos(deg)).
    fn at(deg: f32) -> Vec<f32> {
        let r = deg.to_radians();
        vec![r.cos(), r.sin(), 0.0]
    }

    fn group(c: &Catalog, ids: &[PhotoId], embs: &[Option<Vec<f32>>], sharp: &HashMap<PhotoId, f32>, s: BurstStrictness) -> Vec<Burst> {
        let map: HashMap<PhotoId, Option<Vec<f32>>> = ids.iter().copied().zip(embs.iter().cloned()).collect();
        group_bursts(c, ids, &|id| map.get(&id).cloned().flatten(), sharp, s)
    }

    fn sizes(b: &[Burst]) -> Vec<usize> {
        b.iter().map(|x| x.photos.len()).collect()
    }

    #[test]
    fn constants_are_pinned() {
        assert_eq!((BurstStrictness::Strict.window_secs(), BurstStrictness::Strict.min_cosine()), (2, 0.97));
        assert_eq!((BurstStrictness::Normal.window_secs(), BurstStrictness::Normal.min_cosine()), (5, 0.93));
        assert_eq!((BurstStrictness::Loose.window_secs(), BurstStrictness::Loose.min_cosine()), (15, 0.88));
        assert_eq!(BurstStrictness::default(), BurstStrictness::Normal);
        assert_eq!(BurstExport::default(), BurstExport::BestOnly);
    }

    #[test]
    fn settings_serde_camel_case_with_defaults() {
        let s: BurstSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(s, BurstSettings::default());
        assert!(!s.enabled && s.strictness == BurstStrictness::Normal && s.export == BurstExport::BestOnly);
        let s: BurstSettings = serde_json::from_str(r#"{"enabled":true,"strictness":"loose","export":"all","alsoStackInLibrary":true}"#).unwrap();
        assert!(s.enabled && s.also_stack_in_library);
        assert_eq!((s.strictness, s.export), (BurstStrictness::Loose, BurstExport::All));
    }

    #[test]
    fn time_threshold_at_each_strictness() {
        for (s, w) in [(BurstStrictness::Strict, 2), (BurstStrictness::Normal, 5), (BurstStrictness::Loose, 15)] {
            let t = |sec: i64| format!("2026-04-01T10:00:{:02}", sec + 10);
            let (inside, outside) = (t(w), t(w + 1));
            let (c, ids) = cat(&[(Some("2026-04-01T10:00:10"), 0), (Some(&inside), 0), (Some("2026-04-01T11:00:00"), 0)]);
            let e = vec![Some(at(0.0)); 3];
            assert_eq!(sizes(&group(&c, &ids, &e, &HashMap::new(), s)), vec![2, 1], "{s:?} inside");
            let (c, ids) = cat(&[(Some("2026-04-01T10:00:10"), 0), (Some(&outside), 0)]);
            let e = vec![Some(at(0.0)); 2];
            assert_eq!(sizes(&group(&c, &ids, &e, &HashMap::new(), s)), vec![1, 1], "{s:?} outside");
        }
    }

    #[test]
    fn cosine_threshold_at_each_strictness() {
        for s in [BurstStrictness::Strict, BurstStrictness::Normal, BurstStrictness::Loose] {
            let deg = s.min_cosine().acos().to_degrees();
            let (c, ids) = cat(&[(Some("2026-04-01T10:00:00"), 0), (Some("2026-04-01T10:00:01"), 0)]);
            let inside = [Some(at(0.0)), Some(at(deg - 0.2))];
            let outside = [Some(at(0.0)), Some(at(deg + 0.2))];
            assert_eq!(sizes(&group(&c, &ids, &inside, &HashMap::new(), s)), vec![2], "{s:?} inside");
            assert_eq!(sizes(&group(&c, &ids, &outside, &HashMap::new(), s)), vec![1, 1], "{s:?} outside");
        }
    }

    #[test]
    fn strictness_changes_the_result() {
        let (c, ids) = cat(&[(Some("2026-04-01T10:00:00"), 0), (Some("2026-04-01T10:00:04"), 0)]);
        let e = [Some(at(0.0)), Some(at(20.0))]; // cos ~ 0.94, 4 s apart
        assert_eq!(sizes(&group(&c, &ids, &e, &HashMap::new(), BurstStrictness::Strict)), vec![1, 1]);
        assert_eq!(sizes(&group(&c, &ids, &e, &HashMap::new(), BurstStrictness::Normal)), vec![2]);
        assert_eq!(sizes(&group(&c, &ids, &e, &HashMap::new(), BurstStrictness::Loose)), vec![2]);
    }

    #[test]
    fn chaining_compares_with_previous_photo_only() {
        // 0, 14, 28 degrees: neighbours cos ~0.970, ends cos ~0.883 (< Normal) but still chained.
        let (c, ids) = cat(&[(Some("2026-04-01T10:00:00"), 0), (Some("2026-04-01T10:00:02"), 0), (Some("2026-04-01T10:00:04"), 0)]);
        let e = [Some(at(0.0)), Some(at(14.0)), Some(at(28.0))];
        assert_eq!(sizes(&group(&c, &ids, &e, &HashMap::new(), BurstStrictness::Normal)), vec![3]);
        // A broken link splits the stack.
        let e = [Some(at(0.0)), Some(at(1.0)), Some(at(60.0))];
        assert_eq!(sizes(&group(&c, &ids, &e, &HashMap::new(), BurstStrictness::Normal)), vec![2, 1]);
    }

    #[test]
    fn missing_time_or_embedding_are_singletons() {
        let (c, ids) = cat(&[(None, 0), (None, 0), (Some("2026-04-01T10:00:00"), 0), (Some("2026-04-01T10:00:01"), 0), (Some("junk"), 0)]);
        let e = [Some(at(0.0)), Some(at(0.0)), Some(at(0.0)), None, Some(at(0.0))];
        let g = group(&c, &ids, &e, &HashMap::new(), BurstStrictness::Loose);
        assert_eq!(sizes(&g), vec![1; 5]);
        let mut all: Vec<PhotoId> = g.iter().map(|b| b.photos[0]).collect();
        all.sort();
        assert_eq!(all, ids);
        assert!(g.iter().all(|b| b.best == b.photos[0]));
    }

    #[test]
    fn best_shot_tie_break_order() {
        let t = ["2026-04-01T10:00:00", "2026-04-01T10:00:01", "2026-04-01T10:00:02"];
        let e = [Some(at(0.0)), Some(at(0.0)), Some(at(0.0))];
        // sharpness wins over rating
        let (c, ids) = cat(&[(Some(t[0]), 5), (Some(t[1]), 0), (Some(t[2]), 1)]);
        let sh: HashMap<_, _> = [(ids[0], 1.0), (ids[1], 9.0), (ids[2], 2.0)].into();
        assert_eq!(group(&c, &ids, &e, &sh, BurstStrictness::Normal)[0].best, ids[1]);
        // equal sharpness: rating
        let sh: HashMap<_, _> = ids.iter().map(|i| (*i, 3.0)).collect();
        assert_eq!(group(&c, &ids, &e, &sh, BurstStrictness::Normal)[0].best, ids[0]);
        // equal sharpness and rating: earliest capture (ids deliberately in reverse time order)
        let (c, ids) = cat(&[(Some(t[2]), 2), (Some(t[0]), 2), (Some(t[1]), 2)]);
        let g = group(&c, &ids, &e, &HashMap::new(), BurstStrictness::Normal);
        assert_eq!(g[0].photos, vec![ids[1], ids[2], ids[0]]);
        assert_eq!(g[0].best, ids[1]);
        // identical capture time: lowest id
        let (c, ids) = cat(&[(Some(t[0]), 0), (Some(t[0]), 0)]);
        let g = group(&c, &[ids[1], ids[0]], &[Some(at(0.0)), Some(at(0.0))], &HashMap::new(), BurstStrictness::Normal);
        assert_eq!(g[0].best, ids[0]);
    }

    #[test]
    fn export_modes() {
        let (c, ids) = cat(&[(Some("2026-04-01T10:00:00"), 0), (Some("2026-04-01T10:00:01"), 0), (Some("2026-04-01T12:00:00"), 0)]);
        let e = vec![Some(at(0.0)); 3];
        let sh: HashMap<_, _> = [(ids[1], 5.0)].into();
        let g = group(&c, &ids, &e, &sh, BurstStrictness::Normal);
        assert_eq!(export_ids(&g, BurstExport::BestOnly), vec![ids[1], ids[2]]);
        assert_eq!(export_ids(&g, BurstExport::All), vec![ids[0], ids[1], ids[2]]);
    }

    #[test]
    fn deterministic_under_permutation_and_covers_every_id() {
        let specs: Vec<(Option<&str>, u8)> = vec![
            (Some("2026-04-01T10:00:00"), 1),
            (Some("2026-04-01T10:00:01"), 3),
            (Some("2026-04-01T10:00:02"), 2),
            (Some("2026-04-01T10:05:00"), 0),
            (Some("2026-04-01T10:05:01"), 0),
            (None, 0),
        ];
        let (c, ids) = cat(&specs);
        let embs: HashMap<PhotoId, Vec<f32>> = ids.iter().map(|i| (*i, at(0.0))).collect();
        let sh: HashMap<PhotoId, f32> = ids.iter().map(|i| (*i, 1.0)).collect();
        let f = |id: PhotoId| embs.get(&id).cloned();
        let base = group_bursts(&c, &ids, &f, &sh, BurstStrictness::Normal);
        assert_eq!(sizes(&base), vec![3, 2, 1]);
        let mut perm = ids.clone();
        for k in 0..6 {
            perm.rotate_left(k % 5 + 1);
            if k % 2 == 0 {
                perm.reverse();
            }
            assert_eq!(group_bursts(&c, &perm, &f, &sh, BurstStrictness::Normal), base);
        }
        let mut covered: Vec<PhotoId> = base.iter().flat_map(|b| b.photos.clone()).collect();
        covered.sort();
        assert_eq!(covered, ids);
        let dup: Vec<PhotoId> = ids.iter().chain(ids.iter()).copied().collect();
        assert_eq!(group_bursts(&c, &dup, &f, &sh, BurstStrictness::Normal), base);
    }

    fn checker(n: usize, cell: usize) -> Rgba8 {
        Rgba8::from_fn(n, n, |x, y| {
            let v = if ((x / cell) + (y / cell)).is_multiple_of(2) { 235 } else { 20 };
            [v, v, v, 255]
        })
    }

    fn box_blur(img: &Rgba8, r: isize) -> Rgba8 {
        Rgba8::from_fn(img.width, img.height, |x, y| {
            let (mut s, mut n) = (0u32, 0u32);
            for dy in -r..=r {
                for dx in -r..=r {
                    s += img.get_clamped(x as isize + dx, y as isize + dy)[0] as u32;
                    n += 1;
                }
            }
            let v = (s / n) as u8;
            [v, v, v, 255]
        })
    }

    #[test]
    fn sharpness_ranks_sharp_above_blurred() {
        let sharp = checker(64, 4);
        let soft = box_blur(&sharp, 2);
        let softer = box_blur(&sharp, 4);
        let flat = Rgba8::from_fn(64, 64, |_, _| [128, 128, 128, 255]);
        let (a, b, c, d) = (sharpness(&sharp), sharpness(&soft), sharpness(&softer), sharpness(&flat));
        assert!(a > b && b > c && c >= d, "{a} {b} {c} {d}");
        assert_eq!(d, 0.0);
        assert_eq!(sharpness(&Rgba8::new(2, 2)), 0.0);
        // large images are analysed downsampled and still rank correctly
        let big = checker(600, 8);
        assert!(sharpness(&big) > sharpness(&box_blur(&big, 3)));
    }
}
