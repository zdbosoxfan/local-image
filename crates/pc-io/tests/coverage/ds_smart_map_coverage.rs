use photocraft_color::BlendMode;
use photocraft_doc::{DevelopLink, SmartFilter};
use photocraft_geom::Affine;
use photocraft_io::develop_filter::COMMAND as DEVELOP_COMMAND;
use photocraft_io::smart_map::{
    DEVELOP_LAYER_MARKER, FilterStack, PlacedSpec, UNSUPPORTED_FILTER, develop_layer_filter, filter_from_item, filter_fx, filter_stack, item_for_filter,
    parse_sold, plld_bytes, sold_bytes, sold_descriptor, stored_size, uuid_from,
};
use photocraft_psd::TaggedBlock;
use photocraft_psd::descriptor::{Descriptor, Id, UnicodeString, Value};
use serde_json::json;

fn sf(command: &str, params: serde_json::Value) -> SmartFilter {
    SmartFilter { command: command.into(), params, blend: BlendMode::Normal, opacity: 1.0, visible: true }
}

fn unit(unit: &[u8; 4], value: f64) -> Value {
    Value::UnitFloat { unit: *unit, value }
}

fn enum_value(type_id: &str, value: &str) -> Value {
    Value::Enumerated { type_id: Id::new(type_id), value: Id::new(value) }
}

fn code4(s: &[u8; 4]) -> i32 {
    i32::from_be_bytes(*s)
}

fn blend_options_descriptor(blend: Option<BlendMode>, opacity: f32) -> Value {
    let op = if opacity.is_finite() { f64::from(opacity.clamp(0.0, 1.0)) * 100.0 } else { 100.0 };
    let md = match blend.unwrap_or(BlendMode::Normal) {
        BlendMode::Normal => enum_value("BlnM", "normal"),
        BlendMode::Overlay => enum_value("BlnM", "overlay"),
        BlendMode::Screen => enum_value("BlnM", "screen"),
        _ => enum_value("BlnM", "normal"),
    };
    Value::Descriptor(Descriptor::new("blendOptions").with("Opct", unit(b"#Prc", op)).with("Md  ", md))
}

fn filter_item(name: &str, filter_id: i32, fltr: Option<Descriptor>, blend: Option<BlendMode>, opacity: f32, visible: bool) -> Descriptor {
    let mut d = Descriptor::new("filterFX")
        .with("Nm  ", Value::Text(UnicodeString::new_nul(name)))
        .with("blendOptions", blend_options_descriptor(blend, opacity))
        .with("enab", Value::Boolean(visible))
        .with("hasoptions", Value::Boolean(fltr.is_some()))
        .with(
            "FrgC",
            Value::Descriptor(Descriptor::new("RGBC").with("Rd  ", Value::Double(0.0)).with("Grn ", Value::Double(0.0)).with("Bl  ", Value::Double(0.0))),
        )
        .with(
            "BckC",
            Value::Descriptor(Descriptor::new("RGBC").with("Rd  ", Value::Double(1.0)).with("Grn ", Value::Double(1.0)).with("Bl  ", Value::Double(1.0))),
        );
    if let Some(f) = fltr {
        d = d.with("Fltr", Value::Descriptor(f));
    }
    d.with("filterID", Value::Integer(filter_id))
}

fn simple_placed_spec<'a>(filter_fx: Option<Descriptor>) -> PlacedSpec<'a> {
    PlacedSpec {
        idnt: "id-1",
        placed: "pl-2",
        transform: Affine { m: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0] },
        perspective: None,
        size: (64.0, 32.0),
        dpi: 72.0,
        warp: None,
        filter_fx,
    }
}

#[test]
fn sold_descriptor_rejects_invalid_prefix_and_short_data() {
    assert_eq!(sold_descriptor(b"notsold"), None);
    assert_eq!(sold_descriptor(b""), None);
    assert_eq!(sold_descriptor(b"soLD"), None);
}

#[test]
fn sold_descriptor_rejects_malformed_after_prefix() {
    let mut data = b"soLD".to_vec();
    data.extend_from_slice(&4u32.to_be_bytes());
    data.extend_from_slice(&[0xFF; 32]);
    assert_eq!(sold_descriptor(&data), None);
}

#[test]
fn sold_descriptor_parses_generated_sold() {
    let spec = simple_placed_spec(None);
    let mut w = Vec::new();
    let sold = sold_bytes(None, &spec, &mut w);
    let d = sold_descriptor(&sold).unwrap();
    assert!(d.get("Idnt").is_some());
    assert!(d.get("placed").is_some());
}

#[test]
fn parse_sold_invalid_data_returns_none() {
    assert!(parse_sold(b"").is_none());
    assert!(parse_sold(b"not sold").is_none());
}

#[test]
fn parse_sold_extracts_fields_from_generated_block() {
    let stack = FilterStack { enabled: false, filters: vec![sf("filter.blur.gaussianBlur", json!({"radius": 3.0}))], mask_enabled: false, mask_linked: true };
    let mut w = Vec::new();
    let fx = filter_fx(&stack, "L", &mut w);
    let spec = PlacedSpec {
        idnt: "id-1",
        placed: "pl-2",
        transform: Affine { m: [0.5, 0.1, -0.1, 0.5, 10.0, 20.0] },
        perspective: None,
        size: (64.0, 32.0),
        dpi: 72.0,
        warp: None,
        filter_fx: Some(fx),
    };
    let sold = sold_bytes(None, &spec, &mut w);
    let p = parse_sold(&sold).unwrap();
    assert_eq!(p.idnt, "id-1");
    assert_eq!(p.placed, "pl-2");
    for (a, b) in p.transform.m.iter().zip(spec.transform.m) {
        assert!((a - b).abs() < 1e-9);
    }
    let parsed_stack = p.stack.unwrap();
    assert!(!parsed_stack.enabled);
    assert!(!parsed_stack.mask_enabled);
    assert!(parsed_stack.mask_linked);
    assert_eq!(parsed_stack.filters.len(), 1);
    assert_eq!(parsed_stack.filters[0].command, "filter.blur.gaussianBlur");
    assert_eq!(parsed_stack.filters[0].params["radius"], json!(3.0));
}

#[test]
fn filter_stack_defaults_on_empty_descriptor() {
    let d = Descriptor::new("filterFXStyle");
    let stack = filter_stack(&d);
    assert!(stack.enabled);
    assert!(stack.filters.is_empty());
    assert!(stack.mask_enabled);
    assert!(!stack.mask_linked);
}

#[test]
fn filter_stack_reads_flags_and_empty_list() {
    let d = Descriptor::new("filterFXStyle")
        .with("enab", Value::Boolean(false))
        .with("filterMaskEnable", Value::Boolean(false))
        .with("filterMaskLinked", Value::Boolean(true))
        .with("filterFXList", Value::List(vec![]));
    let stack = filter_stack(&d);
    assert!(!stack.enabled);
    assert!(!stack.mask_enabled);
    assert!(stack.mask_linked);
    assert!(stack.filters.is_empty());
}

#[test]
fn filter_from_item_parses_gaussian_blur() {
    let fltr = Descriptor::new("GsnB").with("Rds ", unit(b"#Pxl", 4.0));
    let item = filter_item("Gaussian Blur...", code4(b"GsnB"), Some(fltr), Some(BlendMode::Overlay), 0.75, false);
    let f = filter_from_item(&item);
    assert_eq!(f.command, "filter.blur.gaussianBlur");
    assert_eq!(f.params["radius"], json!(4.0));
    assert_eq!(f.blend, BlendMode::Overlay);
    assert!((f.opacity - 0.75).abs() < f32::EPSILON);
    assert!(!f.visible);
}

#[test]
fn unknown_filter_is_unsupported_and_roundtrips() {
    let fltr = Descriptor::new("Twrl").with("Angl", Value::Integer(50));
    let item = filter_item("Twirl...", code4(b"Twrl"), Some(fltr), None, 1.0, true);
    let f = filter_from_item(&item);
    assert_eq!(f.command, UNSUPPORTED_FILTER);
    assert_eq!(f.params["name"], "Twirl...");
    let back = item_for_filter(&f).unwrap();
    assert_eq!(back, item);
}

#[test]
fn item_for_filter_unknown_command_errors() {
    let f = sf("filter.distort.twirl", json!({"angle": 50}));
    let err = item_for_filter(&f).err().unwrap();
    assert!(err.contains("Photoshop has no equivalent filter"));
}

#[test]
fn all_modelled_filters_roundtrip() {
    let filters = vec![
        sf("filter.blur.gaussianBlur", json!({"radius": 4.0})),
        sf("filter.blur.boxBlur", json!({"radius": 5.0})),
        sf("filter.blur.motionBlur", json!({"angle": -20, "distance": 10.0})),
        sf("filter.sharpen.unsharpMask", json!({"amount": 150.0, "radius": 2.0, "threshold": 0})),
        sf("filter.noise.addNoise", json!({"amount": 12.0, "distribution": "uniform", "monochromatic": false, "seed": 1234})),
        sf("filter.noise.median", json!({"radius": 3.0})),
        sf("filter.other.highPass", json!({"radius": 3.0})),
        sf("filter.other.maximum", json!({"radius": 2.0})),
        sf("filter.other.minimum", json!({"radius": 2.0})),
        sf("filter.pixelate.mosaic", json!({"cellSize": 8.0})),
        sf("filter.stylize.emboss", json!({"angle": 135, "height": 3, "amount": 100})),
        sf("image.adjustments.levels", json!({"inBlack": 20, "inWhite": 220, "gamma": 1.4, "red": {"inBlack": 0, "inWhite": 255, "gamma": 0.8}})),
        sf("image.adjustments.curves", json!({"points": [[0.0, 0.0], [64.0, 40.0], [255.0, 255.0]], "blue": [[0.0, 20.0], [255.0, 235.0]]})),
        sf("image.adjustments.shadowsHighlights", json!({})),
    ];
    for mut f in filters {
        f.blend = BlendMode::Overlay;
        f.opacity = 0.75;
        f.visible = false;
        let d = item_for_filter(&f).unwrap();
        let bytes = d.to_bytes();
        let back = filter_from_item(&Descriptor::from_bytes(&bytes).unwrap());
        assert_eq!(back, f, "{}", f.command);
    }
}

#[test]
fn filter_fx_skips_unwritable_and_warns() {
    let stack = FilterStack {
        filters: vec![
            sf("filter.distort.twirl", json!({"angle": 50})),
            sf("filter.blur.gaussianBlur", json!({"radius": 2.0})),
            sf(UNSUPPORTED_FILTER, json!({"psd": "zz"})),
            sf("image.adjustments.levels", json!({"cyan": {"gamma": 2.0}})),
            sf("image.adjustments.shadowsHighlights", json!({"shadowAmount": 50})),
        ],
        ..Default::default()
    };
    let mut warnings = Vec::new();
    let d = filter_fx(&stack, "L", &mut warnings);
    assert_eq!(warnings.len(), 4, "{warnings:?}");
    let back = filter_stack(&d);
    assert_eq!(back.filters.len(), 1);
    assert_eq!(back.filters[0].command, "filter.blur.gaussianBlur");
}

#[test]
fn filter_fx_preserves_flags_and_order() {
    let stack = FilterStack {
        enabled: false,
        filters: vec![sf("filter.blur.gaussianBlur", json!({"radius": 1.0})), sf("filter.blur.boxBlur", json!({"radius": 2.0}))],
        mask_enabled: false,
        mask_linked: true,
    };
    let mut warnings = Vec::new();
    let d = filter_fx(&stack, "L", &mut warnings);
    assert!(warnings.is_empty());
    let back = filter_stack(&d);
    assert!(!back.enabled);
    assert!(!back.mask_enabled);
    assert!(back.mask_linked);
    let commands: Vec<_> = back.filters.iter().map(|f| f.command.as_str()).collect();
    assert_eq!(commands, vec!["filter.blur.gaussianBlur", "filter.blur.boxBlur"]);
}

#[test]
fn sold_bytes_is_padded_and_starts_with_sold() {
    let spec = simple_placed_spec(None);
    let mut w = Vec::new();
    let bytes = sold_bytes(None, &spec, &mut w);
    assert!(bytes.starts_with(b"soLD"));
    assert_eq!(bytes.len() % 4, 0);
}

#[test]
fn sold_bytes_template_preserves_extra_keys_and_clears_filter_fx() {
    let stack = FilterStack { filters: vec![sf("filter.blur.gaussianBlur", json!({"radius": 2.0}))], ..Default::default() };
    let mut w = Vec::new();
    let fx = filter_fx(&stack, "L", &mut w);
    let spec_with_fx = simple_placed_spec(Some(fx));
    let sold = sold_bytes(None, &spec_with_fx, &mut w);
    let mut template = sold_descriptor(&sold).unwrap();
    let original_trnf = template.get("Trnf").cloned();
    template.items.push((Id::new("extraKey"), Value::Integer(7)));
    let spec_no_fx = simple_placed_spec(None);
    let sold2 = sold_bytes(Some(&template), &spec_no_fx, &mut w);
    let d = sold_descriptor(&sold2).unwrap();
    assert_eq!(d.get("extraKey"), Some(&Value::Integer(7)));
    assert!(d.get("filterFX").is_none());
    assert_eq!(d.get("Trnf"), original_trnf.as_ref());
}

#[test]
fn sold_bytes_updates_transform_when_moved() {
    let mut spec = simple_placed_spec(None);
    let mut w = Vec::new();
    let sold = sold_bytes(None, &spec, &mut w);
    let template = sold_descriptor(&sold).unwrap();
    let original_trnf = template.get("Trnf").cloned();
    spec.transform = Affine { m: [1.0, 0.0, 0.0, 1.0, 5.0, 0.0] };
    let sold2 = sold_bytes(Some(&template), &spec, &mut w);
    let d = sold_descriptor(&sold2).unwrap();
    assert_ne!(d.get("Trnf"), original_trnf.as_ref());
}

#[test]
fn plld_bytes_valid_structure() {
    let spec = simple_placed_spec(None);
    let mut w = Vec::new();
    let pl = plld_bytes(&spec, &mut w);
    assert!(pl.starts_with(b"plcL"));
    assert_eq!(&pl[4..8], &3u32.to_be_bytes()[..]);
    assert_eq!(pl.len() % 4, 0);
    let tb = TaggedBlock::new(*b"PlLd", pl);
    assert!(tb.check_structure().is_ok());
}

#[test]
fn stored_size_reads_sz_descriptor() {
    let d =
        Descriptor::new("null").with("Sz  ", Value::Descriptor(Descriptor::new("Pnt ").with("Wdth", Value::Double(123.0)).with("Hght", Value::Double(45.0))));
    assert_eq!(stored_size(&d), Some((123.0, 45.0)));

    let d2 = Descriptor::new("null");
    assert_eq!(stored_size(&d2), None);

    let d3 =
        Descriptor::new("null").with("Sz  ", Value::Descriptor(Descriptor::new("Pnt ").with("Wdth", Value::Double(0.0)).with("Hght", Value::Double(10.0))));
    assert_eq!(stored_size(&d3), None);
}

#[test]
fn uuid_from_deterministic_and_unique() {
    let a = uuid_from(b"hello");
    let b = uuid_from(b"hello");
    let c = uuid_from(b"world");
    assert_eq!(a, b);
    assert_ne!(a, c);
    assert_eq!(a.len(), 36);
    assert!(a.chars().all(|c| c.is_ascii_hexdigit() || c == '-'));
}

#[test]
fn develop_layer_filter_creates_marker_and_command() {
    let link = DevelopLink { settings: json!({"exposure": 0.5, "contrast": 10}), photo: Some(42) };
    let f = develop_layer_filter(&link);
    assert_eq!(f.command, DEVELOP_COMMAND);
    let marker = &f.params[DEVELOP_LAYER_MARKER];
    assert_eq!(marker["settings"], link.settings);
    assert_eq!(marker["photo"], json!(42));
}
