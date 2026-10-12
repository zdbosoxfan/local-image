//! Independent hayro-syntax structural assertions and Poppler rendering/extraction.
//! Synthetic GPL-3.0-or-later fixtures; source/versions/thresholds in fixtures/export/SOURCES.md.
use hayro_syntax::{
    Pdf,
    object::{Name, Object, Stream},
};
use photocraft_doc::vector_design::exchange::ExportScope;
use photocraft_doc::{Color, ColorMode, Document, Fill, GradientStyle, Layer, LayerContent, Path, SampleType, ShapeLayer, Size};
use photocraft_pdf::export::{self, ExportError, ExportOptions};
use std::{path::PathBuf, process::Command};
fn scene(path: Path, fill: Fill) -> Document {
    let mut d = Document::new("PDF fixture", Size::new(160, 128), ColorMode::Rgb, SampleType::U8);
    d.resolution_dpi = 96.0;
    let mut s = ShapeLayer { path, fill: Some(fill), ..Default::default() };
    s.cache = Some(photocraft_vector::render_shape(&s, d.pixel_format(), d.bounds()));
    d.layers.push(Layer::new("Artwork", LayerContent::Shape(s)));
    d
}
fn rect() -> Path {
    photocraft_vector::shapes::rect(16., 20., 120., 88.)
}
fn output(d: &Document) -> Vec<u8> {
    export::export(d, &ExportScope::Document, &ExportOptions::default()).unwrap().bytes
}
fn streams(pdf: &Pdf) -> Vec<(Vec<u8>, String)> {
    pdf.objects()
        .into_iter()
        .filter_map(|o| if let Object::Stream(s) = o { Some((s.decoded().unwrap().into_owned(), format!("{:?}", s.dict()))) } else { None })
        .collect()
}
fn has_paths(pdf: &Pdf) -> bool {
    use hayro_syntax::content::{TypedIter, ops::TypedInstruction};
    streams(pdf).iter().any(|(bytes, _)| {
        let mut iter = TypedIter::new(bytes);
        while let Some(op) = iter.next() {
            if matches!(op, TypedInstruction::MoveTo(_) | TypedInstruction::RectPath(_) | TypedInstruction::CubicTo(_)) {
                return true;
            }
        }
        false
    })
}
fn images(pdf: &Pdf) -> usize {
    pdf.objects().into_iter().filter(|o| matches!(o,Object::Stream(s) if s.dict().get::<Name<'_>>(b"Subtype").is_some_and(|n|n.as_ref()==b"Image"))).count()
}
fn files(bytes: &[u8], label: &str) -> PathBuf {
    let dir = std::env::current_dir().unwrap().join("target/refvec/pdf21");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join(format!("{label}.pdf"));
    std::fs::write(&file, bytes).unwrap();
    file
}
fn render(bytes: &[u8], label: &str, dpi: f32) -> image::RgbImage {
    let file = files(bytes, label);
    let prefix = file.with_extension("");
    let result =
        Command::new("pdftoppm").args(["-singlefile", "-png", "-r", &dpi.to_string()]).arg(&file).arg(&prefix).output().expect("Poppler reference is required");
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    // Poppler appends the suffix; replacing an extension would strip decimal
    // fixture labels such as native-group-Multiply-0.75.
    let mut output = prefix.into_os_string();
    output.push(".png");
    image::open(PathBuf::from(output)).unwrap().into_rgb8()
}
fn oracle(d: &Document, label: &str) {
    let got = render(&output(d), label, d.resolution_dpi);
    let expected = photocraft_compose::flatten(d).to_rgba8();
    assert_eq!(got.dimensions(), (d.size.width, d.size.height));
    let mut sum = 0u64;
    let mut intersection = 0usize;
    let mut union = 0usize;
    let mut cpu = Vec::<[u8; 3]>::new();
    for (a, b) in got.pixels().zip(expected.pixels.as_chunks::<4>().0) {
        let white = std::array::from_fn::<_, 3, _>(|k| ((u32::from(b[k]) * u32::from(b[3]) + 127) / 255 + 255 - u32::from(b[3])) as u8);
        cpu.push(white);
        sum += (0..3).map(|k| u64::from(a[k].abs_diff(white[k]))).sum::<u64>();
        let present_a = a.0.iter().any(|v| *v < 240);
        let present_b = white.iter().any(|v| *v < 240);
        intersection += usize::from(present_a && present_b);
        union += usize::from(present_a || present_b);
    }
    let mean = sum as f64 / (got.width() as f64 * got.height() as f64 * 3.0 * 255.0);
    let iou = intersection as f64 / union.max(1) as f64;
    eprintln!("{label}: mean RGB error={mean:.7}, visible IoU={iou:.7}");
    assert!(mean < 0.008, "{label}: mean {mean}");
    // Poppler includes boundary pixels conservatively; native exact-area rendering
    // and text antialias/hinting differ there. Require every silhouette disagreement
    // to lie within one pixel of the independent CPU silhouette boundary.
    let width = got.width() as usize;
    let height = got.height() as usize;
    let present = |p: [u8; 3]| p.iter().any(|v| *v < 240);
    let mut interior_errors = 0;
    for y in 1..height - 1 {
        for x in 1..width - 1 {
            let expected = present(cpu[y * width + x]);
            let actual = present(got.get_pixel(x as u32, y as u32).0);
            if expected != actual
                && (-1isize..=1).all(|dy| (-1isize..=1).all(|dx| present(cpu[((y as isize + dy) as usize) * width + (x as isize + dx) as usize]) == expected))
            {
                interior_errors += 1;
            }
        }
    }
    if has_paths(&Pdf::new(output(d)).unwrap()) {
        assert_eq!(interior_errors, 0, "{label}: silhouette differences beyond a one-pixel boundary");
    }
    let expected = image::RgbImage::from_raw(got.width(), got.height(), cpu.into_iter().flatten().collect()).unwrap();
    let path = std::env::current_dir().unwrap().join("target/refvec/pdf21").join(format!("{label}-cpu.png"));
    expected.save(path).unwrap();
}
#[test]
fn pdf_export_pure_vector_structural_and_dpi_y_flip() {
    let d = scene(rect(), Fill::Solid(Color::BLACK));
    let bytes = output(&d);
    let pdf = Pdf::new(bytes.clone()).unwrap();
    assert_eq!(pdf.pages().len(), 1);
    assert_eq!(images(&pdf), 0);
    assert_eq!(pdf.pages()[0].base_dimensions(), (120., 96.));
    assert!(has_paths(&pdf));
    let raster = render(&bytes, "orientation", 96.);
    assert_eq!(raster.get_pixel(20, 24).0, [0, 0, 0]);
    assert_eq!(raster.get_pixel(20, 114).0, [255, 255, 255]);
}
#[test]
fn pdf_export_poppler_geometry_stroke_and_compound_oracles() {
    for (label, p) in [("rectangle", rect()), ("ellipse", photocraft_vector::shapes::ellipse(16., 20., 120., 88.))] {
        oracle(&scene(p, Fill::Solid(Color::rgb(0.2, 0.4, 0.8))), label);
    }
    let mut d = scene(rect(), Fill::Solid(Color::BLACK));
    let format = d.pixel_format();
    let bounds = d.bounds();
    if let LayerContent::Shape(s) = &mut d.layers[0].content {
        s.path.subpaths.push(photocraft_doc::Subpath::polygon(&[(40., 40.), (112., 40.), (112., 88.), (40., 88.)]));
        s.path.subpaths[1].op = photocraft_doc::PathOp::Subtract;
        s.cache = Some(photocraft_vector::render_shape(s, format, bounds));
    }
    oracle(&d, "ordered-subtract");
    assert_eq!(images(&Pdf::new(output(&d)).unwrap()), 0);
}
#[test]
fn pdf_export_poppler_gradient_opacity_and_resource_oracles() {
    for style in [GradientStyle::Linear, GradientStyle::Reflected, GradientStyle::Radial] {
        let fill = Fill::Gradient {
            stops: vec![(1., Color::WHITE), (0., Color::rgb(0.1, 0.2, 0.4))],
            style,
            angle: 31.,
            scale: 0.8,
            reverse: true,
            opacity_stops: vec![(0., 0.5), (1., 1.)],
            midpoints: vec![0.3],
            offset: (0.1, -0.05),
            dither: false,
            align: true,
        };
        let d = scene(rect(), fill);
        oracle(&d, &format!("gradient-{style:?}"));
        let pdf = Pdf::new(output(&d)).unwrap();
        assert_eq!(images(&pdf), 0);
        let objects = format!("{:?}", pdf.objects().into_iter().collect::<Vec<_>>());
        assert!(objects.contains("ShadingType"));
        assert!(objects.contains("SMask"));
    }
}
#[test]
fn pdf_export_hard_gradient_stops_have_valid_pdf_function_bounds() {
    use hayro_syntax::object::Array;
    for style in [GradientStyle::Linear, GradientStyle::Reflected, GradientStyle::Radial] {
        let d = scene(
            rect(),
            Fill::Gradient {
                stops: vec![(0., Color::BLACK), (0.5, Color::rgb(0.1, 0.2, 0.8)), (0.5, Color::rgb(0.8, 0.1, 0.2)), (1., Color::WHITE)],
                opacity_stops: vec![(0., 0.5), (0.25, 0.5), (0.25, 0.9), (1., 1.)],
                style,
                angle: 31.,
                scale: 0.8,
                reverse: true,
                midpoints: Vec::new(),
                offset: (0., 0.),
                dither: false,
                align: true,
            },
        );
        let pdf = Pdf::new(output(&d)).unwrap();
        assert_eq!(images(&pdf), 0);
        let mut functions = 0;
        for object in pdf.objects() {
            if let Object::Dict(dict) = object
                && dict.get::<i32>(b"FunctionType") == Some(3)
            {
                let bounds = dict.get::<Array<'_>>(b"Bounds").unwrap().iter::<f32>().collect::<Vec<_>>();
                // PDF 1.7 §3.9.3 requires strictly increasing stitching bounds.
                assert!(bounds.iter().all(|b| *b > 0. && *b < 1.));
                assert!(bounds.windows(2).all(|w| w[0] < w[1]), "hard stops must not create zero-width PDF function intervals: {bounds:?}");
                functions += 1;
            }
        }
        assert!(functions >= 2, "color and alpha stitching functions");
        oracle(&d, &format!("hard-gradient-{style:?}"));
    }
}
#[test]
fn pdf_export_group_opacity_clips_and_vector_density_stay_vector() {
    let mut d = scene(rect(), Fill::Solid(Color::BLACK));
    let mut second = d.layers[0].duplicate();
    if let LayerContent::Shape(sh) = &mut second.content {
        sh.path = photocraft_vector::shapes::rect(48., 16., 80., 96.);
        sh.cache = Some(photocraft_vector::render_shape(sh, d.pixel_format(), d.bounds()));
    }
    let mut g = Layer::group("Isolated", vec![d.layers.remove(0), second]);
    g.opacity = 0.5;
    g.blend = photocraft_doc::BlendMode::Normal;
    g.vector.clip = Some(photocraft_vector::shapes::rect(24., 24., 96., 72.));
    // The compositor does not consume vector.clip until the shared display bridge lands;
    // oracle uses the equivalent legacy vector mask to independently validate the clip.
    g.vector_mask =
        Some(photocraft_doc::VectorMask { path: photocraft_vector::shapes::rect(24., 24., 96., 72.), enabled: true, linked: true, density: 1., feather: 0. });
    d.layers.push(g);
    oracle(&d, "group-opacity-clip");
    assert_eq!(images(&Pdf::new(output(&d)).unwrap()), 0);
    d.layers[0].vector.clip = None;
    d.layers[0].vector_mask.as_mut().unwrap().density = 0.4;
    oracle(&d, "vector-mask-density");
    assert_eq!(images(&Pdf::new(output(&d)).unwrap()), 0);
}
#[test]
fn pdf_export_soft_masks_keep_path_artwork_and_match_pixels() {
    let mut d = scene(rect(), Fill::Solid(Color::rgb(0.1, 0.2, 0.4)));
    d.layers[0].vector_mask =
        Some(photocraft_doc::VectorMask { path: photocraft_vector::shapes::rect(36., 28., 72., 64.), enabled: true, linked: true, density: 0.7, feather: 3. });
    oracle(&d, "feathered-vector-mask");
    let pdf = Pdf::new(output(&d)).unwrap();
    assert!(images(&pdf) > 0);
    assert!(has_paths(&pdf));
    d.layers[0].vector_mask = None;
    d.layers[0].mask = Some(photocraft_doc::LayerMask::reveal_all());
    d.layers[0].mask.as_mut().unwrap().surface.write_region(photocraft_geom::Rect::new(16, 20, 64, 108), &vec![0.25; 48 * 88]);
    oracle(&d, "pixel-mask");
}
#[test]
fn pdf_export_pages_selection_target_validation_and_order() {
    let mut d = scene(rect(), Fill::Solid(Color::BLACK));
    let selected = d.layers[0].id;
    let options = ExportOptions::default();
    let b = export::export(&d, &ExportScope::Selection(vec![selected]), &options).unwrap();
    assert_eq!(Pdf::new(b.bytes).unwrap().pages()[0].base_dimensions(), (90., 66.));
    let mut a = Layer::group("A", d.layers.clone());
    let mut b = Layer::group("B", vec![d.layers[0].duplicate()]);
    if let LayerContent::Group(g) = &mut a.content {
        g.artboard = Some(photocraft_doc::Artboard::new(photocraft_geom::Rect::new(10, 20, 110, 100)));
    }
    if let LayerContent::Group(g) = &mut b.content {
        g.artboard = Some(photocraft_doc::Artboard::new(photocraft_geom::Rect::new(20, 10, 140, 110)));
    }
    let ids = vec![b.id, a.id];
    d.layers = vec![a, b];
    let opts = ExportOptions { artboards: ids.clone(), ..Default::default() };
    let bytes = export::export(&d, &ExportScope::Document, &opts).unwrap().bytes;
    let pdf = Pdf::new(bytes).unwrap();
    assert_eq!(pdf.pages().len(), 2);
    assert_eq!(pdf.pages()[0].base_dimensions(), (90., 75.));
    assert_eq!(pdf.pages()[1].base_dimensions(), (75., 60.));
    for targets in [vec![ids[0], ids[0]], vec![ids[0], photocraft_doc::LayerId(0)]] {
        assert!(export::export(&d, &ExportScope::Document, &ExportOptions { artboards: targets, ..Default::default() }).is_err());
    }
    assert!(export::export(&d, &ExportScope::Selection(vec![selected, photocraft_doc::LayerId(0)]), &options).is_err());
}
#[test]
fn pdf_export_embeds_subset_font_unicode_and_outlines_option() {
    let mut d = scene(rect(), Fill::Solid(Color::WHITE));
    let mut t = photocraft_doc::TextLayer {
        text: "ABC Café".into(),
        font_family: "Inter".into(),
        size_pt: 22.,
        transform: photocraft_geom::Affine::translate(16., 64.),
        ..Default::default()
    };
    let mut engine = photocraft_text::TextEngine::new();
    engine.render_layer(&mut t, d.resolution_dpi, d.pixel_format());
    d.layers.push(Layer::new("Type", LayerContent::Text(t)));
    let result = export::export(&d, &ExportScope::Document, &Default::default()).unwrap();
    assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    let pdf = Pdf::new(result.bytes.clone()).unwrap();
    assert_eq!(images(&pdf), 0);
    let objects = pdf.objects().into_iter().collect::<Vec<_>>();
    let text = format!("{objects:?}");
    assert!(text.contains("FontFile2"));
    assert!(text.contains("ToUnicode"));
    let font_len = objects
        .iter()
        .filter_map(|o| if let Object::Dict(d) = o { d.get::<Stream<'_>>(b"FontFile2").map(|s| s.decoded().unwrap().len()) } else { None })
        .max()
        .unwrap();
    assert!(font_len < 100_000, "actual subset size {font_len}");
    let file = files(&result.bytes, "embedded-text");
    let text_file = file.with_extension("txt");
    let status = Command::new("pdftotext").arg(&file).arg(&text_file).status().unwrap();
    assert!(status.success());
    assert!(std::fs::read_to_string(text_file).unwrap().contains("ABC Café"));
    oracle(&d, "text-fidelity");
    let curves = export::export(&d, &ExportScope::Document, &ExportOptions { text_as_curves: true, ..Default::default() }).unwrap();
    let pdf = Pdf::new(curves.bytes).unwrap();
    assert_eq!(images(&pdf), 0);
    assert!(!format!("{:?}", pdf.objects().into_iter().collect::<Vec<_>>()).contains("FontFile2"));
}
#[test]
fn pdf_export_cancelled_and_nonfinite_leave_source_unchanged() {
    let mut d = scene(rect(), Fill::Solid(Color::BLACK));
    let before = d.clone();
    let mut text = photocraft_text::TextEngine::new();
    let mut cx = export::ExportContext { cancelled: &|| true, projection: None, text: &mut text };
    assert!(matches!(export::export_with(&d, &ExportScope::Document, &Default::default(), &mut cx), Err(ExportError::Cancelled)));
    assert_eq!(d, before);
    for value in [f32::NAN, f32::INFINITY, 0., 9601.] {
        d.resolution_dpi = value;
        assert!(export::export(&d, &ExportScope::Document, &Default::default()).is_err());
    }
    d = before;
    d.layers[0].opacity = f32::NAN;
    assert!(export::export(&d, &ExportScope::Document, &Default::default()).is_err());
    assert!(export::export(&d, &ExportScope::Document, &ExportOptions { max_bytes: 0, ..Default::default() }).is_err());
}
#[test]
fn pdf_export_missing_projection_never_uses_stale_cache() {
    let mut d = scene(rect(), Fill::Solid(Color::BLACK));
    d.layers[0].vector.on_path = Some(photocraft_doc::vector_design::text::PathText {
        path: rect(),
        start: 0.0,
        end: 1.0,
        flip: false,
        align: photocraft_doc::vector_design::text::PathTextAlign::Baseline,
    });
    assert!(matches!(export::export(&d, &ExportScope::Document, &Default::default()), Err(ExportError::Projection("path text"))));
}
#[test]
fn pdf_export_empty_masks_density_and_cancel_during_nested_drawing() {
    let mut d = scene(rect(), Fill::Solid(Color::BLACK));
    d.layers[0].vector_mask = Some(photocraft_doc::VectorMask { path: Path::default(), enabled: true, linked: true, density: 1., feather: 0. });
    oracle(&d, "empty-reveal-mask");
    d.layers[0].vector_mask.as_mut().unwrap().path.inverted = true;
    let p = render(&output(&d), "empty-inverted-mask", 96.);
    assert!(p.pixels().all(|p| p.0 == [255, 255, 255]));
    d.layers[0].vector_mask.as_mut().unwrap().density = 0.4;
    oracle(&d, "empty-inverted-density");
    let mut g = Layer::group("Nested", d.layers.clone());
    g.opacity = 0.5;
    g.blend = photocraft_doc::BlendMode::Normal;
    d.layers = vec![g];
    let before = d.clone();
    let mut text = photocraft_text::TextEngine::new();
    for limit in 0..24 {
        let calls = std::cell::Cell::new(0usize);
        let cancelled = || {
            let n = calls.get();
            calls.set(n + 1);
            n >= limit
        };
        let result = export::export_with(
            &d,
            &ExportScope::Document,
            &Default::default(),
            &mut export::ExportContext { cancelled: &cancelled, projection: None, text: &mut text },
        );
        assert!(matches!(result, Err(ExportError::Cancelled)) || result.is_ok());
        assert_eq!(d, before);
    }
}
#[test]
fn pdf_export_strokes_dashes_alignment_and_unsupported_warning() {
    for align in [photocraft_doc::StrokeAlign::Center, photocraft_doc::StrokeAlign::Inside, photocraft_doc::StrokeAlign::Outside] {
        let mut d = scene(rect(), Fill::Solid(Color::WHITE));
        let fmt = d.pixel_format();
        let bounds = d.bounds();
        if let LayerContent::Shape(sh) = &mut d.layers[0].content {
            sh.stroke = Some(photocraft_doc::ShapeStroke {
                width: 4.,
                paint: Fill::Solid(Color::BLACK),
                align,
                dashes: vec![3., 2.],
                dash_offset: 0.5,
                cap: photocraft_doc::LineCap::Round,
                join: photocraft_doc::LineJoin::Round,
                ..Default::default()
            });
            sh.cache = Some(photocraft_vector::render_shape(sh, fmt, bounds));
        }
        oracle(&d, &format!("stroke-{align:?}"));
        assert_eq!(images(&Pdf::new(output(&d)).unwrap()), 0);
    }
    let fill = Fill::Gradient {
        stops: vec![(0., Color { alpha: 0.5, ..Color::BLACK }), (1., Color { alpha: 0.5, ..Color::WHITE })],
        style: GradientStyle::Diamond,
        angle: 0.,
        scale: 1.,
        reverse: false,
        opacity_stops: Vec::new(),
        midpoints: Vec::new(),
        offset: (0., 0.),
        dither: false,
        align: true,
    };
    let d = scene(rect(), fill);
    let result = export::export(&d, &ExportScope::Document, &Default::default()).unwrap();
    assert_eq!(result.warnings.len(), 1);
    let pdf = Pdf::new(result.bytes).unwrap();
    assert!(images(&pdf) > 0);
    let image_rect = photocraft_compose::composite_bounds(&d.layers[0], d.bounds()).unwrap().intersect(&d.bounds());
    let rgba = photocraft_compose::render(&d, image_rect).to_rgba8();
    let rgb = rgba.pixels.as_chunks::<4>().0.iter().flat_map(|p| p[..3].iter().copied()).collect::<Vec<_>>();
    assert!(pdf.objects().into_iter().any(|o|matches!(o,Object::Stream(s) if s.dict().get::<Name<'_>>(b"Subtype").is_some_and(|n|n.as_ref()==b"Image") && s.decoded().is_ok_and(|v|v.as_ref()==rgb.as_slice()))),"embedded fallback RGB equals native compositor exactly");
    let alpha = rgba.pixels.as_chunks::<4>().0.iter().map(|p| p[3]).collect::<Vec<_>>();
    assert!(pdf.objects().into_iter().any(|o|matches!(o,Object::Stream(s) if s.dict().get::<Name<'_>>(b"Subtype").is_some_and(|n|n.as_ref()==b"Image") && s.decoded().is_ok_and(|v|v.as_ref()==alpha.as_slice()))),"embedded fallback alpha equals native compositor exactly");
    oracle(&d, "diamond-raster-warning");
}
#[test]
fn pdf_export_text_ligatures_and_mark_fallback_remain_vector() {
    for label in ["office", "e\u{301}", "ABC\nDEF"] {
        let mut d = scene(rect(), Fill::Solid(Color::WHITE));
        let mut t = photocraft_doc::TextLayer {
            text: label.into(),
            font_family: "Inter".into(),
            size_pt: 16.,
            transform: photocraft_geom::Affine::translate(16., 64.),
            ..Default::default()
        };
        photocraft_text::TextEngine::new().render_layer(&mut t, d.resolution_dpi, d.pixel_format());
        d.layers.push(Layer::new("Type", LayerContent::Text(t)));
        let result = export::export(&d, &ExportScope::Document, &Default::default()).unwrap();
        assert_eq!(images(&Pdf::new(result.bytes).unwrap()), 0);
    }
}
#[test]
fn pdf_export_appearance_stack_and_pattern_use_public_renderer() {
    use photocraft_doc::vector_design::appearance::{Appearance, AppearanceItem, FillItem, StrokeItem};
    let mut d = scene(rect(), Fill::Solid(Color::BLACK));
    let mut translucent = FillItem::new(Fill::Solid(Color::rgb(0.2, 0.4, 0.8)));
    translucent.opacity = 0.4;
    let a = Appearance {
        items: vec![
            AppearanceItem::Fill(FillItem::new(Fill::Solid(Color::rgb(0.8, 0.2, 0.1)))),
            AppearanceItem::Fill(translucent),
            AppearanceItem::Stroke(StrokeItem {
                base: photocraft_doc::ShapeStroke { width: 8., paint: Fill::Solid(Color::BLACK), ..Default::default() },
                ..Default::default()
            }),
            AppearanceItem::Stroke(StrokeItem {
                base: photocraft_doc::ShapeStroke { width: 2., paint: Fill::Solid(Color::WHITE), ..Default::default() },
                ..Default::default()
            }),
        ],
    };
    d.layers[0].vector.appearance = Some(a.clone());
    let fmt = d.pixel_format();
    let bounds = d.bounds();
    if let LayerContent::Shape(sh) = &mut d.layers[0].content {
        sh.cache = Some(photocraft_vector::appearance::render_shape(sh, Some(&a), &[], fmt, bounds).unwrap());
    }
    oracle(&d, "appearance-stack");
    assert_eq!(images(&Pdf::new(output(&d)).unwrap()), 0);
    let mut surface = photocraft_doc::Surface::new(fmt);
    surface.write_region(photocraft_geom::Rect::new(0, 0, 2, 2), &[1., 0., 0., 1., 0., 0., 1., 1., 0., 0., 1., 1., 1., 0., 0., 1.]);
    let pattern = photocraft_doc::Pattern::new("Synthetic checker", surface, 2, 2);
    let paint = Fill::Pattern { name: pattern.name.clone(), id: pattern.id.clone(), scale: 1., angle: 0., link: true, phase: (0., 0.) };
    d.patterns.push(pattern);
    d.layers[0].vector.appearance = None;
    if let LayerContent::Shape(sh) = &mut d.layers[0].content {
        sh.fill = Some(paint);
        sh.cache = Some(photocraft_vector::appearance::render_shape(sh, None, &d.patterns, fmt, bounds).unwrap());
    }
    let result = export::export(&d, &ExportScope::Document, &Default::default()).unwrap();
    assert_eq!(result.warnings.len(), 1);
    assert!(images(&Pdf::new(result.bytes).unwrap()) > 0);
    let before = d.clone();
    d.patterns.clear();
    assert!(export::export(&d, &ExportScope::Document, &Default::default()).is_err());
    assert_eq!(before.layers, d.layers);
}

#[test]
fn review_additional_pdf_selection_of_empty_artboard_keeps_page_and_background() {
    let mut doc = scene(rect(), Fill::Solid(Color::BLACK));
    doc.layers.clear();
    let mut board = Layer::group("Empty board", vec![]);
    if let LayerContent::Group(group) = &mut board.content {
        let mut artboard = photocraft_doc::Artboard::new(photocraft_geom::Rect::new(24, 16, 104, 80));
        artboard.background = photocraft_doc::ArtboardBackground::Black;
        group.artboard = Some(artboard);
    }
    let id = board.id;
    doc.layers.push(board);
    let bytes = export::export(&doc, &ExportScope::Selection(vec![id]), &ExportOptions::default()).unwrap().bytes;
    let pdf = Pdf::new(bytes.clone()).unwrap();
    assert_eq!(pdf.pages()[0].base_dimensions(), (60., 48.));
    assert_eq!(render(&bytes, "review-empty-board", 96.).get_pixel(40, 32).0, [0, 0, 0]);
}

mod blends;
