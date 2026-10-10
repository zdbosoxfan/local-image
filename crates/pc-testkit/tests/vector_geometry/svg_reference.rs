//! Test-only usvg normalization of externally generated SVGs. No pathops dependency.
use photocraft_doc::{FillRule, Knot, Path, PathOp, Subpath};
use photocraft_geom::Point;
use resvg::{tiny_skia::PathSegment, usvg};
use serde_json::{Value, json};
use std::path::PathBuf;
type TestResult = Result<(), Box<dyn std::error::Error>>;

fn reference_paths(svg: &str) -> Result<Vec<Path>, Box<dyn std::error::Error>> {
    fn walk(group: &usvg::Group, out: &mut Vec<Path>) -> Result<(), String> {
        if group.clip_path().is_some() || group.mask().is_some() || !group.filters().is_empty() {
            return Err("geometry golden contains a clip/mask/filter".into());
        }
        for node in group.children() {
            match node {
                usvg::Node::Group(g) => walk(g, out)?,
                usvg::Node::Path(p) => {
                    if p.stroke().is_some() {
                        return Err("geometry golden still contains an unexpanded stroke".into());
                    }
                    let Some(fill) = p.fill() else { continue };
                    let t = p.abs_transform();
                    let point = |p: resvg::tiny_skia::Point| {
                        Point::new(
                            f64::from(t.sx) * f64::from(p.x) + f64::from(t.kx) * f64::from(p.y) + f64::from(t.tx),
                            f64::from(t.ky) * f64::from(p.x) + f64::from(t.sy) * f64::from(p.y) + f64::from(t.ty),
                        )
                    };
                    let mut path =
                        Path { fill_rule: if fill.rule() == usvg::FillRule::EvenOdd { FillRule::EvenOdd } else { FillRule::NonZero }, ..Default::default() };
                    let mut sub = Subpath::default();
                    for segment in p.data().segments() {
                        match segment {
                            PathSegment::MoveTo(p) => {
                                if !sub.knots.is_empty() {
                                    path.subpaths.push(std::mem::take(&mut sub));
                                }
                                let p = point(p);
                                sub.knots.push(Knot::corner(p.x, p.y));
                            }
                            PathSegment::LineTo(p) => {
                                let p = point(p);
                                sub.knots.push(Knot::corner(p.x, p.y));
                            }
                            PathSegment::QuadTo(c, p) => {
                                let (c, p) = (point(c), point(p));
                                let prev = sub.knots.last_mut().ok_or("quadratic without start")?;
                                prev.out_ctrl = Point::new(prev.anchor.x + (c.x - prev.anchor.x) * 2. / 3., prev.anchor.y + (c.y - prev.anchor.y) * 2. / 3.);
                                sub.knots.push(Knot::smooth(p, Point::new(p.x + (c.x - p.x) * 2. / 3., p.y + (c.y - p.y) * 2. / 3.), p));
                            }
                            PathSegment::CubicTo(c1, c2, p) => {
                                let (c1, c2, p) = (point(c1), point(c2), point(p));
                                sub.knots.last_mut().ok_or("cubic without start")?.out_ctrl = c1;
                                sub.knots.push(Knot::smooth(p, c2, p));
                            }
                            PathSegment::Close => {
                                sub.closed = true;
                                if sub.knots.len() > 1 && sub.knots.first().map(|k| k.anchor) == sub.knots.last().map(|k| k.anchor) {
                                    let last = sub.knots.pop().ok_or("missing closing knot")?;
                                    sub.knots.first_mut().ok_or("missing first knot")?.in_ctrl = last.in_ctrl;
                                }
                            }
                        }
                    }
                    if !sub.knots.is_empty() {
                        path.subpaths.push(sub);
                    }
                    for sub in path.subpaths.iter_mut().skip(1) {
                        sub.op = PathOp::Join;
                    }
                    out.push(path);
                }
                _ => return Err("geometry golden contains image/text nodes".into()),
            }
        }
        Ok(())
    }
    if svg.len() > 50_000 {
        return Err("geometry golden exceeds 50 KB".into());
    }
    let tree = usvg::Tree::from_str(svg, &usvg::Options::default())?;
    if tree.size().width() != 96.0 || tree.size().height() != 96.0 {
        return Err("geometry golden page must retain the 96x96 input viewport".into());
    }
    let mut paths = Vec::new();
    walk(tree.root(), &mut paths)?;
    Ok(paths)
}

#[test]
fn geometry31_svg_normalizer_preserves_transforms_holes_and_cubic_handles() -> TestResult {
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="96" height="96"><g transform="translate(4 6) scale(2)"><path fill-rule="evenodd" d="M0 0 H16 V16 H0 Z M4 4 H12 V12 H4 Z"/><path d="M0 0 C2 3 4 5 6 0 Z"/></g></svg>"#;
    let paths = reference_paths(svg)?;
    assert_eq!(paths.len(), 2);
    assert_eq!(paths[0].fill_rule, FillRule::EvenOdd);
    assert_eq!(crate::vector::area(&paths[0], FillRule::EvenOdd)?, 768.);
    assert_eq!(paths[0].subpaths[0].knots[0].anchor, Point::new(4., 6.));
    let k = &paths[1].subpaths[0].knots;
    assert_eq!(k[0].out_ctrl, Point::new(8., 12.));
    assert_eq!(k[1].in_ctrl, Point::new(12., 16.));
    assert_eq!(k[1].anchor, Point::new(16., 6.));
    assert!(
        reference_paths(r#"<svg xmlns="http://www.w3.org/2000/svg" width="96" height="96"><path d="M1 1 H20" fill="none" stroke="black"/></svg>"#).is_err()
    );
    assert!(reference_paths(r#"<svg xmlns="http://www.w3.org/2000/svg" width="96" height="96"/>"#)?.is_empty());
    Ok(())
}

#[test]
#[ignore = "coordinator-only normalization after external Inkscape generation, no CLI invocation"]
fn geometry31_prepare_inkscape_sidecars() -> TestResult {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../pc-pathops/tests/tricky");
    let manifest: Value = serde_json::from_str(&std::fs::read_to_string(root.join("manifest.json"))?)?;
    // The coordinator records the actual --version result and every executed action recipe.
    // Without that external provenance record, never stamp invented Inkscape metadata.
    let record: Value = serde_json::from_str(&std::fs::read_to_string(root.join("goldens/generation.json"))?)?;
    assert_eq!(record["tool"], "Inkscape 1.4.4");
    let cases = manifest["cases"].as_array().ok_or("missing cases")?;
    let operations = manifest["operations"].as_object().ok_or("missing operations")?;
    for case in cases {
        let id = case["id"].as_str().ok_or("missing id")?;
        let input = std::fs::read_to_string(root.join("inputs").join(format!("{id}.svg")))?;
        assert_eq!(record["inputs"][id], input, "generation record must match the actual input");
        for (operation, action) in operations {
            let base = root.join("goldens").join(id).join(operation);
            let svg = std::fs::read_to_string(base.with_extension("svg"))?;
            let recipe = format!("select-by-id:a,b;{}", action.as_str().ok_or("missing action")?);
            assert_eq!(record["actions"][format!("{id}/{operation}")], recipe);
            let paths = reference_paths(&svg)?;
            let golden = json!({"tool":record["tool"],"normalizer":"usvg 0.45.1","case":id,"operation":operation,"actions":recipe,"input_svg":input,"source_svg":svg,"paths":paths});
            std::fs::write(base.with_extension("json"), serde_json::to_string_pretty(&golden)? + "\n")?;
        }
    }
    for case in manifest["unary_cases"].as_array().ok_or("missing unary cases")? {
        let id = case["id"].as_str().ok_or("missing id")?;
        let input = std::fs::read_to_string(root.join("inputs").join(format!("{id}.svg")))?;
        assert_eq!(record["inputs"][id], input, "generation record must match the actual input");
        let operation = case["operation"].as_str().ok_or("missing operation")?;
        let key = format!("{id}/{operation}");
        let recipe = record["actions"][&key].as_str().ok_or("missing executed action recipe")?;
        assert!(!recipe.trim().is_empty());
        if case["actions"].is_string() {
            assert_eq!(case["actions"], recipe);
        }
        assert_eq!(record["parameters"][&key], case["parameters"]);
        let base = root.join("goldens").join(id).join(operation);
        let svg = std::fs::read_to_string(base.with_extension("svg"))?;
        let paths = reference_paths(&svg)?;
        let golden = json!({"tool":record["tool"],"normalizer":"usvg 0.45.1","case":id,"operation":operation,"actions":recipe,"parameters":case["parameters"],"input_svg":input,"source_svg":svg,"paths":paths});
        std::fs::write(base.with_extension("json"), serde_json::to_string_pretty(&golden)? + "\n")?;
    }
    Ok(())
}
