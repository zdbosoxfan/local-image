//! Frozen pre-J18 editor histogram painter, test-only parity oracle.
//! Camera Raw-style continuous RGB ribbons. Raw bins remain exact; display filtering is local.
use crate::theme::Tokens;
use egui::{Mesh, Rect, pos2};
const BINS: usize = 256;

/// A five-bin binomial display kernel softens quantization combs. It cannot bridge
/// a wide empty range. Clipping bins are pinned and excluded from filtering adjacent bins.
fn curves(h: &[[u64; BINS]; 3]) -> [[f32; BINS]; 3] {
    let mut curves = [[0.0; BINS]; 3];
    for (out, bins) in curves.iter_mut().zip(h) {
        for (i, value) in out.iter_mut().enumerate() {
            let center = bins.get(i).copied().unwrap_or(0) as f32;
            if i == 0 || i == BINS - 1 {
                *value = center;
            } else {
                let sample = |j: usize| bins.get(j.clamp(1, BINS - 2)).copied().unwrap_or(0) as f32;
                *value = (sample(i.saturating_sub(2)) + 4.0 * sample(i - 1) + 6.0 * center + 4.0 * sample(i + 1) + sample(i + 2)) / 16.0;
            }
        }
    }
    // One scale for RGB. Edge clipping spikes must not flatten all interior tonal detail;
    // preserve them at the edges, capped at the top. A flat black/white image uses its edge peak.
    let interior = curves.iter().flat_map(|c| c.iter().skip(1).take(BINS - 2)).copied().fold(0.0f32, f32::max);
    let peak = if interior > 0.0 { interior } else { curves.iter().flatten().copied().fold(0.0f32, f32::max) };
    let scale = if peak > 0.0 { peak * 1.1 } else { 1.0 };
    for value in curves.iter_mut().flatten() {
        *value = (*value / scale).min(1.0);
    }
    curves
}

fn quad(mesh: &mut Mesh, rect: Rect, x: [f32; 2], bottom: [f32; 2], top: [f32; 2], color: egui::Color32) {
    let vertex = mesh.vertices.len() as u32; // Bounded: at most 257 spans × 4 cuts × 3 bands × 4 vertices.
    for (x, height) in [(x[0], bottom[0]), (x[0], top[0]), (x[1], top[1]), (x[1], bottom[1])] {
        mesh.colored_vertex(pos2(x, rect.bottom() - height * rect.height()), color);
    }
    mesh.add_triangle(vertex, vertex + 1, vertex + 2);
    mesh.add_triangle(vertex, vertex + 2, vertex + 3);
}

/// Split where two RGB curves cross, so every subspan has a stable channel order. This gives
/// exact overlap colours without draw-order-dependent alpha stacking or cracks between bars.
fn span(mesh: &mut Mesh, rect: Rect, x: [f32; 2], a: [f32; 3], b: [f32; 3], t: &Tokens) {
    let mut cuts = [0.0f32, 1.0, 1.0, 1.0, 1.0];
    let mut n = 2;
    for (i, (ai, bi)) in a.iter().zip(&b).enumerate() {
        for (aj, bj) in a.iter().zip(&b).skip(i + 1) {
            let denominator = (bi - ai) - (bj - aj);
            if denominator.abs() > f32::EPSILON {
                let crossing = (aj - ai) / denominator;
                if crossing > 0.0
                    && crossing < 1.0
                    && let Some(cut) = cuts.get_mut(n)
                {
                    *cut = crossing;
                    n += 1;
                }
            }
        }
    }
    let Some(cuts) = cuts.get_mut(..n) else { return };
    cuts.sort_unstable_by(f32::total_cmp);
    for cut in cuts.windows(2) {
        let (start, end) = (cut[0], cut[1]);
        if end <= start {
            continue;
        }
        let left = std::array::from_fn::<_, 3, _>(|c| a[c] + (b[c] - a[c]) * start);
        let right = std::array::from_fn::<_, 3, _>(|c| a[c] + (b[c] - a[c]) * end);
        let mut levels = [(0usize, 1u8), (1, 2), (2, 4)];
        levels.sort_unstable_by(|(i, _), (j, _)| (left[*i] + right[*i]).total_cmp(&(left[*j] + right[*j])));
        let x = [x[0] + (x[1] - x[0]) * start, x[0] + (x[1] - x[0]) * end];
        let mut floor = [0.0; 2];
        let mut mask = 7;
        for (channel, bit) in levels {
            let top = [left[channel], right[channel]];
            if top[0] > floor[0] || top[1] > floor[1] {
                quad(mesh, rect, x, floor, top, t.histogram_fill(mask));
            }
            floor = top;
            mask &= !bit;
        }
    }
}

fn fill(rect: Rect, curves: &[[f32; BINS]; 3], t: &Tokens) -> Mesh {
    let mut mesh = Mesh::default();
    if !rect.is_finite() || !rect.is_positive() || !rect.size().is_finite() {
        return mesh;
    }
    let mut previous_x = rect.left();
    let mut previous = curves.map(|channel| channel[0]);
    for (i, ((r, g), b)) in curves[0].iter().zip(&curves[1]).zip(&curves[2]).enumerate() {
        let x = rect.left() + (i as f32 + 0.5) / BINS as f32 * rect.width();
        let next = [*r, *g, *b];
        span(&mut mesh, rect, [previous_x, x], previous, next, t);
        previous_x = x;
        previous = next;
    }
    span(&mut mesh, rect, [previous_x, rect.right()], previous, previous, t);
    mesh
}

pub(crate) fn paint(painter: &egui::Painter, rect: Rect, h: &[[u64; BINS]; 3], t: &Tokens) {
    if !rect.is_finite() || !rect.is_positive() || !rect.size().is_finite() {
        return;
    }
    let curves = curves(h);
    painter.add(fill(rect, &curves, t));
    for (channel, values) in curves.iter().enumerate() {
        if !values.iter().any(|&v| v > 0.0) {
            continue;
        }
        let mut points = Vec::with_capacity(BINS + 2);
        points.push(pos2(rect.left(), rect.bottom() - values[0] * rect.height()));
        for (i, v) in values.iter().enumerate() {
            points.push(pos2(rect.left() + (i as f32 + 0.5) / BINS as f32 * rect.width(), rect.bottom() - v * rect.height()));
        }
        points.push(pos2(rect.right(), rect.bottom() - values[BINS - 1] * rect.height()));
        painter.add(egui::Shape::line(points, egui::Stroke::new(1.0, t.histogram_color(1 << channel))));
    }
}
