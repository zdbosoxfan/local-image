#![cfg(feature = "synth")]
#![expect(clippy::allow_attributes_without_reason, reason = "baseline 2026-10-25: unreviewed")]
use photocraft_affinity::{
    Error, import,
    synth::{self, F, Method, tag},
};
use photocraft_doc::{FillRule, Layer, LayerContent, PathOp, vector_design::exchange::ImportOptions};
#[path = "support/text_fixture.rs"]
mod text_fixture;
fn document(children: Vec<F>) -> Vec<u8> {
    document_with_transparency(children, true)
}
fn document_with_transparency(children: Vec<F>, transparent: bool) -> Vec<u8> {
    let mut root = vec![(
        tag(b"DocR"),
        F::Obj(
            tag(b"DocN"),
            vec![(
                tag(b"Chld"),
                F::Shared(vec![F::Obj(
                    tag(b"Sprd"),
                    vec![(tag(b"SprB"), F::F64s(vec![0., 0., 64., 64.])), (tag(b"SprT"), F::Bool(transparent)), (tag(b"Chld"), F::Shared(children))],
                )]),
            )],
        ),
    )];
    let mut next = 1000;
    for (_, v) in &mut root {
        text_fixture::shared_objects(v, &mut next);
    }
    let doc = synth::stream(&root);
    synth::container(&[("doc.dat", &doc, Method::Zstd)], None)
}
fn walk(layers: &[Layer]) -> Vec<&Layer> {
    let mut out = vec![];
    for l in layers {
        out.push(l);
        if let LayerContent::Group(g) = &l.content {
            out.extend(walk(&g.children));
        }
    }
    out
}
fn curve() -> F {
    let records = |points: &[(f64, f64)]| {
        F::Records(
            18,
            points
                .iter()
                .map(|&(x, y)| {
                    let mut b = x.to_le_bytes().to_vec();
                    b.extend(y.to_le_bytes());
                    b.extend([0, 0]);
                    b
                })
                .collect(),
        )
    };
    F::Def(
        1,
        vec![tag(b"PCrv")],
        vec![
            (tag(b"Desc"), F::Str("Hollow square".into())),
            (tag(b"BFil"), text_fixture::solid(1.)),
            (
                tag(b"Crvs"),
                F::Obj(
                    tag(b"PCvD"),
                    vec![(
                        tag(b"Data"),
                        F::Pos(vec![
                            F::U8(0),
                            F::U32(2),
                            F::Bool(true),
                            records(&[(4., 4.), (44., 4.), (44., 44.), (4., 44.), (4., 4.)]),
                            F::Bool(true),
                            records(&[(14., 14.), (34., 14.), (34., 34.), (14., 34.), (14., 14.)]),
                        ]),
                    )],
                ),
            ),
        ],
    )
}
#[test]
fn affinity_import_cubic_hole_groups_and_unknown_records() {
    let bytes = document(vec![F::Obj(
        tag(b"Grup"),
        vec![
            (tag(b"Desc"), F::Str("Group".into())),
            (tag(b"Chld"), F::Shared(vec![curve(), F::Obj(tag(b"ZZZZ"), vec![(tag(b"Desc"), F::Str("Future".into()))])])),
        ],
    )]);
    let scene = import(&bytes, &ImportOptions::default()).unwrap();
    let layers = walk(&scene.layers);
    let shape = layers.iter().find_map(|l| if let LayerContent::Shape(s) = &l.content { Some(s) } else { None }).unwrap();
    assert_eq!(shape.path.fill_rule, FillRule::EvenOdd);
    assert_eq!(shape.path.subpaths[1].op, PathOp::Join);
    assert_eq!(shape.path.subpaths[0].knots.len(), 4);
    let s = shape.cache.as_ref().unwrap();
    assert_eq!(s.rgba(20, 20)[3], 0.);
    assert_eq!(s.rgba(10, 10)[3], 1.);
    let area: f32 = (0..64).flat_map(|y| (0..64).map(move |x| s.rgba(x, y)[3])).sum();
    assert!((area - 1200.).abs() < 0.01);
    assert!(layers.iter().any(|l| l.name == "Group" && l.is_group()));
    assert!(scene.warnings.iter().any(|w| w.object.as_deref() == Some("Future")));
}
#[test]
fn affinity_import_text_preserves_utf8_runs_caps_and_fonts() {
    let bytes = text_fixture::document(
        vec![text_fixture::block(
            text_fixture::utf8("Aé😀"),
            vec![
                text_fixture::run(1, Some(text_fixture::attrs("Missing Family", "MissingPS", 700, true, 16.))),
                text_fixture::run(3, Some(text_fixture::attrs("Other Family", "OtherPS", 400, false, 20.))),
            ],
            &[1],
        )],
        false,
        96.,
        text_fixture::IDENTITY,
    );
    let scene = import(&bytes, &ImportOptions { dpi: 96., ..Default::default() }).unwrap();
    let layers = walk(&scene.layers);
    let text = layers.iter().find_map(|l| if let LayerContent::Text(t) = &l.content { Some(t) } else { None }).unwrap();
    assert_eq!(text.text, "Aé😀");
    assert_eq!(text.runs.iter().map(|r| r.len).collect::<Vec<_>>(), [1, 6]);
    assert_eq!(text.runs[0].style.weight, 700);
    assert_eq!(text.runs[0].style.postscript_name.as_deref(), Some("MissingPS"));
    assert_eq!(text.runs[0].style.size_pt, 12.);
    assert!(text.cache.is_some());
    assert!(scene.warnings.iter().any(|w| w.feature == "font-substitution"));
}
#[test]
fn affinity_import_cyclic_compounds_are_rejected_before_expansion() {
    let cycle = F::Def(5, vec![tag(b"Comp")], vec![(tag(b"Chld"), F::Shared(vec![F::Ref(5)]))]);
    assert!(matches!(import(&document(vec![cycle]), &ImportOptions::default()), Err(Error::Limit(_))));
}
#[test]
fn affinity_import_paper_is_not_an_unsupported_artwork_recovery() {
    let bytes = document_with_transparency(vec![F::Obj(tag(b"ZZZZ"), vec![])], false);
    assert!(matches!(import(&bytes, &ImportOptions::default()), Err(Error::Unsupported("no supported artwork"))));
}
#[test]
fn affinity_import_source_numbers_are_finite() {
    for value in [f64::NAN, f64::INFINITY] {
        let bytes = text_fixture::document(vec![text_fixture::block(text_fixture::utf8("A"), vec![], &[])], false, value, text_fixture::IDENTITY);
        assert!(matches!(import(&bytes, &ImportOptions::default()), Err(Error::Malformed(_))));
        let mut shape = curve();
        if let F::Def(_, _, fields) = &mut shape {
            fields.push((tag(b"Opac"), F::F64(value)));
        }
        assert!(matches!(import(&document(vec![shape]), &ImportOptions::default()), Err(Error::Malformed(_))));
    }
}
#[test]
fn affinity_import_spot_colour_cycles_terminate_and_warn() {
    let colour = F::Def(8, vec![tag(b"Pant")], vec![(tag(b"srgb"), F::Ref(8))]);
    let mut shape = curve();
    if let F::Def(_, _, fields) = &mut shape {
        fields.retain(|(t, _)| *t != tag(b"BFil"));
        fields.push((tag(b"BFil"), F::Obj(tag(b"FilS"), vec![(tag(b"Colr"), colour)])));
    }
    let s = import(&document(vec![shape]), &ImportOptions::default()).unwrap();
    assert!(s.warnings.iter().any(|w| w.message.contains("spot colours")));
}
#[test]
fn affinity_import_ellipse_area_and_gradient_stops_are_independent_oracles() {
    let colors = vec![
        vec![(tag(b"_col"), F::Struct([1.0f32, 0., 0., 1.].into_iter().flat_map(f32::to_le_bytes).collect()))],
        vec![(tag(b"_col"), F::Struct([0.0f32, 0., 1., 1.].into_iter().flat_map(f32::to_le_bytes).collect()))],
    ];
    let mut positions = vec![0xa4];
    positions.extend(tag(b"Posn").0.to_le_bytes());
    positions.extend(2u32.to_le_bytes());
    positions.extend([0.0f64, 0.5, 1., 0.5].into_iter().flat_map(f64::to_le_bytes));
    let fill = F::Obj(
        tag(b"FDsc"),
        vec![
            (tag(b"FDeX"), F::F64s(vec![40., 0., 4., 0., 40., 4.])),
            (
                tag(b"FDeF"),
                F::Obj(
                    tag(b"FilG"),
                    vec![
                        (tag(b"Type"), F::Enum(0, 0)),
                        (tag(b"Grad"), F::Obj(tag(b"Grad"), vec![(tag(b"Cols"), F::Objs(tag(b"RGBA"), colors)), (tag(b"Posn"), F::Raw(positions))])),
                    ],
                ),
            ),
        ],
    );
    let bytes = document(vec![F::Obj(
        tag(b"ShpN"),
        vec![(tag(b"ShpB"), F::F64s(vec![4., 4., 44., 44.])), (tag(b"Shpe"), F::Obj(tag(b"ShpE"), vec![])), (tag(b"BFil"), fill)],
    )]);
    let scene = import(&bytes, &ImportOptions::default()).unwrap();
    let layers = walk(&scene.layers);
    let sh = layers.iter().find_map(|l| if let LayerContent::Shape(s) = &l.content { Some(s) } else { None }).unwrap();
    let cache = sh.cache.as_ref().unwrap();
    let area: f32 = (0..64).flat_map(|y| (0..64).map(move |x| cache.rgba(x, y)[3])).sum();
    assert!((f64::from(area) - std::f64::consts::PI * 400.).abs() < 0.5);
    let left = cache.rgba(5, 24);
    let right = cache.rgba(43, 24);
    assert!(left[0] > 0.95 && left[2] < 0.05);
    assert!(right[2] > 0.95 && right[0] < 0.05);
    let middle = cache.rgba(24, 24);
    assert!((middle[0] - 0.5).abs() < 0.002);
    assert!((middle[2] - 0.5).abs() < 0.002);
}
#[test]
fn affinity_import_unknown_record_uses_available_raster_pixels() {
    let mut bitmap = vec![(tag(b"BmpW"), F::I32(2)), (tag(b"BmpH"), F::I32(2)), (tag(b"Frmt"), F::Enum(0, 0))];
    for channel in *b"1234" {
        bitmap.push((tag(&[b'S', b't', b'a', channel]), F::U8s(vec![2])));
    }
    let bytes = document(vec![F::Obj(tag(b"ZZZZ"), vec![(tag(b"Desc"), F::Str("Future pixels".into())), (tag(b"Bitm"), F::Obj(tag(b"DyBm"), bitmap))])]);
    let scene = import(&bytes, &ImportOptions::default()).unwrap();
    let layers = walk(&scene.layers);
    let surface = layers.iter().find_map(|l| if let LayerContent::Raster(s) = &l.content { Some(s) } else { None }).unwrap();
    assert_eq!(surface.rgba(0, 0), [1.; 4]);
    assert_eq!(surface.rgba(2, 0), [0.; 4]);
    assert!(scene.warnings.iter().any(|w| w.message.contains("cached pixels")));
}

#[test]
fn affinity_import_unsupported_compound_operand_never_publishes_partial_geometry() {
    let compound = F::Def(100, vec![tag(b"Comp")], vec![(tag(b"Chld"), F::Shared(vec![curve(), F::Obj(tag(b"ZZZZ"), vec![])]))]);
    assert!(matches!(import(&document(vec![compound]), &ImportOptions::default()), Err(Error::Unsupported("no supported artwork"))));
}

#[test]
fn dsfix_stack_channel_decode_preserves_rgba_grey_and_cmyk_pixels() {
    for (format, channels, expected) in [(0, 4, [1.; 4]), (1, 4, [1.; 4]), (6, 1, [1.; 4]), (7, 1, [1.; 4]), (4, 5, [0., 0., 0., 1.])] {
        let mut bitmap = vec![(tag(b"BmpW"), F::I32(2)), (tag(b"BmpH"), F::I32(2)), (tag(b"Frmt"), F::Enum(format, 0))];
        for channel in 0..channels {
            bitmap.push((tag(&[b'S', b't', b'a', b'1' + channel]), F::U8s(vec![2])));
        }
        let bytes = document(vec![F::Obj(tag(b"Rstr"), vec![(tag(b"Bitm"), F::Obj(tag(b"DyBm"), bitmap))])]);
        let scene = import(&bytes, &ImportOptions::default()).unwrap();
        let layers = walk(&scene.layers);
        let surface = layers.iter().find_map(|l| if let LayerContent::Raster(s) = &l.content { Some(s) } else { None }).unwrap();
        assert_eq!(surface.rgba(1, 1), expected, "format {format}");
        assert_eq!(surface.rgba(2, 1), [0.; 4]);
    }
}
