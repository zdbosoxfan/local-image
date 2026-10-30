//! Bounds on the number of segment pairs supplied to a planar sweep.

use kurbo::{BezPath, ParamCurveExtrema, Rect};

pub(crate) const MAX_SEGMENT_PAIRS: usize = 1 << 22;

struct Counts(Vec<isize>);

impl Counts {
    fn add(&mut self, slot: usize, delta: isize) {
        let mut i = slot + 1;
        while let Some(count) = self.0.get_mut(i) {
            *count += delta;
            i += i & i.wrapping_neg();
        }
    }

    fn before(&self, end: usize) -> isize {
        let mut i = end.min(self.0.len().saturating_sub(1));
        let mut sum = 0;
        while i > 0 {
            sum += self.0[i];
            i -= i & i.wrapping_neg();
        }
        sum
    }
}

/// Whether at most `cap` pairs of bounds overlap, counting touching edges.
pub(crate) fn overlaps_within(boxes: &[Rect], cap: usize) -> bool {
    let boxes: Vec<Rect> = boxes.iter().map(|b| Rect::new(b.x0 + 0.0, b.y0 + 0.0, b.x1 + 0.0, b.y1 + 0.0)).collect();
    let mut ys: Vec<f64> = boxes.iter().flat_map(|b| [b.y0, b.y1]).collect();
    ys.sort_by(f64::total_cmp);
    ys.dedup_by(|a, b| a.total_cmp(b).is_eq());
    let below = |y: f64| ys.partition_point(|v| v.total_cmp(&y).is_lt());
    let upto = |y: f64| ys.partition_point(|v| v.total_cmp(&y).is_le());
    let mut events: Vec<(f64, bool, usize)> = boxes.iter().enumerate().flat_map(|(i, b)| [(b.x0, false, i), (b.x1, true, i)]).collect();
    events.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    let mut by_y0 = Counts(vec![0; ys.len() + 1]);
    let mut by_y1 = Counts(vec![0; ys.len() + 1]);
    let (mut active, mut overlaps) = (0isize, 0usize);
    for (_, end, i) in events {
        let b = &boxes[i];
        if end {
            by_y0.add(below(b.y0), -1);
            by_y1.add(below(b.y1), -1);
            active -= 1;
            continue;
        }
        let above = active - by_y0.before(upto(b.y1));
        let under = by_y1.before(below(b.y0));
        let Ok(overlapping) = usize::try_from(active - above - under) else {
            return false;
        };
        overlaps = overlaps.saturating_add(overlapping);
        if overlaps > cap {
            return false;
        }
        by_y0.add(below(b.y0), 1);
        by_y1.add(below(b.y1), 1);
        active += 1;
    }
    true
}

pub(crate) fn sweep_fits<'a>(paths: impl IntoIterator<Item = &'a BezPath>) -> bool {
    let boxes: Vec<Rect> = paths.into_iter().flat_map(|path| path.segments().map(|segment| segment.bounding_box())).collect();
    overlaps_within(&boxes, MAX_SEGMENT_PAIRS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlapping_bounds_count_segment_pairs() {
        let bars = |n: usize| -> Vec<Rect> {
            let across = (0..n).map(|i| Rect::new(0.0, i as f64 * 2.0, 100.0, i as f64 * 2.0 + 1.0));
            let down = (0..n).map(|i| Rect::new(i as f64 * 2.0, 0.0, i as f64 * 2.0 + 1.0, 100.0));
            across.chain(down).collect()
        };
        assert!(overlaps_within(&bars(10), 100));
        assert!(!overlaps_within(&bars(10), 99));

        let apart: Vec<Rect> = (0..10_000).map(|i| Rect::new(i as f64 * 3.0, 0.0, i as f64 * 3.0 + 1.0, 1.0)).collect();
        assert!(overlaps_within(&apart, 0));
    }

    #[test]
    fn touching_at_signed_zero_counts_as_overlap() {
        let below = Rect::new(0.0, -1.0, 1.0, -0.0);
        let above = Rect::new(0.0, 0.0, 1.0, 1.0);
        assert!(!overlaps_within(&[below, above], 0));
        assert!(overlaps_within(&[below, above], 1));
    }
}
