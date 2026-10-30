//! Adapted from VectorCraft `crates/pathops/src/budget.rs` at caf10cc5816d5f26bee02837650ce6952b69b878.
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! SPDX-License-Identifier: MIT OR Apache-2.0
//! See licenses/vectorcraft-{LICENSE-MIT,LICENSE-APACHE,NOTICE}.
//!
//! Bounds on the number of segment pairs supplied to planar sweeps, per sweep and per operation.

use crate::PathOpsError;
use kurbo::{BezPath, ParamCurveExtrema, Rect};
use std::{cell::Cell, marker::PhantomData};

const MAX_SEGMENT_PAIRS: usize = 1 << 22;
/// Many components, or a chain of booleans over a growing result, pass the sweep limit one sweep
/// at a time. The cost that matters to the user is their sum.
const MAX_OPERATION_PAIRS: usize = 4 * MAX_SEGMENT_PAIRS;

thread_local! {
    /// Pairs charged to the operation open on this thread. Past the limit, it stays refused.
    static OPERATION_PAIRS: Cell<Option<usize>> = const { Cell::new(None) };
    static REFUSED_SWEEPS: Cell<u64> = const { Cell::new(0) };
}

/// One budget shared by every sweep this thread runs until the value is dropped. Each `begin`
/// inside an open operation joins it, so a path operation made of smaller ones is charged once.
pub struct WorkBudget {
    outermost: bool,
    charges_this_thread: PhantomData<*const ()>,
}

impl WorkBudget {
    pub fn begin() -> Self {
        let outermost = OPERATION_PAIRS.get().is_none();
        if outermost {
            OPERATION_PAIRS.set(Some(0));
        }
        Self { outermost, charges_this_thread: PhantomData }
    }
}

impl Drop for WorkBudget {
    fn drop(&mut self) {
        if self.outermost {
            OPERATION_PAIRS.set(None);
        }
    }
}

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

/// Notices sweeps refused on this thread, for callers whose errors reach them only as text.
/// Unlike [`WorkBudget`], it shares nothing: the operations it watches keep their own budgets.
pub struct RefusalWatch {
    refused_before: u64,
}

impl RefusalWatch {
    pub fn begin() -> Self {
        Self { refused_before: REFUSED_SWEEPS.get() }
    }

    pub fn refused(&self) -> bool {
        REFUSED_SWEEPS.get() != self.refused_before
    }
}

/// How many pairs of bounds overlap, counting touching edges, if at most `cap` do.
fn overlaps_within(boxes: &[Rect], cap: usize) -> Option<usize> {
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
        overlaps = overlaps.saturating_add((active - above - under).unsigned_abs());
        if overlaps > cap {
            return None;
        }
        by_y0.add(below(b.y0), 1);
        by_y1.add(below(b.y1), 1);
        active += 1;
    }
    Some(overlaps)
}

/// Charge one sweep of `paths` to the open operation, or refuse it.
pub(crate) fn charge_sweep<'a>(paths: impl IntoIterator<Item = &'a BezPath>) -> Result<(), PathOpsError> {
    let boxes: Vec<Rect> = paths.into_iter().flat_map(|path| path.segments().map(|segment| segment.bounding_box())).collect();
    let pairs = overlaps_within(&boxes, MAX_SEGMENT_PAIRS).unwrap_or(MAX_OPERATION_PAIRS + 1);
    let spent = OPERATION_PAIRS.get().unwrap_or(0).saturating_add(pairs);
    OPERATION_PAIRS.set(OPERATION_PAIRS.get().map(|_| spent));
    if spent > MAX_OPERATION_PAIRS {
        REFUSED_SWEEPS.set(REFUSED_SWEEPS.get().wrapping_add(1));
        return Err(PathOpsError::TooComplex);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PathOpsError;
    use photocraft_doc::{Path, PathOp, Subpath};

    #[test]
    fn overlapping_bounds_count_segment_pairs() {
        let bars = |n: usize| -> Vec<Rect> {
            let across = (0..n).map(|i| Rect::new(0.0, i as f64 * 2.0, 100.0, i as f64 * 2.0 + 1.0));
            let down = (0..n).map(|i| Rect::new(i as f64 * 2.0, 0.0, i as f64 * 2.0 + 1.0, 100.0));
            across.chain(down).collect()
        };
        assert_eq!(overlaps_within(&bars(10), 100), Some(100));
        assert_eq!(overlaps_within(&bars(10), 99), None);

        let apart: Vec<Rect> = (0..10_000).map(|i| Rect::new(i as f64 * 3.0, 0.0, i as f64 * 3.0 + 1.0, 1.0)).collect();
        assert_eq!(overlaps_within(&apart, 0), Some(0));
    }

    #[test]
    fn touching_at_signed_zero_counts_as_overlap() {
        let below = Rect::new(0.0, -1.0, 1.0, -0.0);
        let above = Rect::new(0.0, 0.0, 1.0, 1.0);
        assert_eq!(overlaps_within(&[below, above], 0), None);
        assert_eq!(overlaps_within(&[below, above], 1), Some(1));
    }

    /// 2048 lines crossing 2048 others: exactly the sweep limit.
    fn grid() -> BezPath {
        let mut grid = BezPath::new();
        for i in 0..2048 {
            let along = f64::from(i) * 2.0;
            grid.move_to((0.0, along));
            grid.line_to((8192.0, along));
            grid.move_to((along, 0.0));
            grid.line_to((along, 8192.0));
        }
        grid
    }

    #[test]
    fn an_operation_spends_four_sweep_limits_then_stays_refused() {
        let grid = grid();
        let watch = RefusalWatch::begin();
        {
            let _operation = WorkBudget::begin();
            for _ in 0..3 {
                let _joined = WorkBudget::begin();
                assert_eq!(charge_sweep([&grid]), Ok(()));
            }
            assert_eq!(charge_sweep([&grid]), Ok(()));
            assert!(!watch.refused());

            assert_eq!(charge_sweep([&grid]), Err(PathOpsError::TooComplex));
            assert!(watch.refused());
            assert_eq!(charge_sweep([&BezPath::new()]), Err(PathOpsError::TooComplex));
        }
        let later = RefusalWatch::begin();
        let _next_operation = WorkBudget::begin();
        assert_eq!(charge_sweep([&grid]), Ok(()));
        assert!(!later.refused());
    }

    /// `strips` thin parallel diagonals whose bounds all overlap, far from any other `band`.
    fn diagonal_strips(band: usize, strips: usize) -> Vec<Subpath> {
        let left = band as f64 * 5000.0;
        (0..strips)
            .map(|i| {
                let x = left + i as f64;
                Subpath::polygon(&[(x, 0.0), (x + 0.5, 0.0), (x + 1000.5, 1000.0), (x + 1000.0, 1000.0)]).with_op(if i == 0 {
                    PathOp::Combine
                } else {
                    PathOp::Join
                })
            })
            .collect()
    }

    #[test]
    fn the_sweeps_of_one_path_operation_share_a_budget() {
        let bands = Path::new((0..8).flat_map(|band| diagonal_strips(band, 350)).collect());
        assert_eq!(crate::finish_compound(&bands, None), Err(PathOpsError::TooComplex));

        let band = Path::new(diagonal_strips(0, 350));
        assert_eq!(crate::finish_compound(&band, None).map(|path| path.subpaths.len()), Ok(350));
    }
}
