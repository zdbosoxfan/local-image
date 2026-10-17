//! Compiles the `on_path` source directly rather than through the public module.
//! Independent analytic curves avoid sharing the V1 sampler.
pub use photocraft_text::{TextEngine, layout, raster, render};
#[path = "../../src/on_path.rs"]
#[allow(dead_code)]
mod on_path;
use on_path::{Curve, place};
use photocraft_color::PixelFormat;
use photocraft_doc::{
    Path, TextLayer,
    text::{CharStyle, Orientation, TextAlign, TextRun},
    vector_design::text::{PathText, PathTextAlign},
};
use photocraft_geom::{Affine, Point, Rect};
struct Line {
    length: f64,
}
impl Curve for Line {
    fn length(&self) -> f64 {
        self.length
    }
    fn closed(&self) -> bool {
        false
    }
    fn sample(&self, s: f64) -> (Point, Point) {
        (Point::new(s.clamp(0., self.length), 80.), Point::new(1., 0.))
    }
}
struct Circle;
impl Curve for Circle {
    fn length(&self) -> f64 {
        200. * std::f64::consts::PI
    }
    fn closed(&self) -> bool {
        true
    }
    fn sample(&self, s: f64) -> (Point, Point) {
        let t = s / 100.;
        (Point::new(128. + 100. * t.cos(), 128. + 100. * t.sin()), Point::new(-t.sin(), t.cos()))
    }
}
fn options() -> PathText {
    PathText { path: Path::default(), start: 0., end: 1., flip: false, align: PathTextAlign::Baseline }
}
fn text(t: &str) -> TextLayer {
    TextLayer { text: t.into(), font_family: "Inter".into(), size_pt: 20., ..Default::default() }
}

#[test]
fn on_path_analytic_origins_tangents_and_advance_sum() {
    let mut e = TextEngine::new();
    let shaped = e.layout(&text("AV office a\u{301}"), 72.);
    let expected: f64 = shaped.clusters.iter().map(|c| f64::from(c.advance)).sum();
    let result = place(shaped.clone(), &Circle, &options(), TextAlign::Left).unwrap();
    assert_eq!(result.shaped.glyphs, shaped.glyphs);
    assert!((result.advance - expected).abs() < 0.001);
    assert!((result.used - result.advance).abs() < 0.001);
    for c in &result.clusters {
        let t = (c.distance + c.advance * 0.5) / 100.;
        let expected = Point::new(128. + 100. * t.cos(), 128. + 100. * t.sin());
        assert!((expected.x - c.anchor.x).hypot(expected.y - c.anchor.y) < 0.5);
        let tangent = t + std::f64::consts::FRAC_PI_2;
        let delta = (c.angle - tangent).sin().atan2((c.angle - tangent).cos());
        assert!(delta.abs().to_degrees() < 0.5);
    }
}
#[test]
fn on_path_closed_seam_flip_overflow_and_zero_span() {
    let mut e = TextEngine::new();
    let shaped = e.layout(&text("ABCDEF"), 72.);
    let mut o = options();
    o.start = 0.98;
    o.end = 0.25;
    let result = place(shaped.clone(), &Circle, &o, TextAlign::Left).unwrap();
    assert!(!result.overflow);
    assert!((result.available - 0.27 * Circle.length()).abs() < 1e-7);
    o.flip = true;
    let flipped = place(shaped.clone(), &Circle, &o, TextAlign::Left).unwrap();
    assert_eq!(flipped.advance, result.advance);
    assert_eq!(flipped.start, result.start);
    assert_eq!(flipped.end, result.end);
    let (point, tangent) = Circle.sample(o.start * Circle.length() + flipped.available - flipped.clusters[0].advance * 0.5);
    let c = &flipped.clusters[0];
    assert!((c.anchor.x - point.x).hypot(c.anchor.y - point.y) < 1e-6);
    assert!((c.angle.sin() + tangent.y).abs() < 1e-6);
    o.start = 0.;
    o.end = 0.03;
    o.flip = false;
    let overflow = place(shaped.clone(), &Circle, &o, TextAlign::Left).unwrap();
    assert!(overflow.overflow && overflow.used < overflow.advance);
    o.end = 0.;
    assert!(place(shaped, &Circle, &o, TextAlign::Left).unwrap().clusters.iter().all(|c| !c.visible));
}
#[test]
fn on_path_baseline_alignment_and_straight_render_oracle() {
    let mut e = TextEngine::new();
    let layer = text("ABC");
    let shaped = e.layout(&layer, 72.);
    let line = Line { length: 300. };
    let mut o = options();
    o.start = 0.1;
    let result = place(shaped.clone(), &line, &o, TextAlign::Left).unwrap();
    let rendered = on_path::rasterize(&result, &Affine::IDENTITY, PixelFormat::RGBA8, layer.antialias, Rect::new(0, 0, 400, 200)).unwrap();
    let straight = render::rasterize(&shaped, &Affine::translate(30., 80.), PixelFormat::RGBA8, layer.antialias);
    let roi = Rect::new(0, 0, 400, 200);
    let mut a = vec![[0u8; 4]; 80000];
    let mut b = a.clone();
    rendered.surface.read_rgba8_into(roi, &mut a);
    straight.surface.read_rgba8_into(roi, &mut b);
    let (mut intersection, mut union) = (0, 0);
    for (a, b) in a.iter().zip(&b) {
        if a[3] > 127 && b[3] > 127 {
            intersection += 1;
        }
        if a[3] > 127 || b[3] > 127 {
            union += 1;
        }
    }
    assert!(intersection as f64 / union as f64 > 0.97);
    for (alignment, rise) in [
        (PathTextAlign::Ascender, f64::from(shaped.lines[0].ascent)),
        (PathTextAlign::Descender, -f64::from(shaped.lines[0].descent)),
        (PathTextAlign::Center, f64::from(shaped.lines[0].ascent - shaped.lines[0].descent) * 0.5),
    ] {
        o.align = alignment;
        let aligned = place(shaped.clone(), &line, &o, TextAlign::Left).unwrap();
        let g = shaped.glyphs[0];
        let p = aligned.clusters[0].transform.apply(Point::new(f64::from(g.x), f64::from(g.y)));
        assert!((p.y - 80. - rise).abs() < 1e-6);
    }
}
#[test]
fn on_path_bidi_marks_ligatures_and_vertical_glyphs_stay_shaped() {
    let mut e = TextEngine::new();
    for input in ["אבג ABC", "office a\u{301}", "العربية"] {
        let shaped = e.layout(&text(input), 72.);
        let result = place(shaped.clone(), &Line { length: 1000. }, &options(), TextAlign::Right).unwrap();
        assert_eq!(result.shaped.glyphs, shaped.glyphs);
        assert_eq!(result.shaped.clusters, shaped.clusters);
        assert_eq!(result.glyph_clusters.len(), shaped.glyphs.len());
        assert!(result.clusters.iter().all(|c| c.visible));
    }
    let layer = TextLayer { orientation: Orientation::Vertical, ..text("ABC") };
    let shaped = e.layout(&layer, 72.);
    let result = place(shaped.clone(), &Circle, &options(), TextAlign::Left).unwrap();
    assert!(shaped.glyphs.iter().all(|g| g.orient == layout::GlyphOrient::Rotated));
    assert_eq!(result.shaped.glyphs, shaped.glyphs);
    assert!(!on_path::outlines(&result, &Affine::IDENTITY).is_empty());
}
#[test]
fn on_path_invalid_inputs_and_clipped_cache_are_bounded() {
    let mut e = TextEngine::new();
    let shaped = e.layout(&text("ABC"), 72.);
    for invalid in [f64::NAN, f64::INFINITY, -0.1, 1.1] {
        let mut o = options();
        o.start = invalid;
        assert!(place(shaped.clone(), &Circle, &o, TextAlign::Left).is_err());
    }
    let mut o = options();
    o.start = 0.9;
    o.end = 0.1;
    assert!(place(shaped.clone(), &Line { length: 100. }, &o, TextAlign::Left).is_err());
    assert!(place(shaped.clone(), &Line { length: 0. }, &options(), TextAlign::Left).is_err());
    let result = place(shaped, &Circle, &options(), TextAlign::Left).unwrap();
    let r =
        on_path::rasterize(&result, &Affine::translate(10000., 10000.), PixelFormat::RGBA8, photocraft_doc::text::AntiAlias::Smooth, Rect::new(0, 0, 256, 256))
            .unwrap();
    assert!(r.rect.is_empty());
}
#[test]
fn on_path_styled_colors_decorations_and_faux_styles_project() {
    let mut e = TextEngine::new();
    let mut layer = text("AB");
    layer.runs = vec![
        TextRun { len: 1, style: CharStyle { font_family: "Inter".into(), size_pt: 20., underline: true, faux_bold: true, ..Default::default() } },
        TextRun { len: 1, style: CharStyle { color: photocraft_doc::Color::rgb(1., 0., 0.), strikethrough: true, ..Default::default() } },
    ];
    let result = on_path::layout(&mut e, &layer, &Circle, &options(), 72.).unwrap();
    let outlines = on_path::outlines(&result, &Affine::IDENTITY);
    assert!(outlines.len() >= 10);
    assert!(outlines.iter().any(|o| o.color == photocraft_doc::Color::rgb(1., 0., 0.)));
}

#[test]
fn on_path_japanese_vertical_font_metrics_and_alternates() {
    // A required external OFL fixture, never silently counted as a passing skip.
    let font = std::env::var("LOCAL_IMAGE_PATH_TEXT_CJK_FONT").unwrap_or_else(|_| "/usr/share/fonts/google-noto-sans-cjk-vf-fonts/NotoSansCJK-VF.ttc".into());
    let bytes = std::fs::read(font).expect("Noto Sans CJK OFL fixture required; see on_path/SOURCES.md");
    let mut engine = TextEngine::new();
    let families = engine.fonts.register_font_data(bytes);
    let family = families.iter().find(|f| f.contains("JP")).or(families.first()).expect("CJK font family").clone();
    let mut layer =
        TextLayer {
            text: "縦書き「テストー」ABC".into(), font_family: family, size_pt: 20., orientation: Orientation::Vertical, ..Default::default()
        };
    layer.runs = vec![TextRun {
        len: layer.text.len(),
        style: CharStyle { font_family: layer.font_family.clone(), size_pt: 20., underline: true, ..Default::default() },
    }];
    let shaped = engine.layout(&layer, 72.);
    assert!(shaped.glyphs.iter().all(|g| g.id != 0));
    assert!(shaped.glyphs.iter().any(|g| g.orient == layout::GlyphOrient::Upright));
    assert!(shaped.glyphs.iter().any(|g| g.orient == layout::GlyphOrient::Rotated));
    let result = place(shaped.clone(), &Circle, &options(), TextAlign::Left).unwrap();
    assert_eq!(result.shaped.glyphs, shaped.glyphs);
    assert_eq!(result.shaped.clusters, shaped.clusters);
    assert_eq!(result.glyph_clusters.len(), shaped.glyphs.len());
    assert!(!result.overflow);
    assert!(!on_path::rasterize(&result, &Affine::IDENTITY, PixelFormat::RGBA8, layer.antialias, Rect::new(0, 0, 256, 256)).unwrap().rect.is_empty());
    // Independent straight vertical raster, including the underline outside
    // the logical column's ascent/descent box.
    let straight = place(shaped.clone(), &Line { length: 1000. }, &options(), TextAlign::Left).unwrap();
    let ours = on_path::rasterize(&straight, &Affine::IDENTITY, PixelFormat::RGBA8, layer.antialias, Rect::new(0, 0, 512, 256)).unwrap();
    let original =
        render::rasterize(&shaped, &Affine::translate(0., 80.).mul(&Affine::rotate(-std::f64::consts::FRAC_PI_2)), PixelFormat::RGBA8, layer.antialias);
    let rect = Rect::new(0, 0, 512, 256);
    let a = ours.surface.read_region(rect);
    let b = original.surface.read_region(rect);
    let (mut intersection, mut union) = (0, 0);
    for (a, b) in a.as_chunks::<4>().0.iter().zip(b.as_chunks::<4>().0) {
        if a[3] > 0.5 && b[3] > 0.5 {
            intersection += 1;
        }
        if a[3] > 0.5 || b[3] > 0.5 {
            union += 1;
        }
    }
    assert!(union > 0);
    assert!(intersection as f64 / union as f64 > 0.97, "vertical underline IoU={}", intersection as f64 / union as f64);
}

#[test]
fn on_path_trailing_spaces_and_caps_expansion_keep_advance_and_cluster_identity() {
    let mut e = TextEngine::new();
    let mut layer = text("straße  ");
    layer.runs = vec![TextRun {
        len: layer.text.len(),
        style: CharStyle { font_family: "Inter".into(), size_pt: 20., caps: photocraft_doc::text::Caps::AllCaps, ..Default::default() },
    }];
    let shaped = e.layout(&layer, 72.);
    let expected: f64 = shaped.clusters.iter().map(|c| f64::from(c.advance)).sum();
    let result = place(shaped.clone(), &Line { length: 1000. }, &options(), TextAlign::Left).unwrap();
    assert!((result.advance - expected).abs() < 0.001);
    assert!((result.used - expected).abs() < 0.001);
    for (g, &index) in shaped.glyphs.iter().zip(&result.glyph_clusters) {
        let c = &result.clusters[index];
        let original = &shaped.clusters[c.source_index];
        assert!(g.x >= original.x - 0.01 && g.x <= original.x + original.advance + 0.01);
    }
}

#[test]
fn on_path_256_analytic_line_properties() {
    let mut e = TextEngine::new();
    let shaped = e.layout(&text("AV office"), 72.);
    for case in 0..256 {
        let line = Line { length: 150. + f64::from(case) * 3. };
        let start = f64::from(case % 7) / 20.;
        let end = 0.7 + f64::from(case % 4) / 10.;
        let o = PathText { start, end, flip: case % 2 == 0, ..options() };
        let result = place(shaped.clone(), &line, &o, TextAlign::Left).unwrap();
        assert!((result.available - (end - start) * line.length()).abs() < 1e-7);
        for c in &result.clusters {
            let origin = c.transform.apply(Point::new(f64::from(shaped.clusters[c.source_index].x), 0.));
            let expected = if o.flip { end * line.length() - c.distance } else { start * line.length() + c.distance };
            assert!((origin.x - expected).abs() < 1e-6);
            assert!((origin.y - 80.).abs() < 1e-6);
            assert_eq!(c.visible, c.distance + c.advance <= result.available + 1e-6);
        }
        assert_eq!(result.overflow, result.advance > result.available + 1e-6);
    }
}
