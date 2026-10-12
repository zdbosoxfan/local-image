//! Read-only SVG export command. Captured document/revision/targets are checked before
//! detached serialization; exporting changes neither history nor the session journal.
use super::vector_export_job::{self, Capture};
use crate::{EngineError, Result, Session, commands::CommandSpec};
use photocraft_doc::vector_design::exchange::{ExportScope, ExportedVector};
use photocraft_doc::{Document, LayerId};
use photocraft_svg::export::{ExportContext, ExportOptions};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::OnceLock;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum Scope {
    Document,
    Selection { layers: Vec<LayerId> },
    Artboard { layer: LayerId },
}
impl Scope {
    pub fn exchange(&self) -> ExportScope {
        match self {
            Self::Document => ExportScope::Document,
            Self::Selection { layers } => ExportScope::Selection(layers.clone()),
            Self::Artboard { layer } => ExportScope::Artboard(*layer),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub document: u64,
    pub revision: u64,
    pub scope: Scope,
    #[serde(default)]
    pub options: ExportOptions,
}
/// Registered by 00 once enhanced projection providers integrate; same detached API as display.
pub type Provider = fn(&Document, &ExportScope, &ExportOptions, &mut ExportContext<'_>) -> photocraft_svg::export::Result<ExportedVector>;
static PROVIDER: OnceLock<Provider> = OnceLock::new();
pub fn install_provider(provider: Provider) -> std::result::Result<(), Provider> {
    PROVIDER.set(provider)
}
fn error(message: impl Into<String>) -> EngineError {
    EngineError::Other(message.into())
}
pub fn preflight(s: &Session, r: &Request) -> Result<()> {
    let state = s.active().ok_or(EngineError::NoDocument)?;
    if state.doc.id.0 != r.document || state.revision != r.revision {
        return Err(error("SVG export is stale; reopen the export dialog"));
    }
    photocraft_svg::export::scoped_document(&state.doc, &r.scope.exchange()).map_err(|e| error(e.to_string()))?;
    Ok(())
}
pub fn export(s: &Session, r: &Request, cancelled: &dyn Fn() -> bool) -> Result<ExportedVector> {
    preflight(s, r)?;
    let capture = Capture::new(s, r.document, r.revision)?;
    export_document(&capture.snapshot, r, cancelled)
}

pub fn export_document(doc: &Document, r: &Request, cancelled: &dyn Fn() -> bool) -> Result<ExportedVector> {
    if cancelled() {
        return Err(EngineError::Cancelled);
    }
    if doc.id.0 != r.document {
        return Err(error("Export snapshot does not match the captured document"));
    }
    let mut text = vector_export_job::font_snapshot(cancelled)?;
    let (projected, warnings) = crate::vector_integration::project_for_export_with(doc, &mut text, cancelled)?;
    let mut cx = ExportContext { cancelled, projection: None, text: &mut text };
    let run = PROVIDER.get().copied().unwrap_or(photocraft_svg::export::export_with);
    let mut output =
        run(&projected, &r.scope.exchange(), &r.options, &mut cx).map_err(|e| if cancelled() { EngineError::Cancelled } else { error(e.to_string()) })?;
    if cancelled() {
        return Err(EngineError::Cancelled);
    }
    output.warnings.extend(warnings);
    Ok(output)
}

/// Prepare arbitrary owned output (binary bytes or a staged host token) on the worker.
pub fn start<T: Send + 'static>(
    s: &mut Session,
    r: &Request,
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    prepare: impl FnOnce(ExportedVector, &crate::jobs::JobCtx, &(dyn Fn() -> bool + Sync)) -> Result<T> + Send + 'static,
) -> Result<(crate::jobs::Started, std::sync::mpsc::Receiver<T>)> {
    preflight(s, r)?;
    let capture = Capture::new(s, r.document, r.revision)?;
    let params = serde_json::to_value(r).map_err(|e| error(e.to_string()))?;
    let request = r.clone();
    vector_export_job::start(s, "file.svg.export", params, capture, move |doc, ctx| {
        ctx.progress(0.05, "Preparing artwork");
        let cancelled = || ctx.cancelled() || cancel.load(std::sync::atomic::Ordering::Relaxed);
        let output = export_document(doc, &request, &cancelled)?;
        if cancelled() {
            return Err(EngineError::Cancelled);
        }
        ctx.progress(0.8, "Preparing output file");
        let output = prepare(output, ctx, &cancelled)?;
        if cancelled() {
            return Err(EngineError::Cancelled);
        }
        Ok(output)
    })
}

fn enabled(s: &Session) -> std::result::Result<(), String> {
    s.active().map(|_| ()).ok_or_else(|| "no document open".into())
}
fn run(s: &mut Session, p: &Value) -> Result<Value> {
    let (params, base64) = vector_export_job::request(p)?;
    let r: Request = serde_json::from_value(params).map_err(|e| error(format!("file.svg.export: {e}")))?;
    preflight(s, &r)?;
    let capture = Capture::new(s, r.document, r.revision)?;
    let snapshot = capture.snapshot.clone();
    crate::jobs::run(
        s,
        "Export SVG",
        false,
        move |ctx| {
            ctx.progress(0.05, "Preparing artwork");
            let output = export_document(&snapshot, &r, &|| ctx.cancelled())?;
            ctx.check()?;
            let result = vector_export_job::response(output, "image/svg+xml", base64);
            ctx.check()?;
            Ok(result)
        },
        move |s, result| {
            capture.check(s)?;
            Ok(result)
        },
    )
}

pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec {
        id: "file.svg.export",
        label: "Export SVG",
        menu: &[],
        shortcut: None,
        params: "{encoding?:array|base64,document:u64,revision:u64,scope:{kind:document}|{kind:selection,layers:[id]}|{kind:artboard,layer:id},options?:{textAsCurves?:bool,precision?:0..9,minify?:bool}}",
        enabled,
        run,
        journal: false,
    }]
}
#[cfg(all(test, feature = "svg-import-integrated"))]
#[path = "svg_export/gpu_tests.rs"]
mod gpu_tests;
#[cfg(test)]
#[path = "svg_export/tests.rs"]
mod tests;
