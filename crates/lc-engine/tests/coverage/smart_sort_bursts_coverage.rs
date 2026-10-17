use lightcraft_engine::catalog::{Catalog, Op, Photo, PhotoId, Source};
use lightcraft_engine::smart_sort::bursts::{Burst, BurstStrictness, group_bursts};
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
