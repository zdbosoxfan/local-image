use super::super::path_text::{self, on_path};
use super::*;
use photocraft_doc::{ShapeLayer, TextLayer};
use photocraft_text::{layout::TextLayout, render};
// Ordinary conversion, path text and transforms share the same painted contour adapter.
use super::super::path_text::curves as contours;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConversionChoice {
    #[default]
    Ask,
    Curves,
    Envelope,
}
/// Typed 20 handoff; the envelope provider must implement detached preview and atomic commit.
#[derive(Clone, Debug, PartialEq)]
pub struct EnvelopeRequest {
    pub transform: Commit,
}
pub trait EnvelopeRouter {
    fn preview(&self, session: &Session, request: &EnvelopeRequest) -> Result<Document>;
    fn commit(&self, session: &mut Session, request: &EnvelopeRequest) -> Result<Vec<LayerId>>;
}
pub fn conversion_targets(doc: &Document, ids: &[LayerId]) -> Vec<LayerId> {
    fn walk(layer: &Layer, out: &mut Vec<LayerId>) {
        match &layer.content {
            LayerContent::Text(_) | LayerContent::Symbol(_) => out.push(layer.id),
            LayerContent::Group(g) => {
                for child in &g.children {
                    walk(child, out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    for id in ids {
        if let Some(layer) = doc.layer(*id) {
            walk(layer, &mut out);
        }
    }
    out
}
pub(crate) fn ordinary(t: &TextLayer, shaped: TextLayout) -> Result<Vec<on_path::PaintedOutline>> {
    let warp = render::layout_warp(&shaped, t.warp.as_ref());
    if let Some(warp) = warp {
        if !shaped.decorations.is_empty() || shaped.styles.iter().any(|s| s.faux_bold) || shaped.faces.iter().any(|f| f.embolden) {
            return Err(error("styled warped text needs a painted warp outline provider"));
        }
        return Ok(render::painted_outlines(&shaped, &t.transform, Some(&warp))
            .into_iter()
            .map(|(color, elements)| on_path::PaintedOutline { color, elements })
            .collect());
    }
    // Identity placement reuses 12's per-style glyph, faux-bold and decoration projector.
    // Every glyph sees the same identity transform; decoration clusters retain their spans.
    let clusters = shaped
        .clusters
        .iter()
        .enumerate()
        .map(|(source_index, c)| on_path::Cluster {
            source: c.range.clone(),
            source_index,
            advance: f64::from(c.advance),
            distance: 0.,
            anchor: Point::default(),
            angle: 0.,
            transform: Affine::IDENTITY,
            visible: true,
            rtl: c.rtl,
        })
        .collect();
    let glyph_clusters = vec![0; shaped.glyphs.len()];
    let layout = on_path::PathLayout {
        shaped,
        clusters,
        glyph_clusters,
        advance: 0.,
        used: 0.,
        available: 0.,
        overflow: false,
        start: Point::default(),
        end: Point::default(),
    };
    on_path::validated_outlines(&layout, &t.transform).map_err(|e| error(e.to_string()))
}
pub(super) fn convert(doc: &Document, mut layer: Layer, choice: ConversionChoice) -> Result<Layer> {
    match choice {
        ConversionChoice::Ask => return Err(error("text and symbols need conversion; choose Convert to Curves or Non-destructive Envelope")),
        ConversionChoice::Envelope => return Err(error("route Non-destructive Envelope to package 20")),
        ConversionChoice::Curves => {}
    }
    match &layer.content {
        LayerContent::Symbol(_) => return super::super::symbols::detach_prepared(doc, &layer),
        LayerContent::Text(t) => {
            let mut engine = photocraft_text::shared().lock().map_err(|_| error("text engine unavailable"))?;
            let outlines = if let Some(path) = &layer.vector.on_path {
                let layout = path_text::project(&mut engine, doc, t, path)?;
                on_path::validated_outlines(&layout, &t.transform).map_err(|e| error(e.to_string()))?
            } else {
                ordinary(t, engine.layout(t, doc.resolution_dpi))?
            };
            drop(engine);
            let mut children = Vec::new();
            for (color, path) in contours::paths(&outlines) {
                super::validate_path(&path)?;
                children.push(Layer::new("Text curves", LayerContent::Shape(ShapeLayer { path, fill: Some(Fill::Solid(color)), ..Default::default() })));
            }
            if children.is_empty() || children.len() > MAX_TARGETS {
                return Err(error("text has no convertible outlines or exceeds the object budget"));
            }
            layer.content = if children.len() == 1 {
                children.remove(0).content
            } else {
                if doc.path_of(layer.id).is_some_and(|p| p.len() >= photocraft_doc::MAX_GROUP_DEPTH) {
                    return Err(error("text conversion exceeds the group depth limit"));
                }
                Layer::group("Text curves", children).content
            };
            layer.vector.on_path = None;
            layer.psd_blocks.retain(|(key, _)| key != b"TySh");
        }
        _ => {}
    }
    Ok(layer)
}
