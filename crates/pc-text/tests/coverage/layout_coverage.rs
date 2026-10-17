use std::ops::Range;

use photocraft_text::layout::{
    ClusterInfo, LineInfo, TextLayout, VClass, byte_index, char_index, hit_char, line_edge, line_index, line_step, split_paragraphs, text_point_inside,
    vertical_class, word_boundary,
};

fn line(range: Range<usize>, baseline: f32, x0: f32, x1: f32, ascent: f32, descent: f32, paragraph: usize) -> LineInfo {
    LineInfo { range, baseline, x0, x1, ascent, descent, paragraph }
}

fn cluster(range: Range<usize>, x: f32, advance: f32, line: usize, rtl: bool) -> ClusterInfo {
    ClusterInfo { range, x, advance, line, rtl }
}

fn layout_with(lines: Vec<LineInfo>, clusters: Vec<ClusterInfo>) -> TextLayout {
    TextLayout { lines, clusters, vertical: false, ..Default::default() }
}

#[test]
fn byte_index_ascii_and_past_end() {
    let text = "abc";
    assert_eq!(byte_index(text, 0), 0);
    assert_eq!(byte_index(text, 1), 1);
    assert_eq!(byte_index(text, 2), 2);
    assert_eq!(byte_index(text, 3), 3);
    assert_eq!(byte_index(text, 99), 3);
}

#[test]
fn byte_index_multibyte() {
    let text = "aé🙂";
    assert_eq!(byte_index(text, 0), 0);
    assert_eq!(byte_index(text, 1), 1);
    assert_eq!(byte_index(text, 2), 3);
    assert_eq!(byte_index(text, 3), 7);
    assert_eq!(byte_index(text, 4), 7);
}

#[test]
fn char_index_floors_to_char_boundary() {
    let text = "aé🙂";
    assert_eq!(char_index(text, 0), 0);
    assert_eq!(char_index(text, 1), 1);
    assert_eq!(char_index(text, 2), 1);
    assert_eq!(char_index(text, 3), 2);
    assert_eq!(char_index(text, 7), 3);
    assert_eq!(char_index(text, 8), 3);
    assert_eq!(char_index("", 4), 0);
}

#[test]
fn word_boundary_forward_and_backward() {
    let text = "hello world";
    assert_eq!(word_boundary(text, 0, true), 5);
    assert_eq!(word_boundary(text, 1, true), 5);
    assert_eq!(word_boundary(text, 5, true), 11);
    assert_eq!(word_boundary(text, 8, false), 6);
    assert_eq!(word_boundary(text, 10, false), 6);
    assert_eq!(word_boundary(text, 0, false), 0);
}

#[test]
fn word_boundary_empty_past_end_and_unicode() {
    assert_eq!(word_boundary("", 0, true), 0);
    assert_eq!(word_boundary("abc", 99, true), 3);
    assert_eq!(word_boundary("abc", 99, false), 0);

    let text = "héllo wörld";
    assert_eq!(word_boundary(text, 1, true), 5);
    assert_eq!(word_boundary(text, 1, false), 0);
}

#[test]
fn line_index_within_past_and_empty() {
    let layout = layout_with(vec![line(0..5, 0.0, 0.0, 50.0, 10.0, 5.0, 0), line(5..10, 20.0, 0.0, 50.0, 10.0, 5.0, 1)], vec![]);

    assert_eq!(line_index(&layout, 0), 0);
    assert_eq!(line_index(&layout, 4), 0);
    assert_eq!(line_index(&layout, 5), 0);
    assert_eq!(line_index(&layout, 6), 1);
    assert_eq!(line_index(&layout, 9), 1);
    assert_eq!(line_index(&layout, 10), 1);
    assert_eq!(line_index(&TextLayout::default(), 0), 0);
}

#[test]
fn line_edge_uses_line_ranges() {
    let layout = layout_with(vec![line(0..5, 0.0, 0.0, 50.0, 10.0, 5.0, 0), line(5..10, 20.0, 0.0, 50.0, 10.0, 5.0, 1)], vec![]);

    let text = "abcdefghij";
    assert_eq!(line_edge(&layout, text, 2, false), 0);
    assert_eq!(line_edge(&layout, text, 2, true), 5);
    assert_eq!(line_edge(&layout, text, 7, false), 5);
    assert_eq!(line_edge(&layout, text, 7, true), 10);
    assert_eq!(line_edge(&layout, text, 20, true), 10);
}

#[test]
fn text_point_inside_bounds_and_slop() {
    let layout = layout_with(vec![line(0..3, 20.0, 10.0, 30.0, 5.0, 5.0, 0)], vec![]);

    assert!(text_point_inside(&layout, 10.0, 15.0, 0.0));
    assert!(text_point_inside(&layout, 30.0, 25.0, 0.0));
    assert!(!text_point_inside(&layout, 9.9, 15.0, 0.0));

    assert!(text_point_inside(&layout, 9.0, 15.0, 1.0));
    assert!(!text_point_inside(&layout, 9.0, 15.0, 0.0));

    assert!(!text_point_inside(&layout, 9.9, 15.0, f32::NAN));
    assert!(!text_point_inside(&layout, 9.9, 15.0, f32::INFINITY));
    assert!(text_point_inside(&layout, 10.0, 15.0, f32::NEG_INFINITY));

    assert!(!text_point_inside(&TextLayout::default(), 0.0, 0.0, 10.0));
}

#[test]
fn to_text_and_to_line_round_trip() {
    let horizontal = TextLayout::default();
    assert_eq!(horizontal.to_text(3.0, 4.0), (3.0, 4.0));
    assert_eq!(horizontal.to_line(3.0, 4.0), (3.0, 4.0));

    let vertical = TextLayout { vertical: true, ..TextLayout::default() };
    assert_eq!(vertical.to_text(3.0, 4.0), (-4.0, 3.0));
    assert_eq!(vertical.to_line(-4.0, 3.0), (3.0, 4.0));

    let (x, y) = vertical.to_text(1.5, -2.25);
    assert_eq!(vertical.to_line(x, y), (1.5, -2.25));
}

#[test]
fn bounds_and_line_bounds() {
    let horizontal = layout_with(vec![line(0..3, 20.0, 10.0, 30.0, 5.0, 5.0, 0)], vec![]);
    assert_eq!(horizontal.line_bounds(), Some([10.0, 15.0, 30.0, 25.0]));
    assert_eq!(horizontal.bounds(), Some([10.0, 15.0, 30.0, 25.0]));

    let vertical = TextLayout { vertical: true, ..horizontal.clone() };
    assert_eq!(vertical.line_bounds(), Some([10.0, 15.0, 30.0, 25.0]));
    assert_eq!(vertical.bounds(), Some([-25.0, 10.0, -15.0, 30.0]));

    assert_eq!(TextLayout::default().line_bounds(), None);
    assert_eq!(TextLayout::default().bounds(), None);
}

#[test]
fn hit_test_line_respects_cluster_edges() {
    let layout = layout_with(
        vec![line(0..3, 0.0, 0.0, 30.0, 10.0, 10.0, 0)],
        vec![cluster(0..1, 0.0, 10.0, 0, false), cluster(1..2, 10.0, 10.0, 0, false), cluster(2..3, 20.0, 10.0, 0, false)],
    );

    assert_eq!(layout.hit_test_line(4.0, 0.0), 0);
    assert_eq!(layout.hit_test_line(14.0, 0.0), 1);
    assert_eq!(layout.hit_test_line(16.0, 0.0), 2);
    assert_eq!(layout.hit_test_line(-10.0, 0.0), 0);
    assert_eq!(layout.hit_test_line(25.0, 0.0), 2);
    assert_eq!(TextLayout::default().hit_test_line(10.0, 0.0), 0);
}

#[test]
fn hit_test_converts_vertical_coordinates() {
    let layout = TextLayout {
        vertical: true,
        ..layout_with(
            vec![line(0..3, 0.0, 0.0, 30.0, 10.0, 10.0, 0)],
            vec![cluster(0..1, 0.0, 10.0, 0, false), cluster(1..2, 10.0, 10.0, 0, false), cluster(2..3, 20.0, 10.0, 0, false)],
        )
    };

    assert_eq!(layout.hit_test(4.0, 0.0), 0);
    assert_eq!(layout.hit_test(14.0, 0.0), 0);
    assert_eq!(layout.hit_test(0.0, 14.0), 1);
}

#[test]
fn caret_and_caret_segment_horizontal_and_vertical() {
    let horizontal = layout_with(vec![line(0..3, 20.0, 0.0, 30.0, 5.0, 5.0, 0)], vec![cluster(0..1, 10.0, 10.0, 0, false)]);
    assert_eq!(horizontal.caret(0), (10.0, 15.0, 25.0));
    assert_eq!(horizontal.caret_segment(0), [(10.0, 15.0), (10.0, 25.0)]);

    let vertical = TextLayout { vertical: true, ..horizontal.clone() };
    assert_eq!(vertical.caret(0), (10.0, 15.0, 25.0));
    assert_eq!(vertical.caret_segment(0), [(-15.0, 10.0), (-25.0, 10.0)]);

    let rtl = layout_with(vec![line(0..1, 0.0, 0.0, 10.0, 10.0, 10.0, 0)], vec![cluster(0..1, 0.0, 10.0, 0, true)]);
    assert_eq!(rtl.caret(0), (10.0, -10.0, 10.0));
}

#[test]
fn caret_end_falls_back_to_line_end() {
    let layout = layout_with(vec![line(0..3, 0.0, 0.0, 30.0, 5.0, 5.0, 0)], vec![cluster(0..1, 10.0, 10.0, 0, false)]);

    assert_eq!(layout.caret(1), (20.0, -5.0, 5.0));
    assert_eq!(layout.caret(3), (30.0, -5.0, 5.0));
}

#[test]
fn split_paragraphs_handles_line_breaks_and_trailing_empty() {
    assert_eq!(split_paragraphs("a\nb\r\nc\rd"), vec![0..2, 2..5, 5..7, 7..8]);
    assert_eq!(split_paragraphs("a\n"), vec![0..2, 2..2]);
    assert_eq!(split_paragraphs("\r\n"), vec![0..2, 2..2]);
    assert_eq!(split_paragraphs(""), vec![0..0]);
}

#[test]
fn vertical_class_classifies_representative_chars() {
    assert_eq!(vertical_class('A'), VClass::Rotate);
    assert_eq!(vertical_class('0'), VClass::Rotate);
    assert_eq!(vertical_class('\u{6F22}'), VClass::Upright); // 漢
    assert_eq!(vertical_class('（'), VClass::AlternateOrRotate);
    assert_eq!(vertical_class(' '), VClass::Rotate);
    assert_eq!(vertical_class('\u{FF61}'), VClass::Rotate);
}

#[test]
fn byte_and_char_index_round_trip_for_valid_indices() {
    let text = "aé🙂z";
    let char_len = text.chars().count();
    for idx in 0..=char_len {
        assert_eq!(char_index(text, byte_index(text, idx)), idx);
    }
}

#[test]
fn line_step_moves_to_neighbouring_lines_or_edges() {
    let layout = layout_with(
        vec![line(0..3, 0.0, 0.0, 30.0, 10.0, 10.0, 0), line(3..6, 30.0, 0.0, 30.0, 10.0, 10.0, 1)],
        vec![
            cluster(0..1, 0.0, 10.0, 0, false),
            cluster(1..2, 10.0, 10.0, 0, false),
            cluster(2..3, 20.0, 10.0, 0, false),
            cluster(3..4, 0.0, 10.0, 1, false),
            cluster(4..5, 10.0, 10.0, 1, false),
            cluster(5..6, 20.0, 10.0, 1, false),
        ],
    );

    let text = "abcdef";
    assert_eq!(line_step(&layout, text, 0, 10.0, 1), 4);
    assert_eq!(line_step(&layout, text, 0, 10.0, -1), 0);
    assert_eq!(line_step(&layout, text, 5, 10.0, -1), 1);
    assert_eq!(line_step(&layout, text, 6, 10.0, 1), 6);
}

#[test]
fn hit_char_combines_hit_test_line_and_char_index() {
    let layout = layout_with(
        vec![line(0..5, 0.0, 0.0, 50.0, 10.0, 10.0, 0), line(5..10, 20.0, 0.0, 50.0, 10.0, 10.0, 1)],
        vec![
            cluster(0..1, 0.0, 10.0, 0, false),
            cluster(1..2, 10.0, 10.0, 0, false),
            cluster(2..3, 20.0, 10.0, 0, false),
            cluster(3..4, 30.0, 10.0, 0, false),
            cluster(4..5, 40.0, 10.0, 0, false),
            cluster(5..6, 0.0, 10.0, 1, false),
            cluster(6..7, 10.0, 10.0, 1, false),
            cluster(7..8, 20.0, 10.0, 1, false),
            cluster(8..9, 30.0, 10.0, 1, false),
            cluster(9..10, 40.0, 10.0, 1, false),
        ],
    );

    let text = "abcdefghij";
    assert_eq!(hit_char(&layout, text, 4.0, 0.0), (0, 0));
    assert_eq!(hit_char(&layout, text, 14.0, 0.0), (1, 0));
    assert_eq!(hit_char(&layout, text, 14.0, 20.0), (6, 1));
}

#[test]
fn line_step_uses_adjacent_index_despite_wide_leading_overlap_or_empty_lines() {
    for vertical in [false, true] {
        for baselines in [[0., 100., 200.], [0., 1., 2.]] {
            let mut layout = layout_with(
                vec![line(0..2, baselines[0], 0., 20., 9., 3., 0), line(3..3, baselines[1], 0., 0., 1., 1., 1), line(4..6, baselines[2], 0., 20., 35., 8., 2)],
                vec![cluster(0..1, 0., 10., 0, false), cluster(1..2, 10., 10., 0, false), cluster(4..5, 0., 10., 2, false), cluster(5..6, 10., 10., 2, false)],
            );
            layout.vertical = vertical;
            let text = "ab\n\ncd";
            assert_eq!(line_step(&layout, text, 1, 10., 1), 3, "first to empty exactly once");
            assert_eq!(line_step(&layout, text, 3, 10., 1), 5, "empty to next");
            assert_eq!(line_step(&layout, text, 5, 10., -1), 3, "last to empty");
            assert_eq!(line_step(&layout, text, 3, 10., -1), 1, "empty to previous");
            assert_eq!(line_step(&layout, text, 1, 10., -1), 0);
            assert_eq!(line_step(&layout, text, 5, 10., 1), 6);
            assert_eq!(line_step(&layout, text, usize::MAX, 10., 1), 6);
        }
    }
    assert_eq!(line_step(&TextLayout::default(), "abc", 2, 0., 1), 2);
    assert_eq!(line_step(&TextLayout::default(), "", 100, 0., -1), 0);
}

#[test]
fn line_step_matches_caret_affinity_at_shared_wrap_boundary() {
    let layout = layout_with(
        vec![line(0..3, 0., 0., 30., 9., 3., 0), line(3..6, 100., 0., 30., 9., 3., 0), line(6..9, 200., 0., 30., 9., 3., 0)],
        (0..9).map(|i| cluster(i..i + 1, (i % 3) as f32 * 10., 10., i / 3, false)).collect(),
    );
    assert_eq!(line_index(&layout, 3), 0, "public inclusive range semantics unchanged");
    assert_eq!(layout.caret(3).1, 91., "caret belongs to next wrapped line");
    assert_eq!(line_step(&layout, "abcdefghi", 3, 10., -1), 1);
    assert_eq!(line_step(&layout, "abcdefghi", 3, 10., 1), 7);
}

#[test]
fn line_step_keeps_multibyte_grapheme_and_bidi_cluster_edges() {
    let text = "a\né\u{301}ב";
    let mut layout = layout_with(
        vec![line(0..1, 0., 0., 10., 9., 3., 0), line(2..8, 100., 0., 20., 9., 3., 1)],
        vec![cluster(0..1, 0., 10., 0, false), cluster(2..6, 0., 10., 1, false), cluster(6..8, 10., 10., 1, true)],
    );
    for vertical in [false, true] {
        layout.vertical = vertical;
        assert_eq!(line_step(&layout, text, 0, 0., 1), 2);
        assert_eq!(line_step(&layout, text, 0, 9., 1), 4, "combining grapheme has no interior stop");
        assert_eq!(line_step(&layout, text, 0, 19., 1), 4, "RTL right edge is its logical start");
        assert_eq!(line_step(&layout, text, 4, 0., -1), 0);
    }
}
