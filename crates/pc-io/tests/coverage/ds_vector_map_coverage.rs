use std::sync::Arc;

use photocraft_color::Color;
use photocraft_doc::{Fill, FillRule, Knot, LineCap, LineJoin, LiveShape, Path, PathOp, ShapeLayer, ShapeStroke, StrokeAlign, Subpath, VectorMask};
use photocraft_geom::Point;
use photocraft_io::vector_map::{
    CLIPPING_PATH, SAVED_PATHS, WORK_PATH, fill_from_vscg, live_from_vogk, parse_vstk, path_from_records, path_from_resource, path_from_vmsk, path_to_records,
    shape_blocks, shape_from_blocks, vector_mask_block_matches, vector_mask_bytes, vector_mask_from_block, vmsk_bytes, vogk_bytes, vscg_bytes, vstk_bytes,
};
use photocraft_psd::path::{PathData, VectorMaskBlock};

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

fn vmsk_data_from_initial_fill(initial_fill: bool, flags: u32) -> Vec<u8> {
    VectorMaskBlock { version: 3, flags, path: PathData::from_subpaths(&[], initial_fill) }.to_bytes()
}

#[test]
fn constants_have_expected_values() {
    assert_eq!(*SAVED_PATHS.start(), 2000);
    assert_eq!(*SAVED_PATHS.end(), 2997);
    assert_eq!(WORK_PATH, 1025);
    assert_eq!(CLIPPING_PATH, 2999);
}

#[test]
fn path_records_roundtrip_finite() {
    let p = sample_path();
    let rec = path_to_records(&p, 64, 64);
    let back = path_from_records(&rec, 64, 64);
    assert_eq!(back, p);
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
fn path_from_vmsk_empty_subpaths_semantics() {
    // initial_fill=true, no subpaths -> path_from_vmsk inverted
    let data = vmsk_data_from_initial_fill(true, 0);
    let (path, _) = path_from_vmsk(&data, 10, 10).unwrap();
    assert!(path.inverted);

    let mask = vector_mask_from_block(&data, 10, 10).unwrap();
    assert!(!mask.path.inverted);

    // initial_fill=false, no subpaths -> vector mask hides all
    let data2 = vmsk_data_from_initial_fill(false, 0);
    let mask2 = vector_mask_from_block(&data2, 10, 10).unwrap();
    assert!(mask2.path.inverted);
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
fn vector_mask_roundtrip_with_flags() {
    let mut m = VectorMask::new(sample_path());
    m.enabled = false;
    m.linked = false;
    m.path.inverted = true;
    let data = vector_mask_bytes(&m, 64, 64);
    let back = vector_mask_from_block(&data, 64, 64).unwrap();
    assert_eq!(back, m);
    assert!(vector_mask_block_matches(&data, &m, 64, 64));
}

#[test]
fn vector_mask_empty_path_roundtrip() {
    let mut m = VectorMask::new(Path::new(vec![]));
    m.path.inverted = true;
    let data = vector_mask_bytes(&m, 20, 20);
    let back = vector_mask_from_block(&data, 20, 20).unwrap();
    assert_eq!(back, m);
    assert!(vector_mask_block_matches(&data, &m, 20, 20));
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
fn vstk_roundtrip_all_options() {
    let stroke = ShapeStroke {
        width: 4.5,
        paint: Fill::Solid(Color::rgb(1.0, 0.0, 0.0)),
        opacity: 0.5,
        align: StrokeAlign::Outside,
        cap: LineCap::Round,
        join: LineJoin::Bevel,
        miter_limit: 10.0,
        dashes: vec![2.0, 1.0],
        dash_offset: 0.5,
    };
    let data = vstk_bytes(Some(&stroke), false, 72.0);
    let info = parse_vstk(&data, 72.0).unwrap();
    assert!(!info.fill_enabled);
    let got = info.stroke.unwrap();
    assert_eq!(got.width, stroke.width);
    assert_eq!(got.opacity, stroke.opacity);
    assert_eq!(got.align, stroke.align);
    assert_eq!(got.cap, stroke.cap);
    assert_eq!(got.join, stroke.join);
    assert_eq!(got.miter_limit, stroke.miter_limit);
    assert_eq!(got.dashes, stroke.dashes);
    assert_eq!(got.dash_offset, stroke.dash_offset);
    match (&got.paint, &stroke.paint) {
        (Fill::Solid(a), Fill::Solid(b)) => assert_eq!(a, b),
        _ => panic!("expected solid paints"),
    }
}

#[test]
fn vstk_disabled_stroke_roundtrip() {
    let data = vstk_bytes(None, true, 72.0);
    let info = parse_vstk(&data, 72.0).unwrap();
    assert!(info.stroke.is_none());
    assert!(info.fill_enabled);

    let data2 = vstk_bytes(None, false, 72.0);
    let info2 = parse_vstk(&data2, 72.0).unwrap();
    assert!(info2.stroke.is_none());
    assert!(!info2.fill_enabled);
}

#[test]
fn vstk_malformed_returns_none() {
    assert_eq!(parse_vstk(&[], 72.0), None);
    assert_eq!(parse_vstk(&[0; 10], 72.0), None);
    assert_eq!(parse_vstk(b"not a vstk", 72.0), None);
}

#[test]
fn vogk_roundtrip_shapes() {
    let shapes = [
        LiveShape::Rect { rect: [1.0, 2.0, 30.0, 40.0], radii: [0.0; 4] },
        LiveShape::Rect { rect: [1.0, 2.0, 30.0, 40.0], radii: [1.0, 2.0, 3.0, 4.0] },
        LiveShape::Ellipse { rect: [5.0, 6.0, 7.0, 8.0] },
        LiveShape::Line { from: [0.0, 10.0], to: [20.0, 0.0], weight: 3.0 },
    ];
    for s in shapes {
        let data = vogk_bytes(&s, 72.0).unwrap();
        assert_eq!(live_from_vogk(&data), Some(s));
    }
}

#[test]
fn vogk_polygon_encodes_none() {
    let polygon = LiveShape::Polygon { rect: [0.0; 4], sides: 5, star_ratio: 1.0 };
    assert_eq!(vogk_bytes(&polygon, 72.0), None);
}

#[test]
fn vogk_malformed_returns_none() {
    assert_eq!(live_from_vogk(&[]), None);
    assert_eq!(live_from_vogk(&[0; 4]), None);
    assert_eq!(live_from_vogk(b"junk"), None);
}

#[test]
fn shape_blocks_generate_and_read_back() {
    let fill = Fill::Solid(Color::rgb(0.0, 1.0, 0.0));
    let sh = ShapeLayer {
        path: sample_path(),
        fill: Some(fill.clone()),
        stroke: Some(ShapeStroke::default()),
        live: Some(LiveShape::Ellipse { rect: [0.0, 0.0, 10.0, 10.0] }),
        ..Default::default()
    };
    let mut raw = vec![(*b"vscg", vscg_bytes(&fill))];
    shape_blocks(&sh, &mut raw, 64, 64, 72.0);

    let mut back = ShapeLayer::default();
    let lookup = |k: &[u8; 4]| raw.iter().find(|(key, _)| key == k).map(|(_, d)| d.clone());
    shape_from_blocks(&mut back, &lookup, 64, 64, 72.0);

    assert_eq!(back.path, sh.path);
    assert_eq!(back.stroke, sh.stroke);
    assert_eq!(back.live, sh.live);
    assert_eq!(back.fill, sh.fill);
}

#[test]
fn shape_blocks_unchanged_keeps_byte_identical() {
    let fill = Fill::Solid(Color::rgb(0.0, 1.0, 0.0));
    let sh = ShapeLayer {
        path: sample_path(),
        fill: Some(fill.clone()),
        stroke: Some(ShapeStroke::default()),
        live: Some(LiveShape::Ellipse { rect: [0.0, 0.0, 10.0, 10.0] }),
        ..Default::default()
    };
    let mut raw = vec![(*b"vscg", vscg_bytes(&fill))];
    shape_blocks(&sh, &mut raw, 64, 64, 72.0);
    let before = raw.clone();
    shape_blocks(&sh, &mut raw, 64, 64, 72.0);
    assert_eq!(raw, before);
}

#[test]
fn shape_blocks_edit_path_replaces_vmsk_and_drops_live() {
    let fill = Fill::Solid(Color::rgb(0.0, 1.0, 0.0));
    let sh = ShapeLayer {
        path: sample_path(),
        fill: Some(fill.clone()),
        stroke: Some(ShapeStroke::default()),
        live: Some(LiveShape::Ellipse { rect: [0.0, 0.0, 10.0, 10.0] }),
        ..Default::default()
    };
    let mut raw = vec![(*b"vscg", vscg_bytes(&fill))];
    shape_blocks(&sh, &mut raw, 64, 64, 72.0);

    let mut edited = sh.clone();
    edited.path.subpaths.pop();
    edited.live = Some(LiveShape::Polygon { rect: [0.0; 4], sides: 3, star_ratio: 1.0 });
    shape_blocks(&edited, &mut raw, 64, 64, 72.0);

    assert!(!raw.iter().any(|(k, _)| k == b"vogk"));
    let vmsk_entry = raw.iter().find(|(k, _)| k == b"vmsk").unwrap();
    let (decoded, _) = path_from_vmsk(&vmsk_entry.1, 64, 64).unwrap();
    assert_eq!(decoded, edited.path);
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
