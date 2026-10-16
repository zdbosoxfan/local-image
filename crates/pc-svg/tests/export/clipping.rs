use super::*;
#[path = "../../../pc-export-common/cases/clipping.rs"]
mod cases;
use cases::reference_mask_bytes;
use photocraft_doc::{BlendMode, Rect};

fn check_native(doc: &Document, label: &str, mask_images: bool) {
    let out = export::export(doc, &ExportScope::Document, &ExportOptions { text_as_curves: true, ..Default::default() }).unwrap();
    assert!(!out.warnings.iter().any(|w| w.feature == "compositing" || w.feature == "clipping chain"), "{label}: {:?}", out.warnings);
    let source = String::from_utf8(out.bytes).unwrap();
    let parsed = xml(&source);
    assert!(parsed.descendants().filter(|n| n.has_tag_name("path")).count() >= 50, "{label}: unrelated vector paths/type retained");
    let images = parsed.descendants().filter(|n| n.has_tag_name("image")).collect::<Vec<_>>();
    assert_eq!(!images.is_empty(), mask_images, "{label}");
    for image in images {
        assert!(image.attribute("width").unwrap().parse::<f64>().unwrap() < 128., "{label}: mask/base image must be cropped");
    }
    let rgba = oracle::native_raster_render(&source);
    let text_base = doc.layers.windows(2).position(|pair| !pair[0].clipped && pair[1].clipped && matches!(pair[0].content, LayerContent::Text(_)));
    let mask_only_images =
        parsed.descendants().filter(|node| node.has_tag_name("image")).all(|node| node.ancestors().any(|parent| parent.has_tag_name("mask")));
    if mask_images
        && mask_only_images
        && let Some(base_index) = text_base
    {
        assert_hybrid_text_mask(doc, &source, &rgba, base_index, label);
    } else if mask_images {
        assert_raster_oracle_svg(doc, &source, label);
    } else {
        // TEST-CHANGED: image-free vector geometry uses the established 8x
        // geometry oracle, not 1x cross-renderer glyph/AA alpha agreement.
        // Color and interior alpha still come from independent Inkscape output.
        let iou = oracle::coverage_iou(doc, &source);
        assert!(iou > 0.995, "{label}: vector geometry IoU {iou}");
        assert_inkscape_color(doc, &rgba, label);
    }
    let expected = photocraft_compose::flatten(doc).get(32, 32);
    assert!((f32::from(rgba[32 * 128 + 32][3]) / 255. - expected[3]).abs() <= 1.0 / 255. + 0.00001, "{label}: retained base alpha");
}

fn assert_inkscape_color(doc: &Document, rgba: &[[u8; 4]], label: &str) {
    let expected = photocraft_compose::flatten(doc).to_rgba8();
    assert_eq!(rgba.len() * 4, expected.pixels.len());
    let mut delta = 0.0;
    for (a, b) in rgba.iter().zip(expected.pixels.as_chunks::<4>().0) {
        let white = |p: &[u8]| std::array::from_fn::<_, 3, _>(|c| ((u32::from(p[c]) * u32::from(p[3]) + 127) / 255 + 255 - u32::from(p[3])).min(255) as u8);
        let a = photocraft_testkit::vector::oklab(white(a));
        let b = photocraft_testkit::vector::oklab(white(b));
        delta += a.iter().zip(b).map(|(a, b)| (a - b).powi(2)).sum::<f64>().sqrt() * 100.;
    }
    let mean = delta / rgba.len() as f64;
    assert!(mean < 0.5, "{label}: Inkscape DeltaE {mean}");
}

// The actual native glyph coverage is calibrated independently at 8x. The
// raster mask field and its application then have their own exact/pointwise
// controls; 1x native-font AA is not treated as raster-artwork alpha fidelity.
fn assert_hybrid_text_mask(doc: &Document, source: &str, masked: &[[u8; 4]], base_index: usize, label: &str) {
    use image::ImageEncoder as _;
    let layer = &doc.layers[base_index];
    assert_eq!(photocraft_compose::text_gamma(layer), 1.0);
    let tree = xml(source);
    let masks = tree.descendants().filter(|node| node.has_tag_name("mask")).collect::<Vec<_>>();
    assert_eq!(masks.len(), 1);
    let mask = masks[0];
    let id = mask.attribute("id").unwrap();
    let target = format!("url(#{id})");
    assert_eq!(mask.attribute("maskUnits"), Some("userSpaceOnUse"));
    assert_eq!(mask.attribute("maskContentUnits"), Some("userSpaceOnUse"));
    let owners = tree.descendants().filter(|node| node.attribute("mask") == Some(target.as_str())).collect::<Vec<_>>();
    assert_eq!(owners.len(), 1);
    let owner = owners[0];
    assert_eq!(owner.attribute("data-name"), Some(layer.name.as_str()));
    let attribute = owner.attributes().find(|attribute| attribute.name() == "mask").unwrap();
    let image = mask.descendants().find(|node| node.has_tag_name("image")).unwrap();
    let x = image.attribute("x").unwrap().parse::<i32>().unwrap();
    let y = image.attribute("y").unwrap().parse::<i32>().unwrap();
    let width = image.attribute("width").unwrap().parse::<u32>().unwrap();
    let height = image.attribute("height").unwrap().parse::<u32>().unwrap();
    let href = image.attribute(("http://www.w3.org/1999/xlink", "href")).unwrap();
    let png = base64::engine::general_purpose::STANDARD.decode(href.strip_prefix("data:image/png;base64,").unwrap()).unwrap();
    let decoded = image::load_from_memory(&png).unwrap().into_rgba8();
    assert_eq!(decoded.dimensions(), (width, height));
    let full = photocraft_compose::masks::combined_mask(layer, doc.bounds());
    let mut field = vec![0.0f64; doc.size.area() as usize];
    let mut intersection = 0u64;
    let mut union = 0u64;
    for yy in 0..height {
        for xx in 0..width {
            let gx = x + xx as i32;
            let gy = y + yy as i32;
            assert!(gx >= 0 && gy >= 0 && gx < doc.size.width as i32 && gy < doc.size.height as i32);
            let expected =
                full.as_ref().map_or_else(|| layer.mask.as_ref().filter(|m| m.enabled).map_or(1.0, |m| m.value(gx, gy)), |m| m.sample_channel(gx, gy, 0));
            let expected = (expected.clamp(0.0, 1.0) * 255. + 0.5) as u8;
            let actual = decoded.get_pixel(xx, yy)[3];
            assert_eq!(actual, expected, "{label}: full-canvas mask sample at {gx},{gy}");
            intersection += u64::from(actual.min(expected));
            union += u64::from(actual.max(expected));
            field[gy as usize * doc.size.width as usize + gx as usize] = f64::from(actual) / 255.;
        }
    }
    assert!(union > 0);
    assert!(intersection as f64 / union as f64 > 0.995, "{label}: raster-mask field IoU");
    let mut unmasked_source = source.to_owned();
    unmasked_source.replace_range(attribute.range(), "");
    let mut geometry_doc = doc.clone();
    geometry_doc.layers[base_index].mask = None;
    geometry_doc.layers[base_index].vector_mask = None;
    assert!(oracle::coverage_iou(&geometry_doc, &unmasked_source) > 0.995, "{label}: native glyph geometry IoU");
    // Remove the entire generated base+clipped chain, preserving the backdrop
    // and all unrelated emitted vectors/type. No orphan siblings are created.
    let mut backdrop_source = source.to_owned();
    backdrop_source.replace_range(owner.range(), "");
    let unmasked = oracle::native_raster_render(&unmasked_source);
    let backdrop = oracle::native_raster_render(&backdrop_source);
    assert_eq!(masked.len(), unmasked.len());
    assert_eq!(masked.len(), backdrop.len());
    assert_eq!(masked.len(), field.len());
    for gy in 0..doc.size.height as i32 {
        for gx in 0..doc.size.width as i32 {
            if gx < x || gy < y || gx >= x + width as i32 || gy >= y + height as i32 {
                let i = gy as usize * doc.size.width as usize + gx as usize;
                assert_eq!(unmasked[i], backdrop[i], "{label}: cropped mask must not discard source ink outside its image");
            }
        }
    }
    let errors = paired_mask_errors(masked, &unmasked, &backdrop, &field);
    assert!(errors.0 <= 1.00001 && errors.1 <= 2.00001, "{label}: paired alpha/premultiplied errors {errors:?}");
    assert_inkscape_color(doc, masked, label); // Original, actually masked output.
    // These are actual rendered mutations, not just a PNG-content comparison.
    // Omitted, squared, shifted and extra-opacity masks must fail application.
    let reject = |pixels: &[[u8; 4]], kind: &str| {
        let errors = paired_mask_errors(pixels, &unmasked, &backdrop, &field);
        assert!(errors.0 > 1.00001 || errors.1 > 2.00001, "{label}: {kind} mutation was not detected");
    };
    reject(&unmasked, "omitted mask");
    let mut square = decoded.into_raw();
    for pixel in square.as_chunks_mut::<4>().0 {
        pixel[3] = ((u32::from(pixel[3]) * u32::from(pixel[3]) + 127) / 255) as u8;
    }
    let mut bytes = Vec::new();
    image::codecs::png::PngEncoder::new(&mut bytes).write_image(&square, width, height, image::ExtendedColorType::Rgba8).unwrap();
    let data = format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes));
    let href_range = image.attributes().find(|attribute| attribute.name() == "href").unwrap().range_value();
    let mut squared = source.to_owned();
    squared.replace_range(href_range, &data);
    reject(&oracle::native_raster_render(&squared), "squared mask");
    let mut shifted = source.to_owned();
    shifted.replace_range(image.attributes().find(|attribute| attribute.name() == "x").unwrap().range_value(), &(x + 1).to_string());
    reject(&oracle::native_raster_render(&shifted), "shifted mask");
    let opacity = owner.attributes().find(|attribute| attribute.name() == "opacity").unwrap();
    let mut faded = source.to_owned();
    faded.replace_range(opacity.range_value(), &(opacity.value().parse::<f64>().unwrap() * 0.5).to_string());
    reject(&oracle::native_raster_render(&faded), "extra opacity");
}
fn paired_mask_errors(actual: &[[u8; 4]], unmasked: &[[u8; 4]], backdrop: &[[u8; 4]], field: &[f64]) -> (f64, f64) {
    let mut alpha = 0.0f64;
    let mut premultiplied = 0.0f64;
    for (((a, u), b), m) in actual.iter().zip(unmasked).zip(backdrop).zip(field) {
        let aa = f64::from(a[3]) / 255.;
        let ua = f64::from(u[3]) / 255.;
        let ba = f64::from(b[3]) / 255.;
        alpha = alpha.max((aa - (ba + (ua - ba) * m)).abs() * 255.);
        for c in 0..3 {
            let ap = f64::from(a[c]) * aa;
            let up = f64::from(u[c]) * ua;
            let bp = f64::from(b[c]) * ba;
            premultiplied = premultiplied.max((ap - (bp + (up - bp) * m)).abs());
        }
    }
    (alpha, premultiplied)
}

fn check_local_fallback(doc: &Document, label: &str) {
    let out = export::export(doc, &ExportScope::Document, &ExportOptions { text_as_curves: true, ..Default::default() }).unwrap();
    assert!(!out.warnings.iter().any(|w| w.feature == "compositing"));
    assert!(out.warnings.iter().any(|w| w.feature == "clipping chain"));
    let source = String::from_utf8(out.bytes).unwrap();
    let parsed = xml(&source);
    assert!(parsed.descendants().filter(|n| n.has_tag_name("path")).count() >= 50);
    let images = parsed.descendants().filter(|n| n.has_tag_name("image")).collect::<Vec<_>>();
    assert_eq!(images.len(), 1);
    assert!(images[0].attribute("width").unwrap().parse::<f64>().unwrap() < 128.);
    assert_raster_oracle_svg(doc, &source, label);
}

fn profile_export_oracle(input: &Document, reference: &Document, label: &str, glyph: Option<(i32, i32)>) {
    let out = export::export(input, &ExportScope::Document, &Default::default()).unwrap();
    assert!(out.warnings.iter().any(|w| w.feature == "clipping chain"));
    let rebuilt = export::export(reference, &ExportScope::Document, &Default::default()).unwrap();
    assert_eq!(out.bytes, rebuilt.bytes, "reference cache must not replace actual cache regeneration");
    let source = String::from_utf8(out.bytes).unwrap();
    if let Some((x, y)) = glyph {
        let parsed = xml(&source);
        let image = parsed.descendants().find(|n| n.has_tag_name("image")).unwrap();
        let href = image.attribute(("http://www.w3.org/1999/xlink", "href")).unwrap();
        let bytes = base64::engine::general_purpose::STANDARD.decode(href.strip_prefix("data:image/png;base64,").unwrap()).unwrap();
        let decoded = image::load_from_memory(&bytes).unwrap().into_rgba8();
        let got = decoded.get_pixel(x as u32, y as u32).0;
        let expected = photocraft_compose::flatten(reference).to_rgba8();
        let at = (y as usize * 128 + x as usize) * 4;
        for c in 0..3 {
            assert!(got[c].abs_diff(expected.pixels[at + c]) <= 2, "{label}: interior glyph RGB must match independent CMS inks");
        }
    }
    assert_raster_oracle_svg(reference, &source, label);
    let rgba = oracle::native_raster_render(&source);
    for y in [1, 257, 513] {
        assert!((f32::from(rgba[y * 128 + 64][3]) / 255. - 0.192).abs() <= 1. / 255. + 0.00001);
    }
}

fn check_closed_fallback(doc: &Document, label: &str) {
    let out = export::export(doc, &ExportScope::Document, &ExportOptions { text_as_curves: true, ..Default::default() }).unwrap();
    assert!(!out.warnings.iter().any(|w| w.feature == "compositing"), "{label}: {:?}", out.warnings);
    assert!(out.warnings.iter().any(|w| w.feature == "isolated composition"));
    let source = String::from_utf8(out.bytes).unwrap();
    let parsed = xml(&source);
    assert!(parsed.descendants().filter(|n| n.has_tag_name("path")).count() >= 50);
    let images = parsed.descendants().filter(|n| n.has_tag_name("image")).collect::<Vec<_>>();
    assert_eq!(images.len(), 1);
    let image = images[0];
    let x = image.attribute("x").unwrap().parse::<i32>().unwrap();
    let y = image.attribute("y").unwrap().parse::<i32>().unwrap();
    let width = image.attribute("width").unwrap().parse::<u32>().unwrap();
    let height = image.attribute("height").unwrap().parse::<u32>().unwrap();
    assert!(width < 128 && height < 128, "closed fallback is cropped");
    let href = image.attribute(("http://www.w3.org/1999/xlink", "href")).unwrap();
    let bytes = base64::engine::general_purpose::STANDARD.decode(href.strip_prefix("data:image/png;base64,").unwrap()).unwrap();
    let decoded = image::load_from_memory(&bytes).unwrap().into_rgba8();
    let mut isolated = doc.clone();
    let mut group = doc.layers.last().unwrap().clone();
    group.blend = BlendMode::Normal;
    isolated.layers = vec![group];
    let expected = photocraft_compose::render(&isolated, Rect::new(x, y, x + width as i32, y + height as i32)).to_rgba8();
    assert_eq!(decoded.as_raw(), &expected.pixels, "internal backdrop, gamma edges, outer masks/opacity must be baked exactly once");
    assert_raster_oracle_svg(doc, &source, label);
}

fn check_mask_field(doc: &Document, bytes: &[u8], rect: Rect, label: &str) {
    let source = std::str::from_utf8(bytes).unwrap();
    let tree = xml(source);
    let mask = tree.descendants().find(|node| node.has_tag_name("mask")).unwrap();
    let image = mask.descendants().find(|node| node.has_tag_name("image")).unwrap();
    assert_eq!(image.attribute("x").unwrap().parse::<i32>().unwrap(), rect.x0);
    assert_eq!(image.attribute("y").unwrap().parse::<i32>().unwrap(), rect.y0);
    let href = image.attribute(("http://www.w3.org/1999/xlink", "href")).unwrap();
    let png = base64::engine::general_purpose::STANDARD.decode(href.strip_prefix("data:image/png;base64,").unwrap()).unwrap();
    let image = image::load_from_memory(&png).unwrap().into_rgba8();
    assert_eq!(image.dimensions(), (rect.width(), rect.height()));
    let actual = image.pixels().map(|p| p[3]).collect::<Vec<_>>();
    assert_eq!(actual, reference_mask_bytes(doc, rect), "{label}: cropped field must equal canonical full-canvas mask");
}
