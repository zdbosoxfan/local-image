//! Bounded outlines for SVG text subtrees whose opacity requires real compositing groups.
//! Shape once in pc-text, retain its source-run/cluster mapping, and group the resulting
//! contours without reflowing text or applying group opacity separately to every glyph.
use super::super::{ImportError, Result};
use photocraft_doc::{Affine, Color, vector_design::exchange::ImportOptions};
use photocraft_text::{
    TextLayout,
    render::{OutlineEvent, OutlineVisitError, OutlineVisitLimits, PathEl},
};
use std::sync::atomic::{AtomicBool, Ordering};
use usvg::tiny_skia_path::{Path, PathBuilder, Transform};

#[derive(Clone, Copy)]
pub(super) struct GroupRef {
    pub id: roxmltree::NodeId,
    pub opacity: f32,
}

pub(crate) enum OutlineNode {
    Group { id: roxmltree::NodeId, opacity: f32, children: Vec<OutlineNode> },
    Paint { color: Color, paths: Vec<Path> },
}

struct Glyph {
    run: usize,
    radius: f64,
    bold: bool,
    builder: PathBuilder,
}
const BOLD_OFFSETS: &[(f64, f64)] = &[(-1., 0.), (1., 0.), (0., -1.), (0., 1.), (0.7, 0.7), (-0.7, -0.7), (0.7, -0.7), (-0.7, 0.7)];
fn finish_glyph(glyph: Option<Glyph>, runs: &mut [PathBuilder], transform: &Affine) -> Result<()> {
    let Some(glyph) = glyph else { return Ok(()) };
    let Some(path) = glyph.builder.finish() else { return Ok(()) };
    let run = runs.get_mut(glyph.run).ok_or_else(|| ImportError::Invalid("missing text source run".into()))?;
    if glyph.bold {
        for &(x, y) in BOLD_OFFSETS {
            let dx = (transform.m[0] * x + transform.m[2] * y) * glyph.radius;
            let dy = (transform.m[1] * x + transform.m[3] * y) * glyph.radius;
            let path = path.clone().transform(Transform::from_translate(dx as f32, dy as f32)).ok_or(ImportError::Limit("text outline coordinate"))?;
            run.push_path(&path);
        }
    } else {
        run.push_path(&path);
    }
    Ok(())
}
fn insert(nodes: &mut Vec<OutlineNode>, groups: &[GroupRef], color: Color, path: Path) {
    if let Some((group, rest)) = groups.split_first() {
        if !matches!(nodes.last(), Some(OutlineNode::Group { id, .. }) if *id == group.id) {
            nodes.push(OutlineNode::Group { id: group.id, opacity: group.opacity, children: Vec::new() });
        }
        if let Some(OutlineNode::Group { children, .. }) = nodes.last_mut() {
            insert(children, rest, color, path);
        }
    } else if let Some(OutlineNode::Paint { color: previous, paths }) = nodes.last_mut()
        && *previous == color
    {
        paths.push(path);
    } else {
        nodes.push(OutlineNode::Paint { color, paths: vec![path] });
    }
}

pub(super) fn build(
    layout: &TextLayout,
    transform: &Affine,
    sources: &[Vec<GroupRef>],
    options: &ImportOptions,
    cancel: &AtomicBool,
    used_segments: &mut usize,
) -> Result<Vec<OutlineNode>> {
    // Charge retained outline coordinates before allocation, including synthetic bold copies.
    // This budget is shared by every outlined text object in the import, not reset per span.
    let cap = options.limits.segments.min(1_000_000).min((options.limits.decompressed_bytes / 64) as usize);
    let limits = OutlineVisitLimits {
        max_glyphs: options.limits.objects,
        max_segments: cap,
        max_scratch_bytes: (options.limits.decompressed_bytes.min(16 << 20)) as usize,
    };
    let mut runs: Vec<_> = layout.styles.iter().map(|_| PathBuilder::new()).collect();
    let mut current: Option<Glyph> = None;
    photocraft_text::render::visit_outlines(layout, transform, None, limits, &|| cancel.load(Ordering::Relaxed), |event| {
        match event {
            OutlineEvent::Glyph { index, .. } => {
                finish_glyph(current.take(), &mut runs, transform)?;
                let glyph = &layout.glyphs[index];
                // Source clusters and run brushes come from the single authoritative layout.
                if layout.clusters.get(glyph.source_cluster).is_none() || sources.get(glyph.style as usize).is_none() {
                    return Err(ImportError::Invalid("missing text source cluster/run".into()));
                }
                let face = &layout.faces[glyph.face as usize];
                current = Some(Glyph {
                    run: glyph.style as usize,
                    radius: f64::from(face.size_px * photocraft_text::render::FAUX_BOLD_RADIUS),
                    bold: layout.styles[glyph.style as usize].faux_bold || face.embolden,
                    builder: PathBuilder::new(),
                });
            }
            OutlineEvent::Element(element) => {
                let glyph = current.as_mut().ok_or_else(|| ImportError::Invalid("text outline has no glyph".into()))?;
                let count = if glyph.bold { BOLD_OFFSETS.len() } else { 1 };
                *used_segments = used_segments.saturating_add(count);
                if *used_segments > cap {
                    return Err(ImportError::Limit("text outline geometry"));
                }
                let point = |p: [f64; 2]| -> Result<[f32; 2]> {
                    if p.iter().any(|v| !v.is_finite() || v.abs() > super::MAX_COORD) {
                        return Err(ImportError::Limit("text outline coordinate"));
                    }
                    Ok([p[0] as f32, p[1] as f32])
                };
                match element {
                    PathEl::MoveTo(p) => {
                        let p = point(p)?;
                        glyph.builder.move_to(p[0], p[1]);
                    }
                    PathEl::LineTo(p) => {
                        let p = point(p)?;
                        glyph.builder.line_to(p[0], p[1]);
                    }
                    PathEl::QuadTo(c, p) => {
                        let c = point(c)?;
                        let p = point(p)?;
                        glyph.builder.quad_to(c[0], c[1], p[0], p[1]);
                    }
                    PathEl::CurveTo(a, b, p) => {
                        let a = point(a)?;
                        let b = point(b)?;
                        let p = point(p)?;
                        glyph.builder.cubic_to(a[0], a[1], b[0], b[1], p[0], p[1]);
                    }
                    PathEl::Close => glyph.builder.close(),
                }
            }
        }
        Ok(())
    })
    .map_err(|error| match error {
        OutlineVisitError::Cancelled => ImportError::Cancelled,
        OutlineVisitError::Budget(kind) => ImportError::Limit(kind),
        OutlineVisitError::Visitor(error) => error,
        OutlineVisitError::InvalidOutline => ImportError::Invalid("invalid text outline".into()),
    })?;
    finish_glyph(current, &mut runs, transform)?;
    let mut result = Vec::new();
    for (index, run) in runs.into_iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return Err(ImportError::Cancelled);
        }
        let color = layout.styles[index].color;
        if color.alpha > 0.0
            && let Some(path) = run.finish()
        {
            insert(&mut result, &sources[index], color, path);
        }
    }
    Ok(result)
}
