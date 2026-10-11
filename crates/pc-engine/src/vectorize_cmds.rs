//! Image trace commands: snapshot-based background work, scaled preview, one undoable group.
use crate::{
    EngineError, Result, Session,
    commands::CommandSpec,
    jobs,
    vector_cmds::bad,
    vector_packages::trace_output::{self, Capture, Output},
};
use pc_trace::{Params, Preset};
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
    trace_output::capture(s, Some(id)).map(|_| ()).map_err(|e| e.to_string())
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
    let request: Capture = match p.get("capture") {
        Some(v) => serde_json::from_value(v.clone()).map_err(|e| bad("layer.vectorize", e.to_string()))?,
        None => trace_output::capture(s, Some(id))?,
    };
    if request.layer != id {
        return Err(bad("layer.vectorize", "captured source differs from layer"));
    }
    trace_output::check(s, &request)?;
    let output: Output = serde_json::from_value(p.get("output").cloned().unwrap_or_else(|| json!({}))).map_err(|e| bad("layer.vectorize", e.to_string()))?;
    output.validate(&params)?;
    let scale = if preview {
        match p.get("scale") {
            Some(v) => v.as_f64().ok_or_else(|| bad("layer.vectorize.preview", "scale must be numeric"))?,
            None => 0.5,
        }
    } else {
        1.
    };
    if !scale.is_finite() || !(0.01..=1.).contains(&scale) {
        return Err(bad("layer.vectorize.preview", "scale must be finite and 0.01..1"));
    }
    let doc = s.active().ok_or(EngineError::NoDocument)?.doc.clone();
    if preview {
        let keys = vec![format!("trace-preview:{}:{}", request.document, request.layer.0)];
        let mailbox = match p.get("preview_mailbox") {
            Some(id) => Some(
                trace_output::take_mailbox(id.as_u64().ok_or_else(|| bad("layer.vectorize.preview", "invalid preview mailbox"))?)
                    .ok_or_else(|| bad("layer.vectorize.preview", "missing preview mailbox"))?,
            ),
            None => None,
        };
        return jobs::run_latest(
            s,
            "Vectorize Preview",
            keys,
            move |ctx| {
                if let Some(mailbox) = mailbox {
                    let result = trace_output::trace(&doc, &request, &params, &output, scale, ctx)?;
                    ctx.check()?;
                    let summary = json!({"capture":result.capture,"scale":result.scale,"colors":result.trace.layers.len(),"nodes":result.trace.nodes});
                    let _ = mailbox.send(result);
                    Ok(summary)
                } else {
                    // Automation retains detached T1 paths without allocating pixel JSON.
                    let result = trace_output::trace_geometry(&doc, &request, &params, &output, scale, ctx)?;
                    serde_json::to_value(result).map_err(|e| bad("layer.vectorize.preview", e.to_string()))
                }
            },
            |_, result| Ok(result),
        );
    }
    let captured = request.clone();
    jobs::run(
        s,
        "Vectorize",
        true,
        move |ctx| {
            let result = trace_output::trace_geometry(&doc, &request, &params, &output, 1., ctx)?;
            trace_output::prepare(&doc, &result, &output, ctx)
        },
        move |s, result| trace_output::publish(s, &captured, result),
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
            params: r#"{"layer":id?,"params":{"preset":"logo|black_white|few_colors|silhouette|photo|pixel_art",…},"capture":Capture?,"output":{"expand":bool,"work_path":bool,"palette_edits":[{from:rgb,to:rgb|null}]}} → {group|path,colors,nodes,shapes}"#,
            enabled,
            run: apply,
            journal: true,
        },
        CommandSpec {
            id: "layer.vectorize.preview",
            label: "Vectorize Preview",
            menu: &[],
            shortcut: None,
            params: r#"{"layer":id?,"scale":0.01..1=0.5,"params":{…}} → {capture,trace,scale,scale_xy,bounds,elapsed_ms}; paths use preview pixels relative to bounds"#,
            enabled,
            run: preview,
            journal: false,
        },
    ]
}
