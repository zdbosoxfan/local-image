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
    let options = crate::batch::Options::from_params(p, cmd)?;
    let format = p.get("format").and_then(Value::as_str).unwrap_or("same").to_string();
    let source = p.get("source").and_then(Value::as_str).unwrap_or("folder");
    if !matches!(source, "folder" | "open") {
        return Err(EngineError::BadParams { cmd: cmd.into(), msg: format!("unknown source `{source}` (folder or open)") });
    }
    let save = SaveOpts::from_params(p);
    if source == "open" {
        let documents = s.documents().len();
        charge_steps(s, documents, steps.len(), cmd)?;
        return batch_open_documents(s, &steps, p, &options, &format, save);
    }
    let inputs = batch_inputs(p, cmd)?;
    let output = str_param(p, "output", cmd)?.to_string();
    charge_steps(s, inputs.len(), steps.len(), cmd)?;
    let charged = s.actions.script_steps;
    let actions = crate::actions_cmds::ActionState { playing: s.actions.playing, script_depth: s.actions.script_depth, ..Default::default() };
    let authorize = s.authorize;
    // The files are processed on a worker when started as a job (the UI does): the status bar
    // shows "n of total: file" and Cancel stops after the current file.
    crate::jobs::run(
        s,
        "Batch",
        false,
        move |ctx| {
            let carried = std::cell::Cell::new(charged);
            let r = process_files(&inputs, &output, &format, save, &options, Some(ctx), &|scratch, out| {
                scratch.actions = actions.clone();
                scratch.actions.script_steps = carried.get();
                scratch.authorize = authorize;
                let result = run_steps(scratch, &steps).and_then(|marked| match marked {
                    Some(doc) => save_doc(&doc, out, save).map(|_| true),
                    None => Ok(false),
                });
                carried.set(scratch.actions.script_steps);
                result
            })?;
            Ok((r, carried.get()))
        },
        |s, (r, spent)| {
            s.actions.script_steps = spent;
            Ok(r)
        },
    )
}

/// Reserve the complete input and direct-step cost before reading any files. Nested work
/// spends the same counter, but cannot make later imports escape their original charge.
fn charge_steps(s: &mut Session, inputs: usize, steps: usize, cmd: &str) -> Result<()> {
    let cost = (inputs as u64).saturating_mul((steps as u64).saturating_add(1));
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
    Ok(())
}

/// Runs the action's steps on the active document. Returns the document a recorded save marked
/// when a later step left another document active: that marked state is the result. While the
/// marked document stays active, the steps after the save still reach the result.
fn run_steps(s: &mut Session, steps: &[(String, Value)]) -> Result<Option<Arc<Document>>> {
    let mut saved: Option<Arc<Document>> = None;
    for (id, params) in steps {
        if batch_save_point(id) {
            saved = Some(s.active().ok_or(EngineError::NoDocument)?.doc.clone());
            continue;
        }
        let result = s.execute(id, params.clone())?;
        if let Some(error) = crate::automate_cmds::nested_failure(id, &result) {
            return Err(EngineError::Other(error));
        }
    }
    Ok(saved.filter(|doc| s.active().is_none_or(|d| d.doc.id != doc.id)))
}

/// Batch with Source: Opened Files. The steps run on every open document in turn (as history
/// steps of each); with `"output"` a copy of each is saved there, named by the template from
/// the document's name (an untitled document saved as `"same"` becomes a PSD, as it would in
/// Photoshop); without, the documents just stay changed (Destination: None).
fn batch_open_documents(s: &mut Session, steps: &[(String, Value)], p: &Value, options: &crate::batch::Options, format: &str, save: SaveOpts) -> Result<Value> {
    let n = s.documents().len();
    if n == 0 {
        return Err(EngineError::NoDocument);
    }
    let output = p.get("output").and_then(Value::as_str).map(str::trim).filter(|o| !o.is_empty()).map(str::to_string);
    let date = crate::batch::Date::today();
    let prev = s.active_index();
    let mut files = Vec::new();
    let mut errors = Vec::new();
    let mut written = OutputClaims::default();
    let mut stopped = None;
    let mut done = 0;
    let names: Vec<String> = s.documents().iter().map(|d| d.path.clone().unwrap_or_else(|| d.doc.name.clone())).collect();
    for (i, name) in names.iter().enumerate() {
        if !s.set_active(i) {
            break;
        }
        let r = (|| -> Result<()> {
            let marked = run_steps(s, steps)?;
            let Some(output) = &output else { return Ok(()) };
            let d = s.documents().get(i).ok_or(EngineError::NoDocument)?;
            let ext = if format == "same" {
                d.path.as_deref().and_then(extension).unwrap_or_else(|| "psd".into())
            } else {
                format.trim_start_matches('.').to_ascii_lowercase()
            };
            let out = join(output, &options.naming.file_name(&stem(name), &ext, i as u64, date));
            written.check(&out).map_err(EngineError::Other)?;
            save_doc(marked.as_deref().unwrap_or(&*d.doc), &out, save)?;
            written.record(&out, name);
            files.push(out);
            Ok(())
        })();
        done = i + 1;
        if let Err(e) = r {
            errors.push(json!({"document": name, "error": e.to_string()}));
            if options.stop_on_error {
                stopped = Some(name.clone());
                break;
            }
        }
    }
    if let Some(i) = prev {
        s.set_active(i);
    }
    if let Some(log) = &options.error_log
        && !errors.is_empty()
    {
        crate::batch::write_error_log(log, &errors, date)?;
    }
    let skipped = || names.iter().skip(done).cloned().collect();
    let mut r = crate::batch::outcome(files, errors, stopped.map(|at| (at, skipped())), options.error_log.as_deref());
    r["documents"] = json!(n);
    Ok(r)
}
