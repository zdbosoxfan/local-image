//! Atomic path-text edits and detached pc-text projection. Original Local Image code.
//! V1 owns arc-length/nearest geometry. See integration/12.md for shared type routing.
use crate::{EngineError, Result, Session, commands::CommandSpec};
use photocraft_doc::{
    Document, Fill, Layer, LayerContent, LayerId, Path, ShapeLayer, TextLayer,
    text::{CharStyle, ParagraphRun, ParagraphStyle, TextRun},
    vector_design::text::{PathText, PathTextAlign},
};
use photocraft_geom::Point;
use photocraft_pathops::{geom::arc::ArcPath, to_geometry};
use photocraft_text::TextEngine;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashSet;

// Compile the SINGLE owned pc-text source until 00 adds `pub mod on_path` to
// pc-text/lib.rs. Afterwards replace this path module/imports with
// `pub use photocraft_text::on_path;`. No second implementation or shaper.
pub mod curves;
pub use photocraft_text::on_path;

pub struct Sampler(ArcPath);
impl Sampler {
    pub fn new(path: &Path) -> Result<Self> {
        crate::vector_packages::pen::validate(path)?;
        if path.inverted || path.subpaths.len() != 1 || path.subpaths[0].knots.len() < 2 {
            return Err(error("path text needs one non-inverted continuous subpath"));
        }
        let arc = ArcPath::new(&to_geometry(path));
        if arc.is_empty() || arc.len() > 1_000_000. {
            return Err(error("path text curve has invalid length"));
        }
        Ok(Self(arc))
    }
    pub fn fraction(&self, point: Point) -> Result<f64> {
        if ![point.x, point.y].into_iter().all(|v| v.is_finite() && v.abs() <= 1_000_000.) {
            return Err(error("invalid marker position"));
        }
        self.0.fraction_at((point.x, point.y).into()).ok_or_else(|| error("empty path text curve"))
    }
}
impl on_path::Curve for Sampler {
    fn length(&self) -> f64 {
        self.0.len()
    }
    fn closed(&self) -> bool {
        self.0.is_closed()
    }
    fn sample(&self, s: f64) -> (Point, Point) {
        let (p, t) = self.0.at(s);
        (Point::new(p.x, p.y), Point::new(t.x, t.y))
    }
}
fn error(message: &str) -> EngineError {
    EngineError::Other(message.into())
}
fn text_error(e: on_path::Error) -> EngineError {
    error(e.0)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum Target {
    New { curve: LayerId },
    Text { layer: LayerId },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Edit {
    pub target: Target,
    pub path: PathText,
    #[serde(default)]
    pub text: Option<String>,
    /// Full style on creation; existing styled runs remain owned by TextLayer.
    #[serde(default)]
    pub style: Option<CharStyle>,
    #[serde(default)]
    pub orientation: Option<photocraft_doc::text::Orientation>,
    #[serde(default)]
    pub paragraph: Option<ParagraphStyle>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Commit {
    pub document: u64,
    pub revision: u64,
    pub edits: Vec<Edit>,
}

pub fn check_fresh(s: &Session, document: u64, revision: u64) -> Result<()> {
    let state = s.active().ok_or(EngineError::NoDocument)?;
    if state.doc.id.0 != document || state.revision != revision {
        return Err(error("path text gesture is stale"));
    }
    Ok(())
}
fn preflight(doc: &Document, edits: &[Edit]) -> Result<()> {
    if edits.is_empty() || edits.len() > 256 {
        return Err(error("invalid path text target count"));
    }
    let mut seen = HashSet::new();
    for edit in edits {
        let id = match edit.target {
            Target::New { curve } => curve,
            Target::Text { layer } => layer,
        };
        if !seen.insert(id) {
            return Err(error("duplicate path text target"));
        }
        let l = doc.layer(id).ok_or(EngineError::NoLayer(id))?;
        let locks = doc.effective_locks(id);
        if locks.all || locks.pixels {
            return Err(error("path text target is locked"));
        }
        match edit.target {
            Target::New { .. } => {
                let LayerContent::Shape(sh) = &l.content else {
                    return Err(error("type click requires a curve layer"));
                };
                if edit.path.path != sh.path {
                    return Err(error("type curve changed before attachment"));
                }
                if l.vector.live_shape.is_some() || l.vector.live.is_some() {
                    return Err(error("type click requires evaluated curve geometry"));
                }
            }
            Target::Text { .. } => {
                if !matches!(l.content, LayerContent::Text(_)) {
                    return Err(error("path text edit needs a type layer"));
                }
                if edit.style.is_some() || edit.orientation.is_some() || edit.paragraph.is_some() {
                    return Err(error("existing path text retains character runs"));
                }
            }
        }
        Sampler::new(&edit.path.path)?;
        if ![edit.path.start, edit.path.end].into_iter().all(|v| v.is_finite() && (0.0..=1.0).contains(&v)) {
            return Err(error("invalid path text brackets"));
        }
        if edit.text.as_ref().is_some_and(|t| t.len() > 262_144) {
            return Err(error("path text exceeds the text budget"));
        }
    }
    Ok(())
}
pub fn project(engine: &mut TextEngine, doc: &Document, t: &TextLayer, path: &PathText) -> Result<on_path::PathLayout> {
    on_path::layout(engine, t, &Sampler::new(&path.path)?, path, doc.resolution_dpi).map_err(text_error)
}
/// Refresh hook for 00's ordinary type edits/native reload/display projection.
pub fn refresh(doc: &Document, t: &mut TextLayer, path: &PathText) -> Result<on_path::PathLayout> {
    let mut engine = photocraft_text::shared().lock().map_err(|_| error("text engine is unavailable"))?;
    let layout = project(&mut engine, doc, t, path)?;
    t.cache = Some(on_path::rasterize(&layout, &t.transform, doc.pixel_format(), t.antialias, doc.bounds()).map_err(text_error)?.surface);
    t.psd_raw = None;
    Ok(layout)
}
fn replace_text(layer: &mut TextLayer, new: &str) {
    if layer.text == new {
        return;
    }
    let prefix = layer.text.chars().zip(new.chars()).take_while(|(a, b)| a == b).map(|(c, _)| c.len_utf8()).sum::<usize>();
    let suffix = layer.text[prefix..].chars().rev().zip(new[prefix..].chars().rev()).take_while(|(a, b)| a == b).map(|(c, _)| c.len_utf8()).sum::<usize>();
    let end = layer.text.len() - suffix;
    crate::type_cmds::replace_text(layer, prefix, end, &new[prefix..new.len() - suffix]);
}
fn prepare(s: &Session, c: &Commit) -> Result<(Document, Vec<LayerId>)> {
    check_fresh(s, c.document, c.revision)?;
    let source = &s.active().ok_or(EngineError::NoDocument)?.doc;
    preflight(source, &c.edits)?;
    let mut doc = (**source).clone();
    let mut ids = Vec::new();
    for edit in &c.edits {
        let mut t = match edit.target {
            Target::New { .. } => TextLayer::default(),
            Target::Text { layer } => match &source.layer(layer).ok_or(EngineError::NoLayer(layer))?.content {
                LayerContent::Text(t) => t.clone(),
                _ => return Err(error("type layer disappeared")),
            },
        };
        if let Some(text) = &edit.text {
            replace_text(&mut t, text);
        }
        if let Some(style) = &edit.style {
            t.runs = vec![TextRun { len: t.text.len(), style: style.clone() }];
        }
        if let Some(orientation) = edit.orientation {
            t.orientation = orientation;
        }
        if let Some(paragraph) = &edit.paragraph {
            t.paragraphs = vec![ParagraphRun { len: t.text.len(), style: paragraph.clone() }];
        }
        t.runs = t.char_runs();
        t.sync_summary();
        refresh(source, &mut t, &edit.path)?;
        let id = match edit.target {
            Target::New { curve } => {
                let mut layer = Layer::new(doc.next_layer_name("Path text"), LayerContent::Text(t));
                layer.vector.on_path = Some(edit.path.clone());
                doc.insert_above(Some(curve), layer)
            }
            Target::Text { layer } => {
                let l = doc.layer_mut(layer).ok_or(EngineError::NoLayer(layer))?;
                l.content = LayerContent::Text(t);
                l.vector.on_path = Some(edit.path.clone());
                l.psd_blocks.retain(|(key, _)| key != b"TySh");
                layer
            }
        };
        ids.push(id);
    }
    Ok((doc, ids))
}
pub fn preview(s: &Session, c: &Commit) -> Result<(Document, Vec<LayerId>)> {
    prepare(s, c)
}
pub fn preview_document(s: &Session, c: &Commit) -> Result<Document> {
    prepare(s, c).map(|p| p.0)
}
fn commit(s: &mut Session, p: &Value) -> Result<Value> {
    let c: Commit = serde_json::from_value(p.clone()).map_err(|e| error(&e.to_string()))?;
    let (doc, ids) = prepare(s, &c)?;
    check_fresh(s, c.document, c.revision)?;
    preflight(&s.active().ok_or(EngineError::NoDocument)?.doc, &c.edits)?;
    let active = ids.last().copied();
    s.edit("Path text", move |d, a| {
        *d = doc;
        *a = active;
        Ok(())
    })?;
    Ok(json!({"layers":ids.iter().map(|id| id.0).collect::<Vec<_>>()}))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Convert {
    document: u64,
    revision: u64,
    layers: Vec<LayerId>,
}
pub(crate) fn convert(s: &mut Session, p: &Value) -> Result<Value> {
    let request: Convert = serde_json::from_value(p.clone()).map_err(|e| error(&e.to_string()))?;
    check_fresh(s, request.document, request.revision)?;
    let source = &s.active().ok_or(EngineError::NoDocument)?.doc;
    if request.layers.is_empty() || request.layers.len() > 256 {
        return Err(error("invalid conversion targets"));
    }
    let mut seen = HashSet::new();
    for &id in &request.layers {
        if !seen.insert(id) {
            return Err(error("duplicate conversion target"));
        }
        let layer = source.layer(id).ok_or(EngineError::NoLayer(id))?;
        let locks = source.effective_locks(id);
        if locks.all || locks.pixels || !matches!(layer.content, LayerContent::Text(_)) || layer.vector.on_path.is_none() {
            return Err(error("conversion needs unlocked path text"));
        }
    }
    let mut doc = (**source).clone();
    for &id in &request.layers {
        let original = source.layer(id).ok_or(EngineError::NoLayer(id))?;
        let LayerContent::Text(t) = &original.content else {
            return Err(error("type layer disappeared"));
        };
        let path = original.vector.on_path.as_ref().ok_or_else(|| error("path text metadata missing"))?;
        let mut engine = photocraft_text::shared().lock().map_err(|_| error("text engine unavailable"))?;
        let layout = project(&mut engine, source, t, path)?;
        let mut children = Vec::new();
        for (color, path) in curves::paths(&on_path::validated_outlines(&layout, &t.transform).map_err(text_error)?) {
            let mut shape = ShapeLayer { path, fill: Some(Fill::Solid(color)), ..Default::default() };
            shape.cache = Some(photocraft_vector::render_shape(&shape, source.pixel_format(), source.bounds()));
            children.push(Layer::new("Text curves", LayerContent::Shape(shape)));
        }
        let content = if children.len() == 1 {
            children.remove(0).content
        } else {
            if source.path_of(id).is_some_and(|p| p.len() >= photocraft_doc::MAX_GROUP_DEPTH) {
                return Err(error("text conversion exceeds the group depth limit"));
            }
            Layer::group("Text curves", children).content
        };
        let layer = doc.layer_mut(id).ok_or(EngineError::NoLayer(id))?;
        layer.content = content;
        layer.vector.on_path = None;
        layer.psd_blocks.retain(|(key, _)| key != b"TySh");
    }
    check_fresh(s, request.document, request.revision)?;
    s.edit("Convert path text to curves", move |d, _| {
        *d = doc;
        Ok(())
    })?;
    Ok(json!({"layers":request.layers.iter().map(|id| id.0).collect::<Vec<_>>()}))
}
/// Hit-test curve layers front-to-back in document pixels.
pub fn curve_at(doc: &Document, point: Point, tolerance: f64) -> Result<Option<(LayerId, PathText)>> {
    if !tolerance.is_finite() || !(0.0..=1000.0).contains(&tolerance) {
        return Err(error("invalid type hit tolerance"));
    }
    for (path, _, layer) in doc.walk().into_iter().rev() {
        if (1..path.len()).any(|depth| doc.layer_at(&path[..depth]).is_some_and(|parent| !parent.visible))
            || !layer.visible
            || doc.effective_locks(layer.id).all
            || doc.effective_locks(layer.id).pixels
        {
            continue;
        }
        let LayerContent::Shape(shape) = &layer.content else { continue };
        let Ok(sampler) = Sampler::new(&shape.path) else { continue };
        if let Some((_, _, _, _, distance)) = photocraft_pathops::nearest(&shape.path, point).map_err(|e| error(&e.to_string()))?
            && distance <= tolerance
        {
            return Ok(Some((
                layer.id,
                PathText { path: shape.path.clone(), start: sampler.fraction(point)?, end: 1., flip: false, align: PathTextAlign::Baseline },
            )));
        }
    }
    Ok(None)
}
pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "type.path.commit",
            label: "Path text",
            menu: &[],
            shortcut: None,
            params: "{document:u64,revision:u64,edits:[{target,path,text?,style?,orientation?,paragraph?}]}",
            enabled: crate::vector_cmds::has_doc,
            run: commit,
            journal: true,
        },
        CommandSpec {
            id: "type.path.convert",
            label: "Convert path text to curves",
            menu: &[],
            shortcut: None,
            params: "{document:u64,revision:u64,layers:[id]}",
            enabled: crate::vector_cmds::has_doc,
            run: convert,
            journal: true,
        },
    ]
}
#[cfg(test)]
#[path = "path_text/tests.rs"]
mod tests;
