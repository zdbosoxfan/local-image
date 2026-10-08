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
