use photocraft_psd::{LayerSpec, PixelData, PsdBuilder, PsdFile};

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
