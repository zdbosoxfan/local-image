//! Independent scene/pixel/budget oracles using original fixed-layout fixtures.
#![expect(clippy::let_underscore_must_use, reason = "baseline 2026-10-25: unreviewed")]

use photocraft_affinity::{Archive, Error, Limits, import, import_preview, import_with};
use photocraft_doc::{LayerContent, vector_design::exchange::ImportOptions};
use std::sync::atomic::AtomicBool;
const SHAPES: &[u8] = include_bytes!("fixtures/shapes.afdesign");
#[test]
fn affinity_import_fixed_rectangle_pixels_and_geometry() {
    let scene = import(SHAPES, &ImportOptions::default()).unwrap();
    assert_eq!((scene.size.width, scene.size.height), (48, 48));
    let shape = scene
        .layers
        .iter()
        .flat_map(|p| if let LayerContent::Group(g) = &p.content { g.children.iter() } else { panic!() })
        .find_map(|l| if let LayerContent::Shape(s) = &l.content { Some(s) } else { None })
        .unwrap();
    assert_eq!(shape.path.control_bounds(), Some((4., 4., 44., 44.)));
    let cache = shape.cache.as_ref().unwrap();
    assert_eq!(cache.rgba(10, 10), [1., 0., 0., 1.]);
    assert_eq!(cache.rgba(45, 10), [0.; 4]);
    let area: f32 = (0..48).flat_map(|y| (0..48).map(move |x| cache.rgba(x, y)[3])).sum();
    assert_eq!(area, 1600.);
}
#[test]
fn affinity_import_warning_and_preview_are_explicit() {
    let scene = import(include_bytes!("fixtures/warnings.afphoto"), &ImportOptions::default()).unwrap();
    assert!(scene.warnings.iter().any(|w| w.message.contains("attached")));
    let bytes = include_bytes!("fixtures/preview.afphoto");
    assert!(matches!(import(bytes, &ImportOptions::default()), Err(Error::Unsupported(_))));
    let scene = import_preview(bytes, &ImportOptions::default(), &AtomicBool::new(false), "unsupported object stream").unwrap();
    assert_eq!((scene.size.width, scene.size.height), (8, 6));
    assert_eq!(scene.warnings[0].feature, "preview-fallback");
    assert!(matches!(scene.layers[0].content, LayerContent::Raster(_)));
}
#[test]
fn affinity_import_limits_cancellation_corruption_and_pages() {
    let mut options = ImportOptions::default();
    for limits in [
        photocraft_doc::vector_design::exchange::ImportLimits { bytes: 10, ..options.limits },
        photocraft_doc::vector_design::exchange::ImportLimits { objects: 1, ..options.limits },
        photocraft_doc::vector_design::exchange::ImportLimits { depth: 0, ..options.limits },
        photocraft_doc::vector_design::exchange::ImportLimits { pixels: 100, ..options.limits },
        photocraft_doc::vector_design::exchange::ImportLimits { segments: 1, ..options.limits },
        photocraft_doc::vector_design::exchange::ImportLimits { decompressed_bytes: 100, ..options.limits },
    ] {
        options.limits = limits;
        assert!(matches!(import(SHAPES, &options), Err(Error::Limit(_))), "{limits:?}");
        options = ImportOptions::default();
    }
    assert_eq!(import_with(SHAPES, &options, None, &AtomicBool::new(true)), Err(Error::Cancelled));
    for dpi in [f32::NAN, f32::INFINITY, -1., 0.] {
        options.dpi = dpi;
        assert!(import(SHAPES, &options).is_err());
    }
    options = ImportOptions::default();
    options.pages = vec![1];
    assert!(import(SHAPES, &options).is_err());
    let mut bad = SHAPES.to_vec();
    bad[0] = 1;
    assert!(matches!(import(&bad, &ImportOptions::default()), Err(Error::Malformed(_))));
    let mut bad = SHAPES.to_vec();
    bad[80] ^= 1;
    assert!(import(&bad, &ImportOptions::default()).is_err());
    assert!(Archive::open(SHAPES, Limits::default()).is_ok());
}
#[test]
fn affinity_import_dpi_preserves_physical_size() {
    let scene = import(SHAPES, &ImportOptions { dpi: 144., ..Default::default() }).unwrap();
    assert_eq!((scene.size.width, scene.size.height), (96, 96));
    assert_eq!(scene.resolution_dpi, 144.);
}
proptest::proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(256))]
    #[test]
    fn affinity_import_mutations_never_panic(edits in proptest::collection::vec((0usize..500,proptest::prelude::any::<u8>()),0..12)){
        let mut bytes=SHAPES.to_vec();for (i,v) in edits{if let Some(b)=bytes.get_mut(i){*b=v;}}
        let _=import(&bytes,&ImportOptions::default());
    }
}
#[test]
fn affinity_import_cancellation_between_archive_operations() {
    let cancel = AtomicBool::new(false);
    let mut a = Archive::open_with(SHAPES, Limits::default(), Some(&cancel)).unwrap();
    cancel.store(true, std::sync::atomic::Ordering::Relaxed);
    assert_eq!(a.read("doc.dat"), Err(Error::Cancelled));
}
#[test]
#[ignore = "Requires the pinned MIT external corpus via AFFINITY_CORPUS_DIR"]
fn affinity_import_licensed_real_files() {
    let dir = std::env::var_os("AFFINITY_CORPUS_DIR").expect("external corpus required");
    for file in ["afdesignload-shape_test.afdesign", "afdesignload-raster_test.afdesign", "patchy-text-runs.af", "patchy-rgba8.af"] {
        let bytes = std::fs::read(std::path::Path::new(&dir).join(file)).unwrap();
        let scene = import(&bytes, &ImportOptions::default()).unwrap();
        assert!(scene.size.width > 0 && scene.size.height > 0 && !scene.layers.is_empty());
        eprintln!("real {file}: {}x{}, warnings {:?}", scene.size.width, scene.size.height, scene.warnings);
    }
}
