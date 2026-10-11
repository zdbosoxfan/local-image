#![allow(clippy::unwrap_used)]
use super::*;
#[test]
fn colour39_ase_recorded_rgb_gray_lab_cmyk_fixture() {
    let bytes = include_bytes!("fixture.ase");
    let result = read(bytes).unwrap();
    assert!(result.warnings.is_empty());
    assert_eq!(result.palette.swatches[0], Swatch { name: "Brand".into(), color: Color::rgb(0.25, 0.5, 0.75), global: true, spot: false });
    assert_eq!(result.palette.groups[0].name, "Café 🎨");
    assert_eq!(result.palette.groups[0].swatches[0].color, Color { mode: ColorMode::Cmyk, c: [0.25, 0.5, 0.75, 1.], alpha: 1. });
    assert!(result.palette.groups[0].swatches[0].spot);
    assert_eq!(result.palette.groups[0].swatches[1].color, Color::gray(0.75));
    assert_eq!(result.palette.groups[0].swatches[2].color, Color { mode: ColorMode::Lab, c: [0.5, 148. / 255., 98. / 255., 0.], alpha: 1. });
    assert_eq!(write(&result.palette).unwrap(), bytes);
    for n in 0..bytes.len() {
        assert!(read(&bytes[..n]).is_err(), "truncated at {n}");
    }
}
#[test]
fn colour39_ase_writer_stability_and_hostile_blocks() {
    let p = Palette {
        swatches: vec![Swatch { name: "Spot".into(), color: Color::rgb(0.125, 0.25, 0.5), global: true, spot: true }],
        groups: vec![Group { name: "Empty".into(), swatches: vec![] }],
    };
    let bytes = write(&p).unwrap();
    assert_eq!(read(&bytes).unwrap().palette, p);
    assert_eq!(write(&read(&bytes).unwrap().palette).unwrap(), bytes);
    let mut bad = bytes.clone();
    bad[4] = 0xff;
    assert!(read(&bad).is_err());
    let mut bad = bytes.clone();
    bad[8..12].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(read(&bad).is_err());
    let mut bad = bytes.clone();
    bad[14..18].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(read(&bad).is_err());
    let mut bad = bytes.clone();
    bad.extend([0]);
    assert!(read(&bad).is_err());
    let mut p = p;
    p.swatches[0].color.c[0] = f32::NAN;
    assert!(write(&p).is_err());
    p.swatches[0].color = Color::rgba(1., 0., 0., 0.5);
    assert!(write(&p).is_err());
    p.swatches[0].color = Color::BLACK;
    p.swatches[0].name = "x\0y".into();
    assert!(write(&p).is_err());
}
#[test]
fn colour39_ase_unknown_model_and_block_are_warnings() {
    let mut bytes = b"ASEF\0\x01\0\0\0\0\0\x02".to_vec();
    bytes.extend([0, 42, 0, 0, 0, 1, 99]);
    bytes.extend([0, 1, 0, 0, 0, 8, 0, 1, 0, 0, b'H', b'S', b'B', b' ']);
    let result = read(&bytes).unwrap();
    assert!(result.palette.swatches.is_empty());
    assert_eq!(result.warnings.len(), 2);
}
