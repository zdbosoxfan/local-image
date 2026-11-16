//! Stroke cost limit: the dab-area budget.

use super::*;

fn brush(size: f32, spacing: f32) -> BrushSettings {
    BrushSettings { size, spacing, hardness: 0.5, ..BrushSettings::default() }
}

fn line(r: &mut StrokeRenderer, from: f64, to: f64) {
    r.push(&[StrokePoint::new(from, 50.0, 1.0), StrokePoint::new(to, 50.0, 1.0)]);
    r.finish();
}

#[test]
fn the_budget_stops_painting_but_keeps_counting_dabs() {
    for b in [brush(20.0, 0.25), BrushSettings { hardness: 1.0, ..brush(20.0, 0.25) }] {
        // A 20 px brush has a dab rectangle of about 30x30; room for about ten of them.
        let mut r = StrokeRenderer::new(&b, None, 1.0).with_dab_area_budget(10_000);
        line(&mut r, 0.0, 2_000.0);
        assert!(r.budget_exhausted());
        assert!(r.dab_count() > 300, "{}", r.dab_count());
        assert!(r.coverage_at(1_000, 50) == 0.0);
        assert!(r.bounds().x1 < 200, "{:?}", r.bounds());
        // The first dabs did paint.
        assert!(r.coverage_at(2, 50) > 0.0);
        // Further input is ignored: no growth.
        let tiles = r.cov.tiles.len();
        line(&mut r, 3_000.0, 4_000.0);
        assert_eq!(r.cov.tiles.len(), tiles);
    }
}

#[test]
fn dual_dabs_are_charged_apart_and_chunking_does_not_change_the_result() {
    let mut b = brush(20.0, 0.25);
    b.dual_brush.enabled = true;
    b.dual_brush.size = 10.0;
    let pts: Vec<_> = (0..200).map(|i| StrokePoint::new(f64::from(i) * 7.0, 50.0, 1.0)).collect();
    let run = |chunk: usize| {
        let mut r = StrokeRenderer::new(&b, None, 1.0).with_dab_area_budget(20_000);
        for c in pts.chunks(chunk) {
            r.push(c);
        }
        r.finish();
        r
    };
    let (whole, chunked) = (run(1000), run(3));
    assert!(whole.budget_exhausted());
    for x in 0..1400 {
        assert_eq!(whole.coverage_at(x, 50).to_bits(), chunked.coverage_at(x, 50).to_bits(), "x {x}");
        assert_eq!(whole.dual.as_ref().unwrap().get(x, 50).to_bits(), chunked.dual.as_ref().unwrap().get(x, 50).to_bits(), "dual x {x}");
    }
    assert!(whole.dual.as_ref().unwrap().get(2, 50) > 0.0);
}

#[test]
fn a_dab_too_big_for_what_is_left_does_not_end_the_stroke() {
    let (mut left, mut refused) = (1000, false);
    assert!(!admit(&mut left, &mut refused, Rect::new(0, 0, 100, 100)));
    assert!(refused);
    assert_eq!(left, 1000);
    assert!(admit(&mut left, &mut refused, Rect::new(0, 0, 10, 10)));
    assert_eq!(left, 900);
}
