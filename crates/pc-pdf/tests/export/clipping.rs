use super::*;
#[path = "../../../pc-export-common/cases/clipping.rs"]
mod cases;
use cases::reference_mask_bytes;
use photocraft_doc::{BlendMode, Rect};

fn check_native(doc: &Document, label: &str, mask_images: bool) {
    let out = export::export(doc, &ExportScope::Document, &Default::default()).unwrap();
    assert!(!out.warnings.iter().any(|w| w.feature == "compositing" || w.feature == "clipping chain"), "{label}: {:?}", out.warnings);
    let pdf = Pdf::new(out.bytes).unwrap();
    assert!(has_paths(&pdf));
    assert_eq!(images(&pdf) > 0, mask_images, "{label}");
    let objects = format!("{:?}", pdf.objects().into_iter().collect::<Vec<_>>());
    assert!(objects.contains("FontFile2"), "{label}: native type retained");
    let paths = streams(&pdf)
        .iter()
        .map(|(bytes, _)| {
            use hayro_syntax::content::{TypedIter, ops::TypedInstruction};
            let mut iter = TypedIter::new(bytes);
            let mut n = 0;
            while let Some(op) = iter.next() {
                if matches!(op, TypedInstruction::MoveTo(_) | TypedInstruction::RectPath(_)) {
                    n += 1;
                }
            }
            n
        })
        .sum::<usize>();
    assert!(paths >= 50, "{label}: paths retained");
    for object in pdf.objects() {
        if let Object::Stream(s) = object
            && s.dict().get::<Name<'_>>(b"Subtype").is_some_and(|n| n.as_ref() == b"Image")
        {
            assert!(s.dict().get::<i32>(b"Width").unwrap() < 128, "{label}: mask/base image must be cropped");
        }
    }
    oracle(doc, label);
}

fn check_local_fallback(doc: &Document, label: &str) {
    let out = export::export(doc, &ExportScope::Document, &Default::default()).unwrap();
    assert!(!out.warnings.iter().any(|w| w.feature == "compositing"));
    assert!(out.warnings.iter().any(|w| w.feature == "clipping chain"));
    let pdf = Pdf::new(out.bytes).unwrap();
    assert!(has_paths(&pdf));
    let path_count = streams(&pdf)
        .iter()
        .map(|(bytes, _)| {
            use hayro_syntax::content::{TypedIter, ops::TypedInstruction};
            let mut iter = TypedIter::new(bytes);
            let mut count = 0;
            while let Some(op) = iter.next() {
                if matches!(op, TypedInstruction::MoveTo(_) | TypedInstruction::RectPath(_)) {
                    count += 1;
                }
            }
            count
        })
        .sum::<usize>();
    assert!(path_count >= 50, "unrelated vector artwork must survive local fallback");
    assert!(images(&pdf) > 0);
    assert!(format!("{:?}", pdf.objects().into_iter().collect::<Vec<_>>()).contains("FontFile2"));
    for object in pdf.objects() {
        if let Object::Stream(s) = object
            && s.dict().get::<Name<'_>>(b"Subtype").is_some_and(|n| n.as_ref() == b"Image")
        {
            assert!(s.dict().get::<i32>(b"Width").unwrap() < 128);
        }
    }
    oracle(doc, label);
}

fn profile_export_oracle(input: &Document, reference: &Document, label: &str, glyph: Option<(i32, i32)>) {
    let out = export::export(input, &ExportScope::Document, &Default::default()).unwrap();
    assert!(out.warnings.iter().any(|w| w.feature == "clipping chain"));
    let rebuilt = export::export(reference, &ExportScope::Document, &Default::default()).unwrap();
    assert_eq!(out.bytes, rebuilt.bytes, "reference cache must not replace actual cache regeneration");
    let pdf = Pdf::new(out.bytes).unwrap();
    assert!(images(&pdf) > 0);
    if let Some((x, y)) = glyph {
        let rgb = pdf
            .objects()
            .into_iter()
            .find_map(|object| {
                let Object::Stream(stream) = object else { return None };
                if !stream.dict().get::<Name<'_>>(b"Subtype").is_some_and(|n| n.as_ref() == b"Image") {
                    return None;
                }
                let data = stream.decoded().ok()?;
                (data.len() == 128 * 768 * 3).then(|| data.to_vec())
            })
            .expect("clipping fallback must contain its RGB image payload");
        let expected = photocraft_compose::flatten(reference).to_rgba8();
        let pixel = y as usize * 128 + x as usize;
        for c in 0..3 {
            assert!(rgb[pixel * 3 + c].abs_diff(expected.pixels[pixel * 4 + c]) <= 2, "{label}: interior glyph RGB must match independent CMS inks");
        }
    }
    oracle(reference, label);
}

fn check_closed_fallback(doc: &Document, label: &str) {
    let out = export::export(doc, &ExportScope::Document, &Default::default()).unwrap();
    assert!(!out.warnings.iter().any(|w| w.feature == "compositing"), "{label}: {:?}", out.warnings);
    assert!(out.warnings.iter().any(|w| w.feature == "isolated composition"));
    let pdf = Pdf::new(out.bytes).unwrap();
    assert!(has_paths(&pdf));
    let objects = format!("{:?}", pdf.objects().into_iter().collect::<Vec<_>>());
    assert!(objects.contains("FontFile2"), "outside type stays native");
    let path_count = streams(&pdf)
        .iter()
        .map(|(bytes, _)| {
            use hayro_syntax::content::{TypedIter, ops::TypedInstruction};
            let mut iter = TypedIter::new(bytes);
            let mut count = 0;
            while let Some(op) = iter.next() {
                if matches!(op, TypedInstruction::MoveTo(_) | TypedInstruction::RectPath(_)) {
                    count += 1;
                }
            }
            count
        })
        .sum::<usize>();
    assert!(path_count >= 50);
    let group = doc.layers.last().unwrap();
    let mut isolated = doc.clone();
    let mut plain = group.clone();
    plain.blend = BlendMode::Normal;
    isolated.layers = vec![plain];
    // The authored union is [8,12,108,90]: the clipped rectangles begin at
    // y12, and the internal backdrop bounds all glyph ink. Compare every RGB
    // and alpha image sample to the same independently composed source.
    let mut checked = false;
    let mut checked_alpha = false;
    for object in pdf.objects() {
        let Object::Stream(stream) = object else { continue };
        if !stream.dict().get::<Name<'_>>(b"Subtype").is_some_and(|n| n.as_ref() == b"Image") {
            continue;
        }
        let width = stream.dict().get::<i32>(b"Width").unwrap();
        let height = stream.dict().get::<i32>(b"Height").unwrap();
        assert!(width < 128 && height < 128);
        let data = stream.decoded().unwrap();
        if data.len() == width as usize * height as usize * 3 {
            let expected = photocraft_compose::render(&isolated, Rect::new(8, 12, 8 + width, 12 + height)).to_rgba8();
            let rgb = expected.pixels.as_chunks::<4>().0.iter().flat_map(|p| p[..3].iter().copied()).collect::<Vec<_>>();
            assert_eq!(data.as_ref(), rgb.as_slice(), "internal gamma-edge color and parent opacity are baked once");
            checked = true;
        } else if data.len() == width as usize * height as usize {
            let expected = photocraft_compose::render(&isolated, Rect::new(8, 12, 8 + width, 12 + height)).to_rgba8();
            let alpha = expected.pixels.as_chunks::<4>().0.iter().map(|p| p[3]).collect::<Vec<_>>();
            assert_eq!(data.as_ref(), alpha.as_slice(), "parent masks and opacity apply once to source alpha");
            checked_alpha = true;
        }
    }
    assert!(checked && checked_alpha);
    oracle(doc, label);
}

fn check_mask_field(doc: &Document, bytes: &[u8], rect: Rect, label: &str) {
    let pdf = Pdf::new(bytes.to_vec()).unwrap();
    let expected = reference_mask_bytes(doc, rect);
    let mut checked = false;
    for object in pdf.objects() {
        let Object::Stream(stream) = object else { continue };
        if !stream.dict().get::<Name<'_>>(b"Subtype").is_some_and(|name| name.as_ref() == b"Image") {
            continue;
        }
        let data = stream.decoded().unwrap();
        if data.len() == rect.size().area() as usize {
            assert_eq!(stream.dict().get::<i32>(b"Width").unwrap(), rect.width() as i32);
            assert_eq!(stream.dict().get::<i32>(b"Height").unwrap(), rect.height() as i32);
            assert_eq!(data.as_ref(), expected.as_slice(), "{label}: cropped field must equal canonical full-canvas mask");
            checked = true;
        }
    }
    assert!(checked, "{label}: expected the encoded scalar alpha image");
}
