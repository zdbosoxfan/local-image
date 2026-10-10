//! Bounded numeric/tree preflight precedes cloning, normalization, shaping and raster work.
use super::*;
const MAX: f64 = 1_000_000.0;
fn bounded(n: f64) -> Result<()> {
    if n.is_finite() && n.abs() <= MAX { Ok(()) } else { Err(invalid("numeric input must be finite and bounded")) }
}
fn unit(n: f32) -> Result<()> {
    if (0.0..=1.0).contains(&n) { Ok(()) } else { Err(invalid("opacity/position must be in 0..=1")) }
}
fn color(c: &photocraft_doc::Color) -> Result<()> {
    for n in c.c {
        bounded(f64::from(n))?;
    }
    unit(c.alpha)
}
fn path(p: &Path, segments: &mut usize) -> Result<()> {
    for s in &p.subpaths {
        *segments = segments.saturating_add(s.knots.len());
        if *segments > 1_000_000 {
            return Err(ExportError::Budget("segments"));
        }
        for k in &s.knots {
            for p in [k.anchor, k.in_ctrl, k.out_ctrl] {
                bounded(p.x)?;
                bounded(p.y)?;
            }
        }
    }
    Ok(())
}
fn fill(f: &Fill) -> Result<()> {
    match f {
        Fill::Solid(c) => color(c)?,
        Fill::Gradient { stops, angle, scale, opacity_stops, midpoints, offset, .. } => {
            if stops.len() > 4096 || opacity_stops.len() > 4096 || midpoints.len() > 4096 {
                return Err(ExportError::Budget("gradient stops"));
            }
            for n in [*angle, *scale, offset.0, offset.1] {
                bounded(f64::from(n))?;
            }
            if *scale <= 0.0 {
                return Err(invalid("gradient scale must be positive"));
            }
            for (p, c) in stops {
                unit(*p)?;
                color(c)?;
            }
            for (p, a) in opacity_stops {
                unit(*p)?;
                unit(*a)?;
            }
            for m in midpoints {
                unit(*m)?;
            }
        }
        Fill::Pattern { scale, angle, phase, .. } => {
            for n in [*scale, *angle, phase.0, phase.1] {
                bounded(f64::from(n))?;
            }
            if *scale <= 0.0 {
                return Err(invalid("pattern scale must be positive"));
            }
        }
    }
    Ok(())
}
fn stroke(s: &ShapeStroke) -> Result<()> {
    fill(&s.paint)?;
    unit(s.opacity)?;
    for n in [s.width, s.dash_offset, s.miter_limit] {
        bounded(f64::from(n))?;
    }
    if s.width < 0.0 || s.width > 5000.0 || s.miter_limit < 1.0 || s.dashes.len() > 1024 {
        return Err(invalid("invalid stroke dimensions"));
    }
    for d in &s.dashes {
        bounded(f64::from(*d))?;
        if *d < 0.0 {
            return Err(invalid("negative dash length"));
        }
    }
    Ok(())
}
pub(super) fn document(doc: &Document, opts: &ExportOptions) -> Result<()> {
    if opts.max_pixels == 0 || opts.max_pixels > 100_000_000 || opts.max_bytes == 0 || opts.max_bytes > 512 << 20 {
        return Err(invalid("invalid export options"));
    }
    if doc.size.is_empty()
        || doc.size.width > 1_000_000
        || doc.size.height > 1_000_000
        || !doc.resolution_dpi.is_finite()
        || !(1.0..=9600.0).contains(&doc.resolution_dpi)
    {
        return Err(invalid("invalid document dimensions or DPI"));
    }
    fn walk(layers: &[Layer], depth: usize, segments: &mut usize, objects: &mut usize, ids: &mut BTreeSet<u64>) -> Result<()> {
        if depth > photocraft_doc::MAX_GROUP_DEPTH {
            return Err(ExportError::Budget("depth"));
        }
        for l in layers {
            *objects += 1;
            if *objects > 100_000 {
                return Err(ExportError::Budget("objects"));
            }
            if !ids.insert(l.id.0) {
                return Err(invalid("duplicate layer IDs"));
            }
            unit(l.opacity)?;
            unit(l.fill_opacity)?;
            if let Some((x, y)) = l.effects.reference {
                bounded(x)?;
                bounded(y)?;
            }
            if let Some(m) = &l.mask {
                unit(m.density)?;
                bounded(f64::from(m.feather))?;
                if m.feather < 0.0 {
                    return Err(invalid("negative mask feather"));
                }
            }
            // Effects are serde values. Null is legal only for optional bevel contour/texture;
            // serde's nonfinite-float-to-null conversion must never conceal a bad number.
            fn numbers(v: &serde_json::Value, nullable: bool) -> Result<()> {
                match v {
                    serde_json::Value::Number(n) => bounded(n.as_f64().ok_or_else(|| invalid("invalid number"))?),
                    serde_json::Value::Null if !nullable => Err(invalid("nonfinite effect number")),
                    serde_json::Value::Array(a) => {
                        for v in a {
                            numbers(v, false)?;
                        }
                        Ok(())
                    }
                    serde_json::Value::Object(o) => {
                        for (k, v) in o {
                            numbers(v, k == "contour" || k == "texture")?;
                        }
                        Ok(())
                    }
                    _ => Ok(()),
                }
            }
            let effects = serde_json::to_value(&l.effects.items).map_err(|e| invalid(e.to_string()))?;
            numbers(&effects, false)?;
            let vector = serde_json::to_value(&l.vector).map_err(|e| invalid(e.to_string()))?;
            numbers(&vector, false)?;
            if l.name.len() > 1 << 20 {
                return Err(ExportError::Budget("layer name"));
            }
            if let Some(p) = &l.vector.clip {
                path(p, segments)?;
            }
            if let Some(m) = &l.vector_mask {
                path(&m.path, segments)?;
                unit(m.density)?;
                bounded(f64::from(m.feather))?;
                if m.feather < 0.0 {
                    return Err(invalid("negative mask feather"));
                }
            }
            if let Some(a) = &l.vector.appearance {
                photocraft_vector::appearance::validate(a).map_err(invalid)?;
                if a.items.len() > 4096 {
                    return Err(ExportError::Budget("appearance items"));
                }
                for i in &a.items {
                    match i {
                        AppearanceItem::Fill(f) => {
                            unit(f.opacity)?;
                            fill(&f.paint)?;
                        }
                        AppearanceItem::Stroke(s) => stroke(&s.base)?,
                    }
                }
            }
            match &l.content {
                LayerContent::Group(g) => {
                    if let Some(a) = &g.artboard
                        && let photocraft_doc::ArtboardBackground::Custom(c) = a.background
                    {
                        color(&c)?;
                    }
                    if let Some(a) = &g.artboard
                        && (a.rect.is_empty() || [a.rect.x0, a.rect.y0, a.rect.x1, a.rect.y1].iter().any(|v| v.unsigned_abs() > 1_000_000))
                    {
                        return Err(invalid("invalid artboard dimensions"));
                    }
                    walk(&g.children, depth + 1, segments, objects, ids)?;
                }
                LayerContent::Shape(s) => {
                    path(&s.path, segments)?;
                    if let Some(f) = &s.fill {
                        fill(f)?;
                    }
                    if let Some(st) = &s.stroke {
                        stroke(st)?;
                    }
                }
                LayerContent::Fill(f) => fill(f)?,
                LayerContent::Text(t) => {
                    if t.runs.len() > 1024 || t.paragraphs.len() > 1024 {
                        return Err(ExportError::Budget("text runs"));
                    }
                    if t.text.len() > 1 << 20 {
                        return Err(ExportError::Budget("text bytes"));
                    }
                    for n in t.transform.m {
                        bounded(n)?;
                    }
                    if let Some(w) = &t.warp {
                        for v in [w.value, w.horizontal_distortion, w.vertical_distortion] {
                            bounded(f64::from(v))?;
                        }
                    }
                    if let photocraft_doc::text::TextShape::Box { x, y, width, height } = t.shape {
                        for v in [x, y, width, height] {
                            bounded(f64::from(v))?;
                        }
                        if width <= 0.0 || height <= 0.0 {
                            return Err(invalid("invalid text box dimensions"));
                        }
                    }
                    for p in t.paragraph_runs() {
                        for v in [
                            p.style.first_line_indent_pt,
                            p.style.start_indent_pt,
                            p.style.end_indent_pt,
                            p.style.space_before_pt,
                            p.style.space_after_pt,
                            p.style.auto_leading,
                        ] {
                            bounded(f64::from(v))?;
                        }
                    }
                    for s in t.char_runs().iter().map(|r| &r.style) {
                        color(&s.color)?;
                        for n in [s.size_pt, s.tracking, s.baseline_shift_pt, s.horizontal_scale, s.vertical_scale, s.kern, s.leading_pt.unwrap_or(0.0)] {
                            bounded(f64::from(n))?;
                        }
                        for v in &s.variations {
                            bounded(f64::from(v.value))?;
                        }
                        if s.size_pt <= 0.0 || s.size_pt > 5000.0 {
                            return Err(invalid("invalid text size"));
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
    walk(&doc.layers, 0, &mut 0, &mut 0, &mut BTreeSet::new())
}
