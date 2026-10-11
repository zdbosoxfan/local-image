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
    let mut observed_depth = 0usize;
    let mut class_terms = 1usize;
    let mut count = 0;
    let mut segments = 0;
    let mut segment_weights = HashMap::new();
    let mut embedded = 0u64;
    let mut ids = HashMap::new();
    let max_depth = limits.depth.min(photocraft_doc::MAX_GROUP_DEPTH).saturating_sub(2);
    for n in root.descendants() {
        cancelled(cancel)?;
        let node_depth = n.ancestors().take(max_depth.saturating_add(1)).count();
        observed_depth = observed_depth.max(node_depth);
        if node_depth > max_depth {
            return Err(ImportError::Limit("XML depth"));
        }
        if !n.is_element() {
            continue;
        }
        class_terms = class_terms.max(n.attribute("class").map_or(0, |c| c.split_whitespace().count()));
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
            if (a.name().starts_with("marker") && a.value().len() > 1024) || (a.name() == "style" && a.value().len() > 16384) {
                return Err(ImportError::Limit("CSS declaration byte"));
            }
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
    // simplecss (used by usvg) backtracks descendant selectors. Admission limits
    // account for that parser too, before either style cascade is allowed to run.
    let mut css_work = 0usize;
    let mut rules = 0usize;
    for style in root.descendants().filter(|n| n.is_element() && n.tag_name().name() == "style") {
        let css: String = style.children().filter_map(|n| n.text()).collect();
        for block in css.split('}') {
            let Some((head, body)) = block.split_once('{') else { continue };
            if body.len() > 16384 || body.split(';').count() > 128 {
                return Err(ImportError::Limit("CSS declaration byte"));
            }
            for decl in body.split(';') {
                if let Some((key, value)) = decl.split_once(':')
                    && key.trim().to_ascii_lowercase().starts_with("marker")
                    && value.len() > 1024
                {
                    return Err(ImportError::Limit("CSS declaration byte"));
                }
            }
            for selector in head.split(',') {
                cancelled(cancel)?;
                if body.to_ascii_lowercase().contains("marker") && !super::css::Styles::marker_selector_supported(selector.trim()) {
                    return Err(ImportError::Limit("CSS marker selector"));
                }
                rules += 1;
                let compounds = selector.split_whitespace().filter(|s| !matches!(*s, ">" | "+" | "~")).count().max(1);
                // Combinators without surrounding whitespace must also be charged.
                let compounds = compounds.max(selector.bytes().filter(|b| matches!(b, b'>' | b'+' | b'~')).count() + 1);
                if compounds > 8 || selector.len() > 1024 || rules > 256 {
                    return Err(ImportError::Limit("CSS selector complexity"));
                }
                let class_work = if selector.contains('.') { class_terms.saturating_mul(selector.bytes().filter(|b| *b == b'.').count()) } else { 1 };
                css_work =
                    css_work.saturating_add(count.saturating_mul(observed_depth.saturating_pow(compounds.saturating_sub(1) as u32)).saturating_mul(class_work));
                if css_work > 2_000_000 {
                    return Err(ImportError::Limit("CSS matching work"));
                }
            }
        }
    }
    let styles = super::css::Styles::markers(&xml);
    let mut marker_refs = HashMap::new();
    for node in root
        .descendants()
        .filter(|n| n.is_element() && matches!(n.tag_name().name(), "path" | "line" | "rect" | "circle" | "ellipse" | "polygon" | "polyline" | "use"))
    {
        cancelled(cancel)?;
        let mut refs = Vec::new();
        for key in ["marker", "marker-start", "marker-mid", "marker-end"] {
            if let Some(value) = styles.prop(node, key) {
                if value.len() > 1024 {
                    return Err(ImportError::Limit("CSS declaration byte"));
                }
                if let Some((_, tail)) = value.split_once("url(")
                    && let Some((id, _)) = tail.split_once(')')
                    && let Some(id) = id.trim().trim_matches(['\'', '"']).strip_prefix('#')
                    && let Some(marker) = ids.get(id)
                    && marker.tag_name().name() == "marker"
                    // usvg suppresses a marker while converting its own artwork.
                    && !node.ancestors().any(|owner| owner.is_element() && owner.tag_name().name() == "marker" && owner.id() == marker.id())
                {
                    refs.push(*marker);
                }
            }
        }
        if !refs.is_empty() {
            marker_refs.insert(node.id(), refs);
        }
    }
    drop(styles);
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
        markers: Option<&HashMap<roxmltree::NodeId, Vec<roxmltree::Node<'a, 'a>>>>,
        vertices: &HashMap<roxmltree::NodeId, usize>,
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
            total = total.saturating_add(cost(child, ids, active, memo, cap, depth + 1, weights, markers, vertices)?);
            if total > cap {
                return Err(ImportError::Limit(if weights.is_some() { "expanded segment" } else { "expanded object" }));
            }
        }
        if let Some(targets) = markers.and_then(|m| m.get(&node.id())) {
            let vertex_count = if node.tag_name().name() == "use" {
                cost(node, ids, &mut HashSet::new(), &mut HashMap::new(), usize::MAX, 0, Some(vertices), None, vertices)?.saturating_add(4)
            } else {
                vertices.get(&node.id()).copied().unwrap_or(0).saturating_add(4)
            };
            for marker in targets {
                total = total.saturating_add(
                    cost(*marker, ids, active, memo, cap, depth + 1, weights, markers, vertices)?.saturating_mul(vertex_count).saturating_mul(3),
                );
                if total > cap {
                    return Err(ImportError::Limit("expanded marker"));
                }
            }
        }
        active.remove(&node.id());
        memo.insert(node.id(), total);
        Ok(total)
    }
    cost(root, &ids, &mut HashSet::new(), &mut HashMap::new(), limits.objects, 0, None, None, &segment_weights)?;
    cost(root, &ids, &mut HashSet::new(), &mut HashMap::new(), limits.segments, 0, Some(&segment_weights), None, &segment_weights)?;
    // usvg converts marker art for each path vertex. Charge all marker definitions
    // conservatively for all vertices, including inherited and CSS marker declarations.
    let mut marker_objects = 0usize;
    let mut marker_segments = 0usize;
    let mut object_memo = HashMap::new();
    let mut segment_memo = HashMap::new();
    for marker in root.descendants().filter(|n| n.is_element() && n.tag_name().name() == "marker") {
        marker_objects = marker_objects.saturating_add(cost(
            marker,
            &ids,
            &mut HashSet::new(),
            &mut object_memo,
            limits.objects,
            0,
            None,
            Some(&marker_refs),
            &segment_weights,
        )?);
        marker_segments = marker_segments.saturating_add(cost(
            marker,
            &ids,
            &mut HashSet::new(),
            &mut segment_memo,
            limits.segments,
            0,
            Some(&segment_weights),
            Some(&marker_refs),
            &segment_weights,
        )?);
    }
    let uses_markers = !marker_refs.is_empty();
    let mut referenced_vertices = 0usize;
    let mut vertex_memo = HashMap::new();
    for node in root.descendants().filter(|n| n.is_element() && n.tag_name().name() == "use" && marker_refs.contains_key(&n.id())) {
        referenced_vertices = referenced_vertices.saturating_add(cost(
            node,
            &ids,
            &mut HashSet::new(),
            &mut vertex_memo,
            limits.segments,
            0,
            Some(&segment_weights),
            None,
            &segment_weights,
        )?);
    }
    let vertices = if uses_markers { segments.saturating_add(count.saturating_mul(4)).saturating_add(referenced_vertices) } else { 0 };
    if marker_objects.saturating_mul(vertices).saturating_mul(3) > limits.objects
        || marker_segments.saturating_mul(vertices).saturating_mul(3) > limits.segments
    {
        return Err(ImportError::Limit("expanded marker"));
    }
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod hostile_svg_tests {
    use super::*;
    #[test]
    fn marker_work_is_charged_per_vertex_before_usvg() {
        let paths = " L1 1".repeat(200);
        let art = format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg"><defs><marker id="m"><rect width="1" height="1"/></marker></defs><path d="M0 0{paths}" marker-mid="url(#m)"/></svg>"##
        );
        let mut options = ImportOptions::default();
        options.limits.objects = 100;
        assert!(matches!(xml(&art, &options, &AtomicBool::new(false)), Err(ImportError::Limit("expanded marker"))));
    }
    #[test]
    fn exponential_css_is_rejected_before_either_style_matcher() {
        let art = format!(r#"<svg xmlns="http://www.w3.org/2000/svg"><style>{} {{fill:red}}</style><text>Hi</text></svg>"#, "g ".repeat(20));
        assert!(matches!(xml(&art, &ImportOptions::default(), &AtomicBool::new(false)), Err(ImportError::Limit("CSS selector complexity"))));
    }
}

#[cfg(test)]
mod unused_marker_tests {
    use super::*;
    #[test]
    fn unused_marker_definitions_do_not_multiply_path_work() {
        let art = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg"><defs><marker id="unused"><rect width="1" height="1"/></marker></defs><path d="M0 0{}"/></svg>"#,
            " L1 1".repeat(200)
        );
        let mut options = ImportOptions::default();
        options.limits.objects = 100;
        assert!(xml(&art, &options, &AtomicBool::new(false)).is_ok());
    }
}

#[cfg(test)]
mod nested_marker_regression {
    use super::*;
    #[test]
    fn nested_css_marker_chains_are_charged_before_usvg() {
        let points = " L1 1".repeat(30);
        let source = format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg"><style>.outer {{marker-mid:url(#a)}} .first {{marker-mid:url(#b)}} .second {{marker-mid:url(#c)}}</style><defs><marker id="a"><path class="first" d="M0 0{points}"/></marker><marker id="b"><path class="second" d="M0 0{points}"/></marker><marker id="c"><rect width="1" height="1"/></marker></defs><path class="outer" d="M0 0{points}"/></svg>"##
        );
        let mut options = ImportOptions::default();
        options.limits.objects = 10_000;
        assert!(matches!(xml(&source, &options, &AtomicBool::new(false)), Err(ImportError::Limit(_))));
    }
}

#[cfg(test)]
mod referenced_marker_regression {
    use super::*;
    #[test]
    fn markers_on_use_charge_the_referenced_path_vertices() {
        let points = " L1 1".repeat(100);
        let uses = r##"<use href="#p" marker-mid="url(#m)"/>"##.repeat(200);
        let source = format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg"><defs><marker id="m"><rect width="1" height="1"/></marker><path id="p" d="M0 0{points}"/></defs>{uses}</svg>"##
        );
        let mut options = ImportOptions::default();
        options.limits.objects = 20_000;
        assert!(matches!(xml(&source, &options, &AtomicBool::new(false)), Err(ImportError::Limit("expanded marker"))));
    }
}

#[cfg(test)]
mod benign_complexity_regressions {
    use super::*;
    #[test]
    fn short_documents_admit_bounded_longer_descendant_selectors() {
        let source = include_str!("../../fixtures/import/resvg/structure/style/rule-specificity.svg");
        assert!(xml(source, &ImportOptions::default(), &AtomicBool::new(false)).is_ok());
    }
    #[test]
    fn inherited_markers_inside_their_own_art_do_not_recurse() {
        let source = include_str!("../../fixtures/import/resvg/painting/marker/recursive-3.svg");
        assert!(xml(source, &ImportOptions::default(), &AtomicBool::new(false)).is_ok());
    }
}

#[cfg(test)]
mod class_matching_regression {
    use super::*;
    #[test]
    fn large_class_compounds_are_charged_before_style_matching() {
        let names: Vec<_> = (0..100).map(|n| format!("a{n}")).collect();
        let selector = names.iter().map(|n| format!(".{n}")).collect::<String>();
        let classes = format!("{}{}", "b ".repeat(900), names[..99].join(" "));
        let shapes = format!(r#"<rect class="{classes}" width="1" height="1"/>"#).repeat(100);
        let source = format!(r#"<svg xmlns="http://www.w3.org/2000/svg"><style>{selector} {{fill:red}}</style>{shapes}</svg>"#);
        assert!(matches!(xml(&source, &ImportOptions::default(), &AtomicBool::new(false)), Err(ImportError::Limit("CSS matching work"))));
    }
}
