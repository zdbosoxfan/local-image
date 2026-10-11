#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use photocraft_doc::{ColorMode, SampleType, Size};
use photocraft_vector::appearance::NoProjection;

pub(super) fn setup() -> (Session, LayerId, CenterlineTrace) {
    let mut doc = Document::new("Trace", Size::new(96, 96), ColorMode::Rgb, SampleType::F32);
    let mut source = Layer::raster("Scan", doc.pixel_format());
    if let LayerContent::Raster(surface) = &mut source.content {
        surface.fill_rect(doc.bounds(), &[1., 1., 1., 1.]);
    }
    let id = source.id;
    doc.layers.push(source);
    let mut s = Session::new();
    s.add_document(doc, None);
    s.active_mut().unwrap().active_layer = Some(id);
    s.active_mut().unwrap().selected_layers = vec![id];
    let data: Vec<u8> = (0..32 * 32).flat_map(|i| if i / 32 == 16 && (4..28).contains(&(i % 32)) { [0, 0, 0, 255] } else { [255; 4] }).collect();
    let p = pc_trace::Params { adaptive: false, speckle: 0, ..pc_trace::Params::for_preset(pc_trace::Preset::LineArt) };
    let out = pc_trace::centerline::trace(pc_trace::Image { width: 32, height: 32, rgba: &data }, &p).unwrap();
    assert_eq!(out.strokes.len(), 1);
    assert!(out.strokes[0].profile.is_none());
    (s, id, out)
}
#[test]
fn centerline_prepared_preview_release_native_roundtrip_undo_are_identical() {
    let (mut s, id, out) = setup();
    let c = capture(&s, id, [8., 12.], 0.5).unwrap();
    let before = s.active().unwrap().doc.clone();
    let rev = s.active().unwrap().revision;
    let prepared = prepare(&s, &c, &out, &NoProjection, || false).unwrap();
    let shown = preview(&s, &prepared).unwrap();
    assert!(std::sync::Arc::ptr_eq(&s.active().unwrap().doc, &before));
    assert_eq!(s.active().unwrap().revision, rev);
    assert!(s.journal.is_empty());
    let LayerContent::Group(g) = &prepared.group().content else { panic!("group") };
    let LayerContent::Shape(shape) = &g.children[0].content else { panic!("shape") };
    assert!(shape.fill.is_none());
    assert_eq!(shape.stroke.as_ref().unwrap().width, 2.);
    assert_eq!(shape.path.subpaths[0].knots[0].anchor.x, 17.);
    assert_eq!(shape.path.subpaths[0].knots[0].anchor.y, 45.);
    assert!(shape.cache.as_ref().unwrap().content_bounds().width() < 96);
    let group = publish(&mut s, prepared).unwrap();
    assert_eq!(s.active().unwrap().revision, rev + 1);
    assert_eq!(s.active().unwrap().history.past_len(), 1);
    assert_eq!(*s.active().unwrap().doc, shown);
    assert_eq!(s.active().unwrap().active_layer, Some(group));
    let bytes = photocraft_format::save_to_bytes(&shown, &Default::default()).unwrap();
    assert_eq!(photocraft_format::load_from_bytes(&bytes).unwrap(), shown);
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
    assert!(s.redo());
    assert_eq!(*s.active().unwrap().doc, shown);
}
#[test]
fn centerline_stale_selection_revision_targets_and_locks_never_publish() {
    let (mut s, id, out) = setup();
    let c = capture(&s, id, [0.; 2], 1.).unwrap();
    let prepared = prepare(&s, &c, &out, &NoProjection, || false).unwrap();
    let before = s.active().unwrap().doc.clone();
    s.active_mut().unwrap().selected_layers.clear();
    assert!(publish(&mut s, prepared.clone()).is_err());
    s.active_mut().unwrap().selected_layers = vec![id];
    let mut invalid = c.clone();
    invalid.document += 1;
    assert!(check(&s, &invalid).is_err());
    invalid = c.clone();
    invalid.revision += 1;
    assert!(check(&s, &invalid).is_err());
    invalid = c.clone();
    invalid.targets.push(id);
    assert!(check(&s, &invalid).is_err());
    invalid = c.clone();
    invalid.source = LayerId(u64::MAX);
    assert!(check(&s, &invalid).is_err());
    let mut locked = (*before).clone();
    locked.layer_mut(id).unwrap().locks.pixels = true;
    s.active_mut().unwrap().doc = std::sync::Arc::new(locked);
    assert!(publish(&mut s, prepared).is_err());
    assert_eq!(s.active().unwrap().history.past_len(), 0);
    assert!(s.journal.is_empty());
}
#[test]
fn centerline_invalid_output_and_cancel_fail_atomically_before_caches() {
    let (mut s, id, out) = setup();
    let c = capture(&s, id, [0.; 2], 1.).unwrap();
    let before = s.active().unwrap().doc.clone();
    for bad in [f64::NAN, f64::INFINITY, -1.] {
        let mut invalid = c.clone();
        invalid.scale = bad;
        assert!(check(&s, &invalid).is_err());
    }
    for edit in 0..5 {
        let mut invalid = out.clone();
        match edit {
            0 => invalid.strokes[0].path.subpaths[0].closed = true,
            1 => invalid.strokes[0].path.subpaths[0].knots[0].anchor.x = f64::NAN,
            2 => invalid.strokes[0].stroke.width = f32::INFINITY,
            3 => invalid.nodes += 1,
            _ => invalid.strokes[0].path.inverted = true,
        }
        assert!(prepare(&s, &c, &invalid, &NoProjection, || false).is_err());
    }
    assert!(prepare(&s, &c, &out, &NoProjection, || true).is_err());
    let empty = CenterlineTrace { strokes: Vec::new(), nodes: 0, ..out };
    let prepared = prepare(&s, &c, &empty, &NoProjection, || false).unwrap();
    assert!(publish(&mut s, prepared).is_err());
    assert_eq!(s.active().unwrap().doc, before);
    assert_eq!(s.active().unwrap().history.past_len(), 0);
}
#[test]
fn centerline_varying_width_requires_real_package11_projection() {
    let (s, id, mut out) = setup();
    let c = capture(&s, id, [0.; 2], 1.).unwrap();
    out.strokes[0].profile = Some(photocraft_doc::vector_design::appearance::WidthProfile {
        points: vec![
            photocraft_doc::vector_design::appearance::WidthPoint { t: 0., left: 0.5, right: 0.5 },
            photocraft_doc::vector_design::appearance::WidthPoint { t: 1., left: 2., right: 2. },
        ],
    });
    let err = prepare(&s, &c, &out, &NoProjection, || false).unwrap_err().to_string();
    assert!(err.contains("package 11"), "{err}");
    assert_eq!(s.active().unwrap().history.past_len(), 0);
}

#[test]
fn centerline_profile_admission_rejects_before_any_projection_or_cache() {
    struct Probe(std::cell::Cell<usize>);
    impl StrokeProjection for Probe {
        fn outline(&self, _: &photocraft_doc::Path, _: &StrokeItem) -> std::result::Result<photocraft_doc::Path, String> {
            self.0.set(self.0.get() + 1);
            Err("projection was reached".into())
        }
    }
    let (s, id, mut out) = setup();
    let c = capture(&s, id, [0.; 2], 1.).unwrap();
    let before = s.active().unwrap().doc.clone();
    let profile = |count: usize| photocraft_doc::vector_design::appearance::WidthProfile {
        points: (0..count).map(|i| photocraft_doc::vector_design::appearance::WidthPoint { t: i as f64 / (count - 1) as f64, left: 0.5, right: 0.5 }).collect(),
    };
    let probe = Probe(std::cell::Cell::new(0));
    out.strokes[0].profile = Some(profile(4097));
    assert!(prepare(&s, &c, &out, &probe, || false).unwrap_err().to_string().contains("profile budget"));
    out.strokes[0].profile = Some(profile(4096));
    out.strokes = vec![out.strokes[0].clone(); 257];
    out.nodes = out.strokes.iter().flat_map(|p| &p.path.subpaths).map(|p| p.knots.len()).sum();
    assert!(prepare(&s, &c, &out, &probe, || false).unwrap_err().to_string().contains("profile budget"));
    assert_eq!(probe.0.get(), 0);
    assert!(std::sync::Arc::ptr_eq(&s.active().unwrap().doc, &before));
    assert_eq!(s.active().unwrap().history.past_len(), 0);
}

#[test]
fn centerline_last_target_and_locked_ancestor_are_revalidated_on_release() {
    let (mut s, id, out) = setup();
    let mut doc = (*s.active().unwrap().doc).clone();
    let other = Layer::raster("Other", doc.pixel_format());
    let other_id = other.id;
    let group = Layer::group("Parent", vec![other]);
    let parent_id = group.id;
    doc.layers.push(group);
    s.active_mut().unwrap().doc = std::sync::Arc::new(doc);
    s.active_mut().unwrap().selected_layers = vec![id, other_id];
    let c = capture(&s, id, [0.; 2], 1.).unwrap();
    let prepared = prepare(&s, &c, &out, &NoProjection, || false).unwrap();
    let before = s.active().unwrap().doc.clone();
    let mut locked = (*before).clone();
    locked.layer_mut(parent_id).unwrap().locks.all = true;
    s.active_mut().unwrap().doc = std::sync::Arc::new(locked);
    assert!(publish(&mut s, prepared.clone()).is_err());
    assert_eq!(s.active().unwrap().history.past_len(), 0);
    let mut missing = (*before).clone();
    if let LayerContent::Group(g) = &mut missing.layer_mut(parent_id).unwrap().content {
        g.children.clear();
    }
    s.active_mut().unwrap().doc = std::sync::Arc::new(missing);
    assert!(publish(&mut s, prepared).is_err());
    assert_eq!(s.active().unwrap().history.past_len(), 0);
    assert!(s.journal.is_empty());
}
