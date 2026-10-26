use super::*;

fn sf(command: &str, params: J) -> SmartFilter {
    SmartFilter { command: command.into(), params, blend: BlendMode::Normal, opacity: 1.0, visible: true }
}

fn count(hay: &[u8], needle: &[u8]) -> usize {
    hay.windows(needle.len()).filter(|w| *w == needle).count()
}

/// The filters without options, their measured `filterID`s (as Photoshop wrote them).
const MEASURED_NO_OPTIONS: [(&str, i32); 10] = [
    ("filter.blur.blur", 1114403360),
    ("filter.blur.blurMore", 1114403405),
    ("filter.noise.despeckle", 1148416099),
    ("filter.stylize.findEdges", 1181639749),
    ("filter.pixelate.facet", 1180922912),
    ("filter.pixelate.fragment", 1181902701),
    ("filter.sharpen.sharpen", 1399353968),
    ("filter.sharpen.sharpenMore", 1399353933),
    ("filter.sharpen.sharpenEdges", 1399353925),
    ("filter.stylize.solarize", 1399616122),
];

/// A filter without options as Photoshop saves it: `hasoptions` false, no `Fltr`, a localized
/// name and the current colours.
fn photoshop_no_options_item(name: &str, filter_id: Value) -> Descriptor {
    let rgb = |r: f64, g: f64, b: f64| {
        Value::Descriptor(Descriptor::new("RGBC").with("Rd  ", Value::Double(r)).with("Grn ", Value::Double(g)).with("Bl  ", Value::Double(b)))
    };
    Descriptor::new("filterFX")
        .with("Nm  ", Value::Text(UnicodeString::new_nul(name)))
        .with("blendOptions", Value::Descriptor(Descriptor::new("blendOptions").with("Opct", unit(b"#Prc", 60.0)).with("Md  ", enumv("BlnM", "Mltp"))))
        .with("enab", Value::Boolean(false))
        .with("hasoptions", Value::Boolean(false))
        .with("FrgC", rgb(167.0, 157.0, 147.0))
        .with("BckC", rgb(255.0, 255.0, 255.0))
        .with("filterID", filter_id)
}

#[test]
fn filters_without_options_round_trip_in_the_measured_shape() {
    let keys = ["Nm  ", "blendOptions", "enab", "hasoptions", "FrgC", "BckC", "filterID"];
    for (command, id) in MEASURED_NO_OPTIONS {
        let mut f = sf(command, json!({}));
        f.blend = BlendMode::Overlay;
        f.opacity = 0.75;
        f.visible = false;
        let d = item_for_filter(&f).unwrap();
        let got: Vec<String> = d.items.iter().map(|(k, _)| String::from_utf8_lossy(k.as_bytes()).into_owned()).collect();
        assert_eq!(got, keys, "{command}");
        assert_eq!(d.get("hasoptions"), Some(&Value::Boolean(false)), "{command}");
        assert_eq!(d.get("filterID"), Some(&Value::Integer(id)), "{command}");
        let back = filter_from_item(&Descriptor::from_bytes(&d.to_bytes()).unwrap());
        assert_eq!(back, f, "{command}");
        // Keys an old record may carry have no effect on these filters: still written.
        let extra = item_for_filter(&sf(command, json!({"radius": 3.0, "layer": "L"}))).unwrap();
        assert_eq!(extra.get("filterID"), Some(&Value::Integer(id)), "{command}");
        for p in [json!(null), json!("x"), json!([1, 2])] {
            assert!(item_for_filter(&sf(command, p)).is_ok(), "{command}");
        }
    }
}

#[test]
fn photoshop_filters_without_options_open_editable() {
    for (command, id) in MEASURED_NO_OPTIONS {
        let item = photoshop_no_options_item("Weichzeichnen", Value::Integer(id));
        let f = filter_from_item(&Descriptor::from_bytes(&item.to_bytes()).unwrap());
        assert_eq!(f.command, command);
        assert_eq!(f.params, json!({}));
        assert_eq!((f.visible, f.blend), (false, BlendMode::Multiply));
        assert!((f.opacity - 0.6).abs() < 1e-6);
    }
}

#[test]
fn malformed_filters_without_options_are_kept_verbatim() {
    let blur = code(b"Blr ");
    let cases = [
        // An id no filter has, a missing id, ids of the wrong type.
        photoshop_no_options_item("X", Value::Integer(code(b"Zzzz"))),
        Descriptor::new("filterFX").with("hasoptions", Value::Boolean(false)),
        photoshop_no_options_item("X", Value::Double(f64::from(blur))),
        photoshop_no_options_item("X", Value::Double(f64::NAN)),
        photoshop_no_options_item("X", Value::Text(UnicodeString::new_nul("Blr "))),
        photoshop_no_options_item("X", Value::Integer(-1)),
        // Settings we don't know of on a filter that has none.
        photoshop_no_options_item("X", Value::Integer(blur)).with("Fltr", Value::Descriptor(Descriptor::new("Blr ").with("Amnt", Value::Integer(5)))),
        // Settings that aren't a descriptor, on a filter that has none.
        photoshop_no_options_item("X", Value::Integer(blur)).with("Fltr", Value::Integer(5)),
        photoshop_no_options_item("X", Value::Integer(blur)).with("Fltr", Value::Text(UnicodeString::new_nul("Blr "))),
        photoshop_no_options_item("X", Value::Integer(781)).with("Fltr", Value::Integer(5)),
    ];
    for item in cases {
        let f = filter_from_item(&item);
        assert_eq!(f.command, UNSUPPORTED_FILTER, "{item:?}");
        // Written back (a bare item gains blending and visibility) and still not editable.
        let written = item_for_filter(&f).unwrap();
        assert_eq!(written.get("Fltr"), item.get("Fltr"), "{item:?}");
        let back = filter_from_item(&written);
        assert_eq!((back.command.as_str(), &back.params["filterID"]), (UNSUPPORTED_FILTER, &f.params["filterID"]));
    }
}

/// Smart Sharpen's `Fltr` in the measured shape (no `presetKind`, no `Angl`).
fn smart_sharpen(amount: f64, radius: f64, noise: f64, blur: &str) -> Descriptor {
    Descriptor::new("smartSharpen")
        .with("useLegacy", Value::Boolean(false))
        .with("Amnt", unit(b"#Prc", amount))
        .with("Rds ", unit(b"#Pxl", radius))
        .with("noiseReduction", unit(b"#Prc", noise))
        .with("blur", enumv("blurType", blur))
}

fn dust(radius: i32, threshold: i32) -> Descriptor {
    Descriptor::new("DstS").with("Rds ", Value::Integer(radius)).with("Thsh", Value::Integer(threshold))
}

/// The `Fltr` descriptor PhotoCraft writes for `f`, after a byte round trip.
fn written_fltr(f: &SmartFilter) -> Descriptor {
    let d = Descriptor::from_bytes(&item_for_filter(f).unwrap().to_bytes()).unwrap();
    get_desc(&d, "Fltr").unwrap().clone()
}

/// Measured in Photoshop: Smart Sharpen 120 %, 2.5 px, noise 5 %, Gaussian Blur
/// (filterID 1131); the default entry adds `presetKind` first, which is not kept.
#[test]
fn smart_sharpen_photoshop_entry_opens_and_writes_back() {
    let fltr = smart_sharpen(120.0, 2.5, 5.0, "GsnB");
    let item = ps_item("Smart Sharpen...", fltr.clone(), 1131);
    let f = filter_from_item(&Descriptor::from_bytes(&item.to_bytes()).unwrap());
    assert_eq!(f.command, "filter.sharpen.smartSharpen");
    assert_eq!(f.params, json!({"amount": 120.0, "radius": 2.5, "reduceNoise": 5.0}));
    assert_eq!(written_fltr(&f), fltr);
    let d = item_for_filter(&f).unwrap();
    assert_eq!(int(&d, "filterID"), Some(1131));
    assert_eq!(get_desc(&d, "Fltr").unwrap().class_id, Id::Str(b"smartSharpen".to_vec()));
    // The four-character keys and enum values are codes, as Photoshop writes them.
    let bytes = d.to_bytes();
    assert_eq!(count(&bytes, b"\0\0\0\0blurenum\0\0\0\x08blurType\0\0\0\0GsnB"), 1);
    assert_eq!(count(&bytes, b"\0\0\0\0Rds UntF#Pxl"), 1);

    // With `presetKind` (any value): opens, but the params hold only the engine's own keys
    // (it rejects unknown ones), and a re-save leaves it out as Photoshop does for explicit
    // values.
    for kind in ["presetKindDefault", "presetKindCustom", "Xxxx"] {
        let mut with_preset = Descriptor::new("smartSharpen").with("presetKind", enumv("presetKindType", kind));
        with_preset.items.extend(smart_sharpen(200.0, 1.0, 10.0, "GsnB").items);
        let f = filter_from_item(&Descriptor::from_bytes(&ps_item("Smart Sharpen...", with_preset, 1131).to_bytes()).unwrap());
        assert_eq!(f.command, "filter.sharpen.smartSharpen");
        let keys: Vec<&str> = f.params.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(keys.len(), 3, "{keys:?}");
        assert!(keys.iter().all(|k| ["amount", "radius", "reduceNoise"].contains(k)), "{keys:?}");
        assert_eq!(f.params, json!({"amount": 200.0, "radius": 1.0, "reduceNoise": 10.0}));
        assert_eq!(written_fltr(&f), smart_sharpen(200.0, 1.0, 10.0, "GsnB"));
    }

    // Without options PhotoCraft writes the engine's defaults, Gaussian Blur.
    assert_eq!(written_fltr(&sf("filter.sharpen.smartSharpen", json!({}))), smart_sharpen(100.0, 1.0, 10.0, "GsnB"));
}

/// Lens Blur (Photoshop's default), Motion Blur and legacy mode render differently from
/// PhotoCraft's Smart Sharpen, so those entries stay verbatim.
#[test]
fn smart_sharpen_settings_we_dont_render_stay_verbatim() {
    let mut default_entry = Descriptor::new("smartSharpen").with("presetKind", enumv("presetKindType", "presetKindDefault"));
    default_entry.items.extend(smart_sharpen(200.0, 1.0, 10.0, "lensBlur").items);
    let motion = smart_sharpen(80.0, 1.5, 0.0, "MtnB").with("Angl", Value::Integer(30));
    let mut legacy = smart_sharpen(100.0, 1.0, 10.0, "GsnB");
    set(&mut legacy, "useLegacy", Value::Boolean(true));
    for fltr in [default_entry, motion, legacy] {
        let item = ps_item("Smart Sharpen...", fltr, 1131);
        let f = filter_from_item(&item);
        assert_eq!(f.command, UNSUPPORTED_FILTER);
        assert_eq!(f.params["filterID"], 1131);
        assert_eq!(item_for_filter(&f).unwrap(), item);
    }
}

#[test]
fn malformed_smart_sharpen_is_kept_verbatim() {
    let g = || smart_sharpen(100.0, 1.0, 10.0, "GsnB");
    let without = |key: &str| {
        let mut d = g();
        d.items.retain(|(k, _)| !k.is(key));
        d
    };
    let with = |key: &str, v: Value| {
        let mut d = g();
        set(&mut d, key, v);
        d
    };
    let cases = vec![
        without("useLegacy"),
        without("Amnt"),
        without("Rds "),
        without("noiseReduction"),
        without("blur"),
        with("Amnt", Value::Double(100.0)),
        with("Amnt", unit(b"#Pxl", 100.0)),
        with("Rds ", Value::Integer(1)),
        with("useLegacy", Value::Integer(0)),
        with("Amnt", unit(b"#Prc", 0.0)),
        with("Amnt", unit(b"#Prc", 501.0)),
        with("Rds ", unit(b"#Pxl", 0.05)),
        with("Rds ", unit(b"#Pxl", 65.0)),
        with("Rds ", unit(b"#Pxl", f64::NAN)),
        with("noiseReduction", unit(b"#Prc", -1.0)),
        with("noiseReduction", unit(b"#Prc", f64::INFINITY)),
        with("blur", enumv("blurType", "Xxxx")),
        with("blur", enumv("BlrM", "GsnB")),
        with("blur", Value::Integer(0)),
        with("presetKind", Value::Integer(1)),
        g().with("Angl", Value::Integer(30)),
        g().with("shadowMode", Value::Descriptor(Descriptor::new("adaptCorrectTones"))),
    ];
    for fltr in cases {
        let item = ps_item("Smart Sharpen...", fltr, 1131);
        let f = filter_from_item(&Descriptor::from_bytes(&item.to_bytes()).unwrap());
        assert_eq!(f.command, UNSUPPORTED_FILTER, "{:?}", get_desc(&item, "Fltr"));
        assert_eq!(item_for_filter(&f).unwrap().to_bytes(), item.to_bytes());
    }
    // No `Fltr` at all (hasoptions false).
    let bare = Descriptor::new("filterFX").with("filterID", Value::Integer(1131));
    assert_eq!(filter_from_item(&bare).command, UNSUPPORTED_FILTER);
}

/// Measured in Photoshop: Dust & Scratches at its defaults (1, 0) and at 4, 20 (filterID
/// `DstS`, 1148417107; both settings `long`).
#[test]
fn dust_and_scratches_photoshop_entry_opens_and_writes_back() {
    for (radius, threshold) in [(1, 0), (4, 20), (500, 255)] {
        let fltr = dust(radius, threshold);
        let item = ps_item("Dust & Scratches...", fltr.clone(), 1_148_417_107);
        let f = filter_from_item(&Descriptor::from_bytes(&item.to_bytes()).unwrap());
        assert_eq!(f.command, "filter.noise.dustAndScratches");
        assert_eq!(f.params, json!({"radius": radius, "threshold": threshold}));
        assert_eq!(written_fltr(&f), fltr);
        assert_eq!(int(&item_for_filter(&f).unwrap(), "filterID"), Some(1_148_417_107));
    }
    // No `Fltr` (hasoptions false): kept verbatim.
    let bare = Descriptor::new("filterFX").with("filterID", Value::Integer(code(b"DstS")));
    assert_eq!(filter_from_item(&bare).command, UNSUPPORTED_FILTER);
}

#[test]
fn malformed_dust_and_scratches_is_kept_verbatim() {
    let cases = vec![
        Descriptor::new("DstS").with("Thsh", Value::Integer(0)),
        Descriptor::new("DstS").with("Rds ", Value::Integer(1)),
        Descriptor::new("DstS").with("Rds ", Value::Double(1.0)).with("Thsh", Value::Integer(0)),
        Descriptor::new("DstS").with("Rds ", unit(b"#Pxl", 1.0)).with("Thsh", Value::Integer(0)),
        Descriptor::new("DstS").with("Rds ", Value::Integer(1)).with("Thsh", Value::Boolean(false)),
        dust(0, 0),
        dust(501, 0),
        dust(1, -1),
        dust(1, 256),
        dust(i32::MIN, i32::MAX),
        dust(1, 0).with("Xtra", Value::Integer(1)),
    ];
    for fltr in cases {
        let item = ps_item("Dust & Scratches...", fltr, code(b"DstS"));
        let f = filter_from_item(&item);
        assert_eq!(f.command, UNSUPPORTED_FILTER, "{:?}", get_desc(&item, "Fltr"));
        assert_eq!(item_for_filter(&f).unwrap(), item);
    }
}

/// Every option's range ends round trip; hostile params are written in range (the engine
/// clamps the same way), never a panic.
#[test]
fn smart_sharpen_and_dust_option_values_round_trip() {
    let mut cases = Vec::new();
    for amount in [1.0, 100.0, 500.0] {
        for radius in [0.1, 1.0, 64.0] {
            for noise in [0.0, 10.0, 100.0] {
                cases.push(sf("filter.sharpen.smartSharpen", json!({"amount": amount, "radius": radius, "reduceNoise": noise})));
            }
        }
    }
    for radius in [1, 2, 100, 500] {
        for threshold in [0, 1, 128, 255] {
            cases.push(sf("filter.noise.dustAndScratches", json!({"radius": radius, "threshold": threshold})));
        }
    }
    for f in cases {
        let back = filter_from_item(&Descriptor::from_bytes(&item_for_filter(&f).unwrap().to_bytes()).unwrap());
        assert_eq!(back, f);
    }
    let defaults = json!({"amount": 100.0, "radius": 1.0, "reduceNoise": 10.0});
    for (command, p, want) in [
        (
            "filter.sharpen.smartSharpen",
            json!({"amount": 1e9, "radius": -5.0, "reduceNoise": "x"}),
            json!({"amount": 500.0, "radius": 0.1, "reduceNoise": 10.0}),
        ),
        ("filter.sharpen.smartSharpen", json!({"presetKind": "presetKindDefault", "amount": null}), defaults.clone()),
        ("filter.sharpen.smartSharpen", json!(null), defaults.clone()),
        ("filter.noise.dustAndScratches", json!({"radius": 2.4, "threshold": 1e12}), json!({"radius": 2, "threshold": 255})),
        ("filter.noise.dustAndScratches", json!({"radius": -3, "threshold": -3}), json!({"radius": 1, "threshold": 0})),
        ("filter.noise.dustAndScratches", json!([]), json!({"radius": 1, "threshold": 0})),
    ] {
        let back = filter_from_item(&item_for_filter(&sf(command, p.clone())).unwrap());
        assert_eq!((back.command.as_str(), &back.params), (command, &want), "{p}");
    }
}

fn measured_item(fltr: Descriptor, fg: f64, bg: f64) -> Descriptor {
    let id = code(fltr.class_id.as_bytes().try_into().unwrap());
    Descriptor::new("filterFX")
        .with("Nm  ", Value::Text(UnicodeString::new_nul("Filter...")))
        .with("blendOptions", blend_options(&sf("", J::Null)))
        .with("enab", Value::Boolean(true))
        .with("hasoptions", Value::Boolean(true))
        .with("FrgC", rgbc(fg))
        .with("BckC", rgbc(bg))
        .with("Fltr", Value::Descriptor(fltr))
        .with("filterID", Value::Integer(id))
}

/// The `Fltr` descriptors Photoshop 27 wrote for these filters (one smart object each, saved as
/// PSD and read back), with the settings PhotoCraft reads from them.
const PHOTOSHOP_FLTR: [(&str, &str, &str); 5] = [
    ("filter.distort.pinch", r#"{"amount":-28}"#, "0000000600500069006e00630068000000000000506e63680000000100000000416d6e746c6f6e67ffffffe4"),
    (
        "filter.distort.ripple",
        r#"{"amount":-273,"size":"large"}"#,
        "000000070052006900700070006c006500000000000052706c650000000200000000416d6e746c6f6e67fffffeef0000000052706c53656e756d0000000052706c53000000004c726720",
    ),
    (
        "filter.distort.spherize",
        r#"{"amount":-1,"mode":"horizontalOnly"}"#,
        "00000009005300700068006500720069007a0065000000000000537068720000000200000000416d6e746c6f6e67ffffffff000000005370684d656e756d000000005370684d0000000048727a4f",
    ),
    (
        "filter.distort.polarCoordinates",
        r#"{"mode":"polarToRectangular"}"#,
        "000000120050006f006c0061007200200043006f006f007200640069006e0061007400650073000000000000506c72200000000100000000436e7672656e756d00000000436e767200000000506c7252",
    ),
    (
        "filter.distort.zigZag",
        r#"{"amount":-42,"ridges":11,"style":"pondRipples"}"#,
        "00000007005a00690067005a006100670000000000005a675a670000000300000000416d6e746c6f6e67ffffffd6000000004e6d62526c6f6e670000000b000000005a5a5479656e756d000000005a5a547900000000506e6452",
    ),
];

/// A `filterFX` list item as Photoshop writes it (enabled, Normal at 100%, with options), wrapping the `Fltr` descriptor `fltr`.
fn photoshop_fltr_item(fltr: Descriptor) -> Descriptor {
    let id = code(fltr.class_id.as_bytes().try_into().unwrap());
    Descriptor::new("filterFX")
        .with("Nm  ", Value::Text(UnicodeString::new_nul("from Photoshop")))
        .with("blendOptions", Value::Descriptor(Descriptor::new("blendOptions").with("Opct", unit(b"#Prc", 100.0)).with("Md  ", enumv("BlnM", "normal"))))
        .with("enab", Value::Boolean(true))
        .with("hasoptions", Value::Boolean(true))
        .with("Fltr", Value::Descriptor(fltr))
        .with("filterID", Value::Integer(id))
}

fn wind(method: &str, direction: &str) -> Descriptor {
    Descriptor::new("Wnd ").with("WndM", enumv("WndM", method)).with("Drct", enumv("Drct", direction))
}

fn extrude(t: &str, size: Value, depth: Value, mode: &str, solid: Value, mask: Value) -> Descriptor {
    Descriptor::new("Extr")
        .with("ExtT", enumv("ExtT", t))
        .with("ExtS", size)
        .with("ExtD", depth)
        .with("ExtR", enumv("ExtR", mode))
        .with("ExtF", solid)
        .with("ExtM", mask)
}

fn tiles(count: Value, offset: Value, fill: &str) -> Descriptor {
    Descriptor::new("Tls ").with("TlNm", count).with("TlOf", offset).with("FlCl", enumv("FlCl", fill))
}

#[test]
fn wind_extrude_tiles_open_from_photoshop_shapes() {
    let (long, yes, no) = (Value::Integer, Value::Boolean(true), Value::Boolean(false));
    let cases = [
        // The measured files.
        (wind("Blst", "Left"), "filter.stylize.wind", json!({"method": "blast", "direction": "fromRight"})),
        (wind("Stgr", "Rght"), "filter.stylize.wind", json!({"method": "stagger", "direction": "fromLeft"})),
        (wind("Wnd ", "Rght"), "filter.stylize.wind", json!({"method": "wind", "direction": "fromLeft"})),
        (
            extrude("Blks", long(7), long(9), "Rndm", yes.clone(), no.clone()),
            "filter.stylize.extrude",
            json!({"type": "blocks", "size": 7, "depth": 9, "depthMode": "random", "solidFrontFaces": true, "maskIncompleteBlocks": false}),
        ),
        (
            extrude("Pyrm", long(12), long(20), "LvlB", no, yes),
            "filter.stylize.extrude",
            json!({"type": "pyramids", "size": 12, "depth": 20, "depthMode": "levelBased", "solidFrontFaces": false, "maskIncompleteBlocks": true}),
        ),
        (
            tiles(long(6), long(20), "FlFr"),
            "filter.stylize.tiles",
            json!({"count": 6, "maxOffset": 20, "fill": "foreground", "foreground": [0.2, 0.2, 0.2, 1.0], "background": [1.0, 1.0, 1.0, 1.0]}),
        ),
        (
            tiles(long(4), long(5), "FlIn"),
            "filter.stylize.tiles",
            json!({"count": 4, "maxOffset": 5, "fill": "inverse", "foreground": [0.2, 0.2, 0.2, 1.0], "background": [1.0, 1.0, 1.0, 1.0]}),
        ),
        (
            tiles(long(10), long(10), "FlBc"),
            "filter.stylize.tiles",
            json!({"count": 10, "maxOffset": 10, "fill": "background", "foreground": [0.2, 0.2, 0.2, 1.0], "background": [1.0, 1.0, 1.0, 1.0]}),
        ),
        (
            tiles(long(99), long(1), "FlSm"),
            "filter.stylize.tiles",
            json!({"count": 99, "maxOffset": 1, "fill": "unaltered", "foreground": [0.2, 0.2, 0.2, 1.0], "background": [1.0, 1.0, 1.0, 1.0]}),
        ),
    ];
    for (fltr, command, params) in cases {
        let item = measured_item(fltr, 51.0, 255.0);
        // Through bytes, as read from a file.
        let f = filter_from_item(&Descriptor::from_bytes(&item.to_bytes()).unwrap());
        assert_eq!((f.command.as_str(), &f.params), (command, &params));
        // Written back in the measured shape: same class, keys and their order, value types
        // (long, bool, enum with its type), filterID; Tiles also its colours.
        let back = item_for_filter(&f).unwrap();
        assert_eq!(get_desc(&back, "Fltr").map(Descriptor::to_bytes), get_desc(&item, "Fltr").map(Descriptor::to_bytes), "{command}");
        assert_eq!(back.get("filterID"), item.get("filterID"), "{command}");
        if command == "filter.stylize.tiles" {
            assert_eq!((back.get("FrgC"), back.get("BckC")), (item.get("FrgC"), item.get("BckC")));
        }
    }
    // The class codes: numeric filterIDs of the four-char classes.
    for (c, id) in [("filter.stylize.wind", 1466852384), ("filter.stylize.extrude", 1165522034), ("filter.stylize.tiles", 1416393504)] {
        assert_eq!(item_for_filter(&sf(c, json!({}))).unwrap().get("filterID"), Some(&Value::Integer(id)));
    }
    // A filter found by its filterID alone (no `Fltr` class to go by) still needs its settings.
    let bare = Descriptor::new("filterFX").with("hasoptions", Value::Boolean(false)).with("filterID", Value::Integer(1466852384));
    assert_eq!(filter_from_item(&bare).command, UNSUPPORTED_FILTER);
    // Only Tiles reads the entry's colours; Wind and Extrude write the defaults.
    let f = filter_from_item(&measured_item(wind("Wnd ", "Left"), 51.0, 0.0));
    assert_eq!(f.params, json!({"method": "wind", "direction": "fromRight"}));
    let back = item_for_filter(&f).unwrap();
    assert_eq!((back.get("FrgC"), back.get("BckC")), (Some(&rgbc(0.0)), Some(&rgbc(255.0))));
    // Tiles without colours in its params (or the entry) writes the engine's black and white.
    let f = filter_from_item(&Descriptor::new("filterFX").with("Fltr", Value::Descriptor(tiles(long(4), long(5), "FlBc"))));
    assert_eq!(f.params, json!({"count": 4, "maxOffset": 5, "fill": "background"}));
    let back = item_for_filter(&f).unwrap();
    assert_eq!((back.get("FrgC"), back.get("BckC")), (Some(&rgbc(0.0)), Some(&rgbc(255.0))));
}

#[test]
fn wind_extrude_tiles_write_defaults_and_photoshop_ranges() {
    let fltr = |c: &str, p: J| match item_for_filter(&sf(c, p)).unwrap().get("Fltr") {
        Some(Value::Descriptor(d)) => d.clone(),
        other => panic!("no Fltr: {other:?}"),
    };
    let long = Value::Integer;
    let (yes, no) = (Value::Boolean(true), Value::Boolean(false));
    // Defaults are the engine's; our random seed has no PSD key and is not written.
    assert_eq!(fltr("filter.stylize.wind", json!({"seed": 9})), wind("Wnd ", "Left"));
    assert_eq!(fltr("filter.stylize.extrude", json!({"seed": 9})), extrude("Blks", long(30), long(30), "Rndm", no.clone(), no.clone()));
    assert_eq!(fltr("filter.stylize.tiles", json!({"seed": 9})), tiles(long(10), long(10), "FlBc"));
    // Out of range, fractional, non-numeric and non-bool values as the engine reads them.
    assert_eq!(
        fltr("filter.stylize.extrude", json!({"size": 1e12, "depth": -5, "solidFrontFaces": "yes", "maskIncompleteBlocks": true})),
        extrude("Blks", long(255), long(1), "Rndm", no.clone(), yes)
    );
    assert_eq!(fltr("filter.stylize.tiles", json!({"count": 0, "maxOffset": 12.6})), tiles(long(1), long(13), "FlBc"));
    assert_eq!(fltr("filter.stylize.tiles", json!({"count": "many", "maxOffset": 1000})), tiles(long(10), long(99), "FlBc"));
    assert_eq!(fltr("filter.stylize.extrude", json!({"size": f64::NAN})), extrude("Blks", long(30), long(30), "Rndm", no.clone(), no));
}

#[test]
fn malformed_wind_extrude_tiles_are_kept_verbatim() {
    let long = Value::Integer;
    let (yes, no) = (Value::Boolean(true), Value::Boolean(false));
    let cases = [
        // Missing keys.
        Descriptor::new("Wnd "),
        Descriptor::new("Wnd ").with("WndM", enumv("WndM", "Blst")),
        Descriptor::new("Wnd ").with("Drct", enumv("Drct", "Left")),
        Descriptor::new("Extr").with("ExtT", enumv("ExtT", "Blks")).with("ExtS", long(7)).with("ExtD", long(9)).with("ExtR", enumv("ExtR", "Rndm")),
        Descriptor::new("Tls ").with("TlNm", long(6)).with("TlOf", long(20)),
        Descriptor::new("Tls ").with("TlNm", long(6)).with("FlCl", enumv("FlCl", "FlFr")),
        // Unknown enum values and enum types (Photoshop falls back to a default for these;
        // we keep them as they are rather than guess).
        wind("Wild", "Left"),
        wind("Blst", "Up  "),
        Descriptor::new("Wnd ").with("WndM", enumv("Drct", "Blst")).with("Drct", enumv("Drct", "Left")),
        extrude("Cube", long(7), long(9), "Rndm", yes.clone(), no.clone()),
        extrude("Blks", long(7), long(9), "Sine", yes.clone(), no.clone()),
        tiles(long(6), long(20), "FlXx"),
        Descriptor::new("Tls ").with("TlNm", long(6)).with("TlOf", long(20)).with("FlCl", enumv("BlnM", "FlFr")),
        // Wrong types (also NaN).
        Descriptor::new("Wnd ").with("WndM", long(1)).with("Drct", enumv("Drct", "Left")),
        extrude("Blks", Value::Double(7.0), long(9), "Rndm", yes.clone(), no.clone()),
        extrude("Blks", long(7), unit(b"#Pxl", 9.0), "Rndm", yes.clone(), no.clone()),
        extrude("Blks", long(7), long(9), "Rndm", long(1), no.clone()),
        extrude("Blks", long(7), long(9), "Rndm", yes.clone(), Value::Double(f64::NAN)),
        tiles(Value::Double(f64::NAN), long(20), "FlFr"),
        tiles(long(6), Value::Text(UnicodeString::new_nul("20")), "FlFr"),
        // Out of range.
        extrude("Blks", long(1), long(9), "Rndm", yes.clone(), no.clone()),
        extrude("Blks", long(256), long(9), "Rndm", yes.clone(), no.clone()),
        extrude("Blks", long(7), long(0), "Rndm", yes.clone(), no.clone()),
        extrude("Blks", long(7), long(i32::MAX), "Rndm", yes, no),
        tiles(long(0), long(20), "FlFr"),
        tiles(long(100), long(20), "FlFr"),
        tiles(long(6), long(0), "FlFr"),
        tiles(long(6), long(i32::MIN), "FlFr"),
    ];
    for fltr in cases {
        let item = measured_item(fltr, 0.0, 255.0);
        let f = filter_from_item(&Descriptor::from_bytes(&item.to_bytes()).unwrap());
        assert_eq!(f.command, UNSUPPORTED_FILTER, "{item:?}");
        assert_eq!(item_for_filter(&f).unwrap().to_bytes(), item.to_bytes());
    }
    // A Tiles entry whose colour is unreadable is kept verbatim too.
    for (key, bad) in [
        ("FrgC", Value::Integer(3)),
        ("BckC", Value::Descriptor(Descriptor::new("RGBC").with("Rd  ", Value::Double(0.0)))),
        (
            "BckC",
            Value::Descriptor(
                Descriptor::new("RGBC").with("Rd  ", Value::Double(f64::NAN)).with("Grn ", Value::Double(0.0)).with("Bl  ", Value::Double(0.0)),
            ),
        ),
    ] {
        let mut item = measured_item(tiles(long(6), long(20), "FlFr"), 0.0, 255.0);
        set(&mut item, key, bad);
        assert_eq!(filter_from_item(&item).command, UNSUPPORTED_FILTER);
    }
    // Values with no PSD equivalent are reported, not guessed.
    for (c, p) in [
        ("filter.stylize.wind", json!({"method": "gale"})),
        ("filter.stylize.wind", json!({"direction": "fromTop"})),
        ("filter.stylize.wind", json!({"direction": 1})),
        ("filter.stylize.extrude", json!({"type": "cubes"})),
        ("filter.stylize.extrude", json!({"depthMode": null})),
        ("filter.stylize.tiles", json!({"fill": "transparent"})),
        ("filter.stylize.tiles", json!({"foreground": "teal"})),
        ("filter.stylize.tiles", json!({"background": [1.0]})),
        ("filter.stylize.tiles", json!({"background": [f64::NAN, 0.0, 0.0]})),
    ] {
        assert!(item_for_filter(&sf(c, p.clone())).is_err(), "{c} {p}");
    }
}

/// Photoshop's default Custom kernel: a 3 × 3 sharpen centred in the 5 × 5.
const SHARPEN: [i32; 25] = [0, 0, 0, 0, 0, 0, 0, -1, 0, 0, 0, -1, 5, -1, 0, 0, 0, -1, 0, 0, 0, 0, 0, 0, 0];

/// A `filterFXList` item as measured in Photoshop (applied through Action Manager to a smart
/// object, saved, `filterFX` read back).
fn measured_named_item(id: &[u8; 4], name: &str, fltr: Descriptor) -> Descriptor {
    Descriptor::new("filterFX")
        .with("Nm  ", Value::Text(UnicodeString::new_nul(name)))
        .with("blendOptions", blend_options(&sf("", J::Null)))
        .with("enab", Value::Boolean(true))
        .with("hasoptions", Value::Boolean(true))
        .with("FrgC", rgbc(0.0))
        .with("BckC", rgbc(255.0))
        .with("Fltr", Value::Descriptor(fltr))
        .with("filterID", Value::Integer(code(id)))
}

fn offset_fltr(h: Value, v: Value, fill: Value) -> Descriptor {
    Descriptor::new("Ofst").with("Hrzn", h).with("Vrtc", v).with("Fl  ", fill)
}

fn custom_fltr(scale: Value, offset: Value, m: Vec<Value>) -> Descriptor {
    Descriptor::new("Cstm").with("Scl ", scale).with("Ofst", offset).with("Mtrx", Value::List(m))
}

fn longs(m: &[i32]) -> Vec<Value> {
    m.iter().map(|v| Value::Integer(*v)).collect()
}

#[test]
fn offset_round_trips_every_fill_mode() {
    for (fill, ours) in [("Wrp ", "wrap"), ("Rpt ", "repeat"), ("Bckg", "transparent")] {
        for (h, v) in [(0, 0), (12, -7), (5, 3), (i32::MIN, i32::MAX)] {
            let f = sf("filter.other.offset", json!({"horizontal": h, "vertical": v, "undefinedAreas": ours}));
            let d = item_for_filter(&f).unwrap();
            let fltr = get_desc(&d, "Fltr").unwrap();
            assert_eq!(*fltr, offset_fltr(Value::Integer(h), Value::Integer(v), enumv("FlMd", fill)));
            assert_eq!(int(&d, "filterID"), Some(1_332_114_292));
            assert_eq!(filter_from_item(&Descriptor::from_bytes(&d.to_bytes()).unwrap()), f);
        }
    }
}

/// The measured descriptors (Wrap 0/0, Set to Background 12/−7, Repeat 5/3) open as Offset.
#[test]
fn offset_measured_descriptors_open_as_offset() {
    for (h, v, fill, ours) in [(0, 0, "Wrp ", "wrap"), (12, -7, "Bckg", "transparent"), (5, 3, "Rpt ", "repeat")] {
        let item = measured_named_item(b"Ofst", "Offset...", offset_fltr(Value::Integer(h), Value::Integer(v), enumv("FlMd", fill)));
        let f = filter_from_item(&item);
        assert_eq!(f.command, "filter.other.offset");
        assert_eq!(f.params, json!({"horizontal": h, "vertical": v, "undefinedAreas": ours}));
        // Written back exactly as measured.
        assert_eq!(item_for_filter(&f).unwrap(), item);
    }
}

#[test]
fn offset_written_with_engine_defaults() {
    // No params: 0, 0, Wrap Around (the engine's defaults); fractional pixels round.
    let d = item_for_filter(&sf("filter.other.offset", json!({}))).unwrap();
    assert_eq!(*get_desc(&d, "Fltr").unwrap(), offset_fltr(Value::Integer(0), Value::Integer(0), enumv("FlMd", "Wrp ")));
    let d = item_for_filter(&sf("filter.other.offset", json!({"horizontal": 2.6, "vertical": 3e12, "undefinedAreas": "bogus"}))).unwrap();
    assert_eq!(*get_desc(&d, "Fltr").unwrap(), offset_fltr(Value::Integer(3), Value::Integer(i32::MAX), enumv("FlMd", "Wrp ")));
}

#[test]
fn malformed_offset_is_kept_verbatim() {
    let i = Value::Integer;
    for fltr in [
        offset_fltr(i(1), i(2), enumv("FlMd", "Xxxx")),
        offset_fltr(i(1), i(2), Value::Integer(1)),
        offset_fltr(Value::Double(1.5), i(2), enumv("FlMd", "Wrp ")),
        offset_fltr(i(1), unit(b"#Pxl", f64::NAN), enumv("FlMd", "Wrp ")),
        Descriptor::new("Ofst").with("Hrzn", i(1)).with("Fl  ", enumv("FlMd", "Wrp ")),
        Descriptor::new("Ofst"),
        // A mistyped enum, a setting we don't write back and a repeated one.
        offset_fltr(i(1), i(2), enumv("Xxxx", "Wrp ")),
        offset_fltr(i(1), i(2), enumv("FlMd", "Wrp ")).with("Xtra", i(1)),
        offset_fltr(i(1), i(2), enumv("FlMd", "Wrp ")).with("Hrzn", i(3)),
    ] {
        let item = measured_named_item(b"Ofst", "Offset...", fltr);
        let f = filter_from_item(&item);
        assert_eq!(f.command, UNSUPPORTED_FILTER, "{item:?}");
        assert_eq!(item_for_filter(&f).unwrap().to_bytes(), item.to_bytes(), "kept byte for byte");
    }
    let bare = Descriptor::new("filterFX").with("filterID", Value::Integer(code(b"Ofst")));
    assert_eq!(filter_from_item(&bare).command, UNSUPPORTED_FILTER);
}

#[test]
fn custom_round_trips() {
    let edge: Vec<i32> = (0..25)
        .map(|i| {
            if i == 12 {
                8
            } else if (6..=18).contains(&i) && i % 5 != 0 && i % 5 != 4 {
                -1
            } else {
                0
            }
        })
        .collect();
    let full: Vec<i32> = (0..25).map(|i| i * 83 - 999).collect();
    for (kernel, scale, offset) in [(SHARPEN.to_vec(), 1, 0), (edge, 2, 10), (full, 9999, -9999), (vec![999; 25], 1, 9999)] {
        let f = sf("filter.other.custom", json!({"kernel": kernel, "scale": scale, "offset": offset}));
        let d = item_for_filter(&f).unwrap();
        assert_eq!(*get_desc(&d, "Fltr").unwrap(), custom_fltr(Value::Integer(scale), Value::Integer(offset), longs(&kernel)));
        assert_eq!(int(&d, "filterID"), Some(1_131_639_917));
        assert_eq!(filter_from_item(&Descriptor::from_bytes(&d.to_bytes()).unwrap()), f);
    }
}

/// The measured descriptors (the default sharpen; a blur at Scale 2, Offset 10) open as Custom.
#[test]
fn custom_measured_descriptors_open_as_custom() {
    let blur = [0, 0, 0, 0, 0, 0, 1, 2, 1, 0, 0, 2, 4, 2, 0, 0, 1, 2, 1, 0, 0, 0, 0, 0, 0];
    for (kernel, scale, offset) in [(SHARPEN, 1, 0), (blur, 2, 10)] {
        let item = measured_named_item(b"Cstm", "Custom...", custom_fltr(Value::Integer(scale), Value::Integer(offset), longs(&kernel)));
        let f = filter_from_item(&item);
        assert_eq!(f.command, "filter.other.custom");
        assert_eq!(f.params, json!({"kernel": kernel, "scale": scale, "offset": offset}));
        assert_eq!(item_for_filter(&f).unwrap(), item);
    }
}

#[test]
fn custom_written_like_the_engine_reads_it() {
    let fltr = |p: J| get_desc(&item_for_filter(&sf("filter.other.custom", p)).unwrap(), "Fltr").cloned().unwrap();
    // No kernel: the engine's identity; short kernels pad with 0; out-of-range values clamp.
    let mut id = [0; 25];
    id[12] = 1;
    assert_eq!(fltr(json!({})), custom_fltr(Value::Integer(1), Value::Integer(0), longs(&id)));
    let mut short = [0; 25];
    short[0] = 3;
    short[1] = -999;
    assert_eq!(fltr(json!({"kernel": [3, -5000], "scale": 0, "offset": 20000})), custom_fltr(Value::Integer(1), Value::Integer(9999), longs(&short)));
    // Fractions have no PSD equivalent.
    for p in [json!({"kernel": [0.5]}), json!({"scale": 1.5}), json!({"offset": -0.25})] {
        assert!(item_for_filter(&sf("filter.other.custom", p.clone())).is_err(), "{p}");
    }
}

#[test]
fn malformed_custom_is_kept_verbatim() {
    let i = Value::Integer;
    let mut wrong_type = longs(&SHARPEN);
    wrong_type[3] = Value::Double(1.0);
    for fltr in [
        custom_fltr(i(1), i(0), longs(&SHARPEN[..24])),
        custom_fltr(i(1), i(0), longs(&[0; 26])),
        custom_fltr(i(1), i(0), wrong_type),
        custom_fltr(i(1), i(0), longs(&[1000; 25])),
        custom_fltr(i(0), i(0), longs(&SHARPEN)),
        custom_fltr(i(10_000), i(0), longs(&SHARPEN)),
        custom_fltr(i(1), i(-10_000), longs(&SHARPEN)),
        custom_fltr(Value::Double(f64::NAN), i(0), longs(&SHARPEN)),
        custom_fltr(i(1), unit(b"#Pxl", 1.0), longs(&SHARPEN)),
        Descriptor::new("Cstm").with("Scl ", i(1)).with("Ofst", i(0)).with("Mtrx", i(5)),
        Descriptor::new("Cstm").with("Scl ", i(1)).with("Mtrx", Value::List(longs(&SHARPEN))),
        Descriptor::new("Cstm"),
        // A setting we don't write back.
        custom_fltr(i(1), i(0), longs(&SHARPEN)).with("Xtra", i(1)),
    ] {
        let item = measured_named_item(b"Cstm", "Custom...", fltr);
        let f = filter_from_item(&item);
        assert_eq!(f.command, UNSUPPORTED_FILTER, "{item:?}");
        assert_eq!(item_for_filter(&f).unwrap().to_bytes(), item.to_bytes(), "kept byte for byte");
    }
}

/// A `filterFX` item holding `fltr`, as Photoshop writes a filter with options.
fn pixelate_item(name: &str, fltr: Descriptor, filter_id: i32) -> Descriptor {
    Descriptor::new("filterFX")
        .with("Nm  ", Value::Text(UnicodeString::new_nul(name)))
        .with("blendOptions", Value::Descriptor(Descriptor::new("blendOptions").with("Opct", unit(b"#Prc", 100.0)).with("Md  ", enumv("BlnM", "Nrml"))))
        .with("enab", Value::Boolean(true))
        .with("hasoptions", Value::Boolean(true))
        .with("FrgC", rgbc(0.0))
        .with("BckC", rgbc(255.0))
        .with("Fltr", Value::Descriptor(fltr))
        .with("filterID", Value::Integer(filter_id))
}

#[test]
fn mezzotint_round_trips_every_type_and_reads_photoshops_shape() {
    for (code, ty) in MEZZOTINT_TYPES {
        let f = sf("filter.pixelate.mezzotint", json!({"type": ty}));
        let d = item_for_filter(&f).unwrap();
        assert_eq!(int(&d, "filterID"), Some(1_299_870_830));
        let fltr = get_desc(&d, "Fltr").unwrap();
        assert!(fltr.class_id.is("Mztn"));
        assert_eq!(enum_of(fltr, "MztT"), Some(&code[..]));
        assert!(matches!(fltr.get("MztT"), Some(Value::Enumerated { type_id, .. }) if type_id.is("MztT")));
        assert_eq!(filter_from_item(&Descriptor::from_bytes(&d.to_bytes()).unwrap()), f, "{ty}");
    }
    // Measured in Photoshop: `{'_class': 'Mztn', 'MztT': 'MztT.CrsD'}` and `LngS`, filterID 1299870830.
    for (code, ty) in [("CrsD", "coarseDots"), ("LngS", "longStrokes")] {
        let item = pixelate_item("Mezzotint...", Descriptor::new("Mztn").with("MztT", enumv("MztT", code)), 1_299_870_830);
        let f = filter_from_item(&item);
        assert_eq!((f.command.as_str(), &f.params), ("filter.pixelate.mezzotint", &json!({"type": ty})));
    }
    // No type or an unknown type is Fine Dots, like the engine; the seed
    // (PhotoCraft only) is not saved.
    for p in [json!({}), json!({"type": "sparkles"}), json!({"type": "fineDots", "seed": 77})] {
        let d = item_for_filter(&sf("filter.pixelate.mezzotint", p)).unwrap();
        let fltr = get_desc(&d, "Fltr").unwrap();
        assert_eq!(enum_of(fltr, "MztT"), Some(&b"FnDt"[..]));
        assert_eq!(fltr.items.len(), 1);
    }
}

#[test]
fn color_halftone_round_trips_and_reads_photoshops_shape() {
    for p in [
        json!({"maxRadius": 8, "channel1": 108, "channel2": 162, "channel3": 90, "channel4": 45}),
        json!({"maxRadius": 4, "channel1": -360, "channel2": 0, "channel3": 360, "channel4": -45}),
        json!({"maxRadius": 127, "channel1": 1, "channel2": 2, "channel3": 3, "channel4": 4}),
    ] {
        let f = sf("filter.pixelate.colorHalftone", p);
        let d = item_for_filter(&f).unwrap();
        assert_eq!(int(&d, "filterID"), Some(1_131_180_616));
        let fltr = get_desc(&d, "Fltr").unwrap();
        assert!(fltr.class_id.is("ClrH"));
        for k in ["Rds ", "Ang1", "Ang2", "Ang3", "Ang4"] {
            assert!(matches!(fltr.get(k), Some(Value::Integer(_))), "{k} is a long");
        }
        assert_eq!(filter_from_item(&Descriptor::from_bytes(&d.to_bytes()).unwrap()), f);
    }
    // Measured in Photoshop: `{'_class': 'ClrH', 'Rds ': 6, 'Ang1': 10, 'Ang2': 20, 'Ang3': 30, 'Ang4': 40}`.
    let measured = Descriptor::new("ClrH")
        .with("Rds ", Value::Integer(6))
        .with("Ang1", Value::Integer(10))
        .with("Ang2", Value::Integer(20))
        .with("Ang3", Value::Integer(30))
        .with("Ang4", Value::Integer(40));
    let item = pixelate_item("Color Halftone...", measured.clone(), 1_131_180_616);
    let f = filter_from_item(&item);
    assert_eq!(f.command, "filter.pixelate.colorHalftone");
    assert_eq!(f.params, json!({"maxRadius": 6, "channel1": 10, "channel2": 20, "channel3": 30, "channel4": 40}));
    assert_eq!(get_desc(&item_for_filter(&f).unwrap(), "Fltr"), Some(&measured));
    // Defaults and hostile values write longs within the dialog's ranges.
    let d = item_for_filter(&sf("filter.pixelate.colorHalftone", json!({"maxRadius": f64::NAN, "channel1": 1e12, "channel2": -1e12, "channel3": "x"})))
        .unwrap();
    let fltr = get_desc(&d, "Fltr").unwrap();
    let got: Vec<_> = ["Rds ", "Ang1", "Ang2", "Ang3", "Ang4"].iter().map(|k| int(fltr, k)).collect();
    assert_eq!(got, [Some(8), Some(360), Some(-360), Some(90), Some(45)]);
}

#[test]
fn malformed_pixelate_filters_stay_verbatim() {
    let mz = |v: Value| Descriptor::new("Mztn").with("MztT", v);
    let ch = |rds: Value, ang1: Value| {
        Descriptor::new("ClrH")
            .with("Rds ", rds)
            .with("Ang1", ang1)
            .with("Ang2", Value::Integer(20))
            .with("Ang3", Value::Integer(30))
            .with("Ang4", Value::Integer(40))
    };
    let cases = [
        pixelate_item("Mezzotint...", Descriptor::new("Mztn"), code(b"Mztn")),
        pixelate_item("Mezzotint...", mz(enumv("MztT", "Sprk")), code(b"Mztn")),
        pixelate_item("Mezzotint...", mz(Value::Integer(1)), code(b"Mztn")),
        pixelate_item("Color Halftone...", Descriptor::new("ClrH"), code(b"ClrH")),
        pixelate_item("Color Halftone...", ch(Value::Double(6.0), Value::Integer(10)), code(b"ClrH")),
        pixelate_item("Color Halftone...", ch(unit(b"#Pxl", 6.0), Value::Integer(10)), code(b"ClrH")),
        pixelate_item("Color Halftone...", ch(Value::Integer(3), Value::Integer(10)), code(b"ClrH")),
        pixelate_item("Color Halftone...", ch(Value::Integer(128), Value::Integer(10)), code(b"ClrH")),
        pixelate_item("Color Halftone...", ch(Value::Integer(6), Value::Integer(361)), code(b"ClrH")),
        pixelate_item("Color Halftone...", ch(Value::Integer(6), Value::Integer(i32::MIN)), code(b"ClrH")),
        pixelate_item("Color Halftone...", ch(Value::Integer(6), Value::Double(f64::NAN)), code(b"ClrH")),
        // A mistyped enum and settings we don't write back.
        pixelate_item("Mezzotint...", mz(enumv("Xxxx", "FnDt")), code(b"Mztn")),
        pixelate_item("Mezzotint...", mz(enumv("MztT", "FnDt")).with("Xtra", Value::Integer(1)), code(b"Mztn")),
        pixelate_item("Color Halftone...", ch(Value::Integer(6), Value::Integer(10)).with("Xtra", Value::Integer(1)), code(b"ClrH")),
        pixelate_item("Color Halftone...", ch(Value::Integer(6), Value::Integer(10)).with("Ang1", Value::Integer(11)), code(b"ClrH")),
    ];
    for item in cases {
        let f = filter_from_item(&item);
        assert_eq!(f.command, UNSUPPORTED_FILTER, "{:?}", get_desc(&item, "Fltr"));
        // Written back with its own settings (blending is rewritten from the model).
        let back = item_for_filter(&f).unwrap();
        // Compared as bytes: a NaN setting is not equal to itself.
        let fltr = |d: &Descriptor| get_desc(d, "Fltr").map(Descriptor::to_bytes);
        assert_eq!((fltr(&back), int(&back, "filterID")), (fltr(&item), int(&item, "filterID")));
    }
    // Without a `Fltr` the filter has no settings to read: kept verbatim too.
    let bare = Descriptor::new("filterFX").with("filterID", Value::Integer(code(b"ClrH")));
    assert_eq!(filter_from_item(&bare).command, UNSUPPORTED_FILTER);
}

/// Distort smart filters from a Photoshop file open as their PhotoCraft filters, and the ones
/// PhotoCraft writes carry the same settings, keys and order as Photoshop's.
#[test]
fn photoshop_distort_filters_read_and_write_like_photoshop() {
    for (command, params, hex) in PHOTOSHOP_FLTR {
        let ps = Descriptor::from_bytes(&from_hex(hex).unwrap()).unwrap();
        let f = filter_from_item(&photoshop_fltr_item(ps.clone()));
        let want: J = serde_json::from_str(params).unwrap();
        assert_eq!((f.command.as_str(), &f.params), (command, &want), "{command}");
        let ours = match item_for_filter(&f).unwrap().get("Fltr") {
            Some(Value::Descriptor(d)) => d.clone(),
            other => panic!("{command}: no Fltr ({other:?})"),
        };
        assert_eq!((&ours.class_id, &ours.items), (&ps.class_id, &ps.items), "{command}: same keys, values and order");
    }
    // An extra or repeated setting PhotoCraft doesn't model keeps the filter Photoshop's, unchanged.
    let fltrs: Vec<Descriptor> = PHOTOSHOP_FLTR.iter().map(|(_, _, hex)| Descriptor::from_bytes(&from_hex(hex).unwrap()).unwrap()).collect();
    for ps in fltrs {
        let name = String::from_utf8_lossy(ps.class_id.as_bytes()).into_owned();
        let mut repeated = ps.clone();
        repeated.items.push(ps.items[0].clone());
        for odd in [ps.clone().with("Xtra", Value::Integer(7)), repeated] {
            let item = photoshop_fltr_item(odd);
            let f = filter_from_item(&item);
            assert_eq!(f.command, UNSUPPORTED_FILTER, "{name}");
            assert_eq!(item_for_filter(&f).unwrap().get("Fltr"), item.get("Fltr"), "{name}: written back unchanged");
        }
    }
}

#[test]
fn invert_smart_filter_reads_and_writes_like_photoshop() {
    let item = Descriptor::new("filterFX")
        .with("Nm  ", Value::Text(UnicodeString::new_nul("Invert")))
        .with("blendOptions", Value::Descriptor(Descriptor::new("blendOptions").with("Opct", unit(b"#Prc", 100.0)).with("Md  ", enumv("BlnM", "normal"))))
        .with("enab", Value::Boolean(true))
        .with("hasoptions", Value::Boolean(false))
        .with("FrgC", rgbc(255.0))
        .with("BckC", rgbc(0.0))
        .with("filterID", Value::Integer(code(b"Invr")));
    let f = filter_from_item(&item);
    assert_eq!((f.command.as_str(), &f.params), ("image.adjustments.invert", &json!({})));
    let back = item_for_filter(&f).unwrap();
    assert_eq!(back.get("Fltr"), None);
    assert_eq!(back.get("hasoptions"), Some(&Value::Boolean(false)));
    assert_eq!(back.get("filterID"), Some(&Value::Integer(code(b"Invr"))));
    assert_eq!(back.get("Nm  "), Some(&Value::Text(UnicodeString::new_nul("Invert"))));
}

/// A filter PhotoCraft doesn't model, or a known one with settings it doesn't, is kept verbatim; only its visibility and blend change.
#[test]
fn photoshop_sharpen_without_fltr_imports_as_sharpen() {
    // #6267: Photoshop's Sharpen item holds only its name, blending and `filterID`.
    let item = Descriptor::new("filterFX")
        .with("Nm  ", Value::Text(UnicodeString::new_nul("Sharpen")))
        .with("blendOptions", Value::Descriptor(Descriptor::new("blendOptions").with("Opct", unit(b"#Prc", 100.0)).with("Md  ", enumv("BlnM", "normal"))))
        .with("enab", Value::Boolean(true))
        .with("hasoptions", Value::Boolean(false))
        .with("filterID", Value::Integer(1_399_353_968));
    let f = filter_from_item(&item);
    assert_eq!((f.command.as_str(), &f.params), ("filter.sharpen.sharpen", &json!({})));
    // Hue/Saturation with an unnumbered range and Dust & Scratches with a missing setting fail
    // cleanly instead of panicking.
    let bad = SmartFilter {
        command: "image.adjustments.hueSaturation".into(),
        params: json!({"reds": {"range": [1, "x", 3, 4]}}),
        blend: BlendMode::Normal,
        opacity: 1.0,
        visible: true,
    };
    assert!(item_for_filter(&bad).is_err());
    let dust = Descriptor::new("filterFX")
        .with("Fltr", Value::Descriptor(Descriptor::new("DstS").with("Rds ", Value::Integer(3))))
        .with("filterID", Value::Integer(code(b"DstS")));
    assert_eq!(filter_from_item(&dust).command, UNSUPPORTED_FILTER);
}

fn ps_item(name: &str, fltr: Descriptor, filter_id: i32) -> Descriptor {
    Descriptor::new("filterFX")
        .with("Nm  ", Value::Text(UnicodeString::new_nul(name)))
        .with("blendOptions", Value::Descriptor(Descriptor::new("blendOptions").with("Opct", unit(b"#Prc", 100.0)).with("Md  ", enumv("BlnM", "normal"))))
        .with("enab", Value::Boolean(true))
        .with("hasoptions", Value::Boolean(true))
        .with("FrgC", rgbc(0.0))
        .with("BckC", rgbc(255.0))
        .with("Fltr", Value::Descriptor(fltr))
        .with("filterID", Value::Integer(filter_id))
}

fn surface_blur(rds: f64, thsh: i32) -> Descriptor {
    Descriptor::new("surfaceBlur").with("Rds ", unit(b"#Pxl", rds)).with("Thsh", Value::Integer(thsh))
}

fn smart_blur(rds: f64, thsh: f64, q: &str, m: &str) -> Descriptor {
    Descriptor::new("SmrB").with("Rds ", Value::Double(rds)).with("Thsh", Value::Double(thsh)).with("SmBQ", enumv("SmBQ", q)).with("SmBM", enumv("SmBM", m))
}

fn radial_blur(amnt: i32, m: &str, q: &str) -> Descriptor {
    Descriptor::new("RdlB").with("Amnt", Value::Integer(amnt)).with("BlrM", enumv("BlrM", m)).with("BlrQ", enumv("BlrQ", q))
}

/// The `Fltr` shapes measured in Photoshop (each filter applied to a smart object through
/// Action Manager, saved, and `filterFX` read back) open as our filters and are written back
/// with the same keys, value types, units, enums and filter ids.
#[test]
fn blur_family_matches_photoshop_shapes() {
    let smart = code(b"SmrB");
    let radial = code(b"RdlB");
    assert_eq!((smart, radial), (1_399_681_602, 1_382_313_026));
    let cases = [
        (surface_blur(5.0, 15), 1135, "filter.blur.surfaceBlur", json!({"radius": 5.0, "threshold": 15})),
        (surface_blur(12.0, 40), 1135, "filter.blur.surfaceBlur", json!({"radius": 12.0, "threshold": 40})),
        (
            smart_blur(5.0, 25.0, "SBQH", "SBMN"),
            smart,
            "filter.blur.smartBlur",
            json!({"radius": 5.0, "threshold": 25.0, "quality": "high", "mode": "normal"}),
        ),
        (
            smart_blur(3.0, 30.0, "SBQL", "SBME"),
            smart,
            "filter.blur.smartBlur",
            json!({"radius": 3.0, "threshold": 30.0, "quality": "low", "mode": "edgeOnly"}),
        ),
        (
            smart_blur(3.0, 30.0, "SBQM", "SBMO"),
            smart,
            "filter.blur.smartBlur",
            json!({"radius": 3.0, "threshold": 30.0, "quality": "medium", "mode": "overlayEdge"}),
        ),
        (radial_blur(10, "Spn ", "Drft"), radial, "filter.blur.radialBlur", json!({"amount": 10, "method": "spin", "quality": "draft"})),
        (radial_blur(20, "Spn ", "Bst "), radial, "filter.blur.radialBlur", json!({"amount": 20, "method": "spin", "quality": "best"})),
        (radial_blur(20, "Zm  ", "Gd  "), radial, "filter.blur.radialBlur", json!({"amount": 20, "method": "zoom", "quality": "good"})),
    ];
    for (fltr, id, command, params) in cases {
        let item = ps_item("x", fltr.clone(), id);
        let f = filter_from_item(&Descriptor::from_bytes(&item.to_bytes()).unwrap());
        assert_eq!((f.command.as_str(), &f.params), (command, &params), "{fltr:?}");
        let out = item_for_filter(&f).unwrap();
        assert_eq!(out.get("filterID"), Some(&Value::Integer(id)));
        assert_eq!(out.get("hasoptions"), Some(&Value::Boolean(true)));
        // Byte-identical filter descriptor: class id, four-char keys, types, units and enums.
        let Some(Value::Descriptor(w)) = out.get("Fltr") else { panic!("no Fltr for {command}") };
        assert_eq!(w.to_bytes(), fltr.to_bytes(), "{command}");
    }
    // Surface Blur's class is a string id; Smart Blur's and Radial Blur's are four-char codes.
    assert_eq!(surface_blur(1.0, 2).class_id, Id::Str(b"surfaceBlur".to_vec()));
    assert_eq!(radial_blur(1, "Spn ", "Gd  ").class_id, Id::Code(*b"RdlB"));
    // Unset options are written with the dialog defaults.
    let fltr = |c: &str, p: J| item_for_filter(&sf(c, p)).unwrap().get("Fltr").cloned();
    assert_eq!(fltr("filter.blur.surfaceBlur", json!({})), Some(Value::Descriptor(surface_blur(5.0, 15))));
    assert_eq!(fltr("filter.blur.smartBlur", json!({})), Some(Value::Descriptor(smart_blur(3.0, 25.0, "SBQH", "SBMN"))));
    assert_eq!(fltr("filter.blur.radialBlur", json!({"centerX": 0.5, "centerY": 0.5})), Some(Value::Descriptor(radial_blur(10, "Spn ", "Gd  "))));
}

/// `d` with `key` set to `v` (replacing the measured value).
fn replaced(mut d: Descriptor, key: &str, v: Value) -> Descriptor {
    set(&mut d, key, v);
    d
}

/// Malformed or unmodelled blur descriptors stay verbatim (and are written back unchanged);
/// settings PSD can't hold are reported, never a panic.
#[test]
fn blur_family_malformed_is_kept_verbatim() {
    let smart = code(b"SmrB");
    let radial = code(b"RdlB");
    let text = |s: &str| Value::Text(UnicodeString::new_nul(s));
    let bad = [
        // Missing keys.
        (Descriptor::new("surfaceBlur").with("Rds ", unit(b"#Pxl", 5.0)), 1135),
        (Descriptor::new("SmrB").with("Rds ", Value::Double(3.0)).with("Thsh", Value::Double(25.0)), smart),
        (Descriptor::new("RdlB").with("Amnt", Value::Integer(10)).with("BlrM", enumv("BlrM", "Spn ")), radial),
        // Wrong types.
        (Descriptor::new("surfaceBlur").with("Rds ", unit(b"#Pxl", 5.0)).with("Thsh", Value::Double(15.0)), 1135),
        (
            Descriptor::new("SmrB")
                .with("Rds ", text("3"))
                .with("Thsh", Value::Double(25.0))
                .with("SmBQ", enumv("SmBQ", "SBQH"))
                .with("SmBM", enumv("SmBM", "SBMN")),
            smart,
        ),
        (Descriptor::new("RdlB").with("Amnt", text("10")).with("BlrM", enumv("BlrM", "Spn ")).with("BlrQ", enumv("BlrQ", "Gd  ")), radial),
        (Descriptor::new("RdlB").with("Amnt", Value::Integer(10)).with("BlrM", Value::Integer(1)).with("BlrQ", enumv("BlrQ", "Gd  ")), radial),
        // Out of range, NaN and infinity.
        (surface_blur(0.5, 15), 1135),
        (surface_blur(5.0, 300), 1135),
        (surface_blur(f64::NAN, 15), 1135),
        (smart_blur(f64::INFINITY, 25.0, "SBQH", "SBMN"), smart),
        (smart_blur(3.0, 0.0, "SBQH", "SBMN"), smart),
        (radial_blur(0, "Spn ", "Gd  "), radial),
        (radial_blur(101, "Zm  ", "Gd  "), radial),
        // Unknown enum values.
        (smart_blur(3.0, 25.0, "SBQX", "SBMN"), smart),
        (smart_blur(3.0, 25.0, "Hgh ", "SBMN"), smart),
        (radial_blur(10, "Twrl", "Gd  "), radial),
        (radial_blur(10, "Spn ", "Hgh "), radial),
        // A key we don't model (an off-centre Radial Blur would carry one).
        (radial_blur(10, "Zm  ", "Gd  ").with("Cntr", Value::Descriptor(Descriptor::new("Pnt ").with("Hrzn", Value::Double(10.0)))), radial),
        (surface_blur(5.0, 15).with("Edg ", Value::Integer(1)), 1135),
        // Enums of another type, units and value types we would not write back.
        (replaced(smart_blur(3.0, 25.0, "SBQH", "SBMN"), "SmBQ", enumv("Othr", "SBQH")), smart),
        (replaced(smart_blur(3.0, 25.0, "SBQH", "SBMN"), "SmBM", enumv("SmBQ", "SBMN")), smart),
        (replaced(radial_blur(10, "Spn ", "Gd  "), "BlrM", enumv("Othr", "Spn ")), radial),
        (replaced(radial_blur(10, "Spn ", "Gd  "), "BlrQ", enumv("BlrM", "Gd  ")), radial),
        (Descriptor::new("surfaceBlur").with("Rds ", unit(b"#Prc", 5.0)).with("Thsh", Value::Integer(15)), 1135),
        (Descriptor::new("surfaceBlur").with("Rds ", Value::Double(5.0)).with("Thsh", Value::Integer(15)), 1135),
        (replaced(smart_blur(3.0, 25.0, "SBQH", "SBMN"), "Rds ", unit(b"#Pxl", 3.0)), smart),
        (replaced(smart_blur(3.0, 25.0, "SBQH", "SBMN"), "Thsh", unit(b"#Prc", 25.0)), smart),
        (replaced(smart_blur(3.0, 25.0, "SBQH", "SBMN"), "Thsh", Value::Integer(25)), smart),
    ];
    for (fltr, id) in bad {
        let item = ps_item("x", fltr.clone(), id);
        let f = filter_from_item(&item);
        assert_eq!(f.command, UNSUPPORTED_FILTER, "{fltr:?}");
        // Compared as bytes: a NaN setting is not equal to itself.
        assert_eq!(item_for_filter(&f).unwrap().to_bytes(), item.to_bytes(), "{fltr:?}");
    }
    // Radial Blur at its defaults has no `Fltr` (measured): its values are not stored, so the
    // entry stays verbatim instead of guessing them.
    let no_opts = Descriptor::new("filterFX").with("hasoptions", Value::Boolean(false)).with("filterID", Value::Integer(radial));
    assert_eq!(filter_from_item(&no_opts).command, UNSUPPORTED_FILTER);
    // Settings with no PSD equivalent are errors when writing.
    for (command, p) in [
        ("filter.blur.smartBlur", json!({"quality": "ultra"})),
        ("filter.blur.smartBlur", json!({"mode": 3})),
        ("filter.blur.radialBlur", json!({"method": "twirl"})),
        ("filter.blur.radialBlur", json!({"quality": ["best"]})),
        ("filter.blur.radialBlur", json!({"centerX": 0.25})),
        ("filter.blur.radialBlur", json!({"centerY": 1.0})),
        // A centre, or a value, that is there but can't be written as Photoshop would open it.
        ("filter.blur.radialBlur", json!({"centerX": "middle"})),
        ("filter.blur.radialBlur", json!({"centerY": true})),
        ("filter.blur.radialBlur", json!({"amount": 0})),
        ("filter.blur.radialBlur", json!({"amount": 101})),
        ("filter.blur.radialBlur", json!({"amount": "10"})),
        ("filter.blur.surfaceBlur", json!({"radius": 0.5})),
        ("filter.blur.surfaceBlur", json!({"radius": 100.5})),
        ("filter.blur.surfaceBlur", json!({"threshold": 1})),
        ("filter.blur.surfaceBlur", json!({"threshold": 256})),
        ("filter.blur.surfaceBlur", json!({"radius": "5"})),
        ("filter.blur.smartBlur", json!({"radius": 0.05})),
        ("filter.blur.smartBlur", json!({"threshold": 100.5})),
        ("filter.blur.smartBlur", json!({"threshold": [25]})),
    ] {
        assert!(item_for_filter(&sf(command, p.clone())).is_err(), "{command} {p}");
    }
    // Every value that is written opens again as the same editable filter (range ends included).
    for (command, p) in [
        ("filter.blur.surfaceBlur", json!({"radius": 1.0, "threshold": 2})),
        ("filter.blur.surfaceBlur", json!({"radius": 100.0, "threshold": 255})),
        ("filter.blur.smartBlur", json!({"radius": 0.1, "threshold": 100.0, "quality": "low", "mode": "edgeOnly"})),
        ("filter.blur.radialBlur", json!({"amount": 1, "method": "zoom", "quality": "best", "centerX": 0.5, "centerY": null})),
        ("filter.blur.radialBlur", json!({"amount": 100, "method": "spin", "quality": "draft"})),
    ] {
        let back = filter_from_item(&item_for_filter(&sf(command, p.clone())).unwrap());
        assert_eq!(back.command, command, "{p}");
    }
    // Hostile numbers write without panicking.
    for p in [json!({"radius": f64::MAX, "threshold": -1e300, "amount": 1e300}), json!({"radius": "x", "threshold": null}), json!(null), json!([1, 2])] {
        for command in ["filter.blur.surfaceBlur", "filter.blur.smartBlur", "filter.blur.radialBlur"] {
            let _ = item_for_filter(&sf(command, p.clone()));
        }
    }
}

/// A `filterFX` entry shaped as Photoshop saves it (measured in Photoshop: the filter applied
/// through Action Manager to a smart object, the PSD saved, `filterFX` read back).
fn photoshop_item(filter_id: &[u8; 4], fltr: Descriptor) -> Descriptor {
    Descriptor::new("filterFX")
        .with("Nm  ", Value::Text(UnicodeString::new_nul("Filter...")))
        .with("blendOptions", blend_options(&sf("", J::Null)))
        .with("enab", Value::Boolean(true))
        .with("hasoptions", Value::Boolean(true))
        .with("FrgC", rgbc(0.0))
        .with("BckC", rgbc(255.0))
        .with("Fltr", Value::Descriptor(fltr))
        .with("filterID", Value::Integer(code(filter_id)))
}

#[test]
fn stylize_and_pixelate_filters_open_from_photoshop_shapes() {
    let cases = [
        (b"Dfs ", Descriptor::new("Dfs ").with("Md  ", enumv("DfsM", "Nrml")), "filter.stylize.diffuse", json!({"mode": "normal"})),
        (b"Dfs ", Descriptor::new("Dfs ").with("Md  ", enumv("DfsM", "DrkO")), "filter.stylize.diffuse", json!({"mode": "darkenOnly"})),
        (b"Dfs ", Descriptor::new("Dfs ").with("Md  ", enumv("DfsM", "LghO")), "filter.stylize.diffuse", json!({"mode": "lightenOnly"})),
        (b"Dfs ", Descriptor::new("Dfs ").with("Md  ", enumv("DfsM", "anisotropic")), "filter.stylize.diffuse", json!({"mode": "anisotropic"})),
        (
            b"TrcC",
            Descriptor::new("TrcC").with("Lvl ", Value::Integer(128)).with("Edg ", enumv("CntE", "Upr ")),
            "filter.stylize.traceContour",
            json!({"level": 128, "edge": "upper"}),
        ),
        (
            b"TrcC",
            Descriptor::new("TrcC").with("Lvl ", Value::Integer(60)).with("Edg ", enumv("CntE", "Lwr ")),
            "filter.stylize.traceContour",
            json!({"level": 60, "edge": "lower"}),
        ),
        (b"Crst", Descriptor::new("Crst").with("ClSz", Value::Integer(10)), "filter.pixelate.crystallize", json!({"cellSize": 10})),
        (
            b"Pntl",
            Descriptor::new("Pntl").with("ClSz", Value::Integer(7)),
            "filter.pixelate.pointillize",
            json!({"cellSize": 7, "background": [1.0, 1.0, 1.0, 1.0]}),
        ),
    ];
    for (id, fltr, command, params) in cases {
        let item = photoshop_item(id, fltr);
        // Through bytes, as read from a file.
        let f = filter_from_item(&Descriptor::from_bytes(&item.to_bytes()).unwrap());
        assert_eq!((f.command.as_str(), &f.params), (command, &params));
        // Written back in the measured shape: same class, keys, value types and filterID.
        let back = item_for_filter(&f).unwrap();
        for key in ["Fltr", "filterID", "BckC"] {
            assert_eq!(back.get(key), item.get(key), "{command} {key}");
        }
    }
    // The `anisotropic` mode is a long id, as Photoshop writes it.
    let d = item_for_filter(&sf("filter.stylize.diffuse", json!({"mode": "anisotropic"}))).unwrap();
    let Some(Value::Descriptor(fltr)) = d.get("Fltr") else { panic!("no Fltr") };
    assert!(matches!(fltr.get("Md  "), Some(Value::Enumerated { value: Id::Str(v), .. }) if v == b"anisotropic"));
    // Pointillize takes the entry's background colour; the others ignore it.
    let mut item = photoshop_item(b"Pntl", Descriptor::new("Pntl").with("ClSz", Value::Integer(7)));
    set(&mut item, "BckC", rgbc(51.0));
    let f = filter_from_item(&item);
    assert_eq!(f.params["background"], json!([0.2, 0.2, 0.2, 1.0]));
    assert_eq!(item_for_filter(&f).unwrap().get("BckC"), Some(&rgbc(51.0)));
    let mut item = photoshop_item(b"Crst", Descriptor::new("Crst").with("ClSz", Value::Integer(7)));
    set(&mut item, "BckC", rgbc(51.0));
    assert_eq!(filter_from_item(&item).params, json!({"cellSize": 7}));
    // Our random seed has no PSD key: it is not written.
    let fltr = |c: &str, p: J| item_for_filter(&sf(c, p)).unwrap().get("Fltr").cloned();
    assert_eq!(
        fltr("filter.stylize.diffuse", json!({"mode": "normal", "seed": 7})),
        Some(Value::Descriptor(Descriptor::new("Dfs ").with("Md  ", enumv("DfsM", "Nrml"))))
    );
    // Defaults and out-of-range values are written in Photoshop's ranges.
    assert_eq!(
        fltr("filter.stylize.traceContour", json!({})),
        Some(Value::Descriptor(Descriptor::new("TrcC").with("Lvl ", Value::Integer(128)).with("Edg ", enumv("CntE", "Lwr "))))
    );
    assert_eq!(fltr("filter.pixelate.crystallize", json!({})), Some(Value::Descriptor(Descriptor::new("Crst").with("ClSz", Value::Integer(10)))));
    assert_eq!(fltr("filter.pixelate.pointillize", json!({})), Some(Value::Descriptor(Descriptor::new("Pntl").with("ClSz", Value::Integer(5)))));
    assert_eq!(
        fltr("filter.pixelate.crystallize", json!({"cellSize": 1e9})),
        Some(Value::Descriptor(Descriptor::new("Crst").with("ClSz", Value::Integer(300))))
    );
    assert_eq!(
        fltr("filter.pixelate.pointillize", json!({"cellSize": "big"})),
        Some(Value::Descriptor(Descriptor::new("Pntl").with("ClSz", Value::Integer(5))))
    );
    assert_eq!(
        fltr("filter.stylize.traceContour", json!({"level": -40, "edge": "upper"})),
        Some(Value::Descriptor(Descriptor::new("TrcC").with("Lvl ", Value::Integer(0)).with("Edg ", enumv("CntE", "Upr "))))
    );
}

#[test]
fn malformed_stylize_and_pixelate_filters_are_kept_verbatim() {
    let long = Value::Integer;
    let cases = [
        // Missing keys.
        (b"Dfs ", Descriptor::new("Dfs ")),
        (b"TrcC", Descriptor::new("TrcC").with("Lvl ", long(60))),
        (b"TrcC", Descriptor::new("TrcC").with("Edg ", enumv("CntE", "Upr "))),
        (b"Crst", Descriptor::new("Crst")),
        (b"Pntl", Descriptor::new("Pntl")),
        // Unknown enum values or enum types.
        (b"Dfs ", Descriptor::new("Dfs ").with("Md  ", enumv("DfsM", "Wild"))),
        (b"Dfs ", Descriptor::new("Dfs ").with("Md  ", enumv("BlnM", "Nrml"))),
        (b"TrcC", Descriptor::new("TrcC").with("Lvl ", long(60)).with("Edg ", enumv("CntE", "Mddl"))),
        // Wrong types (also NaN).
        (b"Dfs ", Descriptor::new("Dfs ").with("Md  ", long(1))),
        (b"TrcC", Descriptor::new("TrcC").with("Lvl ", Value::Double(60.0)).with("Edg ", enumv("CntE", "Upr "))),
        (b"Crst", Descriptor::new("Crst").with("ClSz", unit(b"#Pxl", 10.0))),
        (b"Pntl", Descriptor::new("Pntl").with("ClSz", Value::Double(f64::NAN))),
        // Out of range.
        (b"TrcC", Descriptor::new("TrcC").with("Lvl ", long(256)).with("Edg ", enumv("CntE", "Upr "))),
        (b"TrcC", Descriptor::new("TrcC").with("Lvl ", long(-1)).with("Edg ", enumv("CntE", "Upr "))),
        (b"Crst", Descriptor::new("Crst").with("ClSz", long(2))),
        (b"Pntl", Descriptor::new("Pntl").with("ClSz", long(i32::MAX))),
    ];
    for (id, fltr) in cases {
        let item = photoshop_item(id, fltr);
        let f = filter_from_item(&Descriptor::from_bytes(&item.to_bytes()).unwrap());
        assert_eq!(f.command, UNSUPPORTED_FILTER, "{item:?}");
        assert_eq!(item_for_filter(&f).unwrap().to_bytes(), item.to_bytes());
    }
    // Saved without options (`hasoptions` false, no `Fltr`): kept verbatim.
    let bare = Descriptor::new("filterFX").with("hasoptions", Value::Boolean(false)).with("filterID", Value::Integer(code(b"Crst")));
    assert_eq!(filter_from_item(&bare).command, UNSUPPORTED_FILTER);
    // A Pointillize entry whose background colour is unreadable is kept verbatim too.
    for bad in [Value::Integer(3), Value::Descriptor(Descriptor::new("RGBC").with("Rd  ", Value::Double(0.0)))] {
        let mut item = photoshop_item(b"Pntl", Descriptor::new("Pntl").with("ClSz", Value::Integer(7)));
        set(&mut item, "BckC", bad);
        assert_eq!(filter_from_item(&item).command, UNSUPPORTED_FILTER);
    }
    // Values with no PSD equivalent are reported, not guessed.
    for (c, p) in [
        ("filter.stylize.diffuse", json!({"mode": "sideways"})),
        ("filter.stylize.diffuse", json!({"mode": 3})),
        ("filter.stylize.traceContour", json!({"edge": "middle"})),
        ("filter.pixelate.pointillize", json!({"background": "teal"})),
        ("filter.pixelate.pointillize", json!({"background": [1.0]})),
    ] {
        assert!(item_for_filter(&sf(c, p.clone())).is_err(), "{c} {p}");
    }
}


#[test]
fn average_stays_verbatim() {
    let item = photoshop_no_options_item("Average", Value::Integer(code(b"Avrg")));
    let filter = filter_from_item(&item);
    assert_eq!(filter.command, UNSUPPORTED_FILTER);
    assert_eq!(item_for_filter(&filter).unwrap(), item);
}
