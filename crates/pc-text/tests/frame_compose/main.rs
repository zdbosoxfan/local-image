use photocraft_color::PixelFormat;
use photocraft_doc::{
    TextLayer,
    text::{CharStyle, Orientation, ParagraphRun, ParagraphStyle, TextAlign, TextRun, TextShape},
};
use photocraft_geom::Rect;
use photocraft_text::{
    TextEngine,
    frame_compose::{self, Options, Route},
    layout::TextLayout,
};
fn layer(text: &str, width: f32) -> TextLayer {
    TextLayer {
        text: text.into(),
        shape: TextShape::Box { x: 8.0, y: 10.0, width, height: 500.0 },
        runs: vec![TextRun {
            len: text.len(),
            style: CharStyle { font_family: "JetBrains Mono".into(), size_pt: 12.0, ligatures: false, ..Default::default() },
        }],
        ..Default::default()
    }
}
fn options() -> Options {
    Options { enabled: true, ..Default::default() }
}
fn same(a: &TextLayout, b: &TextLayout) {
    assert_eq!(a.glyphs, b.glyphs);
    assert_eq!(a.lines, b.lines);
    assert_eq!(a.clusters, b.clusters);
    assert_eq!(a.styles, b.styles);
    assert_eq!(a.decorations, b.decorations);
    assert_eq!(a.faces.len(), b.faces.len());
    for (a, b) in a.faces.iter().zip(&b.faces) {
        assert_eq!(a.font.data.as_ref(), b.font.data.as_ref());
        assert_eq!(a.coords, b.coords);
        assert_eq!(a.size_px, b.size_px);
    }
}
#[test]
fn frame_compose_default_identity_point_and_native_fallback_runs() {
    let mut e = TextEngine::new();
    for text in ["office café e\u{301} 😀", "שלום world", "ไทย ภาษาไทย", "日本語、かな。", "abc\u{3}def", "a\tb", "soft\u{ad}hyphen"]
    {
        let mut t = layer(text, 80.0);
        let native = e.layout(&t, 72.0);
        same(&native, &frame_compose::compose(&mut e, &t, 72.0, Options::default()).unwrap().layout);
        if text.contains('ש') || text.contains('ไ') || text.contains('\t') || text.contains('\u{3}') || text.contains('\u{ad}') {
            let c = frame_compose::compose(&mut e, &t, 72.0, options()).unwrap();
            assert_eq!(c.route, Route::NativeComplexText);
            same(&native, &c.layout);
        }
        t.shape = TextShape::Point;
        let native = e.layout(&t, 72.0);
        same(&native, &frame_compose::compose(&mut e, &t, 72.0, options()).unwrap().layout);
        t.shape = layer(text, 80.0).shape;
        t.orientation = Orientation::Vertical;
        let native = e.layout(&t, 72.0);
        same(&native, &frame_compose::compose(&mut e, &t, 72.0, options()).unwrap().layout);
    }
}
#[test]
fn frame_compose_reference_monospace_paragraph_and_reflow() {
    let mut e = TextEngine::new();
    let cell = e.layout(&layer("M", 100.0), 72.0).clusters[0].advance;
    let t = layer("aa aa aa aa", cell * 5.5);
    let c = frame_compose::compose(&mut e, &t, 72.0, options()).unwrap();
    assert_eq!(c.route, Route::Composed);
    assert_eq!(c.overflow, None);
    assert_eq!(c.layout.lines.iter().map(|l| l.range.clone()).collect::<Vec<_>>(), [0..5, 6..11]);
    assert!(c.layout.lines.iter().all(|l| (l.x1 - l.x0 - cell * 5.0).abs() < 0.02));
    let balanced = frame_compose::compose(&mut e, &t, 72.0, Options { balanced: true, ..options() }).unwrap();
    assert_eq!(balanced.layout.lines.iter().map(|l| l.range.clone()).collect::<Vec<_>>(), [0..5, 6..11]);
    assert_eq!(balanced.overflow, None);
    let wide = layer(&t.text, cell * 12.0);
    assert_eq!(frame_compose::compose(&mut e, &wide, 72.0, options()).unwrap().layout.lines.len(), 1);
}
#[test]
fn frame_compose_justification_grid_optical_and_run_preservation() {
    let mut e = TextEngine::new();
    let cell = e.layout(&layer("M", 100.0), 72.0).clusters[0].advance;
    let mut t = layer("aa aa aa aa", cell * 5.15);
    t.paragraphs = vec![ParagraphRun { len: t.text.len(), style: ParagraphStyle { align: TextAlign::JustifyLeft, ..Default::default() } }];
    let original = t.clone();
    let c = frame_compose::compose(&mut e, &t, 72.0, Options { grid_spacing: 18.0, grid_origin: 3.0, ..options() }).unwrap();
    assert!((c.layout.lines[0].x1 - c.layout.lines[0].x0 - cell * 5.15).abs() < 0.03);
    assert!((c.layout.lines[1].x1 - c.layout.lines[1].x0 - cell * 5.0).abs() < 0.03);
    for l in &c.layout.lines {
        assert!(((l.baseline - 3.0) / 18.0 - ((l.baseline - 3.0) / 18.0).round()).abs() < 1e-5);
    }
    assert_eq!(t, original);
    let t = layer("\"word.\"", 300.0);
    let plain = frame_compose::compose(&mut e, &t, 72.0, options()).unwrap();
    let optical = frame_compose::compose(&mut e, &t, 72.0, Options { optical_margins: true, ..options() }).unwrap();
    assert!(optical.layout.lines[0].x0 < plain.layout.lines[0].x0);
    // A final punctuation hang is allowed beyond the measure; it is not overset text.
    let narrow = layer("word.", cell * 4.5);
    assert!(frame_compose::compose(&mut e, &narrow, 72.0, options()).unwrap().overflow.is_some());
    assert_eq!(frame_compose::compose(&mut e, &narrow, 72.0, Options { optical_margins: true, ..options() }).unwrap().overflow, None);
    assert_eq!(
        plain.layout.glyphs.iter().map(|g| (g.id, g.style)).collect::<Vec<_>>(),
        optical.layout.glyphs.iter().map(|g| (g.id, g.style)).collect::<Vec<_>>()
    );
}
#[test]
fn frame_compose_multilingual_clusters_overflow_and_mixed_styles() {
    let mut e = TextEngine::new();
    let mut t = layer("office e\u{301} café 😀 日本語、かな。\nNext paragraph", 90.0);
    t.runs = vec![
        TextRun { len: 7, style: CharStyle { size_pt: 18.0, underline: true, ..t.runs[0].style.clone() } },
        TextRun { len: t.text.len() - 7, style: t.runs[0].style.clone() },
    ];
    let c = frame_compose::compose(&mut e, &t, 72.0, options()).unwrap();
    assert!(!c.layout.decorations.is_empty());
    assert!(c.layout.styles.iter().any(|s| s.size_pt == 18.0));
    assert!(c.layout.styles.iter().any(|s| s.size_pt == 12.0));
    for cluster in &c.layout.clusters {
        assert!(t.text.is_char_boundary(cluster.range.start));
        assert!(t.text.is_char_boundary(cluster.range.end));
    }
    // Native Parley exposes a separate caret cluster for the combining mark.
    // The composer must keep that mark with its base on one line, not erase its caret.
    let base = c.layout.clusters.iter().find(|c| c.range.start == 7).unwrap();
    let mark = c.layout.clusters.iter().find(|c| c.range.start == 8).unwrap();
    assert_eq!(base.line, mark.line);
    assert!(c.layout.lines.iter().all(|l| l.range.start != 8));
    t.shape = TextShape::Box { x: 8.0, y: 10.0, width: 90.0, height: 18.0 };
    let c = frame_compose::compose(&mut e, &t, 72.0, options()).unwrap();
    assert!(c.overflow.is_some());
    assert!(c.layout.lines.len() < 3);
    let c = frame_compose::compose(&mut e, &layer("averylongunbreakableword", 1.0), 72.0, options()).unwrap();
    assert_eq!(c.overflow, Some(0));
}
#[test]
fn frame_compose_native_render_identity_clipped_and_validation() {
    let mut e = TextEngine::new();
    let t = layer("default pixels", 300.0);
    let (native, r) = e.render(&t, 72.0, PixelFormat::RGBA8);
    let c = frame_compose::compose(&mut e, &t, 72.0, Options::default()).unwrap();
    same(&native, &c.layout);
    let clip = Rect::new(0, 0, 64, 64);
    let bounded = frame_compose::render_bounded(&c, &t, PixelFormat::RGBA8, clip).unwrap();
    assert_eq!(r.surface.read_region(clip), bounded.surface.read_region(clip));
    assert_eq!(bounded.rect, bounded.rect.intersect(&clip));
    assert!(frame_compose::compose(&mut e, &t, f32::NAN, options()).is_err());
    assert!(frame_compose::compose(&mut e, &t, 72.0, Options { grid_spacing: f32::INFINITY, ..options() }).is_err());
    let mut bad = t;
    bad.runs[0].style.horizontal_scale = 0.0;
    assert!(frame_compose::compose(&mut e, &bad, 72.0, options()).is_err());
}
#[test]
fn frame_compose_cjk_break_oracle_and_elastic_limits() {
    use frame_compose::breaker::{cjk_break_between, tiered_ratio};
    assert!(cjk_break_between('日', '本'));
    assert!(!cjk_break_between('日', '。'));
    assert!(!cjk_break_between('「', '日'));
    assert_eq!(tiered_ratio(5.0, [10.0, 0.0, 0.0], [0.0; 3]), (0.5, true));
    assert_eq!(tiered_ratio(-5.0, [0.0; 3], [10.0, 0.0, 0.0]), (-0.5, true));
    assert!(!tiered_ratio(-11.0, [0.0; 3], [10.0, 0.0, 0.0]).1);
}
