//! Live blend evaluation adapted from VectorCraft `crates/doc/src/blend.rs` and
//! `live.rs` at d522c1d7be4035bd4f4a84cd6ebfca44f5155092.
//! Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.
//! SPDX-License-Identifier: MIT OR Apache-2.0
//! See licenses/vectorcraft-{LICENSE-MIT,LICENSE-APACHE,NOTICE}.
//!
//! Only ordinary layers are projected. The persisted group retains its ordered editable
//! keys; generated layers and their deterministic IDs never enter the source document.
use photocraft_doc::vector_design::live::{BlendOrientation, BlendSpacing, BlendSpec, LiveGroup};
use photocraft_doc::{Document, Layer, LayerContent, LayerId, Path};
use photocraft_geom::{Affine, Point};

#[path = "blend/correspondence.rs"]
pub mod correspondence;
#[path = "blend/paint.rs"]
pub mod paint;
#[path = "blend/spine.rs"]
pub mod spine;

pub const MAX_KEYS: usize = 32;
pub const MAX_STEPS: u32 = 1000;
pub const MAX_LAYERS: usize = 4096;
pub const MAX_KNOTS: usize = 4096;
pub const MAX_PROJECTED_KNOTS: usize = 1_048_576;
pub const MAX_COORDINATE: f64 = 1_000_000.;
pub const MAX_CACHE_BYTES: u128 = 256 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct BlendError(pub String);
pub type Result<T> = std::result::Result<T, BlendError>;
pub fn error(message: impl Into<String>) -> BlendError {
    BlendError(message.into())
}
pub fn validate_path(path: &Path) -> Result<()> {
    photocraft_pathops::validate(path).map_err(|e| error(e.to_string()))?;
    if path.subpaths.len() > 64 || path.subpaths.iter().map(|s| s.knots.len()).sum::<usize>() > MAX_KNOTS {
        return Err(error("blend path exceeds subpath or knot budget"));
    }
    if path.subpaths.iter().any(|s| s.knots.is_empty())
        || path
            .subpaths
            .iter()
            .flat_map(|s| &s.knots)
            .flat_map(|k| [k.anchor, k.in_ctrl, k.out_ctrl])
            .any(|p| p.x.abs() > MAX_COORDINATE || p.y.abs() > MAX_COORDINATE)
    {
        return Err(error("blend path has empty subpaths or out-of-range coordinates"));
    }
    Ok(())
}
pub fn validate_spec(spec: &BlendSpec) -> Result<()> {
    match spec.spacing {
        BlendSpacing::Steps(n) if n > MAX_STEPS => return Err(error("blend steps must be between 0 and 1000")),
        BlendSpacing::Distance(d) if !d.is_finite() || !(0.01..=MAX_COORDINATE).contains(&d) => {
            return Err(error("blend distance must be finite and between 0.01 and 1000000"));
        }
        _ => {}
    }
    if let Some(path) = &spec.spine {
        validate_path(path)?;
        if path.subpaths.len() != 1 || path.subpaths[0].knots.len() < 2 || path.subpaths[0].knots.len() > 128 || path.inverted {
            return Err(error("blend spine requires one non-inverted path with 2 to 128 knots"));
        }
        spine::Spine::new(path)?;
    }
    Ok(())
}
pub fn center(layer: &Layer) -> Result<Point> {
    let b = match &layer.content {
        LayerContent::Shape(s) => photocraft_pathops::bounds(&s.path).map_err(|e| error(e.to_string()))?,
        LayerContent::Group(g) => g.children.iter().map(center_bounds).collect::<Result<Vec<_>>>()?.into_iter().reduce(union),
        _ => return Err(error("blend keys must be curves or groups of curves")),
    }
    .ok_or_else(|| error("blend key has no geometry"))?;
    Ok(Point::new((b.0 + b.2) * 0.5, (b.1 + b.3) * 0.5))
}
fn center_bounds(layer: &Layer) -> Result<(f64, f64, f64, f64)> {
    match &layer.content {
        LayerContent::Shape(s) => photocraft_pathops::bounds(&s.path).map_err(|e| error(e.to_string()))?.ok_or_else(|| error("empty blend curve")),
        LayerContent::Group(g) => {
            g.children.iter().map(center_bounds).collect::<Result<Vec<_>>>()?.into_iter().reduce(union).ok_or_else(|| error("empty blend group"))
        }
        _ => Err(error("blend keys must be curves or groups of curves")),
    }
}
fn union(a: (f64, f64, f64, f64), b: (f64, f64, f64, f64)) -> (f64, f64, f64, f64) {
    (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3))
}
fn validate_layer(layer: &Layer, depth: usize, layers: &mut usize, knots: &mut usize) -> Result<()> {
    *layers += 1;
    if *layers > MAX_LAYERS || depth > 16 {
        return Err(error("blend key hierarchy exceeds budget"));
    }
    if !layer.opacity.is_finite() || !(0. ..=1.).contains(&layer.opacity) || !layer.fill_opacity.is_finite() || !(0. ..=1.).contains(&layer.fill_opacity) {
        return Err(error("blend key opacity must be finite and bounded"));
    }
    if layer.vector.live.is_some() || !layer.vector.corners.is_empty() || layer.vector.clip.is_some() || layer.mask.is_some() || layer.vector_mask.is_some() {
        return Err(error("blend keys require projected corners/clips/masks/live geometry"));
    }
    match &layer.content {
        LayerContent::Shape(s) => {
            validate_path(&s.path)?;
            if s.path.is_empty() {
                return Err(error("empty blend curve"));
            }
            *knots += s.path.subpaths.iter().map(|s| s.knots.len()).sum::<usize>();
            let a = layer.vector.appearance.clone().unwrap_or_else(|| photocraft_vector::appearance::legacy_appearance(s));
            photocraft_vector::appearance::validate(&a).map_err(error)?;
        }
        LayerContent::Group(g) if !g.children.is_empty() && g.artboard.is_none() => {
            for child in &g.children {
                validate_layer(child, depth + 1, layers, knots)?;
            }
        }
        _ => return Err(error("blend keys must be nonempty curves or ordinary groups of curves")),
    }
    if *knots > MAX_PROJECTED_KNOTS {
        return Err(error("blend geometry exceeds aggregate knot budget"));
    }
    Ok(())
}
pub fn validate_keys(keys: &[Layer]) -> Result<()> {
    if !(2..=MAX_KEYS).contains(&keys.len()) {
        return Err(error("blend requires 2 to 32 ordered keys"));
    }
    let (mut layers, mut knots) = (0, 0);
    for key in keys {
        validate_layer(key, 0, &mut layers, &mut knots)?;
    }
    Ok(())
}
pub fn default_spine(keys: &[Layer]) -> Result<Path> {
    validate_keys(keys)?;
    let points = keys.iter().map(center).collect::<Result<Vec<_>>>()?;
    Ok(Path::new(vec![photocraft_doc::Subpath::polyline(&points.iter().map(|p| (p.x, p.y)).collect::<Vec<_>>())]))
}
pub fn step_count(a: &Layer, b: &Layer, spacing: BlendSpacing, length: f64) -> Result<usize> {
    validate_spec(&BlendSpec { spacing, ..Default::default() })?;
    if !length.is_finite() || length < 0. {
        return Err(error("invalid blend span length"));
    }
    Ok(match spacing {
        BlendSpacing::Steps(n) => n as usize,
        // Match the upstream closest spacing convention, including both endpoint keys.
        BlendSpacing::Distance(d) => ((length / d).round() - 1.).clamp(0., f64::from(MAX_STEPS)) as usize,
        BlendSpacing::SmoothColor => {
            let delta = paint::distance(a, b);
            if delta > 1. / 255. { (delta * 255. / 2.).ceil().clamp(1., 256.) as usize } else { (length / 2.).ceil().clamp(1., 256.) as usize }
        }
    })
}
#[derive(Clone, Debug)]
enum Pair {
    Curve { a: Layer, b: Layer, path: correspondence::PathPair },
    Group { a: Layer, b: Layer, children: Vec<Pair> },
}
impl Pair {
    fn new(a: &Layer, b: &Layer) -> Result<Self> {
        match (&a.content, &b.content) {
            (LayerContent::Shape(x), LayerContent::Shape(y)) => {
                Ok(Self::Curve { a: a.clone(), b: b.clone(), path: correspondence::PathPair::new(&x.path, &y.path)? })
            }
            (LayerContent::Group(x), LayerContent::Group(y)) => {
                let (ca, cb) = (center(a)?, center(b)?);
                let mut children = Vec::new();
                for i in 0..x.children.len().max(y.children.len()) {
                    children.push(match (x.children.get(i), y.children.get(i)) {
                        (Some(a), Some(b)) => Self::new(a, b)?,
                        (Some(a), None) => Self::new(a, &collapsed(a, cb))?,
                        (None, Some(b)) => Self::new(&collapsed(b, ca), b)?,
                        _ => continue,
                    });
                }
                Ok(Self::Group { a: a.clone(), b: b.clone(), children })
            }
            _ => Err(error("blend key pair must have matching curve/group structure")),
        }
    }
    fn at(&self, t: f64) -> Result<Layer> {
        let (a, b) = match self {
            Self::Curve { a, b, .. } | Self::Group { a, b, .. } => (a, b),
        };
        let mut out = if t < 0.5 { a.clone() } else { b.clone() };
        clear_generated(&mut out);
        out.opacity = lerp(a.opacity, b.opacity, t);
        out.fill_opacity = lerp(a.fill_opacity, b.fill_opacity, t);
        match self {
            Self::Curve { path, .. } => {
                let appearance = paint::appearance(a, b, t)?;
                let LayerContent::Shape(shape) = &mut out.content else { return Err(error("expected blend curve")) };
                shape.path = path.at(t)?;
                shape.live = None;
                shape.psd_raw = None;
                shape.cache = None;
                photocraft_vector::appearance::mirror_legacy(shape, &appearance);
                // Legacy keys retain legacy clipping/split semantics; explicit stacks stay explicit.
                out.vector.appearance = (a.vector.appearance.is_some() || b.vector.appearance.is_some()).then_some(appearance);
            }
            Self::Group { children, .. } => {
                if let LayerContent::Group(group) = &mut out.content {
                    group.children = children.iter().map(|p| p.at(t)).collect::<Result<_>>()?;
                }
            }
        }
        Ok(out)
    }
}
fn lerp(a: f32, b: f32, t: f64) -> f32 {
    (f64::from(a) + (f64::from(b) - f64::from(a)) * t) as f32
}
fn clear_generated(layer: &mut Layer) {
    layer.id = LayerId(0);
    layer.name = "Blend step".into();
    layer.locks = Default::default();
    layer.psd_id = None;
    layer.psd_blocks.clear();
    layer.link_group = None;
    layer.fill_cache = None;
    layer.vector.live_shape = None;
    layer.vector.corners.clear();
    layer.vector.live = None;
}
fn collapsed(layer: &Layer, p: Point) -> Layer {
    let mut out = layer.clone();
    // Shrink geometry exactly; a missing object grows from the opposite group centre.
    transform(&mut out, Affine { m: [0., 0., 0., 0., p.x, p.y] });
    out
}
fn transform(layer: &mut Layer, matrix: Affine) {
    match &mut layer.content {
        LayerContent::Shape(s) => {
            s.path = s.path.transform(&matrix);
            s.cache = None;
            s.live = None;
            s.psd_raw = None;
            layer.vector.live_shape = None;
        }
        LayerContent::Group(g) => {
            for child in &mut g.children {
                transform(child, matrix);
            }
        }
        _ => {}
    }
}
/// Evaluate keys and intermediate steps in bottom-to-top order. Returns detached geometry
/// without raster allocation. Explicit spine moves the projected keys, never their sources.
pub fn evaluate(keys: &[Layer], spec: &BlendSpec) -> Result<Vec<Layer>> {
    validate_keys(keys)?;
    validate_spec(spec)?;
    let rail = spec.spine.as_ref().map(spine::Spine::new).transpose()?;
    let centers = keys.iter().map(center).collect::<Result<Vec<_>>>()?;
    let mut counts = Vec::new();
    for (i, pair) in keys.windows(2).enumerate() {
        let length =
            rail.as_ref().map_or_else(|| (centers[i + 1].x - centers[i].x).hypot(centers[i + 1].y - centers[i].y), |s| s.length() / (keys.len() - 1) as f64);
        counts.push(step_count(&pair[0], &pair[1], spec.spacing, length)?);
    }
    let total = keys.len() + counts.iter().sum::<usize>();
    if total > MAX_LAYERS {
        return Err(error("blend expansion exceeds layer budget"));
    }
    let mut out = Vec::with_capacity(total);
    let (mut layers, mut knots) = (0, 0);
    for (i, key) in keys.iter().enumerate() {
        let mut projected = key.clone();
        place(&mut projected, &centers, i, 0., rail.as_ref(), spec.orientation)?;
        validate_layer(&projected, 0, &mut layers, &mut knots)?;
        out.push(projected);
        let Some(next) = keys.get(i + 1) else { break };
        if counts[i] == 0 {
            continue;
        }
        let pair = Pair::new(key, next)?;
        for j in 1..=counts[i] {
            let t = j as f64 / (counts[i] + 1) as f64;
            let mut step = pair.at(t)?;
            place(&mut step, &centers, i, t, rail.as_ref(), spec.orientation)?;
            validate_layer(&step, 0, &mut layers, &mut knots)?;
            out.push(step);
        }
    }
    Ok(out)
}
fn place(layer: &mut Layer, centers: &[Point], i: usize, t: f64, rail: Option<&spine::Spine>, orientation: BlendOrientation) -> Result<()> {
    let c0 = centers[i];
    let c1 = centers.get(i + 1).copied().unwrap_or(c0);
    let base = Point::new(c0.x + (c1.x - c0.x) * t, c0.y + (c1.y - c0.y) * t);
    let (point, angle) = if let Some(s) = rail {
        s.at((i as f64 + t) / (centers.len() - 1) as f64)?
    } else {
        let start = if i + 1 == centers.len() { centers[i.saturating_sub(1)] } else { c0 };
        (base, (c1.y - start.y).atan2(c1.x - start.x))
    };
    let off = Affine::translate(point.x - base.x, point.y - base.y);
    let matrix = if orientation == BlendOrientation::AlignToPath {
        let c = center(layer)?;
        let c = off.apply(c);
        Affine::translate(c.x, c.y).mul(&Affine::rotate(angle)).mul(&Affine::translate(-c.x, -c.y)).mul(&off)
    } else {
        off
    };
    if matrix != Affine::IDENTITY {
        transform(layer, matrix);
    }
    Ok(())
}
/// Shared display/export projection. Generated IDs are deterministic and cannot collide with
/// source IDs. Bounds-limited caches are prepared in two passes before any document publishes.
pub fn max_document_id(doc: &Document) -> u64 {
    let mut max = doc.id.0;
    let mut stack: Vec<_> = doc.layers.iter().collect();
    for symbol in &doc.symbols {
        max = max.max(symbol.id.0);
        stack.extend(&symbol.layers);
    }
    while let Some(layer) = stack.pop() {
        max = max.max(layer.id.0);
        if let Some(children) = layer.children() {
            stack.extend(children);
        }
    }
    max
}

pub fn project_document(source: &Document) -> Result<Document> {
    let mut doc = source.clone();
    let mut next = max_document_id(source);
    let (mut layers, mut knots) = (0, 0);
    fn walk(input: &mut [Layer], next: &mut u64, depth: usize, layers: &mut usize, knots: &mut usize) -> Result<()> {
        if depth > 16 {
            return Err(error("live blend document exceeds depth budget"));
        }
        for layer in input {
            if let Some(LiveGroup::Blend(spec)) = layer.vector.live.clone() {
                let LayerContent::Group(group) = &mut layer.content else { return Err(error("live blend must be on a group")) };
                group.children = evaluate(&group.children, &spec)?;
                layer.vector.live = None;
            }
            *layers += 1;
            if *layers > MAX_LAYERS {
                return Err(error("blend document projection exceeds layer budget"));
            }
            if layer.id.0 == 0 {
                *next = next.checked_add(1).ok_or_else(|| error("blend projection ID overflow"))?;
                layer.id = LayerId(*next);
            }
            if let LayerContent::Shape(s) = &layer.content {
                *knots += s.path.subpaths.iter().map(|s| s.knots.len()).sum::<usize>();
                if *knots > MAX_PROJECTED_KNOTS {
                    return Err(error("blend document exceeds geometry budget"));
                }
            }
            if let Some(children) = layer.children_mut() {
                walk(children, next, depth + 1, layers, knots)?;
            }
        }
        Ok(())
    }
    // ID uniqueness is already guaranteed by the document. All generated IDs are > max.
    walk(&mut doc.layers, &mut next, 0, &mut layers, &mut knots)?;
    prepare_caches(&mut doc)?;
    Ok(doc)
}
/// Rasterize only missing/transformed/generated curve caches, after checking the aggregate
/// allocation budget. Unchanged raster/type/effect layers remain ordinary compositor inputs.
pub fn prepare_caches(doc: &mut Document) -> Result<()> {
    if doc.max_group_depth() > photocraft_doc::MAX_GROUP_DEPTH {
        return Err(error("blend cache document exceeds depth budget"));
    }
    let mut bytes = 0u128;
    for (_, _, layer) in doc.walk() {
        if let LayerContent::Shape(shape) = &layer.content
            && shape.cache.is_none()
        {
            validate_path(&shape.path)?;
            let compiled = photocraft_vector::appearance::CompiledAppearance::with_projection(
                shape,
                layer.vector.appearance.as_ref(),
                &doc.patterns,
                doc.bounds(),
                &crate::Projection,
            )
            .map_err(error)?;
            bytes += compiled.cache_bytes(doc.pixel_format());
            if bytes > MAX_CACHE_BYTES {
                return Err(error("blend caches exceed 256 MiB budget"));
            }
        }
    }
    let (format, bounds, patterns) = (doc.pixel_format(), doc.bounds(), doc.patterns.clone());
    fn render(
        layers: &mut [Layer],
        format: photocraft_color::PixelFormat,
        bounds: photocraft_geom::Rect,
        patterns: &[photocraft_doc::Pattern],
        depth: usize,
    ) -> Result<()> {
        if depth > photocraft_doc::MAX_GROUP_DEPTH {
            return Err(error("blend cache document exceeds depth budget"));
        }
        for layer in layers {
            if let LayerContent::Shape(shape) = &mut layer.content
                && shape.cache.is_none()
            {
                shape.cache = Some(
                    photocraft_vector::appearance::CompiledAppearance::with_projection(
                        shape,
                        layer.vector.appearance.as_ref(),
                        patterns,
                        bounds,
                        &crate::Projection,
                    )
                    .and_then(|plan| plan.render(format, bounds))
                    .map_err(error)?,
                );
            }
            if let Some(children) = layer.children_mut() {
                render(children, format, bounds, patterns, depth + 1)?;
            }
        }
        Ok(())
    }
    render(&mut doc.layers, format, bounds, &patterns, 0)
}
/// No engine/UI dependency: format adapters project the exchange DTO with this same evaluator.
pub fn project_scene(
    scene: &photocraft_doc::vector_design::exchange::ImportedScene,
    format: photocraft_color::PixelFormat,
    patterns: &[photocraft_doc::Pattern],
) -> Result<photocraft_doc::vector_design::exchange::ImportedScene> {
    let mut out = scene.clone();
    let mut doc = Document::new("Blend projection", scene.size, format.mode, format.sample);
    // The exchange DTO has no document identity; temporary global IDs must not change
    // deterministic generated layer IDs on repeated exports of the same scene.
    doc.id = photocraft_doc::DocId(0);
    doc.layers = scene.layers.clone();
    doc.symbols = scene.symbols.clone();
    doc.resolution_dpi = scene.resolution_dpi;
    doc.design_units = scene.units;
    doc.patterns = patterns.to_vec();
    out.layers = project_document(&doc)?.layers;
    Ok(out)
}
