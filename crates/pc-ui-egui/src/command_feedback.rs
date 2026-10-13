//! Shared command feedback: plain status text, diagnostic history and disabled reasons.
use crate::PhotocraftApp;
use photocraft_doc::LayerContent;

/// Only engine messages reproduced by `reasons_ux_tests` are rewritten. Other diagnostics
/// retain their wording; the sink always keeps the original bytes in Details / History.
pub fn message(raw: &str) -> String {
    let detail = raw.strip_prefix("command `").and_then(|s| s.split_once("` is not available right now: ").map(|(_, why)| why)).unwrap_or(raw);
    let text = match detail {
        "no active document" | "no document open" => tl!("Open a photo first."),
        "no active layer" => tl!("Select a layer first."),
        "no selection" => tl!("Make a selection first."),
        "the clipboard is empty" => tl!("Copy an image first."),
        "nothing to undo" => tl!("There are no edits to undo."),
        "nothing to redo" => tl!("There are no edits to redo."),
        s if s.starts_with("active layer is a ") || s == "the active layer has no pixels" || s == "not a pixel layer" => tl!("Select a pixel layer first."),
        s if s.starts_with("Could not complete your request because the layer \"") && s.ends_with(" is locked") => tl!("This layer is locked."),
        s if s.starts_with("no such layer LayerId(") => tl!("This layer is no longer available. Select another layer."),
        s if s.starts_with("invalid parameters for `") => tl!("Check the settings and try again."),
        _ => return detail.to_owned(),
    };
    text.into()
}

/// One error sink for engine invocations and vector tools. Repeated forwarding of the same
/// failure is coalesced; a later separate failure is still recorded after a successful action.
pub fn report(app: &mut PhotocraftApp, raw: impl Into<String>) {
    let raw = raw.into();
    let title = message(&raw);
    if !(app.ui.status_error && app.ui.status == title && app.ui.notice_history.last().is_some_and(|n| n.error && n.lines == [raw.clone()])) {
        crate::notices::command_error(app, title.clone(), raw);
    }
    app.ui.status = title;
    app.ui.status_error = true;
}

/// A usable pixel selection, including after Inverse leaves an empty mask object.
/// Matches the AI operations' bounded selection-mask requirement without allocating it.
pub fn has_pixel_selection(app: &PhotocraftApp) -> bool {
    app.session.active().is_some_and(|st| st.doc.selection.as_ref().is_some_and(|mask| !mask.content_bounds().intersect(&st.doc.bounds()).is_empty()))
}

/// Shared reason resolver for disabled main, panel and canvas menu entries. Call only for
/// a disabled row, including rows whose contextual precondition is stricter than the registry.
pub fn disabled_reason(app: &PhotocraftApp, id: &str) -> String {
    if !crate::menus::is_live(id) {
        return tl!("This command is not available in this build.").into();
    }
    if matches!(id, crate::overwrite_original::SAVE_ID | crate::overwrite_original::RETURN_ID)
        && let Some(why) = app.session.active().and_then(|d| crate::overwrite_original::disabled_reason(app, d.doc.id))
    {
        return crate::i18n::t(why).into();
    }
    let Some(st) = app.session.active() else { return tl!("Open a photo first.").into() };
    if let Some(why) = crate::vector_integration::disabled_reason(app, id) {
        return why;
    }
    if matches!(id, "li.generativeFill" | "li.removeSelection" | "ai.remove") && !has_pixel_selection(app) {
        return tl!("Make a selection first.").into();
    }
    let layer = st.active_layer.and_then(|a| st.doc.layer(a));
    if locked_edit(app, id) {
        return tl!("This layer is locked.").into();
    }
    if let Some(l) = layer
        && let Some(reason) = layer_reason(app, id, l)
    {
        return reason;
    }
    if matches!(id, "file.save" | "file.saveAs" | "file.export.exportAs" | "file.export.quickExportAsPng") {
        return tl!("Saving is unavailable in this session.").into();
    }
    if app.ui.transform.is_some() && id.contains("ransform") {
        return tl!("Finish or cancel the current transform first.").into();
    }
    if id == "file.clearRecent" {
        return tl!("There are no recent files to clear.").into();
    }
    if let Some(raw) = app.session.disabled_reason_with(id, &app.with_mask_target(id, serde_json::Value::Null))
        && !raw.starts_with("unknown command `")
    {
        return message(&raw);
    }
    if layer.is_none() {
        return tl!("Select a layer first.").into();
    }
    if id.starts_with("path.style.paste") {
        return tl!("Copy a matching path style first.").into();
    }
    if id.starts_with("path.") || id == "paint.symmetryFromPath" {
        return tl!("Select a suitable path first.").into();
    }
    if id == "type.editText" && !layer.is_some_and(|l| matches!(l.content, LayerContent::Text(_))) {
        return tl!("Select a text layer first.").into();
    }
    tl!("This command is unavailable for the current target.").into()
}

/// A menu/dialog owns the selection it was opened for. Surface clones share tiles until
/// edited, so retaining the mask is cheap and catches selection changes without a revision.
#[derive(Clone, Debug, PartialEq)]
pub struct SelectionTarget {
    target: crate::toolsets_ui::persona::GestureTarget,
    selection: Option<photocraft_raster::Surface>,
    persona: photocraft_engine::toolsets::Persona,
}
impl SelectionTarget {
    pub fn capture(app: &PhotocraftApp) -> Option<Self> {
        Some(Self {
            target: crate::toolsets_ui::persona::GestureTarget::capture(app)?,
            selection: app.session.active()?.doc.selection.clone(),
            persona: app.session.prefs().toolbar.persona,
        })
    }
    pub fn is_current(&self, app: &PhotocraftApp) -> bool {
        self.persona == app.session.prefs().toolbar.persona
            && self.target.is_current(app)
            && app.session.active().is_some_and(|st| st.doc.selection == self.selection)
    }
}

/// Evaluate the clicked layer before the Layers menu changes the active selection.
/// Main and panel menus reuse the same locks, Background and layer-kind reasons.
pub fn layer_reason(app: &PhotocraftApp, id: &str, layer: &photocraft_doc::Layer) -> Option<String> {
    let st = app.session.active()?;
    let locks = st.doc.effective_locks(layer.id);
    if lock_blocks(id, &locks) {
        return Some(tl!("This layer is locked.").into());
    }
    if crate::enable_rules::disabled_for(&st.doc, Some(layer), id) {
        let reason = if crate::doc_props_ui::is_background(&st.doc, layer) {
            tl!("Convert the Background to a regular layer first.")
        } else if id.starts_with("layer.rasterize.") || id.starts_with("layer.combineShapes.") {
            tl!("Select a layer of the matching type first.")
        } else if id.starts_with("layer.layerStyle.") {
            tl!("Add a layer style first.")
        } else {
            tl!("There are no matching layers for this command.")
        };
        return Some(reason.into());
    }
    None
}

fn lock_blocks(id: &str, locks: &photocraft_doc::Locks) -> bool {
    let edits_pixels = id.starts_with("paint.")
        || id.starts_with("filter.")
        || id.starts_with("image.adjustments.")
        || matches!(id, "edit.fill" | "edit.stroke" | "edit.clear" | "edit.cut" | "li.removeSelection" | "li.generativeFill")
        || id.starts_with("layer.combineShapes.")
        || id.starts_with("layer.rasterize.");
    let edits_position = id.starts_with("layer.arrange.") || id.starts_with("edit.transform.") || id == "edit.freeTransform";
    (edits_pixels || edits_position) && (locks.all || (edits_pixels && locks.pixels) || (edits_position && locks.position))
}

/// Editing commands whose engine enablement does not account for inherited locks.
/// Read-only commands and the lock/unlock actions themselves remain available.
pub fn locked_edit(app: &PhotocraftApp, id: &str) -> bool {
    app.session.active().is_some_and(|st| st.selected_layers().iter().any(|layer| lock_blocks(id, &st.doc.effective_locks(*layer))))
}
