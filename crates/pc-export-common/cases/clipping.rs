//! Writer-independent sibling-clipping cases, compiled into the pc-svg and pc-pdf
//! `export` tests as `clipping::cases`. The including module supplies the format's
//! `check_native`, `check_local_fallback`, `check_closed_fallback`, `check_mask_field`
//! and `profile_export_oracle` assertions.

use super::*;
use photocraft_doc::{BlendMode, GroupIsolation, LayerMask, PixelFormat, Rect, Surface, VectorMask};

fn shape(doc: &Document, name: &str, path: Path, color: Color) -> Layer {
    let mut sh = ShapeLayer { path, fill: Some(Fill::Solid(color)), ..Default::default() };
    sh.cache = Some(photocraft_vector::render_shape(&sh, doc.pixel_format(), doc.bounds()));
    Layer::new(name, LayerContent::Shape(sh))
}
fn fixture(backdrop: f32, siblings: usize) -> Document {
    let mut doc = Document::new("Sibling clipping", Size::new(128, 128), ColorMode::Rgb, SampleType::F32);
    doc.resolution_dpi = 96.0;
    let mut bg = Color::rgb(0.2, 0.4, 0.6);
    bg.alpha = backdrop;
    doc.layers.push(shape(&doc, "Backdrop", photocraft_vector::shapes::rect(0., 0., 128., 128.), bg));
    for i in 0..50 {
        doc.layers.push(shape(
            &doc,
            "Retained vector",
            photocraft_vector::shapes::rect(2. + f64::from(i % 25) * 5., 100. + f64::from(i / 25) * 5., 2., 2.),
            Color::BLACK,
        ));
    }
    let mut text = photocraft_doc::TextLayer {
        text: "Native type".into(),
        font_family: "Inter".into(),
        size_pt: 8.,
        transform: photocraft_doc::Affine::translate(4., 124.),
        ..Default::default()
    };
    photocraft_text::TextEngine::new().render_layer(&mut text, doc.resolution_dpi, doc.pixel_format());
    doc.layers.push(Layer::new("Retained type", LayerContent::Text(text)));
    let mut color = Color::rgb(0.8, 0.6, 0.4);
    color.alpha = 0.6;
    let mut base = shape(&doc, "Base", photocraft_vector::shapes::rect(20.5, 20.5, 64., 64.), color);
    base.opacity = 0.4;
    base.fill_opacity = 0.8;
    base.blend = BlendMode::Screen;
    doc.layers.push(base);
    for i in 0..siblings {
        let mut child = shape(&doc, "Clipped vector", photocraft_vector::shapes::rect(12., 12., 64., 64.), Color::rgb(0.2, 0.8, 0.6));
        child.clipped = true;
        child.opacity = 0.5;
        child.blend = if i == 0 { BlendMode::Multiply } else { BlendMode::Screen };
        doc.layers.push(child);
    }
    doc
}
fn base(doc: &mut Document) -> &mut Layer {
    &mut doc.layers[52]
}
fn verify(doc: &Document, label: &str, mask_images: bool) {
    check_native(doc, label, mask_images);
}

#[test]
fn sibling_clipping_keeps_fifty_paths_type_and_exact_source_atop_alpha() {
    for ab in [0.0_f32, 0.4, 1.0] {
        for count in [1, 2, 3] {
            let doc = fixture(ab, count);
            let pixel = photocraft_compose::flatten(&doc).get(32, 32);
            let mut color = [0.8_f32, 0.6, 0.4];
            for i in 0..count {
                for (c, paint) in color.iter_mut().zip([0.2, 0.8, 0.6]) {
                    let blended = if i == 0 { *c * paint } else { *c + paint - *c * paint };
                    *c = 0.5 * (*c + blended);
                }
            }
            let alpha = 0.6 * 0.4 * 0.8;
            let output_alpha = ab + alpha * (1.0 - ab);
            for (c, background) in [0.2, 0.4, 0.6].into_iter().enumerate() {
                let blended = background + color[c] - background * color[c];
                let expected = (ab * (1.0 - alpha) * background + alpha * ((1.0 - ab) * color[c] + ab * blended)) / output_alpha;
                assert!((pixel[c] - expected).abs() < 0.00001, "{pixel:?} expected {expected}");
            }
            assert!((pixel[3] - output_alpha).abs() < 0.00001);
            verify(&doc, &format!("clipping-{ab}-{count}"), false);
        }
    }
}
#[test]
fn clipping_geometry_masks_and_stroke_order_keep_vector_artwork() {
    for kind in ["evenodd", "inverted", "stroke", "cancelled-fill-stroke", "pixel", "feather"] {
        let mut doc = fixture(0.4, 2);
        let bounds = doc.bounds();
        let format = doc.pixel_format();
        let b = base(&mut doc);
        match kind {
            "evenodd" => {
                if let LayerContent::Shape(sh) = &mut b.content {
                    sh.path.subpaths.push(photocraft_doc::Subpath::polygon(&[(40., 40.), (64., 40.), (64., 64.), (40., 64.)]));
                    sh.path.fill_rule = photocraft_doc::FillRule::EvenOdd;
                }
            }
            "inverted" => b.vector_mask = Some(VectorMask::new(Path { inverted: true, ..photocraft_vector::shapes::rect(45., 0., 10., 128.) })),
            "stroke" => {
                if let LayerContent::Shape(sh) = &mut b.content {
                    sh.stroke = Some(photocraft_doc::ShapeStroke { width: 8., paint: Fill::Solid(Color::rgb(0.0, 0.0, 1.0)), ..Default::default() });
                }
            }
            "cancelled-fill-stroke" => {
                if let LayerContent::Shape(sh) = &mut b.content {
                    let mut subtract = sh.path.subpaths[0].clone();
                    subtract.op = photocraft_doc::PathOp::Subtract;
                    sh.path.subpaths.push(subtract);
                    sh.stroke = Some(photocraft_doc::ShapeStroke { width: 8., paint: Fill::Solid(Color::rgb(0., 0., 1.)), ..Default::default() });
                }
            }
            "pixel" => {
                let mut mask = LayerMask::reveal_all();
                mask.surface.fill_rect(Rect::new(16, 16, 48, 96), &[0.4]);
                b.mask = Some(mask);
            }
            "feather" => {
                let mut mask = VectorMask::new(photocraft_vector::shapes::rect(28., 24., 44., 52.));
                mask.density = 0.7;
                mask.feather = 2.;
                b.vector_mask = Some(mask);
            }
            _ => unreachable!(),
        }
        if let LayerContent::Shape(sh) = &mut b.content {
            sh.cache = Some(photocraft_vector::render_shape(sh, format, bounds));
        }
        if kind == "cancelled-fill-stroke" {
            assert!(photocraft_compose::flatten(&doc).get(20, 32)[2] > 0.6, "empty fill must retain its stroke");
        }
        verify(&doc, &format!("clip-{kind}"), matches!(kind, "pixel" | "feather"));
    }
}
#[test]
fn raster_base_alpha_keeps_clipped_vector_children_and_does_not_increase_alpha() {
    let mut doc = fixture(0.0, 2);
    let mut surface = Surface::new(PixelFormat::RGBA32F);
    surface.fill_rect(Rect::new(20, 20, 85, 85), &[0.8, 0.6, 0.4, 0.6]);
    surface.fill_rect(Rect::new(40, 20, 60, 85), &[0.8, 0.6, 0.4, 0.2]);
    base(&mut doc).content = LayerContent::Raster(surface);
    let pixels = photocraft_compose::flatten(&doc);
    assert!((pixels.get(32, 32)[3] - 0.192).abs() < 0.00001);
    assert!((pixels.get(48, 32)[3] - 0.064).abs() < 0.00001);
    verify(&doc, "raster-alpha-clipping", true);
}
#[test]
fn clipping_visibility_orphans_empty_and_partial_dependency_are_explicit() {
    let mut doc = fixture(0.0, 2);
    base(&mut doc).visible = false;
    let hidden = photocraft_compose::flatten(&doc);
    assert_eq!(hidden.get(32, 32), [0.0; 4]);
    verify(&doc, "hidden-clipping-base", false);
    base(&mut doc).visible = true;
    doc.layers.remove(52);
    // A clipped layer after the retained type would clip to that type. Move
    // these to the start of the real sibling list to create genuine orphans.
    let orphans = doc.layers.split_off(52);
    doc.layers.splice(0..0, orphans);
    // CPU treats an orphan clipped layer as an ordinary source, without
    // gathering the next clipped sibling; export must match that behavior.
    verify(&doc, "orphan-clipped", false);
    let mut doc = fixture(0.0, 1);
    let child = doc.layers.last().unwrap().id;
    assert!(export::export(&doc, &ExportScope::Selection(vec![child]), &Default::default()).is_err());
    let b = base(&mut doc).id;
    assert!(export::export(&doc, &ExportScope::Selection(vec![b, child]), &Default::default()).is_ok());
    let empty = ShapeLayer { fill: Some(Fill::Solid(Color::BLACK)), ..Default::default() };
    base(&mut doc).content = LayerContent::Shape(empty);
    verify(&doc, "empty-clipping-base", false);
}
#[test]
fn isolated_nested_chain_and_dependent_group_keep_their_distinct_boundaries() {
    let mut doc = fixture(1.0, 2);
    let children = doc.layers.split_off(52);
    let mut group = Layer::group("Isolated chain", children);
    group.blend = BlendMode::Normal;
    if let LayerContent::Group(g) = &mut group.content {
        g.isolation = GroupIsolation::Isolated;
    }
    doc.layers.push(group);
    verify(&doc, "nested-native-clipping", false);
    let b = doc.layers.len() - 1;
    if let LayerContent::Group(g) = &mut doc.layers[b].content {
        g.isolation = GroupIsolation::NonIsolated;
    }
    doc.layers[b].blend = BlendMode::Multiply;
    let result = export::export(&doc, &ExportScope::Document, &Default::default()).unwrap();
    assert!(result.warnings.iter().any(|w| w.feature == "compositing"));
}
#[test]
fn clipping_cancel_and_byte_budget_leave_source_unchanged() {
    let doc = fixture(0.4, 3);
    let before = doc.clone();
    let mut text = photocraft_text::TextEngine::new();
    for limit in [0, 10, 80, 180] {
        let calls = std::cell::Cell::new(0);
        let cancelled = || {
            let n = calls.get();
            calls.set(n + 1);
            n >= limit
        };
        let result = export::export_with(
            &doc,
            &ExportScope::Document,
            &Default::default(),
            &mut export::ExportContext { cancelled: &cancelled, projection: None, text: &mut text },
        );
        if limit == 0 {
            assert!(matches!(result, Err(ExportError::Cancelled)));
        } else {
            assert!(matches!(result, Err(ExportError::Cancelled)) || result.is_ok());
        }
        assert_eq!(doc, before);
    }
    assert!(export::export(&doc, &ExportScope::Document, &ExportOptions { max_bytes: 32, ..Default::default() }).is_err());
}

#[test]
fn unsupported_independent_chain_is_cropped_without_flattening_unrelated_artwork() {
    for kind in ["gradient-base", "gamma-text-child", "adjustment-child"] {
        let mut doc = fixture(0.4, 1);
        let format = doc.pixel_format();
        let bounds = doc.bounds();
        match kind {
            "gradient-base" => {
                if let LayerContent::Shape(shape) = &mut base(&mut doc).content {
                    shape.fill = Some(Fill::Gradient {
                        method: Default::default(),
                        stops: vec![(0., Color::rgb(0.8, 0.2, 0.4)), (1., Color::rgb(0.2, 0.8, 0.6))],
                        style: GradientStyle::Linear,
                        angle: 0.,
                        scale: 1.,
                        reverse: false,
                        opacity_stops: vec![(0., 0.2), (1., 0.8)],
                        midpoints: vec![],
                        offset: (0., 0.),
                        dither: false,
                        align: true,
                    });
                    shape.cache = Some(photocraft_vector::render_shape(shape, format, bounds));
                }
            }
            "gamma-text-child" => {
                let mut text = photocraft_doc::TextLayer {
                    text: "Clip".into(),
                    font_family: "Inter".into(),
                    size_pt: 18.,
                    transform: photocraft_doc::Affine::translate(28., 56.),
                    ..Default::default()
                };
                photocraft_text::TextEngine::new().render_layer(&mut text, doc.resolution_dpi, format);
                doc.layers.last_mut().unwrap().content = LayerContent::Text(text);
            }
            "adjustment-child" => doc.layers.last_mut().unwrap().content = LayerContent::Adjustment(photocraft_doc::Adjustment::Invert),
            _ => unreachable!(),
        }
        check_local_fallback(&doc, kind);
    }
}

#[test]
fn tagged_cmyk_base_uses_document_profile_in_bounded_chain_fallback() {
    // A valid authored variant of the repository's CC0 coated CMYK profile:
    // scale only CLUT L* nodes, preserving the mft2 tables and tag directory.
    let mut bytes = include_bytes!("../../pc-cms/profiles/photocraft-coated-cmyk.icc").to_vec();
    let u32_at = |data: &[u8], at: usize| u32::from_be_bytes(data[at..at + 4].try_into().unwrap()) as usize;
    let count = u32_at(&bytes, 128);
    let tag = (0..count).map(|i| 132 + i * 12).find(|at| &bytes[*at..*at + 4] == b"A2B1").unwrap();
    let offset = u32_at(&bytes, tag + 4);
    assert_eq!(&bytes[offset..offset + 4], b"mft2");
    let inputs = usize::from(bytes[offset + 8]);
    let outputs = usize::from(bytes[offset + 9]);
    let grid = usize::from(bytes[offset + 10]);
    let entries = usize::from(u16::from_be_bytes(bytes[offset + 48..offset + 50].try_into().unwrap()));
    let start = offset + 52 + inputs * entries * 2;
    for node in 0..grid.pow(inputs as u32) {
        let at = start + node * outputs * 2;
        let value = u16::from_be_bytes(bytes[at..at + 2].try_into().unwrap());
        bytes[at..at + 2].copy_from_slice(&((u32::from(value) * 3 / 4) as u16).to_be_bytes());
    }
    let mut doc = fixture(0.4, 1);
    doc.mode = ColorMode::Cmyk;
    let mut surface = Surface::new(PixelFormat::new(ColorMode::Cmyk, SampleType::F32, true));
    surface.fill_rect(Rect::new(20, 20, 85, 85), &[0.7, 0.2, 0.4, 0.1, 0.6]);
    base(&mut doc).content = LayerContent::Raster(surface);
    // Keep this control about source-profile conversion, independent of vector
    // cache regeneration in the existing CMYK raster fallback.
    let mut child = Surface::new(PixelFormat::RGBA32F);
    child.fill_rect(Rect::new(12, 12, 76, 76), &[0.2, 0.8, 0.6, 1.0]);
    doc.layers.last_mut().unwrap().content = LayerContent::Raster(child);
    let untagged = photocraft_compose::flatten(&doc).get(32, 32);
    doc.icc_profile = Some(std::sync::Arc::new(bytes));
    assert!(photocraft_compose::cmyk_space(&doc).is_some(), "nondefault CMYK profile must be active");
    let tagged = photocraft_compose::flatten(&doc).get(32, 32);
    assert!((0..3).any(|c| (tagged[c] - untagged[c]).abs() > 0.01), "profile control must change color");
    assert!((tagged[3] - (0.4 + 0.192 * 0.6)).abs() < 0.00001);
    check_local_fallback(&doc, "tagged-cmyk-clipping");
}

fn propagation_profile() -> photocraft_cms::Profile {
    photocraft_cms::synth::cmyk_profile(&photocraft_cms::synth::CmykParams {
        description: "Export profile propagation".into(),
        tvi: [0.3, 0.28, 0.26, 0.32],
        grid_a2b: 7,
        grid_b2a: 11,
        ..Default::default()
    })
}
fn cms_eval<const N: usize>(transform: &photocraft_cms::Transform, input: &[f32]) -> [f32; N] {
    let mut out = [0.; N];
    transform.eval(input, &mut out);
    out
}
#[test]
fn tagged_cmyk_vector_and_text_cache_rebuild_matches_independent_cms_inks() {
    use photocraft_cms::{Builtin, Intent, Transform};
    let profile = propagation_profile();
    let a2b = Transform::new(&profile, Builtin::Srgb.profile(), Intent::RelativeColorimetric, true).unwrap();
    let b2a = Transform::new(Builtin::Srgb.profile(), &profile, Intent::RelativeColorimetric, true).unwrap();
    let default_a2b = Transform::new(Builtin::CoatedCmyk.profile(), Builtin::Srgb.profile(), Intent::RelativeColorimetric, true).unwrap();
    let default_b2a = Transform::new(Builtin::Srgb.profile(), Builtin::CoatedCmyk.profile(), Intent::RelativeColorimetric, true).unwrap();
    let inks = [[0.7, 0.1, 0.2, 0.1], [0.1, 0.6, 0.3, 0.2]];
    let rgbs = [[0.25, 0.6, 0.3], [0.7, 0.25, 0.5]];
    assert!((0..3).any(|c| (cms_eval::<3>(&a2b, &inks[0])[c] - cms_eval::<3>(&default_a2b, &inks[0])[c]).abs() > 0.01));
    assert!((0..4).any(|c| (cms_eval::<4>(&b2a, &rgbs[0])[c] - cms_eval::<4>(&default_b2a, &rgbs[0])[c]).abs() > 0.01));
    for kind in ["rgb-solid", "cmyk-solid", "rgb-gradient", "cmyk-gradient", "rgb-text"] {
        let mut doc = Document::new("Profile fallback", Size::new(128, 768), ColorMode::Cmyk, SampleType::F32);
        doc.resolution_dpi = 96.;
        doc.icc_profile = Some(profile.to_bytes());
        let bounds = doc.bounds();
        let format = doc.pixel_format();
        let mut surface = Surface::new(format);
        surface.fill_rect(bounds, &[0.5, 0.2, 0.3, 0.1, 0.6]);
        let mut base = Layer::new("CMYK base", LayerContent::Raster(surface));
        base.opacity = 0.4;
        base.fill_opacity = 0.8;
        let cmyk = kind.starts_with("cmyk");
        let gradient = kind.ends_with("gradient");
        let colors: [Color; 2] = std::array::from_fn(|i| {
            if cmyk { Color { mode: ColorMode::Cmyk, c: inks[i], alpha: 0.5 } } else { Color::rgba(rgbs[i][0], rgbs[i][1], rgbs[i][2], 0.5) }
        });
        let mut reference_cache = Surface::new(format);
        let mut child = if kind == "rgb-text" {
            let text = photocraft_doc::TextLayer {
                text: "Color\nProfile\nBands".into(),
                font_family: "Inter".into(),
                size_pt: 22.,
                color: Color { alpha: 1.0, ..colors[0] },
                transform: photocraft_doc::Affine::translate(4., 80.),
                ..Default::default()
            };
            // Glyph coverage is rendered in RGB white. Its color is not used:
            // the reference inks come only from the independent CMS transform.
            let mut coverage = text.clone();
            coverage.color = Color::WHITE;
            let mut engine = photocraft_text::TextEngine::new();
            engine.render_layer(&mut coverage, doc.resolution_dpi, PixelFormat::RGBA32F);
            let alpha = coverage.cache.as_ref().unwrap();
            let reference_ink = cms_eval::<4>(&b2a, &rgbs[0]);
            for y in 0..768 {
                let mut row = Vec::new();
                for x in 0..128 {
                    row.extend(reference_ink);
                    row.push(alpha.rgba(x, y)[3]);
                }
                reference_cache.write_region(Rect::new(0, y, 128, y + 1), &row);
            }
            Layer::new("RGB-authored text", LayerContent::Text(text))
        } else {
            let paint = if gradient {
                Fill::Gradient {
                    method: Default::default(),
                    stops: vec![(0., colors[0]), (1., colors[1])],
                    style: GradientStyle::Linear,
                    angle: 0.,
                    scale: 1.,
                    reverse: false,
                    opacity_stops: vec![],
                    midpoints: vec![],
                    offset: (0., 0.),
                    dither: false,
                    align: true,
                }
            } else {
                Fill::Solid(colors[0])
            };
            let rgb: [[f32; 3]; 2] = std::array::from_fn(|i| if cmyk { cms_eval(&a2b, &inks[i]) } else { rgbs[i] });
            let mut row = Vec::new();
            for x in 0..128 {
                let t = if gradient { x as f32 / 128. } else { 0. };
                let color: [f32; 3] = std::array::from_fn(|c| rgb[0][c] * (1. - t) + rgb[1][c] * t);
                row.extend(cms_eval::<4>(&b2a, &color));
                row.push(0.5);
            }
            for y in 0..768 {
                reference_cache.write_region(Rect::new(0, y, 128, y + 1), &row);
            }
            Layer::new(
                "Authored vector",
                LayerContent::Shape(ShapeLayer { path: photocraft_vector::shapes::rect(0., 0., 128., 768.), fill: Some(paint), ..Default::default() }),
            )
        };
        child.clipped = true;
        child.opacity = if kind == "rgb-text" { 1.0 } else { 0.6 };
        child.blend = if kind == "rgb-text" { BlendMode::Normal } else { BlendMode::Multiply };
        doc.layers = vec![base, child];
        let glyph = if kind == "rgb-text" {
            let point = (0..768).flat_map(|y| (0..128).map(move |x| (x, y))).find(|(x, y)| reference_cache.sample_channel(*x, *y, 4) > 0.99);
            Some(point.expect("the text reference must contain an opaque interior glyph pixel"))
        } else {
            None
        };
        let original = doc.clone();
        let mut reference = doc.clone();
        match &mut reference.layers[1].content {
            LayerContent::Shape(shape) => shape.cache = Some(reference_cache),
            LayerContent::Text(text) => text.cache = Some(reference_cache),
            _ => unreachable!(),
        }
        let pixels = photocraft_compose::flatten(&reference);
        for y in [1, 257, 513] {
            assert!((pixels.get(64, y)[3] - 0.192).abs() < 0.00001);
        }
        if let Some((x, y)) = glyph {
            let mut wrong = reference.clone();
            let wrong_ink = cms_eval::<4>(&default_b2a, &rgbs[0]);
            if let LayerContent::Text(text) = &mut wrong.layers[1].content {
                text.cache.as_mut().unwrap().write_region(Rect::new(x, y, x + 1, y + 1), &[wrong_ink[0], wrong_ink[1], wrong_ink[2], wrong_ink[3], 1.0]);
            }
            let wrong = photocraft_compose::flatten(&wrong).get(x, y);
            let expected = pixels.get(x, y);
            assert!((0..3).any(|c| (wrong[c] - expected[c]).abs() > 3. / 255.), "glyph control must detect use of the default B2A profile");
        }
        profile_export_oracle(&doc, &reference, &format!("profile-{kind}"), glyph);
        assert_eq!(doc, original, "cache preparation is detached");
    }
}

fn text_base_fixture(backdrop: f32) -> Document {
    let mut doc = fixture(backdrop, 2);
    let mut text = photocraft_doc::TextLayer {
        text: "BO".into(),
        font_family: "Inter".into(),
        size_pt: 40.,
        color: Color::rgba(0.8, 0.6, 0.4, 0.6),
        transform: photocraft_doc::Affine::translate(16., 80.),
        ..Default::default()
    };
    photocraft_text::TextEngine::new().render_layer(&mut text, doc.resolution_dpi, doc.pixel_format());
    base(&mut doc).content = LayerContent::Text(text);
    doc
}
#[test]
fn gamma_off_uniform_text_base_stays_native() {
    const CHILD: &str = "LOCAL_IMAGE_GAMMA_OFF_CLIP_CHILD";
    if std::env::var_os(CHILD).is_none() {
        use std::process::{Command, Stdio};
        use std::time::{Duration, Instant};
        let gamma = photocraft_compose::psblend::text_gamma();
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/text-clip-qa").join(format!(
            "{}-{}",
            env!("CARGO_PKG_NAME"),
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("gamma-off.log");
        let log = std::fs::File::create(&path).unwrap();
        // Inherit the parent's private profile/fonts unchanged; mutate
        // only this exact-test subprocess's gamma setting.
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "clipping::cases::gamma_off_uniform_text_base_stays_native", "--test-threads=1", "--nocapture"])
            .env(CHILD, "1")
            .stdout(Stdio::from(log.try_clone().unwrap()))
            .stderr(Stdio::from(log))
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(120);
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
                outcome => {
                    let _ = child.kill();
                    let _ = child.wait();
                    panic!("gamma-off test child failed or timed out: {outcome:?}; {}", path.display());
                }
            }
        };
        let mut detail = String::new();
        use std::io::Read;
        std::fs::File::open(&path).unwrap().take(64 << 10).read_to_string(&mut detail).unwrap();
        assert!(status.success(), "gamma-off isolated child failed: {detail}");
        assert_eq!(photocraft_compose::psblend::text_gamma(), gamma);
        return;
    }
    photocraft_compose::psblend::set_text_gamma(1.0);
    assert_eq!(photocraft_compose::psblend::text_gamma(), 1.0);
    for backdrop in [0.0, 0.4, 1.0] {
        for kind in ["plain", "transformed", "soft-mask"] {
            let mut doc = text_base_fixture(backdrop);
            let format = doc.pixel_format();
            let dpi = doc.resolution_dpi;
            let layer = base(&mut doc);
            if kind == "transformed" {
                if let LayerContent::Text(text) = &mut layer.content {
                    text.transform = text.transform.mul(&photocraft_doc::Affine::rotate(-0.12));
                    text.runs = vec![photocraft_doc::text::TextRun {
                        len: text.text.len(),
                        style: photocraft_doc::text::CharStyle {
                            font_family: "Inter".into(),
                            size_pt: 40.,
                            color: text.color,
                            underline: true,
                            ..Default::default()
                        },
                    }];
                    photocraft_text::TextEngine::new().render_layer(text, dpi, format);
                }
            } else if kind == "soft-mask" {
                let mut mask = LayerMask::reveal_all();
                mask.surface.fill_rect(Rect::new(8, 16, 56, 96), &[0.4]);
                layer.mask = Some(mask);
                let mut vector = VectorMask::new(photocraft_vector::shapes::rect(20., 24., 72., 64.));
                vector.feather = 1.;
                vector.density = 0.8;
                layer.vector_mask = Some(vector);
            }
            let before = doc.clone();
            verify(&doc, &format!("gamma-off-text-{backdrop}-{kind}"), kind == "soft-mask");
            assert_eq!(doc, before);
            let text = if let LayerContent::Text(text) = &base(&mut doc).content { text.clone() } else { unreachable!() };
            let coverage = text.cache.as_ref().unwrap();
            let edge = (0..96)
                .flat_map(|y| (0..112).map(move |x| (x, y)))
                .find(|(x, y)| {
                    let a = coverage.rgba(*x, *y)[3];
                    a > 0.05 && a < 0.5
                })
                .expect("glyph edge coverage");
            let interior = (0..96).flat_map(|y| (0..112).map(move |x| (x, y))).find(|(x, y)| coverage.rgba(*x, *y)[3] > 0.59).expect("glyph interior coverage");
            // Clipped paints cannot alter the base's source-atop alpha, at
            // either the partial glyph edge or its fully covered interior.
            let mut unclipped = doc.clone();
            unclipped.layers.truncate(53);
            let a = photocraft_compose::flatten(&doc);
            let b = photocraft_compose::flatten(&unclipped);
            for (x, y) in [edge, interior] {
                assert!((a.get(x, y)[3] - b.get(x, y)[3]).abs() < 0.00001);
            }
        }
    }
    let mut multicolor = text_base_fixture(0.4);
    let format = multicolor.pixel_format();
    let dpi = multicolor.resolution_dpi;
    if let LayerContent::Text(text) = &mut base(&mut multicolor).content {
        text.runs = vec![
            photocraft_doc::text::TextRun {
                len: 1,
                style: photocraft_doc::text::CharStyle { font_family: "Inter".into(), size_pt: 40., color: text.color, ..Default::default() },
            },
            photocraft_doc::text::TextRun {
                len: 1,
                style: photocraft_doc::text::CharStyle {
                    font_family: "Inter".into(),
                    size_pt: 40.,
                    color: Color::rgba(0.2, 0.7, 0.3, 0.6),
                    ..Default::default()
                },
            },
        ];
        photocraft_text::TextEngine::new().render_layer(text, dpi, format);
    }
    check_local_fallback(&multicolor, "gamma-off-multicolor-text");
    let mut doc = text_base_fixture(0.4);
    doc.layers.drain(0..52);
    let before = doc.clone();
    let calls = std::cell::Cell::new(0usize);
    let count = || {
        calls.set(calls.get() + 1);
        false
    };
    let mut text = photocraft_text::TextEngine::new();
    export::export_with(&doc, &ExportScope::Document, &Default::default(), &mut export::ExportContext { cancelled: &count, projection: None, text: &mut text })
        .unwrap();
    let stop = calls.get() / 2;
    assert!(stop > 0);
    calls.set(0);
    let cancel = || {
        let n = calls.get();
        calls.set(n + 1);
        n >= stop
    };
    let mut text = photocraft_text::TextEngine::new();
    assert!(matches!(
        export::export_with(
            &doc,
            &ExportScope::Document,
            &Default::default(),
            &mut export::ExportContext { cancelled: &cancel, projection: None, text: &mut text }
        ),
        Err(ExportError::Cancelled)
    ));
    assert!(export::export(&doc, &ExportScope::Document, &ExportOptions { max_bytes: 32, ..Default::default() }).is_err());
    assert_eq!(doc, before);
}
fn isolated_text_fixture(backdrop: f32) -> Document {
    let mut doc = text_base_fixture(backdrop);
    let mut children = doc.layers.split_off(52);
    children.insert(0, shape(&doc, "Internal backdrop", photocraft_vector::shapes::rect(8., 28., 100., 62.), Color::rgba(0.3, 0.5, 0.7, 0.65)));
    let mut group = Layer::group("Closed text group", children);
    group.blend = BlendMode::Multiply;
    group.opacity = 0.7;
    group.fill_opacity = 0.8;
    if let LayerContent::Group(g) = &mut group.content {
        g.isolation = GroupIsolation::Isolated;
    }
    doc.layers.push(group);
    doc
}
#[test]
fn default_gamma_text_chain_rasterizes_only_its_closed_isolated_parent() {
    assert_eq!(photocraft_compose::psblend::text_gamma(), photocraft_compose::psblend::TEXT_GAMMA);
    for backdrop in [0.0, 0.4, 1.0] {
        for mask in [false, true] {
            let mut doc = isolated_text_fixture(backdrop);
            if mask {
                let group = doc.layers.last_mut().unwrap();
                let mut pixels = LayerMask::reveal_all();
                pixels.surface.fill_rect(Rect::new(8, 24, 56, 100), &[0.4]);
                group.mask = Some(pixels);
                group.vector_mask = Some(VectorMask::new(photocraft_vector::shapes::rect(12., 32., 90., 48.)));
            }
            let before = doc.clone();
            check_closed_fallback(&doc, &format!("closed-text-{backdrop}-{mask}"));
            assert_eq!(doc, before);
            let group = doc.layers.last().unwrap().id;
            assert!(export::export(&doc, &ExportScope::Selection(vec![group]), &Default::default()).is_ok());
        }
    }
    let mut doc = isolated_text_fixture(0.4);
    if let LayerContent::Group(g) = &mut doc.layers.last_mut().unwrap().content {
        g.isolation = GroupIsolation::Auto;
    }
    check_closed_fallback(&doc, "legacy-normal-closed-text");
}
#[test]
fn text_clipping_external_dependencies_and_parent_effects_keep_whole_backdrop_guard() {
    let root = text_base_fixture(0.4);
    let out = export::export(&root, &ExportScope::Document, &Default::default()).unwrap();
    assert!(out.warnings.iter().any(|w| w.feature == "compositing"), "default-gamma root text remains backdrop-dependent");
    for kind in ["nonisolated", "blend-if", "channels", "effects"] {
        let mut doc = isolated_text_fixture(0.4);
        let group = doc.layers.last_mut().unwrap();
        match kind {
            "nonisolated" => {
                if let LayerContent::Group(g) = &mut group.content {
                    g.isolation = GroupIsolation::NonIsolated;
                }
            }
            "blend-if" => group.blend_if.set(0, [photocraft_doc::BlendRange::FULL, photocraft_doc::BlendRange { black: [60, 160], white: [255, 255] }]),
            "channels" => group.excluded_channels = 1,
            "effects" => group.effects.items.push(photocraft_doc::Effect::default_drop_shadow()),
            _ => unreachable!(),
        }
        let out = export::export(&doc, &ExportScope::Document, &Default::default()).unwrap();
        assert!(out.warnings.iter().any(|w| w.feature == "compositing"), "{kind} cannot be flattened against a substituted transparent backdrop");
    }
}

#[test]
fn cropped_soft_mask_matches_full_canvas_field_at_halo_and_canvas_edges() {
    for (label, canvas, rect, pixel_feather, vector_feather, disable_pixel, disable_vector) in [
        ("glyph-crop", Size::new(128, 128), Rect::new(19, 39, 90, 82), 0.0, 1.0, false, false),
        ("unequal-blurs", Size::new(128, 128), Rect::new(19, 39, 90, 82), 2.0, 1.0, false, false),
        ("vector-larger", Size::new(128, 128), Rect::new(19, 39, 90, 82), 1.0, 3.0, false, false),
        ("off-canvas-selection", Size::new(16, 16), Rect::new(-4, -3, 8, 7), 1.0, 2.0, false, false),
        ("disabled-vector", Size::new(128, 128), Rect::new(19, 39, 90, 82), 0.0, 1000.0, false, true),
        ("disabled-pixel", Size::new(128, 128), Rect::new(19, 39, 90, 82), 1000.0, 1.0, true, false),
    ] {
        let mut doc = Document::new(label, canvas, ColorMode::Rgb, SampleType::F32);
        doc.resolution_dpi = 96.;
        let mut layer = shape(
            &doc,
            "coverage",
            photocraft_vector::shapes::rect(f64::from(rect.x0), f64::from(rect.y0), f64::from(rect.width()), f64::from(rect.height())),
            Color::rgb(0.3, 0.6, 0.8),
        );
        let mut pixel = LayerMask::reveal_all();
        pixel.feather = pixel_feather;
        pixel.enabled = !disable_pixel;
        pixel.surface.fill_rect(if rect.x0 < 0 { Rect::new(-2, -2, 4, 12) } else { Rect::new(8, 16, 56, 96) }, &[0.4]);
        layer.mask = Some(pixel);
        let mut vector =
            VectorMask::new(if rect.x0 < 0 { photocraft_vector::shapes::rect(0., 0., 4., 6.) } else { photocraft_vector::shapes::rect(20., 24., 72., 64.) });
        vector.density = 0.8;
        vector.feather = vector_feather;
        vector.enabled = !disable_vector;
        layer.vector_mask = Some(vector);
        doc.layers = vec![layer];
        let before = doc.clone();
        let out = export::export(&doc, &ExportScope::Selection(vec![doc.layers[0].id]), &Default::default()).unwrap();
        check_mask_field(&doc, &out.bytes, rect, label);
        assert_eq!(doc, before);
    }
}
pub(super) fn reference_mask_bytes(doc: &Document, rect: Rect) -> Vec<u8> {
    let layer = &doc.layers[0];
    let full = photocraft_compose::masks::combined_mask(layer, doc.bounds());
    let mut values = Vec::new();
    for y in rect.y0..rect.y1 {
        for x in rect.x0..rect.x1 {
            let value = full.as_ref().map_or_else(|| layer.mask.as_ref().filter(|m| m.enabled).map_or(1., |m| m.value(x, y)), |m| m.sample_channel(x, y, 0));
            values.push((value.clamp(0., 1.) * 255. + 0.5) as u8);
        }
    }
    values
}
#[test]
fn mask_halo_budget_cancel_and_nonfinite_refusal_preserve_source() {
    let mut doc = Document::new("halo admission", Size::new(128, 128), ColorMode::Rgb, SampleType::F32);
    let mut layer = shape(&doc, "small selected mask", photocraft_vector::shapes::rect(30., 30., 4., 4.), Color::BLACK);
    let mut mask = VectorMask::new(photocraft_vector::shapes::rect(31., 31., 3., 3.));
    mask.feather = 2.;
    layer.vector_mask = Some(mask);
    doc.layers = vec![layer];
    let scope = ExportScope::Selection(vec![doc.layers[0].id]);
    let before = doc.clone();
    assert!(
        matches!(export::export(&doc, &scope, &ExportOptions { max_pixels: 16, ..Default::default() }), Err(ExportError::Budget("pixels"))),
        "output fits16 pixels but its blur support must be admitted before allocation"
    );
    let calls = std::cell::Cell::new(0usize);
    let count = || {
        calls.set(calls.get() + 1);
        false
    };
    let mut text = photocraft_text::TextEngine::new();
    export::export_with(&doc, &scope, &Default::default(), &mut export::ExportContext { cancelled: &count, projection: None, text: &mut text }).unwrap();
    let stop = calls.get() / 2;
    assert!(stop > 0);
    calls.set(0);
    let cancel = || {
        let n = calls.get();
        calls.set(n + 1);
        n >= stop
    };
    let mut text = photocraft_text::TextEngine::new();
    assert!(matches!(
        export::export_with(&doc, &scope, &Default::default(), &mut export::ExportContext { cancelled: &cancel, projection: None, text: &mut text }),
        Err(ExportError::Cancelled)
    ));
    assert_eq!(doc, before);
    doc.layers[0].vector_mask.as_mut().unwrap().feather = f32::NAN;
    assert!(export::export(&doc, &scope, &Default::default()).is_err());
    assert!(doc.layers[0].vector_mask.as_ref().unwrap().feather.is_nan());
}
