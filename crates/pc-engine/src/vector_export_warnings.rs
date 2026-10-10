//! Shared vector export warning presentation and read-only PSD export guard.
//! File-format loss detection lives in pc-io, below the engine/UI boundary.

use photocraft_doc::Document;
use photocraft_doc::vector_design::exchange::{ExportedVector, VectorWarning};
use photocraft_io::PsdExportOptions;
pub use photocraft_io::vector_map::fallback::{LossCollector, Projection};

use crate::{EngineError, Result, Session};

pub fn collect(doc: &Document) -> Vec<VectorWarning> {
    LossCollector::document(doc).warnings()
}

/// Other adapters (14/21) pass their structured losses here for the same one-notice behavior.
pub fn notice(format: &str, warnings: &[VectorWarning]) -> Option<String> {
    notice_with(format, warnings, &str::to_owned)
}

pub fn notice_with(format: &str, warnings: &[VectorWarning], translate: &dyn Fn(&str) -> String) -> Option<String> {
    let mut collector = LossCollector::default();
    collector.extend(warnings.iter().cloned());
    collector.notice_with(format, translate)
}

/// Serialization is detached/read-only: no Session::edit, journal event or undo step.
pub fn export_psd(session: &Session, document: u64, revision: u64, options: &PsdExportOptions, projection: Option<&dyn Projection>) -> Result<ExportedVector> {
    let state = session.active().ok_or(EngineError::NoDocument)?;
    if state.doc.id.0 != document || state.revision != revision {
        return Err(EngineError::Other("PSD export is stale; reopen the export dialog".into()));
    }
    photocraft_io::vector_map::fallback::export(&state.doc, options, projection).map_err(|e| EngineError::Other(e.to_string()))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use photocraft_doc::{ColorMode, SampleType, Size};

    #[test]
    fn vector_export_psd_staleness_and_read_only_serialization() {
        let doc = Document::new("Export", Size::new(8, 8), ColorMode::Rgb, SampleType::U8);
        let id = doc.id.0;
        let mut session = Session::new();
        session.open_document(doc, None);
        let state = session.active().unwrap();
        let revision = state.revision;
        let history = state.history.past_len();
        assert!(export_psd(&session, id + 1, revision, &Default::default(), None).is_err());
        assert!(export_psd(&session, id, revision + 1, &Default::default(), None).is_err());
        let output = export_psd(&session, id, revision, &Default::default(), None).unwrap();
        assert!(output.bytes.starts_with(b"8BPS"));
        assert!(output.warnings.is_empty());
        assert!(session.journal.is_empty());
        let state = session.active().unwrap();
        assert_eq!(state.revision, revision);
        assert_eq!(state.history.past_len(), history);
    }

    #[test]
    fn vector_export_warning_boundary_aggregates_adapter_losses() {
        let warning = VectorWarning { feature: "width-profile".into(), object: Some("A".into()), message: "width profiles".into() };
        assert!(notice("SVG", &[]).is_none());
        let line = notice("SVG", &[warning.clone(), warning]).unwrap();
        assert!(line.starts_with("SVG"));
        assert_eq!(line.matches("width profiles").count(), 1);
    }
}
