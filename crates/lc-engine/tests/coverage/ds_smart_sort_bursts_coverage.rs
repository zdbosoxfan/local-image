use lightcraft_engine::catalog::{Catalog, Op, Photo, PhotoId, Source};
use lightcraft_engine::smart_sort::bursts::{Burst, BurstExport, BurstSettings, BurstStrictness, export_ids, group_bursts};
use std::collections::HashMap;

// Helper: create a catalog with one photo per `(captured, rating)` entry.
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

// Helper: create an embedding vector at `deg` degrees from the x-axis.
fn at(deg: f32) -> Vec<f32> {
    let r = deg.to_radians();
    vec![r.cos(), r.sin(), 0.0]
}

// Helper: call group_bursts with a map built from `embs`.
fn group(c: &Catalog, ids: &[PhotoId], embs: &[Option<Vec<f32>>], sharp: &HashMap<PhotoId, f32>, s: BurstStrictness) -> Vec<Burst> {
    let map: HashMap<PhotoId, Option<Vec<f32>>> = ids.iter().copied().zip(embs.iter().cloned()).collect();
    group_bursts(c, ids, &|id| map.get(&id).cloned().flatten(), sharp, s)
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
fn settings_serde_defaults() {
    let s: BurstSettings = serde_json::from_str("{}").unwrap();
    assert_eq!(s, BurstSettings::default());
    assert!(!s.enabled);
    assert_eq!(s.strictness, BurstStrictness::Normal);
    assert_eq!(s.export, BurstExport::BestOnly);
    assert!(!s.also_stack_in_library);
}

#[test]
fn settings_serde_round_trip() {
    let original = BurstSettings { enabled: true, strictness: BurstStrictness::Loose, export: BurstExport::All, also_stack_in_library: true };
    let json = serde_json::to_string(&original).unwrap();
    let round_tripped: BurstSettings = serde_json::from_str(&json).unwrap();
    assert_eq!(original, round_tripped);
}

#[test]
fn burst_and_export_serde_round_trip() {
    let photos = vec![PhotoId(5), PhotoId(3), PhotoId(7)];
    let best = PhotoId(3);
    let original_burst = Burst { photos, best };
    let json = serde_json::to_string(&original_burst).unwrap();
    let round_tripped_burst: Burst = serde_json::from_str(&json).unwrap();
    assert_eq!(original_burst, round_tripped_burst);

    let original_export = BurstExport::All;
    let json = serde_json::to_string(&original_export).unwrap();
    let round_tripped_export: BurstExport = serde_json::from_str(&json).unwrap();
    assert_eq!(original_export, round_tripped_export);
}

#[test]
fn group_bursts_empty_and_single() {
    let (c, ids) = cat(&[(Some("2026-04-01T10:00:00"), 0)]);
    let emb = vec![Some(at(0.0))];
    let g = group(&c, &ids, &emb, &HashMap::new(), BurstStrictness::Normal);
    assert_eq!(g.len(), 1);
    assert_eq!(g[0].photos, ids);
    assert_eq!(g[0].best, ids[0]);

    let empty = group_bursts(&c, &[], &|_| None, &HashMap::new(), BurstStrictness::Normal);
    assert!(empty.is_empty());
}

#[test]
fn group_bursts_unknown_ids_are_singletons() {
    let (c, _) = cat(&[(Some("2026-04-01T10:00:00"), 0)]);
    let unknown = [PhotoId(999), PhotoId(1000)];
    let g = group_bursts(&c, &unknown, &|_| None, &HashMap::new(), BurstStrictness::Normal);
    assert_eq!(g.len(), 2);
    assert!(g.iter().all(|b| b.photos.len() == 1 && b.best == b.photos[0]));
}

#[test]
fn group_bursts_duplicate_ids_ignored() {
    let (c, ids) = cat(&[(Some("2026-04-01T10:00:00"), 0), (Some("2026-04-01T10:00:01"), 0)]);
    let emb = [Some(at(0.0)), Some(at(0.0))];
    let dup = vec![ids[0], ids[1], ids[0], ids[1]];
    let g = group(&c, &dup, &emb, &HashMap::new(), BurstStrictness::Normal);
    assert_eq!(g.len(), 1);
    assert_eq!(g[0].photos, ids);
}

#[test]
fn time_threshold_at_each_strictness() {
    for (s, w) in [(BurstStrictness::Strict, 2), (BurstStrictness::Normal, 5), (BurstStrictness::Loose, 15)] {
        let t = |sec: i64| format!("2026-04-01T10:00:{:02}", sec + 10);
        let inside = t(w);
        let outside = t(w + 1);

        // inside window: second photo joins first
        let (c, ids) = cat(&[(Some("2026-04-01T10:00:10"), 0), (Some(&inside), 0), (Some("2026-04-01T11:00:00"), 0)]);
        let e = vec![Some(at(0.0)); 3];
        let sizes: Vec<usize> = group(&c, &ids, &e, &HashMap::new(), s).iter().map(|b| b.photos.len()).collect();
        assert_eq!(sizes, vec![2, 1], "{s:?} inside");

        // outside window: separate bursts
        let (c, ids) = cat(&[(Some("2026-04-01T10:00:10"), 0), (Some(&outside), 0)]);
        let e = vec![Some(at(0.0)); 2];
        let sizes: Vec<usize> = group(&c, &ids, &e, &HashMap::new(), s).iter().map(|b| b.photos.len()).collect();
        assert_eq!(sizes, vec![1, 1], "{s:?} outside");
    }
}

#[test]
fn cosine_threshold_at_each_strictness() {
    for s in [BurstStrictness::Strict, BurstStrictness::Normal, BurstStrictness::Loose] {
        let deg = s.min_cosine().acos().to_degrees();
        let (c, ids) = cat(&[(Some("2026-04-01T10:00:00"), 0), (Some("2026-04-01T10:00:01"), 0)]);

        // just above threshold -> one burst
        let inside = [Some(at(0.0)), Some(at(deg - 0.2))];
        assert_eq!(group(&c, &ids, &inside, &HashMap::new(), s).iter().map(|b| b.photos.len()).collect::<Vec<_>>(), vec![2], "{s:?} inside");

        // just below threshold -> two singletons
        let outside = [Some(at(0.0)), Some(at(deg + 0.2))];
        assert_eq!(group(&c, &ids, &outside, &HashMap::new(), s).iter().map(|b| b.photos.len()).collect::<Vec<_>>(), vec![1, 1], "{s:?} outside");
    }
}

#[test]
fn strictness_changes_the_result() {
    let (c, ids) = cat(&[(Some("2026-04-01T10:00:00"), 0), (Some("2026-04-01T10:00:04"), 0)]);
    let e = [Some(at(0.0)), Some(at(20.0))]; // cos ~0.94, 4 s apart

    assert_eq!(group(&c, &ids, &e, &HashMap::new(), BurstStrictness::Strict).iter().map(|b| b.photos.len()).collect::<Vec<_>>(), vec![1, 1]);
    assert_eq!(group(&c, &ids, &e, &HashMap::new(), BurstStrictness::Normal).iter().map(|b| b.photos.len()).collect::<Vec<_>>(), vec![2]);
    assert_eq!(group(&c, &ids, &e, &HashMap::new(), BurstStrictness::Loose).iter().map(|b| b.photos.len()).collect::<Vec<_>>(), vec![2]);
}

#[test]
fn chaining_compares_with_previous_photo_only() {
    // 0, 14, 28 degrees: neighbours cos ~0.970, ends cos ~0.883 (< Normal) but still chained.
    let (c, ids) = cat(&[(Some("2026-04-01T10:00:00"), 0), (Some("2026-04-01T10:00:02"), 0), (Some("2026-04-01T10:00:04"), 0)]);
    let e = [Some(at(0.0)), Some(at(14.0)), Some(at(28.0))];
    assert_eq!(group(&c, &ids, &e, &HashMap::new(), BurstStrictness::Normal).iter().map(|b| b.photos.len()).collect::<Vec<_>>(), vec![3]);

    // A broken link splits the stack.
    let e = [Some(at(0.0)), Some(at(1.0)), Some(at(60.0))];
    assert_eq!(group(&c, &ids, &e, &HashMap::new(), BurstStrictness::Normal).iter().map(|b| b.photos.len()).collect::<Vec<_>>(), vec![2, 1]);
}

#[test]
fn missing_time_or_embedding_are_singletons() {
    let (c, ids) = cat(&[(None, 0), (None, 0), (Some("2026-04-01T10:00:00"), 0), (Some("2026-04-01T10:00:01"), 0), (Some("junk"), 0)]);
    let e = [Some(at(0.0)), Some(at(0.0)), Some(at(0.0)), None, Some(at(0.0))];
    let g = group(&c, &ids, &e, &HashMap::new(), BurstStrictness::Loose);
    let sizes: Vec<usize> = g.iter().map(|b| b.photos.len()).collect();
    assert_eq!(sizes, vec![1; 5]);
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
fn untimed_singletons_ordered_by_id_last() {
    let specs = vec![(Some("2026-04-01T10:00:00"), 0), (Some("2026-04-01T10:00:01"), 0), (None, 0), (None, 0)];
    let (c, ids) = cat(&specs);
    // Embeddings for all: timed pair should stack, untimed stay singletons.
    let emb = [Some(at(0.0)), Some(at(0.0)), Some(at(0.0)), Some(at(0.0))];
    let g = group(&c, &ids, &emb, &HashMap::new(), BurstStrictness::Normal);

    assert_eq!(g.len(), 3); // one burst of 2 + two singletons
    assert_eq!(g[0].photos, vec![ids[0], ids[1]]);
    assert_eq!(g[1].photos, vec![ids[2]]);
    assert_eq!(g[2].photos, vec![ids[3]]);
    // Untimed ids sorted ascending, placed after timed burst.
    assert!(g[1].photos[0] < g[2].photos[0]);
}

#[test]
fn deterministic_under_permutation_and_covers_all() {
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

    let sizes: Vec<usize> = base.iter().map(|b| b.photos.len()).collect();
    assert_eq!(sizes, vec![3, 2, 1]);

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

#[test]
fn export_ids_modes() {
    let (c, ids) = cat(&[(Some("2026-04-01T10:00:00"), 0), (Some("2026-04-01T10:00:01"), 0), (Some("2026-04-01T12:00:00"), 0)]);
    let e = vec![Some(at(0.0)); 3];
    let sh: HashMap<_, _> = [(ids[1], 5.0)].into();
    let bursts = group(&c, &ids, &e, &sh, BurstStrictness::Normal);

    assert_eq!(export_ids(&bursts, BurstExport::BestOnly), vec![ids[1], ids[2]]);
    assert_eq!(export_ids(&bursts, BurstExport::All), vec![ids[0], ids[1], ids[2]]);
    assert!(export_ids(&[], BurstExport::BestOnly).is_empty());
}

#[test]
fn non_finite_sharpness_ignored_in_best_shot() {
    let t = ["2026-04-01T10:00:00", "2026-04-01T10:00:01", "2026-04-01T10:00:02"];
    let e = [Some(at(0.0)), Some(at(0.0)), Some(at(0.0))];

    // finite sharpness should win over NaN/Inf
    let (c, ids) = cat(&[(Some(t[0]), 0), (Some(t[1]), 0), (Some(t[2]), 0)]);
    let sh: HashMap<_, _> = [(ids[0], f32::NAN), (ids[1], f32::INFINITY), (ids[2], 1.5)].into();
    let g = group(&c, &ids, &e, &sh, BurstStrictness::Normal);
    assert_eq!(g[0].best, ids[2]);

    // all non-finite: should fall back to rating/time/id
    let sh: HashMap<_, _> = [(ids[0], f32::NAN), (ids[1], f32::NEG_INFINITY), (ids[2], f32::INFINITY)].into();
    let g = group(&c, &ids, &e, &sh, BurstStrictness::Normal);
    assert_eq!(g[0].best, ids[0]); // equal rating, earliest capture
}

#[test]
fn unparseable_time_is_singleton_last() {
    let (c, ids) = cat(&[(Some("2026-04-01T10:00:00"), 0), (Some("not-a-date"), 0)]);
    let e = [Some(at(0.0)), Some(at(0.0))];
    let g = group(&c, &ids, &e, &HashMap::new(), BurstStrictness::Normal);
    assert_eq!(g.len(), 2);
    assert_eq!(g[0].photos, vec![ids[0]]);
    assert_eq!(g[1].photos, vec![ids[1]]);
    assert!(g[1].best == ids[1]);
}
