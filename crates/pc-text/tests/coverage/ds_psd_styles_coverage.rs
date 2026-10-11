use photocraft_doc::text::{CharStyle, ParagraphRun, ParagraphStyle, TextAlign, TextRun};
use photocraft_doc::text_styles::{CharacterStyleDef, ParagraphStyleDef, diff_attrs};
use photocraft_doc::{TextLayer, TextStyles};
use photocraft_text::fonts::DEFAULT_FAMILY;
use photocraft_text::psd;
use photocraft_text::psd_styles::{has_named_sheets, read_style_sheets, write_style_sheets};

fn make_layer(text: &str, runs: Vec<TextRun>, paragraphs: Vec<ParagraphRun>) -> TextLayer {
    let mut layer = TextLayer { text: text.to_string(), runs, paragraphs, ..Default::default() };
    layer.sync_summary();
    layer
}

fn char_def(id: u32, name: &str, style: CharStyle) -> CharacterStyleDef {
    CharacterStyleDef { id, name: name.to_string(), attrs: diff_attrs(&style, &CharStyle::default()) }
}

fn para_def(id: u32, name: &str, style: ParagraphStyle) -> ParagraphStyleDef {
    ParagraphStyleDef { id, name: name.to_string(), para_attrs: diff_attrs(&style, &ParagraphStyle::default()), ..Default::default() }
}

#[test]
fn malformed_input_returns_none() {
    assert!(read_style_sheets(&[], 72.0).is_none());
    assert!(read_style_sheets(&[0u8; 60], 72.0).is_none());
    assert!(write_style_sheets(&[1, 2, 3], &TextLayer::default(), &TextStyles::default(), 72.0).is_none());
    assert!(!has_named_sheets(b"junk"));
}

#[test]
fn plain_layer_has_no_named_sheets() {
    let layer = make_layer("hello", vec![TextRun { len: 5, style: CharStyle::default() }], vec![]);
    let tysh = psd::build_tysh(&layer, 72.0, None);
    assert!(!has_named_sheets(&tysh));

    let sheets = read_style_sheets(&tysh, 72.0).expect("read");
    assert!(sheets.char_sheets.is_empty());
    assert!(sheets.para_sheets.is_empty());
    assert_eq!(sheets.run_char.len(), 1);
    assert!(sheets.run_char[0].is_none());
}

#[test]
fn no_styles_round_trip_keeps_no_named_sheets() {
    let base = CharStyle { font_family: DEFAULT_FAMILY.into(), ..Default::default() };
    let layer = make_layer("abc", vec![TextRun { len: 3, style: base }], vec![]);
    let tysh = psd::build_tysh(&layer, 72.0, None);
    let out = write_style_sheets(&tysh, &layer, &TextStyles::default(), 72.0).expect("write");

    assert!(!has_named_sheets(&out));
    let r = read_style_sheets(&out, 72.0).expect("read");
    assert!(r.char_sheets.is_empty());
    assert!(r.para_sheets.is_empty());
    assert_eq!(r.run_char, vec![None]);
}

#[test]
fn named_sheets_round_trip() {
    let mut styles = TextStyles::default();
    styles.character.push(char_def(4, "Emphasis", CharStyle { size_pt: 30.0, underline: true, ..Default::default() }));
    styles.paragraph.push(para_def(2, "Centered", ParagraphStyle { align: TextAlign::Center, ..Default::default() }));

    let base = CharStyle { font_family: DEFAULT_FAMILY.into(), ..Default::default() };
    let emph = styles.resolve_char(Some(2), Some(4), DEFAULT_FAMILY);

    let layer = make_layer(
        "plain bold",
        vec![TextRun { len: 6, style: base }, TextRun { len: 4, style: emph }],
        vec![ParagraphRun { len: 10, style: styles.resolve_para(Some(2)) }],
    );

    let tysh = psd::build_tysh(&layer, 72.0, None);
    assert!(!has_named_sheets(&tysh));

    let out = write_style_sheets(&tysh, &layer, &styles, 72.0).expect("write");
    assert!(has_named_sheets(&out));

    let r = read_style_sheets(&out, 72.0).expect("read");
    assert_eq!(r.char_sheets.len(), 1);
    assert_eq!(r.char_sheets[0].1, "Emphasis");
    assert!((r.char_sheets[0].2.size_pt - 30.0).abs() < 0.01);
    assert!(r.char_sheets[0].2.underline);

    assert_eq!(r.para_sheets[0].1, "Centered");
    assert_eq!(r.para_sheets[0].2.align, TextAlign::Center);

    assert_eq!(r.run_char, vec![None, Some(1)]);
    assert_eq!(r.run_para, vec![Some(1)]);

    let back = psd::text_layer_from_tysh(&out, 72.0).expect("parse");
    assert_eq!(back.text, layer.text);
    assert!((back.char_runs()[1].style.size_pt - 30.0).abs() < 0.01);

    let plain = write_style_sheets(&out, &layer, &TextStyles::default(), 72.0).expect("write plain");
    assert!(!has_named_sheets(&plain));
    let pr = read_style_sheets(&plain, 72.0).expect("read plain");
    assert!(pr.run_char.iter().all(Option::is_none));
}

#[test]
fn empty_styles_shrink_sets() {
    let mut styles = TextStyles::default();
    styles.character.push(char_def(7, "S", CharStyle { size_pt: 9.0, ..Default::default() }));

    let layer = make_layer("ab", vec![TextRun { len: 2, style: styles.resolve_char(None, Some(7), DEFAULT_FAMILY) }], vec![]);
    let tysh = psd::build_tysh(&layer, 72.0, None);
    let out = write_style_sheets(&tysh, &layer, &styles, 72.0).expect("write");
    assert!(has_named_sheets(&out));

    let plain = write_style_sheets(&out, &layer, &TextStyles::default(), 72.0).expect("write plain");
    assert!(!has_named_sheets(&plain));
    let r = read_style_sheets(&plain, 72.0).expect("read");
    assert!(r.run_char.iter().all(Option::is_none));
}

#[test]
fn writes_all_styles_even_if_unreferenced() {
    let mut styles = TextStyles::default();
    styles.character.push(char_def(9, "Unused", CharStyle { size_pt: 11.0, ..Default::default() }));

    let layer = make_layer("abc", vec![TextRun { len: 3, style: CharStyle::default() }], vec![]);
    let tysh = psd::build_tysh(&layer, 72.0, None);
    let out = write_style_sheets(&tysh, &layer, &styles, 72.0).expect("write");

    assert!(has_named_sheets(&out));
    let r = read_style_sheets(&out, 72.0).expect("read");
    assert_eq!(r.char_sheets.len(), 1);
    assert_eq!(r.char_sheets[0].1, "Unused");
    assert!(r.run_char.iter().all(Option::is_none));
}

#[test]
fn multiple_styles_round_trip() {
    let mut styles = TextStyles::default();
    for (id, name, size) in [(1, "One", 12.0), (3, "Three", 24.0), (7, "Seven", 36.0)] {
        styles.character.push(char_def(id, name, CharStyle { size_pt: size, ..Default::default() }));
    }
    for (id, name) in [(5, "Left"), (6, "Right")] {
        styles.paragraph.push(para_def(id, name, ParagraphStyle { align: TextAlign::Left, ..Default::default() }));
    }

    let base = CharStyle { font_family: DEFAULT_FAMILY.into(), ..Default::default() };
    let styled = |id| styles.resolve_char(Some(5), Some(id), DEFAULT_FAMILY);

    let layer = make_layer(
        "abcdef",
        vec![TextRun { len: 1, style: base }, TextRun { len: 2, style: styled(1) }, TextRun { len: 3, style: styled(3) }],
        vec![ParagraphRun { len: 6, style: styles.resolve_para(Some(5)) }],
    );

    let tysh = psd::build_tysh(&layer, 72.0, None);
    let out = write_style_sheets(&tysh, &layer, &styles, 72.0).expect("write");

    let r = read_style_sheets(&out, 72.0).expect("read");
    assert_eq!(r.char_sheets.len(), 3);
    assert_eq!(r.char_sheets.iter().map(|(_, n, _)| n.as_str()).collect::<Vec<_>>(), vec!["One", "Three", "Seven"]);
    assert_eq!(r.para_sheets.len(), 2);
    assert_eq!(r.run_char, vec![None, Some(1), Some(2)]);
    assert_eq!(r.run_para, vec![Some(1)]);
}

#[test]
fn dpi_scales_size_points_with_ratio() {
    let mut styles = TextStyles::default();
    styles.character.push(char_def(10, "Scaled", CharStyle { size_pt: 30.0, ..Default::default() }));

    let base = CharStyle { font_family: DEFAULT_FAMILY.into(), ..Default::default() };
    let layer = make_layer("ab", vec![TextRun { len: 1, style: base }, TextRun { len: 1, style: styles.resolve_char(None, Some(10), DEFAULT_FAMILY) }], vec![]);

    let tysh = psd::build_tysh(&layer, 72.0, None);
    // Write at 144 dpi: stored values are scaled.
    let out = write_style_sheets(&tysh, &layer, &styles, 144.0).expect("write");

    // Reading with the same dpi recovers the original size.
    let at144 = read_style_sheets(&out, 144.0).expect("read 144");
    assert!((at144.char_sheets[0].2.size_pt - 30.0).abs() < 0.01);

    // Reading with a different dpi scales the size by dpi_write / dpi_read.
    let at72 = read_style_sheets(&out, 72.0).expect("read 72");
    let expected = 30.0 * (144.0 / 72.0); // 60.0
    assert!((at72.char_sheets[0].2.size_pt - expected).abs() < 0.01);
}

#[test]
fn non_finite_dpi_is_safe() {
    let mut styles = TextStyles::default();
    styles.character.push(char_def(1, "X", CharStyle { size_pt: 18.0, ..Default::default() }));

    let base = CharStyle { font_family: DEFAULT_FAMILY.into(), ..Default::default() };
    let layer = make_layer("ab", vec![TextRun { len: 1, style: base }, TextRun { len: 1, style: styles.resolve_char(None, Some(1), DEFAULT_FAMILY) }], vec![]);

    let tysh = psd::build_tysh(&layer, 72.0, None);
    for dpi in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 0.0, -5.0] {
        let out = write_style_sheets(&tysh, &layer, &styles, dpi);
        assert!(out.is_some());
        if let Some(bytes) = out {
            let r = read_style_sheets(&bytes, dpi);
            assert!(r.is_some());
        }
    }
}

#[test]
fn write_style_sheets_is_deterministic() {
    let mut styles = TextStyles::default();
    styles.character.push(char_def(2, "Det", CharStyle { size_pt: 15.0, ..Default::default() }));

    let base = CharStyle { font_family: DEFAULT_FAMILY.into(), ..Default::default() };
    let layer = make_layer("ab", vec![TextRun { len: 1, style: base }, TextRun { len: 1, style: styles.resolve_char(None, Some(2), DEFAULT_FAMILY) }], vec![]);

    let tysh = psd::build_tysh(&layer, 72.0, None);
    let a = write_style_sheets(&tysh, &layer, &styles, 72.0).expect("write a");
    let b = write_style_sheets(&tysh, &layer, &styles, 72.0).expect("write b");
    assert_eq!(a, b);
}

#[test]
fn has_named_sheets_detects_character_or_paragraph() {
    let mut styles = TextStyles::default();
    styles.character.push(char_def(1, "C", CharStyle::default()));

    let layer = make_layer("a", vec![TextRun { len: 1, style: CharStyle::default() }], vec![]);
    let tysh = psd::build_tysh(&layer, 72.0, None);
    let out = write_style_sheets(&tysh, &layer, &styles, 72.0).expect("write char");
    assert!(has_named_sheets(&out));

    styles.character.clear();
    styles.paragraph.push(para_def(2, "P", ParagraphStyle::default()));
    let out2 = write_style_sheets(&tysh, &layer, &styles, 72.0).expect("write para");
    assert!(has_named_sheets(&out2));
}

#[test]
fn run_char_lens_are_utf8_byte_lengths() {
    let mut styles = TextStyles::default();
    styles.character.push(char_def(1, "Accent", CharStyle { size_pt: 10.0, ..Default::default() }));

    let base = CharStyle { font_family: DEFAULT_FAMILY.into(), ..Default::default() };
    let styled = styles.resolve_char(None, Some(1), DEFAULT_FAMILY);
    let text = "éa";
    let layer = make_layer(text, vec![TextRun { len: 2, style: base }, TextRun { len: 1, style: styled }], vec![]);

    let tysh = psd::build_tysh(&layer, 72.0, None);
    let out = write_style_sheets(&tysh, &layer, &styles, 72.0).expect("write");
    let r = read_style_sheets(&out, 72.0).expect("read");

    // Run lengths are UTF-8 byte lengths ("é" = 2). Photoshop's engine text always ends with a
    // paragraph break ("\r", 1 byte) and the last run covers it, so "a" + "\r" = 2 bytes.
    assert_eq!(r.run_char_lens, vec![2, 2]);
    assert_eq!(r.run_char, vec![None, Some(1)]);
}

#[test]
fn normal_style_is_resolved() {
    let base = CharStyle { font_family: DEFAULT_FAMILY.into(), size_pt: 22.0, ..Default::default() };
    let layer = make_layer(
        "abc",
        vec![TextRun { len: 3, style: base }],
        vec![ParagraphRun { len: 3, style: ParagraphStyle { align: TextAlign::Right, ..Default::default() } }],
    );

    let tysh = psd::build_tysh(&layer, 72.0, None);
    let r = read_style_sheets(&tysh, 72.0).expect("read");

    assert!(r.normal_char.size_pt.is_finite() && r.normal_char.size_pt > 0.0);
    assert!(!r.normal_char.font_family.is_empty());
}
