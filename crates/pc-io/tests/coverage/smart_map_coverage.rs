use photocraft_color::BlendMode;
use photocraft_doc::{DevelopLink, SmartFilter};
use photocraft_geom::Affine;
use photocraft_io::develop_filter::COMMAND as DEVELOP_COMMAND;
use photocraft_io::smart_map::{
    DEVELOP_LAYER_MARKER, FilterStack, PlacedSpec, develop_layer_filter, filter_fx, filter_stack, item_for_filter, parse_sold, sold_bytes, sold_descriptor,
    stored_size,
};
use photocraft_psd::descriptor::{Descriptor, Value};
use serde_json::json;

fn sf(command: &str, params: serde_json::Value) -> SmartFilter {
    SmartFilter { command: command.into(), params, blend: BlendMode::Normal, opacity: 1.0, visible: true }
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
fn parse_sold_invalid_data_returns_none() {
    assert!(parse_sold(b"").is_none());
    assert!(parse_sold(b"not sold").is_none());
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
fn item_for_filter_unknown_command_errors() {
    let f = sf("filter.distort.twirl", json!({"angle": 50}));
    let err = item_for_filter(&f).err().unwrap();
    assert!(err.contains("Photoshop has no equivalent filter"));
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
fn develop_layer_filter_creates_marker_and_command() {
    let link = DevelopLink { library_baseline: None, source: None, library: None, settings: json!({"exposure": 0.5, "contrast": 10}), photo: Some(42) };
    let f = develop_layer_filter(&link);
    assert_eq!(f.command, DEVELOP_COMMAND);
    let marker = &f.params[DEVELOP_LAYER_MARKER];
    assert_eq!(marker["settings"], link.settings);
    assert_eq!(marker["photo"], json!(42));
}
