//! Save/reopen contracts for merged features awaiting their native persistence adapters.
#![expect(clippy::allow_attributes_without_reason, reason = "baseline 2026-10-25: unreviewed")]

#[path = "../../pc-format/tests/contracts_support/mod.rs"]
mod contracts_support;
use contracts_support::*;
use photocraft_doc::vector_design::effects::*;
use photocraft_doc::vector_design::swatches::*;
use photocraft_doc::*;
use photocraft_engine::{
    Session, vector_api,
    vector_packages::{colours, mesh, path_effects, styles},
};
use photocraft_format::{SaveOptions, load_from_bytes, save_to_bytes};
use std::collections::BTreeMap;
fn roundtrip(d: &Document) -> Document {
    load_from_bytes(&save_to_bytes(d, &SaveOptions::default()).unwrap()).unwrap()
}
fn session() -> Session {
    let mut s = Session::new();
    let mut d = document();
    d.layers.push(shape());
    s.add_document(d, None);
    s
}
#[test]
fn gradient_mesh_native_save_reopen_keeps_control_points_handles_colors_and_opacity() {
    let s = session();
    let mut d = (*s.active().unwrap().doc).clone();
    let id = d.layers[0].id;
    let mut edit = vector_api::snapshot_shapes(&d, &[id]).unwrap().remove(0);
    let mut geometry =
        photocraft_vector::mesh_paint::Mesh::regular([2., 2., 18., 16.], 2, 2, photocraft_vector::mesh_paint::Appearance::Flat, 0., |_| [0.2, 0.4, 0.8, 1.])
            .unwrap();
    geometry.points[1].opacity = 0.625;
    geometry.points[1].handles[0] = [0.25, -0.5];
    let expected: mesh::MeshData = geometry.into();
    expected.validate().unwrap();
    mesh::write_native(&mut edit, &expected).unwrap();
    d.layers[0].vector = edit.vector;
    d.layers[0].content = LayerContent::Shape(edit.shape);
    assert_eq!(mesh::read_native(&d.layers[0]).unwrap(), Some(expected.clone()));
    let reopened = roundtrip(&d);
    assert_eq!(reopened, d);
    assert_eq!(mesh::read_native(&reopened.layers[0]).unwrap(), Some(expected));
}
#[test]
fn every_path_effect_native_save_reopen_keeps_order_targets_and_enabled_state() {
    let s = session();
    let mut d = (*s.active().unwrap().doc).clone();
    let access = path_effects::access().unwrap();
    let effects: Vec<_> = EffectKind::presets()
        .into_iter()
        .enumerate()
        .map(|(i, effect)| PathEffect { effect, target: if i % 2 == 0 { EffectTarget::Layer } else { EffectTarget::Item { index: 0 } }, enabled: i != 2 })
        .collect();
    (access.write)(&mut d.layers[0], effects.clone()).unwrap();
    assert_eq!((access.read)(&d.layers[0]).unwrap(), effects);
    let reopened = roundtrip(&d);
    assert_eq!(reopened, d);
    assert_eq!((access.read)(&reopened.layers[0]).unwrap(), effects);
}
#[test]
fn graphic_styles_and_applied_links_native_save_reopen() {
    let s = session();
    let mut d = (*s.active().unwrap().doc).clone();
    let hooks = styles::installed_hooks().expect("native style persistence hooks must be installed");
    let sample = styles::sample(&d, d.layers[0].id, None).unwrap();
    let expected = styles::Store {
        graphic_styles: vec![styles::GraphicStyle { id: styles::StyleId(1), name: "Contract style".into(), attributes: sample.clone() }],
        links: BTreeMap::from([(
            d.layers[0].id,
            styles::StyleLink { style: styles::StyleId(1), applied: sample, character_runs: vec![], paragraph_runs: vec![] },
        )]),
    };
    (hooks.write)(&mut d, &expected).unwrap();
    assert_eq!((hooks.read)(&d).unwrap(), expected);
    let reopened = roundtrip(&d);
    assert_eq!(reopened, d);
    assert_eq!((hooks.read)(&reopened).unwrap(), expected);
}
#[test]
fn swatches_groups_spot_tints_and_paint_links_native_save_reopen() {
    let s = session();
    let mut d = (*s.active().unwrap().doc).clone();
    let access = colours::access().unwrap();
    let expected = SwatchBook {
        groups: vec![SwatchGroup { id: 1, name: "Brand".into() }],
        swatches: vec![Swatch {
            id: SwatchId(2),
            name: "Brand ink".into(),
            paint: Fill::Solid(Color::BLACK),
            global: true,
            spot: true,
            group: Some(1),
            stops: vec![],
        }],
        links: vec![PaintLink {
            target: PaintTarget { symbol: None, layer: d.layers[0].id, paint: PaintSlot::Fill, stop: None },
            reference: SwatchRef { swatch: SwatchId(2), tint: 50. },
        }],
    };
    expected.validate().unwrap();
    (access.write)(&mut d, expected.clone()).unwrap();
    assert_eq!((access.read)(&d).unwrap(), expected);
    let reopened = roundtrip(&d);
    assert_eq!(reopened, d);
    assert_eq!((access.read)(&reopened).unwrap(), expected);
}
