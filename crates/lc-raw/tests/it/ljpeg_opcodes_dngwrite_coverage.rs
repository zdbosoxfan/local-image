use lightcraft_raw::{
    BlackLevel, Cfa, ColorData, Metadata, Orientation, RawData, RawError, RawFormat, RawImage, Rect, Rgb32f, decode as decode_raw,
    dngwrite::{DngCompression, DngWriteOptions, write_dng},
    ljpeg::{self, Huffman},
    opcodes::{self, Area, Opcode},
    probe,
};

// ---------------------------------------------------------------- helpers

fn raw_u16(w: usize, h: usize, cfa: Option<Cfa>, cpp: usize, data: Vec<u16>) -> RawImage {
    RawImage {
        format: RawFormat::Dng,
        width: w,
        height: h,
        cpp,
        data: RawData::U16(data),
        cfa,
        bits: 16,
        black: BlackLevel::uniform(0.0),
        white: vec![65535.0; cpp],
        active_area: Rect::new(0, 0, w, h),
        crop: Rect::new(0, 0, w, h),
        orientation: Orientation::default(),
        color: ColorData::default(),
        wb_multipliers: None,
        linearized: false,
        opcodes: Default::default(),
        metadata: Metadata::default(),
    }
}

fn raw_f32(w: usize, h: usize, data: Vec<f32>) -> RawImage {
    RawImage {
        format: RawFormat::Dng,
        width: w,
        height: h,
        cpp: 3,
        data: RawData::F32(data),
        cfa: None,
        bits: 32,
        black: BlackLevel::uniform(0.0),
        white: vec![1.0; 3],
        active_area: Rect::new(0, 0, w, h),
        crop: Rect::new(0, 0, w, h),
        orientation: Orientation::default(),
        color: ColorData::default(),
        wb_multipliers: None,
        linearized: false,
        opcodes: Default::default(),
        metadata: Metadata::default(),
    }
}

fn area_full(w: u32, h: u32) -> Area {
    Area { top: 0, left: 0, bottom: h, right: w, plane: 0, planes: 1, row_pitch: 1, col_pitch: 1 }
}

// ---------------------------------------------------------------- ljpeg

#[test]
fn ljpeg_roundtrip_all_predictors_components_precisions() {
    for pred in 1..=7u8 {
        for comps in 1..=4usize {
            for bits in [8u8, 12, 16] {
                let (w, h) = (13, 7);
                let data: Vec<u16> = (0..w * h * comps)
                    .map(|i| {
                        let smooth = ((i % 97) as u32 * 37) % (1 << bits);
                        let jitter = ((i as u32 * 7919) >> 3) % 64;
                        ((smooth + jitter) % (1 << bits)) as u16
                    })
                    .collect();
                let enc = ljpeg::encode(&data, w, h, comps, bits, pred, 0);
                let f = ljpeg::decode(&enc, usize::MAX).unwrap();
                assert_eq!((f.width, f.height, f.components, f.precision, f.predictor), (w, h, comps, bits, pred));
                assert_eq!(f.data, data, "pred {pred} comps {comps} bits {bits}");
            }
        }
    }
}

#[test]
fn ljpeg_roundtrip_extremes_and_restarts() {
    let data: Vec<u16> = (0..64).map(|i| if i % 2 == 0 { 0 } else { 65535 }).collect();
    for pred in 1..=7 {
        let enc = ljpeg::encode(&data, 8, 8, 1, 16, pred, 0);
        assert_eq!(ljpeg::decode(&enc, usize::MAX).unwrap().data, data);
    }

    let data: Vec<u16> = (0..20 * 9 * 2).map(|i| (i % 4096) as u16).collect();
    for rst in [1usize, 7, 20, 40] {
        let enc = ljpeg::encode(&data, 20, 9, 2, 12, 6, rst);
        assert_eq!(ljpeg::decode(&enc, usize::MAX).unwrap().data, data, "restart {rst}");
    }
}

#[test]
fn ljpeg_constant_image_single_symbol_table() {
    let data = vec![1234u16; 16 * 4];
    let enc = ljpeg::encode(&data, 16, 4, 1, 14, 1, 0);
    assert_eq!(ljpeg::decode(&enc, usize::MAX).unwrap().data, data);
}

#[test]
fn ljpeg_frame_info_ok_and_truncated() {
    let data: Vec<u16> = (0..64).map(|i| i as u16).collect();
    let enc = ljpeg::encode(&data, 8, 8, 1, 12, 1, 0);
    assert_eq!(ljpeg::frame_info(&enc).unwrap(), (8, 8, 1, 12));
    assert!(ljpeg::frame_info(&enc[..10]).is_err());
    assert!(ljpeg::frame_info(&[]).is_err());
}

#[test]
fn ljpeg_decode_truncated_prefixes_do_not_panic() {
    let data: Vec<u16> = (0..256).map(|i| (i % 1024) as u16).collect();
    let enc = ljpeg::encode(&data, 16, 16, 1, 10, 1, 0);
    for n in 0..=enc.len() {
        let _ = ljpeg::decode(&enc[..n], usize::MAX);
        let _ = ljpeg::frame_info(&enc[..n]);
    }
}

#[test]
fn ljpeg_decode_respects_max_samples() {
    let data = vec![100u16; 64];
    let enc = ljpeg::encode(&data, 8, 8, 1, 12, 1, 0);
    assert!(matches!(ljpeg::decode(&enc, 63), Err(RawError::Limit(_))));
}

#[test]
fn ljpeg_huffman_new_rejects_bad_tables() {
    assert!(Huffman::new(&[0; 16], &[]).is_err());
    assert!(Huffman::new(&[1; 16], &[1]).is_err());
}

#[test]
fn ljpeg_diff_value_categories() {
    assert_eq!(ljpeg::diff_value(&mut ljpeg::BitReader::new(&[]), 0), 0);
    assert_eq!(ljpeg::diff_value(&mut ljpeg::BitReader::new(&[]), 16), 32768);
    assert_eq!(ljpeg::diff_value(&mut ljpeg::BitReader::new(&[0x00]), 1), -1);
    assert_eq!(ljpeg::diff_value(&mut ljpeg::BitReader::new(&[0x80]), 1), 1);
}

// ---------------------------------------------------------------- opcodes

#[test]
fn opcodes_write_parse_roundtrip_all_variants() {
    let a = area_full(30, 20);
    let list = vec![
        Opcode::WarpRectilinear { planes: vec![[1.0, 0.01, -0.002, 0.0, 1e-4, -2e-4]; 3], center: [0.5, 0.49] },
        Opcode::WarpFisheye { planes: vec![[1.0, 0.1, 0.0, 0.0]], center: [0.5, 0.5] },
        Opcode::FixVignetteRadial { k: [0.1, 0.2, 0.0, 0.0, 0.0], center: [0.5, 0.5] },
        Opcode::FixBadPixelsConstant { constant: 0, bayer_phase: 1 },
        Opcode::FixBadPixelsList { bayer_phase: 0, points: vec![(3, 4)], rects: vec![[1, 1, 2, 2]] },
        Opcode::TrimBounds { top: 1, left: 1, bottom: 9, right: 9 },
        Opcode::MapTable { area: a, table: vec![0, 100, 65535] },
        Opcode::MapPolynomial { area: a, coefficients: vec![0.0, 1.0, -0.1] },
        Opcode::GainMap { area: a, points_v: 2, points_h: 1, spacing: [1.0, 1.0], origin: [0.0, 0.0], map_planes: 1, gains: vec![1.0, 1.5] },
        Opcode::DeltaPerRow { area: a, deltas: vec![0.1, 0.2] },
        Opcode::DeltaPerColumn { area: a, deltas: vec![0.3] },
        Opcode::ScalePerRow { area: a, scales: vec![1.1] },
        Opcode::ScalePerColumn { area: a, scales: vec![0.9, 1.0] },
        Opcode::Unknown { id: 77, flags: 1, params: vec![1, 2, 3] },
    ];
    assert_eq!(opcodes::parse_list(&opcodes::write_list(&list)), list);
}

#[test]
fn opcodes_unknown_roundtrip_preserved() {
    let unknown = Opcode::Unknown { id: 42, flags: 0x102, params: vec![9, 8, 7] };
    let blob = opcodes::write_list(std::slice::from_ref(&unknown));
    assert_eq!(opcodes::parse_list(&blob), vec![unknown]);
}

#[test]
fn opcodes_apply_map_table_scales() {
    // Use a full 65536-entry identity table so any normalised input maps back to
    // approximately the same value, exposing rounding and clamping behaviour.
    let area = area_full(2, 2);
    let mut table = vec![0u16; 65536];
    for (i, v) in table.iter_mut().enumerate() {
        *v = i as u16;
    }
    let list = vec![Opcode::MapTable { area, table }];
    let inputs = [0.0f32, 0.25, 0.5, 0.75];
    let mut buf = inputs.to_vec();
    opcodes::apply_list(&list, &mut buf, 2, 2, 1, None, 1.0);
    for (&got, &want) in buf.iter().zip(&inputs) {
        assert!((got - want).abs() < 1e-4, "got {got}, want {want}");
    }
}

#[test]
fn opcodes_apply_gain_map_delta_and_scale() {
    let area = area_full(2, 2);
    let list = vec![
        Opcode::GainMap { area, points_v: 1, points_h: 1, spacing: [1.0, 1.0], origin: [0.0, 0.0], map_planes: 1, gains: vec![2.0] },
        Opcode::DeltaPerRow { area, deltas: vec![0.1, 0.2] },
        Opcode::ScalePerRow { area, scales: vec![1.0, 0.5] },
    ];
    let mut buf = vec![1.0f32; 4];
    opcodes::apply_list(&list, &mut buf, 2, 2, 1, None, 1.0);
    assert!((buf[0] - 2.1).abs() < 1e-6);
    assert!((buf[1] - 2.1).abs() < 1e-6);
    assert!((buf[2] - 1.1).abs() < 1e-6);
    assert!((buf[3] - 1.1).abs() < 1e-6);
}

#[test]
fn opcodes_apply_bad_pixels_constant() {
    let cfa = Cfa::bayer("RGGB").unwrap();
    let list = vec![Opcode::FixBadPixelsConstant { constant: 7, bayer_phase: 0 }];
    let mut buf = vec![100.0f32; 36];
    buf[2 * 6 + 2] = 7.0;
    opcodes::apply_list(&list, &mut buf, 6, 6, 1, Some(&cfa), 65535.0);
    assert_eq!(buf[2 * 6 + 2], 100.0);
}

#[test]
fn opcodes_apply_bad_pixels_list() {
    let cfa = Cfa::bayer("RGGB").unwrap();
    let list = vec![Opcode::FixBadPixelsList { bayer_phase: 0, points: vec![(2, 2)], rects: vec![] }];
    let mut buf = vec![100.0f32; 36];
    buf[2 * 6 + 2] = 7.0;
    opcodes::apply_list(&list, &mut buf, 6, 6, 1, Some(&cfa), 65535.0);
    assert_eq!(buf[2 * 6 + 2], 100.0);
}

#[test]
fn opcodes_vignette_zero_and_warp_identity_constant() {
    let mut img = Rgb32f::filled(9, 7, [0.5; 3]);
    let vignette = Opcode::FixVignetteRadial { k: [0.0; 5], center: [0.5, 0.5] };
    let warp = Opcode::WarpRectilinear { planes: vec![[1.0, 0.0, 0.0, 0.0, 0.0, 0.0]; 3], center: [0.5, 0.5] };
    let list = vec![vignette, warp];
    opcodes::apply_list3(&list, &mut img);
    for px in &img.data {
        for c in px {
            assert!((*c - 0.5).abs() < 1e-6);
        }
    }
}

#[test]
fn opcodes_truncated_blob_never_panics() {
    let area = area_full(4, 3);
    let list = vec![
        Opcode::GainMap { area, points_v: 2, points_h: 2, spacing: [1.0, 1.0], origin: [0.0, 0.0], map_planes: 1, gains: vec![1.0, 2.0, 3.0, 4.0] },
        Opcode::MapTable { area, table: vec![0, 100, 65535] },
        Opcode::Unknown { id: 99, flags: 0, params: vec![1, 2, 3, 4] },
    ];
    let blob = opcodes::write_list(&list);
    for n in 0..=blob.len() {
        let parsed = opcodes::parse_list(&blob[..n]);
        let mut buf = vec![0.5f32; 12];
        opcodes::apply_list(&parsed, &mut buf, 4, 3, 1, None, 1.0);
    }
}

#[test]
fn opcodes_warp_barrel_moves_outward() {
    let img = Rgb32f::from_fn(31, 31, |x, y| if x == 24 && y == 15 { [1.0; 3] } else { [0.0; 3] });
    let out = opcodes::warp_rectilinear(&img, &[[1.0, -0.6, 0.0, 0.0, 0.0, 0.0]], [0.5, 0.5]);
    let (bx, _) = (0..31).map(|x| (x, out.get(x, 15)[0])).fold((0, 0.0), |a, b| if b.1 > a.1 { b } else { a });
    assert!(bx >= 25, "{bx}");
}

// ---------------------------------------------------------------- dngwrite

#[test]
fn dngwrite_u16_uncompressed_roundtrip() {
    let (w, h) = (6, 4);
    let pattern = "RGGB";
    let cfa = Cfa::bayer(pattern).unwrap();
    let data: Vec<u16> = (0..w * h).map(|i| (i % 4096) as u16).collect();
    let raw = raw_u16(w, h, Some(cfa.clone()), 1, data.clone());
    let opts = DngWriteOptions { compression: DngCompression::Uncompressed, ..Default::default() };
    let bytes = write_dng(&raw, &opts).unwrap();
    assert_eq!(probe(&bytes), Some(RawFormat::Dng));
    let decoded = decode_raw(&bytes).unwrap();
    assert_eq!(decoded.width, w);
    assert_eq!(decoded.height, h);
    assert_eq!(decoded.cpp, 1);
    assert_eq!(decoded.cfa, Some(cfa));
    match decoded.data {
        RawData::U16(v) => assert_eq!(v, data),
        _ => panic!("expected U16"),
    }
}

#[test]
fn dngwrite_u16_lj92_roundtrip() {
    let (w, h) = (8, 6);
    let pattern = "GRBG";
    let cfa = Cfa::bayer(pattern).unwrap();
    let data: Vec<u16> = (0..w * h).map(|i| ((i * 17) % 4096) as u16).collect();
    let raw = raw_u16(w, h, Some(cfa.clone()), 1, data.clone());
    let opts = DngWriteOptions { compression: DngCompression::Lj92 { tile: 4 }, ..Default::default() };
    let bytes = write_dng(&raw, &opts).unwrap();
    let decoded = decode_raw(&bytes).unwrap();
    assert_eq!(decoded.width, w);
    assert_eq!(decoded.height, h);
    assert_eq!(decoded.cpp, 1);
    assert_eq!(decoded.cfa, Some(cfa));
    match decoded.data {
        RawData::U16(v) => assert_eq!(v, data),
        _ => panic!("expected U16"),
    }
}

#[test]
fn dngwrite_f32_uncompressed_roundtrip() {
    let (w, h) = (4, 3);
    let data: Vec<f32> = (0..w * h * 3).map(|i| (i as f32) * 0.1).collect();
    let raw = raw_f32(w, h, data.clone());
    let opts = DngWriteOptions { compression: DngCompression::Uncompressed, ..Default::default() };
    let bytes = write_dng(&raw, &opts).unwrap();
    let decoded = decode_raw(&bytes).unwrap();
    assert_eq!(decoded.width, w);
    assert_eq!(decoded.height, h);
    assert_eq!(decoded.cpp, 3);
    assert!(decoded.cfa.is_none());
    match decoded.data {
        RawData::F32(v) => {
            assert_eq!(v.len(), data.len());
            for (a, b) in v.iter().zip(&data) {
                assert!((a - b).abs() < 1e-7, "{} vs {}", a, b);
            }
        }
        _ => panic!("expected F32"),
    }
}

#[test]
fn dngwrite_f32_deflate_roundtrip() {
    let (w, h) = (5, 4);
    let data: Vec<f32> = (0..w * h * 3).map(|i| ((i % 7) as f32) * 0.25).collect();
    let raw = raw_f32(w, h, data.clone());
    let opts = DngWriteOptions { compression: DngCompression::Deflate { tile: 16, half: false }, ..Default::default() };
    let bytes = write_dng(&raw, &opts).unwrap();
    let decoded = decode_raw(&bytes).unwrap();
    assert_eq!(decoded.width, w);
    assert_eq!(decoded.height, h);
    assert_eq!(decoded.cpp, 3);
    match decoded.data {
        RawData::F32(v) => {
            assert_eq!(v.len(), data.len());
            for (a, b) in v.iter().zip(&data) {
                assert!((a - b).abs() < 1e-7, "{} vs {}", a, b);
            }
        }
        _ => panic!("expected F32"),
    }
}

#[test]
fn dngwrite_f32_deflate_half_roundtrip() {
    let (w, h) = (4, 4);
    // values exactly representable in binary16
    let vals = [0.0f32, 0.5, 1.0, 2.0, -0.5, -1.0, -2.0, 4.0];
    let data: Vec<f32> = (0..w * h * 3).map(|i| vals[i % vals.len()]).collect();
    let raw = raw_f32(w, h, data.clone());
    let opts = DngWriteOptions { compression: DngCompression::Deflate { tile: 16, half: true }, ..Default::default() };
    let bytes = write_dng(&raw, &opts).unwrap();
    let decoded = decode_raw(&bytes).unwrap();
    match decoded.data {
        RawData::F32(v) => {
            assert_eq!(v.len(), data.len());
            for (a, b) in v.iter().zip(&data) {
                assert!((a - b).abs() < 1e-6, "{} vs {}", a, b);
            }
        }
        _ => panic!("expected F32"),
    }
}

#[test]
fn dngwrite_write_dng_validates_length() {
    let (w, h) = (4, 4);
    let data = vec![0u16; w * h + 1]; // wrong length
    let raw = raw_u16(w, h, Some(Cfa::bayer("RGGB").unwrap()), 1, data);
    assert!(write_dng(&raw, &DngWriteOptions::default()).is_err());
}

#[test]
fn dngwrite_lj92_tile_too_large() {
    let (w, h) = (2, 2);
    let data = vec![0u16; w * h];
    let raw = raw_u16(w, h, Some(Cfa::bayer("RGGB").unwrap()), 1, data);
    let opts = DngWriteOptions { compression: DngCompression::Lj92 { tile: 65536 }, ..Default::default() };
    assert!(matches!(write_dng(&raw, &opts), Err(RawError::Limit(_))));
}

#[test]
fn dngwrite_probe_dng() {
    let data = vec![100u16; 4 * 4];
    let raw = raw_u16(4, 4, Some(Cfa::bayer("RGGB").unwrap()), 1, data);
    let opts = DngWriteOptions { compression: DngCompression::Uncompressed, ..Default::default() };
    let bytes = write_dng(&raw, &opts).unwrap();
    assert_eq!(probe(&bytes), Some(RawFormat::Dng));
}

#[test]
fn dngwrite_preserves_black_white_and_cfa() {
    let (w, h) = (3, 3);
    let cfa = Cfa::bayer("BGGR").unwrap();
    let data = vec![500u16; w * h];
    let mut raw = raw_u16(w, h, Some(cfa.clone()), 1, data.clone());
    raw.black = BlackLevel::uniform(1.0);
    raw.white = vec![4095.0];
    let opts = DngWriteOptions { compression: DngCompression::Uncompressed, ..Default::default() };
    let bytes = write_dng(&raw, &opts).unwrap();
    let decoded = decode_raw(&bytes).unwrap();
    assert_eq!(decoded.cfa, Some(cfa));
    assert!((decoded.black.mean() - 1.0).abs() < 1e-6);
    assert!((decoded.white_at(0) - 4095.0).abs() < 1e-6);
    match decoded.data {
        RawData::U16(v) => assert_eq!(v, data),
        _ => panic!("expected U16"),
    }
}
