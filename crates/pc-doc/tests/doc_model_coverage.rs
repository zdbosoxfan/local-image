use photocraft_doc::comps::{capture_states, layer_position, next_comp_id};
use photocraft_doc::text::{CharStyle, TextAlign};
use photocraft_doc::text_styles::{apply_attrs, diff_attrs, validate};
use photocraft_doc::{
    AlphaChannel, Artboard, ArtboardBackground, BlendIf, BlendRange, Color, ColorIndicates, ColorMode, Document, Effect, Fill, FxCommon, GlobalLight,
    GradientStyle, Guides, Layer, LayerContent, LayerId, LayerMask, Locks, PixelFormat, Rect, SampleType, Size, Surface, TextStyles, Variables,
};
use serde_json::json;

fn test_doc() -> Document {
    Document::with_background("test", Size::new(64, 64), ColorMode::Rgb, SampleType::U8, Color::WHITE)
}

fn styles() -> TextStyles {
    let mut s = TextStyles::default();
    s.character.push(photocraft_doc::text_styles::CharacterStyleDef {
        id: 1,
        name: "Big".into(),
        attrs: json!({"size_pt": 40.0, "underline": true}).as_object().unwrap().clone(),
    });
    s.paragraph.push(photocraft_doc::text_styles::ParagraphStyleDef {
        id: 1,
        name: "Head".into(),
        para_attrs: json!({"align": "Center"}).as_object().unwrap().clone(),
        char_attrs: json!({"size_pt": 20.0, "weight": 700}).as_object().unwrap().clone(),
    });
    s
}

#[test]
fn blend_range_full_and_weight() {
    let r = BlendRange::default();
    assert_eq!(r, BlendRange::FULL);
    assert!(r.is_full());
    for v in [0.0, 0.1, 127.0, 254.9, 255.0] {
        assert!((r.weight(v) - 1.0).abs() < 1e-6);
    }
}

#[test]
fn blend_range_unsplit_and_bytes() {
    let r = BlendRange { black: [50, 50], white: [200, 200] };
    assert_eq!(r.weight(49.0), 0.0);
    assert_eq!(r.weight(49.9), 0.0);
    assert_eq!(r.weight(50.0), 1.0);
    assert_eq!(r.weight(200.0), 1.0);
    assert_eq!(r.weight(200.1), 0.0);
    assert!(!r.is_full());

    let bytes = BlendRange::from_bytes([10, 20, 230, 240]).to_bytes();
    assert_eq!(bytes, [10, 20, 230, 240]);
    assert_eq!(BlendRange::from_bytes(bytes), BlendRange { black: [10, 20], white: [230, 240] });
}

#[test]
fn blend_if_set_trims_trailing() {
    let mut b = BlendIf::default();
    let r = BlendRange { black: [30, 30], white: [255, 255] };
    b.set(2, [BlendRange::FULL, r]);
    assert_eq!(b.ranges.len(), 3);
    assert_eq!(b.get(2), [BlendRange::FULL, r]);
    assert_eq!(b.get(7), [BlendRange::FULL; 2]);
    assert!(!b.is_default());
    b.set(2, [BlendRange::FULL; 2]);
    assert_eq!(b, BlendIf::default());
}

#[test]
fn blend_if_serde_round_trip() {
    let mut b = BlendIf::default();
    let r = BlendRange { black: [5, 10], white: [245, 250] };
    b.set(0, [r, BlendRange::FULL]);
    let json = serde_json::to_string(&b).unwrap();
    let back: BlendIf = serde_json::from_str(&json).unwrap();
    assert_eq!(b, back);
}

#[test]
fn new_document_and_background() {
    let d = Document::new("doc", Size::new(10, 20), ColorMode::Rgb, SampleType::U8);
    assert_eq!(d.name, "doc");
    assert_eq!(d.size, Size::new(10, 20));
    assert_eq!(d.resolution_dpi, 72.0);
    assert_eq!(d.mode, ColorMode::Rgb);
    assert_eq!(d.depth, SampleType::U8);
    assert!(d.layers.is_empty());
    assert_eq!(d.guides, Guides::default());
    assert_eq!(d.global_light, GlobalLight::default());

    let bg = test_doc();
    assert_eq!(bg.layers.len(), 1);
    let layer = &bg.layers[0];
    assert_eq!(layer.name, "Background");
    assert!(layer.locks.transparency);
    assert!(layer.locks.position);
    let surface = layer.surface().unwrap();
    assert_eq!(surface.pixel(0, 0), vec![1.0; 4]);
    assert_eq!(surface.pixel(63, 63), vec![1.0; 4]);
    assert_eq!(surface.pixel(64, 0), vec![0.0; 4]);
}

#[test]
fn group_walk_and_paths() {
    let mut d = test_doc();
    let inner = Layer::raster("inner", PixelFormat::RGBA8);
    let inner_id = inner.id;
    let g = Layer::group("G", vec![inner]);
    let gid = g.id;
    d.insert_above(None, g);
    let walk = d.walk();
    assert_eq!(walk.len(), 3);
    assert_eq!(walk[0].0, vec![0]);
    assert_eq!(walk[0].2.name, "Background");
    assert_eq!(walk[1].0, vec![1]);
    assert_eq!(walk[1].2.name, "G");
    assert_eq!(walk[2].0, vec![1, 0]);
    assert_eq!(walk[2].2.name, "inner");
    assert_eq!(d.path_of(inner_id), Some(vec![1, 0]));
    assert_eq!(d.layer(gid).unwrap().blend, photocraft_doc::BlendMode::PassThrough);
}

#[test]
fn insert_remove_shift_siblings() {
    let mut d = test_doc();
    let bg = d.layers[0].id;
    let a = d.insert_above(Some(bg), Layer::raster("A", PixelFormat::RGBA8));
    let b = d.insert_above(Some(bg), Layer::raster("B", PixelFormat::RGBA8));
    assert_eq!(d.layers.iter().map(|l| l.name.as_str()).collect::<Vec<_>>(), ["Background", "B", "A"]);
    assert!(d.shift(b, 1));
    assert_eq!(d.layers[2].id, b);
    assert!(!d.shift(b, 1));
    assert_eq!(d.remove(a).unwrap().name, "A");
    assert!(d.layer(a).is_none());
    assert_eq!(d.layer_count(), 2);
    assert_eq!(d.layers[1].id, b);
}

#[test]
fn duplicate_fresh_ids_recursively() {
    let g = Layer::group("G", vec![Layer::raster("x", PixelFormat::RGBA8)]);
    let dup = g.duplicate();
    assert_ne!(g.id, dup.id);
    assert_ne!(g.children().unwrap()[0].id, dup.children().unwrap()[0].id);
    assert_eq!(dup.children().unwrap()[0].name, "x");
    assert_eq!(dup.blend, photocraft_doc::BlendMode::PassThrough);
}

#[test]
fn effective_locks_unions_group_and_child() {
    let mut d = test_doc();
    let inner = Layer::raster("inner", PixelFormat::RGBA8);
    let inner_id = inner.id;
    let mut g = Layer::group("G", vec![Layer::group("H", vec![inner])]);
    g.locks.position = true;
    let gid = g.id;
    d.insert_above(None, g);
    d.layer_mut(inner_id).unwrap().locks.pixels = true;
    let locks = d.effective_locks(inner_id);
    assert!(locks.position && locks.pixels && !locks.all);
    assert!(!d.effective_locks(gid).pixels);
    assert_eq!(d.effective_locks(LayerId(u64::MAX)), Locks::default());
}

#[test]
fn next_layer_name_generates_unique() {
    let mut d = test_doc();
    assert_eq!(d.next_layer_name("Layer"), "Layer 1");
    d.insert_above(None, Layer::raster("Layer 1", PixelFormat::RGBA8));
    assert_eq!(d.next_layer_name("Layer"), "Layer 2");
}

#[test]
fn top_layer_and_max_depth() {
    let mut d = test_doc();
    assert_eq!(d.top_layer(), Some(d.layers[0].id));
    let l = Layer::raster("Top", PixelFormat::RGBA8);
    let lid = l.id;
    d.insert_above(None, l);
    assert_eq!(d.top_layer(), Some(lid));

    let bg = d.layers[0].id;
    let mut chain = Layer::raster("L", PixelFormat::RGBA8);
    for _ in 0..5 {
        chain = Layer::group("G", vec![chain]);
    }
    d.insert_above(Some(bg), chain);
    assert_eq!(d.max_group_depth(), 5);
    d.insert_above(Some(bg), Layer::group("empty", vec![]));
    assert_eq!(d.max_group_depth(), 5);
}

#[test]
fn fill_gradient_serde_defaults() {
    let fill = Fill::Gradient {
        stops: vec![(0.0, Color::BLACK), (1.0, Color::WHITE)],
        angle: 45.0,
        scale: 2.0,
        style: GradientStyle::Linear,
        reverse: false,
        opacity_stops: vec![],
        midpoints: vec![],
        offset: (0.5, 0.5),
        dither: true,
        align: false,
    };
    let mut value = serde_json::to_value(fill).unwrap();
    let grad = value.as_object_mut().unwrap().get_mut("Gradient").unwrap().as_object_mut().unwrap();
    grad.remove("opacity_stops");
    grad.remove("midpoints");
    grad.remove("offset");
    grad.remove("dither");
    grad.remove("align");
    let loaded: Fill = serde_json::from_value(value).unwrap();
    match loaded {
        Fill::Gradient { opacity_stops, midpoints, offset, dither, align, .. } => {
            assert!(opacity_stops.is_empty());
            assert!(midpoints.is_empty());
            assert_eq!(offset, (0.0, 0.0));
            assert!(!dither);
            assert!(align);
        }
        _ => panic!("expected gradient"),
    }
}

#[test]
fn fill_pattern_serde_defaults() {
    let fill = Fill::Pattern { name: "p".into(), scale: 1.5, id: "id".into(), angle: 30.0, link: false, phase: (10.0, 20.0) };
    let mut value = serde_json::to_value(fill).unwrap();
    let pat = value.as_object_mut().unwrap().get_mut("Pattern").unwrap().as_object_mut().unwrap();
    pat.remove("id");
    pat.remove("angle");
    pat.remove("link");
    pat.remove("phase");
    let loaded: Fill = serde_json::from_value(value).unwrap();
    match loaded {
        Fill::Pattern { id, angle, link, phase, .. } => {
            assert!(id.is_empty());
            assert_eq!(angle, 0.0);
            assert!(link);
            assert_eq!(phase, (0.0, 0.0));
        }
        _ => panic!("expected pattern"),
    }
}

#[test]
fn variables_partial_data_defaults() {
    let vars: Variables = serde_json::from_value(json!({})).unwrap();
    assert!(vars.defs.is_empty());
    assert!(vars.data_sets.is_empty());
    assert_eq!(vars.active, None);
    assert!(vars.is_empty());
}

#[test]
fn text_styles_empty_json_defaults() {
    let styles: TextStyles = serde_json::from_value(json!({})).unwrap();
    assert_eq!(styles.basic.name, "Basic Paragraph");
    assert!(styles.character.is_empty());
    assert!(styles.paragraph.is_empty());
    assert!(styles.is_empty());
}

#[test]
fn develop_link_serde_round_trip() {
    let link = photocraft_doc::DevelopLink { settings: json!({"exposure": 1.0}), photo: Some(42) };
    let json = serde_json::to_string(&link).unwrap();
    let back: photocraft_doc::DevelopLink = serde_json::from_str(&json).unwrap();
    assert_eq!(link, back);
    let link2: photocraft_doc::DevelopLink = serde_json::from_str(r#"{"settings":{"exposure":0.5}}"#).unwrap();
    assert_eq!(link2.photo, None);
}

#[test]
fn global_light_and_locks_defaults() {
    let g = GlobalLight::default();
    assert_eq!(g.angle, 120.0);
    assert_eq!(g.altitude, 30.0);
    let a = Locks { transparency: true, pixels: false, position: true, artboard: false, all: false };
    let b = Locks { transparency: false, pixels: true, position: false, artboard: true, all: true };
    let u = a.union(b);
    assert!(u.transparency && u.pixels && u.position && u.artboard && u.all);
    assert_eq!(Locks::default(), Locks { transparency: false, pixels: false, position: false, artboard: false, all: false });
}

#[test]
fn artboard_background_rgba_and_psd_type() {
    assert_eq!(ArtboardBackground::White.rgba(), Some([1.0, 1.0, 1.0, 1.0]));
    assert_eq!(ArtboardBackground::Black.rgba(), Some([0.0, 0.0, 0.0, 1.0]));
    assert_eq!(ArtboardBackground::Transparent.rgba(), None);
    assert_eq!(ArtboardBackground::White.psd_type(), 1);
    assert_eq!(ArtboardBackground::Black.psd_type(), 2);
    assert_eq!(ArtboardBackground::Transparent.psd_type(), 3);
    assert_eq!(ArtboardBackground::White.name(), "white");
}

#[test]
fn text_styles_resolution_layers() {
    let s = styles();
    let none = s.resolve_char(None, None, "Inter");
    assert_eq!(none.font_family, "Inter");
    assert_eq!(none.size_pt, 12.0);
    assert_eq!(none.style_sheet, None);
    let head = s.resolve_char(Some(1), None, "Inter");
    assert_eq!(head.size_pt, 20.0);
    assert_eq!(head.weight, 700);
    let big = s.resolve_char(Some(1), Some(1), "Inter");
    assert_eq!(big.size_pt, 40.0);
    assert_eq!(big.weight, 700);
    assert!(big.underline);
    assert_eq!(big.style_sheet, Some(1));
    let p = s.resolve_para(Some(1));
    assert_eq!(p.align, TextAlign::Center);
    assert_eq!(p.style_sheet, Some(1));
    assert_eq!(s.resolve_para(None).align, TextAlign::Left);
}

#[test]
fn text_styles_overrides_and_restyle() {
    let old = styles();
    let mut run = old.resolve_char(None, Some(1), "Inter");
    run.italic = true;
    let ov = old.char_overrides(&run, None, "Inter");
    assert_eq!(ov, json!({"italic": true}).as_object().unwrap().clone());
    let mut new = old.clone();
    new.char_style_mut(1).unwrap().attrs.insert("size_pt".into(), json!(50.0));
    let restyled = new.restyle_char(&old, &run, None, None, Some(1), "Inter");
    assert_eq!(restyled.size_pt, 50.0);
    assert!(restyled.italic);
    assert!(restyled.underline);
}

#[test]
fn text_styles_validate_and_diff() {
    let attrs = json!({"size_pt": 3.0}).as_object().unwrap().clone();
    assert!(validate::<CharStyle>(&attrs).is_ok());
    let attrs_bad = json!({"size_pt": "big"}).as_object().unwrap().clone();
    assert!(validate::<CharStyle>(&attrs_bad).is_err());
    let attrs_unknown = json!({"nope": 1}).as_object().unwrap().clone();
    assert!(validate::<CharStyle>(&attrs_unknown).is_err());
    let a = CharStyle { size_pt: 30.0, style_sheet: Some(3), ..Default::default() };
    let diff = diff_attrs(&a, &CharStyle::default());
    assert_eq!(diff, json!({"size_pt": 30.0}).as_object().unwrap().clone());
}

#[test]
fn text_styles_names_and_ids() {
    let s = styles();
    assert_eq!(s.unique_name(false, "Character Style"), "Character Style 1");
    assert_eq!(s.next_char_id(), Some(2));
    assert_eq!(s.next_para_id(), Some(2));
    assert_eq!(s.para_style(0).unwrap().name, "Basic Paragraph");
}

#[test]
fn text_styles_exhausted_ids_round_trip() {
    let mut s = TextStyles::default();
    s.character.push(photocraft_doc::text_styles::CharacterStyleDef { id: u32::MAX, ..Default::default() });
    s.paragraph.push(photocraft_doc::text_styles::ParagraphStyleDef { id: u32::MAX, ..Default::default() });
    let json = serde_json::to_string(&s).unwrap();
    let loaded: TextStyles = serde_json::from_str(&json).unwrap();
    assert_eq!(loaded.next_char_id(), None);
    assert_eq!(loaded.next_para_id(), None);
}

#[test]
fn capture_states_records_every_layer() {
    let mut d = test_doc();
    let mut l = Layer::raster("A", PixelFormat::RGBA8);
    l.surface_mut().unwrap().fill_rect(Rect::new(10, 12, 20, 22), &[1.0, 0.0, 0.0, 1.0]);
    d.layers.push(l);
    let states = capture_states(&d);
    assert_eq!(states.len(), 2);
    assert_eq!(states[1].position, Some((10, 12)));
    assert_eq!(states[1].visible, Some(true));
    assert!(states[1].appearance.is_some());
    assert_eq!(states[1].appearance.as_ref().unwrap().blend, photocraft_doc::BlendMode::Normal);
}

#[test]
fn layer_comp_captured_info_bits() {
    let mut c = photocraft_doc::LayerComp {
        id: 1,
        name: "c".into(),
        comment: String::new(),
        apply_visibility: false,
        apply_position: false,
        apply_appearance: false,
        states: vec![],
    };
    c.set_captured_info(5);
    assert!(c.apply_visibility && !c.apply_position && c.apply_appearance);
    assert_eq!(c.captured_info(), 5);
}

#[test]
fn layer_comp_missing_layers() {
    let mut d = test_doc();
    let states = capture_states(&d);
    let comp = photocraft_doc::LayerComp {
        id: next_comp_id(&d),
        name: "c".into(),
        comment: String::new(),
        apply_visibility: true,
        apply_position: true,
        apply_appearance: true,
        states,
    };
    d.layer_comps.push(comp);
    let gone = d.layers[0].id;
    d.layers.pop();
    assert_eq!(d.layer_comps[0].missing_layers(&d), vec![gone]);
}

#[test]
fn artboards_listed_and_found() {
    let mut d = test_doc();
    let child = Layer::raster("in", PixelFormat::RGBA8);
    let cid = child.id;
    let mut g = Layer::group("Artboard 1", vec![child]);
    if let LayerContent::Group(gr) = &mut g.content {
        gr.artboard = Some(Artboard::new(Rect::new(0, 0, 32, 32)));
    }
    let gid = g.id;
    d.layers.push(g);
    assert!(d.has_artboards());
    assert_eq!(d.artboards().len(), 1);
    assert_eq!(d.artboards()[0].0, gid);
    assert_eq!(d.artboard_of(cid), Some(gid));
    assert_eq!(layer_position(d.layer(gid).unwrap()), Some((0, 0)));
}

#[test]
fn effect_default_drop_shadow_and_labels() {
    let e = Effect::default_drop_shadow();
    match e {
        Effect::DropShadow(s) => {
            assert!(s.common.enabled);
            assert_eq!(s.common.blend, photocraft_doc::BlendMode::Multiply);
            assert!((s.common.opacity - 0.75).abs() < 1e-6);
            assert_eq!(s.color, Color::BLACK);
            assert_eq!(s.angle, 120.0);
            assert!(s.use_global_light);
            assert_eq!(s.distance, 5.0);
            assert_eq!(s.size, 5.0);
            assert!(s.knocks_out);
        }
        _ => panic!("expected drop shadow"),
    }
    let overlay = Effect::ColorOverlay { common: FxCommon::new(photocraft_doc::BlendMode::Normal, 0.5), color: Color::rgb(1.0, 0.0, 0.0) };
    assert!(overlay.enabled());
    assert_eq!(overlay.label(), "Color Overlay");
    let disabled =
        Effect::ColorOverlay { common: FxCommon { enabled: false, blend: photocraft_doc::BlendMode::Normal, opacity: 1.0 }, color: Color::rgb(1.0, 0.0, 0.0) };
    assert!(!disabled.enabled());
}

#[test]
fn layer_mask_and_alpha_channel() {
    let reveal = LayerMask::reveal_all();
    assert_eq!(reveal.value(5, 5), 1.0);
    let mut hide = LayerMask::hide_all();
    assert_eq!(hide.value(5, 5), 0.0);
    hide.density = 0.5;
    assert!((hide.value(5, 5) - 0.5).abs() < 1e-6);
    hide.enabled = false;
    assert_eq!(hide.value(5, 5), 1.0);

    let surface_alpha = Surface::with_default(PixelFormat::GRAY8, &[0.0]);
    let alpha = AlphaChannel::new("alpha", surface_alpha);
    assert_eq!(alpha.indicates, ColorIndicates::MaskedAreas);
    assert_eq!(alpha.overlay_coverage(0.0), 1.0);
    assert_eq!(alpha.overlay_coverage(1.0), 0.0);

    let surface_spot = Surface::with_default(PixelFormat::GRAY8, &[0.0]);
    let mut spot = AlphaChannel::new("spot", surface_spot);
    spot.spot = Some((Color::rgb(1.0, 0.0, 0.0), 1.0));
    assert_eq!(spot.overlay_coverage(0.0), 0.0);
    assert_eq!(spot.overlay_coverage(1.0), 1.0);

    let surface_sel = Surface::with_default(PixelFormat::GRAY8, &[0.0]);
    let mut sel = AlphaChannel::new("sel", surface_sel);
    sel.indicates = ColorIndicates::SelectedAreas;
    assert_eq!(sel.overlay_coverage(0.0), 0.0);
    assert_eq!(sel.overlay_coverage(1.0), 1.0);
}

#[test]
fn malformed_inputs_return_err() {
    assert!(serde_json::from_str::<TextStyles>("not json").is_err());
    let base = CharStyle::default();
    let attrs = json!({"size_pt": "big"}).as_object().unwrap().clone();
    assert!(apply_attrs(&base, &attrs).is_err());
    let attrs_unknown = json!({"nope": 1}).as_object().unwrap().clone();
    assert!(validate::<CharStyle>(&attrs_unknown).is_err());
}
