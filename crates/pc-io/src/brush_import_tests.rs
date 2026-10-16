use super::*;
use photocraft_paint::Pattern;
const GBR: &[u8] = include_bytes!("../tests/fixtures/brushes/rubberduck.gbr");
const KPP: &[u8] = include_bytes!("../tests/fixtures/brushes/krita-basic.kpp");
const BUNDLE: &[u8] = include_bytes!("../tests/fixtures/brushes/krita-small.bundle");
const BRUSH: &[u8] = include_bytes!("../tests/fixtures/brushes/plain-stitch.brush");
const MYB: &[u8] = include_bytes!("../tests/fixtures/brushes/charcoal.myb");
fn load(b: &[u8], ext: &str) -> BrushImport {
    read(b, ext, "Fixture", &Interrupt::NONE).unwrap()
}
fn gbr(version: u32, rgba: bool) -> Vec<u8> {
    let base = if version == 1 { 20 } else { 28 };
    let mut v = Vec::new();
    for n in [base + 2, version, 2, 2, if rgba { 4 } else { 1 }] {
        v.extend_from_slice(&n.to_be_bytes());
    }
    if version == 2 {
        v.extend_from_slice(b"GIMP");
        v.extend_from_slice(&25u32.to_be_bytes());
    }
    v.extend_from_slice(b"A\0");
    if rgba {
        v.extend_from_slice(&[255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 64, 255, 255, 255, 0]);
    } else {
        v.extend_from_slice(&[0, 64, 128, 255]);
    }
    v
}
#[test]
fn brush_import_licensed_examples() {
    for (ext, b) in [("gbr", GBR), ("kpp", KPP), ("bundle", BUNDLE), ("brush", BRUSH), ("myb", MYB)] {
        let out = load(b, ext);
        assert_eq!(out.presets.len(), 1, "{ext}: {:?}", out.skipped);
        assert_eq!(out.presets[0].group, "Fixture");
        assert!(out.presets[0].brush.size > 0.0);
    }
    let b = load(BRUSH, "brush");
    let png = photocraft_paint::tile::b64_decode(&b.presets[0].brush.imported_images[0].png).unwrap();
    let zip = photocraft_format::zip::ZipReader::new(BRUSH).unwrap();
    assert_eq!(png, zip.read_by_name("Shape.png", MAX_ENTRY).unwrap(), "source artwork retained byte-exactly");
    assert_eq!(b.approximate(), 1);
    assert_eq!(load(KPP, "kpp").presets[0].brush.size, 40.0);
}
#[test]
fn brush_import_gbr_versions_and_rgba() {
    for version in [1, 2] {
        let out = load(&gbr(version, false), "gbr");
        let TipShape::Sampled(g) = &out.presets[0].brush.tip else { panic!() };
        assert_eq!(g.data, vec![0, 64 * 257, 128 * 257, 65535]);
        assert_eq!(out.approximate(), 0);
    }
    let out = load(&gbr(2, true), "gbr");
    let b = &out.presets[0].brush;
    let TipShape::Sampled(g) = &b.tip else { panic!() };
    assert_eq!(g.data, vec![65535, 128 * 257, 64 * 257, 0]);
    assert_eq!(common_rgba(b), [255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 64, 255, 255, 255, 0]);
}
fn common_rgba(b: &BrushSettings) -> Vec<u8> {
    image(&photocraft_paint::tile::b64_decode(&b.imported_images[0].png).unwrap()).unwrap().to_rgba8()
}
#[test]
fn brush_import_gih_cells_selection_and_vbr_shapes() {
    let mut pipe = b"Grid\n2 ncells:2 cellwidth:2 cellheight:2 cols:2 rows:1 step:40 dim:1 rank0:2 sel0:angular\n".to_vec();
    pipe.extend(gbr(2, false));
    pipe.extend(gbr(2, true));
    let out = load(&pipe, "gih");
    assert_eq!(out.presets.len(), 2);
    assert_eq!(out.approximate(), 2);
    assert_eq!(out.presets[0].brush.spacing, 0.4);
    assert_eq!(out.presets[0].brush.shape_dynamics.angle.control, photocraft_paint::Control::Direction);
    let mut tiles = Vec::new();
    for shape in ["circle", "square", "diamond"] {
        let b = format!("GIMP-VBR\n1.5\nShaped\n{shape}\n35\n20\n5\n0.7\n2.5\n17.5\n");
        let out = load(b.as_bytes(), "vbr");
        assert_eq!(out.presets[0].brush.spacing, 0.35);
        tiles.push(out.presets[0].brush.tip.clone());
    }
    assert_ne!(tiles[0], tiles[1]);
    assert_ne!(tiles[1], tiles[2]);
    assert!(read(b"GIMP-VBR\n1.5\nBad\ncircle\n10\nNaN\n2\n0.5\n1\n0", "vbr", "x", &Interrupt::NONE).is_err());
}
#[test]
fn brush_import_brushset_grain_and_partial_failure() {
    let original = photocraft_format::zip::ZipReader::new(BRUSH).unwrap();
    let mut zip = photocraft_format::zip::ZipWriter::new();
    for e in &original.entries {
        zip.add(&format!("one/{}", e.name), &original.read(e, MAX_ENTRY).unwrap()).unwrap();
    }
    let shape = original.read_by_name("Shape.png", MAX_ENTRY).unwrap();
    zip.add("one/Grain.png", &shape).unwrap();
    zip.add("bad/Brush.archive", b"broken").unwrap();
    let out = load(&zip.finish().unwrap(), "brushset");
    assert_eq!(out.presets.len(), 1);
    assert_eq!(out.skipped.len(), 1);
    let b = &out.presets[0].brush;
    assert!(b.texture.enabled);
    assert!(matches!(b.texture.pattern, Pattern::Tile(_)));
    assert_eq!(b.imported_images.len(), 2);
    assert_eq!(photocraft_paint::tile::b64_decode(&b.imported_images[1].png).unwrap(), shape);
}
#[test]
fn brush_import_hostile_containers_and_truncation() {
    for path in ["../Shape.png", "/Shape.png", "C:/Shape.png", "a\\b", "a/./b", "a//b"] {
        let mut z = photocraft_format::zip::ZipWriter::new();
        z.add(path, b"bad").unwrap();
        assert!(read(&z.finish().unwrap(), "brushset", "x", &Interrupt::NONE).is_err(), "{path}");
    }
    let mut z = photocraft_format::zip::ZipWriter::new();
    z.add("Shape.png", b"a").unwrap();
    z.add("shape.png", b"b").unwrap();
    assert!(read(&z.finish().unwrap(), "brush", "x", &Interrupt::NONE).is_err());
    for (ext, b) in [("gbr", gbr(2, false)), ("brush", BRUSH.to_vec()), ("bundle", BUNDLE.to_vec())] {
        for n in (0..b.len()).step_by((b.len() / 80).max(1)) {
            let _ = read(&b[..n], ext, "x", &Interrupt::NONE);
        }
    }
    let mut bad = gbr(2, false);
    bad[8..12].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(read(&bad, "gbr", "x", &Interrupt::NONE).is_err());
    assert_eq!(read(&[], "afbrushes", "x", &Interrupt::NONE).unwrap_err(), "Affinity brush files can't be read yet");
    let cancel = || true;
    let progress = |_| {};
    assert!(read(GBR, "gbr", "x", &Interrupt::new(&cancel, &progress)).is_err());
}
#[test]
fn brush_import_plist_limits_and_xml_entities() {
    let z = photocraft_format::zip::ZipReader::new(BRUSH).unwrap();
    let b = z.read_by_name("Brush.archive", MAX_ENTRY).unwrap();
    for n in 0..b.len() {
        assert!(super::super::brush_import_plist::Plist::parse(&b[..n]).is_err());
    }
    let mut bad = b.clone();
    let end = bad.len();
    bad[end - 24..end - 16].copy_from_slice(&u64::MAX.to_be_bytes());
    assert!(super::super::brush_import_plist::Plist::parse(&bad).is_err());
    assert!(super::super::brush_import_xml::parse("<!DOCTYPE a [<!ENTITY x SYSTEM 'file:///tmp/a'>]><a>&x;</a>").is_err());
    assert!(super::super::brush_import_xml::parse("<a><b></a>").is_err());
}
#[test]
fn brush_import_mypaint_pressure_and_density() {
    let out=load(br#"{"version":3,"settings":{"radius_logarithmic":{"base_value":3,"inputs":{"pressure":[[0,-1],[1,0]]}},"hardness":{"base_value":0.25},"opaque":{"base_value":0.75},"dabs_per_actual_radius":{"base_value":4}}}"#,"myb");
    let b = &out.presets[0].brush;
    assert!((b.size - 2.0 * 3.0f32.exp()).abs() < 1e-4);
    assert_eq!(b.spacing, 0.125);
    assert_eq!(b.hardness, 0.25);
    assert_eq!(b.opacity, 0.75);
    assert!(b.pressure_size);
    assert_eq!(out.approximate(), 1);
}

/// Read-only developer corpus; no downloaded packs are copied out or modified.
#[test]
#[ignore = "opt-in local artist pack corpus"]
fn brush_import_local_corpus() {
    let root = std::env::var("LOCAL_IMAGE_BRUSH_CORPUS")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from(std::env::var("HOME").unwrap()).join(".local/share/local-image-dev/test-assets/brushes"));
    assert!(root.exists(), "missing brush corpus");
    fn walk(p: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for e in std::fs::read_dir(p).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out)
            } else if p.extension().is_some_and(|e| EXTENSIONS.contains(&e.to_string_lossy().as_ref())) {
                out.push(p);
            }
        }
    }
    let mut files = Vec::new();
    walk(&root, &mut files);
    let mut counts = BTreeMap::<String, (usize, usize, usize, usize)>::new();
    for path in files {
        let ext = path.extension().unwrap().to_string_lossy().into_owned();
        let b = std::fs::read(&path).unwrap();
        let c = counts.entry(ext.clone()).or_default();
        c.0 += 1;
        match read(&b, &ext, "Corpus", &Interrupt::NONE) {
            Ok(out) => {
                c.1 += out.presets.len();
                c.2 += out.skipped.len();
            }
            Err(e) => {
                c.3 += 1;
                eprintln!("{}: {e}", path.display());
            }
        }
    }
    eprintln!("CORPUS (files, brushes, skipped, rejected): {counts:?}");
    assert!(counts.values().any(|c| c.1 > 0));
}

fn kpp_xml(xml: &str) -> Vec<u8> {
    let mut im = photocraft_codecs::Image::new(2, 2, photocraft_codecs::ChannelLayout::Rgba, photocraft_codecs::SampleType::U8).unwrap();
    im.meta.text.push(("preset".into(), xml.into()));
    photocraft_codecs::encode(&im, photocraft_codecs::Format::Png, &Default::default()).unwrap()
}
#[test]
fn brush_import_krita_mapping_engines_and_embedded_image_pipe() {
    let xml = r#"<Preset name="Sensors" paintopid="paintbrush"><param name="OpacityValue"><![CDATA[0.4]]></param><param name="FlowValue">0.7</param><param name="PressureSize">true</param><param name="PressureOpacity">true</param><param name="PressureFlow">true</param><param name="brush_definition"><![CDATA[<Brush type="auto_brush" spacing="0.3" angle="0.785398"><MaskGenerator diameter="60" ratio="0.5" type="circle" hfade="0.2" vfade="0.2"/></Brush>]]></param></Preset>"#;
    let out = load(&kpp_xml(xml), "kpp");
    let b = &out.presets[0].brush;
    assert_eq!(b.size, 60.0);
    assert_eq!(b.roundness, 0.5);
    assert_eq!(b.opacity, 0.4);
    assert_eq!(b.flow, 0.7);
    assert_eq!(b.spacing, 0.3);
    assert!((b.angle - 45.0).abs() < 0.001);
    assert!(b.pressure_size && b.pressure_opacity);
    assert_eq!(b.transfer.flow.control, photocraft_paint::Control::PenPressure);
    let out = load(&kpp_xml(&xml.replace("paintopid=\"paintbrush\"", "paintopid=\"colorsmudge\"")), "kpp");
    assert_eq!(out.approximate(), 1);
    let out = load(&kpp_xml(&xml.replace("paintopid=\"paintbrush\"", "paintopid=\"particle\"")), "kpp");
    assert!(out.presets.is_empty());
    assert!(out.skipped[0].contains("particle"));
    let mut pipe = b"Pipe\n2 ncells:2 step:20 dim:1 rank0:2 sel0:random\n".to_vec();
    pipe.extend(gbr(2, false));
    pipe.extend(gbr(2, false));
    let xml = format!(
        "<Preset name=\"Pipe preset\" paintopid=\"paintbrush\"><param name=\"brush_definition\"><![CDATA[<Brush type=\"gih_brush\" filename=\"pipe.gih\" scale=\"1\"/>]]></param><resources><resource type=\"brushes\" filename=\"pipe.gih\">{}</resource></resources></Preset>",
        photocraft_paint::tile::b64_encode(&pipe)
    );
    let out = load(&kpp_xml(&xml), "kpp");
    assert_eq!(out.presets.len(), 2);
    assert!(out.presets.iter().all(|p| matches!(p.brush.tip, TipShape::Sampled(_))));
}

#[test]
fn brush_import_procreate_sixteen_bit_shape_and_grain_are_exact() {
    let values = [1u16, 12345, 32768, 65535];
    let im = photocraft_codecs::Image::from_raw(
        2,
        2,
        photocraft_codecs::ChannelLayout::Gray,
        photocraft_codecs::SampleType::U16,
        values.iter().flat_map(|v| v.to_ne_bytes()).collect(),
    )
    .unwrap();
    let png = photocraft_codecs::encode(&im, photocraft_codecs::Format::Png, &Default::default()).unwrap();
    let original = photocraft_format::zip::ZipReader::new(BRUSH).unwrap();
    let mut z = photocraft_format::zip::ZipWriter::new();
    z.add("Brush.archive", &original.read_by_name("Brush.archive", MAX_ENTRY).unwrap()).unwrap();
    z.add("Shape.png", &png).unwrap();
    z.add("Grain.png", &png).unwrap();
    let out = load(&z.finish().unwrap(), "brush");
    let b = &out.presets[0].brush;
    let TipShape::Sampled(shape) = &b.tip else { panic!() };
    assert_eq!(shape.data, values);
    let Pattern::Tile(grain) = &b.texture.pattern else { panic!() };
    assert_eq!(grain.data, values);
    for image in &b.imported_images {
        assert_eq!(photocraft_paint::tile::b64_decode(&image.png).unwrap(), png);
    }
}

#[test]
fn dsfix_procreate_direction_preserves_scatter_and_randomise() {
    let original = photocraft_format::zip::ZipReader::new(BRUSH).unwrap();
    let shape = original.read_by_name("Shape.png", MAX_ENTRY).unwrap();
    for (archive, jitter) in [
        (include_bytes!("../tests/fixtures/brushes/dsfix-direction-false.archive").as_slice(), 0.35),
        (include_bytes!("../tests/fixtures/brushes/dsfix-direction-true.archive").as_slice(), 1.0),
    ] {
        let mut zip = photocraft_format::zip::ZipWriter::new();
        zip.add("Brush.archive", archive).unwrap();
        zip.add("Shape.png", &shape).unwrap();
        let imported = load(&zip.finish().unwrap(), "brush");
        let angle = &imported.presets[0].brush.shape_dynamics.angle;
        assert_eq!(angle.control, photocraft_paint::Control::Direction);
        assert_eq!(angle.jitter, jitter);
    }
}

#[test]
fn dsfix_krita_large_finite_radian_angle_stays_finite() {
    let xml = r#"<Preset name="Rotation" paintopid="paintbrush"><param name="brush_definition"><![CDATA[<Brush type="auto_brush" angle="3e38"><MaskGenerator diameter="10" ratio="1" type="circle"/></Brush>]]></param></Preset>"#;
    let imported = load(&kpp_xml(xml), "kpp");
    let angle = imported.presets[0].brush.angle;
    assert!(angle.is_finite() && (0.0..360.0).contains(&angle), "{angle}");
}
