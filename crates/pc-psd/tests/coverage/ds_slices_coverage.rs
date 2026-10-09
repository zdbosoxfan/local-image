use photocraft_psd::slices::{SLICES, SliceRecord, SlicesResource};

fn sample_with_outsets() -> SlicesResource {
    SlicesResource {
        version: 6,
        bounds: [0, 0, 80, 100],
        group_name: "site".into(),
        slices: vec![
            SliceRecord { id: 0, origin: 0, kind: 1, rect: [0, 0, 100, 10], ..Default::default() },
            SliceRecord {
                id: 1,
                group_id: 0,
                origin: 2,
                name: "logo".into(),
                kind: 1,
                rect: [20, 10, 60, 30],
                url: "https://example.org/".into(),
                target: "_blank".into(),
                message: "hi".into(),
                alt: "Logo".into(),
                cell_text_is_html: true,
                cell_text: "<b>x</b>".into(),
                horizontal_align: 1,
                vertical_align: 2,
                color: [255, 10, 20, 30],
                ..Default::default()
            },
            SliceRecord { id: 2, origin: 1, layer_id: Some(7), kind: 0, rect: [5, 40, 25, 70], outsets: [1, 2, 3, 4], ..Default::default() },
        ],
    }
}

fn sample_without_outsets() -> SlicesResource {
    let mut s = sample_with_outsets();
    for slice in s.slices.iter_mut() {
        slice.outsets = [0; 4];
    }
    s
}

fn sample_descriptor_clean() -> SlicesResource {
    SlicesResource {
        version: 7,
        bounds: [0, 0, 80, 100],
        group_name: "site".into(),
        slices: vec![
            SliceRecord { id: 0, origin: 0, kind: 1, rect: [0, 0, 100, 10], ..Default::default() },
            SliceRecord {
                id: 1,
                group_id: 0,
                origin: 2,
                name: "logo".into(),
                kind: 1,
                rect: [20, 10, 60, 30],
                url: "https://example.org/".into(),
                target: "_blank".into(),
                message: "hi".into(),
                alt: "Logo".into(),
                cell_text_is_html: true,
                cell_text: "<b>x</b>".into(),
                ..Default::default()
            },
            SliceRecord { id: 2, origin: 1, layer_id: Some(7), kind: 0, rect: [5, 40, 25, 70], outsets: [1, 2, 3, 4], ..Default::default() },
        ],
    }
}

#[test]
fn slices_resource_id_matches_spec() {
    assert_eq!(SLICES, 1050);
}

#[test]
fn default_to_bytes_writes_version_6_and_roundtrips() {
    let s = SlicesResource::default();
    let bytes = s.to_bytes();
    assert_eq!(&bytes[..4], &[0, 0, 0, 6]);
    let back = SlicesResource::from_bytes(&bytes).unwrap();
    assert_eq!(back.version, 6);
    assert_eq!(back.bounds, [0, 0, 0, 0]);
    assert_eq!(back.group_name, "");
    assert!(back.slices.is_empty());
}

#[test]
fn v6_roundtrip_with_outsets() {
    let s = sample_with_outsets();
    let bytes = s.to_bytes();
    let back = SlicesResource::from_bytes(&bytes).unwrap();
    assert_eq!(back, s);
    assert_eq!(back.to_bytes(), bytes, "byte-stable");
}

#[test]
fn v6_roundtrip_without_outsets() {
    let s = sample_without_outsets();
    let bytes = s.to_bytes();
    let back = SlicesResource::from_bytes(&bytes).unwrap();
    assert_eq!(back, s);
    assert_eq!(back.to_bytes(), bytes);
}

#[test]
fn v6_roundtrip_unicode_strings() {
    let s = SlicesResource {
        version: 6,
        bounds: [1, 2, 3, 4],
        group_name: "グループ".into(),
        slices: vec![SliceRecord { name: "café 😀".into(), ..Default::default() }],
    };
    let back = SlicesResource::from_bytes(&s.to_bytes()).unwrap();
    assert_eq!(back, s);
}

#[test]
fn v6_roundtrip_negative_bounds() {
    let s = SlicesResource {
        version: 6,
        bounds: [-10, 5, 100, 200],
        group_name: "neg".into(),
        slices: vec![SliceRecord { rect: [-1, -2, -3, -4], ..Default::default() }],
    };
    let back = SlicesResource::from_bytes(&s.to_bytes()).unwrap();
    assert_eq!(back, s);
}

#[test]
fn v6_roundtrip_empty_slices() {
    let s = SlicesResource { version: 6, bounds: [0, 0, 10, 10], group_name: "".into(), slices: vec![] };
    let back = SlicesResource::from_bytes(&s.to_bytes()).unwrap();
    assert_eq!(back, s);
}

#[test]
fn v6_roundtrip_many_slices() {
    let mut slices = Vec::new();
    for i in 0..5u32 {
        let origin = i % 3;
        slices.push(SliceRecord {
            id: i,
            group_id: i % 2,
            origin,
            // Ensure all origin=1 slices have a concrete layer_id so roundtrip succeeds
            layer_id: if origin == 1 { Some(7) } else { None },
            name: format!("slice{i}"),
            kind: i % 3,
            rect: [i as i32, 0, 10, 20],
            ..Default::default()
        });
    }
    let s = SlicesResource { version: 6, bounds: [0, 0, 100, 100], group_name: "many".into(), slices };
    let back = SlicesResource::from_bytes(&s.to_bytes()).unwrap();
    assert_eq!(back, s);
}

#[test]
fn v6_origin1_none_layer_id_roundtrips_as_zero() {
    // The v6 binary layout always carries a layer id for origin=1 slices, so `None` cannot be
    // represented; it is written as 0 and read back as Some(0). That is a format limit, not a bug.
    let s = SlicesResource {
        version: 6,
        bounds: [0, 0, 10, 10],
        group_name: "".into(),
        slices: vec![SliceRecord { id: 1, origin: 1, layer_id: None, ..Default::default() }],
    };
    let back = SlicesResource::from_bytes(&s.to_bytes()).unwrap();
    assert_eq!(back.slices[0].layer_id, Some(0));
}

#[test]
fn to_bytes_prepends_version_6() {
    let s = sample_with_outsets();
    let bytes = s.to_bytes();
    assert_eq!(&bytes[..4], &[0, 0, 0, 6]);
}

#[test]
fn to_descriptor_bytes_prepends_version_7() {
    let s = sample_with_outsets();
    let bytes = s.to_descriptor_bytes();
    assert_eq!(&bytes[..4], &[0, 0, 0, 7]);
}

#[test]
fn v7_descriptor_roundtrip() {
    let s = sample_descriptor_clean();
    let bytes = s.to_descriptor_bytes();
    let back = SlicesResource::from_bytes(&bytes).unwrap();
    assert_eq!(back, s);
}

#[test]
fn v8_descriptor_parses_version_8() {
    let s = sample_descriptor_clean();
    let mut bytes = s.to_descriptor_bytes();
    bytes[0..4].copy_from_slice(&[0, 0, 0, 8]);
    let back = SlicesResource::from_bytes(&bytes).unwrap();
    assert_eq!(back.version, 8);
    assert_eq!(back.bounds, s.bounds);
    assert_eq!(back.group_name, s.group_name);
    assert_eq!(back.slices, s.slices);
}

#[test]
fn from_bytes_unsupported_version_is_error() {
    let bytes = [0, 0, 0, 9];
    assert!(SlicesResource::from_bytes(&bytes).is_err());
}

#[test]
fn from_bytes_empty_input_is_error() {
    assert!(SlicesResource::from_bytes(&[]).is_err());
}

#[test]
fn from_bytes_truncated_various_lengths_return_err() {
    let required_bytes = sample_without_outsets().to_bytes();
    let cuts = [0, 1, 2, 3, 4, 5, 10, 20, 30, 40, 60, 80, 120, 160];
    for cut in cuts {
        if cut < required_bytes.len() {
            assert!(SlicesResource::from_bytes(&required_bytes[..cut]).is_err(), "cut at {cut} should be error");
        }
    }
    assert!(SlicesResource::from_bytes(&required_bytes).is_ok());
}

#[test]
fn from_bytes_random_bytes_never_panics() {
    let mut seed = 0x1234_5678u32;
    for len in 0..512usize {
        let mut data = vec![0u8; len];
        for b in data.iter_mut() {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            *b = (seed >> 24) as u8;
        }
        let _ = SlicesResource::from_bytes(&data);
    }
}

#[test]
fn to_bytes_is_deterministic() {
    let s = sample_with_outsets();
    assert_eq!(s.to_bytes(), s.to_bytes());
}

#[test]
fn to_descriptor_bytes_is_deterministic() {
    let s = sample_with_outsets();
    assert_eq!(s.to_descriptor_bytes(), s.to_descriptor_bytes());
}

#[test]
fn outsets_trailing_descriptor_only_when_present() {
    let with_outsets = sample_with_outsets();
    let without_outsets = sample_without_outsets();
    let bytes_with = with_outsets.to_bytes();
    let bytes_without = without_outsets.to_bytes();
    assert!(bytes_with.len() > bytes_without.len());
    let back_without = SlicesResource::from_bytes(&bytes_without).unwrap();
    assert_eq!(back_without, without_outsets);
    assert!(back_without.slices.iter().all(|s| s.outsets == [0; 4]));
}

#[test]
fn layer_id_only_written_when_origin_is_1() {
    let s = SlicesResource {
        version: 6,
        bounds: [0, 0, 10, 10],
        group_name: "".into(),
        slices: vec![
            SliceRecord { id: 1, origin: 0, layer_id: Some(42), ..Default::default() },
            SliceRecord { id: 2, origin: 1, layer_id: Some(43), ..Default::default() },
            SliceRecord { id: 3, origin: 2, layer_id: Some(44), ..Default::default() },
        ],
    };
    let back = SlicesResource::from_bytes(&s.to_bytes()).unwrap();
    assert_eq!(back.slices[0].layer_id, None);
    assert_eq!(back.slices[1].layer_id, Some(43));
    assert_eq!(back.slices[2].layer_id, None);
}

#[test]
fn from_bytes_ignores_trailing_data() {
    let s = sample_without_outsets();
    let mut bytes = s.to_bytes();
    bytes.extend_from_slice(&[9u8; 10]);
    let back = SlicesResource::from_bytes(&bytes).unwrap();
    assert_eq!(back, s);
}

#[test]
fn v7_descriptor_preserves_alignment_and_color() {
    let s = sample_with_outsets();
    let back = SlicesResource::from_bytes(&s.to_descriptor_bytes()).unwrap();
    let original_slice = &s.slices[1];
    let parsed_slice = &back.slices[1];
    assert_eq!(original_slice.horizontal_align, 1);
    assert_eq!(original_slice.vertical_align, 2);
    assert_eq!(original_slice.color, [255, 10, 20, 30]);
    assert_eq!(parsed_slice.horizontal_align, 1);
    assert_eq!(parsed_slice.vertical_align, 2);
    assert_eq!(parsed_slice.color, [255, 10, 20, 30]);
}
