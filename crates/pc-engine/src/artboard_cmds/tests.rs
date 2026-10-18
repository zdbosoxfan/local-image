use super::*;
use photocraft_geom::Rect;

fn session(depth: u32) -> Session {
    let mut s = Session::new();
    s.execute("file.new", json!({"width": 100, "height": 80, "depth": depth})).unwrap();
    s
}

fn doc(s: &Session) -> &Document {
    &s.active().unwrap().doc
}

fn add_square(s: &mut Session, r: Rect, rgba: [f32; 4]) -> LayerId {
    s.edit("sq", |doc, active| {
        let fmt = doc.pixel_format();
        let mut l = Layer::raster(doc.next_layer_name("Layer"), fmt);
        l.surface_mut().unwrap().fill_rect(r, &photocraft_raster::from_rgba(&fmt, rgba));
        let id = doc.insert_above(None, l);
        *active = Some(id);
        Ok(id)
    })
    .unwrap()
}

#[test]
fn new_artboards_place_and_grow_canvas() {
    let mut s = session(8);
    let r = s.execute("layer.new.artboard", json!({"rect": [0, 0, 100, 80]})).unwrap();
    let a1 = LayerId(r["layer"].as_u64().unwrap());
    assert_eq!(doc(&s).layer(a1).unwrap().name, "Artboard 1");
    // Default: canvas-sized, right of the last board with a gap; the canvas grows.
    let r = s.execute("layer.new.artboard", json!({"background": "black"})).unwrap();
    assert_eq!(r["rect"], json!([200, 0, 100, 80]));
    assert_eq!(doc(&s).size.width, 300);
    let r = s.execute("layer.new.artboard", json!({"preset": "iPhone 14", "x": 0, "y": 100})).unwrap();
    assert_eq!(r["rect"], json!([0, 100, 390, 844]));
    assert_eq!(doc(&s).artboards().len(), 3);
    assert!(s.execute("layer.new.artboard", json!({"preset": "Nope"})).is_err());
    assert!(s.execute("layer.new.artboard", json!({"rect": [0, 0, 0, 5]})).is_err());
    assert!(s.execute("layer.new.artboard", json!({"background": "plaid"})).is_err());
    // Undo removes the board (and the canvas growth).
    s.undo();
    assert_eq!(doc(&s).artboards().len(), 2);
    assert_eq!(doc(&s).size.height, 80);
}

#[test]
fn from_layers_and_group_then_compose_clips() {
    for depth in [8, 16, 32] {
        let mut s = session(depth);
        let red = add_square(&mut s, Rect::new(10, 10, 30, 30), [1.0, 0.0, 0.0, 1.0]);
        let r = s.execute("layer.new.artboardFromLayers", json!({"name": "Board"})).unwrap();
        assert_eq!(r["rect"], json!([10, 10, 20, 20]));
        let gid = LayerId(r["layer"].as_u64().unwrap());
        assert_eq!(doc(&s).artboard_of(red), Some(gid));
        // Grow the board; the white background shows around the square, nothing outside it.
        s.execute("layer.artboard.set", json!({"width": 40, "height": 40})).unwrap();
        let px = |s: &Session, x, y| photocraft_compose::render(doc(s), Rect::new(x, y, x + 1, y + 1)).px[0];
        assert!(px(&s, 40, 40)[0] > 0.99 && px(&s, 40, 40)[1] > 0.99, "{depth}");
        assert!(px(&s, 15, 15)[1] < 0.01);
        // Moving the board moves its contents.
        s.execute("layer.artboard.set", json!({"x": 50, "y": 20})).unwrap();
        let a = doc(&s).layer(gid).unwrap().artboard().unwrap().clone();
        assert_eq!(a.rect, Rect::from_xywh(50, 20, 40, 40));
        let surf = doc(&s).layer(red).unwrap().surface().unwrap();
        assert_eq!(surf.content_bounds(), Rect::new(50, 20, 70, 40));
        // layer.translate on the board does the same.
        s.execute("layer.translate", json!({"layer": gid.0, "dx": -5, "dy": 0})).unwrap();
        assert_eq!(doc(&s).layer(gid).unwrap().artboard().unwrap().rect.x0, 45);
        assert_eq!(doc(&s).layer(red).unwrap().surface().unwrap().content_bounds().x0, 45);
        // Background and name.
        s.execute("layer.artboard.set", json!({"background": "custom", "color": "#00ff00", "name": "Green"})).unwrap();
        let l = doc(&s).layer(gid).unwrap();
        assert_eq!(l.name, "Green");
        assert!(matches!(l.artboard().unwrap().background, ArtboardBackground::Custom(_)));
        // A plain group becomes an artboard sized to its contents.
        add_square(&mut s, Rect::new(0, 60, 8, 70), [0.0, 0.0, 1.0, 1.0]);
        let g = s.execute("layer.groupLayers", json!({})).unwrap();
        let gid2 = g["layer"].as_u64().unwrap();
        assert!(s.is_enabled("layer.new.artboardFromGroup"));
        let r = s.execute("layer.new.artboardFromGroup", json!({"layer": gid2})).unwrap();
        assert_eq!(r["rect"], json!([0, 60, 8, 10]));
        assert!(!s.is_enabled("layer.new.artboardFromGroup"));
    }
}

#[test]
fn clear_artboard_guides_only_inside() {
    let mut s = session(8);
    s.execute("layer.new.artboard", json!({"rect": [0, 0, 50, 50]})).unwrap();
    s.edit("guides", |d, _| {
        d.guides.vertical = vec![10.0, 70.0];
        d.guides.horizontal = vec![20.0];
        Ok(())
    })
    .unwrap();
    assert_eq!(s.execute("view.clearSelectedArtboardGuides", json!({})).unwrap()["cleared"], 2);
    assert_eq!(doc(&s).guides.vertical, vec![70.0]);
    assert!(doc(&s).guides.horizontal.is_empty());
    // Clear Canvas Guides leaves the artboard's guides.
    s.edit("guides", |d, _| {
        d.guides.vertical = vec![10.0, 70.0];
        Ok(())
    })
    .unwrap();
    assert_eq!(s.execute("view.clearCanvasGuides", json!({})).unwrap()["cleared"], 1);
    assert_eq!(doc(&s).guides.vertical, vec![10.0]);
}

#[test]
fn export_to_files_and_pdf() {
    let mut s = session(16);
    add_square(&mut s, Rect::new(0, 0, 10, 10), [1.0, 0.0, 0.0, 1.0]);
    s.execute("layer.new.artboardFromLayers", json!({"name": "Red Board"})).unwrap();
    s.execute("layer.new.artboard", json!({"rect": [20, 0, 30, 15], "name": "Empty/One", "background": "transparent"})).unwrap();
    let dir = std::env::temp_dir().join(format!("photocraft-artboards-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let d = dir.to_string_lossy().into_owned();
    let r = s.execute("file.export.artboardsToFiles", json!({"dir": d, "prefix": "site"})).unwrap();
    let files: Vec<String> = r["files"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
    assert_eq!(files.len(), 2);
    assert!(files[0].ends_with("site_0000_Empty_One.png"), "{files:?}");
    assert!(files[1].ends_with("site_0001_Red Board.png"), "{files:?}");
    let img = photocraft_io::import("x.png", &std::fs::read(&files[1]).unwrap()).unwrap().document;
    assert_eq!((img.size.width, img.size.height), (10, 10));
    let img2 = photocraft_io::import("x.png", &std::fs::read(&files[0]).unwrap()).unwrap().document;
    assert_eq!((img2.size.width, img2.size.height), (30, 15));
    // Layered PSD keeps the board, moved to the origin.
    let r = s.execute("file.export.artboardsToFiles", json!({"dir": d, "prefix": "", "format": "psd", "artboards": [doc(&s).artboards()[0].0.0]})).unwrap();
    let psd = photocraft_io::import("x.psd", &std::fs::read(r["files"][0].as_str().unwrap()).unwrap()).unwrap().document;
    assert_eq!(psd.artboards()[0].2.rect, Rect::new(0, 0, 10, 10));
    let pdf = join(&d, "boards.pdf");
    let r = s.execute("file.export.artboardsToPdf", json!({"path": pdf})).unwrap();
    assert_eq!(r["pages"], 2);
    let bytes = std::fs::read(&pdf).unwrap();
    assert!(bytes.starts_with(b"%PDF-1.4"));
    assert!(bytes.ends_with(b"%%EOF\n"));
    let text = String::from_utf8_lossy(&bytes);
    assert_eq!(text.matches("/Type /Page ").count(), 2);
    assert_eq!(text.matches("/DCTDecode").count(), 2);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Upstream f360cb0b (#6613): each artboard gets its own output even when names are identical or
/// collide after sanitizing or case folding, and each file holds that board's pixels.
#[test]
fn artboard_exports_with_colliding_names_keep_every_board() {
    let cases: [(u32, &[&str], Option<&str>, &str); 4] = [
        (8, &["Artboard 1", "Artboard 1"], None, "png"),
        (16, &["Hero/1", "Hero:1", "Hero?1", ".", " "], Some(""), "tiff"),
        (8, &["Logo", "logo", "Logo_0001", "0000_Logo", "Лого", "лого"], Some("site"), "png"),
        (32, &["Same", "Same"], Some("Launch/Run"), "png"),
    ];
    for (n, (depth, names, prefix, format)) in cases.into_iter().enumerate() {
        let mut s = Session::new();
        s.execute("file.new", json!({"name": "Board Export", "width": 96, "height": 32, "depth": depth})).unwrap();
        let mut boards = Vec::new();
        for (i, name) in names.iter().enumerate() {
            let width = 3 + i as u32;
            let color = if i % 2 == 0 { [1.0, 0.0, 0.0, 1.0] } else { [0.0, 0.0, 1.0, 1.0] };
            s.execute("layer.new.artboard", json!({"name": name, "rect": [12 * i, 0, width, 2], "background": "custom", "color": color})).unwrap();
            boards.push((width, color));
        }
        let dir = std::env::temp_dir().join(format!("photocraft-artboard-names-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut params = json!({"dir": dir.to_string_lossy(), "format": format});
        if let Some(prefix) = prefix {
            params["prefix"] = json!(prefix);
        }
        let revision = s.active().unwrap().revision;
        let r = s.execute("file.export.artboardsToFiles", params).unwrap();
        let files: Vec<&str> = r["files"].as_array().unwrap().iter().map(|f| f.as_str().unwrap()).collect();
        assert_eq!(files.len(), names.len());
        let lower: std::collections::HashSet<String> = files.iter().map(|f| f.to_lowercase()).collect();
        assert_eq!(lower.len(), files.len(), "export paths collide: {files:?}");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), files.len(), "one physical output per returned path");
        // Exports run top of the Layers panel first: the last-created board comes first.
        for (file, (width, color)) in files.iter().zip(boards.iter().rev()) {
            let img = photocraft_io::import(file, &std::fs::read(file).unwrap()).unwrap().document;
            assert_eq!((img.size.width, img.size.height), (*width, 2), "wrong artboard in {file}");
            let px = photocraft_compose::flatten(&img).px[0];
            for (actual, expected) in px.iter().zip(color) {
                assert!((actual - expected).abs() < 1e-3, "wrong pixels in {file}: {px:?}");
            }
        }
        assert_eq!(s.active().unwrap().revision, revision, "export must not edit the source document");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn raster_pdf_xref_offsets_point_at_objects() {
    let pdf = raster_pdf(&[(2, 3, 72.0, vec![0xff, 0xd8, 0xff, 0xd9])]);
    let text = String::from_utf8_lossy(&pdf).into_owned();
    let xref = text.rfind("xref\n").unwrap();
    let offsets: Vec<usize> = text[xref..].lines().skip(3).take(5).map(|l| l[..10].parse().unwrap()).collect();
    for (i, o) in offsets.iter().enumerate() {
        assert!(text[*o..].starts_with(&format!("{} 0 obj", i + 1)));
    }
    assert!(text.contains("/MediaBox [0 0 2.000 3.000]"));
}

#[test]
fn disabled_without_artboards() {
    let mut s = session(8);
    for id in
        ["layer.artboard.set", "view.clearSelectedArtboardGuides", "file.export.artboardsToFiles", "file.export.artboardsToPdf", "layer.new.artboardFromGroup"]
    {
        assert!(!s.is_enabled(id), "{id}");
    }
    assert!(s.is_enabled("layer.new.artboard"));
    assert!(s.is_enabled("layer.new.artboardFromLayers"));
    let _ = s.execute("layer.new.artboard", json!({}));
    assert!(!s.is_enabled("layer.new.artboardFromLayers"));
}

#[test]
fn dsfix_v027_array_artboard_coordinates_already_bounded() {
    let base = Rect::from_xywh(0, 0, 10, 10);
    for value in [1e20, -1e20, 1_000_001., -1_000_001.] {
        for axis in 0..2 {
            let mut r = [0., 0., 10., 10.];
            r[axis] = value;
            assert!(rect_param(&json!({"rect":r}), base, "test").is_err());
        }
    }
    assert_eq!(rect_param(&json!({"rect":[-1_000_000.,999_990.,10.,10.]}), base, "test").unwrap(), Rect::from_xywh(-1_000_000, 999_990, 10, 10));
}
