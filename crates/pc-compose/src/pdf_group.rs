//! PDF nonisolated groups: preserve the incoming backdrop during child
//! composition, but track newly painted group alpha independently of it.
use super::{Buffer, Ctx, composite_stack_covered, mask_k, mask_vals};
use photocraft_doc::{Group, Layer};

pub(super) fn merge(coverage: &mut [f32], source: &[f32]) {
    for (dst, src) in coverage.iter_mut().zip(source) {
        let alpha = src.clamp(0.0, 1.0);
        *dst += (1.0 - *dst) * alpha;
    }
}
pub(super) fn add_source(coverage: &mut [f32], source: &Buffer, opacity: f32) {
    for (dst, src) in coverage.iter_mut().zip(&source.px) {
        let alpha = (src[3] * opacity).clamp(0.0, 1.0);
        *dst += (1.0 - *dst) * alpha;
    }
}
pub(super) fn add_paint(coverage: &mut [f32], source: &Buffer, opacity: f32, mode: photocraft_doc::BlendMode) {
    if mode != photocraft_doc::BlendMode::Dissolve {
        add_source(coverage, source, opacity);
        return;
    }
    let width = source.rect.width() as usize;
    for (i, (dst, pixel)) in coverage.iter_mut().zip(&source.px).enumerate() {
        let x = source.rect.x0 + (i % width) as i32;
        let y = source.rect.y0 + (i / width) as i32;
        if super::dissolve_noise(x, y) < pixel[3] * opacity {
            *dst = 1.0;
        }
    }
}
pub(super) struct Prepared {
    pub paint: Buffer,
    pub this_layer: Buffer,
    pub actual_shape: Vec<f32>,
    residual: Option<Residual>,
}
struct Residual {
    actual: Buffer,
    baseline: Buffer,
    actual_coverage: Vec<f32>,
    baseline_coverage: Vec<f32>,
    masks: Option<Vec<f32>>,
}
impl Prepared {
    pub fn has_residual(&self) -> bool {
        self.residual.is_some()
    }
    pub fn apply_residual(&self, layer: &Layer, clipped: &[Layer], before: &Buffer, out: &mut Buffer, mut coverage: Option<&mut [f32]>, cx: &Ctx) {
        let Some(residual) = &self.residual else { return };
        let operation = |input: &Buffer, paint_coverage: &[f32], cx: &Ctx| {
            let (mut result, mut content) = decompose(layer, input, paint_coverage, before, &self.paint, &residual.baseline_coverage, &residual.masks);
            for child in clipped.iter().filter(|child| child.visible) {
                super::composite_atop(child, &mut content, cx);
            }
            let mut style = layer.clone();
            style.opacity = 1.0;
            let mut alpha = Buffer::transparent(before.rect);
            if super::effects::has_effects(layer) {
                let maps = super::effect_maps(layer, cx);
                let bounds = super::paint_bounds(layer).unwrap_or_else(|| super::layer_bounds(layer, cx.canvas));
                super::effects::composite_with_effects_prepared(&style, &content, &mut result, &maps, bounds, cx.patterns, None);
                super::effects::composite_with_effects_prepared(&style, &content, &mut alpha, &maps, bounds, cx.patterns, None);
            } else {
                super::blend_into(&mut result, &content, layer.blend, layer.fill_opacity);
                super::blend_into(&mut alpha, &content, layer.blend, layer.fill_opacity);
            }
            (result, alpha)
        };
        let mut counter = cx.derived(true, cx.capture.clone());
        counter.counterfactual_root = cx.counterfactual_root.or(Some(layer.id));
        let (actual, actual_alpha) = operation(&residual.actual, &residual.actual_coverage, cx);
        let (baseline, _) = operation(&residual.baseline, &residual.baseline_coverage, &counter);
        let q = layer.opacity;
        for (i, ((pixel, a), b)) in out.px.iter_mut().zip(&actual.px).zip(&baseline.px).enumerate() {
            let old_alpha = pixel[3];
            let alpha = (old_alpha + q * (a[3] - b[3])).clamp(0.0, 1.0);
            if alpha > 0.0 {
                for c in 0..3 {
                    pixel[c] = ((pixel[c] * old_alpha + q * (a[c] * a[3] - b[c] * b[3])) / alpha).clamp(0.0, 1.0);
                }
                pixel[3] = alpha;
            } else {
                *pixel = [0.0; 4];
            }
            if let Some(coverage) = coverage.as_deref_mut() {
                coverage[i] = q * actual_alpha.px[i][3];
            }
        }
    }
}
// Native adjustments are alpha-neutral and cannot be encoded by paint coverage
// alone. Split the actual composite into retained-backdrop and paint components.
// Distribute the native color delta continuously using their alpha weights,
// rebalancing saturation between components without losing premultiplied color.
// The counterfactual (no backdrop-reaching adjustments) keeps the exact original
// PDF source. This makes identity edits and infinitesimal changes continuous.
fn decompose(
    layer: &Layer,
    input: &Buffer,
    coverage: &[f32],
    before: &Buffer,
    reference: &Buffer,
    reference_coverage: &[f32],
    masks: &Option<Vec<f32>>,
) -> (Buffer, Buffer) {
    let mut backdrop = before.clone();
    let mut source = Buffer::transparent(before.rect);
    for (i, (((x, b), ag), p)) in input.px.iter().zip(&before.px).zip(coverage).zip(&mut source.px).enumerate() {
        let ag = ag.clamp(0.0, 1.0);
        let retained = b[3] * (1.0 - ag);
        let mut adjusted_b = [b[0], b[1], b[2]];
        for c in 0..3 {
            let initial_source = if reference_coverage[i] > 0.0 { reference.px[i][c] } else { x[c] };
            let target = x[c] * x[3];
            let delta = target - (retained * b[c] + ag * initial_source);
            let paint = (ag * initial_source + if x[3] > 0.0 { delta * ag / x[3] } else { 0.0 }).clamp(0.0, ag);
            let back = (target - paint).clamp(0.0, retained);
            if retained > 0.0 {
                adjusted_b[c] = back / retained;
            }
            if ag > 0.0 {
                p[c] = ((target - back) / ag).clamp(0.0, 1.0);
            }
        }
        let mask = mask_k(masks, i);
        let mf = mask * layer.fill_opacity;
        let denominator = 1.0 - ag * mf;
        let h = if denominator > 0.0 { mf * (1.0 - ag) / denominator } else { 0.0 };
        let masked_b = std::array::from_fn(|c| b[c] + (adjusted_b[c] - b[c]) * h);
        let cb = [b[0], b[1], b[2]];
        let bm = super::blend::blend_rgb(layer.blend, cb, masked_b);
        let bb = super::blend::blend_rgb(layer.blend, cb, cb);
        for c in 0..3 {
            backdrop.px[i][c] = (b[c] + (1.0 - b[3]) * (masked_b[c] - b[c]) + b[3] * (bm[c] - bb[c])).clamp(0.0, 1.0);
        }
        p[3] = ag * mask;
    }
    // Body blend/fill and each effect's own blend now operate normally on this
    // backdrop. No outer blend is applied to exterior style deltas afterwards.
    (backdrop, source)
}

pub(super) fn reaching_adjustment(layers: &[Layer]) -> bool {
    use photocraft_doc::{BlendMode, GroupIsolation, LayerContent};
    layers.iter().filter(|l| l.visible && !l.clipped).any(|l| match &l.content {
        LayerContent::Adjustment(_) => true,
        LayerContent::Group(g)
            if g.isolation == GroupIsolation::NonIsolated
                || g.isolation == GroupIsolation::Auto && l.blend == BlendMode::PassThrough && l.fill_opacity >= 1.0 && !super::effects::has_effects(l) =>
        {
            reaching_adjustment(&g.children)
        }
        _ => false,
    })
}
fn removed(mut result: Buffer, backdrop: &Buffer, coverage: &[f32], masks: &Option<Vec<f32>>) -> Buffer {
    for (i, ((result, initial), alpha)) in result.px.iter_mut().zip(&backdrop.px).zip(coverage).enumerate() {
        let alpha = alpha.clamp(0.0, 1.0);
        if alpha > 0.0 {
            for c in 0..3 {
                result[c] = ((result[c] * result[3] - initial[c] * initial[3] * (1.0 - alpha)) / alpha).clamp(0.0, 1.0);
            }
            result[3] = alpha * mask_k(masks, i);
        } else {
            *result = [0.0; 4];
        }
    }
    result
}
pub(super) fn prepare(layer: &Layer, group: &Group, backdrop: &Buffer, cx: &Ctx) -> Prepared {
    if !cx.suppress_adjustments && cx.counterfactual_root == Some(layer.id) {
        let local = cx.derived(true, cx.capture.clone());
        return prepare(layer, group, backdrop, &local);
    }
    // Each pair owns one heavy-subtree memo. render_content captures the caller's
    // memo before descending, so its completed isolated result remains available
    // to that caller's counterfactual even after a nested pair's memo is dropped.
    let needs_baseline = !cx.suppress_adjustments && reaching_adjustment(&group.children);
    let mut paired = None;
    if needs_baseline {
        let mut local = cx.derived(cx.suppress_adjustments, cx.capture.clone());
        local.isolated_contents.push(Default::default());
        paired = Some(local);
    }
    let cx = paired.as_ref().unwrap_or(cx);
    let mut actual = backdrop.clone();
    let mut actual_coverage = vec![0.0; backdrop.px.len()];
    composite_stack_covered(&group.children, &mut actual, cx, Some(&mut actual_coverage));
    let masks = mask_vals(layer, backdrop.rect, cx);
    let actual_shape = actual_coverage.iter().enumerate().map(|(i, a)| a * mask_k(&masks, i)).collect();
    if needs_baseline && !cx.capture_done() {
        let mut counterfactual = cx.derived(true, cx.capture.clone());
        counterfactual.counterfactual_root = cx.counterfactual_root.or(Some(layer.id));
        let mut baseline = backdrop.clone();
        let mut baseline_coverage = vec![0.0; backdrop.px.len()];
        composite_stack_covered(&group.children, &mut baseline, &counterfactual, Some(&mut baseline_coverage));
        let paint = removed(baseline.clone(), backdrop, &baseline_coverage, &masks);
        let (_, mut this_layer) = decompose(layer, &actual, &actual_coverage, backdrop, &paint, &baseline_coverage, &masks);
        for (i, p) in this_layer.px.iter_mut().enumerate() {
            if actual_coverage[i] == 0.0 {
                *p = actual.px[i];
                p[3] *= mask_k(&masks, i);
            }
        }
        Prepared { paint, this_layer, actual_shape, residual: Some(Residual { actual, baseline, actual_coverage, baseline_coverage, masks }) }
    } else {
        let paint = removed(actual, backdrop, &actual_coverage, &masks);
        Prepared { this_layer: paint.clone(), paint, actual_shape, residual: None }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use photocraft_doc::{BlendMode, Color, ColorMode, Document, Fill, GroupIsolation, LayerContent, LayerMask, PixelFormat, Rect, SampleType, Size, Surface};
    fn fill(name: &str, rgb: [f32; 3], alpha: f32) -> Layer {
        let mut color = Color::rgb(rgb[0], rgb[1], rgb[2]);
        color.alpha = alpha;
        Layer::new(name, LayerContent::Fill(Fill::Solid(color)))
    }
    fn group(children: Vec<Layer>, mode: BlendMode, isolation: GroupIsolation) -> Layer {
        let mut layer = Layer::group("group", children);
        layer.blend = mode;
        let LayerContent::Group(g) = &mut layer.content else { panic!("group") };
        g.isolation = isolation;
        layer
    }
    #[test]
    fn nested_native_groups_have_polynomial_layer_visits() {
        use photocraft_doc::BlendRange;
        for depth in [8, 16] {
            for kind in 0..5 {
                let mut nested = if kind >= 2 {
                    fill("seed", [0.2; 3], 1.0)
                } else {
                    group(vec![fill("seed", [0.2; 3], 1.0), invert()], BlendMode::Normal, GroupIsolation::NonIsolated)
                };
                for _ in 0..depth {
                    let children = if kind >= 2 {
                        let mut heavy = group(vec![nested], BlendMode::Normal, GroupIsolation::Isolated);
                        if kind == 4 {
                            for _ in 0..2 {
                                heavy = group(vec![heavy, invert()], BlendMode::Normal, GroupIsolation::NonIsolated);
                            }
                        }
                        let mut children = vec![heavy];
                        if kind >= 3 {
                            children.push(group(vec![fill("cheap", [0.0; 3], 0.0)], BlendMode::Normal, GroupIsolation::Isolated));
                        }
                        children.push(invert());
                        children
                    } else {
                        vec![nested]
                    };
                    nested = group(children, BlendMode::Normal, GroupIsolation::NonIsolated);
                    if kind == 1 {
                        // Active This Layer range that admits both authored colors.
                        nested.blend_if.set(0, [BlendRange { black: [1, 1], white: [255, 255] }, BlendRange::FULL]);
                    }
                }
                let mut doc = Document::new("bounded traversal", Size::new(1, 1), ColorMode::Rgb, SampleType::F32);
                doc.layers = vec![fill("backdrop", [0.4; 3], 0.4), nested];
                let patterns = super::super::pattern::PreparedPatterns::new(&doc.patterns, super::super::pattern::PREPARED_PATTERN_BYTES);
                let cx = Ctx::for_doc(&doc, &patterns);
                let mut output = Buffer::transparent(doc.bounds());
                super::super::composite_stack(&doc.layers, &mut output, &cx);
                let visits = cx.layer_visits.load(std::sync::atomic::Ordering::Relaxed);
                assert!(visits <= 24 * depth * depth + 50, "kind={kind}, depth={depth}, visits={visits}");
                assert!(cx.memo_stats.peak.load(std::sync::atomic::Ordering::Relaxed) <= 3 * depth + 1);
                assert!(cx.memo_stats.peak_bytes.load(std::sync::atomic::Ordering::Relaxed) <= (3 * depth + 1) * 16);
                assert_eq!(cx.memo_stats.live.load(std::sync::atomic::Ordering::Relaxed), 0);
                let expected = if kind >= 2 { 0.2 } else { 0.8 };
                exact(output.get(0, 0), [expected, expected, expected, 1.0]);
            }
        }
    }
    #[test]
    fn wide_paired_group_retains_one_isolated_region_buffer() {
        use std::sync::atomic::Ordering::Relaxed;
        let mut children = vec![invert()];
        for _ in 0..128 {
            children.push(group(vec![fill("paint", [0.2; 3], 1.0)], BlendMode::Normal, GroupIsolation::Isolated));
        }
        let mut doc = Document::new("wide memo", Size::new(2, 2), ColorMode::Rgb, SampleType::F32);
        doc.layers = vec![fill("backdrop", [0.4; 3], 1.0), group(children, BlendMode::Normal, GroupIsolation::NonIsolated)];
        let patterns = super::super::pattern::PreparedPatterns::new(&doc.patterns, super::super::pattern::PREPARED_PATTERN_BYTES);
        let cx = Ctx::for_doc(&doc, &patterns);
        let mut output = Buffer::transparent(doc.bounds());
        super::super::composite_stack(&doc.layers, &mut output, &cx);
        exact(output.get(0, 0), [0.2, 0.2, 0.2, 1.0]);
        assert_eq!(cx.memo_stats.peak.load(Relaxed), 1);
        assert_eq!(cx.memo_stats.peak_bytes.load(Relaxed), 4 * 16);
        assert_eq!(cx.memo_stats.live.load(Relaxed), 0);
        assert_eq!(cx.memo_stats.bytes.load(Relaxed), 0);
        assert!(cx.layer_visits.load(Relaxed) < 128 * 5);
    }
    #[test]
    fn outer_multiply_inner_screen_coverage_is_independent_of_backdrop_alpha() {
        for ba in [0.0_f32, 0.4, 1.0] {
            let mut doc = Document::new("coverage", Size::new(2, 2), ColorMode::Rgb, SampleType::F32);
            let cb = [0.2, 0.4, 0.6];
            let cs = [0.8, 0.6, 0.4];
            let mut child = fill("screen", cs, 0.6);
            child.blend = BlendMode::Screen;
            let mut g = group(vec![child], BlendMode::Multiply, GroupIsolation::NonIsolated);
            g.opacity = 0.5;
            let mut mask = LayerMask::hide_all();
            mask.surface = Surface::new(PixelFormat::new(ColorMode::Grayscale, SampleType::F32, false));
            mask.surface.fill_rect(Rect::new(0, 0, 2, 2), &[0.4]);
            g.mask = Some(mask);
            doc.layers = vec![fill("backdrop", cb, ba), g];
            let actual = super::super::flatten(&doc).get(0, 0);
            let source_alpha = 0.6 * 0.5 * 0.4;
            let alpha = ba + source_alpha * (1.0 - ba);
            for c in 0..3 {
                // Single-child group result after removing its initial backdrop:
                // (1-ab)*Cs + ab*Screen(Cb,Cs), independent of child opacity.
                let source = (1.0 - ba) * cs[c] + ba * (cb[c] + cs[c] - cb[c] * cs[c]);
                let expected = ((1.0 - source_alpha) * ba * cb[c] + source_alpha * ((1.0 - ba) * source + ba * cb[c] * source)) / alpha;
                assert!((actual[c] - expected).abs() < 0.00001, "ba={ba}, {actual:?}, c={c}, expected={expected}");
            }
            assert!((actual[3] - alpha).abs() < 0.00001);
        }
    }
    #[test]
    fn normal_nonisolated_and_legacy_passthrough_agree_with_nested_masks() {
        let mut doc = Document::new("normal", Size::new(2, 2), ColorMode::Rgb, SampleType::F32);
        let mut a = fill("screen", [0.8, 0.2, 0.6], 0.6);
        a.blend = BlendMode::Screen;
        let mut b = fill("multiply", [0.3, 0.7, 0.4], 0.4);
        b.blend = BlendMode::Multiply;
        let mut inner = group(vec![a, b], BlendMode::Normal, GroupIsolation::NonIsolated);
        inner.opacity = 0.7;
        let mut mask = LayerMask::hide_all();
        mask.surface = Surface::new(PixelFormat::new(ColorMode::Grayscale, SampleType::F32, false));
        mask.surface.fill_rect(Rect::new(0, 0, 2, 2), &[0.6]);
        inner.mask = Some(mask);
        let outer = group(vec![inner], BlendMode::Normal, GroupIsolation::NonIsolated);
        doc.layers = vec![fill("backdrop", [0.2, 0.4, 0.6], 0.4), outer];
        let explicit = super::super::flatten(&doc);
        fn legacy(l: &mut Layer) {
            if let LayerContent::Group(g) = &mut l.content {
                g.isolation = GroupIsolation::Auto;
                l.blend = BlendMode::PassThrough;
                for c in &mut g.children {
                    legacy(c);
                }
            }
        }
        for layer in &mut doc.layers {
            legacy(layer);
        }
        let old = super::super::flatten(&doc);
        for (a, b) in explicit.px.iter().zip(old.px) {
            for c in 0..4 {
                assert!((a[c] - b[c]).abs() < 0.00001);
            }
        }
    }
    #[test]
    fn isolation_is_independent_of_passthrough_blend_and_transparent_children() {
        let mut doc = Document::new("isolation", Size::new(2, 2), ColorMode::Rgb, SampleType::F32);
        let mut child = fill("multiply", [0.8, 0.6, 0.4], 0.0);
        child.blend = BlendMode::Multiply;
        let bg = fill("backdrop", [0.2, 0.4, 0.6], 1.0);
        doc.layers = vec![bg, group(vec![child], BlendMode::Screen, GroupIsolation::NonIsolated)];
        assert_eq!(super::super::flatten(&doc).get(0, 0), [0.2, 0.4, 0.6, 1.0]);
        let LayerContent::Group(g) = &mut doc.layers[1].content else { panic!("group") };
        g.children[0] = fill("opaque", [0.8, 0.6, 0.4], 1.0);
        g.children[0].blend = BlendMode::Multiply;
        g.isolation = GroupIsolation::Isolated;
        doc.layers[1].blend = BlendMode::PassThrough;
        assert_eq!(super::super::flatten(&doc).get(0, 0), [0.8, 0.6, 0.4, 1.0]);
    }
    fn invert() -> Layer {
        Layer::new("invert", LayerContent::Adjustment(photocraft_doc::Adjustment::Invert))
    }
    fn mask(layer: &mut Layer, value: f32, rect: Rect) {
        let mut m = LayerMask::hide_all();
        m.surface = Surface::new(PixelFormat::new(ColorMode::Grayscale, SampleType::F32, false));
        m.surface.fill_rect(rect, &[value]);
        layer.mask = Some(m);
    }
    fn exact(a: [f32; 4], b: [f32; 4]) {
        for c in 0..4 {
            assert!((a[c] - b[c]).abs() < 0.00001, "{a:?} != {b:?}");
        }
    }
    #[test]
    fn native_adjustment_only_is_alpha_neutral_and_differential_through_outer_blend() {
        for ab in [0.0_f32, 0.4, 1.0] {
            for mode in [BlendMode::Normal, BlendMode::Multiply] {
                let mut doc = Document::new("adjustment", Size::new(2, 2), ColorMode::Rgb, SampleType::F32);
                let mut g = group(vec![invert()], mode, GroupIsolation::NonIsolated);
                g.opacity = 0.5;
                mask(&mut g, 0.4, doc.bounds());
                doc.layers = vec![fill("backdrop", [0.2, 0.4, 0.6], ab), g];
                let p = super::super::flatten(&doc).get(0, 0);
                assert!((p[3] - ab).abs() < 0.00001);
                for (c, b) in [0.2_f32, 0.4, 0.6].into_iter().enumerate() {
                    let gain = if mode == BlendMode::Normal { 1.0 } else { 1.0 - ab + ab * b };
                    let expected = ab * (b + 0.2 * gain * (1.0 - 2.0 * b));
                    assert!((p[c] * p[3] - expected).abs() < 0.00001);
                }
            }
        }
    }
    #[test]
    fn native_mixed_paint_adjustment_preserves_positive_and_negative_residuals() {
        for (b, c, expected) in [(0.2_f32, 0.8, 0.4511628), (0.8, 0.9, 0.46744186)] {
            let mut doc = Document::new("mixed", Size::new(2, 1), ColorMode::Rgb, SampleType::F32);
            let mut paint = Layer::raster("paint", PixelFormat::RGBA32F);
            paint.surface_mut().unwrap().fill_rect(Rect::new(0, 0, 1, 1), &[c, c, c, 0.1]);
            let mut g = group(vec![paint, invert()], BlendMode::Normal, GroupIsolation::NonIsolated);
            g.opacity = 0.5;
            doc.layers = vec![fill("backdrop", [b; 3], 0.4), g];
            let out = super::super::flatten(&doc);
            exact(out.get(0, 0), [expected, expected, expected, 0.43]);
            exact(out.get(1, 0), [0.5, 0.5, 0.5, 0.4]);
        }
    }
    #[test]
    fn native_residual_is_identity_continuous_and_keeps_transparent_painted_color() {
        let mut doc = Document::new("identity", Size::new(2, 2), ColorMode::Rgb, SampleType::F32);
        let g = group(vec![fill("paint", [0.8; 3], 0.1)], BlendMode::Multiply, GroupIsolation::NonIsolated);
        doc.layers = vec![fill("backdrop", [0.2; 3], 1.0), g];
        let before = super::super::flatten(&doc);
        let identity = photocraft_doc::Adjustment::Levels {
            master: Default::default(),
            per_channel: Default::default(),
            space: Default::default(),
            black: Default::default(),
        };
        if let LayerContent::Group(g) = &mut doc.layers[1].content {
            g.children.push(Layer::new("identity", LayerContent::Adjustment(identity)));
        }
        for (a, b) in super::super::flatten(&doc).px.into_iter().zip(&before.px) {
            exact(a, *b);
        }
        if let LayerContent::Group(g) = &mut doc.layers[1].content {
            *g.children.last_mut().unwrap() = invert();
            g.children.last_mut().unwrap().opacity = 0.000001;
        }
        let tiny = super::super::flatten(&doc);
        for (a, b) in tiny.px.iter().zip(&before.px) {
            for c in 0..4 {
                assert!((a[c] - b[c]).abs() < 0.0001);
            }
        }
        let mut g = group(vec![fill("paint", [0.2; 3], 0.5), invert()], BlendMode::Multiply, GroupIsolation::NonIsolated);
        g.opacity = 0.5;
        mask(&mut g, 0.4, doc.bounds());
        doc.layers = vec![g];
        exact(super::super::flatten(&doc).get(0, 0), [0.8, 0.8, 0.8, 0.1]);
    }
    #[test]
    fn isolated_and_clipped_adjustments_remain_ordinary_source_content() {
        for mode in [BlendMode::ColorDodge, BlendMode::HardLight] {
            for clipped in [false, true] {
                let mut doc = Document::new("boundary", Size::new(2, 2), ColorMode::Rgb, SampleType::F32);
                let mut adjustment = invert();
                adjustment.clipped = clipped;
                let children = if clipped {
                    vec![fill("paint", [0.2; 3], 0.5), adjustment]
                } else {
                    vec![group(vec![fill("paint", [0.2; 3], 0.5), adjustment], BlendMode::Normal, GroupIsolation::Isolated)]
                };
                doc.layers = vec![fill("backdrop", [0.4; 3], 0.4), group(children, mode, GroupIsolation::NonIsolated)];
                let actual = super::super::flatten(&doc);
                doc.layers[1] = group(vec![fill("equivalent", [0.8; 3], 0.5)], mode, GroupIsolation::NonIsolated);
                for (a, b) in actual.px.into_iter().zip(super::super::flatten(&doc).px) {
                    exact(a, b);
                }
            }
        }
    }
    #[test]
    fn this_layer_blend_if_uses_the_backdrop_derived_source_including_clipping() {
        use photocraft_doc::BlendRange;
        for ab in [0.0_f32, 0.4, 1.0] {
            for clipped in [false, true] {
                let mut doc = Document::new("Blend If", Size::new(2, 2), ColorMode::Rgb, SampleType::F32);
                let mut paint = fill("multiply", [0.8; 3], 1.0);
                paint.blend = BlendMode::Multiply;
                let mut g = group(vec![paint], BlendMode::Normal, GroupIsolation::NonIsolated);
                g.opacity = 0.5;
                g.clipped = clipped;
                mask(&mut g, 0.4, doc.bounds());
                g.blend_if.set(0, [BlendRange { black: [128, 128], white: [255, 255] }, BlendRange::FULL]);
                doc.layers = vec![fill("backdrop", [0.2; 3], ab), g];
                let actual = super::super::flatten(&doc).get(0, 0);
                let cb = if clipped { 1.0 } else { ab };
                let source = 0.8 * (1.0 - cb) + 0.16 * cb;
                if source * 255.0 < 128.0 || clipped {
                    exact(actual, if ab == 0.0 { [0.0; 4] } else { [0.2, 0.2, 0.2, ab] });
                } else {
                    let alpha = ab + 0.2 * (1.0 - ab);
                    let c = (0.2 * ab * 0.8 + source * 0.2) / alpha;
                    exact(actual, [c, c, c, alpha]);
                }
                let mut normalized = doc.clone();
                normalized.normalize_legacy_groups();
                assert_eq!(normalized, doc, "Blend If/clipped groups are not a proven legacy subset");
            }
        }
    }
    #[test]
    fn legacy_normalization_preserves_clipping_sibling_dependency() {
        let mut doc = Document::new("clipping normalization", Size::new(1, 1), ColorMode::Rgb, SampleType::F32);
        let group = group(vec![fill("paint", [0.8; 3], 0.5)], BlendMode::Normal, GroupIsolation::NonIsolated);
        let mut clipped = fill("clipped", [0.2; 3], 0.5);
        clipped.clipped = true;
        doc.layers = vec![group, clipped];
        let before = doc.clone();
        doc.normalize_legacy_groups();
        assert_eq!(doc, before, "the approximate legacy clipping path is not eligible");
        doc.layers[1].visible = false;
        doc.normalize_legacy_groups();
        assert!(matches!(&doc.layers[0].content, LayerContent::Group(g) if g.isolation == GroupIsolation::Auto));
        assert_eq!(doc.layers[0].blend, BlendMode::PassThrough);
    }
    #[test]
    fn native_adjustment_residual_passes_through_color_overlay_once() {
        use photocraft_doc::{Effect, FxCommon};
        for ab in [0.0_f32, 0.4] {
            for overlay in [0.5_f32, 1.0] {
                for fill_opacity in [0.0_f32, 1.0] {
                    let mut doc = Document::new("styled residual", Size::new(2, 2), ColorMode::Rgb, SampleType::F32);
                    let mut g = group(vec![fill("paint", [0.2; 3], 1.0), invert()], BlendMode::Normal, GroupIsolation::NonIsolated);
                    g.opacity = 0.5;
                    g.fill_opacity = fill_opacity;
                    mask(&mut g, 0.4, doc.bounds());
                    g.effects.items.push(Effect::ColorOverlay { common: FxCommon::new(BlendMode::Normal, overlay), color: Color::rgb(0.0, 0.0, 1.0) });
                    doc.layers = vec![fill("backdrop", [0.3; 3], ab), g];
                    let actual = super::super::flatten(&doc).get(0, 0);
                    let rgb =
                        if fill_opacity == 0.0 { [0.0, 0.0, 1.0] } else { [0.8 * (1.0 - overlay), 0.8 * (1.0 - overlay), 0.8 * (1.0 - overlay) + overlay] };
                    let source_alpha = 0.2 * (fill_opacity + (1.0 - fill_opacity) * overlay);
                    let alpha = ab + (1.0 - ab) * source_alpha;
                    let expected = std::array::from_fn(|c| if c == 3 { alpha } else { (0.3 * ab * (1.0 - source_alpha) + rgb[c] * source_alpha) / alpha });
                    exact(actual, expected);
                }
            }
        }
    }
    #[test]
    fn adjustment_differential_keeps_external_clipped_paint_before_styles() {
        use photocraft_doc::{Effect, FxCommon};
        for ab in [0.0_f32, 0.4] {
            for styled in [false, true] {
                let mut doc = Document::new("clipped residual", Size::new(2, 2), ColorMode::Rgb, SampleType::F32);
                let mut g = group(vec![fill("paint", [0.2; 3], 1.0), invert()], BlendMode::Normal, GroupIsolation::NonIsolated);
                g.opacity = 0.5;
                mask(&mut g, 0.4, doc.bounds());
                if styled {
                    g.effects.items.push(Effect::ColorOverlay { common: FxCommon::new(BlendMode::Normal, 0.5), color: Color::rgb(0.0, 0.0, 1.0) });
                }
                let mut clipped = fill("red clipped", [1.0, 0.0, 0.0], 0.5);
                clipped.clipped = true;
                doc.layers = vec![fill("backdrop", [0.3; 3], ab), g, clipped];
                let actual = super::super::flatten(&doc).get(0, 0);
                let rgb = if styled { [0.45, 0.2, 0.7] } else { [0.9, 0.4, 0.4] };
                let alpha = ab + 0.2 * (1.0 - ab);
                let expected = std::array::from_fn(|c| if c == 3 { alpha } else { (0.3 * ab * 0.8 + rgb[c] * 0.2) / alpha });
                exact(actual, expected);
            }
        }
    }
    #[test]
    fn group_effect_coverage_uses_full_actual_backdrop_and_invalidates_cache() {
        use photocraft_doc::{BlendRange, Effect, FxCommon, Shadow};
        let mut doc = Document::new("effect backdrop", Size::new(520, 40), ColorMode::Rgb, SampleType::F32);
        let mut paint = Layer::raster("edge", PixelFormat::RGBA32F);
        paint.surface_mut().unwrap().fill_rect(Rect::new(252, 8, 260, 16), &[1.0, 0.0, 0.0, 1.0]);
        paint.blend_if.set(0, [BlendRange::FULL, BlendRange { black: [128, 128], white: [255, 255] }]);
        let mut g = group(vec![paint], BlendMode::Normal, GroupIsolation::NonIsolated);
        g.effects.items.push(Effect::DropShadow(Shadow {
            common: FxCommon::new(BlendMode::Normal, 1.0),
            color: Color::BLACK,
            angle: 180.0,
            use_global_light: false,
            distance: 8.0,
            spread: 0.0,
            size: 0.0,
            contour: Default::default(),
            anti_alias: false,
            noise: 0.0,
            knocks_out: true,
        }));
        doc.layers = vec![fill("backdrop", [0.2; 3], 1.0), g];
        let dark = super::super::render_tiled(&doc, doc.bounds(), 256);
        exact(dark.get(254, 12), [0.2, 0.2, 0.2, 1.0]);
        exact(dark.get(266, 12), [0.2, 0.2, 0.2, 1.0]);
        let definition = if let LayerContent::Fill(f) = &doc.layers[0].content { f.clone() } else { panic!("fill") };
        let mut cache = Surface::new(PixelFormat::RGBA32F);
        cache.fill_rect(doc.bounds(), &[0.8, 0.8, 0.8, 1.0]);
        doc.layers[0].fill_cache = Some(photocraft_doc::FillCache { fill: definition, surface: cache });
        let light = super::super::render_tiled(&doc, doc.bounds(), 256);
        exact(light.get(254, 12), [1.0, 0.0, 0.0, 1.0]);
        exact(light.get(266, 12), [0.0, 0.0, 0.0, 1.0]);
        let whole = super::super::render_tiled(&doc, doc.bounds(), 1024);
        for (a, b) in light.px.iter().zip(&whole.px) {
            exact(*a, *b);
        }
        let mut fresh = doc.clone();
        fresh.layers[1].id = photocraft_doc::LayerId::fresh();
        for (a, b) in light.px.iter().zip(super::super::flatten(&fresh).px) {
            exact(*a, b);
        }
        let effect = doc.layers.pop().unwrap();
        doc.layers.push(group(vec![fill("local backdrop", [0.2; 3], 1.0), effect], BlendMode::Normal, GroupIsolation::Isolated));
        let isolated = super::super::flatten(&doc);
        exact(isolated.get(254, 12), [0.2, 0.2, 0.2, 1.0]);
        exact(isolated.get(266, 12), [0.2, 0.2, 0.2, 1.0]);
    }
    #[test]
    fn adjustment_revealed_shadow_keeps_its_own_blend_across_tiles_and_fill_zero() {
        use photocraft_doc::{BlendRange, Effect, FxCommon, Shadow};
        for mode in [BlendMode::Normal, BlendMode::Multiply] {
            for fill_opacity in [0.0_f32, 1.0] {
                let mut doc = Document::new("native exterior residual", Size::new(520, 40), ColorMode::Rgb, SampleType::F32);
                let mut paint = Layer::raster("edge", PixelFormat::RGBA32F);
                paint.surface_mut().unwrap().fill_rect(Rect::new(252, 8, 260, 16), &[1.0, 0.0, 0.0, 1.0]);
                paint.blend_if.set(0, [BlendRange::FULL, BlendRange { black: [128, 128], white: [255, 255] }]);
                let mut g = group(vec![invert(), paint], mode, GroupIsolation::NonIsolated);
                g.fill_opacity = fill_opacity;
                g.effects.items.push(Effect::DropShadow(Shadow {
                    common: FxCommon::new(BlendMode::Normal, 1.0),
                    color: Color::BLACK,
                    angle: 180.0,
                    use_global_light: false,
                    distance: 8.0,
                    spread: 0.0,
                    size: 0.0,
                    contour: Default::default(),
                    anti_alias: false,
                    noise: 0.0,
                    knocks_out: true,
                }));
                doc.layers = vec![fill("backdrop", [0.2; 3], 1.0), g];
                let tiled = super::super::render_tiled(&doc, doc.bounds(), 256);
                exact(tiled.get(266, 12), [0.0, 0.0, 0.0, 1.0]);
                let outside = if fill_opacity == 0.0 {
                    0.2
                } else if mode == BlendMode::Normal {
                    0.8
                } else {
                    0.32
                };
                exact(tiled.get(518, 0), [outside, outside, outside, 1.0]);
                let whole = super::super::render_tiled(&doc, doc.bounds(), 1024);
                for (a, b) in tiled.px.iter().zip(&whole.px) {
                    exact(*a, *b);
                }
                let mut fresh = doc.clone();
                fresh.layers[1].id = photocraft_doc::LayerId::fresh();
                for (a, b) in tiled.px.iter().zip(super::super::flatten(&fresh).px) {
                    exact(*a, b);
                }
            }
        }
    }
}
