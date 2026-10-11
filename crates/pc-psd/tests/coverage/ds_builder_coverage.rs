use photocraft_psd::{BlendMode, ColorMode, Compression, GroupSpec, LayerSpec, MaskSpec, PixelData, PsdBuilder, PsdFile, Rect, Version};
use std::path::{Path, PathBuf};

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("photocraft_builder_{}_{}_{}", tag, std::process::id(), nanos));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn rgba8_2x2() -> Vec<u8> {
    vec![10, 20, 30, 255, 40, 50, 60, 255, 70, 80, 90, 255, 100, 110, 120, 255]
}

#[test]
fn empty_dimensions_are_rejected() {
    let res = PsdBuilder::new(0, 0).to_bytes();
    assert!(res.is_err());
}

#[test]
fn one_by_one_rgb_round_trip() {
    let pixels = PixelData::Rgba8(vec![10, 20, 30, 255]);
    let mut builder = PsdBuilder::new(1, 1);
    builder.push_layer(LayerSpec::new("px", 0, 0, 1, 1, pixels.clone()));
    builder.composite(pixels);

    let bytes = builder.to_bytes().unwrap();
    let file = PsdFile::from_bytes(&bytes).unwrap();

    assert_eq!(file.to_bytes().unwrap(), bytes);
    assert_eq!(file.layer(0).unwrap().name(), "px");
}

#[test]
fn odd_sized_rgb_two_layer_round_trip() {
    let w = 3usize;
    let h = 5usize;
    let n = w * h;
    let mut pixels = Vec::with_capacity(n * 4);
    for i in 0..n {
        pixels.extend_from_slice(&[i as u8, (255 - i) as u8, (i.wrapping_mul(3)) as u8, 255]);
    }

    let mut builder = PsdBuilder::new(w as u32, h as u32);
    builder.push_layer(LayerSpec::new("bottom", 0, 0, w as u32, h as u32, PixelData::Rgba8(pixels.clone())));
    builder.push_layer(LayerSpec::new("top", 0, 0, w as u32, h as u32, PixelData::Rgba8(pixels.clone())));
    builder.composite(PixelData::Rgba8(pixels));

    let bytes = builder.to_bytes().unwrap();
    let file = PsdFile::from_bytes(&bytes).unwrap();

    assert_eq!(file.to_bytes().unwrap(), bytes);
    assert!(file.layer(0).is_some());
    assert!(file.layer(1).is_some());
}

#[test]
fn grayscale_round_trip() {
    let pixels = PixelData::GrayA8(vec![0, 255, 64, 255, 128, 255, 255, 0]);
    let mut builder = PsdBuilder::new(2, 2).color_mode(ColorMode::Grayscale).depth(8);

    builder.push_layer(LayerSpec::new("gray", 0, 0, 2, 2, pixels.clone()));
    builder.composite(pixels);

    let bytes = builder.to_bytes().unwrap();
    let file = PsdFile::from_bytes(&bytes).unwrap();

    assert_eq!(file.to_bytes().unwrap(), bytes);
}

#[test]
fn cmyk_round_trip() {
    let pixels = PixelData::Cmyka8(vec![0, 255, 255, 255, 255, 100, 50, 60, 70, 255]);
    let mut builder = PsdBuilder::new(1, 2).color_mode(ColorMode::Cmyk).depth(8);

    builder.push_layer(LayerSpec::new("cmyk", 0, 0, 1, 2, pixels.clone()));
    builder.composite(pixels);

    let bytes = builder.to_bytes().unwrap();
    let file = PsdFile::from_bytes(&bytes).unwrap();

    assert_eq!(file.to_bytes().unwrap(), bytes);
}

#[test]
fn sixteen_bit_round_trip() {
    let mut samples = Vec::with_capacity(16);
    for i in 0..16u16 {
        if i % 4 == 3 {
            samples.push(65535);
        } else {
            samples.push(i * 1000);
        }
    }
    let pixels = PixelData::Rgba16(samples);

    let mut builder = PsdBuilder::new(2, 2).depth(16);
    builder.push_layer(LayerSpec::new("deep", 0, 0, 2, 2, pixels.clone()));
    builder.composite(pixels);

    let bytes = builder.to_bytes().unwrap();
    let file = PsdFile::from_bytes(&bytes).unwrap();

    assert_eq!(file.to_bytes().unwrap(), bytes);
}

#[test]
fn zip_compression_round_trip() {
    let pixels = PixelData::Rgba8(rgba8_2x2());
    let mut builder = PsdBuilder::new(2, 2).compression(Compression::Zip);

    builder.push_layer(LayerSpec::new("zip", 0, 0, 2, 2, pixels.clone()));
    builder.composite(pixels);

    let bytes = builder.to_bytes().unwrap();
    let file = PsdFile::from_bytes(&bytes).unwrap();

    assert_eq!(file.to_bytes().unwrap(), bytes);
}

#[test]
fn missing_composite_placeholder_round_trip() {
    let bytes = PsdBuilder::new(3, 2).to_bytes().unwrap();
    assert!(!bytes.is_empty());

    let file = PsdFile::from_bytes(&bytes).unwrap();
    assert_eq!(file.to_bytes().unwrap(), bytes);
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
fn unclosed_group_is_error() {
    let mut builder = PsdBuilder::new(1, 1);
    builder.begin_group(GroupSpec::new("g"));
    assert!(builder.to_bytes().is_err());
}

#[test]
fn end_group_without_begin_is_error() {
    let mut builder = PsdBuilder::new(1, 1);
    let res = builder.end_group();
    assert!(res.is_err());
}

#[test]
fn rgb_builder_rejects_gray_pixels() {
    let mut builder = PsdBuilder::new(1, 1);
    builder.push_layer(LayerSpec::new("g", 0, 0, 1, 1, PixelData::GrayA8(vec![0, 255])));
    assert!(builder.to_bytes().is_err());
}

#[test]
fn depth_mismatch_is_error() {
    let mut builder = PsdBuilder::new(1, 1).depth(16);
    builder.push_layer(LayerSpec::new("x", 0, 0, 1, 1, PixelData::Rgba8(vec![1, 2, 3, 255])));
    assert!(builder.to_bytes().is_err());
}

#[test]
fn short_pixel_buffer_is_error() {
    let mut builder = PsdBuilder::new(2, 2);
    builder.push_layer(LayerSpec::new("x", 0, 0, 2, 2, PixelData::Rgba8(vec![1_u8; 8])));
    assert!(builder.to_bytes().is_err());
}

#[test]
fn mask_buffer_size_mismatch_is_error() {
    let spec = LayerSpec {
        name: "mask".to_string(),
        left: 0,
        top: 0,
        width: 2,
        height: 2,
        pixels: PixelData::Rgba8(vec![1_u8; 16]),
        blend_mode: BlendMode::Normal,
        opacity: 255,
        fill_opacity: None,
        visible: true,
        clipping: false,
        mask: Some(MaskSpec { rect: Rect::from_xywh(0, 0, 2, 2), data: vec![0; 3], default_color: 0, disabled: false }),
        extra_blocks: Vec::new(),
    };

    let mut builder = PsdBuilder::new(2, 2);
    builder.push_layer(spec);
    assert!(builder.to_bytes().is_err());
}

#[test]
fn unsupported_color_mode_is_error() {
    let res = PsdBuilder::new(1, 1).color_mode(ColorMode::Bitmap).to_bytes();
    assert!(res.is_err());
}

#[test]
fn unsupported_depth_is_error() {
    let res = PsdBuilder::new(1, 1).depth(7).to_bytes();
    assert!(res.is_err());
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
fn deterministic_output() {
    let make = || {
        let mut builder = PsdBuilder::new(2, 2);
        let pixels = PixelData::Rgba8(rgba8_2x2());
        builder.push_layer(LayerSpec::new("d", 0, 0, 2, 2, pixels.clone()));
        builder.composite(pixels);
        builder.to_bytes().unwrap()
    };

    let a = make();
    let b = make();
    let c = make();

    assert_eq!(a, b);
    assert_eq!(a, c);
}

#[test]
fn dimension_boundaries() {
    let ok = PsdBuilder::new(300000, 1).version(Version::Psb).to_bytes();
    assert!(ok.is_ok());

    let bad = PsdBuilder::new(300001, 1).version(Version::Psb).to_bytes();
    assert!(bad.is_err());
}

#[test]
fn file_write_read_round_trip() {
    let dir = TempDir::new("file_roundtrip");
    let path = dir.path().join("roundtrip.psd");

    let pixels = PixelData::Rgba8(rgba8_2x2());
    let mut builder = PsdBuilder::new(2, 2);
    builder.push_layer(LayerSpec::new("wr", 0, 0, 2, 2, pixels.clone()));
    builder.composite(pixels);
    let bytes = builder.to_bytes().unwrap();

    std::fs::write(&path, &bytes).unwrap();
    let read = std::fs::read(&path).unwrap();

    assert_eq!(read, bytes);

    let file = PsdFile::from_bytes(&read).unwrap();
    assert_eq!(file.to_bytes().unwrap(), bytes);
}

#[test]
fn parser_rejects_malformed_input() {
    for input in [&b""[..], &b"not a psd"[..], &[0_u8, 0, 0, 0][..]] {
        assert!(PsdFile::from_bytes(input).is_err());
    }
}
