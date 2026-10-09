use photocraft_color::Color;
use photocraft_doc::TextLayer;
use photocraft_doc::text::{AntiAlias, CharStyle, Kerning, Orientation, ParagraphRun, ParagraphStyle, TextAlign, TextRun, TextShape};
use photocraft_geom::Affine;
use photocraft_psd::descriptor::{Descriptor, Id, UnicodeString, Value as D};
use photocraft_text::engine_data::Value as E;
use photocraft_text::psd::{TySh, apply_txt2, build_engine_data, build_tysh, engine_data, parse_txt2, parse_tysh, text_layer_from_tysh, write_tysh};

fn identity() -> Affine {
    Affine { m: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0] }
}

fn simple_layer(text: &str) -> TextLayer {
    let mut layer = TextLayer {
        text: text.to_string(),
        transform: identity(),
        orientation: Orientation::Horizontal,
        antialias: AntiAlias::Smooth,
        shape: TextShape::Point,
        runs: vec![TextRun {
            len: text.len(),
            style: CharStyle {
                font_family: "Inter".to_string(),
                weight: 400,
                italic: false,
                postscript_name: None,
                size_pt: 36.0,
                color: Color::rgba(0.1, 0.2, 0.3, 1.0),
                kerning: Kerning::Metrics,
                kern: 0.0,
                ..Default::default()
            },
        }],
        paragraphs: vec![ParagraphRun { len: text.len(), style: ParagraphStyle { align: TextAlign::Left, ..Default::default() } }],
        ..Default::default()
    };
    layer.sync_summary();
    layer
}

#[test]
fn parse_tysh_empty_returns_none() {
    assert!(parse_tysh(b"").is_none());
}

#[test]
fn parse_tysh_short_inputs_return_none() {
    for len in 0..=52 {
        let data = vec![0u8; len];
        assert!(parse_tysh(&data).is_none(), "len {len} should be too short");
    }
}

#[test]
fn parse_tysh_malformed_descriptor_returns_none() {
    let mut data = vec![0u8; 60];
    // Fill descriptor region with bytes that cannot be a valid VersionedDescriptor.
    for b in data.iter_mut().skip(52) {
        *b = 0xFF;
    }
    assert!(parse_tysh(&data).is_none());
}

#[test]
fn write_tysh_parse_roundtrip() {
    let text_desc = Descriptor::new("TxLr").with("Txt ", D::Text(UnicodeString::new_nul("Hello")));
    let tysh = TySh { transform: identity(), text: text_desc, warp: None, bounds: [0, 0, 10, 10] };
    let bytes = write_tysh(&tysh);
    let parsed = parse_tysh(&bytes).expect("parse should succeed");
    assert_eq!(parsed.transform.m, tysh.transform.m);
    assert_eq!(parsed.bounds, tysh.bounds);
    assert!(parsed.warp.is_some(), "write_tysh always adds a default warp descriptor");
    // The text descriptor should contain the same Txt value.
    let original_txt = match tysh.text.get("Txt ") {
        Some(D::Text(s)) => s.to_string_lossy().to_string(),
        _ => panic!("original missing Txt"),
    };
    let parsed_txt = match parsed.text.get("Txt ") {
        Some(D::Text(s)) => s.to_string_lossy().to_string(),
        _ => panic!("parsed missing Txt"),
    };
    assert_eq!(original_txt, parsed_txt);
}

#[test]
fn write_tysh_preserves_nan_transform() {
    let text_desc = Descriptor::new("TxLr");
    let tysh = TySh { transform: Affine { m: [f64::NAN, 0.0, 0.0, 1.0, 0.0, 0.0] }, text: text_desc, warp: None, bounds: [0; 4] };
    let bytes = write_tysh(&tysh);
    let parsed = parse_tysh(&bytes).expect("parse with NaN transform should succeed");
    assert!(parsed.transform.m[0].is_nan());
}

#[test]
fn engine_data_missing_or_invalid_raw_returns_none() {
    let desc = Descriptor::new("TxLr");
    assert!(engine_data(&desc).is_none());

    let desc_with_invalid = Descriptor::new("TxLr").with("EngineData", D::RawData(vec![0, 1, 2, 3]));
    assert!(engine_data(&desc_with_invalid).is_none());
}

#[test]
fn parse_txt2_malformed_returns_none() {
    // Wrapped as <<[0,1,2]>>, which should fail to parse as a dictionary.
    assert!(parse_txt2(&[0, 1, 2]).is_none());
}

#[test]
fn text_layer_from_tysh_without_engine_data() {
    let text_desc = Descriptor::new("TxLr").with("Txt ", D::Text(UnicodeString::new_nul("Hello")));
    let tysh = TySh { transform: identity(), text: text_desc, warp: None, bounds: [0, 0, 10, 10] };
    let bytes = write_tysh(&tysh);
    let layer = text_layer_from_tysh(&bytes, 72.0).expect("parse layer");
    assert_eq!(layer.text, "Hello");
    assert_eq!(layer.orientation, Orientation::Horizontal);
    assert_eq!(layer.shape, TextShape::Point);
    assert_eq!(layer.transform.m, identity().m);
}

#[test]
fn text_layer_roundtrip_basic() {
    let layer = simple_layer("Hello, World!");
    let bytes = build_tysh(&layer, 72.0, Some([0.0, 0.0, 100.0, 50.0]));
    let parsed = text_layer_from_tysh(&bytes, 72.0).expect("parse layer");
    assert_eq!(parsed.text, layer.text);
    assert_eq!(parsed.transform.m, layer.transform.m);
    assert_eq!(parsed.orientation, layer.orientation);
    assert_eq!(parsed.shape, layer.shape);
}

#[test]
fn text_layer_roundtrip_with_style() {
    let mut layer = simple_layer("styled");
    layer.runs[0].style.size_pt = 24.0;
    layer.runs[0].style.kerning = Kerning::Off;
    layer.runs[0].style.kern = 250.0;
    layer.runs[0].style.color = Color::rgba(0.9, 0.1, 0.5, 0.8);
    layer.paragraphs[0].style.align = TextAlign::Center;
    layer.paragraphs[0].style.first_line_indent_pt = 12.0;

    let bytes = build_tysh(&layer, 72.0, Some([0.0, 0.0, 100.0, 50.0]));
    let parsed = text_layer_from_tysh(&bytes, 72.0).expect("parse layer");

    assert_eq!(parsed.text, layer.text);

    let orig_run = &layer.runs[0].style;
    let parsed_run = &parsed.runs[0].style;
    assert!((parsed_run.size_pt - orig_run.size_pt).abs() < 0.001);
    assert_eq!(parsed_run.kerning, orig_run.kerning);
    assert!((parsed_run.kern - orig_run.kern).abs() < 0.5);
    let orig_c = orig_run.color.to_rgb();
    let parsed_c = parsed_run.color.to_rgb();
    assert!((orig_c[0] - parsed_c[0]).abs() < 0.01);
    assert!((orig_c[1] - parsed_c[1]).abs() < 0.01);
    assert!((orig_c[2] - parsed_c[2]).abs() < 0.01);
    // Check alpha directly
    assert!((orig_run.color.alpha - parsed_run.color.alpha).abs() < 0.01);

    let orig_para = &layer.paragraphs[0].style;
    let parsed_para = &parsed.paragraphs[0].style;
    assert_eq!(orig_para.align, parsed_para.align);
    assert!((orig_para.first_line_indent_pt - parsed_para.first_line_indent_pt).abs() < 0.01);
}

#[test]
fn build_engine_data_empty_layer_returns_dict() {
    let layer = TextLayer::default();
    let e = build_engine_data(&layer, None, 72.0);
    assert!(matches!(e, E::Dict(_)));
}

#[test]
fn build_tysh_point_and_box_shape() {
    // Point shape
    let mut point = simple_layer("point");
    point.shape = TextShape::Point;
    let bytes_point = build_tysh(&point, 72.0, Some([0.0, 0.0, 50.0, 20.0]));
    let parsed_point = text_layer_from_tysh(&bytes_point, 72.0).expect("parse point");
    assert_eq!(parsed_point.shape, TextShape::Point);

    // Box shape
    let mut box_layer = simple_layer("box");
    box_layer.shape = TextShape::Box { x: 10.0, y: 20.0, width: 80.0, height: 40.0 };
    let bytes_box = build_tysh(&box_layer, 72.0, None);
    let parsed_box = text_layer_from_tysh(&bytes_box, 72.0).expect("parse box");
    match parsed_box.shape {
        TextShape::Box { x, y, width, height } => {
            assert!((x - 10.0).abs() < 0.01);
            assert!((y - 20.0).abs() < 0.01);
            assert!((width - 80.0).abs() < 0.01);
            assert!((height - 40.0).abs() < 0.01);
        }
        _ => panic!("expected box shape"),
    }

    // Bounds from ink for point shape
    let tysh = parse_tysh(&bytes_point).expect("parse tysh");
    // bounds are floor/ceil of ink; ink was [0,0,50,20]
    assert_eq!(tysh.bounds, [0, 0, 50, 20]);
}

#[test]
fn build_tysh_non_positive_dpi_no_panic() {
    let layer = simple_layer("dpi");
    let _ = build_tysh(&layer, 0.0, None);
    let _ = build_tysh(&layer, -1.0, None);
    let _ = build_tysh(&layer, f32::NAN, None);
    let _ = build_tysh(&layer, f32::INFINITY, None);
}

#[test]
fn build_tysh_with_nan_inf_ink_no_panic() {
    let layer = simple_layer("nan");
    let _ = build_tysh(&layer, 72.0, Some([f32::NAN, 0.0, 0.0, 0.0]));
    let _ = build_tysh(&layer, 72.0, Some([f32::INFINITY, f32::NEG_INFINITY, 0.0, 0.0]));
}

#[test]
fn apply_txt2_mismatched_text_does_not_modify_layer() {
    let mut layer = simple_layer("Hello");
    let tysh = build_tysh(&layer, 72.0, None);

    let text_obj = E::Dict(vec![(
        "0".to_string(),
        E::Dict(vec![("0".to_string(), E::String("World\r".to_string())), ("6".to_string(), E::Dict(vec![("0".to_string(), E::Array(vec![]))]))]),
    )]);
    let txt2 = E::Dict(vec![("1".to_string(), E::Dict(vec![("1".to_string(), E::Array(vec![text_obj]))]))]);

    let original_text = layer.text.clone();
    let original_runs_len = layer.runs.len();
    apply_txt2(&mut layer, &tysh, &txt2);
    assert_eq!(layer.text, original_text);
    assert_eq!(layer.runs.len(), original_runs_len);
}

#[test]
fn apply_txt2_invalid_inputs_do_not_panic() {
    let mut layer = simple_layer("abc");
    apply_txt2(&mut layer, &[], &E::Dict(vec![]));
    apply_txt2(&mut layer, &[0; 10], &E::Dict(vec![]));
    // Layer should remain unchanged
    assert_eq!(layer.text, "abc");
    assert_eq!(layer.runs.len(), 1);
}

#[test]
fn parse_tysh_warp_presence() {
    let text_desc = Descriptor::new("TxLr");
    let tysh_no_warp = TySh { transform: identity(), text: text_desc.clone(), warp: None, bounds: [0; 4] };
    let bytes = write_tysh(&tysh_no_warp);
    let parsed = parse_tysh(&bytes).expect("parse");
    assert!(parsed.warp.is_some());

    let warp_desc = Descriptor::new("warp")
        .with("warpStyle", D::Enumerated { type_id: Id::new("warpStyle"), value: Id::new("warpNone") })
        .with("warpValue", D::Double(0.0))
        .with("warpPerspective", D::Double(0.0))
        .with("warpPerspectiveOther", D::Double(0.0))
        .with("warpRotate", D::Enumerated { type_id: Id::new("Ornt"), value: Id::new("Hrzn") });
    let tysh_some_warp = TySh { transform: identity(), text: text_desc, warp: Some(warp_desc.clone()), bounds: [1, 2, 3, 4] };
    let bytes = write_tysh(&tysh_some_warp);
    let parsed = parse_tysh(&bytes).expect("parse");
    assert!(parsed.warp.is_some());
    match (&parsed.warp, &tysh_some_warp.warp) {
        (Some(a), Some(b)) => assert_eq!(a, b),
        _ => panic!("warp mismatch"),
    }
}
