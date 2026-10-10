#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use photocraft_doc::vector_design::appearance::{Arrowhead, StrokeGradientMode, WidthProfile};
use photocraft_doc::{Color, LineCap, ShapeStroke, Subpath};

fn shape() -> ShapeLayer {
    ShapeLayer {
        path: Path::new(vec![Subpath::polygon(&[(10., 10.), (30., 10.), (30., 30.), (10., 30.)])]),
        fill: Some(Fill::Solid(Color::WHITE)),
        ..Default::default()
    }
}
fn stroke(width: f32, color: Color) -> AppearanceItem {
    AppearanceItem::Stroke(StrokeItem { base: ShapeStroke { width, paint: Fill::Solid(color), ..Default::default() }, ..Default::default() })
}
fn render(s: &ShapeLayer, a: &Appearance) -> Surface {
    render_shape(s, Some(a), &[], PixelFormat::RGBA32F, Rect::new(0, 0, 48, 48)).unwrap()
}
fn close(a: [f32; 4], b: [f32; 4]) {
    for (a, b) in a.into_iter().zip(b) {
        assert!((a - b).abs() < 1e-5, "{a} != {b}");
    }
}

#[test]
fn appearance_two_fills_two_strokes_six_independent_pixels() {
    let a = Appearance {
        items: vec![
            AppearanceItem::Fill(FillItem::new(Fill::Solid(Color::rgb(1., 0., 0.)))),
            AppearanceItem::Fill(FillItem { opacity: 0.5, ..FillItem::new(Fill::Solid(Color::rgb(0., 1., 0.))) }),
            stroke(8., Color::rgb(0., 0., 1.)),
            AppearanceItem::Stroke(StrokeItem {
                base: ShapeStroke { width: 2., opacity: 0.5, paint: Fill::Solid(Color::rgb(1., 1., 0.)), ..Default::default() },
                ..Default::default()
            }),
        ],
    };
    let out = render(&shape(), &a);
    for (p, want) in [
        ((0, 0), [0.; 4]),
        ((6, 20), [0., 0., 1., 1.]),
        ((9, 20), [0.5, 0.5, 0.5, 1.]),
        ((10, 20), [0.5, 0.5, 0.5, 1.]),
        ((13, 20), [0., 0., 1., 1.]),
        ((20, 20), [0.5, 0.5, 0., 1.]),
    ] {
        close(out.rgba(p.0, p.1), want);
    }
}
#[test]
fn appearance_none_legacy_empty_hidden_and_reordering() {
    let sh = shape();
    let canvas = Rect::new(0, 0, 48, 48);
    let legacy = render_shape(&sh, None, &[], PixelFormat::RGBA8, canvas).unwrap();
    assert_eq!(legacy.to_interleaved(canvas), crate::render_shape(&sh, PixelFormat::RGBA8, canvas).to_interleaved(canvas));
    assert_eq!(render(&sh, &Appearance::default()).tile_count(), 0);
    let mut a = Appearance { items: vec![stroke(8., Color::BLACK), AppearanceItem::Fill(FillItem::new(Fill::Solid(Color::WHITE)))] };
    close(render(&sh, &a).rgba(11, 20), [1.; 4]);
    a.items.swap(0, 1);
    close(render(&sh, &a).rgba(11, 20), [0., 0., 0., 1.]);
    if let AppearanceItem::Stroke(s) = &mut a.items[1] {
        s.visible = false;
    }
    close(render(&sh, &a).rgba(11, 20), [1.; 4]);
}
#[test]
fn appearance_multiply_translucent_uses_general_alpha_formula() {
    let mut c = Color::rgb(0.8, 0.4, 0.2);
    c.alpha = 0.5;
    let a = Appearance {
        items: vec![
            AppearanceItem::Fill(FillItem::new(Fill::Solid(c))),
            AppearanceItem::Fill(FillItem { paint: Fill::Solid(Color::rgb(0.25, 0.5, 1.)), opacity: 0.5, blend: BlendMode::Multiply, visible: true }),
        ],
    };
    // ab=as=.5, ao=.75; premul=.25*cb + .25*cs + .25*(cb*cs).
    close(render(&shape(), &a).rgba(20, 20), [1.25 / 3., 1.1 / 3., 1.4 / 3., 0.75]);
}
#[test]
fn appearance_pattern_phase_rotation_scale_and_alpha_wrapping() {
    let mut tile = Surface::new(PixelFormat::RGBA32F);
    tile.write_pixel(0, 0, &[1., 0., 0., 1.]);
    tile.write_pixel(1, 0, &[0., 0., 1., 0.]);
    let p = Pattern::new("tile", tile, 2, 1);
    let mut paint = Fill::Pattern { name: "tile".into(), id: p.id.clone(), scale: 1., angle: 0., phase: (1., 0.), link: false };
    let canvas = Rect::new(0, 0, 48, 48);
    let mut a = Appearance { items: vec![AppearanceItem::Fill(FillItem::new(paint.clone()))] };
    let r = render_shape(&shape(), Some(&a), std::slice::from_ref(&p), PixelFormat::RGBA32F, canvas).unwrap();
    close(r.rgba(11, 20), [1., 0., 0., 1.]);
    close(r.rgba(12, 20), [0.; 4]);
    if let Fill::Pattern { scale, phase, angle, .. } = &mut paint {
        *scale = 2.;
        *phase = (0., 0.);
        *angle = 90.;
    }
    a.items[0] = AppearanceItem::Fill(FillItem::new(paint));
    let r = render_shape(&shape(), Some(&a), &[p], PixelFormat::RGBA32F, canvas).unwrap();
    let px = r.rgba(20, 20);
    assert!(px[0] > 0.99 && px[2] == 0. && px[3] > 0. && px[3] < 1., "alpha interpolation must not add blue fringe: {px:?}");
    assert!(render_shape(&shape(), Some(&a), &[], PixelFormat::RGBA8, canvas).unwrap_err().contains("missing appearance pattern"));
}
#[test]
fn appearance_along_and_across_stroke_gradients_have_geometric_oracles() {
    let sh = ShapeLayer { path: Path::new(vec![Subpath::polyline(&[(4., 20.), (44., 20.)])]), ..Default::default() };
    let mut s = StrokeItem {
        base: ShapeStroke {
            width: 10.,
            paint: Fill::gradient(vec![(0., Color::BLACK), (1., Color::WHITE)], 0., 1., photocraft_doc::GradientStyle::Linear, false),
            ..Default::default()
        },
        gradient_mode: StrokeGradientMode::Along,
        ..Default::default()
    };
    let a = Appearance { items: vec![AppearanceItem::Stroke(s.clone())] };
    let r = render(&sh, &a);
    assert!((r.rgba(24, 20)[0] - 0.5125).abs() < 0.002);
    assert!(r.rgba(6, 20)[0] < 0.1 && r.rgba(41, 20)[0] > 0.9);
    s.gradient_mode = StrokeGradientMode::Across;
    let r = render(&sh, &Appearance { items: vec![AppearanceItem::Stroke(s)] });
    assert!(r.rgba(24, 16)[0] > 0.8 && r.rgba(24, 23)[0] < 0.2);
}
#[test]
fn appearance_dash_align_centres_dashes_on_open_ends_and_corners() {
    let sh = ShapeLayer { path: Path::new(vec![Subpath::polyline(&[(4., 20.), (23., 20.), (23., 39.)])]), ..Default::default() };
    let s = StrokeItem { base: ShapeStroke { width: 2., dashes: vec![2., 2.], ..Default::default() }, dash_align: true, ..Default::default() };
    let r = render(&sh, &Appearance { items: vec![AppearanceItem::Stroke(s)] });
    for p in [(4, 20), (22, 20), (23, 20), (23, 38)] {
        assert!(r.rgba(p.0, p.1)[3] > 0.9, "{p:?}");
    }
    assert_eq!(r.rgba(8, 20)[3], 0.);
}
#[test]
fn appearance_arrow_triangle_area_hollow_head_and_single_opacity() {
    let sh = ShapeLayer { path: Path::new(vec![Subpath::polyline(&[(10., 20.), (30., 20.)])]), ..Default::default() };
    for kind in [
        Arrowhead::Triangle,
        Arrowhead::TriangleOpen,
        Arrowhead::Circle,
        Arrowhead::CircleOpen,
        Arrowhead::Square,
        Arrowhead::SquareOpen,
        Arrowhead::Bar,
        Arrowhead::Diamond,
        Arrowhead::Arrow,
        Arrowhead::ArrowOpen,
        Arrowhead::Barbed,
    ] {
        let s =
            StrokeItem { base: ShapeStroke { width: 2., opacity: 0.5, cap: LineCap::Butt, ..Default::default() }, end_arrow: Some(kind), ..Default::default() };
        let r = render(&sh, &Appearance { items: vec![AppearanceItem::Stroke(s)] });
        assert!(r.tiles().count() > 0);
        assert!(r.read_region(Rect::new(0, 0, 48, 48)).as_chunks::<4>().0.iter().all(|p| p[3] <= 0.50001));
        if matches!(kind, Arrowhead::CircleOpen | Arrowhead::SquareOpen) {
            assert_eq!(r.rgba(33, 20)[3], 0.);
        }
    }
    let s = StrokeItem { base: ShapeStroke { width: 2., ..Default::default() }, end_arrow: Some(Arrowhead::Triangle), ..Default::default() };
    let r = render(&sh, &Appearance { items: vec![AppearanceItem::Stroke(s)] });
    // Triangle extends x=26..34, height=8, area=32; union with 40 px shaft overlaps 8 px.
    let area: f32 = r.read_region(Rect::new(0, 0, 48, 48)).as_chunks::<4>().0.iter().map(|p| p[3]).sum();
    assert!((area - 64.).abs() < 0.001, "triangle + shaft union {area}");
}
#[test]
fn appearance_dash_align_closed_start_corner_retains_miter() {
    let sh = ShapeLayer { path: Path::new(vec![Subpath::polygon(&[(4., 4.), (24., 4.), (24., 24.), (4., 24.)])]), ..Default::default() };
    let a = Appearance {
        items: vec![AppearanceItem::Stroke(StrokeItem {
            base: ShapeStroke { width: 2., dashes: vec![2., 2.], ..Default::default() },
            dash_align: true,
            ..Default::default()
        })],
    };
    let out = render(&sh, &a);
    for p in [(3, 3), (24, 3), (24, 24), (3, 24)] {
        assert!(out.rgba(p.0, p.1)[3] > 0.99, "closed corner {p:?}");
    }
}
#[test]
fn appearance_along_gradient_restarts_on_each_subpath() {
    let sh =
        ShapeLayer { path: Path::new(vec![Subpath::polyline(&[(4., 10.), (44., 10.)]), Subpath::polyline(&[(4., 30.), (44., 30.)])]), ..Default::default() };
    let a = Appearance {
        items: vec![AppearanceItem::Stroke(StrokeItem {
            base: ShapeStroke {
                width: 4.,
                paint: Fill::gradient(vec![(0., Color::BLACK), (1., Color::WHITE)], 0., 1., photocraft_doc::GradientStyle::Linear, false),
                ..Default::default()
            },
            gradient_mode: StrokeGradientMode::Along,
            ..Default::default()
        })],
    };
    let r = render(&sh, &a);
    for x in 4..44 {
        close(r.rgba(x, 10), r.rgba(x, 30));
    }
}
#[test]
fn appearance_mirror_and_scale_with_object_validate_atomically() {
    let mut a = Appearance {
        items: vec![
            AppearanceItem::Fill(FillItem::new(Fill::Solid(Color::BLACK))),
            AppearanceItem::Fill(FillItem { visible: false, ..FillItem::new(Fill::Solid(Color::WHITE)) }),
            stroke(3., Color::WHITE),
        ],
    };
    if let AppearanceItem::Stroke(s) = &mut a.items[2] {
        s.scale_with_object = true;
    }
    let mut sh = shape();
    mirror_legacy(&mut sh, &a);
    assert_eq!(sh.fill, Some(Fill::Solid(Color::BLACK)));
    scale_with_object(&mut a, Affine::scale(2.)).unwrap();
    assert_eq!(
        match &a.items[2] {
            AppearanceItem::Stroke(s) => s.base.width,
            _ => 0.,
        },
        6.
    );
    let saved = a.clone();
    assert!(scale_with_object(&mut a, Affine::scale(f64::NAN)).is_err());
    assert_eq!(a, saved);
}
#[test]
fn appearance_projection_is_explicit_and_invalid_inputs_are_rejected() {
    let a = Appearance { items: vec![AppearanceItem::Stroke(StrokeItem { profile: Some(WidthProfile::default()), ..Default::default() })] };
    assert!(render_shape(&shape(), Some(&a), &[], PixelFormat::RGBA8, Rect::new(0, 0, 48, 48)).unwrap_err().contains("package 11 / 17"));
    struct Fixture;
    impl StrokeProjection for Fixture {
        fn outline(&self, _: &Path, _: &StrokeItem) -> Result<Path, String> {
            Ok(Path::new(vec![Subpath::polygon(&[(4., 4.), (8., 4.), (8., 8.), (4., 8.)])]))
        }
    }
    let r = CompiledAppearance::with_projection(&shape(), Some(&a), &[], Rect::new(0, 0, 48, 48), &Fixture)
        .unwrap()
        .render(PixelFormat::RGBA8, Rect::new(0, 0, 48, 48))
        .unwrap();
    assert_eq!(r.rgba(5, 5)[3], 1.);
    assert_eq!(r.rgba(20, 20)[3], 0.);
    let mut a = legacy_appearance(&shape());
    if let AppearanceItem::Fill(f) = &mut a.items[0] {
        f.opacity = f32::NAN;
    }
    assert!(validate(&a).is_err());
    assert_eq!(check_svg_paint_limits(PaintLimitations { transform: Some([1., 0., 1., 1., 0., 0.]), non_pad_spread: true }).len(), 2);
    assert!(check_svg_paint_limits(PaintLimitations { transform: Some([0., 2., -2., 0., 3., 4.]), non_pad_spread: false }).is_empty());
}

#[test]
fn appearance_across_gradient_spans_aligned_and_profile_bands() {
    let ramp = Fill::gradient(vec![(0., Color::BLACK), (1., Color::WHITE)], 0., 1., photocraft_doc::GradientStyle::Linear, false);
    for align in [StrokeAlign::Inside, StrokeAlign::Outside] {
        let a = Appearance {
            items: vec![AppearanceItem::Stroke(StrokeItem {
                base: ShapeStroke { width: 10., align, paint: ramp.clone(), ..Default::default() },
                gradient_mode: StrokeGradientMode::Across,
                ..Default::default()
            })],
        };
        let r = render(&shape(), &a);
        let (near, far) = if align == StrokeAlign::Inside { (10, 19) } else { (0, 9) };
        assert!((r.rgba(20, near)[0] - 0.95).abs() < 0.002);
        assert!((r.rgba(20, far)[0] - 0.05).abs() < 0.002);
    }
    struct Outline;
    impl StrokeProjection for Outline {
        fn outline(&self, _: &Path, _: &StrokeItem) -> Result<Path, String> {
            Ok(Path::new(vec![Subpath::polygon(&[(4., 14.), (44., 14.), (44., 22.), (4., 22.)])]))
        }
    }
    let sh = ShapeLayer { path: Path::new(vec![Subpath::polyline(&[(4., 20.), (44., 20.)])]), ..Default::default() };
    let a = Appearance {
        items: vec![AppearanceItem::Stroke(StrokeItem {
            base: ShapeStroke { width: 8., paint: ramp, ..Default::default() },
            profile: Some(WidthProfile {
                points: vec![
                    photocraft_doc::vector_design::appearance::WidthPoint { t: 0., left: 2., right: 6. },
                    photocraft_doc::vector_design::appearance::WidthPoint { t: 1., left: 2., right: 6. },
                ],
            }),
            gradient_mode: StrokeGradientMode::Across,
            ..Default::default()
        })],
    };
    let r = CompiledAppearance::with_projection(&sh, Some(&a), &[], Rect::new(0, 0, 48, 48), &Outline)
        .unwrap()
        .render(PixelFormat::RGBA32F, Rect::new(0, 0, 48, 48))
        .unwrap();
    assert!((r.rgba(24, 14)[0] - 0.9375).abs() < 0.002);
    assert!((r.rgba(24, 21)[0] - 0.0625).abs() < 0.002);
}

#[test]
fn appearance_fitted_closed_dashes_do_not_restart_at_smooth_start() {
    // The top edge is 20 pixels: 3 fitted periods of 20/3, dash length 10/3.
    // Its midpoint lies in a gap, regardless of a redundant knot used as path start.
    let sh = ShapeLayer { path: Path::new(vec![Subpath::polygon(&[(14., 4.), (24., 4.), (24., 24.), (4., 24.), (4., 4.)])]), ..Default::default() };
    let a = Appearance {
        items: vec![AppearanceItem::Stroke(StrokeItem {
            base: ShapeStroke { width: 2., dashes: vec![2., 2.], ..Default::default() },
            dash_align: true,
            ..Default::default()
        })],
    };
    let r = render(&sh, &a);
    assert_eq!(r.rgba(14, 4)[3], 0.);
    assert!(r.rgba(24, 4)[3] > 0.99);
}

#[test]
fn appearance_dash_round_cap_geometry_budget_rejects_before_allocation() {
    let sh = ShapeLayer { path: Path::new(vec![Subpath::polyline(&[(0., 20.), (10000., 20.)])]), ..Default::default() };
    let a = Appearance {
        items: vec![AppearanceItem::Stroke(StrokeItem {
            base: ShapeStroke { width: 5000., cap: LineCap::Round, dashes: vec![0.00002, 0.00002], ..Default::default() },
            ..Default::default()
        })],
    };
    assert!(render_shape(&sh, Some(&a), &[], PixelFormat::RGBA32F, Rect::new(0, 0, 48, 48)).unwrap_err().contains("budget"));
}

#[test]
fn appearance_long_cubic_subpath_is_rejected_before_flatten_allocation() {
    let knot = photocraft_doc::Knot::smooth(
        photocraft_geom::Point::new(20., 20.),
        photocraft_geom::Point::new(-999980., 20.),
        photocraft_geom::Point::new(999980., 20.),
    );
    let sh = ShapeLayer {
        path: Path::new(vec![Subpath { knots: vec![knot; 100], closed: false, op: photocraft_doc::PathOp::Combine }]),
        fill: Some(Fill::Solid(Color::BLACK)),
        ..Default::default()
    };
    assert!(render_shape(&sh, None, &[], PixelFormat::RGBA8, Rect::new(0, 0, 48, 48)).unwrap_err().contains("flattening"));
}
