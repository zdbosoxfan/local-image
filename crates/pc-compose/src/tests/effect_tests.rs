use super::*;
use photocraft_doc::{Glow, GlowSource, GlowTechnique, Gradient, GradientStyle, Satin};

#[test]
fn outside_stroke_width() {
    let d = fx_doc(vec![stroke(3.0, StrokePosition::Outside)]);
    assert!(close4(px(&d, 8, 20), [0.0, 0.0, 1.0, 1.0]));
    assert!(close4(px(&d, 7, 20), [0.0, 0.0, 1.0, 1.0]));
    assert!(close4(px(&d, 5, 20), [1.0; 4]), "{:?}", px(&d, 5, 20));
    assert!(close4(px(&d, 20, 20), [1.0, 0.0, 0.0, 1.0]), "interior untouched");
}

#[test]
fn inside_and_center_strokes() {
    let d = fx_doc(vec![stroke(2.0, StrokePosition::Inside)]);
    assert!(close4(px(&d, 10, 20), [0.0, 0.0, 1.0, 1.0]));
    assert!(close4(px(&d, 11, 20), [0.0, 0.0, 1.0, 1.0]));
    assert!(close4(px(&d, 13, 20), [1.0, 0.0, 0.0, 1.0]));
    assert!(close4(px(&d, 9, 20), [1.0; 4]));
    let d = fx_doc(vec![stroke(4.0, StrokePosition::Center)]);
    assert!(close4(px(&d, 8, 20), [0.0, 0.0, 1.0, 1.0]));
    assert!(close4(px(&d, 11, 20), [0.0, 0.0, 1.0, 1.0]));
    assert!(close4(px(&d, 13, 20), [1.0, 0.0, 0.0, 1.0]));
}

#[test]
fn master_switch_and_per_effect_enable() {
    let mut d = fx_doc(vec![stroke(3.0, StrokePosition::Outside)]);
    d.layers[1].effects.enabled = false;
    assert!(close4(px(&d, 8, 20), [1.0; 4]));
    let mut d = fx_doc(vec![stroke(3.0, StrokePosition::Outside)]);
    if let Effect::Stroke(s) = &mut d.layers[1].effects.items[0] {
        s.common.enabled = false;
    }
    assert!(close4(px(&d, 8, 20), [1.0; 4]));
}

#[test]
fn color_overlay_ignores_fill_opacity_but_not_opacity() {
    let mut d = fx_doc(vec![Effect::ColorOverlay { common: FxCommon::new(photocraft_color::BlendMode::Normal, 1.0), color: Color::rgb(0.0, 1.0, 0.0) }]);
    d.layers[1].fill_opacity = 0.0;
    assert!(close4(px(&d, 20, 20), [0.0, 1.0, 0.0, 1.0]), "overlay shows at fill 0");
    d.layers[1].opacity = 0.5;
    assert!(close4(px(&d, 20, 20), [0.5, 1.0, 0.5, 1.0]), "{:?}", px(&d, 20, 20));
}

#[test]
fn drop_shadow_falls_away_from_light() {
    // Light from the top (90°): shadow below the square.
    let d = fx_doc(vec![Effect::DropShadow(shadow(5.0, 90.0))]);
    assert!(close4(px(&d, 20, 32), [0.0, 0.0, 0.0, 1.0]), "{:?}", px(&d, 20, 32));
    assert!(close4(px(&d, 20, 8), [1.0; 4]));
    // Light from the right (0°, 3 o'clock): shadow to the left.
    let d = fx_doc(vec![Effect::DropShadow(shadow(5.0, 0.0))]);
    assert!(close4(px(&d, 8, 20), [0.0, 0.0, 0.0, 1.0]));
    assert!(close4(px(&d, 32, 20), [1.0; 4]));
}

#[test]
fn group_effects_reach_past_children_effects() {
    // Upstream 980cf05b (#3274): a child's outside stroke reaches further than its group's drop
    // shadow: the group's shadow follows the stroke, not the child's pixels alone.
    let mut d = doc_white(48, 48);
    let mut sq = solid_layer("sq", Rect::new(14, 14, 34, 34), [1.0, 0.0, 0.0, 1.0]);
    sq.effects.items = vec![stroke(6.0, StrokePosition::Outside)];
    let mut grp = Layer::group("g", vec![sq]);
    grp.effects.items = vec![Effect::DropShadow(shadow(2.0, 90.0))];
    d.layers.push(grp);
    assert!(close4(px(&d, 9, 24), [0.0, 0.0, 1.0, 1.0]), "stroke left of the pixels: {:?}", px(&d, 9, 24));
    assert!(close4(px(&d, 24, 38), [0.0, 0.0, 1.0, 1.0]), "stroke below the pixels: {:?}", px(&d, 24, 38));
    assert!(close4(px(&d, 24, 41), [0.0, 0.0, 0.0, 1.0]), "shadow below the stroke: {:?}", px(&d, 24, 41));
    assert!(close4(px(&d, 24, 43), [1.0; 4]), "nothing past the shadow: {:?}", px(&d, 24, 43));
}

#[test]
fn drop_shadow_uses_global_light_and_knockout() {
    let mut d = fx_doc(vec![Effect::DropShadow(Shadow { use_global_light: true, ..shadow(5.0, 0.0) })]);
    d.global_light.angle = 90.0;
    assert!(close4(px(&d, 20, 32), [0.0, 0.0, 0.0, 1.0]));
    // Knock-out: with fill 0 the shadow does not show through the layer.
    d.layers[1].fill_opacity = 0.0;
    assert!(close4(px(&d, 20, 28), [1.0; 4]), "{:?}", px(&d, 20, 28));
    if let Effect::DropShadow(s) = &mut d.layers[1].effects.items[0] {
        s.knocks_out = false;
    }
    assert!(close4(px(&d, 20, 28), [0.0, 0.0, 0.0, 1.0]));
}

#[test]
fn soft_shadow_is_blurred_and_bounded() {
    let d = fx_doc(vec![Effect::DropShadow(Shadow { spread: 0.0, size: 6.0, ..shadow(0.0, 90.0) })]);
    let edge = px(&d, 30, 20); // just outside the right edge
    let far = px(&d, 39, 20);
    assert!(edge[0] < 0.8 && edge[0] > 0.2, "{edge:?}");
    assert!(far[0] > 0.99, "{far:?}");
}

#[test]
fn inner_shadow_only_inside() {
    let d = fx_doc(vec![Effect::InnerShadow(Shadow { knocks_out: false, ..shadow(4.0, 90.0) })]);
    // Top rows of the square are shadowed (light from the top pushes the
    // outside's shadow down into the shape).
    assert!(close4(px(&d, 20, 11), [0.0, 0.0, 0.0, 1.0]), "{:?}", px(&d, 20, 11));
    assert!(close4(px(&d, 20, 25), [1.0, 0.0, 0.0, 1.0]));
    assert!(close4(px(&d, 20, 5), [1.0; 4]));
}

fn glow(technique: GlowTechnique, source: GlowSource) -> Glow {
    Glow {
        common: FxCommon::new(photocraft_color::BlendMode::Normal, 1.0),
        paint: FxPaint::Color(Color::rgb(0.0, 1.0, 0.0)),
        technique,
        spread: 0.0,
        size: 4.0,
        contour: photocraft_doc::Contour::Linear,
        anti_alias: false,
        range: 0.5,
        jitter: 0.0,
        noise: 0.0,
        source,
    }
}

#[test]
fn outer_and_inner_glow_regions() {
    let d = fx_doc(vec![Effect::OuterGlow(glow(GlowTechnique::Precise, GlowSource::Edge))]);
    let near = px(&d, 9, 20);
    assert!(near[1] > 0.7 && near[0] < 0.3, "{near:?}");
    assert!(close4(px(&d, 3, 20), [1.0; 4]));
    assert!(close4(px(&d, 20, 20), [1.0, 0.0, 0.0, 1.0]));
    let d = fx_doc(vec![Effect::InnerGlow(glow(GlowTechnique::Softer, GlowSource::Edge))]);
    let edge = px(&d, 10, 20);
    assert!(edge[1] > 0.3, "{edge:?}");
    assert!(close4(px(&d, 20, 20), [1.0, 0.0, 0.0, 1.0]));
    assert!(close4(px(&d, 5, 20), [1.0; 4]));
}

// A glow gradient runs along the glow, not across the canvas: the first stop hugs the shape on
// every side and later stops lie further out, opaque until the glow fades (#443).
#[test]
fn glow_gradient_follows_the_shape() {
    let red_to_blue = Gradient { stops: vec![(0.0, Color::rgb(1.0, 0.0, 0.0)), (1.0, Color::rgb(0.0, 0.0, 1.0))], ..Gradient::default() };
    let g = Glow { paint: FxPaint::Gradient(red_to_blue), technique: GlowTechnique::Softer, size: 8.0, ..glow(GlowTechnique::Softer, GlowSource::Edge) };
    let d = fx_doc(vec![Effect::OuterGlow(g.clone())]);
    let near = px(&d, 9, 20);
    for p in [px(&d, 30, 20), px(&d, 20, 9), px(&d, 20, 30)] {
        assert!((0..4).all(|c| (p[c] - near[c]).abs() < 1.0 / 255.0), "{p:?} vs {near:?}");
    }
    assert!(near[0] > 0.8 && near[2] < 0.2 && near[1] < 0.1, "red at the edge: {near:?}");
    let out = px(&d, 5, 20);
    assert!(out[2] > out[0] && out[1] < 0.9, "bluer and fading further out: {out:?}");
    // An inner glow's gradient also starts at the edge.
    let d = fx_doc(vec![Effect::InnerGlow(g)]);
    let (edge, top) = (px(&d, 10, 20), px(&d, 20, 10));
    assert!((0..4).all(|c| (edge[c] - top[c]).abs() < 1.0 / 255.0), "{edge:?} vs {top:?}");
    assert!(edge[0] > 0.8 && edge[2] < 0.2, "{edge:?}");
}

#[test]
fn gradient_overlay_follows_angle_and_reverse() {
    let g = Gradient { stops: vec![(0.0, Color::BLACK), (1.0, Color::WHITE)], angle: 0.0, ..Gradient::default() };
    let d = fx_doc(vec![Effect::GradientOverlay { common: FxCommon::new(photocraft_color::BlendMode::Normal, 1.0), gradient: g.clone(), dither: false }]);
    let (l, r) = (px(&d, 10, 20), px(&d, 29, 20));
    assert!(l[0] < 0.1 && r[0] > 0.9, "{l:?} {r:?}");
    let d = fx_doc(vec![Effect::GradientOverlay {
        common: FxCommon::new(photocraft_color::BlendMode::Normal, 1.0),
        gradient: Gradient { reverse: true, style: GradientStyle::Linear, ..g },
        dither: false,
    }]);
    assert!(px(&d, 10, 20)[0] > 0.9);
}

#[test]
fn satin_and_bevel_stay_inside_shape() {
    let satin = Effect::Satin(Satin {
        common: FxCommon::new(photocraft_color::BlendMode::Multiply, 1.0),
        color: Color::BLACK,
        angle: 19.0,
        distance: 4.0,
        size: 4.0,
        contour: photocraft_doc::Contour::Linear,
        anti_alias: true,
        invert: false,
    });
    let d = fx_doc(vec![
        satin,
        Effect::BevelEmboss(photocraft_doc::Bevel {
            enabled: true,
            style: photocraft_doc::BevelStyle::InnerBevel,
            technique: photocraft_doc::BevelTechnique::Smooth,
            depth: 1.0,
            up: true,
            size: 4.0,
            soften: 0.0,
            angle: 90.0,
            altitude: 30.0,
            use_global_light: false,
            gloss_contour: photocraft_doc::Contour::Linear,
            highlight: FxCommon::new(photocraft_color::BlendMode::Screen, 0.75),
            highlight_color: Color::WHITE,
            shadow: FxCommon::new(photocraft_color::BlendMode::Multiply, 0.75),
            shadow_color: Color::BLACK,
            contour: None,
            texture: None,
        }),
    ]);
    for (x, y) in [(5, 5), (35, 20), (20, 35)] {
        assert!(close4(px(&d, x, y), [1.0; 4]), "({x},{y}) {:?}", px(&d, x, y));
    }
    // Light from the top: the top bevel edge is brighter than the bottom one.
    let (top, bot) = (px(&d, 20, 11), px(&d, 20, 28));
    assert!(top[1] > bot[1], "{top:?} {bot:?}");
}

#[test]
fn clipped_layer_effects_are_clipped_to_base() {
    let mut d = doc_white(40, 40);
    d.layers.push(solid_layer("base", Rect::new(10, 10, 30, 30), [1.0, 0.0, 0.0, 1.0]));
    let mut c = solid_layer("clip", Rect::new(20, 10, 40, 30), [0.0, 1.0, 0.0, 1.0]);
    c.clipped = true;
    c.effects.items.push(stroke(3.0, StrokePosition::Outside));
    d.layers.push(c);
    assert!(close4(px(&d, 18, 20), [0.0, 0.0, 1.0, 1.0]), "stroke inside base");
    assert!(close4(px(&d, 20, 8), [1.0; 4]), "stroke clipped outside base");
}

#[test]
fn group_effects_apply_to_group_shape() {
    let mut d = doc_white(40, 40);
    let mut g = Layer::group(
        "g",
        vec![solid_layer("a", Rect::new(10, 10, 20, 30), [1.0, 0.0, 0.0, 1.0]), solid_layer("b", Rect::new(20, 10, 30, 30), [1.0, 0.0, 0.0, 1.0])],
    );
    g.effects.items.push(stroke(2.0, StrokePosition::Outside));
    d.layers.push(g);
    assert!(close4(px(&d, 9, 20), [0.0, 0.0, 1.0, 1.0]));
    assert!(close4(px(&d, 20, 20), [1.0, 0.0, 0.0, 1.0]), "no stroke at the seam between children");
}

#[test]
fn effects_render_identically_in_tiles() {
    for depth in [SampleType::U8, SampleType::U16, SampleType::F32] {
        let mut d = fx_doc(vec![Effect::DropShadow(Shadow { spread: 0.0, size: 5.0, ..shadow(4.0, 120.0) }), stroke(2.0, StrokePosition::Outside)]);
        d.depth = depth;
        for layer in &mut d.layers {
            if let Some(surface) = layer.surface_mut() {
                *surface = surface.convert(photocraft_color::PixelFormat { sample: depth, ..surface.format() });
            }
        }
        let full = flatten(&d);
        // Compare every boundary pixel: a few sampled points miss application-halo regressions.
        for tile in [1, 7, 33] {
            assert_eq!(render_tiled(&d, d.bounds(), tile), full, "{depth:?}, tile {tile}");
        }
    }
}

// PSD-fidelity effect semantics (fitted on Photoshop composites)

#[test]
fn stroke_corners_follow_the_5x5_chamfer_metric() {
    // Square 10..30, outside stroke 3: pixel (9, 7) is offset (1, 3) from the corner pixel.
    // Photoshop measures √5 + 1 = 3.236 there (exact distance 3.162): coverage 3 + 1 − 3.236.
    let d = fx_doc(vec![stroke(3.0, StrokePosition::Outside)]);
    let a = 1.0 - px(&d, 9, 7)[0]; // blue stroke over white: red channel drops by coverage
    assert!((a - 0.764).abs() < 0.01, "{a}");
    // (2, 2) stays Euclidean (two diagonal steps).
    let a = 1.0 - px(&d, 8, 8)[0];
    assert!((a - 1.0).abs() < 0.01, "{a}");
}

#[test]
fn inside_stroke_respects_partial_edge_coverage() {
    // A 25 %-alpha edge column puts the edge 0.75 px further out: a 3 px inside stroke then
    // reaches 75 % into the fourth column, and the edge column takes the stroke colour at 25 %.
    let mut d = doc_white(40, 40);
    let mut l = solid_layer("sq", Rect::new(10, 10, 30, 30), [1.0, 0.0, 0.0, 1.0]);
    l.surface_mut().unwrap().fill_rect(Rect::new(10, 10, 11, 30), &[1.0, 0.0, 0.0, 0.25]);
    l.effects.items = vec![stroke(3.0, StrokePosition::Inside)];
    d.layers.push(l);
    let edge = px(&d, 10, 20);
    assert!(close4(edge, [0.75, 0.75, 1.0, 1.0]), "edge = blue stroke at 25 % over white: {edge:?}");
    let p = px(&d, 13, 20);
    assert!((p[2] - 0.75).abs() < 0.02 && (p[0] - 0.25).abs() < 0.02, "{p:?}");
}

#[test]
fn emboss_shades_the_composited_layer() {
    // Emboss styles paint over the layer as composited (for type: mixed at the text gamma), so a
    // Normal white highlight `k` on an edge pixel keeps (1 − R) = (1 − k)(1 − C) whatever C is:
    // the ratio of type to raster results equals that of their plain composites.
    let emboss = Effect::BevelEmboss(photocraft_doc::Bevel {
        enabled: true,
        style: photocraft_doc::BevelStyle::Emboss,
        technique: photocraft_doc::BevelTechnique::Smooth,
        depth: 1.0,
        up: true,
        size: 6.0,
        soften: 0.0,
        angle: 90.0,
        altitude: 30.0,
        use_global_light: false,
        gloss_contour: photocraft_doc::Contour::Linear,
        highlight: FxCommon::new(photocraft_color::BlendMode::Normal, 1.0),
        highlight_color: Color::WHITE,
        shadow: FxCommon::new(photocraft_color::BlendMode::Multiply, 0.0),
        shadow_color: Color::BLACK,
        contour: None,
        texture: None,
    });
    let make = |text: bool, fx: bool| {
        let mut d = doc_white(40, 40);
        let mut l = solid_layer("sq", Rect::new(10, 10, 30, 30), [0.2, 0.1, 0.6, 1.0]);
        l.surface_mut().unwrap().fill_rect(Rect::new(10, 10, 30, 11), &[0.2, 0.1, 0.6, 0.5]);
        if text {
            let t = photocraft_doc::TextLayer { cache: l.surface().cloned(), ..Default::default() };
            l = Layer::new("t", LayerContent::Text(t));
        }
        if fx {
            l.effects.items = vec![emboss.clone()];
        }
        d.layers.push(l);
        px(&d, 20, 10)
    };
    let (ct, cr, rt, rr) = (make(true, false), make(false, false), make(true, true), make(false, true));
    assert!(rr[1] > cr[1] + 0.05, "the top edge is lit: {rr:?} vs {cr:?}");
    assert!((ct[1] - cr[1]).abs() > 0.02, "text gamma changes the edge: {ct:?} {cr:?}");
    for c in 0..3 {
        let want = (1.0 - ct[c]) / (1.0 - cr[c]);
        assert!(((1.0 - rt[c]) / (1.0 - rr[c]) - want).abs() < 2e-3, "channel {c}: {rt:?} {rr:?} {ct:?} {cr:?}");
    }
}

#[test]
fn interior_effects_keep_the_layer_alpha() {
    // A colour overlay replaces a half-transparent pixel's colour without adding coverage.
    let mut d = Document::new("t", Size::new(40, 40), ColorMode::Rgb, SampleType::U8);
    let mut l = solid_layer("sq", Rect::new(10, 10, 30, 30), [1.0, 0.0, 0.0, 0.5]);
    l.effects.items = vec![Effect::ColorOverlay { common: FxCommon::new(photocraft_color::BlendMode::Normal, 1.0), color: Color::rgb(0.0, 1.0, 0.0) }];
    d.layers.push(l);
    assert!(close4(px(&d, 20, 20), [0.0, 1.0, 0.0, 0.5]), "{:?}", px(&d, 20, 20));
}

#[test]
fn upper_stroke_blends_with_the_backdrop_not_the_lower_stroke() {
    // Multiply yellow (3 px) listed above a Normal blue (8 px): the inner ring multiplies the
    // white backdrop (stays yellow), the outer ring is blue.
    let yellow = Effect::Stroke(StrokeFx {
        common: FxCommon::new(photocraft_color::BlendMode::Multiply, 1.0),
        size: 3.0,
        position: StrokePosition::Outside,
        paint: FxPaint::Color(Color::rgb(1.0, 1.0, 0.0)),
    });
    let d = fx_doc(vec![yellow, stroke(8.0, StrokePosition::Outside)]);
    assert!(close4(px(&d, 8, 20), [1.0, 1.0, 0.0, 1.0]), "{:?}", px(&d, 8, 20));
    assert!(close4(px(&d, 4, 20), [0.0, 0.0, 1.0, 1.0]), "{:?}", px(&d, 4, 20));
}

#[test]
fn shadow_knockout_needs_see_through_fill() {
    // At 100 % fill an anti-aliased edge is not attenuated twice: the result equals no knockout.
    let mut base = Document::new("t", Size::new(40, 40), ColorMode::Rgb, SampleType::U8);
    base.layers.push(solid_layer("bg", Rect::new(0, 0, 40, 40), [1.0; 4]));
    let mut l = solid_layer("sq", Rect::new(10, 10, 30, 30), [1.0, 0.0, 0.0, 1.0]);
    l.surface_mut().unwrap().fill_rect(Rect::new(10, 10, 11, 30), &[1.0, 0.0, 0.0, 0.5]);
    l.effects.items = vec![Effect::DropShadow(Shadow { distance: 0.0, spread: 1.0, size: 4.0, ..shadow(0.0, 0.0) })];
    base.layers.push(l);
    let on = px(&base, 10, 20);
    if let Effect::DropShadow(s) = &mut base.layers[1].effects.items[0] {
        s.knocks_out = false;
    }
    assert!(close4(on, px(&base, 10, 20)), "{on:?}");
}

#[test]
fn linked_pattern_overlay_anchors_at_the_effects_reference_point() {
    use photocraft_doc::pattern::Pattern;
    // 2 × 1 tile: red, blue. Anchored at x = 11 (reference point), x = 11 is red, 12 blue.
    let mut tile = photocraft_raster::Surface::new(PixelFormat::RGBA8);
    tile.fill_rect(Rect::new(0, 0, 1, 1), &[1.0, 0.0, 0.0, 1.0]);
    tile.fill_rect(Rect::new(1, 0, 2, 1), &[0.0, 0.0, 1.0, 1.0]);
    let pat = Pattern::new("rb", tile, 2, 1);
    let overlay = Effect::PatternOverlay {
        common: FxCommon::new(photocraft_color::BlendMode::Normal, 1.0),
        name: "rb".into(),
        id: pat.id.clone(),
        scale: 1.0,
        angle: 0.0,
        link: true,
        phase: (0.0, 0.0),
    };
    let mut d = fx_doc(vec![overlay]);
    d.patterns.push(pat);
    // Without a reference point the layer's top-left (10) anchors the tiling.
    assert!(close4(px(&d, 10, 20), [1.0, 0.0, 0.0, 1.0]), "{:?}", px(&d, 10, 20));
    d.layers[1].effects.reference = Some((11.0, 0.0));
    assert!(close4(px(&d, 11, 20), [1.0, 0.0, 0.0, 1.0]), "{:?}", px(&d, 11, 20));
    assert!(close4(px(&d, 12, 20), [0.0, 0.0, 1.0, 1.0]), "{:?}", px(&d, 12, 20));
}

#[test]
fn outside_stroke_at_zero_fill_leaves_the_interior_clear() {
    // Fill 0 % + an Outside stroke is the classic "outline only" look: the stroke lies outside
    // the layer's pixels, so the interior shows the backdrop, not the stroke colour.
    for (depth, fmt) in [(SampleType::U8, PixelFormat::RGBA8), (SampleType::U16, PixelFormat::RGBA16), (SampleType::F32, PixelFormat::RGBA32F)] {
        let mut d = Document::with_background("t", Size::new(40, 40), ColorMode::Rgb, depth, Color::WHITE);
        let mut l = Layer::raster("sq", fmt);
        l.surface_mut().unwrap().fill_rect(Rect::new(10, 10, 30, 30), &[1.0, 0.0, 0.0, 1.0]);
        l.effects.items = vec![stroke(3.0, StrokePosition::Outside)];
        l.fill_opacity = 0.0;
        d.layers.push(l);
        assert!(close4(px(&d, 20, 20), [1.0; 4]), "{depth:?}: interior shows the white backdrop: {:?}", px(&d, 20, 20));
        assert!(close4(px(&d, 11, 20), [1.0; 4]), "{depth:?}: just inside the edge too: {:?}", px(&d, 11, 20));
        assert!(close4(px(&d, 8, 20), [0.0, 0.0, 1.0, 1.0]), "{depth:?}: the stroke itself stays: {:?}", px(&d, 8, 20));
        // Partial fill: the interior is the layer at that fill over the backdrop, still no stroke.
        d.layers[1].fill_opacity = 0.5;
        assert!(close4(px(&d, 20, 20), [1.0, 0.5, 0.5, 1.0]), "{depth:?}: {:?}", px(&d, 20, 20));
        // 100 % fill is unchanged.
        d.layers[1].fill_opacity = 1.0;
        assert!(close4(px(&d, 20, 20), [1.0, 0.0, 0.0, 1.0]), "{depth:?}: {:?}", px(&d, 20, 20));
    }
}
