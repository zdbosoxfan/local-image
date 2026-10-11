//! External references are coordinator-generated, never replaced with Local Image output.
use photocraft_doc::{FillRule, Path};
use photocraft_geom::{Affine, Rect};
use photocraft_pathops as ops;
use photocraft_testkit::vector_geometry::{Outcome, Thresholds, check_ratchet, compare_regions};
use serde_json::Value;
use std::{collections::BTreeSet, path::PathBuf};
type TestResult = Result<(), Box<dyn std::error::Error>>;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/tricky")
}
fn text<'a>(value: &'a Value, name: &str) -> Result<&'a str, String> {
    value[name].as_str().ok_or_else(|| format!("missing string {name}"))
}
fn cases() -> Result<Vec<Value>, Box<dyn std::error::Error>> {
    let manifest: Value = serde_json::from_str(include_str!("../tricky/manifest.json"))?;
    Ok(manifest["cases"].as_array().ok_or("missing corpus cases")?.clone())
}
fn input(case: &Value, which: &str) -> Result<Path, Box<dyn std::error::Error>> {
    let bez = kurbo::BezPath::from_svg(text(case, which)?)?;
    let mut path = ops::from_geometry(&ops::geom::PathData::from_bezpath(&bez));
    path.fill_rule = if text(case, "fill_rule")? == "evenodd" { FillRule::EvenOdd } else { FillRule::NonZero };
    let scale = case["scale"].as_f64().ok_or("missing scale")?;
    let x = case["origin"][0].as_f64().ok_or("missing origin x")?;
    let y = case["origin"][1].as_f64().ok_or("missing origin y")?;
    Ok(path.transform(&Affine { m: [scale, 0., 0., scale, x, y] }))
}

#[test]
fn geometry31_original_corpus_has_sixty_licensed_inputs_and_distinct_cases() -> TestResult {
    let cases = cases()?;
    assert_eq!(cases.len(), 60);
    let mut ids = BTreeSet::new();
    let mut families = BTreeSet::new();
    for case in cases {
        let id = text(&case, "id")?;
        assert!(ids.insert(id.to_owned()));
        families.insert(text(&case, "family")?.to_owned());
        let svg = std::fs::read_to_string(root().join("inputs").join(format!("{id}.svg")))?;
        assert!(svg.contains("SPDX-License-Identifier: CC0-1.0"));
        assert!(svg.contains(text(&case, "a")?));
        assert!(svg.contains(text(&case, "b")?));
        ops::validate(&input(&case, "a")?)?;
        ops::validate(&input(&case, "b")?)?;
    }
    assert_eq!(families.len(), 20);
    Ok(())
}

#[test]
#[ignore = "coordinator acceptance: requires all 345 real Inkscape 1.4.4 SVGs and sidecars"]
fn geometry31_inkscape_goldens_and_shrinking_ratchet() -> TestResult {
    let mut outcomes = Vec::new();
    for case in cases()? {
        let id = text(&case, "id")?;
        for (operation, action) in [
            ("union", "path-union"),
            ("difference", "path-difference"),
            ("intersect", "path-intersection"),
            ("xor", "path-exclusion"),
            ("division", "path-division"),
        ] {
            let key = format!("{id}/{operation}");
            let base = root().join("goldens").join(id).join(operation);
            if !base.with_extension("svg").is_file() || !base.with_extension("json").is_file() {
                outcomes.push((key, Outcome::MissingReference("Inkscape SVG/normalized sidecar absent".into())));
                continue;
            }
            let svg = std::fs::read_to_string(base.with_extension("svg"))?;
            let golden: Value = serde_json::from_str(&std::fs::read_to_string(base.with_extension("json"))?)?;
            assert_eq!(golden["tool"], "Inkscape 1.4.4", "wrong oracle tool for {key}");
            assert_eq!(golden["normalizer"], "usvg 0.45.1 + svgtypes 0.15.3 f64 geometry");
            assert_eq!(golden["case"], id);
            assert_eq!(golden["operation"], operation);
            assert_eq!(golden["actions"], format!("select-by-id:a,b;{action}"));
            assert_eq!(golden["source_svg"], svg, "stale normalized reference for {key}");
            assert_eq!(golden["input_svg"], std::fs::read_to_string(root().join("inputs").join(format!("{id}.svg")))?, "stale input for {key}");
            let reference = reference_regions(&golden)?;
            let a = input(&case, "a")?;
            let b = input(&case, "b")?;
            let result = match operation {
                "division" => {
                    // Inkscape divides the bottom object, while Pathfinder Divide includes
                    // every union face. Select only regions belonging to input A.
                    ops::regions(&[ops::Shape::new(a, 0), ops::Shape::new(b, 1)])
                        .map(|regions| regions.into_iter().filter(|r| r.sources.contains(&0)).map(|r| r.path).collect())
                }
                _ => ops::boolean(
                    &a,
                    &b,
                    match operation {
                        "union" => ops::BoolOp::Union,
                        "difference" => ops::BoolOp::Difference,
                        "intersect" => ops::BoolOp::Intersect,
                        _ => ops::BoolOp::Xor,
                    },
                )
                .map(|p| vec![p]),
            };
            let scale = case["scale"].as_f64().ok_or("missing scale")?;
            let x = case["origin"][0].as_f64().ok_or("missing x")?;
            let y = case["origin"][1].as_f64().ok_or("missing y")?;
            let normalize = Affine { m: [1. / scale, 0., 0., 1. / scale, -x / scale, -y / scale] };
            let outcome = match result {
                Ok(paths) => {
                    let normalized: Vec<_> = paths.iter().map(|p| p.transform(&normalize)).collect();
                    match compare_regions(&normalized, &reference, Affine::scale(8.), Rect::new(0, 0, 768, 768)) {
                        Ok(scores) if scores.passes(Thresholds { hausdorff: 0.05 / scale, ..Default::default() }) => Outcome::Passed,
                        scores => {
                            let dir = root().join("../../../../target/refvec/geometry31");
                            std::fs::create_dir_all(&dir)?;
                            std::fs::write(dir.join(format!("{id}-{operation}.json")), serde_json::to_vec_pretty(&normalized)?)?;
                            eprintln!("{key} crossings: {:?}", normalized.iter().flat_map(photocraft_testkit::vector::self_intersections).collect::<Vec<_>>());
                            Outcome::Failed(format!("{scores:?}"))
                        }
                    }
                }
                Err(error) => Outcome::Failed(error.to_string()),
            };
            outcomes.push((key, outcome));
        }
    }
    let manifest: Value = serde_json::from_str(include_str!("../tricky/manifest.json"))?;
    for case in manifest["unary_cases"].as_array().ok_or("missing unary cases")? {
        let id = text(case, "id")?;
        let operation = text(case, "operation")?;
        let key = format!("{id}/{operation}");
        let base = root().join("goldens").join(id).join(operation);
        if !base.with_extension("svg").is_file() || !base.with_extension("json").is_file() {
            outcomes.push((key, Outcome::MissingReference("Inkscape SVG/normalized sidecar absent".into())));
            continue;
        }
        let svg = std::fs::read_to_string(base.with_extension("svg"))?;
        let golden: Value = serde_json::from_str(&std::fs::read_to_string(base.with_extension("json"))?)?;
        assert_eq!(golden["tool"], "Inkscape 1.4.4");
        assert_eq!(golden["normalizer"], "usvg 0.45.1 + svgtypes 0.15.3 f64 geometry");
        assert_eq!(golden["case"], id);
        assert_eq!(golden["operation"], operation);
        assert_eq!(golden["parameters"], case["parameters"], "unmatched oracle options for {key}");
        assert_eq!(golden["source_svg"], svg);
        assert_eq!(golden["input_svg"], std::fs::read_to_string(root().join("inputs").join(format!("{id}.svg")))?, "stale input for {key}");
        assert!(!text(&golden, "actions")?.trim().is_empty());
        if case["actions"].is_string() {
            assert_eq!(golden["actions"], case["actions"]);
        }
        let reference = reference_regions(&golden)?;
        let path = ops::from_geometry(&ops::geom::PathData::from_bezpath(&kurbo::BezPath::from_svg(text(case, "path")?)?));
        let p = &case["parameters"];
        let join = match text(p, "join").unwrap_or("round") {
            "miter" => ops::Join::Miter,
            "bevel" => ops::Join::Bevel,
            _ => ops::Join::Round,
        };
        let number = |key: &str| p[key].as_f64().ok_or_else(|| format!("missing {key}"));
        if operation == "simplify" {
            if reference.len() != 1 || photocraft_testkit::vector::hausdorff(&path, &reference[0], 64)? > number("tolerance")? {
                return Err(format!("Inkscape simplify settings did not achieve the requested tolerance: {id}").into());
            }
            let result = ops::simplify(&path, number("tolerance")?);
            let outcome = match result {
                Ok(actual) => {
                    // Simplify implementations can choose different curves within the same
                    // error budget. A.2 gates deviation from the source, not cubic equality.
                    let error = photocraft_testkit::vector::hausdorff(&path, &actual, 64)?;
                    let nodes = photocraft_testkit::vector::node_count(&actual);
                    let budget = photocraft_testkit::vector::node_count(&reference[0]) as f64 * 1.5;
                    if error <= number("tolerance")? && nodes as f64 <= budget && photocraft_testkit::vector::self_intersections(&actual).is_empty() {
                        Outcome::Passed
                    } else {
                        Outcome::Failed(format!("simplify source deviation {error}, nodes {nodes}, reference budget {budget}"))
                    }
                }
                Err(error) => Outcome::Failed(error.to_string()),
            };
            outcomes.push((key, outcome));
            continue;
        }
        let result = match operation {
            "offset" => ops::offset_path(&path, number("delta")?, join, number("miter")?),
            "stroke" => {
                let stroke = photocraft_doc::ShapeStroke {
                    width: number("width")? as f32,
                    cap: match text(p, "cap")? {
                        "butt" => photocraft_doc::LineCap::Butt,
                        "square" => photocraft_doc::LineCap::Square,
                        _ => photocraft_doc::LineCap::Round,
                    },
                    join: match join {
                        ops::Join::Miter => photocraft_doc::LineJoin::Miter,
                        ops::Join::Bevel => photocraft_doc::LineJoin::Bevel,
                        ops::Join::Round => photocraft_doc::LineJoin::Round,
                    },
                    miter_limit: number("miter")? as f32,
                    dashes: serde_json::from_value(p["dashes"].clone())?,
                    dash_offset: number("dash_offset")? as f32,
                    ..Default::default()
                };
                ops::outline_stroke(&path, &stroke, 0.001)
            }
            _ => return Err(format!("unknown corpus operation: {operation}").into()),
        };
        outcomes.push((
            key,
            match result {
                Ok(path) => match compare_regions(std::slice::from_ref(&path), &reference, Affine::scale(8.), Rect::new(0, 0, 768, 768)) {
                    Ok(scores) if scores.passes(Thresholds::default()) => Outcome::Passed,
                    scores => {
                        let dir = root().join("../../../../target/refvec/geometry31");
                        std::fs::create_dir_all(&dir)?;
                        std::fs::write(dir.join(format!("{id}-{operation}.json")), serde_json::to_vec_pretty(&[path])?)?;
                        Outcome::Failed(format!("{scores:?}"))
                    }
                },
                Err(error) => Outcome::Failed(error.to_string()),
            },
        ));
    }
    let raw: Vec<Value> = serde_json::from_str(include_str!("../tricky/goldens/ratchet.json"))?;
    let ratchet: Result<Vec<_>, String> = raw.iter().map(|v| Ok((text(v, "case")?.to_owned(), text(v, "reason")?.to_owned()))).collect();
    for (id, outcome) in &outcomes {
        if !matches!(outcome, Outcome::Passed) {
            eprintln!("{id}: {outcome:?}");
        }
    }
    let summary = check_ratchet(&outcomes, &ratchet?, 0.95)?;
    assert_eq!(summary.total, 345);
    eprintln!("Inkscape external corpus: {summary:?}");
    Ok(())
}

fn reference_regions(golden: &Value) -> Result<Vec<Path>, Box<dyn std::error::Error>> {
    let paths: Vec<Path> = serde_json::from_value(golden["paths"].clone())?;
    // Corrupt or unresolved references are invalid external evidence, not ratchetable failures.
    for path in &paths {
        let area = photocraft_testkit::vector::area(path, path.fill_rule)?;
        if !area.is_finite() || area < 0. || !photocraft_testkit::vector::self_intersections(path).is_empty() {
            return Err(format!(
                "invalid external reference geometry: {}/{} area={area}, crossings={:?}",
                golden["case"],
                golden["operation"],
                photocraft_testkit::vector::self_intersections(path)
            )
            .into());
        }
    }
    Ok(paths)
}

#[test]
fn geometry31_unary_oracle_inputs_record_offsets_stroke_units_and_simplify_options() -> TestResult {
    let manifest: Value = serde_json::from_str(include_str!("../tricky/manifest.json"))?;
    let cases = manifest["unary_cases"].as_array().ok_or("missing unary cases")?;
    assert_eq!(cases.len(), 45);
    let mut ids = BTreeSet::new();
    for case in cases {
        let id = text(case, "id")?;
        assert!(ids.insert(id));
        let svg = std::fs::read_to_string(root().join("inputs").join(format!("{id}.svg")))?;
        assert!(svg.contains("SPDX-License-Identifier: CC0-1.0"));
        assert!(svg.contains(text(case, "path")?));
        let bez = kurbo::BezPath::from_svg(text(case, "path")?)?;
        assert!(bez.is_finite());
        if text(case, "operation")? == "stroke" && !case["parameters"]["dashes"].as_array().ok_or("dashes missing")?.is_empty() {
            // SVG uses absolute dash lengths; the native stroke uses width-relative lengths.
            assert!(svg.contains("stroke-dasharray=\"16 8\""));
            assert!(svg.contains("stroke-dashoffset=\"4\""));
        }
    }
    Ok(())
}
