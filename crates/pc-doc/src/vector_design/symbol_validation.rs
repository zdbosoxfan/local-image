//! Original GPL-3.0-or-later symbol validation, following Affinity Symbols and SVG 1.1 §5.5–5.6.
//! Data-only provider shared by the native and file adapters.
use crate::{Affine, Color, Document, Fill, Layer, LayerContent, MAX_GROUP_DEPTH, Path, SymbolDef, SymbolId};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

pub const MAX_OBJECTS: usize = 100_000;
pub const MAX_COORDINATE: f64 = 1_000_000.;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SymbolError(pub String);
impl fmt::Display for SymbolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for SymbolError {}
pub type Result<T> = std::result::Result<T, SymbolError>;
pub fn error(message: impl Into<String>) -> SymbolError {
    SymbolError(message.into())
}

pub fn affine(a: Affine) -> Result<()> {
    if a.m.iter().any(|v| !v.is_finite() || v.abs() > MAX_COORDINATE) || !a.determinant().is_finite() || a.inverse().is_none() {
        return Err(error("symbol transform must be finite, bounded and invertible"));
    }
    Ok(())
}

fn number(v: f64) -> Result<()> {
    if !v.is_finite() || v.abs() > MAX_COORDINATE {
        return Err(error("symbol numeric input must be finite and bounded"));
    }
    Ok(())
}
fn color(c: &Color) -> Result<()> {
    for v in c.c.into_iter().chain([c.alpha]) {
        number(f64::from(v))?;
    }
    Ok(())
}
fn paint(fill: &Fill) -> Result<()> {
    match fill {
        Fill::Solid(c) => color(c)?,
        Fill::Gradient { stops, angle, scale, opacity_stops, midpoints, offset, .. } => {
            if stops.len().saturating_add(opacity_stops.len()).saturating_add(midpoints.len()) > MAX_OBJECTS {
                return Err(error("symbol paint stop budget exceeded"));
            }
            for (at, c) in stops {
                number(f64::from(*at))?;
                color(c)?;
            }
            for (at, opacity) in opacity_stops {
                number(f64::from(*at))?;
                number(f64::from(*opacity))?;
            }
            for v in midpoints.iter().chain([angle, scale, &offset.0, &offset.1]) {
                number(f64::from(*v))?;
            }
        }
        Fill::Pattern { scale, angle, phase, .. } => {
            for v in [scale, angle, &phase.0, &phase.1] {
                number(f64::from(*v))?;
            }
        }
    }
    Ok(())
}
fn path(p: &Path) -> Result<()> {
    if p.subpaths.iter().map(|s| s.knots.len()).sum::<usize>() > 8_000_000 {
        return Err(error("symbol path segment budget exceeded"));
    }
    for point in p.subpaths.iter().flat_map(|s| &s.knots).flat_map(|k| [k.anchor, k.in_ctrl, k.out_ctrl]) {
        number(point.x)?;
        number(point.y)?;
    }
    Ok(())
}
fn layer_data(l: &Layer) -> Result<()> {
    number(f64::from(l.opacity))?;
    number(f64::from(l.fill_opacity))?;
    if let Some(m) = &l.vector_mask {
        path(&m.path)?;
        number(f64::from(m.density))?;
        number(f64::from(m.feather))?;
    }
    match &l.content {
        LayerContent::Fill(f) => paint(f)?,
        LayerContent::Shape(s) => {
            path(&s.path)?;
            if let Some(f) = &s.fill {
                paint(f)?;
            }
            if let Some(stroke) = &s.stroke {
                paint(&stroke.paint)?;
                for v in [stroke.width, stroke.opacity, stroke.miter_limit, stroke.dash_offset].into_iter().chain(stroke.dashes.iter().copied()) {
                    number(f64::from(v))?;
                }
                if stroke.width < 0. || stroke.width > 10_000. || stroke.dashes.len() > MAX_OBJECTS {
                    return Err(error("invalid symbol stroke"));
                }
            }
        }
        LayerContent::Text(t) => {
            affine(t.transform)?;
            color(&t.color)?;
            number(f64::from(t.size_pt))?;
            if t.size_pt <= 0. || t.text.len() > 8_000_000 || t.runs.len() > MAX_OBJECTS {
                return Err(error("invalid symbol text budget/size"));
            }
            if let Some(w) = &t.warp {
                for v in [w.value, w.horizontal_distortion, w.vertical_distortion] {
                    number(f64::from(v))?;
                }
            }
            if t.runs.iter().try_fold(0usize, |count, run| count.checked_add(run.style.variations.len())).is_none_or(|count| count > MAX_OBJECTS) {
                return Err(error("invalid symbol text variation budget"));
            }
            for run in &t.runs {
                let s = &run.style;
                color(&s.color)?;
                for v in [s.size_pt, s.tracking, s.leading_pt.unwrap_or(0.), s.baseline_shift_pt, s.horizontal_scale, s.vertical_scale, s.kern]
                    .into_iter()
                    .chain(s.variations.iter().map(|v| v.value))
                {
                    number(f64::from(v))?;
                }
            }
        }
        _ => {}
    }
    Ok(())
}

/// Longest reference path, with three-colour DFS. Unused masters are checked as well.
pub fn graph_depths(graph: &BTreeMap<u64, Vec<SymbolId>>) -> Result<BTreeMap<u64, usize>> {
    fn visit(id: SymbolId, graph: &BTreeMap<u64, Vec<SymbolId>>, active: &mut BTreeSet<u64>, done: &mut BTreeMap<u64, usize>, depth: usize) -> Result<usize> {
        if depth > MAX_GROUP_DEPTH {
            return Err(error("symbol reference depth exceeds layer limit"));
        }
        if let Some(d) = done.get(&id.0) {
            return Ok(*d);
        }
        if !active.insert(id.0) {
            return Err(error(format!("symbol cycle at {}", id.0)));
        }
        let refs = graph.get(&id.0).ok_or_else(|| error(format!("missing symbol {}", id.0)))?;
        let mut longest = 0;
        for next in refs {
            longest = longest.max(1 + visit(*next, graph, active, done, depth + 1)?);
        }
        active.remove(&id.0);
        done.insert(id.0, longest);
        Ok(longest)
    }
    let mut done = BTreeMap::new();
    for id in graph.keys() {
        visit(SymbolId(*id), graph, &mut BTreeSet::new(), &mut done, 0)?;
    }
    Ok(done)
}

/// Whole target graph and both ID namespaces. Expansion depth/object budgets also bound fan-out.
pub fn validate(symbols: &[SymbolDef], layers: &[Layer]) -> Result<()> {
    validate_inner(symbols, layers, false)
}
// A selected subtree may itself belong to a master. Its IDs are already checked in
// document(), and must remain on the outer projected layers when detaching that subtree.
pub(super) fn validate_selection(symbols: &[SymbolDef], layers: &[Layer]) -> Result<()> {
    validate_inner(symbols, layers, true)
}
fn validate_inner(symbols: &[SymbolDef], layers: &[Layer], selection: bool) -> Result<()> {
    fn collect(layers: &[Layer], ids: &mut BTreeSet<u64>, refs: &mut Vec<SymbolId>, depth: usize, count: &mut usize) -> Result<()> {
        if depth > MAX_GROUP_DEPTH && !layers.is_empty() {
            return Err(error("symbol layer depth exceeds layer limit"));
        }
        for l in layers {
            *count += 1;
            if *count > MAX_OBJECTS {
                return Err(error("symbol object budget exceeded"));
            }
            if l.id.0 == 0 || !ids.insert(l.id.0) {
                return Err(error(format!("duplicate or zero layer ID {}", l.id.0)));
            }
            layer_data(l)?;
            match &l.content {
                LayerContent::Group(g) => collect(&g.children, ids, refs, depth + 1, count)?,
                LayerContent::Symbol(s) => {
                    affine(s.transform)?;
                    refs.push(s.symbol);
                }
                _ => {}
            }
        }
        Ok(())
    }
    if symbols.len() > MAX_OBJECTS {
        return Err(error("symbol master budget exceeded"));
    }
    let mut graph = BTreeMap::new();
    let mut ids = BTreeSet::new();
    let mut count = 0;
    let mut root_refs = Vec::new();
    collect(layers, &mut ids, &mut root_refs, 0, &mut count)?;
    if selection {
        ids.clear();
    }
    for s in symbols {
        if s.id.0 == 0 || graph.contains_key(&s.id.0) {
            return Err(error(format!("duplicate or zero symbol ID {}", s.id.0)));
        }
        let mut refs = Vec::new();
        collect(&s.layers, &mut ids, &mut refs, 0, &mut count)?;
        graph.insert(s.id.0, refs);
    }
    graph_depths(&graph)?;
    for id in root_refs {
        if !graph.contains_key(&id.0) {
            return Err(error(format!("missing symbol {}", id.0)));
        }
    }
    // Memoized template costs include group nesting, repeated references and invisible art.
    fn cost(layers: &[Layer], symbols: &BTreeMap<u64, &SymbolDef>, memo: &mut BTreeMap<u64, (usize, usize)>, depth: usize) -> Result<(usize, usize)> {
        if depth > MAX_GROUP_DEPTH && !layers.is_empty() {
            return Err(error("expanded symbol depth exceeded"));
        }
        let (mut count, mut height) = (0usize, 0usize);
        for l in layers {
            let children = match &l.content {
                LayerContent::Group(g) => cost(&g.children, symbols, memo, depth + 1)?,
                LayerContent::Symbol(s) => {
                    if let Some(c) = memo.get(&s.symbol.0) {
                        *c
                    } else {
                        let def = symbols.get(&s.symbol.0).ok_or_else(|| error("missing symbol"))?;
                        let c = cost(&def.layers, symbols, memo, depth + 1)?;
                        memo.insert(s.symbol.0, c);
                        c
                    }
                }
                _ => (0, 0),
            };
            count = count.saturating_add(1).saturating_add(children.0);
            height = height.max(1 + children.1);
            if count > MAX_OBJECTS || height > MAX_GROUP_DEPTH + 1 {
                return Err(error("expanded symbol exceeds layer/object budget"));
            }
        }
        Ok((count, height))
    }
    let defs: BTreeMap<_, _> = symbols.iter().map(|s| (s.id.0, s)).collect();
    let mut memo = BTreeMap::new();
    for s in symbols {
        cost(&s.layers, &defs, &mut memo, 0)?;
    }
    cost(layers, &defs, &mut memo, 0)?;
    Ok(())
}
pub fn document(doc: &Document) -> Result<()> {
    validate(&doc.symbols, &doc.layers)
}
