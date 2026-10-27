use super::*;
use photocraft_color::{ColorMode, SampleType};
use photocraft_doc::adjust::{CurvePoint, LevelsChannel};

#[test]
fn float_curve_cache_discriminates_document_mode() {
    let id = vec![CurvePoint { input: 0.0, output: 0.0 }, CurvePoint { input: 1.0, output: 1.0 }];
    let adj = Adjustment::Curves {
        master: id.clone(),
        per_channel: [vec![CurvePoint { input: 0.0, output: 0.0 }, CurvePoint { input: 1.0, output: 128.0 / 255.0 }], id.clone(), id.clone()],
        space: ToneSpace::Rgb,
        black: id,
    };
    for mode in [ColorMode::Rgb, ColorMode::Grayscale, ColorMode::Rgb, ColorMode::Lab, ColorMode::Cmyk, ColorMode::Rgb] {
        let cached = cached_adjustment_program(&adj, Transfer::Gamma(1.0), SampleType::F32, mode);
        let fresh = adjustment_program(&adj, Transfer::Gamma(1.0), SampleType::F32, mode);
        assert_eq!(cached.0, 7);
        assert_eq!(cached.1[0], [0.0, 1.0, 1.0, if mode == ColorMode::Rgb { 1.0 } else { 0.0 }]);
        assert_eq!(cached.1, fresh.1);
        assert_eq!(cached.2.unwrap().as_ref(), fresh.2.as_ref().unwrap());
    }
}

#[test]
fn float_levels_pack_four_analytic_records_without_using_native_tone_kinds() {
    let id = LevelsChannel::default();
    let adj = Adjustment::Levels {
        master: LevelsChannel { out_white: 0.5, ..id.clone() },
        per_channel: [id.clone(), LevelsChannel { gamma: 2.0, ..id.clone() }, id.clone()],
        space: ToneSpace::Rgb,
        black: id,
    };
    let (kind, params, rows) = adjustment_program(&adj, Transfer::Gamma(1.0), SampleType::F32, ColorMode::Rgb);
    assert_eq!(kind, 19);
    assert_eq!(params[0], [0.0, 1.0, 0.0, 1.0]);
    assert_eq!(&rows.unwrap()[0][..20], &[0.0, 1.0, 1.0, 0.0, 0.5, 0.0, 1.0, 1.0, 0.0, 1.0, 0.0, 1.0, 2.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0]);
    for mode in [ColorMode::Rgb, ColorMode::Lab] {
        let mut doc = Document::new("float", photocraft_geom::Size::new(1, 1), mode, SampleType::F32);
        doc.layers.push(Layer::new("levels", LayerContent::Adjustment(adj.clone())));
        let plan = plan(&doc).unwrap();
        for pass in plan.passes {
            assert_ne!(pass.flags & F_HDR, 0);
            assert_eq!(pass.flags & F_LAB != 0, mode == ColorMode::Lab);
        }
    }
}
