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

#[test]
fn editable_font_family_roundtrips_css_string_escapes() {
    use photocraft_doc::vector_design::exchange::ExportScope;
    use photocraft_svg::export::{ExportOptions, export};
    for family in ["Mom's Typewriter", r"a'b\c", r"back\slash", r"trailing\", "'quoted'", "comma, family", "Inter"] {
        let mut doc = Document::new("Font family", photocraft_doc::Size::new(128, 128), ColorMode::Rgb, SampleType::U8);
        doc.layers
            .push(Layer::new("Text", LayerContent::Text(photocraft_doc::TextLayer { text: "H".into(), font_family: family.into(), ..Default::default() })));
        let output = export(&doc, &ExportScope::Document, &ExportOptions { text_as_curves: false, ..Default::default() }).unwrap();
        let scene = photocraft_svg::import(&output.bytes, &ImportOptions::default()).unwrap();
        let layers = flat(&scene.layers);
        let text = layers.iter().find_map(|layer| if let LayerContent::Text(text) = &layer.content { Some(text) } else { None }).unwrap();
        assert_eq!(text.text.trim(), "H");
        let mut offset = 0;
        let style = text
            .runs
            .iter()
            .find_map(|run| {
                let end = offset + run.len;
                let contains_glyph = text.text[offset..end].contains('H');
                offset = end;
                contains_glyph.then_some(&run.style)
            })
            .unwrap();
        assert_eq!(style.font_family, family);
    }
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

#[test]
fn review_additional_svg_gzip_has_separate_input_and_expanded_limits() {
    use std::io::Write;
    let mut source = String::from("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"8\" height=\"8\">");
    // Expanded XML and its single tiled cache share the bounded memory budget.
    source.push_str(&" ".repeat(1024 * 1024 + 4096));
    source.push_str("<path id=\"a\" data-name=\"Named path\" d=\"M0 0L8 0L8 8Z\"/></svg>");
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(source.as_bytes()).unwrap();
    let compressed = encoder.finish().unwrap();
    let mut options = ImportOptions::default();
    options.limits.bytes = compressed.len();
    options.limits.decompressed_bytes = source.len() as u64;
    let scene = photocraft_svg::import(&compressed, &options).unwrap();
    assert_eq!(flat(&scene.layers)[0].name, "Named path");
    options.limits.decompressed_bytes -= 1;
    assert!(photocraft_svg::import(&compressed, &options).is_err());
}

#[test]
fn svg_import_external_doctype_is_inert_and_matches_ordinary_svg() {
    let body = "<svg xmlns='http://www.w3.org/2000/svg' width='12' height='8'><g id='group'><rect id='box' x='1' y='1' width='4' height='3' fill='red'/><path id='path' d='M7 1h3v4h-3Z' fill='blue'/></g></svg>";
    let plain = photocraft_svg::import(body.as_bytes(), &ImportOptions::default()).unwrap();
    let names: Vec<_> = flat(&plain.layers).iter().map(|layer| (layer.name.clone(), layer.content.kind_name())).collect();
    let expected = photocraft_compose::flatten(&document(plain)).to_rgba8().pixels;
    for declaration in [
        "<!DOCTYPE svg PUBLIC \"-//W3C//DTD SVG 1.1//EN\" \"http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd\">",
        "<!DOCTYPE svg SYSTEM 'file:///this-file-must-never-be-read.dtd'>",
        "<!DOCTYPE svg SYSTEM 'https://example.invalid/never-fetch[1].dtd'>",
        "<!DOCTYPE svg>",
    ] {
        let source = format!("\u{feff}<?xml version='1.0'?>\n<!-- prolog --><?hint value='<!DOCTYPE inert>'?>\n{declaration}\n<!-- after -->{body}");
        let admitted = photocraft_svg::import::source(source.as_bytes(), &ImportOptions::default()).unwrap();
        assert!(!admitted.contains(declaration));
        assert!(admitted.contains("<?hint value='<!DOCTYPE inert>'?>"));
        let scene = photocraft_svg::import(source.as_bytes(), &ImportOptions::default()).unwrap();
        assert_eq!(scene.size, photocraft_doc::Size::new(12, 8));
        assert_eq!(flat(&scene.layers).iter().map(|layer| (layer.name.clone(), layer.content.kind_name())).collect::<Vec<_>>(), names);
        assert_eq!(photocraft_compose::flatten(&document(scene)).to_rgba8().pixels, expected);
    }
    let prefixed = "<!DOCTYPE s:svg SYSTEM 'never.dtd'><s:svg xmlns:s='http://www.w3.org/2000/svg' width='2' height='2'><s:rect width='2' height='2'/></s:svg>";
    assert!(photocraft_svg::import(prefixed.as_bytes(), &ImportOptions::default()).is_ok());
    // Literal occurrences outside a prolog declaration are not rewritten.
    let literal = "<!-- <!DOCTYPE svg [not a declaration]> --><svg xmlns='http://www.w3.org/2000/svg' width='2' height='2'><desc><![CDATA[<!DOCTYPE svg [still text]>]]></desc><rect width='2' height='2' data-note='&lt;!DOCTYPE quoted&gt;'/></svg>";
    assert_eq!(photocraft_svg::import::source(literal.as_bytes(), &ImportOptions::default()).unwrap(), literal);
    assert!(photocraft_svg::import(literal.as_bytes(), &ImportOptions::default()).is_ok());
}

#[test]
fn svg_import_doctype_rejects_subsets_entities_and_malformed_prologs() {
    let body = "<svg xmlns='http://www.w3.org/2000/svg' width='2' height='2'/>";
    for declaration in [
        "<!DOCTYPE svg []>",
        "<!DOCTYPE svg [<!ENTITY x 'expanded'>]>",
        "<!DOCTYPE svg SYSTEM 'never.dtd' [<!ENTITY % remote SYSTEM 'https://example.invalid/a.dtd'>%remote;]>",
        "<!DOCTYPE svg PUBLIC '-//W3C//DTD SVG 1.1//EN'>",
        "<!DOCTYPE svg SYSTEM unquoted>",
        "<!DOCTYPE svg SYSTEM 'unclosed>",
        "<!DOCTYPE svg SYSTEM 'a.dtd' garbage>",
        "<!DOCTYPE svg OTHER 'a.dtd'>",
        "<!DOCTYPE wrong SYSTEM 'a.dtd'>",
        "<!DOCTYPEsvg SYSTEM 'a.dtd'>",
        "<!DOCTYPE svg PUBLIC 'invalid\tpublic' 'a.dtd'>",
        "<!DOCTYPE svg SYSTEM 'invalid\0uri'>",
        "<!DOCTYPE svg SYSTEM 'a.dtd'><!DOCTYPE svg SYSTEM 'b.dtd'>",
    ] {
        let source = format!("{declaration}{body}");
        let start = Instant::now();
        assert!(photocraft_svg::import(source.as_bytes(), &ImportOptions::default()).is_err(), "{declaration}");
        assert!(start.elapsed() < Duration::from_secs(1));
    }
    let external_entity = "<!DOCTYPE svg SYSTEM 'https://example.invalid/entity.dtd'><svg xmlns='http://www.w3.org/2000/svg' width='2' height='2'><text>&notFetched;</text></svg>";
    assert!(photocraft_svg::import(external_entity.as_bytes(), &ImportOptions::default()).is_err());
    let late = format!("{body}<!DOCTYPE svg SYSTEM 'late.dtd'>");
    assert!(photocraft_svg::import(late.as_bytes(), &ImportOptions::default()).is_err());
}

#[test]
fn svg_import_doctype_does_not_bypass_source_or_decompression_limits() {
    use std::io::Write;
    let source = "<!DOCTYPE svg SYSTEM 'never.dtd'><svg xmlns='http://www.w3.org/2000/svg' width='2' height='2'><rect width='2' height='2'/></svg>";
    let mut options = ImportOptions::default();
    options.limits.bytes = source.len() - 1;
    assert!(matches!(photocraft_svg::import(source.as_bytes(), &options), Err(photocraft_svg::ImportError::Limit("input byte"))));
    let mut zipped = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    zipped.write_all(source.as_bytes()).unwrap();
    let zipped = zipped.finish().unwrap();
    let mut options = ImportOptions::default();
    assert!(photocraft_svg::import(&zipped, &options).is_ok());
    options.limits.decompressed_bytes = source.len() as u64 - 1;
    assert!(matches!(photocraft_svg::import(&zipped, &options), Err(photocraft_svg::ImportError::Limit("decompressed byte"))));
}

#[test]
fn svg_import_duplicate_use_and_text_ids_follow_first_binding() {
    let source = r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"><defs>
        <g id="same"><rect width="4" height="4" fill="red"/></g>
        <g id="same"><rect width="8" height="4" fill="blue"/></g><g id="same"/>
        <use id="alias" href="#same"/><use id="alias" href="#missing"/>
        </defs><use href="#alias" x="2" y="2"/></svg>"##;
    let scene = photocraft_svg::import(source.as_bytes(), &ImportOptions::default()).unwrap();
    for id in ["same", "alias"] {
        assert_eq!(scene.warnings.iter().filter(|warning| warning.feature == "duplicate-id" && warning.object.as_deref() == Some(id)).count(), 1);
    }
    let pixels = photocraft_compose::flatten(&document(scene));
    assert_eq!(pixels.get(3, 3), [1., 0., 0., 1.]);
    assert_eq!(pixels.get(7, 3)[3], 0., "second duplicate's wider blue shape is not the use target");
    let text = r##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20"><defs><text id="t" y="12" font-family="Inter">A</text><text id="t" y="12" font-family="Inter">B</text></defs><use href="#t"/></svg>"##;
    let scene = photocraft_svg::import(text.as_bytes(), &ImportOptions::default()).unwrap();
    let texts: Vec<_> = flat(&scene.layers)
        .into_iter()
        .filter_map(|layer| if let LayerContent::Text(text) = &layer.content { Some(text.text.as_str()) } else { None })
        .collect();
    assert_eq!(texts, ["A"]);
}

#[test]
fn svg_import_duplicate_resources_match_downstream_last_recognized_binding() {
    let source = r##"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><defs>
        <linearGradient id="paint"><stop stop-color="red"/><stop offset="1" stop-color="red"/></linearGradient>
        <linearGradient id="paint"><stop stop-color="blue"/><stop offset="1" stop-color="blue"/></linearGradient>
        <clipPath id="clip"><rect y="4" width="4" height="4"/></clipPath>
        <clipPath id="clip"><rect x="4" y="4" width="4" height="4"/></clipPath>
        </defs><rect width="8" height="3" fill="url(#paint)"/><rect y="4" width="8" height="4" fill="lime" clip-path="url(#clip)"/></svg>"##;
    let scene = photocraft_svg::import(source.as_bytes(), &ImportOptions::default()).unwrap();
    assert_eq!(scene.warnings.iter().filter(|warning| warning.feature == "duplicate-id").count(), 2);
    let pixels = photocraft_compose::flatten(&document(scene));
    assert_eq!(pixels.get(2, 1), [0., 0., 1., 1.]);
    assert_eq!(pixels.get(1, 5)[3], 0.);
    assert_eq!(pixels.get(6, 5), [0., 1., 0., 1.]);
    // Independent downstream renderer confirms the observed 0.45.1 resource policy.
    let tree = usvg::Tree::from_str(source, &usvg::Options::default()).unwrap();
    let mut reference = resvg::tiny_skia::Pixmap::new(8, 8).unwrap();
    resvg::render(&tree, usvg::Transform::identity(), &mut reference.as_mut());
    for (x, y) in [(2, 1), (1, 5), (6, 5)] {
        let expected = reference.pixels()[(y * 8 + x) as usize];
        let actual = pixels.get(x, y);
        assert_eq!(actual, [expected.red(), expected.green(), expected.blue(), expected.alpha()].map(|value| f32::from(value) / 255.));
    }
}

#[test]
fn svg_import_duplicate_references_cannot_hide_cycles_or_expansion() {
    let cycle = r##"<svg xmlns="http://www.w3.org/2000/svg"><defs><g id="loop"><use href="#loop"/></g><g id="loop"><rect width="1" height="1"/></g></defs><use href="#loop"/></svg>"##;
    assert!(matches!(photocraft_svg::import(cycle.as_bytes(), &ImportOptions::default()), Err(photocraft_svg::ImportError::Invalid(_))));
    let paint_cycle = r##"<svg xmlns="http://www.w3.org/2000/svg"><defs><pattern id="p" width="1" height="1"><rect width="1" height="1"/></pattern><pattern id="p" width="1" height="1"><rect width="1" height="1" fill="url(#p)"/></pattern></defs><rect width="10" height="10" fill="url(#p)"/></svg>"##;
    assert!(matches!(photocraft_svg::import(paint_cycle.as_bytes(), &ImportOptions::default()), Err(photocraft_svg::ImportError::Invalid(_))));
    let mut expansion = String::from("<svg xmlns='http://www.w3.org/2000/svg'><defs><g id='a0'><rect width='1' height='1'/></g><g id='a0'/>");
    for i in 1..16 {
        expansion.push_str(&format!("<g id='a{i}'><use href='#a{}'/><use href='#a{}'/></g><g id='a{i}'/>", i - 1, i - 1));
    }
    expansion.push_str("</defs><use href='#a15'/></svg>");
    let start = Instant::now();
    assert!(matches!(photocraft_svg::import(expansion.as_bytes(), &ImportOptions::default()), Err(photocraft_svg::ImportError::Limit(_))));
    assert!(start.elapsed() < Duration::from_secs(1));
    // Last raw XML id is unknown to usvg, so it cannot hide the earlier pattern's cost.
    let resource = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg"><defs><pattern id="p" width="1" height="1">{}</pattern><not-an-svg-element id="p"/></defs><rect width="2" height="2" fill="url(#p)"/></svg>"##,
        "<rect width='1' height='1'/>".repeat(100)
    );
    let mut options = ImportOptions::default();
    options.limits.objects = 150;
    assert!(matches!(photocraft_svg::import(resource.as_bytes(), &options), Err(photocraft_svg::ImportError::Limit(_))));
    let marker = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg"><defs>{}</defs>{}</svg>"##,
        "<marker id='m'><rect width='1' height='1'/></marker>".repeat(40),
        "<path d='M0 0L1 1' marker-start='url(#m)'/>".repeat(40)
    );
    // The selected marker is counted once per reference (243 ordinary expanded
    // nodes here), before the additional per-vertex marker work is charged.
    options.limits.objects = 400;
    assert!(matches!(photocraft_svg::import(marker.as_bytes(), &options), Err(photocraft_svg::ImportError::Limit("expanded marker"))));
}

#[test]
fn svg_import_namespace_less_whitespace_use_cannot_hide_duplicate_expansion() {
    fn graph(depth: usize) -> String {
        // No default namespace is declared: children have namespace None.
        // This differs from xmlns='', represented by roxmltree as Some("").
        let mut source = String::from(
            "<s:svg xmlns:s='http://www.w3.org/2000/svg' width='2' height='2'><defs><g id='a0'><rect width='1' height='1' fill='red'/></g><g id='a0'/>",
        );
        for i in 1..=depth {
            source.push_str(&format!("<g id='a{i}'><use href=' #a{} '/><use href=' #a{} '/></g><g id='a{i}'/>", i - 1, i - 1));
        }
        source.push_str(&format!("</defs><use href=' #a{depth} '/></s:svg>"));
        source
    }
    let mut options = ImportOptions::default();
    options.limits.objects = 1000;
    let source = graph(2);
    let empty_namespace = source.replace("<defs>", "<defs xmlns=''>");
    for (source, expected_pixel) in [(&source, [1., 0., 0., 1.]), (&empty_namespace, [0.; 4])] {
        let scene = photocraft_svg::import(source.as_bytes(), &options).unwrap();
        let pixels = photocraft_compose::flatten(&document(scene));
        assert_eq!(pixels.get(0, 0), expected_pixel);
        let tree = usvg::Tree::from_str(source, &usvg::Options::default()).unwrap();
        let mut reference = resvg::tiny_skia::Pixmap::new(2, 2).unwrap();
        resvg::render(&tree, usvg::Transform::identity(), &mut reference.as_mut());
        for y in 0..2 {
            for x in 0..2 {
                let expected = reference.pixels()[(y * 2 + x) as usize];
                assert_eq!(pixels.get(x, y), [expected.red(), expected.green(), expected.blue(), expected.alpha()].map(|v| f32::from(v) / 255.));
            }
        }
    }
    options.limits.objects = 64;
    let hostile = graph(9);
    let xml = roxmltree::Document::parse(&hostile).unwrap();
    let raw_count = xml.descendants().filter(|node| node.is_element()).count();
    assert!(raw_count < options.limits.objects);
    assert!(xml.root_element().descendants().skip(1).filter(|node| node.is_element()).all(|node| node.tag_name().namespace().is_none()));
    let start = Instant::now();
    let result = photocraft_svg::import(hostile.as_bytes(), &options);
    assert!(
        matches!(&result, Err(photocraft_svg::ImportError::Limit("expanded object"))),
        "raw={raw_count}, result={}",
        match &result {
            Ok(scene) => format!("accepted {} layers", scene.layers.len()),
            Err(error) => error.to_string(),
        }
    );
    assert!(start.elapsed() < Duration::from_secs(1));
}

#[test]
fn svg_import_shadowed_resource_edges_do_not_create_false_cycles() {
    let red = "<stop stop-color='red'/><stop offset='1' stop-color='red'/>";
    for ignored in [
        "",
        "<not-svg id='a' href='#b'><linearGradient id='a' href='#b'/></not-svg>",
        "<f:g xmlns:f='urn:foreign' id='a' href='#b'><linearGradient id='a' href='#b'/></f:g>",
        "<style id='a'/>",
    ] {
        let source = format!(
            "<svg xmlns='http://www.w3.org/2000/svg' width='4' height='4'><defs><linearGradient id='a' href=' #b '/><linearGradient id='a'>{red}</linearGradient><linearGradient id='b' href=' #a '/>{ignored}</defs><rect width='4' height='4' fill='url(#b)'/></svg>"
        );
        let scene = photocraft_svg::import(source.as_bytes(), &ImportOptions::default()).unwrap();
        assert_eq!(photocraft_compose::flatten(&document(scene)).get(1, 1), [1., 0., 0., 1.]);
    }
    // ID attributes use the first admitted namespace spelling on each element.
    let source = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='8' height='4'><defs><linearGradient id='a'>{red}</linearGradient><linearGradient xml:id='a' id='unused'><stop stop-color='blue'/><stop offset='1' stop-color='blue'/></linearGradient></defs><rect width='4' height='4' fill='url(#a)'/><rect x='4' width='4' height='4' fill='url(#unused) lime'/></svg>"
    );
    let pixels = photocraft_compose::flatten(&document(photocraft_svg::import(source.as_bytes(), &ImportOptions::default()).unwrap()));
    assert_eq!(pixels.get(1, 1), [0., 0., 1., 1.]);
    assert_eq!(pixels.get(5, 1), [0., 1., 0., 1.]);
    // IDs on ordinary use clones must not override later original resource bindings.
    let source = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='4' height='4'><defs><g id='source'><linearGradient id='a'><stop stop-color='blue'/></linearGradient></g><linearGradient id='a'>{red}</linearGradient></defs><use href='#source'/><rect width='4' height='4' fill='url(#a)'/></svg>"
    );
    let scene = photocraft_svg::import(source.as_bytes(), &ImportOptions::default()).unwrap();
    assert_eq!(photocraft_compose::flatten(&document(scene)).get(1, 1), [1., 0., 0., 1.]);
}

#[test]
fn svg_import_resolved_duplicate_id_corpus_cases_keep_their_art() {
    let base = fixtures().join("resvg");
    for name in [
        "painting/context/with-gradient-and-gradient-transform.svg",
        "painting/stroke/gradient-with-objectBoundingBox-and-fallback-on-lines.svg",
        "painting/stroke/gradient-with-objectBoundingBox-on-path-without-a-bbox-2.svg",
    ] {
        let source = std::fs::read(base.join(name)).unwrap();
        let options = ImportOptions { dpi: 96., ..Default::default() };
        let scene = photocraft_svg::import(&source, &options).unwrap();
        assert!(scene.warnings.iter().any(|warning| warning.feature == "duplicate-id"));
        let cpu = photocraft_compose::flatten(&document(scene));
        let tree = usvg::Tree::from_data(&source, &usvg::Options { dpi: 96., ..Default::default() }).unwrap();
        let size = tree.size().to_int_size();
        let mut reference = resvg::tiny_skia::Pixmap::new(size.width(), size.height()).unwrap();
        resvg::render(&tree, usvg::Transform::identity(), &mut reference.as_mut());
        if name.contains("/stroke/") {
            for (x, y) in [(100, 40), (40, 100), (100, 100)] {
                let pixel = cpu.get(x, y);
                assert!(
                    pixel[3] > 0.99 && pixel[0] < 0.001 && pixel[2] < 0.001 && (0.49..0.51).contains(&pixel[1]),
                    "green fallback/underlying stroke missing: {name} {pixel:?}"
                );
                let expected = reference.pixels()[(y as u32 * size.width() + x as u32) as usize];
                assert_eq!([expected.red(), expected.green(), expected.blue(), expected.alpha()], [0, 128, 0, 255]);
            }
            assert_eq!(cpu.get(50, 50)[3], 0.);
        } else {
            let mut compared = 0;
            let mut error = 0.0;
            for y in (20..180).step_by(10) {
                for x in (20..180).step_by(10) {
                    let at = (y * size.width() as i32 + x) as usize;
                    let pixel = reference.pixels()[at];
                    let channels = [pixel.red(), pixel.green(), pixel.blue()];
                    if pixel.alpha() != 255 || channels.iter().max().unwrap() - channels.iter().min().unwrap() < 32 {
                        continue;
                    }
                    if [-1, 0, 1]
                        .into_iter()
                        .any(|dy| [-1, 0, 1].into_iter().any(|dx| reference.pixels()[((y + dy) * size.width() as i32 + x + dx) as usize].alpha() != 255))
                    {
                        continue;
                    }
                    let actual = cpu.get(x, y);
                    assert!(actual[3] > 0.99, "context-gradient interior art vanished at {x},{y}");
                    error += actual[..3].iter().zip(channels).map(|(value, expected)| (*value - f32::from(expected) / 255.).abs()).sum::<f32>();
                    compared += 1;
                }
            }
            assert!(compared >= 10, "non-frame gradient art must be checked");
            assert!(error / ((compared * 3) as f32) < 0.08, "context-gradient colour mismatch");
        }
    }
}

#[test]
fn svg_import_removed_text_descendant_ids_cannot_hide_duplicate_expansion() {
    fn graph(depth: usize) -> String {
        let mut source = String::from("<svg xmlns='http://www.w3.org/2000/svg' width='2' height='2'><defs><text id='retained'>");
        for i in 0..=depth {
            source.push_str(&format!("<tspan id='a{i}'/>"));
        }
        source.push_str("</text><g id='a0'><rect width='1' height='1' fill='red'/></g>");
        for i in 1..=depth {
            source.push_str(&format!("<g id='a{i}'><use href='#a{}'/><use href='#a{}'/></g>", i - 1, i - 1));
        }
        source.push_str(&format!("</defs><use href='#a{depth}'/></svg>"));
        source
    }
    let mut options = ImportOptions::default();
    options.limits.objects = 64;
    let small = graph(1);
    let scene = photocraft_svg::import(small.as_bytes(), &options).unwrap();
    let actual = photocraft_compose::flatten(&document(scene)).get(0, 0);
    assert_eq!(actual, [1., 0., 0., 1.]);
    // Independent renderer receives the equivalent prepared wrapper, with the
    // removed tspan IDs absent; it must select the real groups that follow.
    let start = small.find("<text").unwrap();
    let end = small.find("</text>").unwrap() + "</text>".len();
    let prepared = format!("{}<g id='retained'/>{}", &small[..start], &small[end..]);
    let tree = usvg::Tree::from_str(&prepared, &usvg::Options::default()).unwrap();
    let mut reference = resvg::tiny_skia::Pixmap::new(2, 2).unwrap();
    resvg::render(&tree, usvg::Transform::identity(), &mut reference.as_mut());
    assert_eq!(reference.pixels()[0].red(), 255);
    assert_eq!(reference.pixels()[0].alpha(), 255);
    let hostile = graph(8);
    let xml = roxmltree::Document::parse(&hostile).unwrap();
    assert!(xml.descendants().filter(|node| node.is_element()).count() < options.limits.objects);
    let start = Instant::now();
    assert!(matches!(photocraft_svg::import(hostile.as_bytes(), &options), Err(photocraft_svg::ImportError::Limit("expanded object"))));
    assert!(start.elapsed() < Duration::from_secs(1));
}

#[test]
fn svg_text_content_respects_display_visibility_opacity_and_metadata() {
    for (body, expected, alphas) in [
        ("A<tspan display='none'>OMIT</tspan>B<title>TOOLTIP</title><desc>DESCRIPTION</desc>", "AB", vec![1., 1.]),
        ("A<tspan class='gone'>OMIT</tspan>B", "AB", vec![1., 1.]),
        ("A<tspan visibility='hidden'>HIDDEN</tspan>B", "AHIDDENB", vec![1., 0., 1.]),
        ("A<tspan opacity='0'>HIDDEN<tspan opacity='1'>ALSOHIDDEN</tspan></tspan>B", "AHIDDENALSOHIDDENB", vec![1., 0., 0., 1.]),
        ("<tspan visibility='hidden'>H<tspan visibility='visible'>V</tspan>I</tspan>", "HVI", vec![0., 1., 0.]),
    ] {
        let source = format!(
            "<svg xmlns='http://www.w3.org/2000/svg' width='400' height='100'><style>.gone{{display:none}}</style><text x='10' y='40' font-family='Inter'>{body}</text></svg>"
        );
        let scene = photocraft_svg::import(source.as_bytes(), &ImportOptions::default()).unwrap();
        let layers = flat(&scene.layers);
        let text = layers.iter().find_map(|layer| if let LayerContent::Text(text) = &layer.content { Some(text) } else { None }).unwrap();
        assert_eq!(text.text, expected);
        assert_eq!(text.runs.iter().map(|run| run.style.color.alpha).collect::<Vec<_>>(), alphas);
    }
    let source = "<svg xmlns='http://www.w3.org/2000/svg' width='200' height='80'><text visibility='hidden' x='10' y='40' font-family='Inter'>H<tspan visibility='visible'>V</tspan>I</text></svg>";
    let scene = photocraft_svg::import(source.as_bytes(), &ImportOptions::default()).unwrap();
    let layers = flat(&scene.layers);
    let text = layers.iter().find_map(|layer| if let LayerContent::Text(text) = &layer.content { Some(text) } else { None }).unwrap();
    assert_eq!(text.text, "HVI");
    assert_eq!(text.runs.iter().map(|run| run.style.color.alpha).collect::<Vec<_>>(), [0., 1., 0.]);
    let rendered = photocraft_compose::flatten(&document(scene));
    assert!(rendered.px.iter().any(|pixel| pixel[3] > 0.9), "a visible child must survive its hidden text parent");
}

#[test]
fn svg_nested_fractional_text_opacity_preserves_grouped_overlap_and_hidden_runs() {
    use photocraft_doc::{
        Affine, Color, Size, TextLayer,
        text::{CharStyle, TextRun},
    };
    let colored = "H<tspan fill='blue'>H</tspan>H";
    for (body, opacity, hidden) in [
        (format!("<tspan opacity='.5'>{colored}</tspan>"), 0.5, false),
        (format!("<tspan opacity='.5'><tspan opacity='.5'>{colored}</tspan></tspan>"), 0.25, false),
        ("<tspan opacity='.5'>H<tspan fill='blue' visibility='hidden'>H</tspan>H</tspan>".into(), 0.5, true),
    ] {
        let source = format!(
            "<svg xmlns='http://www.w3.org/2000/svg' width='256' height='128'><text x='100' y='90' font-family='Inter' font-size='64' letter-spacing='-41.6' fill='red'>{body}</text></svg>"
        );
        let scene = photocraft_svg::import(source.as_bytes(), &ImportOptions::default()).unwrap();
        assert!(scene.warnings.iter().any(|warning| warning.feature == "text-opacity-outlines"));
        assert!(flat(&scene.layers).iter().any(|layer| matches!(layer.content, LayerContent::Shape(_))));
        assert!(!flat(&scene.layers).iter().any(|layer| matches!(layer.content, LayerContent::Text(_))));
        let got = photocraft_compose::flatten(&document(scene));
        let mut reference = Document::new("group reference", Size::new(256, 128), ColorMode::Rgb, SampleType::F32);
        let mut text = TextLayer { text: "HHH".into(), transform: Affine::translate(100., 90.), ..Default::default() };
        text.runs = (0..3)
            .map(|i| {
                let mut color = if i == 1 { Color::rgb(0., 0., 1.) } else { Color::rgb(1., 0., 0.) };
                if hidden && i == 1 {
                    color.alpha = 0.;
                }
                TextRun { len: 1, style: CharStyle { size_pt: 64., tracking: -650., color, ..Default::default() } }
            })
            .collect();
        photocraft_text::TextEngine::new().render_layer(&mut text, 72., reference.pixel_format());
        let mut layer = Layer::new("grouped native text", LayerContent::Text(text));
        layer.opacity = opacity;
        reference.layers.push(layer);
        let expected = photocraft_compose::flatten(&reference);
        let mean: f32 = got
            .px
            .iter()
            .zip(&expected.px)
            .map(|(a, b)| {
                (0..4)
                    .map(|c| {
                        let premul = |p: &[f32; 4]| if c == 3 { p[3] } else { p[c] * p[3] };
                        (premul(a) - premul(b)).abs()
                    })
                    .sum::<f32>()
            })
            .sum::<f32>()
            / (got.px.len() * 4) as f32;
        assert!(mean < 0.002, "opacity={opacity} hidden={hidden} mean={mean}");
        assert!(got.px.iter().all(|pixel| pixel[3] <= opacity + 1e-6), "apply subtree opacity once, never 0.75 from two half-alpha glyphs");
        assert!((got.get(147, 67)[3] - opacity).abs() < 1e-5);
        let mut limited = ImportOptions::default();
        limited.limits.segments = 10;
        assert!(matches!(photocraft_svg::import(source.as_bytes(), &limited), Err(photocraft_svg::ImportError::Limit(_))));
        assert!(matches!(
            photocraft_svg::import_with(source.as_bytes(), &ImportOptions::default(), None, &AtomicBool::new(true)),
            Err(photocraft_svg::ImportError::Cancelled)
        ));
    }
    // Ordinary text and root opacity retain editable text with the existing group semantics.
    let source = "<svg xmlns='http://www.w3.org/2000/svg' width='200' height='80'><text opacity='.5' x='10' y='50' font-family='Inter'>AB</text></svg>";
    let scene = photocraft_svg::import(source.as_bytes(), &ImportOptions::default()).unwrap();
    assert!(flat(&scene.layers).iter().any(|layer| matches!(layer.content, LayerContent::Text(_))));
    assert!(!scene.warnings.iter().any(|warning| warning.feature == "text-opacity-outlines"));
}

/// Hostile structure from VectorCraft's import fuzzing (MIT OR Apache-2.0): groups nested deep
/// enough to overflow a recursive reader, and a descendant selector whose leftmost part never
/// matches over a deep chain, which backtracks combinatorially without memoization. Both finish
/// quickly, imported or refused.
#[test]
fn svg_deep_nesting_and_backtracking_selectors_finish_quickly() {
    use photocraft_doc::vector_design::exchange::ImportOptions;
    let start = std::time::Instant::now();
    for n in [40, 100_000] {
        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">{}<rect width="5" height="5"/>{}</svg>"#,
            "<g>".repeat(n),
            "</g>".repeat(n)
        );
        let r = photocraft_svg::import(svg.as_bytes(), &ImportOptions::default());
        assert_eq!(r.is_err(), n > 64, "{n} nested groups: {:?}", r.as_ref().err());
    }
    let unread = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><!--{0}--><![CDATA[{0}]]><rect width="5" height="5"/></svg>"#,
        "<g>".repeat(100_000)
    );
    assert!(photocraft_svg::import(unread.as_bytes(), &ImportOptions::default()).is_ok(), "markup in comments and CDATA isn't nesting");
    let chain = 60;
    let selector = format!("x {} rect", "g ".repeat(30));
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><style>{selector} {{ fill: red }}</style>{}<rect width="5" height="5"/>{}</svg>"#,
        "<g>".repeat(chain),
        "</g>".repeat(chain)
    );
    let _ = photocraft_svg::import(svg.as_bytes(), &ImportOptions::default());
    assert!(start.elapsed().as_secs() < 20, "{:?}", start.elapsed());
}
