//! Reject hostile inputs before usvg's expansion or any image/font allocation.
use super::{ImportError, Result, cancelled, geometry::MAX_COORD, warn};
use photocraft_doc::vector_design::exchange::{ImportOptions, VectorWarning};
use std::{
    collections::{HashMap, HashSet},
    io::Read,
    str::FromStr,
    sync::atomic::AtomicBool,
};

pub(super) fn source(bytes: &[u8], options: &ImportOptions) -> Result<String> {
    if !options.dpi.is_finite() || !(1.0..=9600.0).contains(&options.dpi) {
        return Err(ImportError::Invalid("DPI must be finite and between 1 and 9600".into()));
    }
    if bytes.len() > options.limits.bytes {
        return Err(ImportError::Limit("input byte"));
    }
    let data = if bytes.starts_with(&[0x1f, 0x8b]) {
        let limit = options.limits.decompressed_bytes;
        let mut data = Vec::new();
        flate2::read::GzDecoder::new(bytes).take(limit.saturating_add(1)).read_to_end(&mut data).map_err(|e| ImportError::Invalid(e.to_string()))?;
        if data.len() as u64 > limit {
            return Err(ImportError::Limit("decompressed byte"));
        }
        data
    } else {
        if bytes.len() as u64 > options.limits.decompressed_bytes {
            return Err(ImportError::Limit("decompressed byte"));
        }
        bytes.to_vec()
    };
    String::from_utf8(data).map_err(|_| ImportError::Invalid("SVG must be UTF-8 XML".into()))
}

pub(super) fn xml<'a>(source: &'a str, options: &ImportOptions, cancel: &AtomicBool) -> Result<roxmltree::Document<'a>> {
    let limits = options.limits;
    let xml = roxmltree::Document::parse_with_options(
        source,
        roxmltree::ParsingOptions { allow_dtd: false, nodes_limit: limits.objects.saturating_mul(4).min(400_000) as u32 },
    )
    .map_err(|e| ImportError::Invalid(e.to_string()))?;
    let root = xml.root_element();
    if root.tag_name().name() != "svg" || root.tag_name().namespace() != Some("http://www.w3.org/2000/svg") {
        return Err(ImportError::Invalid("expected an SVG namespace root".into()));
    }
    let mut count = 0;
    let mut segments = 0;
    let mut segment_weights = HashMap::new();
    let mut embedded = 0u64;
    let mut ids = HashMap::new();
    let max_depth = limits.depth.min(photocraft_doc::MAX_GROUP_DEPTH).saturating_sub(2);
    for n in root.descendants() {
        cancelled(cancel)?;
        if n.ancestors().take(max_depth.saturating_add(1)).count() > max_depth {
            return Err(ImportError::Limit("XML depth"));
        }
        if !n.is_element() {
            continue;
        }
        count += 1;
        if count > limits.objects {
            return Err(ImportError::Limit("object"));
        }
        if let Some(id) = n.attribute("id")
            && ids.insert(id, n).is_some()
        {
            return Err(ImportError::Invalid("duplicate SVG id".into()));
        }
        for a in n.attributes() {
            match a.name() {
                "viewBox" => {
                    let v = svgtypes::ViewBox::from_str(a.value()).map_err(|e| ImportError::Invalid(e.to_string()))?;
                    if [v.x, v.y, v.w, v.h].iter().any(|v| !v.is_finite() || v.abs() > MAX_COORD) || v.w <= 0.0 || v.h <= 0.0 {
                        return Err(ImportError::Limit("viewBox"));
                    }
                }
                "transform" | "gradientTransform" | "patternTransform" => {
                    let t = svgtypes::Transform::from_str(a.value()).map_err(|e| ImportError::Invalid(e.to_string()))?;
                    if [t.a, t.b, t.c, t.d, t.e, t.f].iter().any(|v| !v.is_finite() || v.abs() > MAX_COORD) {
                        return Err(ImportError::Limit("transform"));
                    }
                }
                "d" => {
                    for p in svgtypes::PathParser::from(a.value()) {
                        let p = p.map_err(|e| ImportError::Invalid(e.to_string()))?;
                        let nums: Vec<f64> = match p {
                            svgtypes::PathSegment::MoveTo { x, y, .. }
                            | svgtypes::PathSegment::LineTo { x, y, .. }
                            | svgtypes::PathSegment::SmoothQuadratic { x, y, .. } => vec![x, y],
                            svgtypes::PathSegment::HorizontalLineTo { x, .. } => vec![x],
                            svgtypes::PathSegment::VerticalLineTo { y, .. } => vec![y],
                            svgtypes::PathSegment::CurveTo { x1, y1, x2, y2, x, y, .. } => vec![x1, y1, x2, y2, x, y],
                            svgtypes::PathSegment::SmoothCurveTo { x2, y2, x, y, .. } => vec![x2, y2, x, y],
                            svgtypes::PathSegment::Quadratic { x1, y1, x, y, .. } => vec![x1, y1, x, y],
                            svgtypes::PathSegment::EllipticalArc { rx, ry, x_axis_rotation, x, y, .. } => vec![rx, ry, x_axis_rotation, x, y],
                            svgtypes::PathSegment::ClosePath { .. } => vec![],
                        };
                        if nums.iter().any(|v| !v.is_finite() || v.abs() > MAX_COORD) {
                            return Err(ImportError::Limit("path coordinate"));
                        }
                        segments += 1;
                        *segment_weights.entry(n.id()).or_insert(0usize) += 4;
                        if segments > limits.segments {
                            return Err(ImportError::Limit("segment"));
                        }
                    }
                }
                "points" => {
                    for (x, y) in svgtypes::PointsParser::from(a.value()) {
                        if [x, y].iter().any(|v| !v.is_finite() || v.abs() > MAX_COORD) {
                            return Err(ImportError::Limit("point coordinate"));
                        }
                        segments += 1;
                        *segment_weights.entry(n.id()).or_insert(0usize) += 1;
                        if segments > limits.segments {
                            return Err(ImportError::Limit("segment"));
                        }
                    }
                    // Point lists include MoveTo; only polygons add ClosePath.
                    if n.tag_name().name() == "polygon" {
                        *segment_weights.entry(n.id()).or_insert(0usize) += 1;
                    }
                }
                "width" | "height" | "x" | "y" | "cx" | "cy" | "r" | "rx" | "ry" | "stroke-width" | "font-size" => {
                    if let Ok(l) = svgtypes::Length::from_str(a.value())
                        && (!l.number.is_finite() || l.number.abs() > MAX_COORD)
                    {
                        return Err(ImportError::Limit("length"));
                    }
                }
                "href" if a.value().starts_with("data:") => {
                    let (mime, payload) = a.value().split_once(',').ok_or_else(|| ImportError::Invalid("invalid data URL".into()))?;
                    // Bound BEFORE usvg's base64 decoder allocates. Percent encoded and
                    // compressed SVG data are not fed into recursive normalization.
                    if !mime.ends_with(";base64") {
                        return Err(ImportError::Invalid("only base64 image data URLs are supported".into()));
                    }
                    let padding = payload.bytes().rev().take_while(|b| *b == b'=').count().min(2) as u64;
                    let decoded = ((payload.len() as u64).saturating_add(3) / 4 * 3).saturating_sub(padding);
                    embedded = embedded.saturating_add(decoded);
                    if embedded > limits.decompressed_bytes {
                        return Err(ImportError::Limit("embedded data byte"));
                    }
                }
                _ => {}
            }
        }
    }
    // Count reference expansion before normalization; cycles and exponential <use> graphs
    // are errors, not silent missing art. Memoized costs keep the check linear in the DAG.
    fn cost<'a>(
        node: roxmltree::Node<'a, 'a>,
        ids: &HashMap<&'a str, roxmltree::Node<'a, 'a>>,
        active: &mut HashSet<roxmltree::NodeId>,
        memo: &mut HashMap<roxmltree::NodeId, usize>,
        cap: usize,
        depth: usize,
        weights: Option<&HashMap<roxmltree::NodeId, usize>>,
    ) -> Result<usize> {
        if depth > 64 {
            return Err(ImportError::Limit("reference depth"));
        }
        if let Some(v) = memo.get(&node.id()) {
            return Ok(*v);
        }
        if !active.insert(node.id()) {
            return Err(ImportError::Invalid("cyclic SVG reference".into()));
        }
        let mut total = weights.map_or(1, |w| w.get(&node.id()).copied().unwrap_or(0));
        let mut refs = Vec::new();
        for a in node.attributes() {
            if a.name() == "href"
                && let Some(id) = a.value().strip_prefix('#')
            {
                refs.push(id);
            }
            let mut rest = a.value();
            while let Some((_, tail)) = rest.split_once("url(") {
                let Some((id, tail)) = tail.split_once(')') else {
                    break;
                };
                if let Some(id) = id.trim().trim_matches(['\'', '"']).strip_prefix('#') {
                    refs.push(id);
                }
                rest = tail;
            }
        }
        for child in node.children().filter(|c| c.is_element()).chain(refs.into_iter().filter_map(|id| ids.get(id).copied())) {
            total = total.saturating_add(cost(child, ids, active, memo, cap, depth + 1, weights)?);
            if total > cap {
                return Err(ImportError::Limit(if weights.is_some() { "expanded segment" } else { "expanded object" }));
            }
        }
        active.remove(&node.id());
        memo.insert(node.id(), total);
        Ok(total)
    }
    cost(root, &ids, &mut HashSet::new(), &mut HashMap::new(), limits.objects, 0, None)?;
    cost(root, &ids, &mut HashSet::new(), &mut HashMap::new(), limits.segments, 0, Some(&segment_weights))?;
    Ok(xml)
}

pub(super) fn warnings(xml: &roxmltree::Document<'_>, warnings: &mut Vec<VectorWarning>) {
    for n in xml.descendants().filter(|n| n.is_element()) {
        let id = n.attribute("id").unwrap_or("");
        match n.tag_name().name() {
            "filter" => warn(warnings, "filter", id, "SVG filters are omitted; editable source art is retained."),
            "foreignObject" => warn(warnings, "foreignObject", id, "Foreign content is omitted."),
            "script" | "animate" | "animateTransform" | "set" => warn(warnings, "dynamic-content", id, "Scripts and animation are omitted."),
            _ => {}
        }
        if n.attributes()
            .any(|a| (a.name() == "shape-rendering" || a.name() == "style") && ["crispEdges", "optimizeSpeed"].iter().any(|v| a.value().contains(v)))
            || (n.tag_name().name() == "style" && n.text().is_some_and(|text| text.contains("crispEdges") || text.contains("optimizeSpeed")))
        {
            warn(warnings, "edge-rendering", id, "SVG edge-rendering hints use native antialiasing.");
        }
        for a in n.attributes().filter(|a| a.name() == "href") {
            if !a.value().starts_with(['#']) && !a.value().starts_with("data:") {
                warn(warnings, "external-resource", id, "Linked resources are omitted; no files or network resources were fetched.");
            } else if a.value().starts_with("data:")
                && !["data:image/png;", "data:image/jpeg;", "data:image/jpg;", "data:image/webp;"].iter().any(|p| a.value().starts_with(p))
            {
                warn(warnings, "embedded-format", id, "This embedded image format is omitted; nested SVG resources are not resolved.");
            }
        }
    }
}
