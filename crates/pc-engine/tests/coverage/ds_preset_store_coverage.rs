use photocraft_engine::paint::{BrushPreset, BrushSettings, GrayTile, TipShape};
use photocraft_engine::preset_store::{INDEX_FILE, MAX_TIP_SIDE, MemBackend, PresetBackend, TIPS_DIR, decode_tip, encode_tip, group_file_name, open};

struct FailingBackend;

impl PresetBackend for FailingBackend {
    fn list(&self) -> Result<Vec<(String, u64)>, String> {
        Err("failing list".into())
    }
    fn read(&self, _name: &str, _max: u64) -> Result<Vec<u8>, String> {
        Err("failing read".into())
    }
    fn write(&self, _name: &str, _bytes: &[u8]) -> Result<(), String> {
        Err("failing write".into())
    }
    fn remove(&self, _name: &str) -> Result<(), String> {
        Err("failing remove".into())
    }
}

fn user_preset(name: &str, group: &str) -> BrushPreset {
    BrushPreset { name: name.to_string(), brush: BrushSettings::default(), builtin: false, group: group.to_string() }
}

fn sampled_preset(name: &str, group: &str) -> BrushPreset {
    let brush = BrushSettings { tip: TipShape::Sampled(GrayTile { width: 2, height: 2, data: vec![0, 257, 514, 65535] }), ..Default::default() };
    BrushPreset { name: name.to_string(), brush, builtin: false, group: group.to_string() }
}

#[test]
fn encode_decode_tip_roundtrip_8bit() {
    let tile = GrayTile { width: 2, height: 2, data: vec![0, 257, 514, 65535] };
    let bytes = encode_tip(&tile);
    assert!(bytes.len() > 15);
    assert_eq!(bytes[6], 8);
    let decoded = decode_tip(&bytes).unwrap();
    assert_eq!(decoded.width, tile.width);
    assert_eq!(decoded.height, tile.height);
    assert_eq!(decoded.data, tile.data);
}

#[test]
fn encode_decode_tip_roundtrip_16bit() {
    let tile = GrayTile { width: 3, height: 2, data: vec![0, 1, 255, 256, 10000, 65535] };
    let bytes = encode_tip(&tile);
    assert!(bytes.len() > 15);
    assert_eq!(bytes[6], 16);
    let decoded = decode_tip(&bytes).unwrap();
    assert_eq!(decoded.width, tile.width);
    assert_eq!(decoded.height, tile.height);
    assert_eq!(decoded.data, tile.data);
}

#[test]
fn decode_tip_rejects_bad_magic() {
    let mut bytes = vec![0u8; 15];
    bytes[..6].copy_from_slice(b"HELLO!");
    bytes[6] = 8;
    bytes[7..11].copy_from_slice(&1u32.to_le_bytes());
    bytes[11..15].copy_from_slice(&1u32.to_le_bytes());
    assert!(decode_tip(&bytes).is_err());
}

#[test]
fn decode_tip_rejects_truncated_header() {
    assert!(decode_tip(&[0u8; 10]).is_err());
}

#[test]
fn decode_tip_rejects_zero_dimensions() {
    let mut bytes = vec![0u8; 15];
    bytes[..6].copy_from_slice(b"PCTIP1");
    bytes[6] = 8;
    bytes[7..11].copy_from_slice(&0u32.to_le_bytes());
    bytes[11..15].copy_from_slice(&1u32.to_le_bytes());
    let err = decode_tip(&bytes).unwrap_err();
    assert!(err.contains("bad tip size"), "unexpected error: {err}");
}

#[test]
fn decode_tip_rejects_oversized_side() {
    let mut bytes = vec![0u8; 15];
    bytes[..6].copy_from_slice(b"PCTIP1");
    bytes[6] = 8;
    bytes[7..11].copy_from_slice(&(MAX_TIP_SIDE + 1).to_le_bytes());
    bytes[11..15].copy_from_slice(&1u32.to_le_bytes());
    let err = decode_tip(&bytes).unwrap_err();
    assert!(err.contains("bad tip size"), "unexpected error: {err}");
}

#[test]
fn decode_tip_rejects_too_many_samples() {
    let mut bytes = vec![0u8; 15];
    bytes[..6].copy_from_slice(b"PCTIP1");
    bytes[6] = 8;
    bytes[7..11].copy_from_slice(&MAX_TIP_SIDE.to_le_bytes());
    bytes[11..15].copy_from_slice(&MAX_TIP_SIDE.to_le_bytes());
    let err = decode_tip(&bytes).unwrap_err();
    assert!(err.contains("tip too large"), "unexpected error: {err}");
}

#[test]
fn decode_tip_rejects_truncated_deflate_data() {
    let mut bytes = vec![0u8; 15];
    bytes[..6].copy_from_slice(b"PCTIP1");
    bytes[6] = 8;
    bytes[7..11].copy_from_slice(&1u32.to_le_bytes());
    bytes[11..15].copy_from_slice(&1u32.to_le_bytes());
    assert!(decode_tip(&bytes).is_err());
}

#[test]
fn group_file_name_sanitizes_and_is_deterministic() {
    let empty = group_file_name("");
    assert!(empty.starts_with("Ungrouped-"));
    assert!(empty.ends_with(".pcbrushes"));

    let weird = group_file_name("a/b\\c");
    assert!(!weird.contains('/') && !weird.contains('\\'));
    assert_eq!(weird, group_file_name("a/b\\c"));
    assert_ne!(group_file_name("x"), group_file_name("y"));
}

#[test]
fn open_empty_backend_is_empty() {
    let backend = MemBackend::default();
    let opened = open(Box::new(backend));
    assert!(opened.presets.is_empty());
    assert!(opened.warnings.is_empty());
    assert_eq!(opened.store.bytes(), 0);
}

#[test]
fn open_malformed_index_warns_and_continues() {
    let backend = MemBackend::default();
    backend.write(INDEX_FILE, b"not json").unwrap();
    let opened = open(Box::new(backend));
    assert!(opened.presets.is_empty());
    assert!(opened.warnings.iter().any(|w| w.contains("brush presets") && w.contains(INDEX_FILE)));
}

#[test]
fn open_malformed_group_file_warns_and_skips() {
    let backend = MemBackend::default();
    backend.write("bad.pcbrushes", b"not json").unwrap();
    let opened = open(Box::new(backend));
    assert!(opened.presets.is_empty());
    assert!(opened.warnings.iter().any(|w| w.contains("bad.pcbrushes") && w.contains("skipped")));
}

#[test]
fn open_unsupported_group_version_warns() {
    let backend = MemBackend::default();
    backend.write("badversion.pcbrushes", br#"{"format":"photocraft-brush-group","version":2,"group":"g","presets":[]}"#).unwrap();
    let opened = open(Box::new(backend));
    assert!(opened.presets.is_empty());
    assert!(opened.warnings.iter().any(|w| w.contains("unsupported version")));
}

#[test]
fn sync_and_reopen_roundtrip_user_preset_no_tips() {
    let backend = MemBackend::default();
    let opened = open(Box::new(backend.clone()));
    let mut store = opened.store;
    let preset = user_preset("My Preset", "My Group");
    store.sync(&[preset]);

    assert!(store.bytes() > 0);
    let files = backend.list().unwrap();
    assert!(files.iter().any(|(n, _)| n.ends_with(".pcbrushes")));

    let reopened = open(Box::new(backend));
    assert_eq!(reopened.presets.len(), 1);
    let p = &reopened.presets[0];
    assert_eq!(p.name, "My Preset");
    assert_eq!(p.group, "My Group");
    assert!(!p.builtin);
}

#[test]
fn sync_deletes_removed_group_and_reopen_is_empty() {
    let backend = MemBackend::default();
    let opened = open(Box::new(backend.clone()));
    let mut store = opened.store;
    store.sync(&[user_preset("P", "G")]);
    assert!(backend.list().unwrap().iter().any(|(n, _)| n.ends_with(".pcbrushes")));

    store.sync(&[]);
    let files = backend.list().unwrap();
    assert!(!files.iter().any(|(n, _)| n.ends_with(".pcbrushes")));

    let reopened = open(Box::new(backend));
    assert!(reopened.presets.is_empty());
}

#[test]
fn sync_with_sampled_tip_roundtrip_tip_file() {
    let backend = MemBackend::default();
    let opened = open(Box::new(backend.clone()));
    let mut store = opened.store;
    store.sync(&[sampled_preset("Sampled", "TipGroup")]);

    assert!(store.bytes() > 0);
    let files = backend.list().unwrap();
    assert!(files.iter().any(|(n, _)| n.starts_with(TIPS_DIR) && n.ends_with(".pctip")));

    let reopened = open(Box::new(backend));
    assert_eq!(reopened.presets.len(), 1);
    let p = &reopened.presets[0];
    assert!(matches!(&p.brush.tip, TipShape::Sampled(_)));
    if let TipShape::Sampled(tile) = &p.brush.tip {
        assert_eq!(tile.width, 2);
        assert_eq!(tile.height, 2);
        assert_eq!(tile.data, vec![0, 257, 514, 65535]);
    } else {
        panic!("expected sampled tip");
    }
}

#[test]
fn open_missing_tip_skips_preset_with_warning() {
    let backend = MemBackend::default();
    let opened = open(Box::new(backend.clone()));
    let mut store = opened.store;
    store.sync(&[sampled_preset("Sampled", "Tips")]);

    let tip_file = backend
        .list()
        .unwrap()
        .into_iter()
        .find_map(|(n, _)| if n.starts_with(TIPS_DIR) && n.ends_with(".pctip") { Some(n) } else { None })
        .expect("tip file should exist");
    backend.remove(&tip_file).unwrap();

    let reopened = open(Box::new(backend));
    assert!(reopened.presets.is_empty());
    assert!(reopened.warnings.iter().any(|w| w.contains("tip") && w.contains("skipped")));
}

#[test]
fn failing_backend_open_warns_and_no_panics() {
    let opened = open(Box::new(FailingBackend));
    assert!(opened.presets.is_empty());
    assert!(opened.warnings.iter().any(|w| w.contains("failing list")));
}
