#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use photocraft_doc::vector_design::appearance::*;
use photocraft_doc::vector_design::drawing::*;
use photocraft_doc::vector_design::live::*;
use photocraft_doc::vector_design::structure::Constraints;
use photocraft_doc::vector_design::text::*;
use photocraft_doc::{Color, ColorMode, Fill, Path, SampleType, ShapeStroke, Size, Subpath, SymbolInstance, TextLayer};
use photocraft_psd::PsdFile;

fn doc() -> Document {
    Document::new("Fallback", Size::new(32, 24), ColorMode::Rgb, SampleType::U8)
}
fn shape() -> Layer {
    let path = Path::new(vec![Subpath::polygon(&[(4., 4.), (20., 4.), (20., 18.), (4., 18.)])]);
    let mut shape = ShapeLayer { path, fill: Some(Fill::Solid(Color::WHITE)), stroke: Some(ShapeStroke::default()), ..Default::default() };
    let mut cache = Surface::new(photocraft_color::PixelFormat::RGBA8);
    // A cache deliberately different from the legacy paint, with translucent edge pixels.
    cache.fill_rect(Rect::new(2, 2, 23, 20), &[0.2, 0.4, 0.8, 0.6]);
    shape.cache = Some(cache);
    Layer::new("Advanced", LayerContent::Shape(shape))
}
fn stack() -> Appearance {
    Appearance {
        items: vec![
            AppearanceItem::Fill(FillItem::new(Fill::Solid(Color::rgb(1., 0., 0.)))),
            AppearanceItem::Stroke(StrokeItem { base: ShapeStroke { width: 5., ..Default::default() }, ..Default::default() }),
            AppearanceItem::Fill(FillItem::new(Fill::Solid(Color::rgb(0., 1., 0.)))),
            AppearanceItem::Stroke(StrokeItem { base: ShapeStroke { width: 2., ..Default::default() }, ..Default::default() }),
        ],
    }
}

#[test]
fn vector_fallback_loss_collector_lists_all_losses_once_in_stable_order() {
    let mut layer = shape();
    let mut appearance = stack();
    let AppearanceItem::Fill(f) = &mut appearance.items[0] else { panic!() };
    f.opacity = 0.5;
    f.blend = BlendMode::Multiply;
    let AppearanceItem::Stroke(s) = &mut appearance.items[1] else { panic!() };
    s.profile = Some(WidthProfile::default());
    s.brush = Some(BrushRef { id: "brush".into() });
    s.start_arrow = Some(Arrowhead::Triangle);
    s.end_arrow = Some(Arrowhead::Circle);
    s.dash_align = true;
    s.scale_with_object = true;
    s.visible = false;
    s.gradient_mode = StrokeGradientMode::Along;
    layer.vector.appearance = Some(appearance);
    layer.vector.on_path = Some(PathText { path: Path::default(), start: 0., end: 1., flip: false, align: PathTextAlign::Baseline });
    layer.vector.corners.push(KnotCorner { subpath: 0, knot: 0, corner: Corner { radius: 2., kind: CornerKind::Round } });
    layer.vector.live_shape = Some(DesignShape { rect: [0., 0., 20., 20.], angle: 0., parameters: ShapeParameters::Ellipse });
    layer.vector.constraints = Some(Constraints::default());
    layer.vector.clip = Some(Path::default());
    let mut blend = Layer::group("Blend", vec![layer.clone(), layer.clone()]);
    blend.vector.live = Some(LiveGroup::Blend(BlendSpec::default()));
    let mut repeat = Layer::group("Repeat", vec![blend]);
    repeat.vector.live = Some(LiveGroup::Repeat(RepeatSpec::Mirror { origin: [0., 0.], angle: 0. }));
    let mut envelope = Layer::group("Envelope", vec![repeat]);
    envelope.vector.live = Some(LiveGroup::Envelope(EnvelopeSpec::TopObject { path: Path::default() }));
    let symbol =
        Layer::new("Instance", LayerContent::Symbol(SymbolInstance { symbol: photocraft_doc::SymbolId(1), transform: Default::default(), cache: None }));
    let mut doc = doc();
    doc.layers = vec![envelope, symbol];
    let collector = LossCollector::document(&doc);
    let features: Vec<_> = collector.warnings().into_iter().map(|w| w.feature).collect();
    assert_eq!(
        features,
        [
            "appearance-stack",
            "arrowheads",
            "constraints",
            "dash-alignment",
            "design-shape",
            "live-blend",
            "live-corners",
            "live-envelope",
            "live-repeat",
            "multiple-fills",
            "multiple-strokes",
            "paint-blend",
            "paint-opacity",
            "paint-visibility",
            "path-text",
            "stroke-gradient",
            "stroke-scaling",
            "symbols",
            "vector-brush",
            "vector-crop",
            "width-profile"
        ]
    );
    let notice = collector.notice().unwrap();
    for warning in collector.warnings() {
        assert_eq!(notice.matches(&warning.message).count(), 1, "{}", warning.feature);
    }
    doc.layers.reverse();
    assert_eq!(collector.warnings(), LossCollector::document(&doc).warnings());
}

#[test]
fn vector_fallback_boundary_deduplicates_objects_and_legacy_is_silent() {
    let mut collector = LossCollector::document(&doc());
    assert!(collector.notice().is_none());
    for object in ["first", "second"] {
        collector.push(VectorWarning { feature: "brush".into(), object: Some(object.into()), message: "vector brushes".into() });
    }
    assert_eq!(collector.warnings().len(), 1);
    assert_eq!(collector.warnings()[0].object, None);
    let translated =
        collector.notice_with("PSD", &|s| s.replace("vector brushes", "pinceaux vectoriels").replace("keeps rendered pixels", "conserve les pixels")).unwrap();
    assert!(translated.contains("pinceaux vectoriels") && translated.contains("conserve les pixels"));
}

#[test]
fn vector_fallback_stroke_only_opacity_is_an_editability_loss() {
    let mut l = shape();
    l.vector.appearance = Some(Appearance {
        items: vec![AppearanceItem::Stroke(StrokeItem { base: ShapeStroke { opacity: 0.25, ..Default::default() }, ..Default::default() })],
    });
    let mut d = doc();
    d.layers.push(l);
    assert_eq!(LossCollector::document(&d).notice().as_deref(), Some("PSD keeps rendered pixels; editability lost: appearance stacks; per-paint opacity."));
}

#[test]
fn vector_fallback_mirror_topmost_visible_supported_paint_not_stale_legacy() {
    let mut layer = shape();
    let mut a = stack();
    a.items.push(AppearanceItem::Fill(FillItem { visible: false, ..FillItem::new(Fill::Solid(Color::BLACK)) }));
    a.items.push(AppearanceItem::Stroke(StrokeItem { visible: false, base: ShapeStroke { width: 99., ..Default::default() }, ..Default::default() }));
    a.items.push(AppearanceItem::Fill(FillItem {
        paint: Fill::Pattern { name: "p".into(), id: "p".into(), scale: 1., angle: 0., phase: (0., 0.), link: true },
        opacity: 0.5,
        blend: BlendMode::Normal,
        visible: true,
    }));
    layer.vector.appearance = Some(a);
    let LayerContent::Shape(shape) = &layer.content else { panic!() };
    let mirror = shape_mirror(&layer, shape);
    assert_eq!(mirror.fill, Some(Fill::Solid(Color::rgb(0., 1., 0.))));
    assert_eq!(mirror.stroke.unwrap().width, 2.);
    assert_eq!(shape.fill, Some(Fill::Solid(Color::WHITE)), "source stays detached");
    layer.vector.appearance = Some(Appearance::default());
    let empty = shape_mirror(&layer, shape);
    assert!(empty.fill.is_none() && empty.stroke.is_none());
}

#[test]
fn vector_fallback_exports_mirrors_with_exact_cache_channels_and_one_notice() {
    let mut d = doc();
    let mut l = shape();
    l.vector.appearance = Some(stack());
    d.layers.push(l);
    for force_psb in [false, true] {
        let (file, warnings) = crate::document_to_psd_with(&d, &PsdExportOptions { force_psb, ..Default::default() });
        assert_eq!(warnings.len(), 1);
        let file = PsdFile::from_bytes(&file.to_bytes().unwrap()).unwrap();
        let record = &file.layers()[0];
        assert!(is_cache_block(&record.block(&CACHE_KEY).unwrap().data));
        let fill = crate::blocks::parse_fill(b"SoCo", &record.block(b"SoCo").unwrap().data).unwrap();
        assert_eq!(fill, Fill::Solid(Color::rgb(0., 1., 0.)));
        assert_eq!(super::super::parse_vstk(&record.block(b"vstk").unwrap().data, 72.).unwrap().stroke.unwrap().width, 2.);
        let imported = crate::psd_to_document(&file).0;
        assert_eq!(imported.layers[0].surface().unwrap().to_interleaved(d.bounds()), d.layers[0].surface().unwrap().to_interleaved(d.bounds()));
        // Independent premultiplied merged-composite oracle, including PSD matting quantization.
        let expected = photocraft_compose::flatten(&d);
        let actual = crate::merged_composite(&file).unwrap();
        let max = expected
            .px
            .iter()
            .zip(actual)
            .flat_map(|(a, b)| (0..4).map(move |k| (if k == 3 { a[k] - b[k] } else { a[k] * a[3] - b[k] * b[3] }).abs()))
            .fold(0_f32, f32::max);
        assert!(max <= 1. / 255., "premultiplied merged max {max}");
    }
}

#[test]
fn vector_fallback_path_text_never_advertises_ordinary_tysh() {
    let mut d = doc();
    let mut cache = Surface::new(d.pixel_format());
    cache.fill_rect(Rect::new(3, 3, 12, 12), &[0.8, 0.3, 0.2, 0.5]);
    let mut l = Layer::new(
        "Path type",
        LayerContent::Text(TextLayer { text: "ABC".into(), cache: Some(cache), psd_raw: Some(std::sync::Arc::new(vec![1, 2, 3])), ..Default::default() }),
    );
    l.psd_blocks.push((*b"TySh", std::sync::Arc::new(vec![1, 2, 3])));
    l.vector.on_path = Some(PathText { path: Path::default(), start: 0., end: 1., flip: false, align: PathTextAlign::Baseline });
    d.layers.push(l);
    let (file, warnings) = crate::document_to_psd_with(&d, &Default::default());
    assert_eq!(warnings.len(), 1);
    assert!(file.layers()[0].block(b"TySh").is_none());
    assert!(file.layers()[0].block(&CACHE_KEY).is_some());
}

#[test]
fn vector_fallback_empty_hidden_stack_disables_both_psd_paints() {
    for appearance in [Appearance::default(), Appearance { items: vec![AppearanceItem::Stroke(StrokeItem { visible: false, ..Default::default() })] }] {
        let mut d = doc();
        let mut l = shape();
        l.vector.appearance = Some(appearance);
        let LayerContent::Shape(sh) = &mut l.content else { panic!() };
        sh.cache = Some(Surface::new(d.pixel_format()));
        // The legacy fidelity exception must not re-enable an explicitly unpainted stack.
        l.psd_blocks.push((*b"vscg", std::sync::Arc::new(vec![4, 3, 2, 1])));
        l.psd_blocks.push((*b"vstk", std::sync::Arc::new(super::super::vstk_bytes(None, true, 72.))));
        d.layers.push(l);
        let (file, _) = crate::document_to_psd_with(&d, &Default::default());
        let stroke = super::super::parse_vstk(&file.layers()[0].block(b"vstk").unwrap().data, 72.).unwrap();
        assert!(!stroke.fill_enabled && stroke.stroke.is_none());
        assert!(crate::merged_composite(&file).unwrap().iter().all(|p| p[3] == 0.));
    }
}

#[test]
fn vector_fallback_cache_guard_preserves_raw_until_raster_edit() {
    let mut l = Layer::new("Reopened", LayerContent::Raster(shape().surface().unwrap().clone()));
    l.psd_blocks.push((CACHE_KEY, std::sync::Arc::new(cache_block(l.surface().unwrap(), CacheKind::Shape, true, None))));
    l.psd_blocks.push((*b"vstk", std::sync::Arc::new(vec![8, 7])));
    l.psd_blocks.push((*b"test", std::sync::Arc::new(vec![4, 3])));
    assert!(cache_matches(&l));
    let mut d = doc();
    d.layers.push(l.clone());
    let (file, _) = crate::document_to_psd_with(&d, &Default::default());
    assert_eq!(file.layers()[0].block(b"vstk").unwrap().data, [8, 7]);
    l.surface_mut().unwrap().write_pixel(3, 3, &[1., 0., 0., 1.]);
    assert!(!cache_matches(&l));
    d.layers[0] = l;
    let (file, _) = crate::document_to_psd_with(&d, &Default::default());
    assert!(!cache_has_shape_proxy(&file.layers()[0].block(&CACHE_KEY).unwrap().data));
    assert_eq!(cache_kind(&file.layers()[0].block(&CACHE_KEY).unwrap().data), Some(CacheKind::Shape));
    assert!(file.layers()[0].block(b"vstk").is_none());
    assert_eq!(file.layers()[0].block(b"test").unwrap().data, [4, 3]);
    d.layers[0].psd_blocks[0].1 = std::sync::Arc::new(vec![9, 9]);
    let (file, _) = crate::document_to_psd_with(&d, &Default::default());
    assert_eq!(file.layers()[0].block(&CACHE_KEY).unwrap().data, [9, 9]);
    assert_eq!(file.layers()[0].block(b"vstk").unwrap().data, [8, 7], "unknown private blocks are opaque");
}

#[test]
fn vector_fallback_cache_marker_rejects_unknown_truncated_versions() {
    for kind in [CacheKind::Raster, CacheKind::Shape, CacheKind::PathText] {
        let b = cache_block(shape().surface().unwrap(), kind, kind == CacheKind::Shape, None);
        assert_eq!(b.len(), 80);
        assert_eq!(cache_kind(&b), Some(kind));
        assert_eq!(cache_has_shape_proxy(&b), kind == CacheKind::Shape);
    }
    let block = cache_block(shape().surface().unwrap(), CacheKind::Shape, true, None);
    assert!(is_cache_block(&block));
    for i in 0..block.len() {
        assert!(!is_cache_block(&block[..i]));
    }
    for (offset, value) in [(0, b'X'), (30, 3), (31, 2)] {
        let mut invalid = block.clone();
        invalid[offset] = value;
        assert!(!is_cache_block(&invalid));
    }
    let mut l = Layer::new("Moved cache", LayerContent::Raster(shape().surface().unwrap().clone()));
    l.psd_blocks.push((CACHE_KEY, std::sync::Arc::new(cache_block(l.surface().unwrap(), CacheKind::Shape, true, Some(Rect::new(4, 4, 20, 18))))));
    assert_eq!(cache_paint_bounds(&l), Some(Rect::new(4, 4, 20, 18)));
    l.content = LayerContent::Raster(l.surface().unwrap().translated(7, 5));
    assert_eq!(cache_paint_bounds(&l), Some(Rect::new(11, 9, 27, 23)));
}

#[test]
fn vector_fallback_cache_is_clipped_before_hash_and_channel_encoding() {
    let mut d = doc();
    let mut l = shape();
    l.vector.appearance = Some(stack());
    let LayerContent::Shape(sh) = &mut l.content else { panic!() };
    sh.cache.as_mut().unwrap().write_pixel(80, 40, &[1., 1., 1., 1.]);
    d.layers.push(l);
    let (file, _) = crate::document_to_psd_with(&d, &Default::default());
    let rect = file.layers()[0].rect;
    assert!(rect.right <= 32 && rect.bottom <= 24);
    let back = crate::psd_to_document(&file).0;
    assert!(cache_matches(&back.layers[0]));
}

#[test]
fn vector_fallback_preflight_requires_caches_valid_numbers_and_live_provider() {
    let mut d = doc();
    let mut l = shape();
    l.vector.appearance = Some(stack());
    let LayerContent::Shape(sh) = &mut l.content else { panic!() };
    sh.cache = None;
    d.layers.push(l);
    assert!(prepare(&d, None).unwrap_err().to_string().contains("rendered cache"));
    d.layers[0] = shape();
    d.layers[0].vector.appearance = Some(stack());
    let AppearanceItem::Fill(f) = &mut d.layers[0].vector.appearance.as_mut().unwrap().items[0] else { panic!() };
    f.opacity = f32::NAN;
    assert!(prepare(&d, None).is_err());
    let mut live = Layer::group("Live", vec![]);
    live.vector.live = Some(LiveGroup::Blend(BlendSpec::default()));
    d.layers = vec![live];
    assert!(prepare(&d, None).unwrap_err().to_string().contains("common feature projection"));
}

#[test]
fn vector_fallback_projection_is_detached_and_losses_survive_evaluation() {
    struct Provider;
    impl Projection for Provider {
        fn layer(&self, _: &Document, layer: &Layer) -> Result<Option<Layer>, String> {
            let mut out = layer.clone();
            out.vector.live = None;
            out.content = LayerContent::Raster(shape().surface().unwrap().clone());
            Ok(Some(out))
        }
    }
    let mut d = doc();
    let mut live = Layer::group("Blend", vec![]);
    live.vector.live = Some(LiveGroup::Blend(BlendSpec::default()));
    d.layers.push(live);
    let result = export(&d, &Default::default(), Some(&Provider)).unwrap();
    assert_eq!(result.warnings.iter().map(|w| w.feature.as_str()).collect::<Vec<_>>(), ["live-blend"]);
    assert!(d.layers[0].vector.live.is_some());
}

#[test]
fn review_additional_psd_projection_cannot_discard_the_user_vector_mask() {
    struct Provider;
    impl Projection for Provider {
        fn layer(&self, _: &Document, layer: &Layer) -> Result<Option<Layer>, String> {
            let mut result = layer.clone();
            result.vector.live = None;
            result.vector_mask = None;
            result.content = LayerContent::Raster(shape().surface().unwrap().clone());
            Ok(Some(result))
        }
    }
    let mut source = doc();
    let mut layer = Layer::group("Masked Blend", vec![]);
    layer.vector.live = Some(LiveGroup::Blend(BlendSpec::default()));
    layer.vector_mask = Some(photocraft_doc::VectorMask::new(photocraft_vector::shapes::rect(1., 1., 10., 10.)));
    source.layers.push(layer);
    assert!(prepare(&source, Some(&Provider)).unwrap_err().to_string().contains("compositing envelope"));
}
