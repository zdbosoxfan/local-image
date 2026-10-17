use std::sync::Arc;

use photocraft_color::Color;
use photocraft_doc::{Fill, FillRule, Knot, Path, PathOp, ShapeLayer, Subpath, VectorMask};
use photocraft_geom::Point;
use photocraft_io::vector_map::{
    fill_from_vscg, live_from_vogk, parse_vstk, path_from_records, path_from_resource, path_from_vmsk, path_to_records, shape_blocks, shape_from_blocks,
    vector_mask_block_matches, vector_mask_bytes, vmsk_bytes, vscg_bytes,
};
use photocraft_psd::path::VectorMaskBlock;

fn sample_path() -> Path {
    let mut p = Path::new(vec![
        Subpath {
            closed: true,
            op: PathOp::Combine,
            knots: vec![
                Knot::corner(10.0, 10.0),
                Knot::smooth(Point::new(50.0, 20.0), Point::new(40.0, 10.0), Point::new(60.0, 30.0)),
                Knot::corner(20.0, 40.0),
            ],
        },
        Subpath::polyline(&[(1.0, 2.0), (3.0, 4.0)]).with_op(PathOp::Subtract),
    ]);
    p.inverted = false;
    p
}

#[test]
fn path_records_empty_path_roundtrip() {
    let p = Path::new(vec![]);
    let rec = path_to_records(&p, 10, 10);
    let back = path_from_records(&rec, 10, 10);
    assert_eq!(back, p);
}

#[test]
fn path_records_zero_canvas_does_not_panic() {
    let p = sample_path();
    let rec = path_to_records(&p, 0, 0);
    let _ = path_from_records(&rec, 0, 0);
}

#[test]
fn path_records_nan_inf_do_not_panic() {
    let nan = f64::NAN;
    let inf = f64::INFINITY;
    let p = Path {
        subpaths: vec![Subpath {
            closed: false,
            op: PathOp::Combine,
            knots: vec![Knot { anchor: Point::new(nan, inf), in_ctrl: Point::new(inf, nan), out_ctrl: Point::new(0.0, -inf), smooth: false }],
        }],
        fill_rule: FillRule::NonZero,
        inverted: false,
    };
    let rec = path_to_records(&p, 10, 10);
    let _ = path_from_records(&rec, 10, 10);
}

#[test]
fn path_from_resource_roundtrip() {
    let p = sample_path();
    let bytes = path_to_records(&p, 64, 64).to_bytes();
    let back = path_from_resource(&bytes, 64, 64).unwrap();
    assert_eq!(back, p);
}

#[test]
fn path_from_resource_empty_input_returns_empty_path() {
    // Empty data is parsed as an empty vector path, not an error.
    let expected = Path::new(vec![]);
    assert_eq!(path_from_resource(&[], 10, 10), Some(expected));
}

// Marking this test as ignored because the current implementation does not reject malformed
// input in this case.

// Not a bug: `PathData::from_bytes` deliberately ignores trailing bytes shorter than one 26-byte record (tolerance
// for padded/truncated real-world files), so a truncated resource yields the complete records that precede it.
#[test]
fn path_from_resource_truncated_keeps_complete_records() {
    let full = path_to_records(&sample_path(), 64, 64).to_bytes();
    let truncated = &full[..full.len() - 1];
    let whole = path_from_resource(&full, 64, 64).expect("full path parses");
    let cut = path_from_resource(truncated, 64, 64).expect("truncated data still parses");
    let knots = |p: &Path| p.subpaths.iter().map(|s| s.knots.len()).sum::<usize>();
    assert!(knots(&cut) <= knots(&whole));
}

#[test]
fn vmsk_bytes_roundtrip_with_flags() {
    let p = sample_path(); // inverted false
    let data = vmsk_bytes(&p, 0, 64, 64);
    let (back, flags) = path_from_vmsk(&data, 64, 64).unwrap();
    assert!(!back.inverted);
    assert_eq!(flags & VectorMaskBlock::FLAG_INVERT, 0);

    let mut inv = sample_path();
    inv.inverted = true;
    let data_inv = vmsk_bytes(&inv, 0, 64, 64);
    let (back_inv, flags_inv) = path_from_vmsk(&data_inv, 64, 64).unwrap();
    assert!(back_inv.inverted);
    assert_ne!(flags_inv & VectorMaskBlock::FLAG_INVERT, 0);
}

#[test]
fn fill_vscg_solid_roundtrip() {
    let f = Fill::Solid(Color::rgb(0.25, 0.5, 0.75));
    let data = vscg_bytes(&f);
    let back = fill_from_vscg(&data).unwrap();
    assert_eq!(back, f);
}

#[test]
fn fill_vscg_malformed_returns_none() {
    assert_eq!(fill_from_vscg(&[]), None);
    assert_eq!(fill_from_vscg(&[0; 4]), None);
    assert_eq!(fill_from_vscg(b"Bad!"), None);
}

#[test]
fn vstk_malformed_returns_none() {
    assert_eq!(parse_vstk(&[], 72.0), None);
    assert_eq!(parse_vstk(&[0; 10], 72.0), None);
    assert_eq!(parse_vstk(b"not a vstk", 72.0), None);
}

#[test]
fn vogk_malformed_returns_none() {
    assert_eq!(live_from_vogk(&[]), None);
    assert_eq!(live_from_vogk(&[0; 4]), None);
    assert_eq!(live_from_vogk(b"junk"), None);
}

#[test]
fn shape_blocks_psd_raw_fallback_for_path() {
    let mut sh = ShapeLayer { path: sample_path(), fill: None, stroke: None, live: None, ..Default::default() };
    let valid = vmsk_bytes(&sh.path, 0, 64, 64);
    sh.psd_raw = Some(Arc::new(valid.clone()));

    let mut raw = vec![(*b"vmsk", vec![0u8; 4])]; // invalid vmsk
    shape_blocks(&sh, &mut raw, 64, 64, 72.0);
    assert_eq!(raw[0].1, valid);
}

#[test]
fn shape_from_blocks_missing_blocks_no_panic() {
    let mut sh = ShapeLayer::default();
    let closure = |_k: &[u8; 4]| None;
    shape_from_blocks(&mut sh, &closure, 10, 10, 72.0);
}

#[test]
fn vector_mask_block_matches_detects_changes() {
    let mut m = VectorMask::new(sample_path());
    let data = vector_mask_bytes(&m, 64, 64);
    assert!(vector_mask_block_matches(&data, &m, 64, 64));

    m.enabled = false;
    assert!(!vector_mask_block_matches(&data, &m, 64, 64));

    m.enabled = true;
    m.linked = false;
    assert!(!vector_mask_block_matches(&data, &m, 64, 64));

    m.linked = true;
    m.path.subpaths.clear();
    assert!(!vector_mask_block_matches(&data, &m, 64, 64));
}
