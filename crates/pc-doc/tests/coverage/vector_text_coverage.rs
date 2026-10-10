#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs::File;
use std::io::BufReader;
use std::path::{Path as FsPath, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use photocraft_doc::text::{
    AntiAlias, CharStyle, FontFeature, FontVariation, ParagraphRun, ParagraphStyle, TextAlign, TextDirection, TextRun, TextShape, TextWarp,
};
use photocraft_doc::vector::{FillRule, Knot, LineCap, LineJoin, LiveShape, Path, PathOp, ShapeStroke, StrokeAlign, Subpath, VectorMask};
use photocraft_doc::{Color, Fill, TextLayer};
use photocraft_geom::Point;

static NEXT_TMP: AtomicUsize = AtomicUsize::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let unique = NEXT_TMP.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("pc-doc-vector-text-coverage-{}-{}", std::process::id(), unique));
        std::fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }

    fn path(&self) -> &FsPath {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn knot_corner_has_retracted_handles() {
    let k = Knot::corner(1.0, 2.0);
    assert_eq!((k.anchor.x, k.anchor.y), (1.0, 2.0));
    assert_eq!(k.in_ctrl, k.anchor);
    assert_eq!(k.out_ctrl, k.anchor);
    assert!(!k.smooth);
}

#[test]
fn knot_smooth_explicit_handles_and_transform() {
    let k = Knot::smooth(Point::new(0.0, 0.0), Point::new(-1.0, 2.0), Point::new(3.0, -4.0));
    assert!(k.smooth);
    assert_eq!(k.in_ctrl, Point::new(-1.0, 2.0));
    assert_eq!(k.out_ctrl, Point::new(3.0, -4.0));

    let t = k.transform(&photocraft_doc::Affine::translate(1.0, 1.0));
    assert_eq!(t.anchor, Point::new(1.0, 1.0));
    assert_eq!(t.in_ctrl, Point::new(0.0, 3.0));
    assert_eq!(t.out_ctrl, Point::new(4.0, -3.0));
}

#[test]
fn subpath_polygon_segments_closed_vs_open() {
    let closed = Subpath::polygon(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)]);
    assert_eq!(closed.segments().len(), 3);

    let open = Subpath::polyline(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)]);
    assert_eq!(open.segments().len(), 2);
    assert_eq!(open.segments()[1][3], Point::new(10.0, 10.0));
}

#[test]
fn subpath_empty_has_no_segments() {
    let s = Subpath { closed: false, knots: vec![], op: PathOp::Combine };
    assert!(s.segments().is_empty());
}

#[test]
fn path_effective_op_first_is_combine() {
    let p = Path::new(vec![
        Subpath::polygon(&[(0.0, 0.0), (1.0, 1.0)]).with_op(PathOp::Intersect),
        Subpath::polygon(&[(2.0, 2.0), (3.0, 3.0)]).with_op(PathOp::Subtract),
    ]);
    assert_eq!(p.effective_op(0), PathOp::Combine);
    assert_eq!(p.effective_op(1), PathOp::Subtract);
}

#[test]
fn path_components_group_join_subpaths() {
    let p = Path::new(vec![
        Subpath::polygon(&[(0.0, 0.0), (1.0, 0.0)]).with_op(PathOp::Combine),
        Subpath::polygon(&[(0.5, 0.5), (0.75, 0.75)]).with_op(PathOp::Join),
        Subpath::polygon(&[(2.0, 2.0), (3.0, 3.0)]).with_op(PathOp::Subtract),
    ]);
    let components = p.components();
    assert_eq!(components, vec![0..2, 2..3]);
}

#[test]
fn path_transform_translates_control_bounds() {
    let p = Path::new(vec![Subpath::polygon(&[(1.0, 2.0), (5.0, 2.0), (5.0, 9.0)])]);
    let t = p.transform(&photocraft_doc::Affine::translate(1.0, 1.0));
    assert_eq!(t.control_bounds(), Some((2.0, 3.0, 6.0, 10.0)));
}

#[test]
fn control_bounds_empty_path_is_none() {
    let p = Path::new(vec![]);
    assert_eq!(p.control_bounds(), None);
}

#[test]
fn control_bounds_single_knot_equals_point() {
    let p = Path::new(vec![Subpath::polygon(&[(4.0, -3.0)])]);
    assert_eq!(p.control_bounds(), Some((4.0, -3.0, 4.0, -3.0)));
}

#[test]
fn control_bounds_with_nan_uses_finite_values() {
    let p = Path::new(vec![Subpath::polygon(&[(f64::NAN, 0.0), (3.0, 4.0)])]);
    let b = p.control_bounds().unwrap();
    assert_eq!(b, (3.0, 0.0, 3.0, 4.0));
}

#[test]
fn path_is_empty_only_when_all_subpaths_empty() {
    assert!(Path::new(vec![]).is_empty());
    assert!(Path::new(vec![Subpath { closed: false, knots: vec![], op: PathOp::Combine }]).is_empty());
    assert!(!Path::new(vec![Subpath { closed: false, knots: vec![], op: PathOp::Combine }, Subpath::polygon(&[(0.0, 0.0), (1.0, 1.0)]),]).is_empty());
}

#[test]
fn vector_mask_new_defaults() {
    let vm = VectorMask::new(Path::new(vec![Subpath::polygon(&[(0.0, 0.0), (1.0, 1.0)])]));
    assert!(vm.enabled);
    assert!(vm.linked);
    assert_eq!(vm.density, 1.0);
    assert_eq!(vm.feather, 0.0);
    assert_eq!(vm.path.subpaths.len(), 1);
}

#[test]
fn shape_stroke_default_values() {
    let s = ShapeStroke::default();
    assert_eq!(s.width, 3.0);
    assert_eq!(s.paint, Fill::Solid(Color::BLACK));
    assert_eq!(s.opacity, 1.0);
    assert_eq!(s.align, StrokeAlign::Center);
    assert_eq!(s.cap, LineCap::Butt);
    assert_eq!(s.join, LineJoin::Miter);
    assert_eq!(s.miter_limit, 100.0);
    assert!(s.dashes.is_empty());
    assert_eq!(s.dash_offset, 0.0);
}

#[test]
fn live_shape_serde_roundtrips_all_variants() {
    let shapes = vec![
        LiveShape::Rect { rect: [0.0, 0.0, 4.0, 5.0], radii: [0.5, 1.0, 1.5, 2.0] },
        LiveShape::Ellipse { rect: [1.0, 2.0, 3.0, 4.0] },
        LiveShape::Polygon { rect: [0.0, 0.0, 10.0, 10.0], sides: 6, star_ratio: 0.5 },
        LiveShape::Line { from: [0.0, 0.0], to: [2.0, 2.0], weight: 1.5 },
    ];

    for shape in shapes {
        let json = serde_json::to_string(&shape).unwrap();
        let back: LiveShape = serde_json::from_str(&json).unwrap();
        assert_eq!(back, shape);
    }
}

#[test]
fn path_and_shape_stroke_serde_roundtrip() {
    let p = Path { subpaths: vec![Subpath::polygon(&[(0.0, 0.0), (3.0, 4.0)]).with_op(PathOp::Exclude)], fill_rule: FillRule::EvenOdd, inverted: true };
    let stroke = ShapeStroke {
        width: 5.0,
        paint: Fill::Solid(Color::rgb(0.2, 0.4, 0.6)),
        opacity: 0.9,
        align: StrokeAlign::Outside,
        cap: LineCap::Round,
        join: LineJoin::Bevel,
        miter_limit: 4.0,
        dashes: vec![2.0, 1.0],
        dash_offset: 1.5,
    };

    let json = serde_json::to_string(&(&p, &stroke)).unwrap();
    let back: (Path, ShapeStroke) = serde_json::from_str(&json).unwrap();
    assert_eq!(back.0, p);
    assert_eq!(back.1, stroke);
}

#[test]
fn path_roundtrip_through_temp_file() {
    let dir = TempDir::new();
    let file_path = dir.path().join("path.json");
    let p = Path::new(vec![Subpath::polygon(&[(0.0, 0.0), (2.0, 0.0), (2.0, 2.0)]).with_op(PathOp::Subtract)]);

    serde_json::to_writer(File::create(&file_path).unwrap(), &p).unwrap();
    let back: Path = serde_json::from_reader(BufReader::new(File::open(&file_path).unwrap())).unwrap();
    assert_eq!(back, p);
}

#[test]
fn malformed_vector_json_returns_err() {
    let bad_paths = [r#"{"#, r#"{"subpaths":[{"closed":true}]}"#, r#"{"subpaths":[{"closed":true,"knots":[{"anchor":[1.0,2.0]}]}]}"#];
    for bad in bad_paths {
        let res: Result<Path, _> = serde_json::from_str(bad);
        assert!(res.is_err(), "unexpected success for Path: {bad}");
    }

    let bad_live: Result<LiveShape, _> = serde_json::from_str(r#"{"kind":"rect","rect":[0.0,0.0]}"#);
    assert!(bad_live.is_err());
}

#[test]
fn char_style_defaults() {
    let c = CharStyle::default();
    assert_eq!(c.size_pt, 12.0);
    assert_eq!(c.weight, 400);
    assert_eq!(c.tracking, 0.0);
    assert_eq!(c.leading_pt, None);
    assert_eq!(c.horizontal_scale, 1.0);
    assert_eq!(c.vertical_scale, 1.0);
    assert!(c.ligatures);
    assert!(!c.discretionary_ligatures);
    assert!(c.variations.is_empty());
    assert!(c.features.is_empty());
}

#[test]
fn paragraph_style_defaults() {
    let p = ParagraphStyle::default();
    assert_eq!(p.align, TextAlign::Left);
    assert_eq!(p.auto_leading, 1.2);
    assert_eq!(p.direction, TextDirection::Auto);
    assert!(!p.hyphenate);
    assert_eq!(p.first_line_indent_pt, 0.0);
}

#[test]
fn char_runs_without_runs_summary() {
    let t = TextLayer { text: "hello".into(), size_pt: 9.0, color: Color::rgb(1.0, 0.0, 0.0), ..Default::default() };
    let runs = t.char_runs();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].len, 5);
    assert_eq!(runs[0].style.size_pt, 9.0);
    assert_eq!(runs[0].style.color, Color::rgb(1.0, 0.0, 0.0));
}

#[test]
fn char_runs_multibyte_and_stretch() {
    let t = TextLayer {
        text: "héllo".into(),
        runs: vec![
            TextRun { len: 2, style: CharStyle { size_pt: 10.0, ..Default::default() } },
            TextRun { len: 1, style: CharStyle { size_pt: 20.0, ..Default::default() } },
        ],
        ..Default::default()
    };
    let runs = t.char_runs();
    assert_eq!(runs.iter().map(|r| r.len).collect::<Vec<_>>(), vec![3, 3]);
    assert_eq!(runs[0].style.size_pt, 10.0);
    assert_eq!(runs[1].style.size_pt, 20.0);
}

#[test]
fn char_runs_with_maximal_len_does_not_overflow() {
    let t = TextLayer {
        text: "ab".into(),
        runs: vec![TextRun { len: 1, style: CharStyle::default() }, TextRun { len: usize::MAX, style: CharStyle { size_pt: 24.0, ..Default::default() } }],
        ..Default::default()
    };
    let runs = t.char_runs();
    assert_eq!(runs.iter().map(|r| r.len).sum::<usize>(), t.text.len());
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[1].len, 1);
    assert_eq!(runs[1].style.size_pt, 24.0);
}

#[test]
fn paragraph_runs_normalized() {
    let t = TextLayer {
        text: "a\nb".into(),
        paragraphs: vec![
            ParagraphRun { len: 1, style: ParagraphStyle::default() },
            ParagraphRun { len: usize::MAX, style: ParagraphStyle { align: TextAlign::Center, ..Default::default() } },
        ],
        ..Default::default()
    };
    let runs = t.paragraph_runs();
    assert_eq!(runs.iter().map(|r| r.len).sum::<usize>(), 3);
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[1].len, 2);
    assert_eq!(runs[1].style.align, TextAlign::Center);
}

#[test]
fn sync_summary_follows_first_run() {
    let mut t = TextLayer {
        text: "ab".into(),
        runs: vec![TextRun { len: 2, style: CharStyle { font_family: "X".into(), size_pt: 7.0, color: Color::rgb(0.1, 0.2, 0.3), ..Default::default() } }],
        ..Default::default()
    };
    t.sync_summary();
    assert_eq!(t.font_family, "X");
    assert_eq!(t.size_pt, 7.0);
    assert_eq!(t.color, Color::rgb(0.1, 0.2, 0.3));
}

#[test]
fn text_shape_serde_roundtrip_and_default() {
    assert_eq!(TextShape::default(), TextShape::Point);

    let shapes = vec![TextShape::Point, TextShape::Box { x: 0.0, y: 1.0, width: 100.0, height: 200.0 }];
    for shape in shapes {
        let json = serde_json::to_string(&shape).unwrap();
        let back: TextShape = serde_json::from_str(&json).unwrap();
        assert_eq!(back, shape);
    }
}

#[test]
fn orientation_antialias_serde_roundtrip_all() {
    for orientation in [photocraft_doc::text::Orientation::Horizontal, photocraft_doc::text::Orientation::Vertical] {
        let json = serde_json::to_string(&orientation).unwrap();
        let back: photocraft_doc::text::Orientation = serde_json::from_str(&json).unwrap();
        assert_eq!(back, orientation);
    }

    for antialias in [AntiAlias::None, AntiAlias::Sharp, AntiAlias::Crisp, AntiAlias::Strong, AntiAlias::Smooth, AntiAlias::Windows, AntiAlias::WindowsLcd] {
        let json = serde_json::to_string(&antialias).unwrap();
        let back: AntiAlias = serde_json::from_str(&json).unwrap();
        assert_eq!(back, antialias);
    }
}

#[test]
fn text_align_justified_variants() {
    assert!(TextAlign::JustifyLeft.is_justified());
    assert!(TextAlign::JustifyCenter.is_justified());
    assert!(TextAlign::JustifyRight.is_justified());
    assert!(TextAlign::JustifyAll.is_justified());
    assert!(!TextAlign::Left.is_justified());
    assert!(!TextAlign::Center.is_justified());
    assert!(!TextAlign::Right.is_justified());
}

#[test]
fn text_warp_serde_roundtrip_and_missing_style_error() {
    let warp = TextWarp { style: "warpArc".into(), value: 15.0, horizontal_distortion: 10.0, vertical_distortion: -5.0, horizontal: true };
    let json = serde_json::to_string(&warp).unwrap();
    let back: TextWarp = serde_json::from_str(&json).unwrap();
    assert_eq!(back, warp);

    let missing_style: Result<TextWarp, _> = serde_json::from_str(r#"{"value":5}"#);
    assert!(missing_style.is_err());
}

#[test]
fn font_feature_variation_serde_roundtrip() {
    let features = vec![FontFeature { tag: "ss01".into(), value: 1 }];
    let variations = vec![FontVariation { axis: "wght".into(), value: 650.0 }];

    let feature_json = serde_json::to_string(&features).unwrap();
    let feature_back: Vec<FontFeature> = serde_json::from_str(&feature_json).unwrap();
    assert_eq!(feature_back, features);

    let variation_json = serde_json::to_string(&variations).unwrap();
    let variation_back: Vec<FontVariation> = serde_json::from_str(&variation_json).unwrap();
    assert_eq!(variation_back, variations);
}

#[test]
fn text_layer_default_conventions() {
    let t = TextLayer::default();
    assert_eq!(t.text, "");
    assert_eq!(t.font_family, "");
    assert_eq!(t.size_pt, 12.0);
    assert_eq!(t.color, Color::BLACK);
    assert_eq!(t.transform, photocraft_doc::Affine::IDENTITY);
    assert_eq!(t.shape, TextShape::Point);
    assert_eq!(t.orientation, photocraft_doc::text::Orientation::Horizontal);
    assert_eq!(t.antialias, AntiAlias::Smooth);
    assert!(t.warp.is_none());
    assert!(t.cache.is_none());
}

#[test]
fn malformed_text_json_returns_err() {
    for bad in [r#"{"#, r#"{"size_pt":"oops"}"#, r#"{"features":[{"tag":"ss01"}]}"#] {
        let res: Result<CharStyle, _> = serde_json::from_str(bad);
        assert!(res.is_err(), "unexpected success for CharStyle: {bad}");
    }

    let res: Result<TextRun, _> = serde_json::from_str(r#"{"len":1}"#);
    assert!(res.is_err());

    let res: Result<ParagraphRun, _> = serde_json::from_str(r#"{"len":1}"#);
    assert!(res.is_err());
}
