//! Regressions from the f73bc0ca test build.
#![allow(clippy::unwrap_used)]
use crate::{Session, file_cmds, vector_integration};
use photocraft_doc::vector_design::{
    drawing::{DesignShape, ShapeParameters},
    live::{BlendSpacing, BlendSpec, EnvelopeSpec, LiveGroup, RepeatSpec},
};
use photocraft_doc::{Color, Fill, Layer, LayerContent, LayerId, Path, ShapeLayer, Subpath};
use serde_json::json;

fn session() -> Session {
    let mut s = Session::new();
    s.execute("file.new", json!({"width": 160, "height": 120, "background": "transparent"})).unwrap();
    s
}
fn design(s: &mut Session, parameters: ShapeParameters) -> LayerId {
    let d = DesignShape { rect: [10., 10., 30., 20.], angle: 0., parameters };
    let st = s.active().unwrap();
    let p = json!({"document": st.doc.id.0, "revision": st.revision, "design": d});
    LayerId(s.execute("shape.design.create", p).unwrap()["layer"].as_u64().unwrap())
}
fn shape(layer: &Layer) -> &ShapeLayer {
    let LayerContent::Shape(sh) = &layer.content else { panic!("shape expected") };
    sh
}
fn assert_projection_matches(s: &Session, id: LayerId) {
    let doc = &s.active().unwrap().doc;
    let shown = vector_integration::project_document(doc).unwrap();
    assert_eq!(shape(doc.layer(id).unwrap()).path.control_bounds(), shape(shown.layer(id).unwrap()).path.control_bounds());
    let stored = shape(doc.layer(id).unwrap()).cache.as_ref().unwrap();
    let displayed = shape(shown.layer(id).unwrap()).cache.as_ref().unwrap();
    assert_eq!(stored.content_bounds(), displayed.content_bounds());
    let b = stored.content_bounds();
    assert_eq!(stored.read_region(b), displayed.read_region(b));
}
#[test]
fn hotfix_live_shapes_follow_move_transform_and_canvas_geometry() {
    for p in [
        ShapeParameters::Rect { radii: [0.; 4], kinds: Default::default() },
        ShapeParameters::Ellipse,
        ShapeParameters::Polygon { sides: 5, curved: 0., corner: 0. },
        ShapeParameters::Line { from: [10., 10.], to: [40., 30.] },
    ] {
        for (cmd, params) in [
            ("layer.translate", json!({"dx": 30, "dy": 20})),
            ("shape.edit", json!({"move": [30., 20.]})),
            ("path.transform", json!({"name": "layer", "translateX": 30., "translateY": 20.})),
            ("edit.transform", json!({"matrix": [2., 0., 0., 2., 5., 7.]})),
            ("edit.transform", json!({"matrix": [1.5, 0., 0., 2., 5., 7.]})),
            ("image.imageSize", json!({"width": 320, "height": 240})),
            ("image.canvasSize", json!({"width": 200, "height": 160, "anchor": "bottomRight", "extensionColor": "transparent"})),
        ] {
            let mut s = session();
            let id = design(&mut s, p.clone());
            let before = s.active().unwrap().doc.clone();
            if cmd == "layer.translate" {
                let preview = crate::layer_multi_cmds::moved(&before, &[id], 30, 20).unwrap();
                let shown = vector_integration::project_document(&preview).unwrap();
                let (x0, y0, x1, y1) = shape(before.layer(id).unwrap()).path.control_bounds().unwrap();
                assert_eq!(shape(shown.layer(id).unwrap()).path.control_bounds(), Some((x0 + 30., y0 + 20., x1 + 30., y1 + 20.)));
                assert_eq!(s.active().unwrap().doc, before, "held move is detached");
            }
            s.execute(cmd, params).unwrap();
            assert_ne!(shape(before.layer(id).unwrap()).path, shape(s.active().unwrap().doc.layer(id).unwrap()).path, "{cmd}");
            assert_projection_matches(&s, id);
            assert!(s.undo());
            assert_eq!(s.active().unwrap().doc, before);
        }
    }
}
#[test]
fn hotfix_boolean_results_do_not_regenerate_the_source_live_shape() {
    let mut s = session();
    let a = design(&mut s, ShapeParameters::Rect { radii: [0.; 4], kinds: Default::default() });
    let b = design(&mut s, ShapeParameters::Ellipse);
    s.execute("layer.translate", json!({"layer": b.0, "dx": 20, "dy": 0})).unwrap();
    s.active_mut().unwrap().selected_layers = vec![a, b];
    s.execute("path.boolean", json!({"op": "add"})).unwrap();
    assert!(s.active().unwrap().doc.layer(a).unwrap().vector.live_shape.is_none());
    assert_projection_matches(&s, a);
}
fn object(x: f64) -> Layer {
    Layer::new(
        "Source",
        LayerContent::Shape(ShapeLayer {
            path: Path::new(vec![Subpath::polygon(&[(x, 10.), (x + 10., 10.), (x + 10., 20.), (x, 20.)])]),
            fill: Some(Fill::Solid(Color::BLACK)),
            ..Default::default()
        }),
    )
}
#[test]
fn hotfix_live_and_clipped_vectors_save_as_psd_psb_and_layered_tiff() {
    let fixtures = [
        Some(LiveGroup::Repeat(RepeatSpec::Grid { rows: 1, cols: 3, spacing: [15., 0.] })),
        Some(LiveGroup::Blend(BlendSpec { spacing: BlendSpacing::Steps(2), ..Default::default() })),
        Some(LiveGroup::Envelope(EnvelopeSpec::Warp { style: "arc".into(), bend: 0.2, horizontal: 0., vertical: 0. })),
        None,
    ];
    for live in fixtures {
        let mut s = session();
        let mut group = Layer::group("Advanced", vec![object(10.), object(55.)]);
        group.vector.live = live;
        let feature = if group.vector.live.is_none() {
            group.vector.clip = Some(Path::new(vec![Subpath::polygon(&[(15., 0.), (70., 0.), (70., 30.), (15., 30.)])]));
            "vector crop controls"
        } else if matches!(group.vector.live, Some(LiveGroup::Repeat(_))) {
            "live repeats"
        } else if matches!(group.vector.live, Some(LiveGroup::Blend(_))) {
            "live blends"
        } else {
            "live envelopes"
        };
        group.opacity = 0.6;
        group.blend = photocraft_doc::BlendMode::Normal;
        let id = group.id;
        s.edit("Fixture", |doc, active| {
            doc.layers.push(group);
            *active = Some(id);
            Ok(())
        })
        .unwrap();
        let before = s.active().unwrap().doc.clone();
        let projected = vector_integration::project_document(&before).unwrap();
        let expected = photocraft_compose::flatten(&projected);
        for ext in ["psd", "psb", "tif"] {
            let (bytes, warnings) = file_cmds::encode(&before, ext, file_cmds::SaveOpts { tiff_layers: true, ..Default::default() }).unwrap();
            assert!(warnings.iter().any(|w| w.contains("editability lost") && w.contains(feature)), "{ext}: {warnings:?}");
            let reopened = photocraft_io::import(ext, &bytes).unwrap().document;
            let actual = photocraft_compose::flatten(&reopened);
            for y in 0..120 {
                for x in 0..160 {
                    for (a, b) in actual.get(x, y).into_iter().zip(expected.get(x, y)) {
                        assert!((a - b).abs() < 0.015, "{ext} at {x},{y}: {a} != {b}");
                    }
                }
            }
            assert!(reopened.walk().len() >= 2, "other layers remain editable");
        }
        assert_eq!(s.active().unwrap().doc, before, "save leaves native metadata untouched");
    }
}
#[test]
fn hotfix_repeat_anchor_and_clip_follow_layer_and_canvas_transforms() {
    let mut s = session();
    let mut group = Layer::group("Radial", vec![object(10.)]);
    group.vector.live = Some(LiveGroup::Repeat(RepeatSpec::Radial { count: 3, center: [30., 30.], angle: 0. }));
    group.vector.clip = Some(Path::new(vec![Subpath::polygon(&[(0., 0.), (80., 0.), (80., 80.), (0., 80.)])]));
    let id = group.id;
    s.edit("Fixture", |doc, active| {
        doc.layers.push(group);
        *active = Some(id);
        Ok(())
    })
    .unwrap();
    for (cmd, p, center, clip) in [
        ("layer.translate", json!({"dx": 10, "dy": 20}), [40., 50.], (10., 20., 90., 100.)),
        ("edit.transform", json!({"matrix": [2., 0., 0., 2., 0., 0.]}), [80., 100.], (20., 40., 180., 200.)),
        ("image.imageSize", json!({"width": 320, "height": 240}), [160., 200.], (40., 80., 360., 400.)),
    ] {
        s.execute(cmd, p).unwrap();
        let l = s.active().unwrap().doc.layer(id).unwrap();
        let Some(LiveGroup::Repeat(RepeatSpec::Radial { center: actual, .. })) = l.vector.live else { panic!("radial repeat") };
        assert_eq!(actual, center, "{cmd}");
        assert_eq!(l.vector.clip.as_ref().unwrap().control_bounds(), Some(clip), "{cmd}");
    }
}

#[test]
fn hotfix_placed_svg_saves_with_the_original_editability_notice() {
    let mut s = session();
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="30"><rect x="5" y="5" width="30" height="20" fill="#ff0000"/></svg>"##;
    let request = crate::vector_packages::svg_import::capture(&s, svg.into()).unwrap();
    s.execute("file.svg.place", json!(request)).unwrap();
    let before = s.active().unwrap().doc.clone();
    let expected = photocraft_compose::flatten(&vector_integration::project_document(&before).unwrap());
    for name in ["photo-Edit.psd", "photo-Edit.tif"] {
        let (bytes, warnings) = file_cmds::encode(&before, name, file_cmds::SaveOpts { tiff_layers: true, ..Default::default() }).unwrap();
        assert_eq!(warnings.iter().filter(|w| w.contains("editability lost")).count(), 1);
        assert!(warnings.iter().any(|w| w.contains("vector crop controls")));
        let reopened = photocraft_io::import(name, &bytes).unwrap().document;
        let actual = photocraft_compose::flatten(&reopened);
        for (a, b) in actual.px.iter().flatten().zip(expected.px.iter().flatten()) {
            assert!((a - b).abs() < 0.015);
        }
    }
    assert_eq!(s.active().unwrap().doc, before);
}
