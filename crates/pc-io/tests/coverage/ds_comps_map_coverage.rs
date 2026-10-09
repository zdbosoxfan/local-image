use photocraft_io::comps_map;

#[test]
fn layer_comps_resource_id_is_correct() {
    assert_eq!(comps_map::LAYER_COMPS, 1065);
}

#[test]
fn artboard_key_order_is_preferred_first() {
    assert_eq!(comps_map::ARTBOARD_KEYS.len(), 3);
    assert_eq!(comps_map::ARTBOARD_KEYS[0], b"artb");
    assert_eq!(comps_map::ARTBOARD_KEYS[1], b"artd");
    assert_eq!(comps_map::ARTBOARD_KEYS[2], b"abdd");
}

#[test]
fn parse_artboard_returns_none_for_empty_input() {
    assert!(comps_map::parse_artboard(&[]).is_none());
}

#[test]
fn parse_artboard_returns_none_for_short_input() {
    for len in 0..8 {
        let data = vec![0u8; len];
        assert!(comps_map::parse_artboard(&data).is_none(), "len {len} should fail");
    }
}

#[test]
fn parse_artboard_does_not_panic_on_garbage() {
    // A few arbitrary byte patterns; the parser must return None or parse, never panic.
    let patterns: &[&[u8]] = &[&[0xFF; 64], &[0x00; 64], &[0xAA; 64], b"not a descriptor".as_slice(), &[0x01, 0x02, 0x03]];
    for p in patterns {
        let _ = comps_map::parse_artboard(p);
    }
}

#[test]
fn parse_artboard_rejects_odd_length_prefix() {
    // VersionedDescriptor prefix parsing likely fails on odd lengths that cannot be padded.
    let data = [0u8; 9];
    assert!(comps_map::parse_artboard(&data).is_none());
}
