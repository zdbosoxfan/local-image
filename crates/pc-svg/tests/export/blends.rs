use super::*;
#[path = "../../../pc-export-common/cases/blends.rs"]
mod cases;
use cases::{fixture, shape};
use photocraft_doc::BlendMode;

fn result(doc: &Document) -> photocraft_doc::vector_design::exchange::ExportedVector {
    export::export(doc, &ExportScope::Document, &ExportOptions { text_as_curves: true, ..Default::default() }).unwrap()
}
fn assert_native(doc: &Document, raster: bool, label: &str) {
    let out = result(doc);
    assert!(!out.warnings.iter().any(|w| w.feature == "compositing"), "{label}: {:?}", out.warnings);
    let source = String::from_utf8(out.bytes).unwrap();
    let parsed = xml(&source);
    assert!(parsed.descendants().filter(|n| n.has_tag_name("path")).count() >= 22, "vectors and outlined type remain: {label}");
    assert_eq!(parsed.descendants().filter(|n| n.has_tag_name("image")).count(), usize::from(raster));
    assert!(source.contains("mix-blend-mode:"));
    assert_raster_oracle_svg(doc, &source, label);
}
fn assert_fallback(doc: &Document, label: &str) {
    let out = result(doc);
    assert!(out.warnings.iter().any(|w| w.feature == "compositing"), "{label}");
    let source = String::from_utf8(out.bytes).unwrap();
    let parsed = xml(&source);
    assert_eq!(parsed.descendants().filter(|n| n.has_tag_name("image")).count(), 1);
    assert!(!parsed.descendants().any(|n| n.has_tag_name("path")));
    assert_raster_oracle_svg(doc, &source, label);
}

#[test]
fn explicit_group_isolation_keeps_vector_isolated_output_and_backdrop_safe_fallback() {
    let mut doc = scene(photocraft_vector::shapes::rect(20., 20., 80., 80.), Fill::Solid(Color::rgb(0.7, 0.3, 0.5)));
    let child = doc.layers.pop().unwrap();
    let mut group = Layer::group("isolated", vec![child]);
    if let LayerContent::Group(g) = &mut group.content {
        g.isolation = photocraft_doc::GroupIsolation::Isolated;
    }
    let id = group.id;
    doc.layers.push(group);
    let text = svg(&doc);
    assert!(text.contains("isolation:isolate"));
    assert!(!text.contains("<image"));
    assert_oracle_svg(&doc, &text, "explicit-isolated-vector");
    let LayerContent::Group(g) = &mut doc.layers[0].content else { panic!("group") };
    g.isolation = photocraft_doc::GroupIsolation::NonIsolated;
    doc.layers[0].blend = BlendMode::Multiply;
    let before = doc.clone();
    let text = svg(&doc);
    assert!(text.contains("<image"));
    assert_raster_oracle_svg(&doc, &text, "explicit-nonisolated-document");
    assert!(export::export(&doc, &ExportScope::Selection(vec![id]), &ExportOptions::default()).is_err());
    assert_eq!(doc, before);
}

#[test]
fn normal_nonisolated_legacy_subset_keeps_native_vectors_and_safe_partial_scope() {
    let mut doc = scene(photocraft_vector::shapes::rect(0., 0., 256., 256.), Fill::Solid(Color::rgb(0.2, 0.4, 0.6)));
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
    let text = svg(&doc);
    assert!(!text.contains("<image"));
    assert_oracle_svg(&doc, &text, "legacy-normal-nonisolated-vector");
    assert!(export::export(&doc, &ExportScope::Selection(vec![id]), &ExportOptions::default()).is_ok());
    doc.layers[1].opacity = 0.6;
    assert!(export::export(&doc, &ExportScope::Selection(vec![id]), &ExportOptions::default()).is_err(), "partial raster fallback must not discard B");
    doc.layers[1].opacity = 1.0;
    assert_eq!(doc, original);
}

#[test]
fn pass_through_masked_group_of_normal_children_keeps_native_clip_path() {
    // A pass-through group (e.g. Make Clipping Mask) with opacity and a vector mask whose
    // children all paint Normal equals an isolated group, so it stays a native <clipPath>.
    let mut doc = fixture(BlendMode::Normal);
    let top = doc.layers.pop().unwrap();
    let second = shape(&doc, "Second", (90., 70., 145., 120.), Color::rgb(0.1, 0.72, 0.31));
    let mut group = Layer::group("Clipping Group", vec![top, second]);
    assert_eq!(group.blend, BlendMode::PassThrough);
    group.vector_mask = Some(photocraft_doc::VectorMask::new(photocraft_vector::shapes::rect(40., 40., 150., 110.)));
    group.opacity = 0.8;
    doc.layers.push(group);
    let out = result(&doc);
    assert!(!out.warnings.iter().any(|w| w.feature == "compositing"), "{:?}", out.warnings);
    let source = String::from_utf8(out.bytes).unwrap();
    assert!(source.contains("<clipPath") && source.contains("clip-path="), "native clip");
    assert!(!source.contains("<image"));
    assert_raster_oracle_svg(&doc, &source, "pass-through-clip-normal");
    // a Multiply child composites with the backdrop through the pass-through group: fallback
    let LayerContent::Group(g) = &mut doc.layers.last_mut().unwrap().content else { unreachable!() };
    g.children[1].blend = BlendMode::Multiply;
    assert_fallback(&doc, "pass-through-clip-multiply");
}
