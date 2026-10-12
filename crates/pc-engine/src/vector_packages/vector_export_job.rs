//! Shared detached vector export jobs. Binary payloads bypass JSON job history.
use crate::{
    EngineError, Result, Session,
    jobs::{JobCtx, Started},
};
use photocraft_doc::{Document, vector_design::exchange::ExportedVector};
use serde_json::{Value, json};
use std::sync::{
    Arc, TryLockError,
    mpsc::{self, Receiver},
};

#[derive(Clone)]
pub struct Capture {
    pub document: u64,
    pub revision: u64,
    pub snapshot: Arc<Document>,
}
impl Capture {
    pub fn new(session: &Session, document: u64, revision: u64) -> Result<Self> {
        let state =
            session.documents().iter().find(|state| state.doc.id.0 == document).ok_or_else(|| EngineError::Other("Export document was closed".into()))?;
        if state.revision != revision {
            return Err(EngineError::Other("Export document changed; reopen the dialog".into()));
        }
        Ok(Self { document, revision, snapshot: state.doc.clone() })
    }
    pub fn check(&self, session: &Session) -> Result<()> {
        let current = Self::new(session, self.document, self.revision)?;
        if !Arc::ptr_eq(&current.snapshot, &self.snapshot) {
            return Err(EngineError::Other("Export document changed; reopen the dialog".into()));
        }
        Ok(())
    }
}

pub fn font_snapshot(cancelled: &dyn Fn() -> bool) -> Result<photocraft_text::TextEngine> {
    loop {
        if cancelled() {
            return Err(EngineError::Cancelled);
        }
        match photocraft_text::shared().try_lock() {
            Ok(engine) => return Ok(engine.fork()),
            Err(TryLockError::Poisoned(_)) => return Err(EngineError::Other("text engine lock poisoned".into())),
            Err(TryLockError::WouldBlock) => std::thread::sleep(std::time::Duration::from_millis(2)),
        }
    }
}

/// This UI-oriented entry point does not lock or mutate the document. The caller must
/// repeat capture/authority checks before committing its prepared output token.
pub fn start<T: Send + 'static>(
    session: &mut Session,
    command: &'static str,
    params: Value,
    capture: Capture,
    work: impl FnOnce(&Document, &JobCtx) -> Result<T> + Send + 'static,
) -> Result<(Started, Receiver<T>)> {
    if let Some(gate) = session.authorize {
        gate(command, &params)?;
    }
    if let Some(why) = session.disabled_reason_with(command, &params) {
        return Err(EngineError::Disabled(command.into(), why));
    }
    capture.check(session)?;
    let (tx, rx) = mpsc::sync_channel(1);
    let snapshot = capture.snapshot.clone();
    let started = session.start_job(
        command,
        params,
        "Export vector artwork",
        false,
        move |ctx| {
            ctx.check()?;
            let output = work(&snapshot, ctx)?;
            ctx.check()?;
            Ok(output)
        },
        move |session, output| {
            capture.check(session)?;
            tx.send(output).map_err(|_| EngineError::Cancelled)?;
            Ok(json!({"ready":true}))
        },
    )?;
    Ok((started, rx))
}

/// Optional command encoding is parsed outside the stable typed Request.
pub fn request(p: &Value) -> Result<(Value, bool)> {
    let mut params = p.clone();
    let object = params.as_object_mut().ok_or_else(|| EngineError::Other("export params must be an object".into()))?;
    let base64 = match object.remove("encoding") {
        None => false,
        Some(Value::String(value)) if value == "array" => false,
        Some(Value::String(value)) if value == "base64" => true,
        _ => return Err(EngineError::Other("export encoding must be array or base64".into())),
    };
    Ok((params, base64))
}
pub fn response(output: ExportedVector, mime: &str, base64: bool) -> Value {
    if base64 {
        json!({"bytes":photocraft_paint::tile::b64_encode(&output.bytes),"encoding":"base64","warnings":output.warnings,"mime":mime})
    } else {
        json!({"bytes":output.bytes,"warnings":output.warnings,"mime":mime})
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use photocraft_doc::{ColorMode, SampleType, Size};
    fn session() -> Session {
        let mut session = Session::new();
        session.add_document(Document::new("export", Size::new(16, 16), ColorMode::Rgb, SampleType::U8), None);
        session
    }
    fn capture(session: &Session) -> Capture {
        let state = session.active().unwrap();
        Capture::new(session, state.doc.id.0, state.revision).unwrap()
    }
    #[test]
    fn vector_export_binary_receipt_is_small_and_preserves_view_history() {
        let mut session = session();
        let captured = capture(&session);
        let original = captured.snapshot.clone();
        let (started, receiver) = start(&mut session, "file.svg.export", json!({}), captured, |_, _| Ok(vec![7u8; 1 << 20])).unwrap();
        session.add_document(Document::new("viewing", Size::new(1, 1), ColorMode::Rgb, SampleType::U8), None);
        let viewing = session.active().unwrap().doc.id;
        let Started::Job(id) = started else { panic!("expected worker") };
        let receipt = session.wait_job(id).unwrap();
        assert!(receipt.to_string().len() < 64);
        assert!(receipt.get("bytes").is_none());
        assert_eq!(receiver.recv().unwrap(), vec![7u8; 1 << 20]);
        assert_eq!(session.active().unwrap().doc.id, viewing);
        assert!(Arc::ptr_eq(&session.documents()[0].doc, &original));
        assert_eq!(session.documents()[0].history.past_len(), 0);
        assert!(session.journal.is_empty());
    }
    #[test]
    fn vector_export_cancel_and_stale_apply_drop_owned_payloads() {
        struct Owned(Arc<std::sync::atomic::AtomicUsize>);
        impl Drop for Owned {
            fn drop(&mut self) {
                self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
        }
        for cancel in [true, false] {
            let mut session = session();
            let captured = capture(&session);
            let drops = Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let worker_drops = drops.clone();
            let (release, gate) = mpsc::channel();
            let (entered, waiting) = mpsc::channel();
            let (started, receiver) = start(&mut session, "file.pdf.export", json!({}), captured, move |_, _| {
                entered.send(()).unwrap();
                gate.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
                Ok(Owned(worker_drops))
            })
            .unwrap();
            let Started::Job(id) = started else { panic!("worker") };
            waiting.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
            if cancel {
                session.cancel_job(id);
            } else {
                session
                    .edit("user edit", |doc, _| {
                        doc.name = "changed".into();
                        Ok(())
                    })
                    .unwrap();
            }
            release.send(()).unwrap();
            assert!(session.wait_job(id).is_err());
            session.join_cancelled_jobs();
            assert!(receiver.recv_timeout(std::time::Duration::from_secs(1)).is_err());
            assert_eq!(drops.load(std::sync::atomic::Ordering::SeqCst), 1);
        }
    }
    #[test]
    fn vector_export_waiting_for_font_snapshot_is_cancellable() {
        let guard = photocraft_text::shared().lock().unwrap();
        let cancelled = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let worker_flag = cancelled.clone();
        let worker = std::thread::spawn(move || font_snapshot(&|| worker_flag.load(std::sync::atomic::Ordering::Relaxed)).map(|_| ()));
        cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
        assert!(matches!(worker.join().unwrap(), Err(EngineError::Cancelled)));
        drop(guard);
    }
    #[test]
    fn vector_export_symbol_text_does_not_lock_the_shared_ui_engine() {
        use photocraft_doc::{Affine, Layer, LayerContent, SymbolDef, SymbolId, SymbolInstance, TextLayer};
        let mut doc = Document::new("symbol text", Size::new(128, 64), ColorMode::Rgb, SampleType::U8);
        let text = Layer::new(
            "text",
            LayerContent::Text(TextLayer {
                text: "Symbol text".into(),
                font_family: "Inter".into(),
                transform: Affine::translate(4., 30.),
                ..Default::default()
            }),
        );
        doc.symbols.push(SymbolDef { id: SymbolId(1), name: "master".into(), layers: vec![text] });
        doc.layers.push(Layer::new("instance", LayerContent::Symbol(SymbolInstance { symbol: SymbolId(1), transform: Affine::IDENTITY, cache: None })));
        let mut local = photocraft_text::TextEngine::new();
        let ui_lock = photocraft_text::shared().lock().unwrap();
        let (tx, rx) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let result = crate::vector_integration::project_for_export_with(&doc, &mut local, &|| false).and_then(|(projected, _)| {
                let mut context = photocraft_svg::export::ExportContext { cancelled: &|| false, projection: None, text: &mut local };
                photocraft_svg::export::export_with(
                    &projected,
                    &photocraft_doc::vector_design::exchange::ExportScope::Document,
                    &Default::default(),
                    &mut context,
                )
                .map_err(|error| EngineError::Other(error.to_string()))
            });
            let _ = tx.send(result);
        });
        let result = rx.recv_timeout(std::time::Duration::from_secs(2));
        drop(ui_lock);
        worker.join().unwrap();
        assert!(result.unwrap().unwrap().bytes.starts_with(b"<svg"));
    }
}
