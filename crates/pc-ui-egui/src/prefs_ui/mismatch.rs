//! The embedded-profile mismatch dialog: its target capture, body and confirmation.
use super::*;

/// Capture before Open Document callbacks can switch, edit or close the admitted tab.
pub(crate) fn mismatch_target(session: &photocraft_engine::Session) -> Option<Value> {
    let state = session.active()?;
    Some(json!({"doc":state.doc.id.0,"name":state.doc.name,"revision":state.revision,
        "profile":photocraft_engine::color_cmds::document_profile(&state.doc).content_hash(),
        "working":session.color.working(state.doc.mode).content_hash(),
        "intent":session.color.settings.intent,"bpc":session.color.settings.bpc}))
}

/// The mismatch report and decision belong to the captured imported document.
pub(crate) fn open_mismatch_for(app: &mut PhotocraftApp, report: &Value, target: Value) -> u64 {
    let msg = if report.get("invalidProfile").and_then(Value::as_bool) == Some(true) {
        format!(
            "The embedded colour profile is unusable or does not match the document's colour model. Working space: {}.",
            report["working"].as_str().unwrap_or("?")
        )
    } else if report.get("missing").and_then(Value::as_bool) == Some(true) {
        format!("The document does not have an embedded colour profile. Working space: {}.", report["working"].as_str().unwrap_or("?"))
    } else {
        format!(
            "The document has an embedded colour profile that does not match the working space.\nEmbedded: {}\nWorking: {}",
            report["embedded"].as_str().unwrap_or("?"),
            report["working"].as_str().unwrap_or("?")
        )
    };
    let choice = |action: Option<&str>| match action {
        Some("converted") => "convert",
        Some("discarded") => "discard",
        Some("assigned") => "assignWorking",
        _ => "preserve",
    };
    let applied = choice(report.get("appliedAction").or_else(|| report.get("action")).and_then(Value::as_str));
    let action = choice(report.get("requestedAction").or_else(|| report.get("action")).and_then(Value::as_str));
    open_kind(
        app,
        "mismatch",
        tl!("Embedded Profile Mismatch"),
        json!({"message": format!("{}\n{msg}",target["name"].as_str().unwrap_or("Document")), "action": action, "applied": applied, "missing": report.get("missing").is_some(), "target":target}),
    )
}

pub(super) fn mismatch_body(ui: &mut egui::Ui, f: &mut Map<String, Value>) {
    let t = Tokens::get(ui.ctx());
    ui.label(RichText::new(f.get("message").and_then(Value::as_str).unwrap_or("")).color(t.text));
    ui.add_space(6.0);
    let mut action = f.get("action").and_then(Value::as_str).unwrap_or("preserve").to_string();
    let missing = f.get("missing").and_then(Value::as_bool).unwrap_or(false);
    let opts: &[(&str, &str)] = if missing {
        &[("preserve", tl!("Leave as is (don't color manage)")), ("assignWorking", tl!("Assign working space"))]
    } else {
        &[
            ("preserve", tl!("Use the embedded profile (instead of the working space)")),
            ("convert", tl!("Convert document's colors to the working space")),
            ("discard", tl!("Discard the embedded profile (don't color manage)")),
        ]
    };
    for (v, label) in opts {
        ui.radio_value(&mut action, v.to_string(), *label);
    }
    f.insert("action".into(), json!(action));
}

pub(super) fn confirm_mismatch(app: &mut PhotocraftApp, fields: &Map<String, Value>) -> Result<Value, String> {
    let target = fields.get("target").ok_or("The colour decision has no document; open the file again")?;
    let doc = target["doc"].as_u64().ok_or("The colour decision has no document")?;
    let index = app.session.documents().iter().position(|state| state.doc.id.0 == doc).ok_or("The colour decision's document was closed")?;
    let viewed = app.session.active().map(|state| state.doc.id);
    app.session.set_active(index);
    let result = (|| {
        if mismatch_target(&app.session).as_ref() != Some(target) {
            return Err("The document or colour settings changed; choose its profile again".into());
        }
        let action = fields.get("action").and_then(Value::as_str).unwrap_or("preserve");
        let applied = fields.get("applied").and_then(Value::as_str).unwrap_or("preserve");
        if action == applied {
            return Ok(json!({"action":action}));
        }
        let params = json!({"action":action});
        if app.automation_input
            && let Some(authorize) = app.services.automation_command.as_ref()
        {
            authorize("color.profileMismatch", &params)?;
        }
        let gate = if app.automation_input && app.session.authorize.is_none() { app.services.automation_authorize } else { None };
        if gate.is_some() {
            app.session.authorize = gate;
        }
        let suppress = app.automation_input && app.session.prefs().script_events.enabled;
        if suppress {
            app.session.edit_prefs(|prefs| prefs.script_events.enabled = false);
        }
        let result = app.session.execute("color.profileMismatch", params).map_err(|error| error.to_string());
        if suppress {
            app.session.edit_prefs(|prefs| prefs.script_events.enabled = true);
        }
        if gate.is_some() {
            app.session.authorize = None;
        }
        result
    })();
    if let Some(index) = viewed.and_then(|id| app.session.documents().iter().position(|state| state.doc.id == id)) {
        app.session.set_active(index);
    }
    app.sync_views();
    result
}
