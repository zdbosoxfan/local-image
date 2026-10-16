use super::*;
#[path = "../../../pc-export-common/cases/blends.rs"]
mod cases;
use cases::{fixture, shape};
use photocraft_doc::BlendMode;

fn assert_native(doc: &Document, raster: bool, label: &str) {
    let out = export::export(doc, &ExportScope::Document, &Default::default()).unwrap();
    assert!(!out.warnings.iter().any(|w| w.feature == "compositing"), "{label}: {:?}", out.warnings);
    let pdf = Pdf::new(out.bytes).unwrap();
    assert!(has_paths(&pdf));
    assert_eq!(images(&pdf) > 0, raster);
    let objects = format!("{:?}", pdf.objects().into_iter().collect::<Vec<_>>());
    assert!(objects.contains("FontFile2"), "editable text font remains: {label}");
    assert!(
        pdf.objects().into_iter().any(|o| matches!(o, Object::Dict(d) if d.get::<Name<'_>>(b"BM").is_some_and(|n| n.as_ref()!=b"Normal"))),
        "native blend resource: {label}"
    );
    let paths = streams(&pdf)
        .iter()
        .map(|(bytes, _)| {
            use hayro_syntax::content::{TypedIter, ops::TypedInstruction};
            let mut iter = TypedIter::new(bytes);
            let mut count = 0;
            while let Some(op) = iter.next() {
                if matches!(op, TypedInstruction::MoveTo(_) | TypedInstruction::RectPath(_)) {
                    count += 1;
                }
            }
            count
        })
        .sum::<usize>();
    assert!(paths >= 21, "many original vector paths remain: {label}");
    oracle(doc, label);
}
fn assert_fallback(doc: &Document, label: &str) {
    let out = export::export(doc, &ExportScope::Document, &Default::default()).unwrap();
    assert!(out.warnings.iter().any(|w| w.feature == "compositing"), "{label}");
    let pdf = Pdf::new(out.bytes).unwrap();
    assert!(images(&pdf) > 0);
    assert!(!has_paths(&pdf));
    oracle(doc, label);
}

#[test]
fn pdf_export_independent_group_flattens_whole_document_and_rejects_backdrop_less_selection() {
    let mut doc = fixture(BlendMode::Normal);
    let child = doc.layers.pop().unwrap();
    let mut group = Layer::group("PDF group", vec![child]);
    group.blend = BlendMode::Multiply;
    if let LayerContent::Group(g) = &mut group.content {
        g.isolation = photocraft_doc::GroupIsolation::NonIsolated;
    }
    let id = group.id;
    doc.layers.push(group);
    let before = doc.clone();
    let exported = export::export(&doc, &ExportScope::Document, &ExportOptions::default()).unwrap();
    assert!(exported.warnings.iter().any(|w| w.feature == "compositing"));
    assert!(images(&Pdf::new(exported.bytes).unwrap()) > 0);
    oracle(&doc, "independent-group-whole-document");
    assert!(export::export(&doc, &ExportScope::Selection(vec![id]), &ExportOptions::default()).is_err());
    assert_eq!(doc, before, "lossy export must preserve native source");
}

#[test]
fn pdf_export_explicit_isolated_passthrough_is_still_native_vector() {
    let mut doc = scene(photocraft_vector::shapes::rect(16., 20., 120., 88.), Fill::Solid(Color::rgb(0.7, 0.3, 0.5)));
    let children = std::mem::take(&mut doc.layers);
    let mut group = Layer::group("isolated", children);
    if let LayerContent::Group(g) = &mut group.content {
        g.isolation = photocraft_doc::GroupIsolation::Isolated;
    }
    doc.layers.push(group);
    let bytes = output(&doc);
    assert_eq!(images(&Pdf::new(bytes).unwrap()), 0);
    oracle(&doc, "explicit-isolated-native-vector");
}

#[test]
fn normal_nonisolated_legacy_subset_keeps_native_vectors_and_safe_partial_scope() {
    let mut doc = scene(photocraft_vector::shapes::rect(0., 0., 160., 128.), Fill::Solid(Color::rgb(0.2, 0.4, 0.6)));
    let mut child = shape(&doc, "multiply", (16., 16., 48., 48.), Color::rgb(0.8, 0.6, 0.4));
    child.blend = BlendMode::Multiply;
    let mut g = Layer::group("normal nonisolated", vec![child]);
    g.blend = BlendMode::Normal;
    if let LayerContent::Group(group) = &mut g.content {
        group.isolation = photocraft_doc::GroupIsolation::NonIsolated;
    }
    let id = g.id;
    doc.layers.push(g);
    let original = doc.clone();
    assert_eq!(images(&Pdf::new(output(&doc)).unwrap()), 0);
    oracle(&doc, "legacy-normal-nonisolated-vector");
    assert!(export::export(&doc, &ExportScope::Selection(vec![id]), &ExportOptions::default()).is_ok());
    doc.layers[1].opacity = 0.6;
    assert!(export::export(&doc, &ExportScope::Selection(vec![id]), &ExportOptions::default()).is_err(), "partial raster fallback must not discard B");
    doc.layers[1].opacity = 1.0;
    assert_eq!(doc, original);
}
