//! local-image: **Filter › Camera Raw Filter…** (`filter.develop`): the Library's develop engine
//! on a layer (see `photocraft_io::develop_filter`), Photoshop's Camera Raw Filter with the
//! Develop module's tools.
//!
//! * `filter.develop {…DevelopSettings, layer?, convert?, index?}` — on a pixel layer it develops
//!   the pixels (inside the selection); on a smart object it adds a re-editable smart filter (the
//!   selection becomes the filter mask); `convert: true` first turns a pixel layer into a smart
//!   object (one history step for both); `index` replaces the settings of that existing
//!   `filter.develop` smart filter (double-click → edit → OK).
//! * `develop.composite {…DevelopSettings, mode: "live"|"stamp"}` — Compositing → Develop on a
//!   layered document: the visible layers grouped into a smart object (`live`, the layers stay
//!   editable inside) or stamped into a new layer turned smart object (`stamp`), with the filter on
//!   it. One history step.
//!
//! The settings are stored as the smart filter's params (the `DevelopSettings` JSON, after
//! [`photocraft_io::develop_filter::sanitize`]); private `__` keys (a Photoshop descriptor
//! template) are kept across edits.

use photocraft_doc::{Document, Layer, LayerContent, LayerId, SmartFilter};
use photocraft_geom::Rect;
use photocraft_io::develop_filter as dev;
use photocraft_raster::Surface;
use serde_json::{Value, json};

use crate::commands::CommandSpec;
use crate::{EngineError, Result, Session};

pub const DEVELOP: &str = dev::COMMAND;
pub const COMPOSITE: &str = "develop.composite";

fn bad(cmd: &str, msg: impl Into<String>) -> EngineError {
    EngineError::BadParams { cmd: cmd.into(), msg: msg.into() }
}

/// The settings to store: the sanitized settings plus the private `__` keys of `p` and `keep`.
fn stored(settings: &dev::DevelopSettings, p: &Value, keep: Option<&Value>) -> Value {
    let mut v = serde_json::to_value(settings).unwrap_or_else(|_| json!({}));
    if let Value::Object(m) = &mut v {
        for src in [keep, Some(p)].into_iter().flatten() {
            if let Value::Object(o) = src {
                for (k, x) in o {
                    if k.starts_with("__") {
                        m.insert(k.clone(), x.clone());
                    }
                }
            }
        }
    }
    v
}

/// The area a layer develops over: the canvas and any pixels past it (vignette and other
/// frame-relative tools see the same frame in the Develop session and the result).
pub fn develop_area(surf: &Surface, canvas: Rect) -> Rect {
    canvas.union(&surf.content_bounds())
}

/// Smart-filter hook (`filters::apply_filter_to_surface_in`): `None` for other ids or when the
/// stored settings can't be read.
pub fn apply_to_surface(id: &str, params: &Value, surf: &Surface, canvas: Rect, profile: Option<&photocraft_cms::Profile>) -> Option<Surface> {
    if id != DEVELOP {
        return None;
    }
    let settings = dev::filter_settings(params).ok()?;
    let profile = profile.unwrap_or_else(|| photocraft_cms::Builtin::Srgb.profile());
    dev::develop_surface(surf, develop_area(surf, canvas), &settings, profile).ok()
}

fn smart_filter(params: Value) -> SmartFilter {
    SmartFilter { command: DEVELOP.into(), params, blend: photocraft_color::BlendMode::Normal, opacity: 1.0, visible: true }
}

fn enabled(s: &Session) -> std::result::Result<(), String> {
    crate::lens_cmds::raw_enabled(s)
}

fn composite_enabled(s: &Session) -> std::result::Result<(), String> {
    let d = s.active().ok_or("no document open")?;
    match d.doc.mode {
        photocraft_color::ColorMode::Rgb | photocraft_color::ColorMode::Grayscale => {}
        m => return Err(format!("Camera Raw Filter needs an RGB or Grayscale document (this one is {m:?})")),
    }
    if d.doc.layers.iter().any(|l| l.visible) { Ok(()) } else { Err("the document has no visible layers".into()) }
}

fn develop_cmd(s: &mut Session, p: &Value) -> Result<Value> {
    let settings = dev::filter_settings(p).map_err(|e| bad(DEVELOP, e))?;
    let id = crate::lens_cmds::target(s, p)?;
    let st = s.active().ok_or(EngineError::NoDocument)?;
    let profile = crate::color_cmds::composite_profile(&st.doc);
    let t0 = crate::photo_cmds::Stopwatch::start();
    // Re-edit an existing smart filter.
    if let Some(index) = p.get("index").and_then(Value::as_u64).map(|i| i as usize) {
        s.edit("Camera Raw Filter", |doc, _| {
            let l = doc.layer_mut(id).ok_or(EngineError::NoLayer(id))?;
            let LayerContent::Smart(sm) = &mut l.content else { return Err(bad(DEVELOP, "`index` needs a smart object")) };
            let f = sm.smart_filters.get_mut(index).ok_or_else(|| bad(DEVELOP, format!("no smart filter {index}")))?;
            if f.command != DEVELOP {
                return Err(bad(DEVELOP, format!("smart filter {index} is not a Camera Raw Filter")));
            }
            f.params = stored(&settings, p, Some(&f.params));
            if crate::smart_cmds::refresh(doc, id)? { Ok(()) } else { Err(EngineError::Other("the smart object's contents are unavailable".into())) }
        })?;
        return Ok(json!({"layer": id.0, "index": index, "ms": t0.ms()}));
    }
    let params = stored(&settings, p, None);
    let convert = p.get("convert").and_then(Value::as_bool).unwrap_or(false);
    let mut out_id = id;
    s.edit("Camera Raw Filter", |doc: &mut Document, active| {
        let canvas = doc.bounds();
        let selection = doc.selection.clone();
        let locks = doc.effective_locks(id);
        let l = doc.layer(id).ok_or(EngineError::NoLayer(id))?;
        if convert && matches!(l.content, LayerContent::Raster(_)) {
            let so = crate::smart_cmds::layer_to_smart(doc, l)?;
            out_id = so.id;
            let path = doc.path_of(id).ok_or(EngineError::NoLayer(id))?;
            *doc.layer_at_mut(&path).ok_or(EngineError::NoLayer(id))? = so;
            *active = Some(out_id);
        }
        let l = doc.layer_mut(out_id).ok_or(EngineError::NoLayer(out_id))?;
        if let LayerContent::Smart(_) = l.content {
            return crate::smart_cmds::add_smart_filter(doc, out_id, smart_filter(params.clone()), selection.as_ref());
        }
        if locks.all || locks.pixels {
            return Err(EngineError::Other(format!("layer \"{}\" is locked", l.name)));
        }
        let LayerContent::Raster(surf) = &mut l.content else {
            return Err(EngineError::Other("Camera Raw Filter needs a pixel layer".into()));
        };
        let out = dev::develop_surface(surf, develop_area(surf, canvas), &settings, &profile).map_err(EngineError::Other)?;
        *surf = match &selection {
            Some(sel) => crate::lens_cmds::mix_by_selection(surf, &out, sel),
            None => out,
        };
        Ok(())
    })?;
    Ok(json!({"layer": out_id.0, "converted": out_id != id, "identity": settings == *dev::identity(), "ms": t0.ms()}))
}

/// Stamps the visible layers of `doc` into one pixel layer (as Merge Visible composites them).
fn stamp_visible(doc: &Document) -> Result<Layer> {
    let mut solo = doc.clone();
    solo.layers.retain(|l| l.visible);
    let buf = photocraft_compose::flatten(&solo);
    let fmt = doc.pixel_format();
    let fmt = photocraft_color::PixelFormat::new(fmt.mode, fmt.sample, true);
    let data: Vec<f32> = buf.px.iter().flat_map(|q| photocraft_raster::from_rgba(&fmt, *q)).collect();
    let mut l = Layer::raster("Composite", fmt);
    let surf = crate::pixels_mut(&mut l)?;
    surf.write_region(doc.bounds(), &data);
    surf.prune();
    Ok(l)
}

fn composite_cmd(s: &mut Session, p: &Value) -> Result<Value> {
    let settings = dev::filter_settings(p).map_err(|e| bad(COMPOSITE, e))?;
    let live = match p.get("mode").and_then(Value::as_str).unwrap_or("live") {
        "live" => true,
        "stamp" => false,
        m => return Err(bad(COMPOSITE, format!("unknown mode `{m}` (live|stamp)"))),
    };
    let params = stored(&settings, p, None);
    let label = if live { "Develop Composite" } else { "Develop Merged Copy" };
    let mut new_id = LayerId(0);
    s.edit(label, |doc, active| {
        let top = doc.layers.iter().rposition(|l| l.visible).ok_or_else(|| EngineError::Other("the document has no visible layers".into()))?;
        let smart = if live {
            // the visible layers, in order, into a group at the top visible layer's place
            let mut kept = Vec::new();
            let mut inside = Vec::new();
            let mut at = 0;
            for (i, l) in std::mem::take(&mut doc.layers).into_iter().enumerate() {
                if l.visible {
                    inside.push(l);
                    if i == top {
                        at = kept.len();
                    }
                } else {
                    kept.push(l);
                }
            }
            let mut group = Layer::group("Composite", inside);
            group.blend = photocraft_color::BlendMode::PassThrough;
            // (on failure the edit is dropped: the document is untouched)
            let so = crate::smart_cmds::layer_to_smart(doc, &group)?;
            doc.layers = kept;
            doc.layers.insert(at.min(doc.layers.len()), so.clone());
            so
        } else {
            let stamp = stamp_visible(doc)?;
            let so = crate::smart_cmds::layer_to_smart(doc, &stamp)?;
            doc.layers.push(so.clone());
            so
        };
        new_id = smart.id;
        *active = Some(new_id);
        crate::smart_cmds::add_smart_filter(doc, new_id, smart_filter(params.clone()), None)
    })?;
    Ok(json!({"layer": new_id.0, "mode": if live { "live" } else { "stamp" }}))
}

const PARAMS: &str = r##"{…DevelopSettings JSON (missing fields = defaults; crop, geometry, lens profile, calibration and Enhance are off for filters),"layer":id?,"convert":bool=false (pixel layer → smart object first),"index":n? (replace that filter.develop smart filter's settings)}"##;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: DEVELOP,
            label: "Camera Raw Filter…",
            menu: &["Filter"],
            shortcut: Some("Cmd+Shift+A"),
            params: PARAMS,
            enabled,
            run: develop_cmd,
            journal: true,
        },
        CommandSpec {
            id: COMPOSITE,
            label: "Develop Composite",
            menu: &[],
            shortcut: None,
            params: r##"{…DevelopSettings JSON,"mode":"live|stamp"} → {layer}"##,
            enabled: composite_enabled,
            run: composite_cmd,
            journal: true,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(depth: u32) -> Session {
        let mut s = Session::new();
        s.execute("file.new", json!({"width": 64, "height": 40, "depth": depth})).unwrap();
        s.execute("layer.new.layer", json!({"name": "photo"})).unwrap();
        s.edit("pattern", |doc, active| {
            let surf = doc.layer_mut(active.unwrap()).unwrap().surface_mut().unwrap();
            let fmt = surf.format();
            for y in 0..40 {
                for x in 0..64 {
                    let v = 0.15 + 0.5 * ((x * 3 + y * 5) % 17) as f32 / 16.0;
                    let a = if x < 4 {
                        0.0
                    } else if x < 8 {
                        0.5
                    } else {
                        1.0
                    };
                    surf.write_pixel(x, y, &photocraft_raster::from_rgba(&fmt, [v, v * 0.8, v * 0.6, a]));
                }
            }
            Ok(())
        })
        .unwrap();
        s
    }

    fn px(s: &Session, x: i32, y: i32) -> [f32; 4] {
        let d = s.active().unwrap();
        let l = d.doc.layer(d.active_layer.unwrap()).unwrap();
        match &l.content {
            LayerContent::Smart(sm) => sm.cache.as_ref().unwrap().rgba(x, y),
            _ => l.surface().unwrap().rgba(x, y),
        }
    }

    fn steps(s: &Session) -> usize {
        s.active().unwrap().history.past_len()
    }

    fn exposure(ev: f64) -> Value {
        json!({"light": {"exposure": ev}})
    }

    #[test]
    fn develops_pixel_layers_in_one_step_keeping_alpha() {
        for depth in [8, 16, 32] {
            let mut s = session(depth);
            assert!(s.is_enabled(DEVELOP));
            let (before, edge) = (px(&s, 30, 20), px(&s, 6, 20));
            let n = steps(&s);
            let r = s.execute(DEVELOP, exposure(1.0)).unwrap();
            assert_eq!(r["identity"], false);
            assert_eq!(steps(&s), n + 1, "{depth}-bit: one history step");
            let after = px(&s, 30, 20);
            assert!(after[1] > before[1] + 0.05, "{depth}-bit: {before:?} → {after:?}");
            assert_eq!(px(&s, 6, 20)[3], edge[3], "{depth}-bit: alpha untouched");
            assert_eq!(px(&s, 1, 20)[3], 0.0);
            s.undo();
            assert_eq!(px(&s, 30, 20), before);
        }
    }

    #[test]
    fn identity_settings_change_nothing_and_frame_tools_are_ignored() {
        let mut s = session(8);
        let before = px(&s, 30, 20);
        let r = s.execute(DEVELOP, json!({"crop": {"flip_h": true}, "geometry": {"rotate": 20.0}, "orientation": "Rotate90"})).unwrap();
        assert_eq!(r["identity"], true);
        assert_eq!(px(&s, 30, 20), before);
    }

    #[test]
    fn selection_limits_the_filter() {
        let mut s = session(8);
        s.execute("select.rect", json!({"x": 0, "y": 0, "width": 32, "height": 40})).unwrap();
        let (inside, outside) = (px(&s, 20, 20), px(&s, 50, 20));
        s.execute(DEVELOP, exposure(-1.0)).unwrap();
        assert!(px(&s, 20, 20)[1] < inside[1]);
        assert_eq!(px(&s, 50, 20), outside);
    }

    #[test]
    fn convert_makes_a_re_editable_smart_filter_in_one_step() {
        let mut s = session(16);
        let destructive = {
            let mut d = session(16);
            d.execute(DEVELOP, exposure(0.7)).unwrap();
            px(&d, 30, 20)
        };
        let n = steps(&s);
        let r = s.execute(DEVELOP, json!({"light": {"exposure": 0.7}, "convert": true})).unwrap();
        assert_eq!(r["converted"], true);
        assert_eq!(steps(&s), n + 1, "convert + filter = one step");
        let id = LayerId(r["layer"].as_u64().unwrap());
        let sm = |s: &Session| match &s.active().unwrap().doc.layer(id).unwrap().content {
            LayerContent::Smart(sm) => sm.clone(),
            _ => panic!("not a smart object"),
        };
        assert_eq!(sm(&s).smart_filters.len(), 1);
        assert_eq!(sm(&s).smart_filters[0].command, DEVELOP);
        assert_eq!(sm(&s).smart_filters[0].params["light"]["exposure"], 0.7);
        let smart = px(&s, 30, 20);
        for c in 0..4 {
            assert!((smart[c] - destructive[c]).abs() < 2.0 / 255.0, "smart {smart:?} vs destructive {destructive:?}");
        }
        // Re-edit: replaces the settings (one step) and re-renders from the source.
        s.execute(DEVELOP, json!({"light": {"exposure": -0.5}, "index": 0, "layer": id.0})).unwrap();
        assert_eq!(sm(&s).smart_filters.len(), 1);
        assert_eq!(sm(&s).smart_filters[0].params["light"]["exposure"], -0.5);
        let darker = px(&s, 30, 20);
        assert!(darker[1] < smart[1]);
        // A filter above re-renders the develop below it from the source.
        s.execute("filter.blur.gaussianBlur", json!({"radius": 1})).unwrap();
        assert_eq!(sm(&s).smart_filters.len(), 2);
        s.execute("layer.smartFilter.setVisible", json!({"layer": id.0, "index": 1, "visible": false})).unwrap();
        assert_eq!(px(&s, 30, 20), darker);
        s.undo();
        s.undo();
        s.undo();
        assert_eq!(px(&s, 30, 20), smart);
        // a stored private key survives a re-edit
        s.execute("layer.smartFilter.setParams", json!({"layer": id.0, "index": 0, "params": {"__cameraRawPsd": "abcd"}})).unwrap();
        s.execute(DEVELOP, json!({"light": {"exposure": 0.2}, "index": 0, "layer": id.0})).unwrap();
        assert_eq!(sm(&s).smart_filters[0].params["__cameraRawPsd"], "abcd");
        assert!(s.execute(DEVELOP, json!({"index": 3, "layer": id.0})).is_err());
    }

    #[test]
    fn composite_live_groups_the_visible_layers_and_stamp_merges_them() {
        let mut s = session(8);
        s.execute("layer.new.layer", json!({"name": "hidden"})).unwrap();
        s.execute("layer.setProps", json!({"visible": false})).unwrap();
        let layers = s.active().unwrap().doc.layers.len();
        let n = steps(&s);
        let r = s.execute(COMPOSITE, json!({"light": {"exposure": 0.5}, "mode": "live"})).unwrap();
        assert_eq!(steps(&s), n + 1);
        let doc = s.active().unwrap().doc.clone();
        // Background + photo grouped; the hidden layer stays outside
        assert_eq!(doc.layers.len(), layers - 1);
        let l = doc.layer(LayerId(r["layer"].as_u64().unwrap())).unwrap();
        let LayerContent::Smart(sm) = &l.content else { panic!() };
        assert_eq!(sm.smart_filters[0].command, DEVELOP);
        assert!(doc.layers.iter().any(|l| l.name == "hidden" && !l.visible));
        s.undo();
        assert_eq!(s.active().unwrap().doc.layers.len(), layers);
        let r = s.execute(COMPOSITE, json!({"light": {"exposure": 0.5}, "mode": "stamp"})).unwrap();
        let doc = s.active().unwrap().doc.clone();
        assert_eq!(doc.layers.len(), layers + 1, "a merged copy on top");
        assert_eq!(doc.layers.last().unwrap().id.0, r["layer"].as_u64().unwrap());
        assert!(s.execute(COMPOSITE, json!({"mode": "flatten"})).is_err());
    }

    #[test]
    fn legacy_camera_raw_filter_still_runs() {
        let mut s = session(8);
        let before = px(&s, 30, 20);
        s.execute(crate::lens_cmds::RAW, json!({"exposure": 1.0})).unwrap();
        assert!(px(&s, 30, 20)[1] > before[1]);
        assert!(crate::commands::find(crate::lens_cmds::RAW).unwrap().menu.is_empty());
        assert_eq!(crate::commands::find(DEVELOP).unwrap().shortcut, Some("Cmd+Shift+A"));
    }

    #[test]
    fn develop_smart_filters_round_trip_through_psd() {
        let mut s = session(8);
        let r =
            s.execute(DEVELOP, json!({"light": {"exposure": 0.4}, "treatment": "bw", "profile": {"id": "lc.vivid", "amount": 80.0}, "convert": true})).unwrap();
        let id = LayerId(r["layer"].as_u64().unwrap());
        let doc = s.active().unwrap().doc.clone();
        let out = photocraft_io::export(&doc, "psd", &Default::default()).unwrap();
        assert!(out.warnings.iter().all(|w| !w.contains("filter.develop")), "{:?}", out.warnings);
        let back = photocraft_io::import("develop.psd", &out.bytes).unwrap().document;
        let sm = |d: &Document| {
            d.walk()
                .into_iter()
                .find_map(|(_, _, l)| match &l.content {
                    LayerContent::Smart(sm) => Some(sm.clone()),
                    _ => None,
                })
                .unwrap()
        };
        let (a, b) = (sm(&doc), sm(&back));
        assert_eq!(b.smart_filters.len(), 1);
        assert_eq!(b.smart_filters[0].command, DEVELOP);
        assert_eq!(dev::filter_settings(&b.smart_filters[0].params).unwrap(), dev::filter_settings(&a.smart_filters[0].params).unwrap());
        let _ = id;
    }
}
