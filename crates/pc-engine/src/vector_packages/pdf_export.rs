//! Read-only PDF export command. Captured document/revision/targets are checked before
//! detached serialization; exporting changes neither history nor the session journal.
use crate::{EngineError, Result, Session, commands::CommandSpec};
use photocraft_doc::vector_design::exchange::{ExportScope, ExportedVector};
use photocraft_doc::{Document, LayerId};
use photocraft_pdf::export::{ExportContext, ExportOptions};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
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
pub type Provider = fn(&Document, &ExportScope, &ExportOptions, &mut ExportContext<'_>) -> photocraft_pdf::export::Result<ExportedVector>;
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
        return Err(error("PDF export is stale; reopen the export dialog"));
    }
    photocraft_pdf::export::page_scopes(&state.doc, &r.scope.exchange(), &r.options).map_err(|e| error(e.to_string()))?;
    Ok(())
}
pub fn export(s: &Session, r: &Request, cancelled: &dyn Fn() -> bool) -> Result<ExportedVector> {
    preflight(s, r)?;
    let state = s.active().ok_or(EngineError::NoDocument)?;
    let (projected, warnings) = crate::vector_integration::project_for_export(&state.doc)?;
    let mut engine = photocraft_text::shared().lock().map_err(|_| error("text engine lock poisoned"))?;
    let mut cx = ExportContext { cancelled, projection: None, text: &mut engine };
    let run = PROVIDER.get().copied().unwrap_or(photocraft_pdf::export::export_with);
    let mut output = run(&projected, &r.scope.exchange(), &r.options, &mut cx).map_err(|e| error(e.to_string()))?;
    output.warnings.extend(warnings);
    preflight(s, r)?;
    Ok(output)
}
fn enabled(s: &Session) -> std::result::Result<(), String> {
    s.active().map(|_| ()).ok_or_else(|| "no document open".into())
}
fn run(s: &mut Session, p: &Value) -> Result<Value> {
    let r = serde_json::from_value(p.clone()).map_err(|e| error(format!("file.pdf.export: {e}")))?;
    let output = export(s, &r, &|| false)?;
    Ok(json!({"bytes":output.bytes,"warnings":output.warnings,"mime":"application/pdf"}))
}
pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec {
        id: "file.pdf.export",
        label: "Export PDF",
        menu: &[],
        shortcut: None,
        params: "{document:u64,revision:u64,scope:{kind:document|selection|artboard,layers?:[id],layer?:id},options?:{textAsCurves?:bool,artboards?:[id],compress?:bool}}",
        enabled,
        run,
        journal: false,
    }]
}
#[cfg(test)]
#[path = "pdf_export/tests.rs"]
mod tests;
