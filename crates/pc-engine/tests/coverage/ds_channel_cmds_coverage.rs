use photocraft_engine::{
    Session,
    channel_cmds::{self, Blending, ChanRef, ChannelTarget},
    doc::{ColorMode, Document, SampleType, Size},
};
use serde_json::json;

fn rgb_session(w: u32, h: u32) -> Session {
    let mut session = Session::new();
    let doc = Document::new("test", Size::new(w, h), ColorMode::Rgb, SampleType::U8);
    session.add_document(doc, None);
    session
}

fn channel_values(session: &Session, channel_idx: usize) -> Vec<f32> {
    let doc = &session.active().unwrap().doc;
    let area = doc.bounds();
    let surface = &doc.channels[channel_idx].surface;
    surface.read_region(area)
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
fn new_channel_black_and_white_fill() {
    let mut s = rgb_session(2, 2);
    // Black (default)
    let res = s.execute("channel.new", json!({})).unwrap();
    let idx = res["channel"].as_u64().unwrap() as usize;
    assert_eq!(s.active().unwrap().doc.channels.len(), 1);
    assert_eq!(s.active().unwrap().doc.channels[0].name, "Alpha 1");
    let vals = channel_values(&s, idx);
    assert!(vals.iter().all(|&v| (v - 0.0).abs() < 1e-6));

    // White
    let res = s.execute("channel.new", json!({"fill": "white"})).unwrap();
    let idx2 = res["channel"].as_u64().unwrap() as usize;
    assert_eq!(s.active().unwrap().doc.channels.len(), 2);
    let vals = channel_values(&s, idx2);
    assert!(vals.iter().all(|&v| (v - 1.0).abs() < 1e-6));
}

#[test]
fn save_load_selection_roundtrip() {
    let mut s = rgb_session(2, 2);
    // Create alpha channel with all white (selection)
    let _ = s.execute("channel.new", json!({"fill": "white"})).unwrap();
    // Load selection from that alpha
    let res = s.execute("select.loadSelection", json!({"channel": 0})).unwrap();
    assert_eq!(res["selected"], true);
    // Save selection to a new channel
    let res = s.execute("select.saveSelection", json!({"operation": "new"})).unwrap();
    let saved_idx = res["channel"].as_u64().unwrap() as usize;
    // Should have two alpha channels now
    assert_eq!(s.active().unwrap().doc.channels.len(), 2);
    // The new channel should be all white as well
    let vals = channel_values(&s, saved_idx);
    assert!(vals.iter().all(|&v| (v - 1.0).abs() < 1e-6));
}

#[test]
fn quick_mask_toggle_on_off() {
    let mut s = rgb_session(2, 2);
    // Enter quick mask
    let res = s.execute("select.editInQuickMaskMode", json!({"on": true})).unwrap();
    assert_eq!(res["quickMask"], true);
    assert!(s.active().unwrap().doc.quick_mask.is_some());
    // Exit
    let res = s.execute("select.editInQuickMaskMode", json!({"on": false})).unwrap();
    assert_eq!(res["quickMask"], false);
    assert!(s.active().unwrap().doc.quick_mask.is_none());
    // No selection should be created from all-white quick mask
    assert!(s.active().unwrap().doc.selection.is_none());
}

#[test]
fn duplicate_channel_defaults_and_invert() {
    let mut s = rgb_session(2, 2);
    // Create a white alpha
    let _ = s.execute("channel.new", json!({"fill": "white"})).unwrap();
    // Duplicate without invert
    let res = s.execute("channel.duplicate", json!({"channel": 0})).unwrap();
    let dup_idx = res["channel"].as_u64().unwrap() as usize;
    assert_eq!(s.active().unwrap().doc.channels.len(), 2);
    assert_eq!(s.active().unwrap().doc.channels[dup_idx].name, "Alpha 1 copy");
    let vals = channel_values(&s, dup_idx);
    assert!(vals.iter().all(|&v| (v - 1.0).abs() < 1e-6));

    // Duplicate with invert
    let res = s.execute("channel.duplicate", json!({"channel": 0, "invert": true})).unwrap();
    let inv_idx = res["channel"].as_u64().unwrap() as usize;
    assert_eq!(s.active().unwrap().doc.channels.len(), 3);
    let vals = channel_values(&s, inv_idx);
    assert!(vals.iter().all(|&v| (v - 0.0).abs() < 1e-6));
}

#[test]
fn delete_and_rename_channel() {
    let mut s = rgb_session(2, 2);
    // Create two channels
    let _ = s.execute("channel.new", json!({})).unwrap();
    let _ = s.execute("channel.new", json!({})).unwrap();
    assert_eq!(s.active().unwrap().doc.channels.len(), 2);

    // Rename first
    s.execute("channel.rename", json!({"channel": 0, "name": "Renamed"})).unwrap();
    assert_eq!(s.active().unwrap().doc.channels[0].name, "Renamed");

    // Delete second (index 1)
    s.execute("channel.delete", json!({"channel": 1})).unwrap();
    assert_eq!(s.active().unwrap().doc.channels.len(), 1);
    assert_eq!(s.active().unwrap().doc.channels[0].name, "Renamed");
}

#[test]
fn move_channel_reorders() {
    let mut s = rgb_session(2, 2);
    // Create three channels
    for _ in 0..3 {
        let _ = s.execute("channel.new", json!({})).unwrap();
    }
    let assert_names = |s: &Session, expected: &[&str]| {
        let doc = &s.active().unwrap().doc;
        let names: Vec<_> = doc.channels.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, expected);
    };
    assert_names(&s, &["Alpha 1", "Alpha 2", "Alpha 3"]);
    // Move index 0 to index 2
    s.execute("channel.move", json!({"channel": 0, "to": 2})).unwrap();
    assert_names(&s, &["Alpha 2", "Alpha 3", "Alpha 1"]);
    // Move index 2 to index 0
    s.execute("channel.move", json!({"channel": 2, "to": 0})).unwrap();
    assert_names(&s, &["Alpha 1", "Alpha 2", "Alpha 3"]);
}

#[test]
fn channel_target_and_visibility() {
    let mut s = rgb_session(2, 2);
    // Create an alpha
    let _ = s.execute("channel.new", json!({})).unwrap();
    // Target that alpha
    s.execute("channel.target", json!({"channel": 0})).unwrap();
    let view = &s.active().unwrap().channel_view;
    assert_eq!(view.target, ChannelTarget::Alpha(0));
    // In the target, color channels should be hidden and alpha visible
    assert!(view.color_hidden.iter().all(|&h| h));
    assert_eq!(view.alpha_visible, vec![true]);

    // Set composite visibility off
    s.execute("channel.setVisible", json!({"channel": "composite", "visible": false})).unwrap();
    let view = &s.active().unwrap().channel_view;
    assert!(view.color_hidden.iter().all(|&h| h)); // still hidden due to target
    assert_eq!(view.visible_colors(3), 0);
}

#[test]
fn spot_channel_create_and_merge() {
    let mut s = rgb_session(2, 2);
    // Create spot channel
    let res = s.execute("channel.newSpot", json!({"color": "#ff0000", "solidity": 50.0})).unwrap();
    let idx = res["channel"].as_u64().unwrap() as usize;
    let doc = &s.active().unwrap().doc;
    assert!(doc.channels[idx].spot.is_some());
    // Merge spot
    s.execute("channel.mergeSpot", json!({"channel": idx})).unwrap();
    let doc = &s.active().unwrap().doc;
    assert_eq!(doc.channels.len(), 0);
    assert_eq!(doc.layers.len(), 1); // flattened to background
    assert_eq!(doc.layers[0].name, "Background");
}

#[test]
fn split_channels_creates_grayscale_docs() {
    let mut s = rgb_session(2, 2);
    let initial_count = s.documents().len();
    let res = s.execute("channel.split", json!({})).unwrap();
    let made: Vec<usize> = res["documents"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as usize).collect();
    // RGB -> 3 grayscale docs, original closed
    assert_eq!(initial_count, 1);
    assert_eq!(made.len(), 3);
    assert_eq!(s.documents().len(), 3);
    for idx in &made {
        let doc = &s.documents()[*idx].doc;
        assert_eq!(doc.mode, ColorMode::Grayscale);
        assert_eq!(doc.size, Size::new(2, 2));
    }
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
fn apply_image_on_alpha_channel_smoke() {
    let mut s = rgb_session(2, 2);
    // Create an alpha channel to target
    let _ = s.execute("channel.new", json!({})).unwrap();
    // Apply image onto that alpha channel (doesn't require a pixel layer)
    let res = s.execute(
        "image.applyImage",
        json!({
            "source": {"channel": "composite"},
            "blending": "multiply",
            "opacity": 100.0,
            "target": {"channel": 0}
        }),
    );
    assert!(res.is_ok());
    // Document still has the alpha channel
    assert_eq!(s.active().unwrap().doc.channels.len(), 1);
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
fn channel_options_set_color_opacity() {
    let mut s = rgb_session(2, 2);
    let _ = s.execute("channel.new", json!({})).unwrap();
    s.execute(
        "channel.options",
        json!({
            "channel": 0,
            "color": "#00ff00",
            "opacity": 75.0
        }),
    )
    .unwrap();
    let doc = &s.active().unwrap().doc;
    let ch = &doc.channels[0];
    assert_eq!(ch.color.to_rgb(), [0.0, 1.0, 0.0]);
    assert!((ch.opacity - 0.75).abs() < 1e-6);
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
fn channel_list_json_structure() {
    let mut s = rgb_session(2, 2);
    let _ = s.execute("channel.new", json!({})).unwrap();
    let res = s.execute("channel.list", json!({})).unwrap();
    assert!(res["composite"].is_string());
    assert!(res["colors"].is_array());
    assert_eq!(res["colors"].as_array().unwrap().len(), 3);
    assert!(res["alpha"].is_array());
    assert_eq!(res["alpha"].as_array().unwrap().len(), 1);
    assert!(res["target"].is_object());
    assert!(res["compositeVisible"].is_boolean());
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

#[test]
fn quick_mask_options_and_toggle_persist() {
    let mut s = rgb_session(2, 2);
    // Set options
    s.execute(
        "channel.options",
        json!({
            "channel": "quickMask",
            "color": "#00ffff",
            "opacity": 30.0
        }),
    )
    .unwrap();
    // Enter quick mask
    s.execute("select.editInQuickMaskMode", json!({"on": true})).unwrap();
    let doc = &s.active().unwrap().doc;
    let qm = doc.quick_mask.as_ref().unwrap();
    assert_eq!(qm.color.to_rgb(), [0.0, 1.0, 1.0]);
    assert!((qm.opacity - 0.3).abs() < 1e-6);
}
