use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use photocraft_psd::{ColorMode, GlobalLayerMask, LayerInfoPlacement, LayerSpec, PixelData, PsdBuilder, PsdFile};

struct Cleanup(PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn pattern_rgba(w: u32, h: u32) -> Vec<u8> {
    let n = (w * h) as usize;
    let mut v = Vec::with_capacity(n * 4);
    for i in 0..n {
        let p = i as u8;
        v.extend_from_slice(&[p, p.wrapping_add(1), p.wrapping_add(2), p.wrapping_add(3)]);
    }
    v
}

fn planar_rgba(interleaved: &[u8]) -> Vec<u8> {
    let px = interleaved.len() / 4;
    let mut out = Vec::with_capacity(interleaved.len());
    for c in 0..4 {
        for p in 0..px {
            out.push(interleaved[p * 4 + c]);
        }
    }
    out
}

fn build_rgba_psd(width: u32, height: u32, interleaved: Vec<u8>) -> Vec<u8> {
    assert_eq!(interleaved.len(), (width * height * 4) as usize);
    let mut b = PsdBuilder::new(width, height);
    let _ = b.push_layer(LayerSpec::new("test", 0, 0, width, height, PixelData::Rgba8(interleaved.clone())));
    b.composite(PixelData::Rgba8(interleaved));
    match b.to_bytes() {
        Ok(bytes) => bytes,
        Err(e) => panic!("builder to_bytes failed: {e:?}"),
    }
}

fn parse(bytes: &[u8]) -> PsdFile {
    match PsdFile::from_bytes(bytes) {
        Ok(file) => file,
        Err(e) => panic!("from_bytes failed: {e:?}"),
    }
}

fn serialize(file: &PsdFile) -> Vec<u8> {
    match file.to_bytes() {
        Ok(bytes) => bytes,
        Err(e) => panic!("to_bytes failed: {e:?}"),
    }
}

#[test]
fn from_bytes_empty_and_short_inputs_error() {
    assert!(PsdFile::from_bytes(&[]).is_err());
    for len in 0..32 {
        let data = vec![0u8; len];
        assert!(PsdFile::from_bytes(&data).is_err(), "zeros len={len} unexpectedly parsed");
    }
    for len in 0..16 {
        let data = vec![0xFFu8; len];
        assert!(PsdFile::from_bytes(&data).is_err(), "ff len={len} unexpectedly parsed");
    }
}

#[test]
fn from_bytes_never_panics_on_garbage() {
    let mut state = 0x9E37_79B9u32;
    for len in 0..=255 {
        let mut data = vec![0u8; len];
        for byte in &mut data {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            *byte = (state >> 24) as u8;
        }
        let _ = PsdFile::from_bytes(&data);
    }
}

#[test]
fn builder_psd_roundtrips_byte_for_byte() {
    let bytes = build_rgba_psd(2, 1, vec![1, 2, 3, 4, 5, 6, 7, 8]);
    let file = parse(&bytes);
    let out = serialize(&file);
    assert_eq!(out, bytes);
}

#[test]
fn parsed_model_roundtrips_through_to_bytes() {
    let bytes = build_rgba_psd(3, 2, pattern_rgba(3, 2));
    let file = parse(&bytes);
    let out = serialize(&file);
    let file2 = parse(&out);
    assert_eq!(file2, file);
}

#[test]
fn layers_reports_builder_layer() {
    let bytes = build_rgba_psd(2, 2, pattern_rgba(2, 2));
    let file = parse(&bytes);
    assert_eq!(file.layers().len(), 1);
}

#[test]
fn decode_merged_planar_rgba_2x2() {
    let input = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
    let bytes = build_rgba_psd(2, 2, input);
    let file = parse(&bytes);
    let decoded = match file.decode_merged() {
        Ok(d) => d,
        Err(e) => panic!("decode_merged failed: {e:?}"),
    };
    let expected = vec![
        1, 5, 9, 13, // R plane
        2, 6, 10, 14, // G plane
        3, 7, 11, 15, // B plane
        4, 8, 12, 16, // A plane
    ];
    assert_eq!(decoded, expected);
}

#[test]
fn decode_merged_planar_rgba_odd_size() {
    let input = pattern_rgba(3, 1);
    let bytes = build_rgba_psd(3, 1, input.clone());
    let file = parse(&bytes);
    let decoded = match file.decode_merged() {
        Ok(d) => d,
        Err(e) => panic!("decode_merged failed: {e:?}"),
    };
    assert_eq!(decoded, planar_rgba(&input));
}

#[test]
fn merged_has_alpha_true_for_rgba_builder_file() {
    let bytes = build_rgba_psd(1, 1, vec![1, 2, 3, 4]);
    let file = parse(&bytes);
    assert!(file.merged_has_alpha());
}

#[test]
fn validate_accepts_builder_file() {
    let bytes = build_rgba_psd(1, 1, vec![1, 2, 3, 4]);
    let file = parse(&bytes);
    assert!(file.validate().is_ok());
}

#[test]
fn validate_rejects_indexed_bad_palette() {
    let bytes = build_rgba_psd(1, 1, vec![1, 2, 3, 4]);
    let mut file = parse(&bytes);
    file.header.color_mode = ColorMode::Indexed;
    file.color_mode_data = vec![0u8; 10];
    assert!(file.validate().is_err());
}

#[test]
fn global_layer_mask_accessors() {
    let mask = GlobalLayerMask {
        data: vec![
            0, 1, // overlay color space = 1
            0, 2, // component 1 = 2
            0, 3, // component 2 = 3
            0, 4, // component 3 = 4
            0, 50, // component 4 = 50
            0, 100, // opacity = 100
            128, // kind = 128
        ],
    };
    assert_eq!(mask.overlay_color_space(), Some(1));
    assert_eq!(mask.color_components(), Some([2, 3, 4, 50]));
    assert_eq!(mask.opacity(), Some(100));
    assert_eq!(mask.kind(), Some(128));
}

#[test]
fn global_layer_mask_empty_returns_none() {
    let mask = GlobalLayerMask { data: vec![] };
    assert_eq!(mask.overlay_color_space(), None);
    assert_eq!(mask.color_components(), None);
    assert_eq!(mask.opacity(), None);
    assert_eq!(mask.kind(), None);
}

#[test]
fn layers_mut_creates_layer_info_if_none() {
    let bytes = build_rgba_psd(1, 1, vec![1, 2, 3, 4]);
    let mut file = parse(&bytes);
    file.layer_info = None;
    assert!(file.layer_info.is_none());
    let layers = file.layers_mut();
    assert!(layers.is_empty());
    assert!(file.layer_info.is_some());
}

#[test]
fn resource_accessors_return_none_when_resources_cleared() {
    let bytes = build_rgba_psd(1, 1, vec![1, 2, 3, 4]);
    let mut file = parse(&bytes);
    file.resources.clear();
    assert!(file.resource(1039).is_none());
    assert!(file.icc_profile().is_none());
    assert!(file.resolution().is_none());
    assert!(file.has_real_merged_data().is_none());
}

#[test]
fn global_block_accessor_searches_by_key() {
    let bytes = build_rgba_psd(1, 1, vec![1, 2, 3, 4]);
    let mut file = parse(&bytes);
    file.global_blocks.clear();
    assert!(file.global_block(b"Lr16").is_none());
}

#[test]
fn from_bytes_rejects_truncated_builder_output() {
    let bytes = build_rgba_psd(2, 2, pattern_rgba(2, 2));
    let cut = &bytes[..bytes.len() / 2];
    assert!(PsdFile::from_bytes(cut).is_err());
}

#[test]
fn save_open_roundtrip_tempfile() {
    let bytes = build_rgba_psd(2, 2, pattern_rgba(2, 2));
    let file = parse(&bytes);

    let nanos = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_nanos(),
        Err(_) => 0,
    };
    let dir = std::env::temp_dir().join(format!("photocraft_psd_file_test_{}_{}", std::process::id(), nanos));
    std::fs::create_dir_all(&dir).expect("failed to create temp dir");
    let _cleanup = Cleanup(dir.clone());

    let path = dir.join("test.psd");
    match file.save(&path) {
        Ok(()) => {}
        Err(e) => panic!("save failed: {e:?}"),
    }

    match PsdFile::open(&path) {
        Ok(Ok(parsed)) => {
            let out = serialize(&parsed);
            assert_eq!(out, bytes);
        }
        Ok(Err(e)) => panic!("parse after open failed: {e:?}"),
        Err(e) => panic!("open failed: {e:?}"),
    }
}

#[test]
fn layer_info_placement_defaults_to_section() {
    let bytes = build_rgba_psd(1, 1, vec![1, 2, 3, 4]);
    let file = parse(&bytes);
    assert_eq!(file.layer_info_placement, LayerInfoPlacement::Section);
}
