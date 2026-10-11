#![allow(clippy::unwrap_used, clippy::panic)]
use super::super::colours::tests::test_access;
use super::*;
use photocraft_doc::{ColorMode, Document, Layer, LayerContent, Path, SampleType, ShapeLayer, Size, Subpath};
fn fixture() -> (Session, Vec<LayerId>) {
    let mut d = Document::new("Twelve colours", Size::new(144, 24), ColorMode::Rgb, SampleType::U8);
    let mut ids = Vec::new();
    for h in 0..3 {
        for i in 0..4 {
            let v = 0.4 + i as f32 * 0.15;
            let mut rgb = [0.05; 3];
            rgb[h] = v;
            let x = ((h * 4 + i) * 12) as f64;
            let mut sh = ShapeLayer {
                path: Path::new(vec![Subpath::polygon(&[(x, 0.), (x + 10., 0.), (x + 10., 20.), (x, 20.)])]),
                fill: Some(Fill::Solid(Color::rgb(rgb[0], rgb[1], rgb[2]))),
                ..Default::default()
            };
            crate::vector_cmds::refresh_shape(&d, &mut sh);
            let l = Layer::new("Colour", LayerContent::Shape(sh));
            ids.push(l.id);
            d.layers.push(l);
        }
    }
    let mut s = Session::new();
    s.add_document(d, None);
    (s, ids)
}
#[test]
fn colour39_recolor_twelve_to_three_seeded_rows_preview_one_undo_group() {
    let (mut s, ids) = fixture();
    let before = s.active().unwrap().doc.clone();
    let mut p = begin(&s, test_access(), &ids).unwrap();
    assert_eq!(p.colors.len(), 12);
    let mut again = p.clone();
    p.reduce(3, 7).unwrap();
    again.reduce(3, 7).unwrap();
    assert_eq!(p.rows, again.rows);
    assert_eq!(p.rows.len(), 3);
    // Independent input families: each row contains all four shades of one primary hue.
    let mut groups = p
        .rows
        .iter()
        .map(|r| {
            let mut ix = r.from.clone();
            ix.sort();
            ix
        })
        .collect::<Vec<_>>();
    groups.sort();
    assert_eq!(groups, vec![vec![0, 1, 2, 3], vec![4, 5, 6, 7], vec![8, 9, 10, 11]]);
    for (i, row) in p.rows.iter_mut().enumerate() {
        row.to = Color::rgb(i as f32 / 2., 0.3, 0.8);
    }
    p.save_group = Some("Results".into());
    let shown = prepare(&s, test_access(), &p).unwrap();
    assert_eq!(s.active().unwrap().doc, before);
    assert!(!s.active().unwrap().history.can_undo());
    assert_eq!(shown.book.groups.len(), 1);
    assert_eq!(shown.book.swatches.len(), 3);
    for row in &p.rows {
        for i in &row.from {
            assert_eq!(colours::color(&shown.document, &p.colors[*i].targets[0]).unwrap(), row.to);
        }
    }
    commit(&mut s, test_access(), &p).unwrap();
    assert_eq!(*s.active().unwrap().doc, shown.document);
    assert_eq!(s.active().unwrap().history.past_len(), 1);
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc, before);
    assert!(!s.undo());
}
#[test]
fn colour39_recolor_lightness_independent_cie_l_oracle_and_alpha() {
    let sources =
        [Color::rgb(1., 0., 0.), Color::rgb(0., 1., 0.), Color::rgb(0., 0., 1.), Color::gray(0.05), Color::gray(0.95), Color::rgba(0.6, 0.2, 0.8, 0.3)];
    // Independent CIE L* oracle from linear sRGB->D50 Y coefficients, without a/b matrices.
    let lightness = |c: Color| {
        let [r, g, b] = c.to_rgb().map(photocraft_color::convert::srgb_to_linear);
        let y = 0.2225045 * r + 0.7168786 * g + 0.0606169 * b;
        if y > 216. / 24389. { 116. * y.cbrt() - 16. } else { 24389. / 27. * y }
    };
    for source in sources {
        for to in [Color::rgb(1., 1., 0.), Color::rgb(0., 0., 1.), Color::BLACK, Color::WHITE] {
            let mapped = preserve_lightness(source, to);
            assert!((lightness(mapped) - lightness(source)).abs() <= 1., "{source:?} -> {mapped:?}");
            assert_eq!(mapped.alpha, source.alpha);
        }
    }
}
#[test]
fn colour39_recolor_global_tints_unselected_art_and_randomizers() {
    let (s, book) = super::super::colours::tests::fixture();
    let id = book.links[0].target.layer;
    let mut plan = begin(&s, test_access(), &[id]).unwrap();
    plan.rows[0].to = Color::rgb(1., 0., 0.);
    plan.method = Method::PreserveTints;
    let preview = prepare(&s, test_access(), &plan).unwrap();
    assert_eq!(colours::color(&preview.document, &book.links[0].target).unwrap(), Color::rgb(1., 0.5, 0.5));
    assert_eq!(colours::color(&preview.document, &book.links[1].target).unwrap(), Color::rgb(0.2, 0.4, 0.6));
    let (s, ids) = fixture();
    let mut plan = begin(&s, test_access(), &ids).unwrap();
    plan.reduce(3, 9).unwrap();
    let mut a = plan.clone();
    let mut b = plan.clone();
    a.randomize_order(42);
    b.randomize_order(42);
    assert_eq!(a.rows, b.rows);
    a.randomize_saturation_brightness(11);
    b.randomize_saturation_brightness(11);
    assert_eq!(a.rows, b.rows);
    assert_ne!(a.rows, plan.rows);
    assert!(plan.reduce(0, 1).is_err());
}
#[test]
fn colour39_recolor_rejects_overlapping_rows_missing_target_and_stale_plan() {
    let (mut s, ids) = fixture();
    let mut p = begin(&s, test_access(), &ids).unwrap();
    let before = s.active().unwrap().doc.clone();
    p.rows[1].from = p.rows[0].from.clone();
    assert!(commit(&mut s, test_access(), &p).is_err());
    assert_eq!(s.active().unwrap().doc, before);
    let mut p = begin(&s, test_access(), &ids).unwrap();
    p.colors.last_mut().unwrap().targets[0].layer = LayerId(u64::MAX);
    assert!(commit(&mut s, test_access(), &p).is_err());
    assert_eq!(s.active().unwrap().doc, before);
    let p = begin(&s, test_access(), &ids).unwrap();
    s.edit("Other", |d, _| {
        d.name = "Other".into();
        Ok(())
    })
    .unwrap();
    assert!(prepare(&s, test_access(), &p).is_err());
}
