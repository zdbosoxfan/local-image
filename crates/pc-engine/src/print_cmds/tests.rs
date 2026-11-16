use super::*;
use photocraft_doc::{Knot, NamedPath, Path, Subpath};

fn tmp(name: &str) -> String {
    let d = std::env::temp_dir().join(format!("photocraft-print-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d.to_string_lossy().into_owned()
}

fn session(mode: &str, depth: u32) -> Session {
    let mut s = Session::new();
    s.execute("file.new", json!({"width": 300, "height": 150, "mode": mode, "depth": depth, "name": "Print Me.psd", "background": "#3366cc"})).unwrap();
    s
}

fn find(h: &[u8], n: &[u8], from: usize) -> usize {
    from + h[from..].windows(n.len()).position(|w| w == n).unwrap()
}

/// The (inflated) image stream of a print PDF: (width, height, colour space, samples).
fn pdf_image(pdf: &[u8]) -> (u32, u32, String, Vec<u8>) {
    let i = find(pdf, b"/Subtype /Image", 0);
    let end = find(pdf, b">>\nstream\n", i);
    let dict = String::from_utf8_lossy(&pdf[i..end]).into_owned();
    let num = |k: &str| dict.split(k).nth(1).unwrap().split_whitespace().next().unwrap().parse::<u32>().unwrap();
    let cs = dict.split("/ColorSpace ").nth(1).unwrap().split(" /BitsPerComponent").next().unwrap().to_string();
    let len = num("/Length ") as usize;
    let start = end + 10;
    let mut out = Vec::new();
    std::io::Read::read_to_end(&mut flate2::read::ZlibDecoder::new(&pdf[start..start + len]), &mut out).unwrap();
    (num("/Width "), num("/Height "), cs, out)
}

#[test]
fn print_dry_run_renders_a_pdf_and_reports_lp() {
    let dir = tmp("print");
    for depth in [8, 16, 32] {
        let mut s = session("rgb", depth);
        let r = s
            .execute(
                "file.print",
                json!({"dryRun": true, "printer": "Office", "copies": 2, "paper": "a4", "cornerCropMarks": true, "registrationMarks": true, "labels": true}),
            )
            .unwrap();
        let cmd: Vec<String> = serde_json::from_value(r["command"].clone()).unwrap();
        assert_eq!(&cmd[..5], &["lp", "-d", "Office", "-n", "2"]);
        assert_eq!(r["sent"], false);
        let pdf = std::fs::read(r["pdf"].as_str().unwrap()).unwrap();
        assert!(pdf.starts_with(b"%PDF-1.4"));
        assert!(String::from_utf8_lossy(&pdf).contains("/MediaBox [0 0 595.28 841.89]"));
        let (w, h, cs, data) = pdf_image(&pdf);
        assert_eq!((w, h), (300, 150));
        assert_eq!(cs, "/DeviceRGB");
        assert_eq!(&data[..3], &[0x33, 0x66, 0xcc]);
        // 300 px at 72 ppi = 300 pt, centred.
        let rect: Vec<f64> = serde_json::from_value(r["imageRect"].clone()).unwrap();
        assert!((rect[2] - 300.0).abs() < 1e-6 && (rect[0] - (595.28 - 300.0) / 2.0).abs() < 1e-6);
        let _ = std::fs::remove_file(r["pdf"].as_str().unwrap());
    }
    // Print to PDF: no spooling by default.
    let mut s = session("gray", 8);
    let out = format!("{dir}/out.pdf");
    let r = s.execute("file.print", json!({"output": out, "scaleToFit": true, "orientation": "landscape"})).unwrap();
    assert_eq!(r["sent"], false);
    assert!(r["command"].is_null());
    let (_, _, cs, _) = pdf_image(&std::fs::read(&out).unwrap());
    assert_eq!(cs, "/DeviceGray");
    let scale = r["scale"].as_f64().unwrap();
    assert!(scale > 200.0, "fit to a landscape letter page: {scale}");
    assert!(s.execute("file.print", json!({"paper": "napkin", "dryRun": true})).is_err());
    assert!(s.execute("file.print", json!({"colorHandling": "photocraftManages", "dryRun": true})).is_err(), "needs a printer profile");
}

#[test]
fn photocraft_manages_colors_converts_to_the_printer_profile() {
    let mut s = session("rgb", 8);
    let r = s
        .execute(
            "file.print",
            json!({"dryRun": true, "colorHandling": "photocraftManages", "printerProfile": "coated-cmyk", "intent": "perceptual", "bpc": false}),
        )
        .unwrap();
    assert_eq!(r["color"]["intent"], "perceptual");
    let pdf = std::fs::read(r["pdf"].as_str().unwrap()).unwrap();
    let (_, _, cs, data) = pdf_image(&pdf);
    assert_eq!(cs, "[/ICCBased 7 0 R]");
    assert!(String::from_utf8_lossy(&pdf).contains("/N 4 /Alternate /DeviceCMYK"));
    assert_eq!(data.len(), 300 * 150 * 4);
    // Blue: lots of cyan, little yellow.
    assert!(data[0] > 150 && data[2] < 60, "{:?}", &data[..4]);
    // The document itself is untouched.
    assert_eq!(s.active().unwrap().doc.mode, ColorMode::Rgb);
    let _ = std::fs::remove_file(r["pdf"].as_str().unwrap());
}

#[test]
fn print_one_copy_repeats_the_last_settings() {
    let mut s = session("rgb", 8);
    s.execute("file.print", json!({"dryRun": true, "copies": 3, "paper": "a5", "scale": 50})).unwrap();
    let r = s.execute("file.printOneCopy", json!({"dryRun": true})).unwrap();
    assert_eq!(r["copies"], 1);
    assert_eq!(r["paper"], json!([419.53, 595.28]));
    assert_eq!(r["scale"], 50.0);
    let cmd: Vec<String> = serde_json::from_value(r["command"].clone()).unwrap();
    assert!(!cmd.contains(&"-n".to_string()));
    let _ = std::fs::remove_file(r["pdf"].as_str().unwrap());
}

#[test]
fn package_copies_links_and_relinks() {
    let dir = tmp("package");
    let link = format!("{dir}/art.png");
    let mut src = Session::new();
    src.execute("file.new", json!({"width": 20, "height": 20, "background": "#ff0000"})).unwrap();
    crate::file_cmds::save_doc(&src.active().unwrap().doc, &link, None).unwrap();
    let mut s = session("rgb", 8);
    s.execute("file.placeLinked", json!({"path": link})).unwrap();
    let r = s.execute("file.package", json!({"dir": format!("{dir}/pkg")})).unwrap();
    let copied = format!("{dir}/pkg/Print Me/Links/art.png");
    assert_eq!(r["links"], json!([copied]));
    assert!(std::path::Path::new(&copied).is_file());
    let docp = r["document"].as_str().unwrap();
    assert!(docp.ends_with("Print Me/Print Me.pcraft"));
    let back = photocraft_io::import("Print Me.pcraft", &std::fs::read(docp).unwrap()).unwrap().document;
    let linked: Vec<String> = back
        .walk()
        .iter()
        .filter_map(|(_, _, l)| match &l.content {
            LayerContent::Smart(so) => match &so.source {
                SmartSource::Linked { path } => Some(path.clone()),
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(linked.len(), 1);
    assert!(linked[0].ends_with("Links/art.png"), "{linked:?}");
    // The open document still points at the original.
    let open = &s.active().unwrap().doc;
    assert!(
        open.walk()
            .iter()
            .any(|(_, _, l)| matches!(&l.content, LayerContent::Smart(so) if matches!(&so.source, SmartSource::Linked { path } if *path == link)))
    );
}

#[test]
fn package_link_names_do_not_collide_or_overwrite_existing_files() {
    let collision_root = tmp("package-collisions");
    let mut document_session = session("rgb", 8);
    let mut sources = Vec::new();
    for (folder, name, color) in [("a", "x.png", "#ff0000"), ("b", "2-x.png", "#00ff00"), ("c", "x.png", "#0000ff"), ("d", "X.PNG", "#ffff00")] {
        std::fs::create_dir_all(format!("{collision_root}/{folder}")).unwrap();
        let path = format!("{collision_root}/{folder}/{name}");
        let mut source = Session::new();
        source.execute("file.new", json!({"width": 4, "height": 4, "background": color})).unwrap();
        crate::file_cmds::save_doc(&source.active().unwrap().doc, &path, None).unwrap();
        sources.push(std::fs::read(&path).unwrap());
        document_session.execute("file.placeLinked", json!({"path": path})).unwrap();
    }
    let result = document_session.execute("file.package", json!({"dir": format!("{collision_root}/pkg")})).unwrap();
    let copies: Vec<String> = result["links"].as_array().unwrap().iter().map(|path| path.as_str().unwrap().to_string()).collect();
    let unique_names: std::collections::HashSet<String> = copies.iter().map(|path| path.to_lowercase()).collect();
    let copy_contents: Vec<Vec<u8>> = copies.iter().map(|path| std::fs::read(path).unwrap()).collect();
    let unique_contents: std::collections::HashSet<Vec<u8>> = copy_contents.iter().cloned().collect();
    let copies_preserved =
        copy_contents.len() == sources.len() && copy_contents.iter().all(|bytes| sources.contains(bytes)) && unique_contents.len() == sources.len();

    let existing_root = tmp("package-existing-link");
    let existing_links = format!("{existing_root}/Print Me/Links");
    std::fs::create_dir_all(&existing_links).unwrap();
    let sentinel = format!("{existing_links}/ART.PNG");
    std::fs::write(&sentinel, b"keep this file").unwrap();
    let art = format!("{existing_root}/art.png");
    let mut source = Session::new();
    source.execute("file.new", json!({"width": 4, "height": 4, "background": "#ff0000"})).unwrap();
    crate::file_cmds::save_doc(&source.active().unwrap().doc, &art, None).unwrap();
    let art_bytes = std::fs::read(&art).unwrap();
    let mut linked = session("rgb", 8);
    linked.execute("file.placeLinked", json!({"path": art})).unwrap();
    let first = linked.execute("file.package", json!({"dir": existing_root})).unwrap();
    let first_copy = first["links"][0].as_str().unwrap().to_string();
    let sentinel_preserved = std::fs::read(&sentinel).is_ok_and(|bytes| bytes == b"keep this file");
    let first_copy_unique = first_copy.to_lowercase() != sentinel.to_lowercase();
    let second = linked.execute("file.package", json!({"dir": existing_root})).unwrap();
    let second_copy = second["links"][0].as_str().unwrap().to_string();
    let repeated_package_preserved = first_copy.to_lowercase() != second_copy.to_lowercase()
        && std::fs::read(&first_copy).is_ok_and(|bytes| bytes == art_bytes)
        && std::fs::read(&second_copy).is_ok_and(|bytes| bytes == art_bytes);

    let missing_root = tmp("package-missing-once");
    let missing = format!("{missing_root}/gone.png");
    let mut source = Session::new();
    source.execute("file.new", json!({"width": 4, "height": 4})).unwrap();
    crate::file_cmds::save_doc(&source.active().unwrap().doc, &missing, None).unwrap();
    let mut linked = session("rgb", 8);
    linked.execute("file.placeLinked", json!({"path": missing})).unwrap();
    linked.execute("file.placeLinked", json!({"path": missing})).unwrap();
    std::fs::remove_file(&missing).unwrap();
    let missing_result = linked.execute("file.package", json!({"dir": format!("{missing_root}/pkg")})).unwrap();
    let missing_once = missing_result["missing"] == json!([missing]);

    assert!(
        unique_names.len() == sources.len() && copies_preserved && first_copy_unique && sentinel_preserved && repeated_package_preserved && missing_once,
        "unique names={}, copies preserved={copies_preserved}, first copy unique={first_copy_unique}, sentinel preserved={sentinel_preserved}, repeated package preserved={repeated_package_preserved}, missing once={missing_once}",
        unique_names.len()
    );
    let _ = std::fs::remove_dir_all(collision_root);
    let _ = std::fs::remove_dir_all(existing_root);
    let _ = std::fs::remove_dir_all(missing_root);
}

#[test]
fn paths_export_as_illustrator_postscript() {
    let mut s = session("rgb", 8);
    assert!(!s.is_enabled("file.export.pathsToIllustrator"));
    s.edit("paths", |d, _| {
        let tri = Subpath::polygon(&[(10.0, 10.0), (100.0, 10.0), (50.0, 90.0)]);
        let mut curve = Subpath::polyline(&[(0.0, 0.0), (300.0, 150.0)]);
        curve.knots[0] = Knot::smooth(photocraft_geom::Point::new(0.0, 0.0), photocraft_geom::Point::new(0.0, 0.0), photocraft_geom::Point::new(50.0, 0.0));
        d.paths.push(NamedPath { name: "Shapes".into(), path: Path::new(vec![tri, curve]), psd_raw: None });
        Ok(())
    })
    .unwrap();
    let r = s.execute("file.export.pathsToIllustrator", json!({})).unwrap();
    let ai = r["ai"].as_str().unwrap();
    assert!(ai.starts_with("%!PS-Adobe-2.0 EPSF-1.2"));
    assert!(ai.contains("%%BoundingBox: 0 0 300 150"));
    assert!(ai.contains("*u\n10.0000 140.0000 m\n100.0000 140.0000 L\n"), "{ai}");
    assert!(ai.contains("\nn\n") && ai.contains("\nN\n*U\n"));
    assert!(ai.contains(" C\n"), "curve segment");
    let dir = tmp("ai");
    let r = s.execute("file.export.pathsToIllustrator", json!({"path": format!("{dir}/p.ai"), "paths": "Shapes"})).unwrap();
    assert_eq!(r["paths"], 1);
    assert!(s.execute("file.export.pathsToIllustrator", json!({"paths": "Nope"})).is_err());
}

/// A document or path name with a line break stays inside its one PostScript comment line.
#[test]
fn illustrator_export_keeps_names_inside_their_comment_lines() {
    let mut s = session("rgb", 8);
    s.edit("paths", |d, _| {
        d.name = "doc\nshowpage".into();
        let tri = Subpath::polygon(&[(1.0, 1.0), (5.0, 1.0), (3.0, 5.0)]);
        d.paths.push(NamedPath { name: "a\nshowpage\r%x".into(), path: Path::new(vec![tri]), psd_raw: None });
        Ok(())
    })
    .unwrap();
    let r = s.execute("file.export.pathsToIllustrator", json!({})).unwrap();
    let ai = r["ai"].as_str().unwrap();
    assert!(ai.contains("%%Title: (doc showpage)\n"), "{ai}");
    assert!(ai.contains("%%Note: a showpage %x\n"), "{ai}");
}

#[test]
fn package_copies_only_links_to_images() {
    let dir = tmp("package-not-image");
    let link = format!("{dir}/art.png");
    let text = format!("{dir}/readme");
    const TEXT: &[u8] = b"plain text, not an image: package marker";
    std::fs::write(&text, TEXT).unwrap();
    let mut s = session("rgb", 8);
    crate::file_cmds::save_doc(&s.active().unwrap().doc, &link, None).unwrap();
    s.execute("file.placeLinked", json!({"path": link})).unwrap();
    s.edit("redirect link", |doc, active| {
        let LayerContent::Smart(so) = &mut doc.layer_mut(active.unwrap()).unwrap().content else { panic!("smart object") };
        so.source = SmartSource::Linked { path: text.clone() };
        Ok(())
    })
    .unwrap();
    let before = s.active().unwrap().doc.clone();
    let pkg = format!("{dir}/pkg");
    let result = s.execute("file.package", json!({"dir": pkg}));
    assert!(!std::path::Path::new(&format!("{pkg}/Print Me/Links/readme")).exists(), "non-image was copied: {result:?}");
    let result = result.unwrap();
    assert_eq!(result["missing"], json!([text]));
    assert_eq!(result["links"], json!([]));
    assert_eq!(s.active().unwrap().doc, before);
    assert!(std::path::Path::new(result["document"].as_str().unwrap()).is_file());
}
