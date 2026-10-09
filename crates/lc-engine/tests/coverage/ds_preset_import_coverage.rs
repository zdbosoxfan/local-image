use lightcraft_engine::preset_import::{Lua, embedded_xmp, group_from_dir, lrtemplate_props, parse_lua, read_presets, read_zip};

// ---------------------------------------------------------------------------
// Minimal ZIP writer (stored entries only) for tests.
// The CRC-32 is set to 0; `read_zip` does not check it.
// ---------------------------------------------------------------------------
fn write_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    write_zip_with_method(entries, 0)
}

fn write_zip_with_method(entries: &[(&str, &[u8])], method: u16) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    let mut offsets = Vec::new();

    for (name, data) in entries {
        let offset = out.len() as u32;
        offsets.push(offset);

        // Local file header
        out.extend_from_slice(&0x04034b50u32.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes()); // version needed
        out.extend_from_slice(&0u16.to_le_bytes()); // flags
        out.extend_from_slice(&method.to_le_bytes()); // compression method
        out.extend_from_slice(&0u16.to_le_bytes()); // mod time
        out.extend_from_slice(&0u16.to_le_bytes()); // mod date
        out.extend_from_slice(&0u32.to_le_bytes()); // crc-32
        out.extend_from_slice(&(data.len() as u32).to_le_bytes()); // compressed size
        out.extend_from_slice(&(data.len() as u32).to_le_bytes()); // uncompressed size
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // extra length
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(data);
    }

    let central_offset = out.len() as u32;

    for (i, (name, data)) in entries.iter().enumerate() {
        let offset = offsets[i];
        central.extend_from_slice(&0x02014b50u32.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes()); // version made by
        central.extend_from_slice(&20u16.to_le_bytes()); // version needed
        central.extend_from_slice(&0u16.to_le_bytes()); // flags
        central.extend_from_slice(&method.to_le_bytes()); // compression method
        central.extend_from_slice(&0u16.to_le_bytes()); // mod time
        central.extend_from_slice(&0u16.to_le_bytes()); // mod date
        central.extend_from_slice(&0u32.to_le_bytes()); // crc-32
        central.extend_from_slice(&(data.len() as u32).to_le_bytes()); // compressed size
        central.extend_from_slice(&(data.len() as u32).to_le_bytes()); // uncompressed size
        central.extend_from_slice(&(name.len() as u16).to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes()); // extra length
        central.extend_from_slice(&0u16.to_le_bytes()); // comment length
        central.extend_from_slice(&0u16.to_le_bytes()); // disk number start
        central.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
        central.extend_from_slice(&0u32.to_le_bytes()); // external attrs
        central.extend_from_slice(&offset.to_le_bytes()); // local header offset
        central.extend_from_slice(name.as_bytes());
    }

    let cd_size = central.len() as u32;
    out.extend_from_slice(&central);

    // End of central directory
    out.extend_from_slice(&0x06054b50u32.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // disk number
    out.extend_from_slice(&0u16.to_le_bytes()); // disk with central dir
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes()); // entries on disk
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes()); // total entries
    out.extend_from_slice(&cd_size.to_le_bytes());
    out.extend_from_slice(&central_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // comment length
    out
}

// A minimal .lrtemplate string used in several tests.
const TEMPLATE: &str = r#"s = {
    id = "0D2F9C1E-TEST",
    internalName = "Warm Fade",
    title = "$$$/Test/Warm=Warm Fade",
    type = "Develop",
    value = {
        settings = {
            Exposure2012 = 0.35,
            Contrast2012 = -20,
            ConvertToGrayscale = false,
            ToneCurvePV2012 = { 0, 20, 128, 128, 255, 240, },
            SplitToningShadowHue = 210,
            SplitToningShadowSaturation = 15,
            HueAdjustmentOrange = -8,
            CameraProfile = "Some Profile",
            RetouchInfo = {},
            ProcessVersion = "11.0",
            EnableColorAdjustments = true,
        },
        uuid = "0D2F9C1E-TEST",
    },
    version = 0,
}
"#;

// A minimal XMP packet for a .xmp file (no crop/wb).
const XMP: &str = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
<rdf:Description rdf:about="" xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/"
 crs:Exposure2012="+0.50" crs:Vibrance="20"/>
</rdf:RDF></x:xmpmeta>"#;

// XMP with crop and white balance for photo (DNG) tests.
const XMP_PHOTO: &str = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
<rdf:Description rdf:about="" xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/"
 crs:Exposure2012="+0.50" crs:Vibrance="20" crs:WhiteBalance="As Shot" crs:Temperature="5600" crs:HasCrop="True" crs:CropLeft="0.1" crs:CropRight="0.9" crs:CropTop="0" crs:CropBottom="1"/>
</rdf:RDF></x:xmpmeta>"#;

#[test]
fn zip_read_roundtrip_stored() {
    let zip = write_zip(&[("file1.txt", b"hello"), ("dir/", b""), ("dir/file2.bin", &[0, 1, 2, 3])]);
    let entries = read_zip(&zip).unwrap();
    assert_eq!(entries.len(), 2, "directory entry must be skipped");
    assert_eq!(entries[0].0, "file1.txt");
    assert_eq!(entries[0].1, b"hello");
    assert_eq!(entries[1].0, "dir/file2.bin");
    assert_eq!(entries[1].1, &[0, 1, 2, 3]);
}

#[test]
fn zip_skips_hidden_and_macosx() {
    let zip = write_zip(&[("__MACOSX/._file", b"junk"), (".hidden", b"hidden"), ("normal.txt", b"ok"), ("folder/", b"")]);
    let entries = read_zip(&zip).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].0, "normal.txt");
    assert_eq!(entries[0].1, b"ok");
}

#[test]
fn zip_rejects_unsupported_compression() {
    let zip = write_zip_with_method(&[("a.txt", b"data")], 12); // 12 = bzip2
    assert!(read_zip(&zip).is_err());
}

#[test]
fn zip_rejects_malformed() {
    assert!(read_zip(b"").is_err());
    assert!(read_zip(b"not a zip").is_err());
    let good = write_zip(&[("a.txt", b"data")]);
    // truncate inside central directory
    let truncated = &good[..good.len() - 5];
    assert!(read_zip(truncated).is_err());
}

#[test]
fn lua_parse_basic_types() {
    let v = parse_lua(
        r#"return { a = 1, ["b c"] = 'x\'y', -2.5e1, 0x10, t = { true, nil }, [[long
text]], z = ZSTR "loc" }"#,
    )
    .unwrap();
    assert_eq!(v.get("a"), Some(&Lua::Num(1.0)));
    assert_eq!(v.get("b c"), Some(&Lua::Str("x'y".into())));
    assert_eq!(v.get("z"), Some(&Lua::Str("loc".into())));
    let Lua::Table(arr, _) = &v else { panic!("expected table") };
    assert_eq!(arr[..2], [Lua::Num(-25.0), Lua::Num(16.0)]);
    assert_eq!(arr[2], Lua::Str("long\ntext".into()));
}

#[test]
fn lua_parse_table_and_accessors() {
    let v = parse_lua("{ name = \"Test\", list = { 1, 2, 3 }, enabled = true }").unwrap();
    assert_eq!(v.get("name").and_then(Lua::str), Some("Test"));
    assert_eq!(v.get("enabled"), Some(&Lua::Bool(true)));
    let Lua::Table(arr, map) = v.get("list").unwrap() else { panic!() };
    assert_eq!(arr.len(), 3);
    assert_eq!(arr[0], Lua::Num(1.0));
    assert_eq!(map.len(), 0);
    assert_eq!(v.get("missing"), None);
}

#[test]
fn lua_parse_leading_name_or_return() {
    let v1 = parse_lua("s = { value = 42 }").unwrap();
    assert_eq!(v1.get("value"), Some(&Lua::Num(42.0)));

    let v2 = parse_lua("return { value = 42 }").unwrap();
    assert_eq!(v2.get("value"), Some(&Lua::Num(42.0)));
}

#[test]
fn lua_parse_errors() {
    assert!(parse_lua("").is_err());
    assert!(parse_lua("{").is_err());
    assert!(parse_lua("{ a = ").is_err());
    assert!(parse_lua("return { a = 1 ").is_err());
}

#[test]
fn lrtemplate_props_missing_settings_errors() {
    assert!(lrtemplate_props("s = { title = 'No settings' }").is_err());
    assert!(lrtemplate_props("").is_err());
    assert!(lrtemplate_props("this is not lua").is_err());
}

#[test]
fn read_presets_lrtemplate_basic() {
    let v = read_presets("Warm Fade.lrtemplate", TEMPLATE.as_bytes(), None).unwrap();
    assert_eq!(v.len(), 1);
    let p = &v[0].preset;
    assert_eq!((p.name.as_str(), p.group.as_str()), ("Warm Fade", "Imported Presets"));
    assert_eq!(p.settings["light"]["exposure"], 0.35);
    assert_eq!(p.settings["light"]["contrast"], -20.0);
    assert_eq!(p.settings["treatment"], "color");
    assert_eq!(p.settings["grading"]["shadows"]["hue"], 210.0);
    assert_eq!(p.settings["mixer"]["orange"]["hue"], -8.0);
    let c = p.settings["curve"]["master"].as_array().unwrap();
    assert_eq!(c.len(), 3);
    assert!((c[0]["y"].as_f64().unwrap() - 20.0 / 255.0).abs() < 1e-9);
    assert_eq!(v[0].unmapped, vec!["CameraProfile".to_string()]);
    assert!(p.id.starts_with("user.xmp."));
}

#[test]
fn read_presets_lrtemplate_old_process_version() {
    let t = "s = { title = \"Old\", value = { settings = { Exposure = 0.5, Contrast = 50, FillLight = 20, HighlightRecovery = 30, Clarity = 10, ToneCurve = { 0, 0, 64, 50, 255, 255 } } } }";
    let p = &read_presets("Old.lrtemplate", t.as_bytes(), None).unwrap()[0].preset;
    assert_eq!(p.settings["light"]["exposure"], 0.5);
    assert_eq!(p.settings["light"]["contrast"], 25.0);
    assert_eq!(p.settings["light"]["shadows"], 20.0);
    assert_eq!(p.settings["light"]["highlights"], -30.0);
    assert_eq!(p.settings["effects"]["clarity"], 10.0);
    assert_eq!(p.settings["curve"]["master"].as_array().unwrap().len(), 3);

    let t2 = "s = { value = { settings = { Contrast2012 = 10, Contrast = 25, Shadows = 5, Brightness = 50 } } }";
    let p2 = &read_presets("New.lrtemplate", t2.as_bytes(), None).unwrap()[0];
    assert_eq!(p2.preset.settings["light"]["contrast"], 10.0);
    assert!(p2.unmapped.is_empty(), "{:?}", p2.unmapped);
}

#[test]
fn read_presets_xmp_basic() {
    let v = read_presets("Bright.xmp", XMP.as_bytes(), None).unwrap();
    assert_eq!(v.len(), 1);
    let p = &v[0].preset;
    assert_eq!(p.name, "Bright");
    assert_eq!(p.group, "Imported Presets");
    assert_eq!(p.settings["light"]["exposure"], 0.5);
    assert_eq!(p.settings["color"]["vibrance"], 20.0);
}

#[test]
fn read_presets_photo_dng_strips_crop_wb() {
    let mut file = b"II*\0 binary junk ".to_vec();
    file.extend_from_slice(XMP_PHOTO.as_bytes());
    file.extend_from_slice(b" more junk");
    let v = read_presets("/x/Moody.dng", &file, None).unwrap();
    let p = &v[0].preset;
    assert_eq!(p.name, "Moody");
    assert_eq!(p.settings["light"]["exposure"], 0.5);
    assert_eq!(p.settings["color"]["vibrance"], 20.0);
    assert!(p.settings.get("crop").is_none());
    assert!(p.settings.get("wb").is_none());
    assert!(p.settings.get("geometry").is_none());
}

#[test]
fn read_presets_photo_missing_xmp_errors() {
    assert!(read_presets("plain.jpg", b"\xff\xd8 no xmp", None).is_err());
}

#[test]
fn read_presets_zip_bundle_groups() {
    let zip = write_zip(&[
        ("Pack/Film/Warm Fade.lrtemplate", TEMPLATE.as_bytes()),
        ("Pack/Film/Bright.xmp", XMP.as_bytes()),
        ("__MACOSX/Pack/._Bright.xmp", b"junk"),
        ("Pack/readme.txt", b"thanks for downloading"),
    ]);
    let v = read_presets("pack.zip", &zip, None).unwrap();
    let names: Vec<_> = v.iter().map(|i| (i.preset.name.as_str(), i.preset.group.as_str())).collect();
    assert_eq!(names, [("Warm Fade", "Film"), ("Bright", "Film")]);
}

#[test]
fn read_presets_zip_no_presets_errors() {
    let zip = write_zip(&[("a.txt", b"x")]);
    assert!(read_presets("empty.zip", &zip, None).is_err());
}

#[test]
fn embedded_xmp_extracts_packet() {
    let mut data = b"prefix".to_vec();
    data.extend_from_slice(XMP.as_bytes());
    data.extend_from_slice(b"suffix");
    let extracted = embedded_xmp(&data).unwrap();
    assert!(extracted.starts_with("<x:xmpmeta"));
    assert!(extracted.ends_with("</x:xmpmeta>"));
    assert!(extracted.contains("crs:Exposure2012"));
}

#[test]
fn embedded_xmp_none_when_absent() {
    assert_eq!(embedded_xmp(b"no xmp here"), None);
}

#[test]
fn group_from_dir_variants() {
    assert_eq!(group_from_dir("Settings"), None);
    assert_eq!(group_from_dir("Develop Presets/"), None);
    assert_eq!(group_from_dir("Looks/My Film Looks"), Some("My Film Looks".into()));
    assert_eq!(group_from_dir("Film Pack/Lightroom Classic"), Some("Film Pack".into()));
    assert_eq!(group_from_dir("Film Pack/Mobile (DNG)/"), Some("Film Pack".into()));
    assert_eq!(group_from_dir("Film Pack/XMP Presets for Lightroom CC 7.3+"), Some("Film Pack".into()));
    assert_eq!(group_from_dir("Sacred Light"), Some("Sacred Light".into()));
    assert_eq!(group_from_dir(""), None);
    assert_eq!(group_from_dir("Looks/__MACOSX"), Some("__MACOSX".into()));
}
