use photocraft_color::{BlendMode, Color, ColorMode, SampleType};
use photocraft_compose::adjust::Transfer;
use photocraft_doc::{Adjustment, Document, Effect, Fill, Layer, LayerContent};
use photocraft_geom::{Rect, Size};
use photocraft_gpu::plan::{Kernel, Plan, Unsupported, adjustment_on_gpu, adjustment_program, mode_index, plan};

fn test_document() -> Document {
    Document::with_background("bg", Size::new(64, 64), ColorMode::Rgb, SampleType::U8, Color::WHITE)
}

fn add_raster(doc: &mut Document, name: &str, rect: Rect) {
    let mut layer = Layer::raster(name.to_string(), doc.pixel_format());
    layer.surface_mut().unwrap().fill_rect(rect, &[1.0, 0.0, 0.0, 1.0]);
    doc.layers.push(layer);
}

fn plan_summary(p: &Plan<'_>) -> Vec<(Kernel, u32, Option<u32>, Option<u32>, Option<u32>, Option<u32>)> {
    p.passes.iter().map(|pass| (pass.kernel, pass.dst, pass.a, pass.b, pass.c, pass.d)).collect()
}

#[test]
fn plan_empty_document_succeeds() {
    let doc = test_document();
    let p = plan(&doc).expect("plan should succeed");
    assert!(!p.passes.is_empty());
    assert!(p.root < p.slots);
    assert_eq!(p.passes[0].kernel, Kernel::Clear);
}

#[test]
fn plan_rejects_multichannel() {
    let doc = Document::with_background("m", Size::new(8, 8), ColorMode::Multichannel, SampleType::U8, Color::WHITE);
    let result = plan(&doc);
    assert!(matches!(result, Err(Unsupported(_))));
    if let Err(e) = result {
        assert!(e.0.contains("Multichannel"), "unexpected error: {e}");
    }
}

#[test]
fn plan_slots_and_references_are_in_range() {
    let mut doc = test_document();
    add_raster(&mut doc, "r1", Rect::new(10, 10, 30, 30));
    doc.layers.push(Layer::new("adj", LayerContent::Adjustment(Adjustment::Invert)));
    let group_child = {
        let mut l = Layer::raster("child", doc.pixel_format());
        l.surface_mut().unwrap().fill_rect(Rect::new(20, 20, 40, 40), &[0.0, 0.0, 1.0, 1.0]);
        l
    };
    doc.layers.push(Layer::group("g", vec![group_child]));

    let p = plan(&doc).expect("plan failed");
    assert!(p.root < p.slots);
    for pass in &p.passes {
        assert!(pass.dst < p.slots, "dst {} out of range", pass.dst);
        for slot in [pass.a, pass.b, pass.c, pass.d].into_iter().flatten() {
            assert!(slot < p.slots, "slot {slot} out of range in {:?}", pass.kernel);
        }
    }
}

#[test]
fn plan_is_deterministic() {
    let mut doc = test_document();
    add_raster(&mut doc, "r1", Rect::new(5, 5, 25, 25));
    doc.layers.push(Layer::new("adj", LayerContent::Adjustment(Adjustment::Invert)));
    let p1 = plan(&doc).expect("first plan failed");
    let p2 = plan(&doc).expect("second plan failed");
    assert_eq!(p1.passes.len(), p2.passes.len());
    assert_eq!(p1.slots, p2.slots);
    assert_eq!(p1.root, p2.root);
    assert_eq!(p1.fx.len(), p2.fx.len());
    assert_eq!(plan_summary(&p1), plan_summary(&p2));
}

#[test]
fn plan_starts_with_clear_and_ends_on_root() {
    let mut doc = test_document();
    add_raster(&mut doc, "r", Rect::new(0, 0, 16, 16));
    let p = plan(&doc).expect("plan failed");
    assert_eq!(p.passes.first().unwrap().kernel, Kernel::Clear);
    assert_eq!(p.passes.last().unwrap().dst, p.root);
}

#[test]
fn kernel_entry_and_is_map_are_consistent() {
    for &k in &Kernel::DRAWN {
        assert!(k.entry().is_some(), "{k:?} should have an entry point");
    }
    assert_eq!(Kernel::Clear.entry(), None);
    assert_eq!(Kernel::CopyRect.entry(), None);
    assert_eq!(Kernel::CopyFull.entry(), None);

    for &k in &Kernel::DRAWN {
        if k.is_map() {
            assert!(matches!(
                k,
                Kernel::MShift
                    | Kernel::MDilate
                    | Kernel::MBlur
                    | Kernel::MGlow
                    | Kernel::MFinish
                    | Kernel::MBevelH
                    | Kernel::MBevelShade
                    | Kernel::MStroke
                    | Kernel::MBevelTex
            ));
        } else {
            assert!(!matches!(
                k,
                Kernel::MShift
                    | Kernel::MDilate
                    | Kernel::MBlur
                    | Kernel::MGlow
                    | Kernel::MFinish
                    | Kernel::MBevelH
                    | Kernel::MBevelShade
                    | Kernel::MStroke
                    | Kernel::MBevelTex
            ));
        }
    }
}

#[test]
fn mode_indices_are_unique() {
    let modes = [
        BlendMode::PassThrough,
        BlendMode::Normal,
        BlendMode::Dissolve,
        BlendMode::Darken,
        BlendMode::Multiply,
        BlendMode::ColorBurn,
        BlendMode::LinearBurn,
        BlendMode::DarkerColor,
        BlendMode::Lighten,
        BlendMode::Screen,
        BlendMode::ColorDodge,
        BlendMode::LinearDodge,
        BlendMode::LighterColor,
        BlendMode::Overlay,
        BlendMode::SoftLight,
        BlendMode::HardLight,
        BlendMode::VividLight,
        BlendMode::LinearLight,
        BlendMode::PinLight,
        BlendMode::HardMix,
        BlendMode::Difference,
        BlendMode::Exclusion,
        BlendMode::Subtract,
        BlendMode::Divide,
        BlendMode::Hue,
        BlendMode::Saturation,
        BlendMode::Color,
        BlendMode::Luminosity,
    ];
    let indices: Vec<i32> = modes.iter().map(|&m| mode_index(m)).collect();
    let mut sorted = indices.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(indices.len(), sorted.len(), "indices not unique: {indices:?}");
    assert!(indices.iter().all(|&i| (0..28).contains(&i)));
}

#[test]
fn adjustment_program_invert() {
    let adj = Adjustment::Invert;
    let (kind, params, lut) = adjustment_program(&adj, Transfer::Srgb, SampleType::U8);
    assert_eq!(kind, 1);
    assert_eq!(params, [[0.0; 4]; 4]);
    assert!(lut.is_none());
}

#[test]
fn adjustment_program_threshold_rounds() {
    let adj = Adjustment::Threshold { level: 0.5 };
    let (kind, params, lut) = adjustment_program(&adj, Transfer::Srgb, SampleType::U8);
    assert_eq!(kind, 2);
    assert_eq!(params[0][0], 128.0);
    assert!(lut.is_none());

    let nan_adj = Adjustment::Threshold { level: f32::NAN };
    let (_, nan_params, _) = adjustment_program(&nan_adj, Transfer::Srgb, SampleType::U8);
    assert!(nan_params[0][0].is_nan());
}

#[test]
fn adjustment_program_posterize_clamps() {
    let low = Adjustment::Posterize { levels: 1 };
    let (kind_low, params_low, _) = adjustment_program(&low, Transfer::Srgb, SampleType::U8);
    assert_eq!(kind_low, 3);
    assert_eq!(params_low[0][0], 2.0);

    let high = Adjustment::Posterize { levels: 300 };
    let (_, params_high, _) = adjustment_program(&high, Transfer::Srgb, SampleType::U8);
    assert_eq!(params_high[0][0], 255.0);
}

#[test]
fn adjustment_on_gpu_true_for_non_cmyk_adjustments() {
    assert!(adjustment_on_gpu(&Adjustment::Invert));
    assert!(adjustment_on_gpu(&Adjustment::Posterize { levels: 4 }));
}

#[test]
fn plan_includes_effect_layer_and_passes() {
    let mut doc = test_document();
    let mut layer = Layer::raster("fx", doc.pixel_format());
    layer.surface_mut().unwrap().fill_rect(Rect::new(8, 8, 20, 20), &[0.0, 0.0, 1.0, 1.0]);
    layer.effects.items.push(Effect::default_drop_shadow());
    doc.layers.push(layer);

    let p = plan(&doc).expect("plan failed");
    assert_eq!(p.fx.len(), 1);
    assert!(p.fx[0].region.width() > 0);
    assert!(
        p.passes.iter().any(|pass| matches!(pass.kernel, Kernel::FxPaint | Kernel::FxMerge)),
        "expected effect passes, got {:?}",
        p.passes.iter().map(|p| p.kernel).collect::<Vec<_>>()
    );
}

#[test]
fn small_layer_uses_local_clip_and_copy_back() {
    let mut doc = test_document();
    let rect = Rect::new(10, 10, 25, 30);
    add_raster(&mut doc, "small", rect);
    let p = plan(&doc).expect("plan failed");

    assert!(p.passes.iter().any(|pass| pass.clip == Some(rect)), "expected a pass clipped to {rect:?}");
    assert!(p.passes.iter().any(|pass| pass.kernel == Kernel::CopyRect && pass.clip == Some(rect)), "expected CopyRect back to backdrop over {rect:?}");
    assert_eq!(p.passes.last().unwrap().dst, p.root);
}

#[test]
fn adjustment_layer_includes_adjust_pass() {
    let mut doc = test_document();
    doc.layers.push(Layer::new("adj", LayerContent::Adjustment(Adjustment::Invert)));
    let p = plan(&doc).expect("plan failed");
    assert!(p.passes.iter().any(|pass| pass.kernel == Kernel::Adjust), "expected an Adjust pass");
}

#[test]
fn group_layer_contains_inner_content() {
    let mut doc = test_document();
    let child = {
        let mut l = Layer::raster("child", doc.pixel_format());
        l.surface_mut().unwrap().fill_rect(Rect::new(0, 0, 16, 16), &[0.0, 1.0, 0.0, 1.0]);
        l
    };
    doc.layers.push(Layer::group("group", vec![child]));
    let p = plan(&doc).expect("plan failed");
    // The group's child should contribute a Content pass.
    assert!(p.passes.iter().any(|pass| pass.kernel == Kernel::Content), "expected a Content pass for group child");
    assert!(p.passes.len() > 3, "expected group overhead");
}

#[test]
fn opaque_fill_skips_layers_below() {
    let mut doc = test_document();
    for i in 0..3 {
        add_raster(&mut doc, &format!("below{i}"), Rect::new(i, i, 10 + i, 10 + i));
    }
    doc.layers.push(Layer::new("opaque", LayerContent::Fill(Fill::Solid(Color::rgba(1.0, 0.0, 0.0, 1.0)))));
    let p_full = plan(&doc).expect("plan failed");

    let mut alone = doc.clone();
    alone.layers.drain(..alone.layers.len() - 1);
    let p_alone = plan(&alone).expect("plan failed");
    assert_eq!(p_full.passes.len(), p_alone.passes.len(), "opaque top layer should skip below layers");
}

#[test]
fn no_passes_have_nan_or_infinite_float_fields() {
    let mut doc = test_document();
    add_raster(&mut doc, "r", Rect::new(1, 1, 10, 10));
    doc.layers.push(Layer::new("transparent", LayerContent::Fill(Fill::Solid(Color::rgba(0.0, 0.0, 0.0, 0.0)))));
    let p = plan(&doc).expect("plan failed");
    for pass in &p.passes {
        assert!(pass.opacity.is_finite(), "opacity not finite: {:?}", pass.kernel);
        for c in &pass.color {
            assert!(c.is_finite(), "color not finite: {:?}", pass.kernel);
        }
        for row in &pass.params {
            for v in row {
                assert!(v.is_finite(), "param not finite: {:?}", pass.kernel);
            }
        }
    }
}
