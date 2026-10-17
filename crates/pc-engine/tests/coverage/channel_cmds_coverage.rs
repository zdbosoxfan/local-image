use photocraft_engine::{
    Session,
    channel_cmds::{self, Blending, ChanRef},
    doc::{ColorMode, Document, SampleType, Size},
};
use serde_json::json;

fn rgb_session(w: u32, h: u32) -> Session {
    let mut session = Session::new();
    let doc = Document::new("test", Size::new(w, h), ColorMode::Rgb, SampleType::U8);
    session.add_document(doc, None);
    session
}

#[test]
fn parse_ref_recognizes_strings_and_numbers() {
    let mut s = rgb_session(2, 2);
    let _ = s.execute("channel.new", json!({})).unwrap();
    let doc = &s.active().unwrap().doc;

    // Composite names
    assert_eq!(channel_cmds::parse_ref(&json!("composite"), doc), Some(ChanRef::Composite));
    assert_eq!(channel_cmds::parse_ref(&json!("rgb"), doc), Some(ChanRef::Composite));
    assert_eq!(channel_cmds::parse_ref(&json!("red"), doc), Some(ChanRef::Color(0)));
    assert_eq!(channel_cmds::parse_ref(&json!("green"), doc), Some(ChanRef::Color(1)));
    assert_eq!(channel_cmds::parse_ref(&json!("blue"), doc), Some(ChanRef::Color(2)));

    // Alpha by index
    assert_eq!(channel_cmds::parse_ref(&json!(0), doc), Some(ChanRef::Alpha(0)));
    // Alpha by name
    assert_eq!(channel_cmds::parse_ref(&json!("Alpha 1"), doc), Some(ChanRef::Alpha(0)));

    // Specials
    assert_eq!(channel_cmds::parse_ref(&json!("quickMask"), doc), Some(ChanRef::QuickMask));
    assert_eq!(channel_cmds::parse_ref(&json!("selection"), doc), Some(ChanRef::Selection));
    assert_eq!(channel_cmds::parse_ref(&json!("transparency"), doc), Some(ChanRef::Transparency));
    assert_eq!(channel_cmds::parse_ref(&json!("mask"), doc), Some(ChanRef::LayerMask));
    assert_eq!(channel_cmds::parse_ref(&json!("vectorMask"), doc), Some(ChanRef::VectorMask));

    // Invalid string
    assert_eq!(channel_cmds::parse_ref(&json!("nonsense"), doc), None);

    // Out-of-range index still returns an Alpha variant; bounds are checked elsewhere.
    assert_eq!(channel_cmds::parse_ref(&json!(999), doc), Some(ChanRef::Alpha(999)));
}

#[test]
fn blending_parse_valid_and_clamped() {
    // Valid modes
    assert!(matches!(Blending::parse(&json!({"blending": "multiply"})).unwrap(), Blending::Mode(_)));
    assert!(matches!(Blending::parse(&json!({"blending": "normal"})).unwrap(), Blending::Mode(_)));
    assert!(
        matches!(Blending::parse(&json!({"blending": "add", "scale": 1.5, "offset": 10.0})).unwrap(), Blending::Add { scale, offset } if (scale - 1.5).abs() < 1e-6 && (offset - (10.0 / 255.0)).abs() < 1e-6)
    );
    // Scale clamped to [1,2]
    assert!(matches!(Blending::parse(&json!({"blending": "add", "scale": 5.0})).unwrap(), Blending::Add { scale, .. } if (scale - 2.0).abs() < 1e-6));
    // Offset clamped to [-255,255] then divided
    assert!(
        matches!(Blending::parse(&json!({"blending": "subtract", "offset": 1000.0})).unwrap(), Blending::Subtract { offset, .. } if (offset - 1.0).abs() < 1e-6)
    );
    // Unsupported blending
    assert!(Blending::parse(&json!({"blending": "dissolve"})).is_err());
    assert!(Blending::parse(&json!({"blending": "unknown"})).is_err());
}

#[test]
fn merge_channels_from_grayscale_docs() {
    let mut s = Session::new();
    // Create three grayscale docs
    let mut doc_indices = vec![];
    for _ in 0..3 {
        let doc = Document::new("gray", Size::new(2, 2), ColorMode::Grayscale, SampleType::U8);
        doc_indices.push(s.add_document(doc, None));
    }
    // Activate first
    s.set_active(doc_indices[0]);
    // Execute merge
    let res = s.execute("channel.merge", json!({"mode": "rgb"})).unwrap();
    let new_idx = res["document"].as_u64().unwrap() as usize;
    // Original 3 closed, 1 new RGB
    assert_eq!(s.documents().len(), 1);
    let doc = &s.documents()[new_idx].doc;
    assert_eq!(doc.mode, ColorMode::Rgb);
    assert_eq!(doc.size, Size::new(2, 2));
}

#[test]
fn calculations_new_channel() {
    let mut s = rgb_session(2, 2);
    let res = s
        .execute(
            "image.calculations",
            json!({
                "source1": {"channel": "composite"},
                "source2": {"channel": "composite"},
                "result": "newChannel"
            }),
        )
        .unwrap();
    let idx = res["channel"].as_u64().unwrap() as usize;
    assert_eq!(s.active().unwrap().doc.channels.len(), 1);
    assert_eq!(s.active().unwrap().doc.channels[idx].name, "Alpha 1");
}

#[test]
fn channel_options_quick_mask() {
    let mut s = rgb_session(2, 2);
    s.execute(
        "channel.options",
        json!({
            "channel": "quickMask",
            "color": "#0000ff",
            "opacity": 60.0,
            "indicates": "selectedAreas"
        }),
    )
    .unwrap();
    let opts = s.quick_mask_options;
    assert_eq!(opts.color.to_rgb(), [0.0, 0.0, 1.0]);
    assert!((opts.opacity - 0.6).abs() < 1e-6);
    assert_eq!(opts.indicates, photocraft_engine::doc::ColorIndicates::SelectedAreas);
}

#[test]
fn error_bad_channel_index_is_disabled() {
    let mut s = rgb_session(2, 2);
    // No channels yet: command is disabled by its precondition.
    let res = s.execute("channel.delete", json!({"channel": 0}));
    assert!(res.is_err());
    assert!(matches!(res.unwrap_err(), photocraft_engine::EngineError::Disabled(..)));
}

#[test]
fn error_missing_required_param() {
    let mut s = rgb_session(2, 2);
    let _ = s.execute("channel.new", json!({})).unwrap();
    // rename without name
    let res = s.execute("channel.rename", json!({"channel": 0}));
    assert!(res.is_err());
    assert!(matches!(res.unwrap_err(), photocraft_engine::EngineError::BadParams { .. }));
}

#[test]
fn tiny_document_operations_no_panic() {
    let mut s = rgb_session(1, 1);
    let _ = s.execute("channel.new", json!({})).unwrap();
    let _ = s.execute("channel.duplicate", json!({"channel": 0})).unwrap();
    let _ = s.execute("select.editInQuickMaskMode", json!({"on": true})).unwrap();
    let _ = s.execute("channel.split", json!({})).unwrap();
    // Original closed, 3 color + 2 alpha = 5 grayscale documents
    assert_eq!(s.documents().len(), 5);
}
