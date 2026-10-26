use super::parity::{check, gpu};
use photocraft_color::{ColorMode, PixelFormat, SampleType};
use photocraft_doc::adjust::{CurvePoint, LevelsChannel};
use photocraft_doc::{Adjustment, Document, Layer, LayerContent, LayerMask};
use photocraft_geom::{Rect, Size};

fn document() -> Document {
    let mut doc = Document::new("HDR", Size::new(530, 2), ColorMode::Rgb, SampleType::F32);
    let mut layer = Layer::raster("pixels", PixelFormat::RGBA32F);
    let pixels = (0..1060)
        .flat_map(|i| match i % 3 {
            0 => [-0.5, 2.0, 8.0, 1.0],
            1 => [2.0, -0.5, 0.25, 0.375],
            _ => [-0.5, 8.0, 2.0, 0.0],
        })
        .collect::<Vec<_>>();
    layer.surface_mut().unwrap().write_region(doc.bounds(), &pixels);
    doc.layers.push(layer);
    doc
}

#[test]
fn float_levels_and_exposure_match_cpu() {
    let mut g = gpu();
    let id = LevelsChannel::default();
    let curve = vec![CurvePoint { input: 0.0, output: 0.0 }, CurvePoint { input: 1.0, output: 1.0 }];
    let cases = [
        (
            Adjustment::Levels { master: id.clone(), per_channel: [id.clone(), id.clone(), id.clone()], space: Default::default(), black: id.clone() },
            [-0.5, 2.0, 8.0],
        ),
        (
            Adjustment::Levels {
                master: LevelsChannel { out_white: 0.5, ..id.clone() },
                per_channel: [id.clone(), id.clone(), id.clone()],
                space: Default::default(),
                black: id,
            },
            [-0.25, 1.0, 4.0],
        ),
        (
            Adjustment::Curves { master: curve.clone(), per_channel: [curve.clone(), curve.clone(), curve.clone()], space: Default::default(), black: curve },
            [-0.5, 2.0, 8.0],
        ),
        (Adjustment::Exposure { exposure: 1.0, offset: -0.25, gamma: 2.0 }, [-1.25f32.sqrt(), 3.75f32.sqrt(), 15.75f32.sqrt()]),
    ];
    for (adj, expected) in cases {
        let mut doc = document();
        doc.layers.push(Layer::new("adjust", LayerContent::Adjustment(adj)));
        let cpu = photocraft_compose::render(&doc, Rect::new(0, 0, 1, 1)).px[0];
        assert!(cpu[..3].iter().zip(expected).all(|(actual, want)| (actual - want).abs() < 2e-5), "{cpu:?} vs {expected:?}");
        check(&mut g, &doc, "signed float adjustment");
        for clipped in [false, true] {
            doc.layers[1].clipped = clipped;
            doc.layers[1].opacity = 0.75;
            let mut mask = LayerMask::reveal_all();
            mask.surface = photocraft_raster::Surface::with_default(PixelFormat::new(ColorMode::Grayscale, SampleType::F32, false), &[0.5]);
            doc.layers[1].mask = Some(mask);
            check(&mut g, &doc, "masked float adjustment");
        }
    }
}
