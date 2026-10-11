#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use photocraft_doc::{Color, ColorMode, Fill, SampleType, Size};
// Test-only persistence bridge; production never stores effects in PSD blocks.
pub(super) fn access_test() -> Access {
    Access {
        read: |l| {
            l.psd_blocks.iter().find(|(k, _)| k == b"tF45").map_or_else(|| Ok(Vec::new()), |(_, v)| serde_json::from_slice(v).map_err(|e| bad(e.to_string())))
        },
        write: |l, e| {
            l.psd_blocks.retain(|(k, _)| k != b"tF45");
            if !e.is_empty() {
                l.psd_blocks.push((*b"tF45", serde_json::to_vec(&e).map_err(|e| bad(e.to_string()))?.into()));
            }
            Ok(())
        },
    }
}
pub(super) fn fixture() -> (Session, Vec<LayerId>) {
    let mut doc = Document::new("Effects45", Size::new(160, 128), ColorMode::Rgb, SampleType::F32);
    let mut ids = Vec::new();
    for x in [10., 90.] {
        let sh = ShapeLayer {
            path: photocraft_vector::shapes::rect(x, 10., 50., 60.),
            fill: Some(Fill::Solid(Color::rgba(0.2, 0.5, 0.8, 0.5))),
            ..Default::default()
        };
        let mut layer = Layer::new("Source", LayerContent::Shape(sh));
        refresh(&doc, &mut layer, access_test()).unwrap();
        ids.push(layer.id);
        doc.layers.push(layer);
    }
    let mut s = Session::new();
    s.add_document(doc, None);
    (s, ids)
}
fn effect() -> PathEffect {
    PathEffect::new(EffectKind::RoundCorners { radius: 10. })
}
#[test]
fn path_effects45_engine_preview_atomic_release_undo_redo_and_source_reprojection() {
    let (mut s, ids) = fixture();
    let before = s.active().unwrap().doc.clone();
    let mut g = Gesture::begin(&s, &ids, access_test()).unwrap();
    g.update(&s, |e| {
        e.push(effect());
        Ok(())
    })
    .unwrap();
    let p = prepare(&s, &g.request, access_test()).unwrap();
    assert_eq!(s.active().unwrap().doc, before);
    assert_eq!(s.active().unwrap().history.past_len(), 0);
    assert!(s.journal.is_empty());
    assert_ne!(photocraft_compose::flatten(&p.preview).px, photocraft_compose::flatten(&before).px);
    commit_with_access(&mut s, &g.request, access_test()).unwrap();
    assert_eq!(s.active().unwrap().history.past_len(), 1);
    for id in &ids {
        assert_eq!((access_test().read)(s.active().unwrap().doc.layer(*id).unwrap()).unwrap(), vec![effect()]);
        assert_eq!(source_path(s.active().unwrap().doc.layer(*id).unwrap()), source_path(before.layer(*id).unwrap()));
    }
    let committed = s.active().unwrap().doc.clone();
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
    assert!(s.redo());
    assert_eq!(s.active().unwrap().doc, committed);
    let mut cache = ProjectionCache::default();
    let d = s.active().unwrap().doc.clone();
    let one = project_document(&d, access_test(), &mut cache).unwrap();
    let mut edited = (*d).clone();
    if let LayerContent::Shape(sh) = &mut edited.layer_mut(ids[0]).unwrap().content {
        sh.path = photocraft_vector::shapes::rect(10., 10., 70., 40.);
    }
    let two = project_document(&edited, access_test(), &mut cache).unwrap();
    assert_ne!(photocraft_compose::flatten(&one).px, photocraft_compose::flatten(&two).px);
    assert_eq!(cache.hits, 1);
    assert_eq!(cache.misses, 3);
}
#[test]
fn path_effects45_engine_rejects_entire_set_and_stale_locked_duplicate_targets() {
    let (mut s, ids) = fixture();
    let before = s.active().unwrap().doc.clone();
    let mut g = Gesture::begin(&s, &ids, access_test()).unwrap();
    g.update(&s, |e| {
        e.push(effect());
        Ok(())
    })
    .unwrap();
    let mut request = g.request.clone();
    request.edits[1].effects[0].effect = EffectKind::Twist { angle: f64::NAN };
    assert!(commit_with_access(&mut s, &request, access_test()).is_err());
    assert_eq!(s.active().unwrap().doc, before);
    assert!(Gesture::begin(&s, &[ids[0], ids[0]], access_test()).is_err());
    s.edit("unrelated edit", |d, _| {
        d.name = "Changed".into();
        Ok(())
    })
    .unwrap();
    let after = s.active().unwrap().doc.clone();
    assert!(commit_with_access(&mut s, &g.request, access_test()).is_err());
    assert_eq!(s.active().unwrap().doc, after);
    s.edit("lock", |d, _| {
        d.layer_mut(ids[1]).unwrap().locks.pixels = true;
        Ok(())
    })
    .unwrap();
    assert!(Gesture::begin(&s, &ids, access_test()).is_err());
}
#[test]
fn path_effects45_engine_cache_hits_unrelated_edits_and_effect_changes_miss() {
    let (s, ids) = fixture();
    let mut d = (*s.active().unwrap().doc).clone();
    let mut c = ProjectionCache::default();
    let effects = vec![effect()];
    let a = c.get(d.layer(ids[0]).unwrap(), &effects, d.resolution_dpi).unwrap();
    d.name = "Unrelated".into();
    d.layer_mut(ids[1]).unwrap().opacity = 0.2;
    let b = c.get(d.layer(ids[0]).unwrap(), &effects, d.resolution_dpi).unwrap();
    assert!(Arc::ptr_eq(&a, &b));
    assert_eq!(c.hits, 1);
    assert_eq!(c.misses, 1);
    c.get(d.layer(ids[0]).unwrap(), &[PathEffect::new(EffectKind::Twist { angle: 45. })], d.resolution_dpi).unwrap();
    assert_eq!(c.misses, 2);
    (access_test().write)(d.layer_mut(ids[0]).unwrap(), effects).unwrap();
    let before = project_document(&d, access_test(), &mut c).unwrap();
    d.name = "Another unrelated edit".into();
    let after = project_document(&d, access_test(), &mut c).unwrap();
    let child_ids = |doc: &Document| doc.layer(ids[0]).unwrap().children().unwrap().iter().map(|l| l.id).collect::<Vec<_>>();
    assert_eq!(child_ids(&before), child_ids(&after), "unrelated edits keep 09 child cache identities");
}
#[test]
fn path_effects45_transform_three_copies_matches_expanded_duplicate_pixels_exactly() {
    let (s, ids) = fixture();
    let d = s.active().unwrap().doc.clone();
    let layer = d.layer(ids[0]).unwrap();
    let effect = PathEffect::new(EffectKind::Transform { scale: [100.; 2], move_by: [12., 4.], angle: 0., reflect: [false; 2], copies: 3 });
    let scene = project(layer, &[effect], d.resolution_dpi).unwrap();
    let plan = geometry::render::Compiled::new(&scene, &d.patterns, d.bounds(), &super::super::expand::Projection).unwrap();
    let projected = projected_layer(layer, &scene, &d).unwrap();
    let mut expanded = (*d).clone();
    expanded.layers = vec![projected];
    let mut independent = (*d).clone();
    independent.layers.clear();
    for i in 0..4 {
        let mut l = layer.clone();
        l.id = LayerId(10000 + i);
        if let LayerContent::Shape(sh) = &mut l.content {
            sh.path = sh.path.transform(&photocraft_geom::Affine::translate(i as f64 * 12., i as f64 * 4.));
            sh.cache = Some(photocraft_vector::render_shape(sh, d.pixel_format(), d.bounds()));
        }
        independent.layers.push(l);
    }
    assert_eq!(photocraft_compose::flatten(&expanded).to_rgba8(), photocraft_compose::flatten(&independent).to_rgba8());
    let rendered = render(&plan, &d).unwrap();
    let reference = photocraft_compose::flatten(&independent);
    for y in 0..d.size.height {
        for x in 0..d.size.width {
            let a = rendered.rgba(x as i32, y as i32);
            let b = reference.px[(y * d.size.width + x) as usize];
            for c in 0..4 {
                assert!((a[c] - b[c]).abs() < 1e-7, "{a:?} {b:?}");
            }
        }
    }
}
#[test]
fn path_effects45_engine_command_journal_flags_and_cache_bounds() {
    let specs = specs();
    assert!(specs[0].journal);
    assert!(!specs[1].journal);
    assert!(!specs[2].journal);
    let (s, ids) = fixture();
    let d = &s.active().unwrap().doc;
    let scene = project(d.layer(ids[0]).unwrap(), &[effect()], d.resolution_dpi).unwrap();
    let plan = geometry::render::Compiled::new(&scene, &d.patterns, d.bounds(), &super::super::expand::Projection).unwrap();
    assert!(plan.bounds().size().area() < d.bounds().size().area());
    // Dissolve belongs to appearance coverage, rather than a second layer-level pass.
    let mut source = d.layer(ids[0]).unwrap().clone();
    let LayerContent::Shape(sh) = &source.content else { panic!("shape") };
    let mut appearance = photocraft_vector::appearance::legacy_appearance(sh);
    let photocraft_doc::vector_design::appearance::AppearanceItem::Fill(fill) = &mut appearance.items[0] else { panic!("fill") };
    fill.blend = photocraft_doc::BlendMode::Dissolve;
    source.vector.appearance = Some(appearance);
    let scene = project(&source, &[effect()], d.resolution_dpi).unwrap();
    let plan = geometry::render::Compiled::new(&scene, &d.patterns, d.bounds(), &super::super::expand::Projection).unwrap();
    let merged = render(&plan, d).unwrap();
    let mut detached = (**d).clone();
    detached.layers = vec![projected_layer(&source, &scene, d).unwrap()];
    let ordinary = photocraft_compose::flatten(&detached);
    for y in 0..d.size.height {
        for x in 0..d.size.width {
            assert_eq!(merged.rgba(x as i32, y as i32), ordinary.px[(y * d.size.width + x) as usize]);
        }
    }
    let g = Gesture::begin(&s, &ids, access_test()).unwrap();
    assert!(prepare(&s, &g.request, Access { write: |_, _| Err(bad("injected persistence failure")), ..access_test() }).is_err());
    assert_eq!(s.active().unwrap().history.past_len(), 0);
}

fn source_path(layer: &Layer) -> &photocraft_doc::Path {
    let LayerContent::Shape(sh) = &layer.content else { panic!("shape") };
    &sh.path
}
