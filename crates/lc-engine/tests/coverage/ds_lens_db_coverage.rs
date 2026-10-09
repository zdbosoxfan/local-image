use lightcraft_engine::lens_db::{self, Named, Query, correction, detect, find_camera, search};
use std::collections::HashSet;

/// A known Canon camera/lens pair with calibration data in the bundled database.
fn canon_query(focal: f32, aperture: Option<f32>, distance: Option<f32>) -> Query {
    let d = detect("Canon Canon EOS 5D Mark IV", "EF24-70mm f/2.8L II USM");
    let camera = d.camera.expect("bundled database knows Canon 5D Mark IV");
    let lens = d.lens.expect("bundled database knows Canon EF 24-70mm f/2.8L II");
    Query {
        exif_camera: "Canon Canon EOS 5D Mark IV".into(),
        exif_lens: "EF24-70mm f/2.8L II USM".into(),
        camera: Some(camera),
        lens: Some(lens),
        focal,
        aperture,
        distance,
    }
}

#[test]
fn database_loads_and_has_expected_entries() {
    let db = lens_db::database().expect("bundled lens database");
    assert!(db.cameras.len() > 500, "expected >500 cameras, got {}", db.cameras.len());
    assert!(db.lenses.len() > 500, "expected >500 lenses, got {}", db.lenses.len());
}

#[test]
fn find_camera_empty_and_unknown_returns_none() {
    let db = lens_db::database().unwrap();
    assert_eq!(find_camera(db, ""), None);
    assert_eq!(find_camera(db, "Nonexistent Cam 9000"), None);
}

#[test]
fn find_camera_known_models() {
    let db = lens_db::database().unwrap();

    let nikon = find_camera(db, "NIKON CORPORATION NIKON D750").expect("D750");
    assert!(nikon.model.contains("D750"), "{nikon:?}");

    let canon = find_camera(db, "Canon Canon EOS 5D Mark IV").expect("5D IV");
    assert!(canon.model.contains("5D Mark IV"), "{}", canon.model);
}

#[test]
fn detect_finds_camera_and_lens() {
    let d = detect("Canon Canon EOS 5D Mark IV", "EF24-70mm f/2.8L II USM");
    assert!(d.camera.as_ref().is_some_and(|c| c.model.contains("5D Mark IV")));
    assert!(d.lens.as_ref().is_some_and(|l| l.model.contains("24-70")));
}

#[test]
fn detect_empty_lens_name_yields_camera_only() {
    let d = detect("Canon Canon EOS 5D Mark IV", "");
    assert!(d.camera.is_some());
    assert_eq!(d.lens, None);
}

#[test]
fn detect_unknown_camera_gives_no_camera() {
    let d = detect("Unknown Camera Brand", "Some Unknown Lens");
    assert!(d.camera.is_none());
}

#[test]
fn named_label_avoids_repeating_maker() {
    let n = Named { maker: "Canon".into(), model: "Canon EOS 5D Mark IV".into() };
    assert_eq!(n.label(), "Canon EOS 5D Mark IV");

    let n = Named { maker: "Nikon".into(), model: "AF-S NIKKOR 50mm f/1.8G".into() };
    assert_eq!(n.label(), "Nikon AF-S NIKKOR 50mm f/1.8G");

    let n = Named { maker: "".into(), model: "Generic Lens".into() };
    assert_eq!(n.label(), "Generic Lens");
}

#[test]
fn named_serde_round_trip() {
    let n = Named { maker: "Sony".into(), model: "FE 35mm F1.4".into() };
    let json = serde_json::to_string(&n).unwrap();
    let back: Named = serde_json::from_str(&json).unwrap();
    assert_eq!(n, back);
}

#[test]
fn search_returns_sorted_unique_limited() {
    let found = search(None, "", 30);
    assert!(found.len() <= 30, "limit not respected: {}", found.len());

    let labels: Vec<String> = found.iter().map(Named::label).map(|s| s.to_lowercase()).collect();
    let mut sorted = labels.clone();
    sorted.sort();
    assert_eq!(labels, sorted, "results are not sorted by label");

    let unique: HashSet<Named> = found.iter().cloned().collect();
    assert_eq!(unique.len(), found.len(), "results contain duplicates");
}

#[test]
fn search_query_filters_lens_names() {
    let cam = detect("Canon Canon EOS 5D Mark IV", "").camera.expect("known camera");
    let found = search(Some(&cam), "24-70", 20);
    assert!(!found.is_empty(), "expected 24-70 lenses for this camera");
    assert!(found.iter().all(|l| l.label().to_lowercase().contains("24-70")), "query filter not applied: {found:?}");
}

#[test]
fn search_limit_zero_returns_empty() {
    assert!(search(None, "any", 0).is_empty());
}

#[test]
fn correction_unknown_camera_and_lens_none() {
    let q = Query {
        exif_camera: "Unknown Camera".into(),
        exif_lens: "Mystery lens".into(),
        camera: None,
        lens: None,
        focal: 50.0,
        aperture: Some(2.8),
        distance: None,
    };
    assert_eq!(correction(&q), None);
}

#[test]
fn correction_missing_or_invalid_focal_none() {
    let mut q = canon_query(0.0, Some(2.8), None);
    assert_eq!(correction(&q), None);

    q.focal = f32::NAN;
    assert_eq!(correction(&q), None);

    q.focal = -1.0;
    assert_eq!(correction(&q), None);
}

#[test]
fn correction_known_lens_non_identity_and_deterministic() {
    let q = canon_query(50.0, Some(2.8), None);
    let first = correction(&q).expect("correction for known lens");
    assert!(!first.is_identity(), "correction should not be identity");

    let second = correction(&q).expect("cached correction");
    assert_eq!(first, second, "same query must produce the same correction");
}

#[test]
fn correction_aperture_none_or_zero_still_some() {
    let q_none = canon_query(50.0, None, None);
    assert!(correction(&q_none).is_some(), "correction with no aperture should be Some");

    let q_zero = canon_query(50.0, Some(0.0), None);
    assert!(correction(&q_zero).is_some(), "correction with zero aperture should be Some");
}

#[test]
fn correction_distance_zero_matches_none() {
    let q_none = canon_query(50.0, Some(2.8), None);
    let q_zero = canon_query(50.0, Some(2.8), Some(0.0));
    assert_eq!(correction(&q_none), correction(&q_zero));
}
