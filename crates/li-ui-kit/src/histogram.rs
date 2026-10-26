//! Camera Raw-style continuous RGB ribbons. Raw bins remain exact; display filtering is local.
use crate::theme::Tokens;
use egui::{Mesh, Rect, pos2};
/// Display work is bounded by these bins, independent of the source image size.
pub const BINS: usize = 256;

/// Borrow a Library preview histogram (`u32` bins) or the exact RGB histogram (`u64`) without copying bins.
#[derive(Clone, Copy)]
pub enum Bins<'a> {
    U32(&'a [u32]),
    U64(&'a [u64]),
}
impl Bins<'_> {
    fn at(self, index: usize) -> f32 {
        match self {
            Self::U32(bins) => bins.get(index).copied().unwrap_or(0) as f32,
            Self::U64(bins) => bins.get(index).copied().unwrap_or(0) as f32,
        }
    }
}

/// A zero-copy histogram view. Missing bins display as zero; extras are ignored.
#[derive(Clone, Copy)]
pub struct Histogram<'a> {
    pub channels: [Bins<'a>; 3],
    pub luma: Option<Bins<'a>>,
}
impl<'a> Histogram<'a> {
    pub fn u64(channels: &'a [[u64; BINS]; 3], luma: Option<&'a [u64]>) -> Self {
        Self { channels: channels.each_ref().map(|bins| Bins::U64(bins)), luma: luma.map(Bins::U64) }
    }
    pub fn u32(channels: [&'a [u32]; 3], luma: Option<&'a [u32]>) -> Self {
        Self { channels: channels.map(Bins::U32), luma: luma.map(Bins::U32) }
    }
}

/// Shared editor histogram shell. Uses exact RGB ribbons and retains its inherited scope style.
pub fn editor(ui: &mut egui::Ui, h: Histogram<'_>, height: f32, label: &str) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width().max(1.0), height.max(1.0)), egui::Sense::hover());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, ui.is_enabled(), label));
    ui.painter().rect_filled(rect, t.radius_sm, t.histogram_background());
    paint_rgb(ui.painter(), rect.shrink(EDITOR_INSET), h, &t);
    ui.painter().rect_stroke(rect, t.radius_sm, egui::Stroke::new(1.0, t.field_border), egui::StrokeKind::Inside);
    response
}

pub const EDITOR_INSET: f32 = 4.0;
pub const LIBRARY_HEIGHT: f32 = 118.0;
pub const LIBRARY_PLOT_TOP: f32 = 6.0;
pub const LIBRARY_METADATA_HEIGHT: f32 = 26.0;
pub const CLIP_INDICATOR_INSET: f32 = 12.0;
pub const CLIP_INDICATOR_TOP: f32 = 10.0;
pub const CLIP_INDICATOR_SIZE: egui::Vec2 = egui::vec2(14.0, 12.0);
pub const CLIP_INDICATOR_RADIUS: f32 = 2.0;
pub const CLIP_THRESHOLD: f32 = 0.003;
pub const LIBRARY_LINE_WIDTH: f32 = 1.5;

/// Library histogram shell, including the accessible clipping toggle. The caller retains metadata
/// and clipping semantics; preview histograms still use display-bin endpoint counts.
pub fn library(ui: &mut egui::Ui, h: Option<Histogram<'_>>, clipping: (f32, f32), enabled: bool, label: &str) -> egui::Response {
    let response = library_shell(ui, enabled, label);
    paint_library_contents(ui, response.rect, h, clipping, enabled);
    response
}

/// Allocate the Library shell before a caller paints tonal-zone feedback behind the curves.
pub fn library_shell(ui: &mut egui::Ui, enabled: bool, label: &str) -> egui::Response {
    let tokens = crate::library::get(ui.ctx());
    let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width().max(1.0), LIBRARY_HEIGHT), egui::Sense::click_and_drag());
    response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, ui.is_enabled(), enabled, label));
    ui.painter().rect_filled(rect, 0.0, tokens.library.button);
    response
}

pub fn library_plot_rect(rect: Rect) -> Rect {
    Rect::from_min_max(rect.min + egui::vec2(0.0, LIBRARY_PLOT_TOP), pos2(rect.right(), rect.bottom() - LIBRARY_METADATA_HEIGHT))
}

pub fn library_clipping_rects(plot: Rect) -> [Rect; 2] {
    [plot.left() + CLIP_INDICATOR_INSET, plot.right() - CLIP_INDICATOR_INSET]
        .map(|x| Rect::from_center_size(pos2(x, plot.top() + CLIP_INDICATOR_TOP), CLIP_INDICATOR_SIZE))
}

/// Paint the borrowed histogram and endpoint indicators into an allocated Library shell.
pub fn paint_library_contents(ui: &egui::Ui, rect: Rect, h: Option<Histogram<'_>>, clipping: (f32, f32), enabled: bool) {
    let tokens = crate::library::get(ui.ctx());
    let painter = ui.painter();
    let plot = library_plot_rect(rect);
    if let Some(h) = h {
        paint_library(painter, plot, h, &tokens);
        for (clipped, indicator) in [clipping.0 > CLIP_THRESHOLD, clipping.1 > CLIP_THRESHOLD].into_iter().zip(library_clipping_rects(plot)) {
            painter.rect_filled(indicator, CLIP_INDICATOR_RADIUS, if enabled { tokens.hover } else { tokens.histogram_clip_background() });
            paint_indicator(painter, indicator.shrink(CLIP_INDICATOR_RADIUS), tokens.histogram_clip_color(clipped));
        }
    }
}

fn paint_indicator(p: &egui::Painter, rect: Rect, color: egui::Color32) {
    let side = rect.width().min(rect.height());
    let ppp = p.ctx().pixels_per_point();
    let px = (side * ppp).round().clamp(1.0, 512.0) as u32;
    let svg = crate::library_icons::svg(crate::library_icons::Icon::ChevronDown);
    if let Some(tex) = crate::icons::texture(p.ctx(), svg, crate::icons::StrokeStyle::Interface, px, egui::Color32::WHITE, false) {
        let r = Rect::from_center_size(rect.center(), egui::Vec2::splat(px as f32 / ppp));
        p.image(tex, r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), color);
    }
}

fn smooth(bins: Bins<'_>) -> [f32; BINS] {
    std::array::from_fn(|i: usize| {
        let (a, b) = (i.max(2) - 2, (i + 3).min(BINS));
        (a..b).map(|index| bins.at(index)).sum::<f32>() / (b - a) as f32
    })
}

/// Library profile: inherited square-root display scale, luma fill and neutral greyscale trace.
/// Only display curves are filtered. Histogram/clipping statistics are never changed.
pub fn paint_library(p: &egui::Painter, plot: Rect, h: Histogram<'_>, t: &Tokens) {
    if !plot.is_finite() || !plot.is_positive() || !plot.size().is_finite() {
        return;
    }
    let chans = h.channels.map(smooth);
    let peak = chans.iter().flat_map(|v| v.iter().skip(2).take(BINS - 4)).fold(1.0f32, |a, b| a.max(*b));
    let to = |i: usize, v: f32| pos2(plot.left() + plot.width() * i as f32 / (BINS - 1) as f32, plot.bottom() - plot.height() * (v / peak).sqrt().min(1.0));
    if let Some(luma) = h.luma {
        let lum = smooth(luma);
        let mut mesh = Mesh::default();
        mesh.vertices.reserve(BINS * 2);
        mesh.indices.reserve((BINS - 1) * 6);
        for (i, value) in lum.iter().enumerate() {
            let top = to(i, *value);
            mesh.colored_vertex(top, t.histogram_library_luma_fill());
            mesh.colored_vertex(pos2(top.x, plot.bottom()), t.histogram_library_luma_fill());
            if i > 0 {
                let k = (i as u32) * 2;
                mesh.indices.extend([k - 2, k - 1, k, k - 1, k + 1, k]);
            }
        }
        p.add(mesh);
    }
    let spread: f32 = (0..BINS).map(|i| (chans[0][i] - chans[1][i]).abs() + (chans[1][i] - chans[2][i]).abs()).sum();
    let total: f32 = chans[1].iter().sum::<f32>().max(1.0);
    let line = |values: &[f32; BINS], color| {
        let pts = values.iter().enumerate().map(|(i, value)| to(i, *value)).collect();
        p.add(egui::Shape::line(pts, egui::Stroke::new(LIBRARY_LINE_WIDTH, color)));
    };
    if spread / total < 0.02 {
        line(&chans[1], t.histogram_library_neutral());
    } else {
        for (channel, values) in chans.iter().enumerate() {
            line(values, t.histogram_library_color(channel));
        }
    }
}

/// A five-bin binomial display kernel softens quantization combs. It cannot bridge
/// a wide empty range. Clipping bins are pinned and excluded from filtering adjacent bins.
fn curves(h: Histogram<'_>) -> [[f32; BINS]; 3] {
    let mut curves = [[0.0; BINS]; 3];
    for (out, bins) in curves.iter_mut().zip(h.channels) {
        for (i, value) in out.iter_mut().enumerate() {
            let center = bins.at(i);
            if i == 0 || i == BINS - 1 {
                *value = center;
            } else {
                let sample = |j: usize| bins.at(j.clamp(1, BINS - 2));
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

pub fn paint_rgb(painter: &egui::Painter, rect: Rect, h: Histogram<'_>, t: &Tokens) {
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

/// One neutral tone marker, using the same RGB weights as Camera Raw's tone controls.
/// This is a constant-size overlay, independent of bin counting and normalization.
pub fn paint_sample(painter: &egui::Painter, rect: Rect, rgb: [f32; 3], t: &Tokens) {
    if !rect.is_finite() || !rect.is_positive() || !rgb.iter().all(|x| x.is_finite()) {
        return;
    }
    let painter = painter.with_clip_rect(rect);
    let [r, g, b] = rgb.map(|v| v.clamp(0.0, 1.0));
    let tone = li_color::luminance_709([r, g, b]);
    let x = rect.left() + tone * rect.width();
    let color = t.histogram_color(7);
    let left = (x - 1.0).round();
    let band = Rect::from_min_max(pos2(left, rect.top()), pos2(left + 2.0, rect.bottom()));
    painter.rect_filled(band, 0.0, color.gamma_multiply(0.40));
}

#[cfg(test)]
#[path = "../tests/reference/histogram_editor.rs"]
pub(crate) mod previous_editor;
#[cfg(test)]
#[path = "../tests/reference/histogram_library.rs"]
pub(crate) mod previous_library;

#[cfg(test)]
#[derive(Clone, Debug, PartialEq)]
struct Fixture {
    channels: [[u64; BINS]; 3],
    luma: [u64; BINS],
}
#[cfg(test)]
impl Default for Fixture {
    fn default() -> Self {
        Self { channels: [[0; BINS]; 3], luma: [0; BINS] }
    }
}
#[cfg(test)]
impl Fixture {
    fn view(&self) -> Histogram<'_> {
        Histogram::u64(&self.channels, Some(&self.luma))
    }
}

#[cfg(test)]
fn mesh(rect: Rect, h: &Fixture, t: &Tokens) -> Mesh {
    fill(rect, &curves(h.view()), t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ThemeKind;

    fn color_at(mesh: &Mesh, point: egui::Pos2) -> Option<egui::Color32> {
        mesh.indices.as_chunks::<3>().0.iter().find_map(|triangle| {
            let a = mesh.vertices[triangle[0] as usize].pos;
            let b = mesh.vertices[triangle[1] as usize].pos;
            let c = mesh.vertices[triangle[2] as usize].pos;
            let cross = |u: egui::Vec2, v: egui::Vec2| u.x * v.y - u.y * v.x;
            if cross(b - a, c - a).abs() < 0.00001 {
                return None;
            }
            let sides = [cross(b - a, point - a), cross(c - b, point - b), cross(a - c, point - c)];
            (sides.iter().all(|&s| s >= -0.00001) || sides.iter().all(|&s| s <= 0.00001)).then_some(mesh.vertices[triangle[0] as usize].color)
        })
    }

    #[test]
    fn histogram_shared_scale_and_overlaps_are_exact_for_every_theme() {
        let h = Fixture { channels: [[3; BINS], [2; BINS], [1; BINS]], ..Default::default() };
        let rect = Rect::from_min_max(pos2(0.0, 0.0), pos2(256.0, 110.0));
        for kind in ThemeKind::ALL {
            let t = Tokens::for_kind(kind);
            let m = mesh(rect, &h, &t);
            for (y, mask) in [(100.0, 7), (60.0, 3), (20.0, 1)] {
                assert_eq!(color_at(&m, pos2(128.7, y)), Some(t.histogram_fill(mask)));
            }
            assert!(m.vertices.iter().all(|v| rect.contains(v.pos)));
        }
    }

    #[test]
    fn histogram_all_overlaps_and_curve_crossings_have_order_independent_colours() {
        let rect = Rect::from_min_max(pos2(0.0, 0.0), pos2(256.0, 100.0));
        let mut h = Fixture::default();
        for mask in 1..=7 {
            for (c, bins) in h.channels.iter_mut().enumerate() {
                bins[mask * 30..mask * 30 + 10].fill(u64::from(mask & (1 << c) != 0));
            }
        }
        for kind in ThemeKind::ALL {
            let t = Tokens::for_kind(kind);
            let m = mesh(rect, &h, &t);
            for mask in 1..=7 {
                assert_eq!(color_at(&m, pos2((mask * 30 + 5) as f32 + 0.5, 95.0)), Some(t.histogram_fill(mask as u8)));
            }
            let mut m = Mesh::default();
            span(&mut m, rect, [0.0, 256.0], [1.0, 0.25, 0.75], [0.25, 1.0, 0.75], &t);
            for (x, y, mask) in [(25.6, 90.0, 7), (25.6, 50.0, 5), (25.6, 10.0, 1), (230.4, 50.0, 6), (230.4, 10.0, 2)] {
                assert_eq!(color_at(&m, pos2(x, y)), Some(t.histogram_fill(mask)));
            }
            // Triangle interiors never stack: alpha fill must be independent of the channel order.
            let area: f32 = m
                .indices
                .as_chunks::<3>()
                .0
                .iter()
                .map(|indices| {
                    let a = m.vertices[indices[0] as usize].pos;
                    let b = m.vertices[indices[1] as usize].pos;
                    let c = m.vertices[indices[2] as usize].pos;
                    ((b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)).abs() * 0.5
                })
                .sum();
            assert!((area - 256.0 * 100.0 * (5.0 / 6.0)).abs() < 0.01);
        }
    }

    #[test]
    fn histogram_edge_spikes_do_not_flatten_or_bleed_into_interior_detail() {
        let mut h = Fixture { channels: [[10; BINS]; 3], ..Default::default() };
        h.channels[0][0] = 1_000_000;
        h.channels[1][255] = 1_000_000;
        let p = curves(h.view());
        assert_eq!(p[0][0], 1.0);
        assert_eq!(p[1][255], 1.0);
        assert!((p[0][1] - 1.0 / 1.1).abs() < 0.0001);
        assert!((p[1][254] - 1.0 / 1.1).abs() < 0.0001);
        let mut h = Fixture::default();
        h.channels[0][255] = 100;
        assert!(curves(h.view())[0][255] > 0.9);
        assert_eq!(curves(h.view())[0][254], 0.0);
    }

    #[test]
    fn histogram_empty_and_bad_rectangles_draw_no_ribbons() {
        let t = Tokens::for_kind(ThemeKind::Pro);
        let h = Fixture::default();
        assert!(mesh(Rect::from_min_max(pos2(0.0, 0.0), pos2(256.0, 100.0)), &h, &t).is_empty());
        assert!(mesh(Rect::NOTHING, &h, &t).is_empty());
    }
    fn covers(mesh: &Mesh, point: egui::Pos2) -> bool {
        color_at(mesh, point).is_some()
    }

    #[test]
    fn histogram_periodic_quantization_gaps_do_not_make_a_sawtooth_contour() {
        let mut h = Fixture::default();
        for channel in &mut h.channels {
            for i in (30..220).step_by(3) {
                channel[i] = 100;
            }
        }
        let p = curves(h.view());
        let low = p[0][40..210].iter().copied().fold(f32::INFINITY, f32::min);
        let high = p[0][40..210].iter().copied().fold(0.0f32, f32::max);
        assert!(high - low < 0.2, "quantization comb still dominates the contour: {}", high - low);
        assert_eq!(p[0][240], 0.0, "wide empty tonal ranges must stay empty");
    }

    #[test]
    fn histogram_isolated_quantization_gaps_do_not_cut_the_ribbon_into_bars() {
        let mut h = Fixture::default();
        for channel in &mut h.channels {
            channel[119] = 100;
            channel[121] = 100;
        }
        let original = h.clone();
        let m = mesh(Rect::from_min_max(pos2(0.0, 0.0), pos2(256.0, 100.0)), &h, &Tokens::for_kind(ThemeKind::Pro));
        assert!(covers(&m, pos2(120.5, 85.0)), "isolated empty bin creates a full-height black stripe");
        assert!(!covers(&m, pos2(180.5, 85.0)), "a real empty tonal range must stay empty");
        assert_eq!(h, original, "display filtering must never change raw statistics");
    }
    #[test]
    fn histogram_frozen_editor_and_library_shape_parity() {
        let ctx = egui::Context::default();
        let shapes = |paint: &dyn Fn(&egui::Painter)| {
            let mut output = ctx.run_ui(Default::default(), |ui| paint(ui.painter()));
            output.textures_delta.clear();
            output.shapes
        };
        let samples: [[[u32; BINS]; 3]; 5] = std::array::from_fn(|sample| {
            std::array::from_fn(|c| {
                std::array::from_fn(|i| match sample {
                    0 => 0,
                    1 => {
                        if i == 0 || i == 255 {
                            100_000
                        } else {
                            10
                        }
                    }
                    2 => (i * 73 % 41) as u32,
                    3 => ((i * (c + 1) * 73 + c * 37) % 101) as u32,
                    _ => {
                        if (30..220).contains(&i) && i % 3 == 0 {
                            100
                        } else {
                            0
                        }
                    }
                })
            })
        });
        for channels in samples {
            let wide = channels.map(|channel| channel.map(u64::from));
            let luma = channels[1];
            for kind in ThemeKind::ALL {
                let t = Tokens::for_kind(kind);
                for width in [250.0, 400.0] {
                    let rect = Rect::from_min_max(pos2(8.0, 8.0), pos2(width - 8.0, 108.0));
                    let old = shapes(&|p| previous_editor::paint(p, rect, &wide, &t));
                    let new = shapes(&|p| paint_rgb(p, rect, Histogram::u64(&wide, None), &t));
                    assert_eq!(format!("{old:?}"), format!("{new:?}"), "editor {kind:?}/{width}");
                    let old = shapes(&|p| previous_library::paint(p, rect, &channels, &luma));
                    let new = shapes(&|p| paint_library(p, rect, Histogram::u32(channels.each_ref().map(|c| c.as_slice()), Some(&luma)), &t));
                    assert_eq!(format!("{old:?}"), format!("{new:?}"), "library {kind:?}/{width}");
                }
            }
        }
    }

    #[test]
    fn histogram_short_legacy_bins_are_zero_padded_without_panics() {
        let short = Histogram::u32([&[1, 2], &[], &[3]], Some(&[]));
        let padded = Histogram::u32([&[1, 2, 0, 0, 0], &[], &[3, 0, 0]], Some(&[]));
        for (a, b) in short.channels.into_iter().zip(padded.channels) {
            assert_eq!(smooth(a), smooth(b));
        }
        assert_eq!(curves(short), curves(padded));
        assert_eq!(smooth(Bins::U32(&[])), [0.0; BINS]);
        assert_eq!(smooth(Bins::U64(&[])), [0.0; BINS]);
    }
}
