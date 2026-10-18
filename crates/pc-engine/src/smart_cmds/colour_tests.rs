use super::*;
use photocraft_cms::Builtin;
use photocraft_color::SampleType;
use photocraft_doc::DevelopLink;

fn document(mode: ColorMode, depth: SampleType, profile: &Profile, pixels: &[f32]) -> Document {
    let fmt = PixelFormat::new(mode, depth, true);
    let mut doc = Document::new("smart colour fixture", Size::new((pixels.len() / fmt.channels()) as u32, 1), mode, depth);
    doc.icc_profile = Some(profile.to_bytes());
    let mut layer = Layer::raster("Pixels", fmt);
    layer.surface_mut().unwrap().write_region(doc.bounds(), pixels);
    doc.layers.push(layer);
    doc
}
fn close(actual: &[f32], expected: &[f32], epsilon: f32) {
    assert_eq!(actual.len(), expected.len());
    let error = actual.iter().zip(expected).map(|(a, b)| (a - b).abs()).fold(0.0, f32::max);
    assert!(error <= epsilon, "maximum colour error {error} > {epsilon}");
}
fn convert(pixels: &[f32], source: &Profile, target: &Profile, channels: usize) -> Vec<f32> {
    let transform = Transform::new(source, target, Intent::RelativeColorimetric, true).unwrap();
    let mut out = vec![0.0; pixels.len() / 4 * channels];
    transform.convert_f32(pixels, 4, &mut out, channels, true);
    out
}
fn custom_cmyk() -> Profile {
    photocraft_cms::synth::cmyk_profile(&photocraft_cms::synth::CmykParams {
        description: "Smart uncoated fixture".into(),
        tvi: [0.26, 0.26, 0.26, 0.3],
        grid_a2b: 5,
        grid_b2a: 9,
        ..Default::default()
    })
}

#[test]
fn smart_colour_cache_separates_destination_profiles_and_preserves_alpha() {
    let pixels = [0.25, 0.45, 0.7, 0.5, 0.7, 0.25, 0.4, 1.0, 0.02, 0.9, 0.2, 0.75];
    let source = document(ColorMode::Rgb, SampleType::F32, Builtin::DisplayP3.profile(), &pixels);
    let bytes = encode_source(&source).unwrap();
    let src = crate::color_cmds::document_profile(&source);
    let mut images = Vec::new();
    for target in [Builtin::Srgb.profile(), Builtin::ProPhotoCompat.profile()] {
        let mut parent = Document::new("Destination", source.size, ColorMode::Rgb, SampleType::F32);
        parent.icc_profile = Some(target.to_bytes());
        let target = crate::color_cmds::document_profile(&parent);
        let sm = SmartObject::new(SmartSource::Embedded { file_name: "source.pcraft".into(), bytes: Arc::new(bytes.clone()) }, Affine::IDENTITY, None);
        let rendered = render(&parent, &sm).unwrap().unwrap();
        let expected = convert(&pixels, &src, &target, 4);
        close(&rendered.read_region(parent.bounds()), &expected, 3e-6);
        for (actual, pixel) in rendered.read_region(parent.bounds()).as_chunks::<4>().0.iter().zip(pixels.as_chunks::<4>().0) {
            assert_eq!(actual[3], pixel[3]);
        }
        let cached = source_image_in_profile("source.pcraft", &bytes, parent.pixel_format(), &target).unwrap();
        let repeated = source_image_in_profile("source.pcraft", &bytes, parent.pixel_format(), &target).unwrap();
        assert!(Arc::ptr_eq(&cached.surface, &repeated.surface), "same target should reuse decode");
        images.push(cached);
    }
    assert!(!Arc::ptr_eq(&images[0].surface, &images[1].surface));
    assert_ne!(images[0].surface.read_region(source.bounds()), images[1].surface.read_region(source.bounds()));
}

#[test]
fn smart_colour_channel_models_use_profile_conversion_before_quantization() {
    let pixels = [0.25, 0.45, 0.7, 0.5, 0.7, 0.25, 0.4, 1.0];
    let source = document(ColorMode::Rgb, SampleType::F32, Builtin::DisplayP3.profile(), &pixels);
    let bytes = encode_source(&source).unwrap();
    let source_profile = crate::color_cmds::document_profile(&source);
    let print = custom_cmyk();
    for (mode, target) in [(ColorMode::Cmyk, &print), (ColorMode::Lab, Builtin::LabD50.profile()), (ColorMode::Grayscale, Builtin::GrayGamma22.profile())] {
        let fmt = PixelFormat::new(mode, SampleType::U16, true);
        let actual = source_image_in_profile("source.pcraft", &bytes, fmt, target).unwrap();
        let reference = convert(&pixels, &source_profile, target, fmt.channels());
        close(&actual.surface.read_region(source.bounds()), &reference, 2.0 / 65535.0);
    }
}

#[test]
fn smart_colour_native_warm_and_cold_sources_preserve_device_and_hdr_numbers() {
    let print = custom_cmyk();
    for (mode, depth, profile, pixels) in [
        (ColorMode::Cmyk, SampleType::U16, &print, vec![0.1, 0.8, 0.2, 0.05, 0.6, 0.65, 0.2, 0.4, 0.1, 1.0]),
        (ColorMode::Rgb, SampleType::F32, Builtin::LinearSrgb.profile(), vec![-0.05, 1.3, 0.5, 0.6, 1.5, 0.4, -0.1, 1.0]),
    ] {
        let doc = document(mode, depth, profile, &pixels);
        let expected = doc.layers[0].surface().unwrap().read_region(doc.bounds());
        let smart = layer_to_smart(&doc, &doc.layers[0]).unwrap();
        let LayerContent::Smart(sm) = &smart.content else { panic!("smart layer") };
        assert_eq!(sm.cache.as_ref().unwrap().read_region(doc.bounds()), expected);
        assert_eq!(render(&doc, sm).unwrap().unwrap().read_region(doc.bounds()), expected);
        let SmartSource::Embedded { bytes, .. } = &sm.source else { panic!("embedded") };
        let key = cache_key(bytes, doc.pixel_format(), &crate::color_cmds::document_profile(&doc));
        SOURCE_CACHE.lock().unwrap().retain(|entry| entry.key != key);
        let mut saved = doc.clone();
        saved.layers = vec![smart];
        let reopened = photocraft_format::load_from_bytes(&encode_source(&saved).unwrap()).unwrap();
        let LayerContent::Smart(sm) = &reopened.layers[0].content else { panic!("reopened smart") };
        assert_eq!(render(&reopened, sm).unwrap().unwrap().read_region(reopened.bounds()), expected);
    }
}

#[test]
fn smart_colour_develop_refresh_uses_the_current_parent_profile() {
    let image = image::RgbaImage::from_pixel(3, 2, image::Rgba([75u8, 125, 185, 160]));
    let mut encoded = std::io::Cursor::new(Vec::new());
    image.write_to(&mut encoded, image::ImageFormat::Png).unwrap();
    let bytes = Arc::new(encoded.into_inner());
    let settings = json!({"light":{"exposure":0.25}});
    let developed = crate::develop_layer_cmds::developed_doc("source.png", &bytes, &settings).unwrap();
    let input = photocraft_compose::flatten(&developed).px;
    let src = crate::color_cmds::composite_profile(&developed);
    let mut sm = SmartObject::new(SmartSource::Embedded { file_name: "source.png".into(), bytes }, Affine::IDENTITY, None);
    sm.develop = Some(DevelopLink { library_baseline: None, source: None, library: None, photo: None, settings });
    for profile in [Builtin::Srgb.profile(), Builtin::DisplayP3.profile()] {
        let mut parent = Document::new("Changed document profile", developed.size, ColorMode::Rgb, SampleType::F32);
        parent.icc_profile = Some(profile.to_bytes());
        let target = crate::color_cmds::document_profile(&parent);
        let actual = render(&parent, &sm).unwrap().unwrap();
        close(&actual.read_region(parent.bounds()), &convert(input.as_flattened(), &src, &target, 4), 3e-6);
    }
}

#[test]
fn smart_colour_stack_retains_custom_cmyk_document_context() {
    let profile = custom_cmyk();
    let mut doc = document(ColorMode::Cmyk, SampleType::F32, &profile, &[0.0, 0.5, 0.5, 0.0, 1.0]);
    let mut second = Layer::raster("Second", doc.pixel_format());
    second.surface_mut().unwrap().write_region(doc.bounds(), &[0.2, 0.1, 0.3, 0.2, 1.0]);
    doc.layers.push(second);
    let actual_profile = crate::color_cmds::document_profile(&doc);
    let to_rgb = Transform::new(&actual_profile, Builtin::Srgb.profile(), Intent::RelativeColorimetric, true).unwrap();
    let mut mean = vec![0.0f32; 4];
    mean[3] = 1.0;
    for layer in &doc.layers {
        let input = layer.surface().unwrap().read_region(doc.bounds());
        let mut rgb = [0.0f32; 3];
        // The compositor deliberately uses the same cached accelerated device transform.
        to_rgb.eval_fast(&input[..4], &mut rgb);
        for i in 0..3 {
            mean[i] += rgb[i] * 0.5;
        }
    }
    let target = Builtin::DisplayP3.profile();
    let expected = convert(&mean, Builtin::Srgb.profile(), target, 4);
    let image = stack_image_in_profile("stack.pcraft", &encode_source(&doc).unwrap(), PixelFormat::RGBA32F, photocraft_doc::StackMode::Mean, target).unwrap();
    close(&image.surface.read_region(doc.bounds()), &expected, 3e-5);
}

#[test]
fn smart_colour_plain_default_coverage_is_bounded_by_source_canvas() {
    let mut doc = document(ColorMode::Rgb, SampleType::F32, Builtin::Srgb.profile(), &[0.3, 0.4, 0.5, 1.0]);
    doc.layers[0].content = LayerContent::Raster(Surface::with_default(doc.pixel_format(), &[0.3, 0.4, 0.5, 1.0]));
    let image = source_image_in_profile("default.pcraft", &encode_source(&doc).unwrap(), doc.pixel_format(), Builtin::Srgb.profile()).unwrap();
    assert_eq!(image.surface.rgba(0, 0), [0.3, 0.4, 0.5, 1.0]);
    assert_eq!(image.surface.rgba(2, 0)[3], 0.0, "source default must not fill outside its canvas");
}

#[test]
fn smart_colour_fast_path_preserves_projected_raster_clip_warm_and_cold() {
    let mut doc = Document::new("clipped raster", Size::new(40, 20), ColorMode::Rgb, SampleType::F32);
    doc.icc_profile = Some(Builtin::DisplayP3.profile().to_bytes());
    let mut layer = Layer::raster("Pixels", doc.pixel_format());
    layer.surface_mut().unwrap().fill_rect(doc.bounds(), &[0.3, 0.5, 0.7, 1.0]);
    layer.vector.clip = Some(photocraft_vector::shapes::rect(0.0, 0.0, 20.0, 20.0));
    doc.layers.push(layer);
    let expected = photocraft_compose::flatten(&crate::vector_integration::project_document(&doc).unwrap()).px;
    let smart = layer_to_smart(&doc, &doc.layers[0]).unwrap();
    let LayerContent::Smart(sm) = &smart.content else { panic!("smart layer") };
    close(&sm.cache.as_ref().unwrap().read_region(doc.bounds()), expected.as_flattened(), 1e-6);
    assert_eq!(sm.cache.as_ref().unwrap().rgba(30, 10)[3], 0.0);
    let SmartSource::Embedded { bytes, .. } = &sm.source else { panic!("embedded") };
    let key = cache_key(bytes, doc.pixel_format(), &crate::color_cmds::document_profile(&doc));
    SOURCE_CACHE.lock().unwrap().retain(|entry| entry.key != key);
    let cold = render(&doc, sm).unwrap().unwrap();
    close(&cold.read_region(doc.bounds()), expected.as_flattened(), 1e-6);
    assert_eq!(cold.rgba(30, 10)[3], 0.0);
}

#[test]
fn smart_colour_plain_lab_keeps_colours_outside_srgb_when_placing_in_p3() {
    let p3 = Builtin::DisplayP3.profile();
    let lab = Builtin::LabD50.profile();
    let mut lab_pixel = [0.0f32; 4];
    Transform::new(p3, lab, Intent::RelativeColorimetric, true).unwrap().convert_f32(&[1.0, 0.0, 0.0, 0.55], 4, &mut lab_pixel, 4, true);
    let doc = document(ColorMode::Lab, SampleType::F32, lab, &lab_pixel);
    let image = source_image_in_profile("wide-lab.pcraft", &encode_source(&doc).unwrap(), PixelFormat::RGBA32F, p3).unwrap();
    let actual = image.surface.read_region(doc.bounds());
    let encoded_profile = crate::color_cmds::document_profile(&doc);
    let direct = convert(&lab_pixel, &encoded_profile, p3, 4);
    eprintln!("Lab direct ICC reference {direct:?}; smart source {actual:?}");
    close(&actual, &direct, 1e-6);
    // Compare against the independently observable old route: Lab is clipped into
    // sRGB by compositing, then converted to P3. The direct route must retain the
    // wider colour instead; ICC encoding/BPC need not be an exact inverse round trip.
    let clipped = photocraft_compose::flatten(&doc);
    let clipped_p3 = convert(clipped.px.as_flattened(), Builtin::Srgb.profile(), p3, 4);
    assert!(actual.iter().take(3).zip(&clipped_p3).any(|(a, b)| (a - b).abs() > 0.05), "wide colour was clipped through sRGB: {actual:?} vs {clipped_p3:?}");
    assert!(actual[0] > 0.97 && actual[1].abs() < 0.04 && actual[2].abs() < 0.04, "wide red changed: {actual:?}");
    assert_eq!(image.surface.rgba(0, 0)[3], 0.55);
}

#[test]
fn smart_colour_plain_raster_does_not_bypass_multichannel_ink_backdrop() {
    let mut doc = Document::new("spot inks", Size::new(2, 1), ColorMode::Multichannel, SampleType::F32);
    doc.layers.push(Layer::raster("Empty overlay", doc.pixel_format()));
    let mut ink = Surface::new(PixelFormat::new(ColorMode::Grayscale, SampleType::U8, false));
    ink.write_region(doc.bounds(), &[1.0, 1.0]);
    doc.channels
        .push(photocraft_doc::AlphaChannel { spot: Some((photocraft_doc::Color::rgb(0.0, 1.0, 1.0), 0.0)), ..photocraft_doc::AlphaChannel::new("Cyan", ink) });
    let image = source_image_in_profile("inks.pcraft", &encode_source(&doc).unwrap(), PixelFormat::RGBA32F, Builtin::Srgb.profile()).unwrap();
    assert_eq!(image.surface.rgba(0, 0), [0.0, 1.0, 1.0, 1.0]);
}

#[test]
fn smart_colour_develop_filter_retains_parent_cmyk_interpretation() {
    let profile = custom_cmyk();
    let doc = document(ColorMode::Cmyk, SampleType::F32, &profile, &[0.0, 0.5, 0.5, 0.0, 1.0]);
    let mut layer = layer_to_smart(&doc, &doc.layers[0]).unwrap();
    let LayerContent::Smart(smart) = &mut layer.content else { panic!("smart") };
    let params = json!({"light":{"exposure":0.4}});
    smart.smart_filters.push(photocraft_doc::SmartFilter {
        command: crate::develop_filter_cmds::DEVELOP.into(),
        params: params.clone(),
        blend: BlendMode::Normal,
        opacity: 1.0,
        visible: true,
    });
    let settings = photocraft_io::develop_filter::filter_settings(&params).unwrap();
    let cmyk = photocraft_compose::cmyk_space(&doc);
    assert!(cmyk.is_some());
    let source = doc.layers[0].surface().unwrap();
    let expected = photocraft_color::convert::with_cmyk_space(cmyk.as_ref(), || {
        photocraft_io::develop_filter::develop_surface(source, doc.bounds(), &settings, Builtin::Srgb.profile()).unwrap()
    });
    let incorrect_default = photocraft_io::develop_filter::develop_surface(source, doc.bounds(), &settings, Builtin::Srgb.profile()).unwrap();
    assert!(expected.read_region(doc.bounds()).iter().zip(incorrect_default.read_region(doc.bounds())).any(|(a, b)| (a - b).abs() > 1e-4));
    close(&render(&doc, smart).unwrap().unwrap().read_region(doc.bounds()), &expected.read_region(doc.bounds()), 1e-6);
}

#[test]
fn smart_colour_filter_mask_mix_preserves_hdr_and_negative_float_values() {
    let area = Rect::from_xywh(0, 0, 1, 1);
    let mut base = Surface::new(PixelFormat::RGBA32F);
    base.write_region(area, &[2.0, -0.4, 0.8, 1.0]);
    let mut filtered = Surface::new(PixelFormat::RGBA32F);
    filtered.write_region(area, &[4.0, 0.2, 1.4, 0.5]);
    let mut mask = LayerMask::reveal_all();
    mask.surface = Surface::new(PixelFormat::new(ColorMode::Grayscale, SampleType::F32, false));
    mask.surface.write_region(area, &[0.5]);
    close(&photocraft_compose::filter_mask::mix(&base, &filtered, &mask).read_region(area), &[8.0 / 3.0, -0.2, 1.0, 0.75], 1e-6);
}

#[test]
fn stack_float_statistics_preserve_signed_hdr_through_smart_source() {
    use photocraft_doc::StackMode;
    let mut doc = document(ColorMode::Rgb, SampleType::F32, Builtin::Srgb.profile(), &[2., -0.25, 0.25, 1.]);
    let mut second = Layer::raster("Second", doc.pixel_format());
    second.surface_mut().unwrap().write_region(doc.bounds(), &[4., -0.5, 0.75, 1.]);
    doc.layers.push(second);
    let bytes = encode_source(&doc).unwrap();
    for (mode, expected) in [
        (StackMode::Mean, [3., -0.375, 0.5, 1.]),
        (StackMode::Median, [3., -0.375, 0.5, 1.]),
        (StackMode::Maximum, [4., -0.25, 0.75, 1.]),
        (StackMode::Minimum, [2., -0.5, 0.25, 1.]),
        (StackMode::Summation, [6., -0.75, 1., 1.]),
        (StackMode::Range, [2., 0.25, 0.5, 1.]),
        (StackMode::Variance, [1., 0.015625, 0.0625, 1.]),
        (StackMode::StandardDeviation, [1., 0.125, 0.25, 1.]),
    ] {
        let image = stack_image_in_profile("hdr.pcraft", &bytes, PixelFormat::RGBA32F, mode, Builtin::Srgb.profile()).unwrap();
        close(&image.surface.read_region(doc.bounds()), &expected, 2e-6);
        // The same values still quantize normally in integer destinations.
        let integer = stack_image_in_profile("hdr.pcraft", &bytes, PixelFormat::RGBA8, mode, Builtin::Srgb.profile()).unwrap();
        close(&integer.surface.read_region(doc.bounds()), &expected.map(|v| v.clamp(0., 1.)), 1. / 255.);
    }
}
