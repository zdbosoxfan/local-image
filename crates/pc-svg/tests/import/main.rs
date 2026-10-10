//! Independent native layer-tree, coverage, hostile-input and resvg corpus checks.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use photocraft_doc::{ColorMode, Document, Layer, LayerContent, SampleType, vector_design::exchange::ImportOptions};
use std::{
    path::PathBuf,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/import")
}
fn load(name: &str) -> photocraft_doc::vector_design::exchange::ImportedScene {
    photocraft_svg::import(&std::fs::read(fixtures().join(format!("{name}.svg"))).unwrap(), &ImportOptions::default()).unwrap()
}
fn flat(layers: &[Layer]) -> Vec<&Layer> {
    let mut out = vec![];
    for l in layers {
        out.push(l);
        if let Some(ch) = l.children() {
            out.extend(flat(ch));
        }
    }
    out
}
fn document(scene: photocraft_doc::vector_design::exchange::ImportedScene) -> Document {
    let mut d = Document::new("test", scene.size, ColorMode::Rgb, SampleType::F32);
    d.layers = scene.layers;
    d
}
fn shape<'a>(scene: &'a photocraft_doc::vector_design::exchange::ImportedScene, name: &str) -> &'a photocraft_doc::ShapeLayer {
    let l = flat(&scene.layers).into_iter().find(|l| l.name == name).unwrap();
    if let LayerContent::Shape(s) = &l.content { s } else { panic!("expected shape") }
}

#[test]
fn svg_import_fifteen_expected_layer_trees() {
    let expected: serde_json::Value = serde_json::from_slice(&std::fs::read(fixtures().join("expected-trees.json")).unwrap()).unwrap();
    assert_eq!(expected.as_object().unwrap().len(), 15);
    for (name, e) in expected.as_object().unwrap() {
        let s = load(name);
        let layers = flat(&s.layers);
        for (key, kind) in [("groups", "Group"), ("shape_names", "Shape"), ("text_names", "Type"), ("raster_names", "Pixel")] {
            if let Some(names) = e.get(key) {
                for n in names.as_array().unwrap() {
                    let layer = layers.iter().find(|l| l.name == n.as_str().unwrap() && l.content.kind_name() == kind).unwrap_or_else(|| {
                        panic!("{name}: missing {kind} {n}, found {:?}", layers.iter().map(|l| (&l.name, l.content.kind_name())).collect::<Vec<_>>())
                    });
                    assert_eq!(layer.content.kind_name(), kind, "{name}");
                }
            }
        }
        if let Some(v) = e.get("shape_count") {
            assert_eq!(layers.iter().filter(|l| matches!(l.content, LayerContent::Shape(_))).count(), v.as_u64().unwrap() as usize);
        }
        if let Some(v) = e.get("vector_masks") {
            assert_eq!(layers.iter().filter(|l| l.vector_mask.is_some()).count(), v.as_u64().unwrap() as usize);
        }
        if let Some(v) = e.get("pixel_masks") {
            assert_eq!(layers.iter().filter(|l| l.mask.is_some()).count(), v.as_u64().unwrap() as usize);
        }
        assert!(layers.iter().all(|l| !matches!(&l.content,LayerContent::Shape(s) if s.cache.is_none())));
    }
}
#[test]
fn svg_import_geometry_units_and_editable_text_oracles() {
    let s = load("transforms");
    assert_eq!(shape(&s, "box").path.control_bounds(), Some((12.0, 16.0, 28.0, 24.0)));
    let s = load("closed-cubic");
    let sp = &shape(&s, "loop").path.subpaths[0];
    assert!(sp.closed);
    assert_eq!(sp.knots.len(), 2);
    assert_eq!(sp.knots[0].in_ctrl, photocraft_geom::Point::new(8.0, 60.0));
    let s = load("holes");
    let d = document(s);
    let px = photocraft_compose::flatten(&d);
    assert_eq!(px.get(32, 32)[3], 0.0);
    assert_eq!(px.get(8, 8)[3], 1.0);
    let s = load("viewbox-units");
    assert_eq!(s.size, photocraft_doc::Size { width: 72, height: 36 });
    assert_eq!(s.units, photocraft_doc::DesignUnits::Millimeters);
    let bounds = shape(&s, "box").path.control_bounds().unwrap();
    assert!((bounds.2 - 72.0).abs() < 1e-5 && (bounds.3 - 36.0).abs() < 1e-5, "physical unit bounds {bounds:?}");
    let s = load("editable-text");
    let layers = flat(&s.layers);
    let text = layers.iter().find_map(|l| if let LayerContent::Text(t) = &l.content { Some(t) } else { None }).unwrap();
    assert_eq!(text.text, "AB");
    assert_eq!(text.runs.len(), 2);
    assert_eq!(text.transform.m, [1.0, 0.0, 0.0, 1.0, 6.0, 28.0]);
    assert!(text.cache.is_some());
    assert!(s.warnings.iter().all(|w| w.feature != "font-substitution"));
    let s = load("mask");
    let px = photocraft_compose::flatten(&document(s));
    assert!((px.get(16, 32)[3] - 1.0).abs() < 0.01);
    assert!((px.get(48, 32)[3] - 128.0 / 255.0).abs() < 0.01);
    let px = photocraft_compose::flatten(&document(load("embedded-image")));
    assert_eq!(px.get(8, 12), [1.0, 0.0, 0.0, 1.0]);
    assert_eq!(px.get(23, 27), [1.0, 0.0, 0.0, 1.0]);
    assert_eq!(px.get(7, 12)[3], 0.0);
    assert_eq!(px.get(24, 27)[3], 0.0);
}
#[test]
fn svg_import_text_use_css_preserves_draw_order_and_no_marker_art() {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="80"><style>* {fill:blue!important} path {opacity:0.2!important} [data-local-image-svg-source] {fill:red!important} .a {font-size:10px;fill:red} #t {font-weight:bold}</style><defs><text id="t" class="a" x="2" y="12" font-family="Inter">Hi</text></defs><rect id="below" width="10" height="10"/><use href="#t" x="10"/><use href="#t" x="40"/><rect id="above" y="30" width="10" height="10"/><path id="ordinary" d="M70 50h10v10h-10Z"/></svg>"##;
    let s = photocraft_svg::import(svg.as_bytes(), &ImportOptions::default()).unwrap();
    let layers = flat(&s.layers);
    let texts: Vec<_> = layers.iter().filter_map(|l| if let LayerContent::Text(t) = &l.content { Some(t) } else { None }).collect();
    assert_eq!(texts.len(), 2);
    assert_eq!(texts[0].transform.m[4], 12.0);
    assert_eq!(texts[1].transform.m[4], 42.0);
    assert_eq!(texts[0].runs[0].style.weight, 700);
    assert_eq!(texts[0].size_pt, 10.0);
    let shapes: Vec<_> = layers.iter().filter(|l| matches!(l.content, LayerContent::Shape(_))).map(|l| l.name.as_str()).collect();
    assert_eq!(shapes, ["below", "above", "ordinary"]);
    let pixels = photocraft_compose::flatten(&document(s));
    assert_eq!(pixels.get(5, 5), [0.0, 0.0, 1.0, 1.0]);
    assert!((pixels.get(75, 55)[3] - 0.2).abs() < 1e-6, "source path CSS remains effective");
    let ink = (0..20).flat_map(|y| (12..30).map(move |x| (x, y))).map(|(x, y)| pixels.get(x, y)[3]).fold(0.0_f32, f32::max);
    assert!(ink > 0.9, "path CSS must not alter synthetic text markers: {ink}");
}
#[test]
fn svg_import_rejects_hostile_inputs_within_one_second() {
    let mut cases = vec![
        "<svg".to_string(),
        "<!DOCTYPE svg [<!ENTITY x 'boom'>]><svg xmlns='http://www.w3.org/2000/svg'>&x;</svg>".into(),
        "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 1e30 1e30'/>".into(),
        "<svg xmlns='http://www.w3.org/2000/svg' width='1e5' height='1e5'/>".into(),
        "<svg xmlns='http://www.w3.org/2000/svg' width='999999' height='1'><rect width='999999' height='1'/></svg>".into(),
        "<svg xmlns='http://www.w3.org/2000/svg'><path d='M0 0 Lbad'/></svg>".into(),
        "<svg xmlns='http://www.w3.org/2000/svg'><path d='M0 0L1000 0' stroke='red' stroke-dasharray='0.0000001 0.0000001'/></svg>".into(),
        format!("<svg xmlns='http://www.w3.org/2000/svg'>{}<rect width='1' height='1'/>{}</svg>", "<g>".repeat(400), "</g>".repeat(400)),
        format!("<svg xmlns='http://www.w3.org/2000/svg' width='100' height='100'><text>{}</text></svg>", "A".repeat(100_001)),
    ];
    let mut expansion = String::from("<svg xmlns='http://www.w3.org/2000/svg'><defs><g id='a0'><rect width='1' height='1'/></g>");
    for i in 1..30 {
        expansion.push_str(&format!("<g id='a{i}'><use href='#a{}'/><use href='#a{}'/></g>", i - 1, i - 1));
    }
    expansion.push_str("</defs><use href='#a29'/></svg>");
    cases.push(expansion);
    for (i, svg) in cases.iter().enumerate() {
        let start = Instant::now();
        assert!(photocraft_svg::import(svg.as_bytes(), &ImportOptions::default()).is_err(), "hostile {i}");
        assert!(start.elapsed() < Duration::from_secs(1), "hostile {i}: {:?}", start.elapsed());
    }
    let mut opts = ImportOptions::default();
    opts.limits.decompressed_bytes = 128;
    let svg = format!("<svg xmlns='http://www.w3.org/2000/svg'><image href='data:image/png;base64,{}'/></svg>", "A".repeat(1024));
    let start = Instant::now();
    assert!(photocraft_svg::import(svg.as_bytes(), &opts).is_err());
    assert!(start.elapsed() < Duration::from_secs(1));
    assert!(matches!(photocraft_svg::import_with(b"", &opts, None, &AtomicBool::new(true)), Err(photocraft_svg::ImportError::Cancelled)));
}
#[test]
fn svg_import_never_opens_external_images_and_aggregates_warnings() {
    // This path would be readable by usvg's default resolver. It is never resolved.
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><image href="/etc/passwd" width="10" height="10"/><image href="https://example.invalid/image.png" width="10" height="10"/><foreignObject/><foreignObject/></svg>"#;
    let s = photocraft_svg::import(svg.as_bytes(), &ImportOptions::default()).unwrap();
    assert_eq!(s.warnings.iter().filter(|w| w.feature == "external-resource").count(), 1);
    assert_eq!(s.warnings.iter().filter(|w| w.feature == "foreignObject").count(), 1);
    assert!(flat(&s.layers).iter().all(|l| !matches!(l.content, LayerContent::Raster(_))));
}

#[test]
fn svg_import_text_position_approximations_are_navigable() {
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="80"><text id="percentage" x="50%" y="20" font-family="Inter">Hi</text><text id="rotated" x="10" y="40" rotate="15" font-family="Inter">AB</text></svg>"#;
    let s = photocraft_svg::import(svg.as_bytes(), &ImportOptions::default()).unwrap();
    assert_eq!(flat(&s.layers).iter().filter(|l| matches!(l.content, LayerContent::Text(_))).count(), 2);
    assert!(s.warnings.iter().any(|w| w.feature == "text-position-percent" && w.object.as_deref() == Some("percentage")));
    assert!(s.warnings.iter().any(|w| w.feature == "text-position-lists" && w.object.as_deref() == Some("rotated")));
}

#[test]
fn svg_import_resvg_independent_original_fixture_renders() {
    for name in
        ["shapes", "curves", "arcs", "holes", "closed-cubic", "transforms", "linear-gradient", "radial-gradient", "clip", "mask", "css-use", "viewbox-units"]
    {
        let source = std::fs::read(fixtures().join(format!("{name}.svg"))).unwrap();
        let tree = usvg::Tree::from_data(&source, &usvg::Options { dpi: 72.0, ..Default::default() }).unwrap();
        let size = tree.size().to_int_size();
        // Use the quality plan's 8x coverage oracle. tiny-skia's 1px hairline
        // antialiasing does not conserve area; 8x box integration compares it
        // meaningfully with pc-vector's exact pixel-area coverage.
        let scale = 8;
        let mut reference = resvg::tiny_skia::Pixmap::new(size.width() * scale, size.height() * scale).unwrap();
        resvg::render(&tree, usvg::Transform::from_scale(scale as f32, scale as f32), &mut reference.as_mut());
        let rgba: Vec<_> = (0..size.height())
            .flat_map(|y| (0..size.width()).map(move |x| (x, y)))
            .map(|(x, y)| {
                let mut sum = [0.0_f32; 4];
                for dy in 0..scale {
                    for dx in 0..scale {
                        let p = reference.pixels()[((y * scale + dy) * size.width() * scale + x * scale + dx) as usize];
                        for (a, b) in sum.iter_mut().zip([p.red(), p.green(), p.blue(), p.alpha()]) {
                            *a += f32::from(b);
                        }
                    }
                }
                let a = sum[3] / (scale * scale) as f32;
                if a == 0.0 {
                    [0; 4]
                } else {
                    [
                        (sum[0] * 255.0 / sum[3]).round().min(255.0) as u8,
                        (sum[1] * 255.0 / sum[3]).round().min(255.0) as u8,
                        (sum[2] * 255.0 / sum[3]).round().min(255.0) as u8,
                        a.round() as u8,
                    ]
                }
            })
            .collect();
        let cpu = photocraft_compose::flatten(&document(load(name)));
        let (iou, mae) = parity(&cpu, &rgba);
        if name.contains("gradient") {
            let actual = cpu.to_rgba8().pixels;
            let distances: Vec<_> = cpu
                .px
                .iter()
                .zip(&rgba)
                .zip(actual.as_chunks::<4>().0)
                .filter(|((a, b), _)| a[3] > 0.999 && b[3] == 255)
                .map(|((_, b), a)| photocraft_testkit::vector::delta_e([a[0], a[1], a[2]], [b[0], b[1], b[2]]))
                .collect();
            assert!(!distances.is_empty());
            let mean = distances.iter().sum::<f64>() / distances.len() as f64;
            let max = distances.into_iter().fold(0.0_f64, f64::max);
            assert!(mean < 0.5 && max < 2.0, "{name}: off-edge delta E Oklab mean={mean} max={max}");
        }
        assert!(iou >= 0.97 && mae <= 0.02, "{name}: IoU={iou}, premultiplied RGB MAE={mae}");
    }
}

#[test]
fn svg_import_byte_segment_cache_and_svgz_limits() {
    use std::io::Write;
    let svg = std::fs::read(fixtures().join("shapes.svg")).unwrap();
    let mut opts = ImportOptions::default();
    opts.limits.bytes = svg.len() - 1;
    assert!(photocraft_svg::import(&svg, &opts).is_err());
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gz.write_all(&svg).unwrap();
    let gz = gz.finish().unwrap();
    assert_eq!(photocraft_svg::import(&gz, &ImportOptions::default()).unwrap().size, load("shapes").size);
    opts = ImportOptions::default();
    opts.limits.decompressed_bytes = 32;
    assert!(photocraft_svg::import(&gz, &opts).is_err());
    assert!(photocraft_svg::import(&svg, &opts).is_err());
    opts = ImportOptions::default();
    opts.limits.segments = 16;
    let source = r##"<svg xmlns="http://www.w3.org/2000/svg"><defs><path id="p" d="M0 0L10 0L10 10L0 10Z"/></defs><use href="#p"/><use href="#p"/></svg>"##;
    assert!(photocraft_svg::import(source.as_bytes(), &opts).is_err());
    let source = r##"<svg xmlns="http://www.w3.org/2000/svg"><defs><polygon id="p" points="0,0 10,0 10,10 0,10"/></defs><use href="#p"/><use href="#p"/><use href="#p"/></svg>"##;
    assert!(matches!(photocraft_svg::import(source.as_bytes(), &opts), Err(photocraft_svg::ImportError::Limit("expanded segment"))));
}

fn parity(cpu: &photocraft_compose::Buffer, rgba: &[[u8; 4]]) -> (f64, f64) {
    let mut intersection = 0.0;
    let mut union = 0.0;
    let mut error = 0.0;
    assert_eq!(cpu.px.len(), rgba.len());
    for (a, b) in cpu.px.iter().zip(rgba) {
        let alpha = f64::from(a[3]);
        let ba = f64::from(b[3]) / 255.0;
        intersection += alpha.min(ba);
        union += alpha.max(ba);
        for c in 0..3 {
            error += (f64::from(a[c]) * alpha - f64::from(b[c]) / 255.0 * ba).abs();
        }
    }
    (if union == 0.0 { 1.0 } else { intersection / union }, error / (cpu.px.len() * 3).max(1) as f64)
}
#[test]
fn svg_import_resvg_subset_ratchet() {
    let base = fixtures().join("resvg");
    let cases = std::fs::read_to_string(base.join("cases.tsv")).unwrap();
    let mut failures = vec![];
    let mut passed = 0;
    let mut total = 0;
    for row in cases.lines().skip(1) {
        let name = row.split('\t').next().unwrap();
        total += 1;
        let source = std::fs::read(base.join(name)).unwrap();
        let reference = image::open(base.join(name).with_extension("png")).unwrap().to_rgba8();
        let options = ImportOptions { dpi: 96.0, ..Default::default() };
        // Upstream's PNGs are rendered at width 300, not natural width 200.
        // Resize the root viewport with its original viewBox (or an explicit original
        // viewport if absent), retaining root opacity/background and child unit semantics.
        let opt = usvg::Options { dpi: 96.0, ..Default::default() };
        let normalized = match usvg::Tree::from_data(&source, &opt) {
            Ok(t) => t,
            Err(e) => {
                failures.push(format!("{name}\treference normalizer: {e}"));
                continue;
            }
        };
        let raw = std::str::from_utf8(&source).unwrap();
        let xml = roxmltree::Document::parse(raw).unwrap();
        let root = xml.root_element();
        let escape = |v: &str| v.replace('&', "&amp;").replace('"', "&quot;").replace('<', "&lt;");
        let ns: String = root
            .namespaces()
            .map(|n| match n.name() {
                Some(prefix) => format!(" xmlns:{prefix}=\"{}\"", escape(n.uri())),
                None => format!(" xmlns=\"{}\"", escape(n.uri())),
            })
            .collect();
        let attrs: String = root
            .attributes()
            .filter(|a| !matches!(a.name(), "width" | "height"))
            .map(|a| {
                let prefix = a.namespace().and_then(|uri| root.namespaces().find(|n| n.uri() == uri).and_then(|n| n.name()));
                let name = prefix.map_or_else(|| a.name().to_string(), |prefix| format!("{prefix}:{}", a.name()));
                format!(" {name}=\"{}\"", escape(a.value()))
            })
            .collect();
        let children: String = root.children().map(|n| &raw[n.range()]).collect();
        let tag = root.lookup_prefix("http://www.w3.org/2000/svg").map_or_else(|| "svg".to_string(), |p| format!("{p}:svg"));
        let viewbox = if root.attribute("viewBox").is_some() {
            String::new()
        } else {
            format!(" viewBox=\"0 0 {} {}\"", normalized.size().width(), normalized.size().height())
        };
        let scaled = format!("<{tag}{ns}{attrs}{viewbox} width=\"{}\" height=\"{}\">{children}</{tag}>", reference.width(), reference.height());
        match photocraft_svg::import(scaled.as_bytes(), &options) {
            Ok(scene) => {
                let cpu = photocraft_compose::flatten(&document(scene));
                let rgba: Vec<_> = reference.pixels().map(|p| p.0).collect();
                if cpu.rect.width() != reference.width() || cpu.rect.height() != reference.height() {
                    failures.push(format!("{name}\tsize mismatch"));
                    continue;
                }
                let (iou, mae) = parity(&cpu, &rgba);
                if iou >= 0.97 && mae <= 0.02 {
                    passed += 1;
                } else {
                    failures.push(format!("{name}\tIoU={iou:.6} premultiplied RGB MAE={mae:.6}"));
                }
            }
            Err(e) => failures.push(format!("{name}\t{e}")),
        }
    }
    eprintln!("resvg subset: {passed}/{total}, failures:\n{}", failures.join("\n"));
    assert_eq!(total, 400);
    assert!(passed * 10 >= total * 9, "resvg pass rate {passed}/{total}");
    let ratchet = std::fs::read_to_string(base.join("ratchet.tsv")).unwrap_or_default();
    let names: Vec<_> = ratchet.lines().skip(1).filter_map(|r| r.split_once('\t').map(|p| p.0)).collect();
    let mut actual: Vec<_> = failures.iter().map(|r| r.split('\t').next().unwrap()).collect();
    let mut expected = names;
    actual.sort_unstable();
    expected.sort_unstable();
    assert_eq!(actual, expected, "ratchet must list every failure and remove resolved cases");
}
#[test]
fn svg_import_external_corpus_no_panics() {
    let base = fixtures().join("external");
    let cases = std::fs::read_to_string(base.join("cases.tsv")).unwrap();
    let mut count = 0;
    for row in cases.lines().skip(1) {
        let name = row.split('\t').next().unwrap();
        let source = std::fs::read(base.join(name)).unwrap();
        let result = std::panic::catch_unwind(|| photocraft_svg::import(&source, &ImportOptions::default()));
        let scene = result.unwrap_or_else(|_| panic!("panic: {name}")).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(!scene.layers.is_empty(), "{name}");
        count += 1;
    }
    assert_eq!(count, 40);
}

#[test]
fn svg_import_external_maps_and_illustrations_no_panics() {
    let base = fixtures().join("external/commons");
    let cases = std::fs::read_to_string(base.join("cases.tsv")).unwrap();
    let mut count = 0;
    for row in cases.lines().skip(1) {
        let name = row.split('\t').next().unwrap();
        let source = std::fs::read(base.join(name)).unwrap();
        let result = std::panic::catch_unwind(|| photocraft_svg::import(&source, &ImportOptions::default())).unwrap_or_else(|_| panic!("panic: {name}"));
        match result {
            Ok(scene) => eprintln!(
                "external stress {name}: imported {} layers; warnings {:?}",
                scene.layers.len(),
                scene.warnings.iter().map(|w| &w.feature).collect::<Vec<_>>()
            ),
            Err(error) => eprintln!("external stress {name}: bounded rejection: {error}"),
        }
        count += 1;
    }
    assert_eq!(count, 5);
}
