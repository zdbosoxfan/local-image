//! V1 constructive geometry commands, one journaled history step per gesture.
//! Paths use pc-doc data and pc-vector caches; the calculation kernel is pc-pathops.
//! Layer inputs are ordered by document stacking (bottom to top), independently of ID order.

use crate::{
    EngineError, Result, Session,
    commands::CommandSpec,
    vector_cmds::{bad, has_doc, refresh_shape},
};
use photocraft_doc::{Layer, LayerContent, LayerId, Path, PathOp};
use photocraft_pathops as ops;
use serde_json::{Value, json};

#[derive(Clone, Copy)]
enum Operation {
    Boolean,
    Pathfinder,
    Outline,
    Offset,
    Simplify,
    Smooth,
    Reverse,
    Join,
    Split,
    Finish,
}

fn selected(s: &Session, p: &Value, cmd: &str) -> Result<Vec<Layer>> {
    let st = s.active().ok_or(EngineError::NoDocument)?;
    let ids: Vec<LayerId> = match p.get("layers") {
        Some(v) => v
            .as_array()
            .ok_or_else(|| bad(cmd, "layers must be an array"))?
            .iter()
            .map(|v| v.as_u64().map(LayerId).ok_or_else(|| bad(cmd, "layer IDs must be unsigned integers")))
            .collect::<Result<_>>()?,
        None => st.selected_layers(),
    };
    if ids.is_empty() {
        return Err(bad(cmd, "select at least one shape layer"));
    }
    let mut positioned = Vec::new();
    for id in ids {
        if positioned.iter().any(|(_, l): &(Vec<usize>, Layer)| l.id == id) {
            return Err(bad(cmd, "duplicate layer ID"));
        }
        let layer = st.doc.layer(id).ok_or(EngineError::NoLayer(id))?;
        if !matches!(layer.content, LayerContent::Shape(_)) {
            return Err(bad(cmd, "all inputs must be shape layers"));
        }
        if st.doc.effective_locks(id).all {
            return Err(bad(cmd, format!("layer {} is locked", id.0)));
        }
        let at = st.doc.path_of(id).ok_or(EngineError::NoLayer(id))?;
        positioned.push((at, layer.clone()));
    }
    positioned.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(positioned.into_iter().map(|(_, l)| l).collect())
}
fn number(p: &Value, k: &str, default: f64, cmd: &str) -> Result<f64> {
    let v = match p.get(k) {
        Some(v) => v.as_f64().ok_or_else(|| bad(cmd, format!("{k} must be a number")))?,
        None => default,
    };
    if v.is_finite() { Ok(v) } else { Err(bad(cmd, format!("{k} must be finite"))) }
}
fn index(p: &Value, k: &str, cmd: &str) -> Result<usize> {
    match p.get(k) {
        Some(v) => v.as_u64().and_then(|v| usize::try_from(v).ok()).ok_or_else(|| bad(cmd, format!("{k} must be an index"))),
        None => Ok(0),
    }
}
fn bool_op(p: &Value, cmd: &str) -> Result<Option<ops::BoolOp>> {
    Ok(match p.get("op").and_then(Value::as_str).unwrap_or("add") {
        "add" => Some(ops::BoolOp::Union),
        "subtract" => Some(ops::BoolOp::Difference),
        "intersect" => Some(ops::BoolOp::Intersect),
        "xor" => Some(ops::BoolOp::Xor),
        "divide" => None,
        _ => return Err(bad(cmd, "op must be add|subtract|intersect|xor|divide")),
    })
}
fn join_option(p: &Value, cmd: &str) -> Result<ops::Join> {
    Ok(match p.get("join").and_then(Value::as_str).unwrap_or("miter") {
        "miter" => ops::Join::Miter,
        "round" => ops::Join::Round,
        "bevel" => ops::Join::Bevel,
        _ => return Err(bad(cmd, "join must be miter|round|bevel")),
    })
}
fn run(s: &mut Session, p: &Value, operation: Operation, cmd: &str, label: &str) -> Result<Value> {
    let input = selected(s, p, cmd)?;
    if matches!(operation, Operation::Outline) {
        return outline_layers(s, &input, label);
    }
    let paths: Vec<Path> = input
        .iter()
        .filter_map(|l| match &l.content {
            LayerContent::Shape(sh) => Some(sh.path.clone()),
            _ => None,
        })
        .collect();
    let error = |e: ops::PathOpsError| bad(cmd, e.to_string());
    let clip = s.active().ok_or(EngineError::NoDocument)?.doc.bounds();
    let clip = ops::geom::Rect::new(f64::from(clip.x0), f64::from(clip.y0), f64::from(clip.x1), f64::from(clip.y1));
    let baked = || paths.iter().map(|p| ops::finish_compound(p, Some(clip)).map_err(error)).collect::<Result<Vec<_>>>();
    let keep = match p.get("keep_compound") {
        Some(v) => v.as_bool().ok_or_else(|| bad(cmd, "keep_compound must be boolean"))?,
        None => false,
    };
    let mut results: Vec<(usize, Path)> = Vec::new();
    let combine = matches!(operation, Operation::Boolean | Operation::Pathfinder | Operation::Join);
    match operation {
        Operation::Boolean => {
            if paths.len() < 2 {
                return Err(bad(cmd, "boolean requires at least two shape layers"));
            }
            let op = bool_op(p, cmd)?;
            if keep && op.is_none() {
                return Err(bad(cmd, "divide cannot be a live compound"));
            }
            let paths = baked()?;
            if let Some(op) = op {
                let mut out = paths[0].clone();
                if keep && out.subpaths.is_empty() {
                    out.subpaths.push(photocraft_doc::Subpath::default());
                }
                for path in &paths[1..] {
                    if keep {
                        let operation = match op {
                            ops::BoolOp::Union => PathOp::Combine,
                            ops::BoolOp::Difference => PathOp::Subtract,
                            ops::BoolOp::Intersect => PathOp::Intersect,
                            ops::BoolOp::Xor => PathOp::Exclude,
                        };
                        if path.subpaths.is_empty() {
                            out.subpaths.push(photocraft_doc::Subpath { op: operation, ..Default::default() });
                        }
                        for (i, sp) in path.subpaths.iter().enumerate() {
                            let mut sp = sp.clone();
                            sp.op = if i == 0 { operation } else { PathOp::Join };
                            out.subpaths.push(sp);
                        }
                    } else {
                        out = ops::boolean(&out, path, op).map_err(error)?;
                    }
                }
                results.push((0, out));
            } else {
                let shapes: Vec<_> = paths.into_iter().enumerate().map(|(i, p)| ops::Shape::new(p, i as u64)).collect();
                results.extend(ops::pathfinder(ops::PathfinderOp::Divide, &shapes).map_err(error)?.into_iter().map(|s| (0, s.path)));
            }
        }
        Operation::Pathfinder => {
            if paths.len() < 2 {
                return Err(bad(cmd, "pathfinder requires at least two shape layers"));
            }
            let op = match p.get("op").and_then(Value::as_str).unwrap_or("trim") {
                "trim" => ops::PathfinderOp::Trim,
                "merge" => ops::PathfinderOp::Merge,
                "crop" => ops::PathfinderOp::Crop,
                "outline" => ops::PathfinderOp::Outline,
                "minusBack" => ops::PathfinderOp::MinusBack,
                _ => return Err(bad(cmd, "op must be trim|merge|crop|outline|minusBack")),
            };
            // Merge paint keys follow appearance equality. All results inherit the bottom layer's
            // appearance, as required by Local Image's Geometry contract.
            let mut keys = Vec::new();
            let shapes: Vec<_> = baked()?
                .into_iter()
                .enumerate()
                .map(|(i, path)| {
                    let sh = match &input[i].content {
                        LayerContent::Shape(s) => Some((&s.fill, &s.stroke)),
                        _ => None,
                    };
                    let key = keys.iter().position(|old| *old == sh).unwrap_or_else(|| {
                        keys.push(sh);
                        keys.len() - 1
                    });
                    ops::Shape::new(path, key as u64)
                })
                .collect();
            results.extend(ops::pathfinder(op, &shapes).map_err(error)?.into_iter().map(|s| (0, s.path)));
        }
        Operation::Join => {
            let tol = number(p, "tolerance", 0.5, cmd)?;
            results.push((0, ops::join(&paths, tol).map_err(error)?));
        }
        _ => {
            for (i, path) in paths.iter().enumerate() {
                let out = match operation {
                    Operation::Offset => {
                        let source = if path.inverted { ops::finish_compound(path, Some(clip)).map_err(error)? } else { path.clone() };
                        ops::offset_path(&source, number(p, "delta", 0.0, cmd)?, join_option(p, cmd)?, number(p, "miter", 4.0, cmd)?)
                    }
                    Operation::Simplify => ops::simplify(path, number(p, "tolerance", 0.5, cmd)?),
                    Operation::Smooth => ops::smooth(path, number(p, "amount", 0.5, cmd)?),
                    Operation::Reverse => ops::reverse(path),
                    Operation::Finish => ops::finish_compound(path, Some(clip)),
                    Operation::Split => {
                        let split = ops::split_at(path, index(p, "subpath", cmd)?, index(p, "segment", cmd)?, number(p, "t", 0.5, cmd)?).map_err(error)?;
                        results.extend(split.into_iter().map(|p| (i, p)));
                        continue;
                    }
                    _ => return Err(bad(cmd, "invalid operation")),
                }
                .map_err(error)?;
                results.push((i, out));
            }
        }
    }
    // All options and geometry have succeeded before the document is mutated.
    let ids = s.edit(label, |doc, active| {
        let snapshot = doc.clone();
        let mut first_for = std::collections::BTreeSet::new();
        let mut ids = Vec::new();
        let mut last_for = std::collections::BTreeMap::new();
        for (source, path) in results {
            let template = &input[source];
            let mut layer = template.clone();
            let LayerContent::Shape(sh) = &mut layer.content else { return Err(bad(cmd, "shape missing")) };
            sh.path = path;
            sh.live = None;
            sh.psd_raw = None;
            if matches!(operation, Operation::Pathfinder) && p.get("op").and_then(Value::as_str) == Some("outline") {
                let paint = sh.fill.clone();
                sh.fill = None;
                if sh.stroke.is_none() {
                    sh.stroke = Some(photocraft_doc::ShapeStroke {
                        width: 1.0,
                        paint: paint.unwrap_or(photocraft_doc::Fill::Solid(photocraft_color::Color::BLACK)),
                        ..Default::default()
                    });
                }
            }
            layer.psd_blocks.retain(|(k, _)| ![b"vmsk", b"vsms", b"vogk", b"vstk", b"vscg", b"SoCo", b"GdFl", b"PtFl"].contains(&k));
            refresh_shape(&snapshot, sh);
            if first_for.insert(source) {
                let id = template.id;
                *doc.layer_mut(id).ok_or(EngineError::NoLayer(id))? = layer;
                ids.push(id);
                last_for.insert(source, id);
            } else {
                layer.id = LayerId::fresh();
                let id = doc.insert_above(last_for.get(&source).copied(), layer);
                ids.push(id);
                last_for.insert(source, id);
            }
        }
        for (i, layer) in input.iter().enumerate() {
            if (combine && i != 0) || !first_for.contains(&i) {
                doc.remove(layer.id);
            }
        }
        *active = ids.first().copied();
        Ok(ids)
    })?;
    if let Some(st) = s.active_mut() {
        st.selected_layers = ids.clone();
        st.layer_anchor = ids.first().copied();
        st.history.set_current_layers(photocraft_ops::LayerTarget { active: st.active_layer, selected: ids.clone() });
    }
    Ok(json!({"layers":ids.iter().map(|id|id.0).collect::<Vec<_>>()}))
}

/// Keep the original fill below its expanded stroke. An isolated group retains the original
/// layer opacity, masks and effects once, rather than applying them to each paint separately.
fn outline_layers(s: &mut Session, input: &[Layer], label: &str) -> Result<Value> {
    const CMD: &str = "path.outlineStroke";
    let doc = s.active().ok_or(EngineError::NoDocument)?.doc.clone();
    let mut plans = Vec::new();
    for original in input {
        let LayerContent::Shape(shape) = &original.content else { return Err(bad(CMD, "shape missing")) };
        let stroke = shape.stroke.as_ref().ok_or_else(|| bad(CMD, "outlineStroke requires a stroke on every input"))?;
        let path = ops::outline_stroke(&shape.path, stroke, photocraft_vector::DEFAULT_TOLERANCE).map_err(|e| bad(CMD, e.to_string()))?;
        let mut outlined = photocraft_doc::ShapeLayer { path, fill: Some(stroke.paint.clone()), ..Default::default() };
        refresh_shape(&doc, &mut outlined);
        let mut layer = original.clone();
        layer.psd_blocks.retain(|(k, _)| ![b"vmsk", b"vsms", b"vogk", b"vstk", b"vscg", b"SoCo", b"GdFl", b"PtFl"].contains(&k));
        let ids = if shape.fill.is_some() {
            let mut fill = shape.clone();
            fill.stroke = None;
            fill.live = None;
            fill.psd_raw = None;
            refresh_shape(&doc, &mut fill);
            let base = Layer::new(original.name.clone(), LayerContent::Shape(fill));
            let mut edge = Layer::new(original.name.clone(), LayerContent::Shape(outlined));
            edge.opacity = stroke.opacity;
            let ids = vec![base.id, edge.id];
            layer.content = LayerContent::Group(photocraft_doc::Group { children: vec![base, edge], expanded: true, artboard: None });
            ids
        } else {
            layer.content = LayerContent::Shape(outlined);
            layer.opacity *= stroke.opacity;
            vec![layer.id]
        };
        plans.push((layer, ids));
    }
    let ids = s.edit(label, |doc, active| {
        let mut ids = Vec::new();
        for (layer, outputs) in plans {
            let id = layer.id;
            *doc.layer_mut(id).ok_or(EngineError::NoLayer(id))? = layer;
            ids.extend(outputs);
        }
        *active = ids.first().copied();
        Ok(ids)
    })?;
    if let Some(st) = s.active_mut() {
        st.selected_layers = ids.clone();
        st.layer_anchor = ids.first().copied();
        st.history.set_current_layers(photocraft_ops::LayerTarget { active: st.active_layer, selected: ids.clone() });
    }
    Ok(json!({"layers":ids.iter().map(|id|id.0).collect::<Vec<_>>()}))
}

pub fn specs() -> Vec<CommandSpec> {
    type Run = fn(&mut Session, &Value) -> Result<Value>;
    let commands: [(&str, &str, &str, Run); 10] = [
        ("path.boolean", "Boolean Paths", r#"{layers:[ids]?,op:add|subtract|intersect|xor|divide,keep_compound:false} → {layers:[ids]}"#, |s, p| {
            run(s, p, Operation::Boolean, "path.boolean", "Boolean Paths")
        }),
        ("path.pathfinder", "Pathfinder", r#"{layers:[ids]?,op:trim|merge|crop|outline|minusBack} → {layers:[ids]}"#, |s, p| {
            run(s, p, Operation::Pathfinder, "path.pathfinder", "Pathfinder")
        }),
        ("path.outlineStroke", "Outline Stroke", r#"{layers:[ids]?} → {layers:[ids]}"#, |s, p| {
            run(s, p, Operation::Outline, "path.outlineStroke", "Outline Stroke")
        }),
        ("path.offset", "Offset Path", r#"{layers:[ids]?,delta:px,join:miter|round|bevel,miter:4} → {layers:[ids]}"#, |s, p| {
            run(s, p, Operation::Offset, "path.offset", "Offset Path")
        }),
        ("path.simplify", "Simplify Paths", r#"{layers:[ids]?,tolerance:0.5} → {layers:[ids]}"#, |s, p| {
            run(s, p, Operation::Simplify, "path.simplify", "Simplify Paths")
        }),
        ("path.smooth", "Smooth Paths", r#"{layers:[ids]?,amount:0..1=0.5} → {layers:[ids]}"#, |s, p| {
            run(s, p, Operation::Smooth, "path.smooth", "Smooth Paths")
        }),
        ("path.reverse", "Reverse Paths", r#"{layers:[ids]?} → {layers:[ids]}"#, |s, p| run(s, p, Operation::Reverse, "path.reverse", "Reverse Paths")),
        ("path.join", "Join Paths", r#"{layers:[ids]?,tolerance:0.5} → {layers:[ids]}"#, |s, p| run(s, p, Operation::Join, "path.join", "Join Paths")),
        ("path.splitAt", "Split Path", r#"{layers:[ids]?,subpath:0,segment:0,t:0..1=0.5} → {layers:[ids]}"#, |s, p| {
            run(s, p, Operation::Split, "path.splitAt", "Split Path")
        }),
        ("path.finishCompound", "Finish Compound", r#"{layers:[ids]?} → {layers:[ids]}"#, |s, p| {
            run(s, p, Operation::Finish, "path.finishCompound", "Finish Compound")
        }),
    ];
    commands
        .into_iter()
        .map(|(id, label, params, run)| CommandSpec { id, label, params, run, menu: &[], shortcut: None, enabled: has_doc, journal: true })
        .collect()
}
