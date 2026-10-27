use super::*;
use photocraft_color::Color;
use photocraft_doc::GradientMethod;

#[test]
fn shape_and_fill_pixels_agree_for_both_methods_and_all_geometries() {
    let canvas = Rect::new(0, 0, 48, 32);
    let frame = Rect::new(8, 4, 40, 28);
    for method in [GradientMethod::Classic, GradientMethod::Perceptual] {
        for style in [GradientStyle::Linear, GradientStyle::Radial, GradientStyle::Reflected, GradientStyle::Angle, GradientStyle::Diamond] {
            for align in [false, true] {
                for dither in [false, true] {
                    let mut fill = Fill::gradient(vec![(1.0, Color::rgba(1.0, 1.0, 0.0, 0.9)), (0.0, Color::rgba(0.0, 0.0, 1.0, 0.3))], 31.0, 0.8, style, true);
                    if let Fill::Gradient { method: m, opacity_stops, midpoints, offset, align: a, dither: d, .. } = &mut fill {
                        *m = method;
                        *opacity_stops = vec![(0.0, 0.9), (0.5, 0.2), (1.0, 0.7)];
                        *midpoints = vec![0.3];
                        *offset = (0.1, -0.15);
                        *a = align;
                        *d = dither;
                    }
                    let paint = photocraft_vector::Paint::from_fill(&fill, (8.0, 4.0, 40.0, 28.0), (0.0, 0.0, 48.0, 32.0));
                    let rendered = render(&fill, canvas, if align { frame } else { canvas });
                    for y in 0..32 {
                        for x in 0..48 {
                            let expected = paint.at(f64::from(x) + 0.5, f64::from(y) + 0.5);
                            let actual = rendered[(y * 48 + x) as usize];
                            for ch in 0..4 {
                                assert!(
                                    (expected[ch] - actual[ch]).abs() < 3e-5,
                                    "{method:?} {style:?} align={align} dither={dither} ({x},{y}) {actual:?} != {expected:?}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}
