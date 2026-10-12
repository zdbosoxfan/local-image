//! Page boxes/rotation mapped as in VectorCraft pdf/pages.rs at
//! d522c1d7be4035bd4f4a84cd6ebfca44f5155092. Copyright (c) 2026 ArtCraft
//! Team and the VectorCraft contributors. MIT OR Apache-2.0. PDF 1.7 §§3.6.2, 10.10.1.
use super::{ImportError, PageBox, PdfOptions, Result, cancelled, device::Builder, geometry, preflight, warn};
use hayro_interpret::{Context, InterpreterCache, InterpreterSettings, InterpreterWarning, interpret_page};
use hayro_syntax::page::{Page, Rotation};
use kurbo::{Affine, Rect};
use photocraft_doc::{
    DesignUnits, Layer, Size, VectorMask,
    vector_design::exchange::{ImportOptions, ImportedScene, VectorWarning},
};
use std::sync::{Arc, Mutex, atomic::AtomicBool};

#[derive(Clone, Debug, PartialEq)]
pub struct PageInfo {
    pub crop: [f64; 4],
    pub media: [f64; 4],
    pub rotation: u16,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PdfInfo {
    pub pages: Vec<PageInfo>,
}
pub(super) fn open(bytes: &[u8], options: &ImportOptions, cancel: &AtomicBool) -> Result<preflight::Admitted> {
    let pdf = preflight::check(bytes, options, cancel)?;
    cancelled(cancel)?;
    if pdf.len() > options.limits.objects || pdf.pages().len() > options.limits.objects {
        return Err(ImportError::Limit("parsed object/page"));
    }
    if pdf.pages().is_empty() {
        return Err(ImportError::Invalid("no pages".into()));
    }
    Ok(pdf)
}
fn box_rect(page: &Page<'_>, which: PageBox) -> Result<Rect> {
    let r = match which {
        PageBox::Crop => page.intersected_crop_box(),
        PageBox::Media => page.media_box(),
    };
    if ![r.x0, r.y0, r.x1, r.y1].iter().all(|n| n.is_finite() && n.abs() <= geometry::MAX_COORD) || r.width() <= 0.0 || r.height() <= 0.0 {
        return Err(ImportError::Invalid("empty or invalid page box".into()));
    }
    Ok(Rect::new(r.x0, r.y0, r.x1, r.y1))
}
pub(super) fn info(pdf: &preflight::Admitted, options: &ImportOptions, cancel: &AtomicBool) -> Result<PdfInfo> {
    let mut pages = Vec::new();
    for p in pdf.pages().iter() {
        cancelled(cancel)?;
        let c = box_rect(p, PageBox::Crop)?;
        let m = box_rect(p, PageBox::Media)?;
        pages.push(PageInfo {
            crop: [c.x0, c.y0, c.x1, c.y1],
            media: [m.x0, m.y0, m.x1, m.y1],
            rotation: match p.rotation() {
                Rotation::None => 0,
                Rotation::Horizontal => 90,
                Rotation::Flipped => 180,
                Rotation::FlippedHorizontal => 270,
            },
        });
    }
    if pages.len() > options.limits.objects {
        return Err(ImportError::Limit("page"));
    }
    Ok(PdfInfo { pages })
}
pub(super) fn convert(pdf: &preflight::Admitted, options: &ImportOptions, extra: &PdfOptions, cancel: &AtomicBool) -> Result<ImportedScene> {
    let selected = if options.pages.is_empty() { vec![0] } else { options.pages.clone() };
    if selected.len() > options.limits.objects {
        return Err(ImportError::Limit("selected page"));
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut frames = Vec::new();
    let (mut x, mut h) = (0.0_f64, 0.0_f64);
    for &number in &selected {
        cancelled(cancel)?;
        if !seen.insert(number) {
            return Err(ImportError::Invalid("duplicate selected page".into()));
        }
        let page = pdf.pages().get(number).ok_or_else(|| ImportError::Invalid(format!("page {} does not exist", number.saturating_add(1))))?;
        let b = box_rect(page, extra.page_box)?;
        let unit = page.raw().get::<f64>(b"UserUnit").unwrap_or(1.0);
        if !unit.is_finite() || !(0.000_001..=75_000.0).contains(&unit) {
            return Err(ImportError::Invalid("invalid UserUnit".into()));
        }
        let scale = f64::from(options.dpi) / 72.0 * unit;
        let base = if extra.ignore_rotation { Affine::new([1.0, 0.0, 0.0, -1.0, 0.0, 0.0]) } else { Affine::new(page.initial_transform(true).as_coeffs()) };
        let projected = base.transform_rect_bbox(b);
        let (w, height) = (projected.width() * scale, projected.height() * scale);
        x += w.ceil();
        h = h.max(height.ceil());
        if !x.is_finite() || !h.is_finite() || x > geometry::MAX_COORD || h > geometry::MAX_COORD || x * h > options.limits.pixels as f64 {
            return Err(ImportError::Limit("page pixel"));
        }
        let frame = Rect::new(x - w.ceil(), 0.0, x, height.ceil());
        let transform = Affine::translate((frame.x0, 0.0)) * Affine::scale(scale) * Affine::translate((-projected.x0, -projected.y0)) * base;
        geometry::transform(transform)?;
        frames.push((number, frame, transform));
    }
    let size = Size::new(x as u32, h as u32);
    let mut b = Builder::new(options, cancel, photocraft_doc::Rect::from_size(size), pdf.groups.clone());
    if pdf.unverified_group_space {
        warn(
            &mut b.warnings,
            "group-colour-space-unverified",
            None,
            "An existing isolated/page group uses a blending colour space whose conversion is not verified; legacy RGB compositing is retained and may differ from the PDF.",
        );
    }
    let cache = InterpreterCache::new();
    let sink: Arc<Mutex<Vec<VectorWarning>>> = Arc::default();
    let warnings = sink.clone();
    let fallback = sink.clone();
    let settings = InterpreterSettings {
        render_annotations: false,
        font_resolver: Arc::new(move |q| match q {
            hayro_interpret::font::FontQuery::Standard(s) => Some(s.get_font_data()),
            hayro_interpret::font::FontQuery::Fallback(f) => {
                if let Ok(mut ws) = fallback.lock() {
                    warn(&mut ws, "font-substitution", None, "An unembedded font was substituted with a bundled standard font; its glyph outlines may differ.");
                }
                Some(f.pick_standard_font().get_font_data())
            }
        }),
        warning_sink: Arc::new(move |w| {
            if let Ok(mut ws) = warnings.lock() {
                let (feature, message) = match w {
                    InterpreterWarning::UnsupportedFont => ("unsupported-font", "Hayro could not decode a font; affected glyphs were omitted."),
                    InterpreterWarning::ImageDecodeFailure => ("image-decode", "Hayro could not decode an image; it was omitted."),
                };
                warn(&mut ws, feature, None, message);
            }
        }),
        ..Default::default()
    };
    let mut layers = Vec::new();
    for (number, frame, transform) in frames {
        let page = &pdf.pages()[number];
        b.begin_page();
        let mut context = Context::new(transform, frame, &cache, pdf.xref(), settings.clone());
        interpret_page(page, &mut context, &mut b);
        cancelled(cancel)?;
        let children = b.end_page()?;
        let mut page_layer = Layer::group(format!("PDF page {}", number + 1), children);
        let path = geometry::native(&kurbo::Shape::to_path(&frame, 0.01), Affine::IDENTITY, hayro_interpret::FillRule::NonZero)?;
        page_layer.vector.clip = Some(path.clone());
        page_layer.vector_mask = Some(VectorMask::new(path));
        layers.push(page_layer);
        if page.raw().contains_key(b"Annots") {
            warn(&mut b.warnings, "annotations", None, "Interactive annotations were omitted; the page artwork was imported.");
        }
        if page.raw().contains_key(b"PieceInfo") {
            warn(&mut b.warnings, "private-data", None, "Only the PDF page artwork was imported; editor-specific private data was omitted.");
        }
    }
    let mut features = std::collections::BTreeSet::new();
    b.warnings.retain(|w| features.insert(w.feature.clone()));
    if let Ok(ws) = sink.lock() {
        for w in ws.iter() {
            if !b.warnings.iter().any(|n| n.feature == w.feature) {
                b.warnings.push(w.clone());
            }
        }
    }
    Ok(ImportedScene { size, resolution_dpi: options.dpi, units: DesignUnits::Points, layers, symbols: vec![], warnings: b.warnings })
}
