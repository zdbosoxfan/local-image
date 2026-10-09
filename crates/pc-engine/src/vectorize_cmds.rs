//! Image trace commands: snapshot-based background work, scaled preview, one undoable group.
use crate::{
    EngineError, Result, Session,
    commands::CommandSpec,
    jobs,
    vector_cmds::{bad, refresh_shape},
};
use pc_trace::{Params, Preset};
use photocraft_color::Color;
use photocraft_doc::{Fill, Layer, LayerContent, ShapeLayer};
use photocraft_geom::Affine;
use serde_json::{Value, json};
fn enabled(s: &Session) -> std::result::Result<(), String> {
    enabled_with(s, &Value::Null)
}
pub(crate) fn enabled_with(s: &Session, p: &Value) -> std::result::Result<(), String> {
    let d = s.active().ok_or("no document open")?;
    let id = match p.get("layer") {
        Some(v) => photocraft_doc::LayerId(v.as_u64().ok_or("layer must be an id")?),
        None => d.active_layer.ok_or("no active layer")?,
    };
    let l = d.doc.layer(id).ok_or("no such layer")?;
    if matches!(l.content, LayerContent::Raster(_) | LayerContent::Smart(_)) { Ok(()) } else { Err("Vectorize requires a pixel layer or smart object".into()) }
}
fn params(p: &Value) -> Result<Params> {
    let v = p.get("params").cloned().unwrap_or_else(|| json!({}));
    let preset: Preset = match v.get("preset") {
        Some(v) => serde_json::from_value(v.clone()).map_err(|e| bad("layer.vectorize", e.to_string()))?,
        None => Preset::Logo,
    };
    let mut base = serde_json::to_value(Params::for_preset(preset)).map_err(|e| bad("layer.vectorize", e.to_string()))?;
    let overrides = v.as_object().ok_or_else(|| bad("layer.vectorize", "params must be an object"))?;
    let map = base.as_object_mut().ok_or_else(|| bad("layer.vectorize", "internal params representation"))?;
    for (k, v) in overrides {
        if !map.contains_key(k) {
            return Err(bad("layer.vectorize", format!("unknown tracing parameter {k}")));
        }
        map.insert(k.clone(), v.clone());
    }
    let params: Params = serde_json::from_value(base).map_err(|e| bad("layer.vectorize", e.to_string()))?;
    params.validate().map_err(|e| bad("layer.vectorize", e.to_string()))?;
    Ok(params)
}
fn execute(s: &mut Session, p: &Value, preview: bool) -> Result<Value> {
    let params = params(p)?;
    let id = crate::commands::layer_param(s, p)?;
    let doc = s.active().ok_or(EngineError::NoDocument)?.doc.clone();
    let l = doc.layer(id).ok_or(EngineError::NoLayer(id))?;
    if !matches!(l.content, LayerContent::Raster(_) | LayerContent::Smart(_)) {
        return Err(bad("layer.vectorize", "requires a pixel layer or smart object"));
    }
    let name = l.name.clone();
    let bounds = doc
        .selection
        .as_ref()
        .map_or(doc.bounds(), |s| if s.default_pixel().first().is_some_and(|v| *v > 0.) { doc.bounds() } else { s.content_bounds().intersect(&doc.bounds()) });
    if bounds.is_empty() {
        return Err(bad("layer.vectorize", "selection is empty"));
    }
    let scale = if preview {
        match p.get("scale") {
            Some(v) => v.as_f64().ok_or_else(|| bad("layer.vectorize.preview", "scale must be numeric"))?,
            None => 0.5,
        }
    } else {
        1.
    };
    if !scale.is_finite() || !(0.01..=1.).contains(&scale) {
        return Err(bad("layer.vectorize.preview", "scale must be 0.01..1"));
    }
    let width = bounds.width();
    let height = bounds.height();
    if u64::from(width) * u64::from(height) > pc_trace::PIXEL_BUDGET as u64 {
        return Err(bad("layer.vectorize", format!("image exceeds the {} pixel tracing budget", pc_trace::PIXEL_BUDGET)));
    }
    jobs::run(
        s,
        "Vectorize",
        !preview,
        move |ctx| {
            ctx.check()?;
            ctx.progress(0.05, "Reading image");
            let l = doc.layer(id).ok_or(EngineError::NoLayer(id))?;
            let rendered = if let LayerContent::Smart(sm) = &l.content { crate::smart_cmds::render(&doc, sm)? } else { None };
            let surface = rendered.as_ref().or_else(|| l.surface()).ok_or_else(|| bad("layer.vectorize", "smart object has no renderable pixels"))?;
            let fmt = surface.format();
            let pixels = surface.read_region(bounds);
            let channels = fmt.channels();
            let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
            for (i, p) in pixels.chunks_exact(channels).enumerate() {
                if i % width as usize == 0 {
                    ctx.check()?;
                }
                let mut c = [0.; 4];
                c[..fmt.mode.color_channels()].copy_from_slice(&p[..fmt.mode.color_channels()]);
                let alpha = if fmt.alpha { p[fmt.mode.color_channels()] } else { 1. };
                let mut color = Color { mode: fmt.mode, c, alpha }.to_rgba8();
                if let Some(selection) = &doc.selection {
                    let x = bounds.x0 + (i % width as usize) as i32;
                    let y = bounds.y0 + (i / width as usize) as i32;
                    color[3] = (f32::from(color[3]) * selection.sample_channel(x, y, 0).clamp(0., 1.)).round() as u8;
                }
                rgba.extend(color);
            }
            let (tw, th) = ((f64::from(width) * scale).round().max(1.) as u32, (f64::from(height) * scale).round().max(1.) as u32);
            if (tw, th) != (width, height) {
                let img = image::RgbaImage::from_raw(width, height, rgba).ok_or_else(|| bad("layer.vectorize", "invalid pixel buffer"))?;
                rgba = image::imageops::resize(&img, tw, th, image::imageops::FilterType::Triangle).into_raw();
            }
            ctx.check()?;
            ctx.progress(0.15, "Tracing regions");
            let result = pc_trace::trace_with_cancel(pc_trace::Image { width: tw, height: th, rgba: &rgba }, &params, || ctx.cancelled())
                .map_err(|e| EngineError::Other(e.to_string()))?;
            ctx.check()?;
            ctx.progress(1., "Trace ready");
            Ok(result)
        },
        move |s, result| {
            if preview {
                return Ok(json!({"trace":result,"scale":scale,"bounds":[bounds.x0,bounds.y0,width,height]}));
            }
            let nodes = result.nodes;
            let colors = result.layers.len();
            let id = s.edit("Vectorize", move |doc, active| {
                let mut children = Vec::new();
                for colorpath in result.layers {
                    let [r, g, b, a] = colorpath.color;
                    let mut sh = ShapeLayer {
                        path: colorpath.path.transform(&Affine::translate(f64::from(bounds.x0), f64::from(bounds.y0))),
                        fill: Some(Fill::Solid(Color::rgba(f32::from(r) / 255., f32::from(g) / 255., f32::from(b) / 255., f32::from(a) / 255.))),
                        ..Default::default()
                    };
                    refresh_shape(doc, &mut sh);
                    children.push(Layer::new(format!("#{r:02X}{g:02X}{b:02X}"), LayerContent::Shape(sh)));
                }
                let group = Layer::group(format!("Vectorized – {name}"), children);
                let group_id = doc.insert_above(Some(id), group);
                *active = Some(group_id);
                Ok(group_id)
            })?;
            Ok(json!({"group":id.0,"colors":colors,"nodes":nodes}))
        },
    )
}
fn apply(s: &mut Session, p: &Value) -> Result<Value> {
    execute(s, p, false)
}
fn preview(s: &mut Session, p: &Value) -> Result<Value> {
    execute(s, p, true)
}
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "layer.vectorize",
            label: "Vectorize…",
            menu: &[],
            shortcut: None,
            params: r#"{"layer":id?,"params":{"preset":"logo|black_white|few_colors|silhouette|photo|pixel_art",…}} → {group,colors,nodes}"#,
            enabled,
            run: apply,
            journal: true,
        },
        CommandSpec {
            id: "layer.vectorize.preview",
            label: "Vectorize Preview",
            menu: &[],
            shortcut: None,
            params: r#"{"layer":id?,"scale":0.01..1=0.5,"params":{…}} → {trace,scale,bounds}; paths use preview pixels relative to bounds"#,
            enabled,
            run: preview,
            journal: false,
        },
    ]
}
