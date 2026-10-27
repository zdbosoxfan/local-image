use super::*;
use photocraft_doc::{GradientMethod, ShapeLayer, ShapeStroke};

#[test]
fn perceptual_fill_and_cached_shape_match_cpu() {
    let mut g = gpu();
    for style in [GradientStyle::Linear, GradientStyle::Radial, GradientStyle::Angle, GradientStyle::Reflected, GradientStyle::Diamond] {
        let mut d = base_doc(320, 48);
        let mut fill = Fill::gradient(vec![(0.0, Color::rgb(0.0, 0.0, 1.0)), (0.7, Color::rgb(1.0, 1.0, 0.0)), (1.0, Color::BLACK)], 31.0, 0.8, style, true);
        if let Fill::Gradient { method, midpoints, opacity_stops, dither, offset, align, .. } = &mut fill {
            *method = GradientMethod::Perceptual;
            *midpoints = vec![0.2, 0.75];
            *opacity_stops = vec![(0.0, 0.9), (0.5, 0.2), (1.0, 0.7)];
            *offset = (0.1, -0.15);
            *dither = true;
            *align = false;
        }
        d.layers.push(Layer::new("Perceptual fill", LayerContent::Fill(fill.clone())));
        check(&mut g, &d, &format!("perceptual fill {style:?}"));
        d.layers.pop();
        let mut shape = ShapeLayer {
            path: photocraft_vector::shapes::rect(4.0, 4.0, 300.0, 38.0),
            fill: Some(fill.clone()),
            stroke: Some(ShapeStroke { width: 3.0, paint: fill, ..Default::default() }),
            ..Default::default()
        };
        shape.cache = Some(photocraft_vector::render_shape(&shape, d.pixel_format(), d.bounds()));
        d.layers.push(Layer::new("Perceptual shape", LayerContent::Shape(shape)));
        check(&mut g, &d, &format!("perceptual cached shape {style:?}"));
    }
}
