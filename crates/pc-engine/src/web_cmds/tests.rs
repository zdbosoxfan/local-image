use super::*;
use photocraft_doc::{Layer, LayerContent};

fn tmp(name: &str) -> String {
    let d = std::env::temp_dir().join(format!("photocraft-web-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d.to_string_lossy().into_owned()
}

/// A 64×48 document: white background, a red square with soft (half-transparent) edges on a
/// transparent layer, and a gradient band.
fn session(depth: u32) -> Session {
    let mut s = Session::new();
    s.execute("file.new", json!({"width": 64, "height": 48, "depth": depth, "name": "web.psd", "background": "transparent"})).unwrap();
    s.edit("paint", |doc, active| {
        let fmt = doc.pixel_format();
        let mut l = Layer::raster("Logo", fmt);
        let surf = l.surface_mut().unwrap();
        surf.fill_rect(Rect::new(8, 8, 32, 32), &photocraft_raster::from_rgba(&fmt, [1.0, 0.0, 0.0, 1.0]));
        surf.fill_rect(Rect::new(32, 8, 34, 32), &photocraft_raster::from_rgba(&fmt, [1.0, 0.0, 0.0, 0.4]));
        for x in 0..64 {
            surf.fill_rect(Rect::new(x, 36, x + 1, 48), &photocraft_raster::from_rgba(&fmt, [x as f32 / 63.0, 0.5, 1.0 - x as f32 / 63.0, 1.0]));
        }
        let id = doc.insert_above(None, l);
        *active = Some(id);
        Ok(())
    })
    .unwrap();
    s
}

fn decode(path: &str) -> photocraft_codecs::Image {
    photocraft_codecs::decode(&std::fs::read(path).unwrap()).unwrap()
}

#[test]
fn estimates_every_format_at_every_depth() {
    for depth in [8, 16, 32] {
        let mut s = session(depth);
        let mut sizes = Vec::new();
        for f in ["gif", "png8", "png24", "jpeg", "wbmp"] {
            let r = s.execute("file.export.saveForWebLegacy", json!({"format": f, "colors": 16})).unwrap();
            assert_eq!((r["width"].as_u64(), r["height"].as_u64()), (Some(64), Some(48)), "{f}");
            assert!(r["bytes"].as_u64().unwrap() > 20, "{f}");
            if f == "gif" || f == "png8" {
                assert!(r["colors"].as_u64().unwrap() <= 16);
            }
            sizes.push(r["bytes"].as_u64().unwrap());
        }
        assert_eq!(sizes[4], 4 + 8 * 48, "WBMP: header plus 1 bit per pixel");
    }
    let mut s = session(8);
    assert!(s.execute("file.export.saveForWebLegacy", json!({"format": "tiff"})).is_err());
    let r = s.execute("file.export.saveForWebLegacy", json!({"preset": "JPEG Low", "percent": 50})).unwrap();
    assert_eq!(r["format"], "jpg");
    assert_eq!(r["width"], 32);
}

#[test]
fn gif_and_png8_keep_transparency_and_palette() {
    let dir = tmp("gif");
    let mut s = session(8);
    let gif = format!("{dir}/a.gif");
    let r = s.execute("file.export.saveForWebLegacy", json!({"format": "gif", "colors": 8, "dither": "none", "path": gif})).unwrap();
    assert_eq!(r["files"][0], json!(gif));
    let img = decode(&gif).convert(photocraft_codecs::ChannelLayout::Rgba, photocraft_codecs::SampleType::U8);
    let px = |x: usize, y: usize| img.data()[(y * 64 + x) * 4..(y * 64 + x) * 4 + 4].to_vec();
    assert_eq!(px(0, 0)[3], 0, "transparent background");
    assert_eq!(px(16, 16), vec![255, 0, 0, 255]);
    // The 40 % edge is under ½: transparent, as Photoshop's hard GIF transparency.
    assert_eq!(px(33, 16)[3], 0);
    let mut colors: Vec<Vec<u8>> = (0..64 * 48).map(|i| img.data()[i * 4..i * 4 + 4].to_vec()).filter(|p| p[3] > 0).collect();
    colors.sort();
    colors.dedup();
    assert!(colors.len() <= 7, "8 colours including the transparent one, got {}", colors.len());
    // Transparency off: everything over the matte.
    let png8 = format!("{dir}/a.png");
    s.execute("file.export.saveForWebLegacy", json!({"format": "png8", "transparency": false, "matte": "#00ff00", "path": png8})).unwrap();
    let img = decode(&png8).convert(photocraft_codecs::ChannelLayout::Rgba, photocraft_codecs::SampleType::U8);
    assert_eq!(&img.data()[..4], &[0, 255, 0, 255]);
}

#[test]
fn jpeg_quality_progressive_and_metadata() {
    let dir = tmp("jpeg");
    let mut s = session(16);
    s.execute("file.fileInfo", json!({"copyright": "© Example", "author": "A. Person", "description": "secret notes"})).unwrap();
    let lo = format!("{dir}/lo.jpg");
    let hi = format!("{dir}/hi.jpg");
    s.execute("file.export.saveForWebLegacy", json!({"format": "jpeg", "quality": 10, "path": lo, "metadata": "none"})).unwrap();
    s.execute("file.export.saveForWebLegacy", json!({"format": "jpeg", "quality": 95, "progressive": true, "path": hi, "metadata": "copyright"})).unwrap();
    let (a, b) = (std::fs::read(&lo).unwrap(), std::fs::read(&hi).unwrap());
    assert!(a.len() < b.len());
    assert!(b.windows(2).any(|w| w == [0xFF, 0xC2]), "progressive");
    let xmp = decode(&hi).meta.xmp.unwrap_or_default();
    assert!(xmp.contains("Example"), "copyright kept");
    assert!(!xmp.contains("secret"), "description dropped");
    assert!(decode(&lo).meta.xmp.is_none());
}

#[test]
fn slices_export_with_html_table() {
    let dir = tmp("slices");
    let mut s = session(8);
    s.execute("slice.new", json!({"rect": [8, 8, 24, 24], "name": "logo", "url": "https://example.org/", "alt": "Logo"})).unwrap();
    s.execute("slice.new", json!({"rect": [40, 0, 24, 20], "kind": "noImage", "cellText": "Hello & bye"})).unwrap();
    let r = s.execute("file.export.saveForWebLegacy", json!({"format": "png24", "dir": dir, "html": true})).unwrap();
    let files: Vec<String> = r["files"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
    let n = photocraft_doc::slices::resolve(&s.active().unwrap().doc).len();
    assert_eq!(files.len(), n - 1, "every slice but the no-image one");
    let logo = files.iter().find(|f| f.ends_with("images/logo.png")).unwrap();
    assert_eq!(decode(logo).dimensions(), (24, 24));
    let html = std::fs::read_to_string(r["html"].as_str().unwrap()).unwrap();
    assert!(html.contains("<a href=\"https://example.org/\">"));
    assert!(html.contains("alt=\"Logo\""));
    assert!(html.contains("Hello &amp; bye"));
    assert!(html.contains("images/spacer.gif"));
    assert!(std::path::Path::new(&format!("{dir}/images/spacer.gif")).is_file());
    // Total area of the exported slices plus the text cell is the canvas.
    let area: u64 = files
        .iter()
        .map(|f| {
            let (w, h) = decode(f).dimensions();
            u64::from(w) * u64::from(h)
        })
        .sum();
    assert_eq!(area + 24 * 20, 64 * 48);
    // Scaled export scales slices.
    let dir2 = tmp("slices2");
    let r = s.execute("file.export.saveForWebLegacy", json!({"format": "gif", "dir": dir2, "percent": 50, "slices": "user"})).unwrap();
    assert_eq!(r["files"].as_array().unwrap().len(), 1);
    assert_eq!(decode(r["files"][0].as_str().unwrap()).dimensions(), (12, 12));
}

#[test]
fn export_preferences_drive_quick_export() {
    let dir = tmp("quick");
    let mut s = session(8);
    let r = s.execute("file.export.exportPreferences", json!({"quickExportFormat": "jpg", "jpegQuality": 40, "quickExportLocation": "sameFolder"})).unwrap();
    assert_eq!(r["values"]["quickExportFormat"], "jpg");
    assert!(s.execute("file.export.exportPreferences", json!({"quickExportFormat": "bmp"})).is_err());
    // Unsaved document: a path is needed.
    assert!(s.execute("file.export.quickExport", json!({})).is_err());
    s.active_mut().unwrap().path = Some(format!("{dir}/web.psd"));
    let r = s.execute("file.export.quickExport", json!({})).unwrap();
    assert_eq!(r["path"], json!(format!("{dir}/web.jpg")));
    assert_eq!(decode(&format!("{dir}/web.jpg")).dimensions(), (64, 48));
    s.execute("file.export.exportPreferences", json!({"quickExportFormat": "gif"})).unwrap();
    let r = s.execute("file.export.quickExport", json!({"path": format!("{dir}/x.gif")})).unwrap();
    assert_eq!(r["format"], "gif");
    assert!(std::fs::read(format!("{dir}/x.gif")).unwrap().starts_with(b"GIF89a"));
    s.execute("prefs.reset", json!({"path": "export"})).unwrap();
}

#[test]
fn generator_naming_grammar() {
    let a = parse_asset_name("200% foo@2x.png, 48x48 icons/bar.png8 + photo.jpg8", 72.0);
    assert_eq!(a.len(), 3);
    assert_eq!((a[0].file.as_str(), a[0].format.as_str(), a[0].scale), ("foo@2x.png", "png32", Some(2.0)));
    assert_eq!((a[1].file.as_str(), a[1].format.as_str(), a[1].width, a[1].height), ("icons/bar.png", "png8", Some(48.0), Some(48.0)));
    assert_eq!((a[2].file.as_str(), a[2].quality), ("photo.jpg", Some(80)));
    let b = parse_asset_name("?x100 thumb.jpg50%", 72.0);
    assert_eq!((b[0].width, b[0].height, b[0].quality), (None, Some(100.0), Some(50)));
    let c = parse_asset_name("1in x 2cm label.gif", 300.0);
    assert_eq!(c[0].width, Some(300.0));
    assert!((c[0].height.unwrap() - 236.22).abs() < 0.01);
    assert!(parse_asset_name("Layer 1", 72.0).is_empty());
    assert!(parse_asset_name("notes.txt", 72.0).is_empty());
    assert!(parse_asset_name("x.jpgq", 72.0).is_empty());
    let d = parse_defaults("default 50% low/ + 200% @2x", 72.0).unwrap();
    assert_eq!(
        d,
        vec![
            AssetDefault { scale: Some(0.5), folder: "low/".into(), ..Default::default() },
            AssetDefault { scale: Some(2.0), suffix: "@2x".into(), ..Default::default() }
        ]
    );
    assert!(parse_defaults("defaults are nice", 72.0).is_none());
}

#[test]
fn image_assets_are_generated_now_and_on_save() {
    let dir = tmp("assets");
    let mut s = session(8);
    let id = s.active().unwrap().active_layer.unwrap();
    s.execute("layer.renameLayer", json!({"layer": id.0, "name": "logo.png, 50% small/logo.gif, logo.jpg80%"})).unwrap();
    let r = s.execute("file.generate.imageAssets", json!({"dir": dir})).unwrap();
    assert_eq!(r["enabled"], true);
    assert_eq!(r["files"].as_array().unwrap().len(), 3, "{r}");
    // Trimmed to the layer's pixels (x 0..64, y 8..48).
    assert_eq!(decode(&format!("{dir}/logo.png")).dimensions(), (64, 40));
    assert_eq!(decode(&format!("{dir}/small/logo.gif")).dimensions(), (32, 20));
    assert_eq!(decode(&format!("{dir}/logo.jpg")).dimensions(), (64, 40));
    // On save: written to <name>-assets next to the file.
    let saved = format!("{dir}/doc.psd");
    s.active_mut().unwrap().path = Some(saved.clone());
    let r = crate::automate_cmds::document_saved(&mut s, 0).unwrap();
    assert_eq!(r["dir"], json!(format!("{dir}/doc-assets")));
    assert!(std::path::Path::new(&format!("{dir}/doc-assets/logo.png")).is_file());
    // Off: nothing on save.
    assert_eq!(s.execute("file.generate.imageAssets", json!({})).unwrap()["enabled"], false);
    assert!(crate::automate_cmds::document_saved(&mut s, 0).is_none());
    // A default layer adds variants.
    let s2 = session(8);
    let doc = s2.active().unwrap().doc.clone();
    let mut d = (*doc).clone();
    d.layers.push(Layer::new(
        "default 200% @2x",
        LayerContent::Group(photocraft_doc::Group { children: vec![], expanded: false, artboard: None, isolation: photocraft_doc::GroupIsolation::Auto }),
    ));
    let lid = d.layers[1].id;
    d.layer_mut(lid).unwrap().name = "icon.png".into();
    let (files, errors) = generate_assets(&d, &dir);
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(files, vec![format!("{dir}/icon@2x.png")]);
    assert_eq!(decode(&files[0]).dimensions(), (128, 80));
}

#[test]
fn web_settings_from_params() {
    let st = WebSettings::from_params(&json!({"preset": "GIF 32 No Dither", "matte": "none", "webSnap": 50}), "x").unwrap();
    assert_eq!((st.format, st.colors, st.dither, st.matte), (WebFormat::Gif, 32, Dither::None, None));
    assert!((st.web_snap - 0.5).abs() < 1e-6);
    assert!(WebSettings::from_params(&json!({"preset": "Nope"}), "x").is_err());
    assert!(WebSettings::from_params(&json!({"palette": "weird"}), "x").is_err());
    for p in PRESETS {
        let st = preset_settings(p).unwrap();
        assert_eq!(WebSettings::from_params(&st.to_params(), "x").unwrap(), st, "{p} round-trips through params");
    }
}

#[test]
fn previews_show_the_optimised_pixels() {
    let s = session(8);
    let doc = s.active().unwrap().doc.clone();
    let (wd, _, _) = web_document(&doc, &json!({}), &WebSettings::default()).unwrap();
    let buf = photocraft_compose::flatten(&wd);
    for f in ["jpeg", "gif", "png8", "png24", "wbmp"] {
        let st = WebSettings::from_params(&json!({"format": f, "quality": 90, "transparency": false}), "x").unwrap();
        let o = optimize(&buf.px, 64, wd.bounds(), &st, None, None, 72.0, true).unwrap();
        assert_eq!(o.preview.len(), 64 * 48 * 4, "{f}");
        // Red square centre stays red (WBMP: dark).
        let i = (16 * 64 + 16) * 4;
        let px = &o.preview[i..i + 4];
        if f == "wbmp" {
            assert_eq!(px[0], 0);
        } else {
            assert!(px[0] > 200 && px[1] < 60 && px[2] < 60, "{f}: {px:?}");
        }
        let none = optimize(&buf.px, 64, wd.bounds(), &st, None, None, 72.0, false).unwrap();
        assert!(none.preview.is_empty());
        assert_eq!(none.bytes, o.bytes);
    }
}

#[test]
fn jpeg_preview_at_odd_sizes() {
    for (w, h, q) in [(640usize, 431usize, 90), (640, 430, 60), (333, 222, 30)] {
        let px: Vec<[f32; 4]> = (0..w * h).map(|i| [((i % w) as f32 / w as f32), 0.3, ((i / w) as f32 / h as f32), 1.0]).collect();
        let st = WebSettings::from_params(&json!({"format": "jpeg", "quality": q}), "x").unwrap();
        let o = optimize(&px, w, Rect::new(0, 0, w as i32, h as i32), &st, None, None, 72.0, true).unwrap();
        for &(x, y) in &[(10usize, 10usize), (w - 10, h / 2), (w / 2, h - 5)] {
            let i = (y * w + x) * 4;
            let want = [(x as f32 / w as f32 * 255.0) as i32, 76, (y as f32 / h as f32 * 255.0) as i32];
            for c in 0..3 {
                assert!((i32::from(o.preview[i + c]) - want[c]).abs() < 12, "{w}x{h} at {x},{y}: {:?} vs {want:?}", &o.preview[i..i + 4]);
            }
        }
    }
}

#[test]
fn slices_with_the_same_name_get_distinct_files() {
    // #908: two slices named `tile` (and names that sanitize alike) used to overwrite each other.
    let dir = tmp("dupes");
    let mut s = session(8);
    s.execute("slice.new", json!({"rect": [0, 0, 10, 10], "name": "tile"})).unwrap();
    s.execute("slice.new", json!({"rect": [20, 0, 10, 10], "name": "tile"})).unwrap();
    s.execute("slice.new", json!({"rect": [40, 0, 10, 10], "name": "a/b"})).unwrap();
    s.execute("slice.new", json!({"rect": [0, 20, 10, 10], "name": "a?b"})).unwrap();
    s.execute("slice.new", json!({"rect": [20, 20, 10, 10], "name": "spacer"})).unwrap();
    let r = s.execute("file.export.saveForWebLegacy", json!({"format": "gif", "dir": dir, "slices": "user", "html": true})).unwrap();
    let mut files: Vec<String> = r["files"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().rsplit('/').next().unwrap().to_string()).collect();
    files.sort();
    assert_eq!(files, ["a_b.gif", "a_b_2.gif", "spacer_2.gif", "tile.gif", "tile_2.gif"]);
    let html = std::fs::read_to_string(r["html"].as_str().unwrap()).unwrap();
    assert!(html.contains("images/tile.gif") && html.contains("images/tile_2.gif"));
}

#[test]
fn overlapping_slices_keep_the_later_slice_in_html() {
    // #909: the later slice is on top; the earlier one shows only where it isn't covered.
    let dir = tmp("overlap");
    let mut s = session(8);
    s.execute("slice.new", json!({"rect": [0, 0, 40, 30], "name": "under", "url": "https://example.org/under"})).unwrap();
    s.execute("slice.new", json!({"rect": [20, 15, 40, 30], "name": "over", "url": "https://example.org/over"})).unwrap();
    let r = s.execute("file.export.saveForWebLegacy", json!({"format": "png24", "dir": dir, "html": true})).unwrap();
    let html = std::fs::read_to_string(r["html"].as_str().unwrap()).unwrap();
    assert!(html.contains("https://example.org/over"), "the later slice's cell is in the table");
    assert!(html.contains("https://example.org/under"));
    let files: Vec<String> = r["files"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
    let over = files.iter().find(|f| f.ends_with("images/over.png")).unwrap();
    assert_eq!(decode(over).dimensions(), (40, 30));
    // The visible pieces of every image tile the canvas exactly once.
    let area: u64 = files
        .iter()
        .filter(|f| !f.ends_with("spacer.gif"))
        .map(|f| {
            let (w, h) = decode(f).dimensions();
            u64::from(w) * u64::from(h)
        })
        .sum();
    assert_eq!(area, 64 * 48);
    assert!(files.iter().any(|f| f.ends_with("images/under_01.png")), "the covered slice is cut into pieces: {files:?}");
}

#[test]
fn library_identity_save_scripts_cannot_retarget_generated_assets() {
    for close_saved in [true, false] {
        let dir = tmp(if close_saved { "saved-closed" } else { "saved-moved" });
        let mut s = session(8);
        let first = (*s.active().unwrap().doc).clone();
        let mut second = first.clone();
        second.id = photocraft_doc::DocId::fresh();
        s.add_document(second, Some(format!("{dir}/b.psd")));
        s.set_active(0);
        s.active_mut().unwrap().path = Some(format!("{dir}/a.psd"));
        for index in [0, 1] {
            s.set_active(index);
            let layer = s.active().unwrap().active_layer.unwrap();
            s.execute("layer.renameLayer", json!({"layer":layer.0,"name":"image.png"})).unwrap();
            s.execute("file.generate.imageAssets", json!({"enabled":true})).unwrap();
        }
        // Enabling may generate once; remove that setup output before the save event.
        for name in ["a-assets", "b-assets"] {
            let _ = std::fs::remove_dir_all(format!("{dir}/{name}"));
        }
        let saved_index = if close_saved { 0 } else { 1 };
        s.set_active(saved_index);
        s.execute("file.scripts.scriptEventsManager", json!({"add":{"event":"saveDocument","steps":[["file.close",{"document":0}]]}})).unwrap();
        let result = crate::automate_cmds::document_saved(&mut s, saved_index);
        if close_saved {
            assert!(result.is_none());
            assert!(!std::path::Path::new(&format!("{dir}/b-assets")).exists(), "saved A's callback generated B's assets");
        } else {
            assert!(result.is_some(), "the surviving saved document was not re-resolved after an earlier tab closed");
            assert!(std::path::Path::new(&format!("{dir}/b-assets/image.png")).is_file());
        }
        assert!(!std::path::Path::new(&format!("{dir}/a-assets")).exists());
        std::fs::remove_dir_all(dir).unwrap();
    }
}

// Upstream 61b6276c: export path safety and HTML/URL hardening

#[test]
fn reserved_document_and_slice_names_export_with_portable_names() {
    let dir = tmp("reserved-slice-names");
    let mut s = session(8);
    s.edit("rename", |doc, _| {
        doc.name = "aux.psd".into();
        Ok(())
    })
    .unwrap();
    s.execute("slice.new", json!({"rect": [0, 0, 16, 16], "name": "con"})).unwrap();
    let result = s.execute("file.export.saveForWebLegacy", json!({"dir": dir, "format": "png24", "html": true})).unwrap();
    assert!(result["html"].as_str().unwrap().ends_with("/aux_.html"), "{result}");
    assert!(result["files"].as_array().unwrap().iter().any(|f| f.as_str().unwrap().ends_with("/con_.png")), "{result}");
    assert!(std::path::Path::new(result["html"].as_str().unwrap()).is_file());
}

#[test]
fn slice_html_escapes_imported_markup_by_default_and_requires_trusted_opt_in() {
    let dir = tmp("slice-html-safety");
    let mut s = session(8);
    let slice = s
        .execute(
            "slice.new",
            json!({"rect": [0, 0, 24, 20], "kind": "noImage", "name": "text", "cellTextIsHtml": true,
            "cellText": "<script>alert(1)</script><b>Trusted label</b>"}),
        )
        .unwrap();
    let default = s.execute("file.export.saveForWebLegacy", json!({"format": "png24", "dir": dir, "html": true})).unwrap();
    let page = std::fs::read_to_string(default["html"].as_str().unwrap()).unwrap();
    assert!(page.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
    assert!(!page.contains("<script>"));
    assert!(page.contains("&lt;b&gt;Trusted label&lt;/b&gt;"));

    let trusted_dir = tmp("slice-html-trusted");
    let trusted = s.execute("file.export.saveForWebLegacy", json!({"format": "png24", "dir": trusted_dir, "html": true, "trustedSliceHtml": true})).unwrap();
    let page = std::fs::read_to_string(trusted["html"].as_str().unwrap()).unwrap();
    assert!(page.contains("<script>alert(1)</script><b>Trusted label</b>"));
    assert!(s.file_menu.last_web.as_ref().unwrap().get("trustedSliceHtml").is_none());

    let plain_dir = tmp("slice-html-plain");
    s.execute("slice.set", json!({"slice": slice["slice"], "cellTextIsHtml": false})).unwrap();
    let plain = s.execute("file.export.saveForWebLegacy", json!({"format": "png24", "dir": plain_dir, "html": true, "trustedSliceHtml": true})).unwrap();
    let page = std::fs::read_to_string(plain["html"].as_str().unwrap()).unwrap();
    assert!(page.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
}

#[test]
fn slice_links_allow_normal_urls_and_drop_active_schemes() {
    let dir = tmp("slice-link-safety");
    let mut s = session(8);
    for (x, url) in [
        (0, "javascript:alert(1)"),
        (10, "data:text/html,<script>alert(1)</script>"),
        (20, "https://example.org/page?a=1&b=2"),
        (30, "../relative/page.html#part"),
        (40, "mailto:hello@example.org"),
        (50, "JaVa\tscript:alert(1)"),
    ] {
        s.execute("slice.new", json!({"rect": [x, 0, 10, 10], "name": format!("link{x}"), "url": url})).unwrap();
    }
    let r = s.execute("file.export.saveForWebLegacy", json!({"format": "png24", "dir": dir, "html": true, "slices": "user"})).unwrap();
    let page = std::fs::read_to_string(r["html"].as_str().unwrap()).unwrap();
    assert!(!page.contains("href=\"javascript:"));
    assert!(!page.contains("href=\"data:"));
    assert!(!page.contains("href=\"JaVa"));
    assert!(page.contains("href=\"https://example.org/page?a=1&amp;b=2\""));
    assert!(page.contains("href=\"../relative/page.html#part\""));
    assert!(page.contains("href=\"mailto:hello@example.org\""));
    assert_eq!(page.matches("<a href=").count(), 3);
}

#[test]
fn images_folder_stays_inside_export_directory_and_is_escaped_in_html() {
    let root = tmp("slice-image-folder");
    let dir = format!("{root}/output");
    std::fs::create_dir_all(&dir).unwrap();
    let mut s = session(8);
    s.execute(
        "slice.new",
        json!({"rect": [0, 0, 20, 20], "name": "logo", "url": "https://example.org", "target": "x\" onclick=\"alert(1)", "alt": "x\" onerror=\"alert(1)"}),
    )
    .unwrap();
    for images in ["../escaped", "..\\escaped", "/absolute", "C:\\absolute", "nested/../../escaped", "/"] {
        assert!(s.execute("file.export.saveForWebLegacy", json!({"format": "gif", "dir": dir, "html": true, "imagesFolder": images})).is_err(), "{images}");
    }
    assert!(!std::path::Path::new(&format!("{root}/escaped")).exists());
    // Windows forbids quotes in path components; an ampersand still exercises HTML escaping.
    #[cfg(windows)]
    let (folder, escaped_folder) = ("nested/im&files", "nested/im&amp;files");
    #[cfg(not(windows))]
    let (folder, escaped_folder) = ("nested/im\" onerror=\"alert(1)&", "nested/im&quot; onerror=&quot;alert(1)&amp;");
    let r = s.execute("file.export.saveForWebLegacy", json!({"format": "gif", "dir": dir, "html": true, "imagesFolder": folder})).unwrap();
    let page = std::fs::read_to_string(r["html"].as_str().unwrap()).unwrap();
    assert!(page.contains(&format!("src=\"{escaped_folder}/logo.gif\"")));
    assert!(page.contains(&format!("src=\"{escaped_folder}/spacer.gif\"")));
    assert!(page.contains("alt=\"x&quot; onerror=&quot;alert(1)\""));
    assert!(!page.contains("onerror=\"alert(1)\""));
    assert!(!page.contains("onclick=\"alert(1)\""));
    assert!(std::path::Path::new(&dir).join(folder).join("spacer.gif").is_file());
}

#[cfg(unix)]
#[test]
fn images_folder_rejects_existing_symlink_outside_export_directory() {
    let dir = tmp("slice-image-symlink");
    let outside = tmp("slice-image-outside");
    std::os::unix::fs::symlink(&outside, format!("{dir}/linked")).unwrap();
    let mut s = session(8);
    s.execute("slice.new", json!({"rect": [0, 0, 20, 20], "name": "logo"})).unwrap();
    assert!(s.execute("file.export.saveForWebLegacy", json!({"format": "gif", "dir": dir, "html": true, "imagesFolder": "linked"})).is_err());
    assert!(std::fs::read_dir(&outside).unwrap().next().is_none());
}

#[test]
fn image_assets_reject_traversal_from_names_and_defaults() {
    let base = tmp("asset-path-guard");
    let dir = format!("{base}/output");
    std::fs::create_dir_all(&dir).unwrap();
    let outside = tmp("asset-path-outside");
    let mut d = (*session(8).active().unwrap().doc).clone();
    let id = d.layers[1].id;
    d.layer_mut(id).unwrap().name =
        format!("nested/ok.png, ../escaped.png, ..\\backslash.png, {outside}/absolute.png, C:\\drive\\escape.png, CON.png, aux/escape.png, foo./trailing.png");
    let (files, errors) = generate_assets(&d, &dir);
    assert_eq!(files.iter().map(|p| std::path::PathBuf::from(p.as_str())).collect::<Vec<_>>(), vec![std::path::Path::new(&dir).join("nested").join("ok.png")]);
    assert_eq!(errors.len(), 7, "{errors:?}");
    assert!(errors.iter().all(|e| e["error"].as_str().unwrap().contains("unsafe component")), "{errors:?}");
    assert!(!std::path::Path::new(&format!("{base}/escaped.png")).exists());
    assert!(!std::path::Path::new(&format!("{outside}/absolute.png")).exists());

    d.layer_mut(id).unwrap().name = "safe.png".into();
    d.layers.push(Layer::new(
        "default ../ + /outside/ + ..\\bad/ + C:\\drive/ + 50% nested/",
        LayerContent::Group(photocraft_doc::Group { children: vec![], expanded: false, artboard: None, isolation: photocraft_doc::GroupIsolation::Auto }),
    ));
    let (files, errors) = generate_assets(&d, &dir);
    assert_eq!(
        files.iter().map(|p| std::path::PathBuf::from(p.as_str())).collect::<Vec<_>>(),
        vec![std::path::Path::new(&dir).join("nested").join("safe.png")]
    );
    assert_eq!(errors.len(), 4, "{errors:?}");
    assert!(errors.iter().all(|e| e["error"].as_str().unwrap().contains("unsafe component")), "{errors:?}");
}

#[cfg(unix)]
#[test]
fn image_assets_reject_existing_symlinks_in_folders_and_files() {
    let dir = tmp("asset-symlink-guard");
    let outside = tmp("asset-symlink-outside");
    let sentinel = format!("{outside}/sentinel.png");
    std::fs::write(&sentinel, b"unchanged").unwrap();
    let webp_sentinel = format!("{outside}/sentinel.webp");
    std::fs::write(&webp_sentinel, b"also unchanged").unwrap();
    std::os::unix::fs::symlink(&outside, format!("{dir}/linked")).unwrap();
    std::os::unix::fs::symlink(&sentinel, format!("{dir}/leaf.png")).unwrap();
    std::os::unix::fs::symlink(&webp_sentinel, format!("{dir}/leaf.webp")).unwrap();
    let mut d = (*session(8).active().unwrap().doc).clone();
    let id = d.layers[1].id;
    d.layer_mut(id).unwrap().name = "linked/new/sub.png, leaf.png, leaf.webp, good/safe.png".into();
    let (files, errors) = generate_assets(&d, &dir);
    assert_eq!(files, vec![format!("{dir}/good/safe.png")]);
    assert_eq!(errors.len(), 3, "{errors:?}");
    assert!(errors.iter().all(|e| e["error"].as_str().unwrap().contains("symlink")), "{errors:?}");
    assert!(!std::path::Path::new(&format!("{outside}/new")).exists());
    assert_eq!(std::fs::read(&sentinel).unwrap(), b"unchanged");
    assert_eq!(std::fs::read(&webp_sentinel).unwrap(), b"also unchanged");

    let linked_root = format!("{dir}/rootlink");
    std::os::unix::fs::symlink(&outside, &linked_root).unwrap();
    d.layer_mut(id).unwrap().name = "root.png".into();
    let (files, errors) = generate_assets(&d, &format!("{linked_root}/"));
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(files, vec![format!("{linked_root}/root.png")]);
    assert!(std::path::Path::new(&format!("{outside}/root.png")).is_file());

    let mut s = session(8);
    let exported = s.execute("file.export.saveForWebLegacy", json!({"dir": linked_root, "format": "png24", "html": true})).unwrap();
    assert!(exported["html"].as_str().unwrap().ends_with("/rootlink/web.html"));
    assert!(std::path::Path::new(&format!("{outside}/web.html")).is_file());
}

#[test]
fn save_for_web_has_no_default_matte_while_presets_and_quick_export_keep_white() {
    let dir = tmp("default-matte");
    let mut s = Session::new();
    s.execute("file.new", json!({"width": 256, "height": 8, "background": "transparent"})).unwrap();
    s.edit("Shadow", |doc, active| {
        let fmt = doc.pixel_format();
        let mut layer = Layer::raster("Shadow", fmt);
        for x in 0..256 {
            layer.surface_mut().unwrap().fill_rect(Rect::new(x, 0, x + 1, 8), &photocraft_raster::from_rgba(&fmt, [0.0, 0.0, 0.0, 1.0 - x as f32 / 255.0]));
        }
        *active = Some(doc.insert_above(None, layer));
        Ok(())
    })
    .unwrap();
    for (name, matte, black) in [("default", None, true), ("white", Some("#ffffff"), false)] {
        let path = format!("{dir}/{name}.png");
        let mut params = json!({"format": "png8", "path": path});
        if let Some(matte) = matte {
            params["matte"] = json!(matte);
        }
        s.execute("file.export.saveForWebLegacy", params).unwrap();
        let image = decode(&path).convert(photocraft_codecs::ChannelLayout::Rgba, photocraft_codecs::SampleType::U8);
        let pixel = &image.data()[64 * 4..65 * 4];
        if black {
            assert_eq!(pixel, &[0, 0, 0, 255]);
        } else {
            assert!(pixel[0] > 40 && pixel[3] == 255);
        }
    }
    assert_eq!(WebSettings::from_params(&json!({"preset": "PNG-8 128 Dithered"}), "export").unwrap().matte, Some([1.0; 3]));
    s.execute("file.export.exportPreferences", json!({"quickExportFormat": "gif"})).unwrap();
    let path = format!("{dir}/quick.gif");
    s.execute("file.export.quickExport", json!({"path": path})).unwrap();
    let image = decode(&path).convert(photocraft_codecs::ChannelLayout::Rgba, photocraft_codecs::SampleType::U8);
    let pixel = &image.data()[64 * 4..65 * 4];
    assert!(pixel[0] > 40 && pixel[3] == 255, "Quick Export keeps its white matte: {pixel:?}");
    s.execute("prefs.reset", json!({"path": "export"})).unwrap();
}

#[test]
fn quick_export_writes_the_format_its_path_names() {
    let dir = tmp("quick_ext");
    let mut s = session(8);
    s.execute("file.export.exportPreferences", json!({"quickExportFormat": "png"})).unwrap();
    for (name, format, magic) in [
        ("a.jpeg", "jpg", &b"\xFF\xD8\xFF"[..]),
        ("b.JPG", "jpg", b"\xFF\xD8\xFF"),
        ("c.gif", "gif", b"GIF8"),
        ("d.webp", "webp", b"RIFF"),
        ("e", "png", b"\x89PNG"),
    ] {
        let path = format!("{dir}/{name}");
        let r = s.execute("file.export.quickExport", json!({"path": path})).unwrap();
        assert_eq!(r["format"], json!(format), "{name}");
        assert!(std::fs::read(&path).unwrap().starts_with(magic), "{name} holds {format} data");
    }
    let tif = format!("{dir}/f.tif");
    let e = s.execute("file.export.quickExport", json!({"path": tif})).unwrap_err().to_string();
    assert!(e.contains("Export As"), "{e}");
    assert!(!std::path::Path::new(&tif).exists());
    s.execute("prefs.reset", json!({"path": "export"})).unwrap();
}
