//! Ordinary layer preparation through the established bounds-limited symbol projection
//! renderer. Advanced paint/geometry must install its shared provider in the integration lane.
use super::*;
use photocraft_doc::{Affine, Fill, vector_design::symbol_expansion};
use photocraft_vecfx::repeat::Expansion;

pub const MAX_CACHE_PIXELS: u64 = 16_000_000;

fn surface_pixels(surface: &photocraft_doc::Surface, a: Affine) -> Result<u64> {
    let r = surface.content_bounds();
    if r.is_empty() {
        return Ok(0);
    }
    let b = evaluator::Bounds::new([f64::from(r.x0), f64::from(r.y0)], [f64::from(r.x1), f64::from(r.y1)])
        .and_then(|b| b.transform(a))
        .map_err(|e| bad(e.to_string()))?;
    let source = u64::from(r.width()) * u64::from(r.height());
    let destination = ((b.width().ceil() + 2.) * (b.height().ceil() + 2.)) as u64;
    Ok(source.max(destination))
}

fn budget(layers: &[Layer], ex: &Expansion, doc: &Document, total: &mut u64) -> Result<()> {
    for l in layers {
        let a = *ex.transforms.get(&l.id.0).ok_or_else(|| bad("repeat projection transform missing"))?;
        if let LayerContent::Group(g) = &l.content
            && g.artboard.is_some()
            && a.m[1].abs().max(a.m[2].abs()) > 1e-12
        {
            return Err(bad("repeat rotated artboard clip requires the shared scaled projection provider (09)"));
        }
        let area = if let LayerContent::Shape(s) = &l.content {
            // Legacy gradient/pattern frames cannot express an arbitrary reflected/rotated
            // paint affine. Refuse pending 08 instead of returning incorrectly painted art.
            if a != Affine::IDENTITY && s.fill.iter().chain(s.stroke.iter().map(|s| &s.paint)).any(|p| !matches!(p, Fill::Solid(_))) {
                return Err(bad("repeat transformed paint requires the shared appearance provider (08)"));
            }
            let mut sh = s.clone();
            sh.path = sh.path.transform(&a);
            crate::vector_api::validate_path(&sh.path)?;
            for p in sh.path.subpaths.iter().flat_map(|p| &p.knots).flat_map(|k| [k.anchor, k.in_ctrl, k.out_ctrl]) {
                evaluator::number(p.x).and_then(|()| evaluator::number(p.y)).map_err(|e| bad(e.to_string()))?;
            }
            let compiled = photocraft_vector::CompiledShape::new(&sh, photocraft_vector::DEFAULT_TOLERANCE, doc.bounds());
            let bounds = compiled.bounds().unwrap_or(photocraft_doc::Rect::EMPTY).intersect(&doc.bounds());
            u64::from(bounds.width()) * u64::from(bounds.height())
        } else if let Some(surface) = l.surface() {
            surface_pixels(surface, a)?
        } else {
            0
        };
        *total = total.checked_add(area).ok_or_else(|| bad("repeat cache budget overflow"))?;
        for surface in l.mask.iter().map(|m| &m.surface).chain(l.fill_cache.iter().map(|c| &c.surface)) {
            *total = total.checked_add(surface_pixels(surface, a)?).ok_or_else(|| bad("repeat mask cache budget overflow"))?;
        }
        if *total > MAX_CACHE_PIXELS {
            return Err(bad("repeat aggregate cache pixel budget exceeded"));
        }
        if let Some(children) = l.children() {
            budget(children, ex, doc, total)?;
        }
    }
    Ok(())
}

/// Every call reads source geometry anew. Callers cache by the complete document Arc and
/// revision, plus preview generation/view key; a source edit can never reuse old instances.
pub fn project_document(doc: &Document) -> Result<Document> {
    let roots: BTreeSet<_> = doc.walk().into_iter().filter(|(_, _, l)| matches!(l.vector.live, Some(LiveGroup::Repeat(_)))).map(|(_, _, l)| l.id).collect();
    if roots.is_empty() {
        return Ok(doc.clone());
    }
    let ex = evaluator::expand_layers(&doc.layers).map_err(|e| bad(e.to_string()))?;
    fn collect(layers: &[Layer], roots: &BTreeSet<LayerId>) -> Vec<Layer> {
        layers.iter().flat_map(|l| if roots.contains(&l.id) { vec![l.clone()] } else { l.children().map(|c| collect(c, roots)).unwrap_or_default() }).collect()
    }
    let affected = collect(&ex.layers, &roots);
    // Validate before compiling curves (malformed stroke/dashes must never enter raster loops).
    photocraft_doc::vector_design::symbol_validation::validate(&[], &affected).map_err(|e| bad(e.to_string()))?;
    budget(&affected, &ex, doc, &mut 0)?;
    fn prepare(doc: &Document, layers: &mut [Layer], roots: &BTreeSet<LayerId>, ex: &Expansion) -> Result<()> {
        for l in layers {
            if roots.contains(&l.id) {
                let mut ready =
                    super::super::symbols::prepare_expansion(doc, symbol_expansion::Expansion { layers: vec![l.clone()], transforms: ex.transforms.clone() })?;
                *l = ready.pop().ok_or_else(|| bad("repeat preparation returned no group"))?;
            } else if let Some(children) = l.children_mut() {
                prepare(doc, children, roots, ex)?;
            }
        }
        Ok(())
    }
    let mut projected = doc.clone();
    projected.layers = ex.layers.clone();
    prepare(doc, &mut projected.layers, &roots, &ex)?;
    Ok(projected)
}
