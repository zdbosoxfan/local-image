//! Layer rendering against Photoshop: stroke edges, shadow knockout, pass-through clipping bases
//! and shape bounds.

use super::*;

#[test]
fn a_wide_outside_stroke_is_round_not_a_polygon() {
    // A point source dilated by 60 px must stay a circle. The 5 × 5 chamfer's octagonal level set
    // made the boundary swing ~2.5 px at this radius (~10 px at 245 px).
    let mut d = doc_white(170, 170);
    let mut l = solid_layer("dot", Rect::new(85, 85, 86, 86), [1.0, 0.0, 0.0, 1.0]);
    l.effects.items = vec![stroke(60.0, StrokePosition::Outside)];
    d.layers.push(l);
    let buf = render(&d, Rect::new(0, 0, 170, 170));
    let at = |x: i32, y: i32| buf.px[(y as usize) * 170 + x as usize];
    let (mut lo, mut hi) = (f64::MAX, 0.0f64);
    let stroke_at = |x: i32, y: i32| x >= 0 && y >= 0 && x < 170 && y < 170 && at(x, y)[2] > 0.5 && at(x, y)[0] < 0.5;
    for k in 0..72 {
        let th = (k as f64) * std::f64::consts::TAU / 72.0;
        let (dx, dy) = (th.cos(), th.sin());
        let mut r = 0.0f64;
        let mut t = 0.0f64;
        while t < 84.0 {
            if stroke_at((85.0 + dx * t).round() as i32, (85.0 + dy * t).round() as i32) {
                r = t;
            }
            t += 0.25;
        }
        lo = lo.min(r);
        hi = hi.max(r);
    }
    assert!(hi - lo <= 2.0, "the 60 px stroke boundary swings {lo}..{hi} px — a polygon, not a circle");
}

#[test]
fn the_stroke_metric_switches_at_the_fitted_threshold() {
    // The widest outside stroke any Photoshop oracle matches with the chamfer is 18 px
    // (ag-psd read-write/strokes): its octagon deviates 0.48 px from a circle there; 20 px keeps
    // it. Wider strokes are round.
    assert!(!effects::stroke_outside_exact(18.0));
    assert!(effects::stroke_outside_exact(effects::STROKE_CHAMFER_MAX));
    assert!(effects::stroke_outside_exact(60.0));
}

#[test]
fn shadow_knockout_beneath_a_layer_in_a_blending_mode() {
    // Layer Knocks Out Drop Shadow keeps the shadow out from under the layer's shape. In Normal
    // mode an opaque layer covers it anyway; in a mode that blends with what's beneath, the shadow
    // would show through: under a white Exclusion square on white, it read as white (white over
    // the black shadow) instead of black (white over white), so a logo's Exclusion bar with a
    // drop shadow came out blank. A red Multiply square came out black instead of red.
    for (blend, colour, inside, depth) in [
        (photocraft_color::BlendMode::Exclusion, [1.0, 1.0, 1.0, 1.0], [0.0, 0.0, 0.0, 1.0]),
        (photocraft_color::BlendMode::Multiply, [1.0, 0.0, 0.0, 1.0], [1.0, 0.0, 0.0, 1.0]),
    ]
    .into_iter()
    .flat_map(|c| [SampleType::U8, SampleType::U16, SampleType::F32].map(|depth| (c, depth)))
    .map(|((b, c, i), depth)| (b, c, i, depth))
    {
        let mut d = Document::new("t", Size::new(40, 40), ColorMode::Rgb, depth);
        d.layers.push(solid_layer("bg", Rect::new(0, 0, 40, 40), [1.0; 4]));
        let mut l = solid_layer("sq", Rect::new(10, 10, 30, 30), colour);
        l.blend = blend;
        l.effects.items = vec![Effect::DropShadow(shadow(0.0, 90.0))];
        d.layers.push(l);
        assert!(close4(px(&d, 20, 20), inside), "{blend:?} {depth:?}: {:?}", px(&d, 20, 20));
        // Not knocked out, the shadow shows through the blend, as in Photoshop with the box unticked.
        if let Effect::DropShadow(s) = &mut d.layers[1].effects.items[0] {
            s.knocks_out = false;
        }
        assert!(!close4(px(&d, 20, 20), inside), "{blend:?} {depth:?} without knockout: {:?}", px(&d, 20, 20));
    }
}

// A pass-through group with an adjustment layer reaches the layers beneath, unless the group is
// the base of a clipping group: then Photoshop renders it isolated, and an adjustment with no
// content of its own in the group changes nothing (psd-tools passthrough_clipping_mask_adjustment).
#[test]
fn a_pass_through_clipping_base_keeps_its_adjustments_inside() {
    let base = || {
        let mut d = doc_white(4, 4);
        let group = Layer::group("group", vec![Layer::new("invert", LayerContent::Adjustment(Adjustment::Invert))]);
        d.layers.push(group);
        d
    };
    // Unclipped: the invert reaches the white background.
    assert!(close4(px(&base(), 1, 1), [0.0, 0.0, 0.0, 1.0]), "{:?}", px(&base(), 1, 1));
    // With a layer clipped to the group: the background stays white.
    let mut d = base();
    let mut clip = Layer::raster("clipped", PixelFormat::RGBA8);
    clip.clipped = true;
    d.layers.push(clip);
    assert!(close4(px(&d, 1, 1), [1.0, 1.0, 1.0, 1.0]), "{:?}", px(&d, 1, 1));
}

#[test]
fn shape_layer_bounds_includes_vector_bounds_clipped_to_canvas_and_preserves_effects() {
    let canvas = Rect::new(0, 0, 100, 100);
    let subpath = photocraft_doc::vector::Subpath::polygon(&[(-20.0, -20.0), (50.0, -20.0), (50.0, 50.0), (-20.0, 50.0)]);
    let path = photocraft_doc::vector::Path { subpaths: vec![subpath], ..Default::default() };
    let sh = photocraft_doc::vector::ShapeLayer {
        path,
        fill: Some(photocraft_doc::Fill::Solid(photocraft_doc::Color::BLACK)),
        stroke: Some(photocraft_doc::vector::ShapeStroke { width: 4.0, opacity: 1.0, ..Default::default() }),
        ..Default::default()
    };
    let mut layer = Layer::new("shape", LayerContent::Shape(sh));

    // Vector bounds reach outside canvas (-24..54), but layer_bounds clips vector bounds to canvas:
    let b = layer_bounds(&layer, canvas);
    assert_eq!(b, Rect::new(0, 0, 54, 54), "shape vector bounds must clip to canvas and not reach past it");

    // When the shape has layer effects, layer_bounds preserves effects union handling:
    layer.effects.items = vec![Effect::DropShadow(Shadow { size: 5.0, spread: 0.0, ..shadow(5.0, 90.0) })];
    let b_fx = layer_bounds(&layer, canvas);
    assert!(!b_fx.is_empty());
    assert!(b_fx.contains_rect(&b));
}
