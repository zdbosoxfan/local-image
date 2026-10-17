use lightcraft_engine::preset_luminar::{Plist, parse_plist, read_lmp};
use serde_json::json;

// helpers

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

// tests

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
fn read_lmp_nan_value_is_not_guarded() {
    // A NaN slider value cannot be represented in a photo editor. read_lmp now skips
    // non-finite values like explicit defaults; with no other adjustments this look has
    // nothing to import.
    let layers = layer(1.0, "Normal", true, &[effect("MIPLExposureEffect", &param("Exposure", f64::NAN))], "");
    let xml = look(&layers, "");
    let err = read_lmp("test.lmp", xml.as_bytes(), None).unwrap_err();
    assert!(err.contains("no adjustments"), "{err}");
}
