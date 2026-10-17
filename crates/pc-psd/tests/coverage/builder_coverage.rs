use photocraft_psd::{GroupSpec, LayerSpec, PixelData, PsdBuilder, PsdFile, Version};

#[test]
fn empty_dimensions_are_rejected() {
    let res = PsdBuilder::new(0, 0).to_bytes();
    assert!(res.is_err());
}

#[test]
fn nested_groups_round_trip() {
    let mut builder = PsdBuilder::new(2, 2);
    builder.begin_group(GroupSpec::new("outer"));
    builder.push_layer(LayerSpec::new("a", 0, 0, 2, 2, PixelData::Rgba8(vec![0_u8; 16])));
    builder.begin_group(GroupSpec::new("inner"));
    builder.push_layer(LayerSpec::new("b", 0, 0, 2, 2, PixelData::Rgba8(vec![0_u8; 16])));
    builder.end_group().unwrap();
    builder.end_group().unwrap();
    builder.composite(PixelData::Rgba8(vec![0_u8; 16]));

    let bytes = builder.to_bytes().unwrap();
    let file = PsdFile::from_bytes(&bytes).unwrap();

    assert_eq!(file.to_bytes().unwrap(), bytes);
}

#[test]
fn depth_mismatch_is_error() {
    let mut builder = PsdBuilder::new(1, 1).depth(16);
    builder.push_layer(LayerSpec::new("x", 0, 0, 1, 1, PixelData::Rgba8(vec![1, 2, 3, 255])));
    assert!(builder.to_bytes().is_err());
}

#[test]
fn resolution_special_floats_do_not_panic() {
    for dpi in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 0.0, f64::MAX] {
        let mut builder = PsdBuilder::new(1, 1);
        builder = builder.resolution(dpi);
        let res = builder.to_bytes();
        assert!(res.is_ok(), "dpi {dpi} should build");
    }
}

#[test]
fn dimension_boundaries() {
    let ok = PsdBuilder::new(300000, 1).version(Version::Psb).to_bytes();
    assert!(ok.is_ok());

    let bad = PsdBuilder::new(300001, 1).version(Version::Psb).to_bytes();
    assert!(bad.is_err());
}
