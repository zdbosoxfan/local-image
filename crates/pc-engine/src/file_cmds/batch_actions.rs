use super::*;

/// Recorded saves mark the result; their paths never override the batch destination.
pub(crate) fn batch_save_point(id: &str) -> bool {
    matches!(id, "file.save" | "file.saveAs" | "file.saveACopy")
}

pub(super) fn batch(s: &mut Session, p: &Value) -> Result<Value> {
    crate::automate_cmds::run_bounded(s, "file.automate.batch", |s| batch_inner(s, p))
}

fn batch_inner(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "file.automate.batch";
    let mut steps =
        parse_steps(p.get("steps").or_else(|| p.get("action")).ok_or_else(|| EngineError::BadParams { cmd: cmd.into(), msg: "missing \"steps\"".into() })?)?;
    steps.retain(|(id, _)| id != "file.export.quickExport");
    if let Some((id, _)) = steps.iter().find(|(id, _)| !batch_save_point(id) && crate::commands::find(id).is_none()) {
        return Err(EngineError::BadParams { cmd: cmd.into(), msg: format!("unknown command `{id}` in the action") });
    }
    let inputs = batch_inputs(p, cmd)?;
    let output = str_param(p, "output", cmd)?.to_string();
    let format = p.get("format").and_then(Value::as_str).unwrap_or("same").to_string();
    let save = SaveOpts::from_params(p);
    // Reserve the complete input and direct-step cost before reading any files. Nested work
    // spends the same counter, but cannot make later imports escape their original charge.
    let cost = (inputs.len() as u64).saturating_mul((steps.len() as u64).saturating_add(1));
    let charged = s.actions.script_steps.saturating_add(cost);
    if charged > crate::automate_cmds::MAX_SCRIPT_STEPS {
        return Err(EngineError::BadParams {
            cmd: cmd.into(),
            msg: format!(
                "this batch would exceed the {} step limit for scripts and batches; use fewer input files or action steps",
                crate::automate_cmds::MAX_SCRIPT_STEPS
            ),
        });
    }
    s.actions.script_steps = charged;
    let carried = std::cell::Cell::new(s.actions.script_steps);
    let actions = crate::actions_cmds::ActionState { playing: s.actions.playing, script_depth: s.actions.script_depth, ..Default::default() };
    let authorize = s.authorize;
    let r = process_files(&inputs, &output, &format, save, "", &|scratch, out| {
        scratch.actions = actions.clone();
        scratch.actions.script_steps = carried.get();
        scratch.authorize = authorize;
        let result = (|| {
            let mut saved: Option<Arc<Document>> = None;
            for (id, params) in &steps {
                if batch_save_point(id) {
                    saved = Some(scratch.active().ok_or(EngineError::NoDocument)?.doc.clone());
                    continue;
                }
                let result = scratch.execute(id, params.clone())?;
                if let Some(error) = crate::automate_cmds::nested_failure(id, &result) {
                    return Err(EngineError::Other(error));
                }
            }
            // Delay publication until all steps succeed. While the marked document remains
            // active, subsequent edits still reach the result.
            match saved {
                Some(doc) if scratch.active().is_none_or(|d| d.doc.id != doc.id) => {
                    save_doc(&doc, out, save)?;
                    Ok(true)
                }
                _ => Ok(false),
            }
        })();
        carried.set(scratch.actions.script_steps);
        result
    });
    s.actions.script_steps = carried.get();
    Ok(r)
}
