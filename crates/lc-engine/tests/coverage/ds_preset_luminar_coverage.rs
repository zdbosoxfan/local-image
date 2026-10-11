use lightcraft_engine::preset_luminar::{Plist, parse_plist, read_lmp, read_mplumpack};
use serde_json::json;

// ----------------------------------------------------------------------------- helpers

/// Build an XML plist snippet for one effect parameter.
fn param(name: &str, v: f64) -> String {
    format!("<key>{name}</key><dict><key>OptionalDataType</key><integer>0</integer><key>Value</key><real>{v}</real></dict>")
}

/// Build an XML plist snippet for one effect.
fn effect(id: &str, params: &str) -> String {
    format!("<dict><key>Identifier</key><string>{id}</string><key>Parameters</key><dict>{params}</dict></dict>")
}

/// Build an XML layer snippet.
fn layer(amount: f64, blend: &str, enabled: bool, effects: &[String], sublayers: &str) -> String {
    let fx: String = effects.iter().map(|e| e.to_string()).collect();
    let sub = if sublayers.is_empty() {
        String::new()
    } else {
        format!("<key>Sublayers</key><dict><key>AdjustmentLayers</key><array>{sublayers}</array></dict>")
    };
    let enabled_str = if enabled { "<key>Enabled</key><true/>" } else { "<key>Enabled</key><false/>" };
    format!(
        "<dict><key>Amount</key><real>{amount}</real><key>BlendModeIdentifier</key><string>{blend}</string>\
         <key>Effects</key><array>{fx}</array>{enabled_str}<key>Identifier</key><string>layer</string>{sub}</dict>"
    )
}

/// Build a full Luminar look plist.
fn look(layers: &str, extra: &str) -> String {
    format!(
        "<?xml version=\"1.0\"?>\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\
         <plist version=\"1.0\"><dict><key>AdjustmentLayers</key><array>{layers}</array>{extra}</dict></plist>"
    )
}

/// Minimal valid zip writer (stored, no compression).
fn crc32(data: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (i, t) in table.iter_mut().enumerate() {
        let mut c = i as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB88320 ^ (c >> 1) } else { c >> 1 };
        }
        *t = c;
    }
    let mut crc = 0xFFFFFFFFu32;
    for &b in data {
        crc = table[((crc ^ b as u32) & 0xFF) as usize] ^ (crc >> 8);
    }
    crc ^ 0xFFFFFFFF
}

fn create_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    let mut offset = 0u32;
    for (name, data) in entries {
        let name_bytes = name.as_bytes();
        let crc = crc32(data);
        let size = data.len() as u32;
        // local file header
        out.extend_from_slice(&0x04034b50u32.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes()); // version needed
        out.extend_from_slice(&0u16.to_le_bytes()); // flags
        out.extend_from_slice(&0u16.to_le_bytes()); // compression (stored)
        out.extend_from_slice(&0u16.to_le_bytes()); // mod time
        out.extend_from_slice(&0u16.to_le_bytes()); // mod date
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes()); // compressed size
        out.extend_from_slice(&size.to_le_bytes()); // uncompressed size
        out.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // extra len
        out.extend_from_slice(name_bytes);
        out.extend_from_slice(data);
        // central directory entry
        central.extend_from_slice(&0x02014b50u32.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes()); // version made by
        central.extend_from_slice(&20u16.to_le_bytes()); // version needed
        central.extend_from_slice(&0u16.to_le_bytes()); // flags
        central.extend_from_slice(&0u16.to_le_bytes()); // compression
        central.extend_from_slice(&0u16.to_le_bytes()); // mod time
        central.extend_from_slice(&0u16.to_le_bytes()); // mod date
        central.extend_from_slice(&crc.to_le_bytes());
        central.extend_from_slice(&size.to_le_bytes());
        central.extend_from_slice(&size.to_le_bytes());
        central.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes()); // extra len
        central.extend_from_slice(&0u16.to_le_bytes()); // comment len
        central.extend_from_slice(&0u16.to_le_bytes()); // disk start
        central.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
        central.extend_from_slice(&0u32.to_le_bytes()); // external attrs
        central.extend_from_slice(&offset.to_le_bytes()); // local header offset
        central.extend_from_slice(name_bytes);
        offset += 30 + name_bytes.len() as u32 + size;
    }
    let central_size = central.len() as u32;
    let central_offset = out.len() as u32;
    out.extend_from_slice(&central);
    // end of central directory
    out.extend_from_slice(&0x06054b50u32.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // disk
    out.extend_from_slice(&0u16.to_le_bytes()); // cd start disk
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    out.extend_from_slice(&central_size.to_le_bytes());
    out.extend_from_slice(&central_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // comment len
    out
}

// ----------------------------------------------------------------------------- tests

#[test]
fn plist_parses_basic_types() {
    let xml = br#"<?xml version="1.0"?><plist><dict>
        <key>string</key><string>hello &amp; &lt;world&gt;</string>
        <key>int</key><integer>-42</integer>
        <key>real</key><real>2.75</real>
        <key>bool_true</key><true/>
        <key>bool_false</key><false/>
        <key>data</key><data>AAEC</data>
        <key>array</key><array><string>a</string><integer>1</integer></array>
        <key>empty_dict</key><dict/>
        <key>empty_string</key><string></string>
    </dict></plist>"#;
    let p = parse_plist(xml).unwrap();
    assert!(matches!(p, Plist::Dict(_)));
    assert_eq!(p.get("string").and_then(Plist::str), Some("hello & <world>"));
    assert_eq!(p.get("int").and_then(Plist::num), Some(-42.0));
    assert_eq!(p.get("real").and_then(Plist::num), Some(2.75));
    assert_eq!(p.get("bool_true"), Some(&Plist::Bool(true)));
    assert_eq!(p.get("bool_false"), Some(&Plist::Bool(false)));
    assert_eq!(p.get("data"), Some(&Plist::Data));
    let arr = p.get("array").unwrap().array();
    assert_eq!(arr.len(), 2);
    assert_eq!(arr[0].str(), Some("a"));
    assert_eq!(arr[1].num(), Some(1.0));
    assert_eq!(p.get("empty_dict"), Some(&Plist::Dict(vec![])));
    assert_eq!(p.get("empty_string").and_then(Plist::str), Some(""));
}

#[test]
fn plist_handles_self_closing_and_empty() {
    assert_eq!(parse_plist(b"<plist/>"), Ok(Plist::Dict(Vec::new())));
    assert_eq!(parse_plist(b"<plist><dict/></plist>"), Ok(Plist::Dict(Vec::new())));
    assert_eq!(parse_plist(b"<plist><array/></plist>"), Ok(Plist::Array(Vec::new())));
    assert_eq!(parse_plist(b"<plist><string/></plist>"), Ok(Plist::Str(String::new())));
    assert_eq!(parse_plist(b"<plist><true/></plist>"), Ok(Plist::Bool(true)));
    assert_eq!(parse_plist(b"<plist><false/></plist>"), Ok(Plist::Bool(false)));
}

#[test]
fn plist_rejects_binary_and_malformed() {
    assert!(parse_plist(b"bplist00").is_err());
    assert!(parse_plist(b"<plist><dict><key>a</key></dict>").is_err()); // missing </plist>
    assert!(parse_plist(b"<plist><dict><key>a</key><string></dict></plist>").is_err()); // missing </string>
    assert!(parse_plist(b"<plist><foo/></plist>").is_err());
    assert!(parse_plist(b"<plist><integer>abc</integer></plist>").is_err());
}

#[test]
fn plist_enforces_depth_limit() {
    let ok = format!("<plist>{}{}</plist>", "<array>".repeat(20), "</array>".repeat(20));
    assert!(parse_plist(ok.as_bytes()).is_ok());
    let bad = format!("<plist>{}{}</plist>", "<array>".repeat(200_000), "</array>".repeat(200_000));
    assert!(parse_plist(bad.as_bytes()).unwrap_err().contains("nested too deeply"));
}

#[test]
fn plist_skips_comments_doctype_decl() {
    let xml = br#"<?xml version="1.0"?><!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd"><!-- comment --><plist><!-- inner --><dict><key>a</key><string>x</string></dict></plist>"#;
    let p = parse_plist(xml).unwrap();
    assert_eq!(p.get("a").and_then(Plist::str), Some("x"));
}

#[test]
fn plist_parses_nan_and_inf() {
    let p = parse_plist(b"<plist><real>NaN</real></plist>").unwrap();
    match p {
        Plist::Num(n) => assert!(n.is_nan()),
        _ => panic!("expected Num"),
    }
    let p = parse_plist(b"<plist><real>inf</real></plist>").unwrap();
    match p {
        Plist::Num(n) => assert!(n.is_infinite() && n > 0.0),
        _ => panic!("expected Num"),
    }
    let p = parse_plist(b"<plist><real>-inf</real></plist>").unwrap();
    match p {
        Plist::Num(n) => assert!(n.is_infinite() && n < 0.0),
        _ => panic!("expected Num"),
    }
}

#[test]
fn read_lmp_maps_basic_sliders() {
    let layers = [
        layer(
            1.0,
            "Normal",
            true,
            &[effect(
                "MIPLDevelopCommonEffectID",
                &(param("Exposure", 25.0) + &param("Contrast", 12.0) + &param("Highlights", -30.0) + &param("Shadows", 10.0)),
            )],
            "",
        ),
        layer(
            1.0,
            "Normal",
            true,
            &[effect(
                "MIPLWhiteBalanceEffect",
                &(param("Temperature", 15.0) + &param("Tint", 5.0) + &param("Saturation", 20.0) + &param("Vibrance", 15.0)),
            )],
            "",
        ),
        layer(1.0, "Normal", true, &[effect("MIPLClarityEffect", &param("Clarity", 20.0))], ""),
    ]
    .concat();
    let xml = look(&layers, "<key>uuid</key><string>AB12-CD34</string>");
    let i = read_lmp("test.lmp", xml.as_bytes(), None).unwrap();
    let p = &i.preset;
    assert_eq!((p.name.as_str(), p.group.as_str(), p.id.as_str()), ("test", "Imported Presets", "user.lmp.ab12-cd34"));
    let s = &p.settings;
    assert_eq!(s["light"]["exposure"], json!(1.0));
    assert_eq!(s["light"]["contrast"], json!(12.0));
    assert_eq!(s["light"]["highlights"], json!(-30.0));
    assert_eq!(s["light"]["shadows"], json!(10.0));
    assert_eq!(s["color"]["saturation"], json!(20.0));
    assert_eq!(s["color"]["vibrance"], json!(15.0));
    assert_eq!(s["effects"]["clarity"], json!(20.0));
    assert_eq!(s["wb"]["tint"], json!(5.0));
    assert!(s["wb"]["temp"].as_f64().unwrap() > 6500.0);
    assert!(i.unmapped.is_empty(), "{:?}", i.unmapped);
}

#[test]
fn read_lmp_applies_opacity_and_disabled() {
    let layers = [
        layer(0.5, "Normal", true, &[effect("MIPLVibranceEffect", &param("Vibrance", 30.0))], ""),
        layer(1.0, "Normal", false, &[effect("MIPLDevelopCommonEffectID", &param("Shadows", 50.0))], ""),
    ]
    .concat();
    let xml = look(&layers, "");
    let i = read_lmp("test.lmp", xml.as_bytes(), None).unwrap();
    let s = &i.preset.settings;
    assert_eq!(s["color"]["vibrance"], json!(15.0));
    assert!(s["light"].get("shadows").is_none(), "disabled layer should be ignored");
}

#[test]
fn read_lmp_handles_sublayers() {
    let sub = layer(1.0, "Normal", true, &[effect("MIPLDevelopCommonEffectID", &param("Exposure", 25.0))], "");
    let parent = layer(1.0, "Normal", true, &[], &sub);
    let xml = look(&parent, "");
    let i = read_lmp("test.lmp", xml.as_bytes(), None).unwrap();
    let s = &i.preset.settings;
    assert_eq!(s["light"]["exposure"], json!(1.0));
}

#[test]
fn read_lmp_skips_blend_modes_and_masks() {
    let layers = [
        // Screened layer should not be folded in.
        layer(1.0, "Screen", true, &[effect("MIPLContrastEffect", &param("Contrast", 40.0))], ""),
        // Masked layer should be considered local and skipped.
        {
            let mut l = layer(1.0, "Normal", true, &[effect("MIPLExposureEffect", &param("Exposure", 50.0))], "");
            // Insert a mask key into the dict (after Identifier).
            l = l.replace("<key>Identifier</key><string>layer</string>", "<key>Identifier</key><string>layer</string><key>Mask</key><true/>");
            l
        },
        // A normal mapped layer so the look itself is valid.
        layer(1.0, "Normal", true, &[effect("MIPLClarityEffect", &param("Clarity", 20.0))], ""),
    ]
    .concat();
    let xml = look(&layers, "");
    let i = read_lmp("test.lmp", xml.as_bytes(), None).unwrap();
    let s = &i.preset.settings;
    assert_eq!(s["effects"]["clarity"], json!(20.0));
    assert!(s["light"].get("contrast").is_none());
    assert!(s["light"].get("exposure").is_none());
    assert!(i.unmapped.contains(&"Contrast.Contrast".to_string()));
    assert!(i.unmapped.iter().any(|u| u.contains("blend mode Screen")));
    assert!(i.unmapped.iter().any(|u| u.contains("mask")));
}

#[test]
fn read_lmp_maps_curves() {
    let curve = "<key>RGB</key><dict><key>OptionalData</key><array>\
        <real>0.5</real><real>0</real><real>0.1</real><real>0.5</real><real>0.5</real><real>1</real><real>0.9</real>\
        </array><key>OptionalDataType</key><integer>1</integer><key>Value</key><real>50</real></dict>";
    let layers = layer(1.0, "Normal", true, &[effect("MIPLCurveEffect", curve)], "");
    let xml = look(&layers, "");
    let i = read_lmp("test.lmp", xml.as_bytes(), None).unwrap();
    let s = &i.preset.settings;
    let curve_arr = s["curve"]["master"].as_array().unwrap();
    assert_eq!(curve_arr.len(), 3);
    assert!((curve_arr[0]["x"].as_f64().unwrap() - 0.0).abs() < 1e-9);
    assert!((curve_arr[0]["y"].as_f64().unwrap() - 0.1).abs() < 1e-9);
    assert!((curve_arr[1]["x"].as_f64().unwrap() - 0.5).abs() < 1e-9);
    assert!((curve_arr[1]["y"].as_f64().unwrap() - 0.5).abs() < 1e-9);
    assert!((curve_arr[2]["x"].as_f64().unwrap() - 1.0).abs() < 1e-9);
    assert!((curve_arr[2]["y"].as_f64().unwrap() - 0.9).abs() < 1e-9);
}

#[test]
fn read_lmp_extracts_uuid_name_and_bundle() {
    // Name from Name key, UUID from uuid.
    let layers = layer(1.0, "Normal", true, &[effect("MIPLExposureEffect", &param("Exposure", 10.0))], "");
    let xml = look(&layers, "<key>Name</key><string>My Look</string><key>uuid</key><string>AB12-CD34</string>");
    let i = read_lmp("whatever.lmp", xml.as_bytes(), None).unwrap();
    assert_eq!(i.preset.name, "My Look");
    assert_eq!(i.preset.id, "user.lmp.ab12-cd34");

    // Bundle path: name from bundle folder.
    let xml_no_name = look(&layers, "");
    let i = read_lmp("/p/Vintage.lmp/Contents/preset.lmp", xml_no_name.as_bytes(), None).unwrap();
    assert_eq!(i.preset.name, "Vintage");
    assert!(i.preset.id.starts_with("user.lmp."));
    assert!(!i.preset.id.is_empty());
}

#[test]
fn read_lmp_absolute_temperature() {
    // Absolute temperature: value > 1000 maps to absolute Temperature, not incremental.
    let layers = layer(1.0, "Normal", true, &[effect("MIPLWhiteBalanceEffect", &param("Temperature", 7500.0))], "");
    let xml = look(&layers, "");
    let i = read_lmp("test.lmp", xml.as_bytes(), None).unwrap();
    let s = &i.preset.settings;
    assert_eq!(s["wb"]["temp"], json!(7500.0));

    // Relative shift: value <= 1000 maps to incremental temperature (base + shift).
    let layers = layer(1.0, "Normal", true, &[effect("MIPLWhiteBalanceEffect", &param("Temperature", 500.0))], "");
    let xml = look(&layers, "");
    let i = read_lmp("test.lmp", xml.as_bytes(), None).unwrap();
    let s = &i.preset.settings;
    assert!(s["wb"]["temp"].as_f64().unwrap() > 6500.0);
}

#[test]
fn read_lmp_errors_no_layers() {
    let xml = b"<plist><dict/></plist>";
    assert!(read_lmp("x.lmp", xml, None).is_err());
}

#[test]
fn read_lmp_errors_all_unmapped() {
    let layers = layer(1.0, "Normal", true, &[effect("MIPLOrtonFilterEffect", &param("Amount", 15.0))], "");
    let xml = look(&layers, "");
    let err = read_lmp("Glow.lmp", xml.as_bytes(), None).unwrap_err();
    assert!(err.contains("OrtonFilter.Amount"), "{err}");
}

#[test]
fn read_lmp_handles_unmapped_and_mapped() {
    let layers = [
        layer(1.0, "Normal", true, &[effect("MIPLExposureEffect", &param("Exposure", 10.0))], ""),
        layer(1.0, "Normal", true, &[effect("MIPLOrtonFilterEffect", &param("Amount", 15.0))], ""),
    ]
    .concat();
    let xml = look(&layers, "");
    let i = read_lmp("test.lmp", xml.as_bytes(), None).unwrap();
    assert!(i.unmapped.contains(&"OrtonFilter.Amount".to_string()));
    assert_eq!(i.preset.settings["light"]["exposure"], json!(0.4));
}

#[test]
fn read_mplumpack_groups_by_plist() {
    let a = look(&layer(1.0, "Normal", true, &[effect("MIPLClarityEffect", &param("Clarity", 20.0))], ""), "<key>Name</key><string>Pop 1</string>");
    let b = look(&layer(1.0, "Normal", true, &[effect("MIPLSaturationEffect", &param("Saturation", 25.0))], ""), "");
    let info = b"<?xml version=\"1.0\"?><plist version=\"1.0\"><dict><key>GroupName</key><string>Magic Light</string></dict></plist>";
    let pack_bytes = create_zip(&[
        ("Pop 1.lmp", a.as_bytes()),
        ("__MACOSX/._Pop 1.lmp", b"junk"),
        ("Pop 2.lmp", b.as_bytes()),
        ("icon@2x.png", b"\x89PNG"),
        ("PresetsInfo.plist", info),
    ]);
    let v = read_mplumpack("/d/Magic Light-2.mplumpack", &pack_bytes).unwrap();
    let got: Vec<_> = v.iter().map(|i| (i.preset.name.as_str(), i.preset.group.as_str())).collect();
    assert_eq!(got, [("Pop 1", "Magic Light"), ("Pop 2", "Magic Light")]);
}

#[test]
fn read_mplumpack_falls_back_to_filename() {
    let look_bytes = look(&layer(1.0, "Normal", true, &[effect("MIPLClarityEffect", &param("Clarity", 20.0))], ""), "");
    let pack_bytes = create_zip(&[("Pop 1.lmp", look_bytes.as_bytes())]);
    let v = read_mplumpack("Seaside Looks.mplumpack", &pack_bytes).unwrap();
    assert_eq!(v[0].preset.group, "Seaside Looks");
}

#[test]
fn read_mplumpack_handles_bundle_looks_in_zip() {
    let bundle_look = look(&layer(1.0, "Normal", true, &[effect("MIPLExposureEffect", &param("Exposure", 25.0))], ""), "");
    let flat_look =
        look(&layer(1.0, "Normal", true, &[effect("MIPLClarityEffect", &param("Clarity", 20.0))], ""), "<key>Name</key><string>Pop 1</string>");
    let pack_bytes = create_zip(&[
        ("Wild Pack/Vintage.lmp/Contents/preset.lmp", bundle_look.as_bytes()),
        ("Wild Pack/Vintage.lmp/Contents/Info.plist", b"<plist><dict/></plist>"),
        ("Wild Pack/Flat.lmp", flat_look.as_bytes()),
    ]);
    let v = read_mplumpack("/d/Wild Pack.mplumpack", &pack_bytes).unwrap();
    let got: Vec<_> = v.iter().map(|i| (i.preset.name.as_str(), i.preset.group.as_str())).collect();
    assert_eq!(got, [("Vintage", "Wild Pack"), ("Pop 1", "Wild Pack")]);
}

#[test]
fn read_mplumpack_errors_no_looks() {
    let pack_bytes = create_zip(&[("icon.png", b"x")]);
    assert!(read_mplumpack("empty.mplumpack", &pack_bytes).is_err());
}

#[test]
fn read_lmp_rejects_binary_plist() {
    let xml = b"bplist00\x01\x02";
    assert!(read_lmp("x.lmp", xml, None).is_err());
}

#[test]
fn read_lmp_nan_value_is_not_guarded() {
    // A NaN slider value cannot be represented in a photo editor. read_lmp now skips
    // non-finite values like explicit defaults; with no other adjustments this look has
    // nothing to import.
    let layers = layer(1.0, "Normal", true, &[effect("MIPLExposureEffect", &param("Exposure", f64::NAN))], "");
    let xml = look(&layers, "");
    let err = read_lmp("test.lmp", xml.as_bytes(), None).unwrap_err();
    assert!(err.contains("no adjustments"), "{err}");
}
