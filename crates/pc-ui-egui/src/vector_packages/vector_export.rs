//! Shared asynchronous vector export lifecycle. No encoded document bytes enter JSON.
use crate::{PhotocraftApp, PreparedWrite, Services};
use photocraft_doc::vector_design::exchange::{ExportedVector, VectorWarning};
use photocraft_engine::{
    EngineError, Session,
    jobs::{JobId, Started},
    vector_packages::{pdf_export, svg_export, vector_export_job::Capture},
};
use serde_json::Value;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{Receiver, TryRecvError},
};

#[derive(Clone)]
pub(super) enum Request {
    Svg(svg_export::Request),
    Pdf(pdf_export::Request),
}
impl Request {
    fn command(&self) -> &'static str {
        match self {
            Self::Svg(_) => "file.svg.export",
            Self::Pdf(_) => "file.pdf.export",
        }
    }
    fn extension(&self) -> &'static str {
        match self {
            Self::Svg(_) => "svg",
            Self::Pdf(_) => "pdf",
        }
    }
    fn params(&self) -> Result<Value, String> {
        match self {
            Self::Svg(r) => serde_json::to_value(r),
            Self::Pdf(r) => serde_json::to_value(r),
        }
        .map_err(|e| e.to_string())
    }
    fn preflight(&self, s: &Session) -> Result<(), String> {
        match self {
            Self::Svg(r) => svg_export::preflight(s, r),
            Self::Pdf(r) => pdf_export::preflight(s, r),
        }
        .map_err(|e| e.to_string())
    }
    fn capture(&self, s: &Session) -> Result<Capture, String> {
        let (id, revision) = match self {
            Self::Svg(r) => (r.document, r.revision),
            Self::Pdf(r) => (r.document, r.revision),
        };
        Capture::new(s, id, revision).map_err(|e| e.to_string())
    }
}
enum Payload {
    Staged(PreparedWrite),
    Bytes(Vec<u8>),
}
struct Ready {
    payload: Payload,
    warnings: Vec<VectorWarning>,
}
#[derive(Debug)]
pub(super) struct Completion {
    pub path: String,
    pub warnings: Vec<VectorWarning>,
}
#[derive(Debug)]
pub(super) enum Poll {
    Pending,
    Cancelled,
    Complete(Completion),
}
enum Terminal {
    Cancelled,
    Failed(String),
}
pub(super) fn report(app: &mut PhotocraftApp, completion: Completion) {
    let lines = completion.warnings.into_iter().map(|warning| warning.message).collect::<Vec<_>>();
    app.ui.status_error = !lines.is_empty();
    crate::notices::file_result(app, tl!("Exported"), &completion.path, &lines);
}

pub(super) struct Pending {
    job: Option<JobId>,
    receiver: Receiver<Ready>,
    cancelled: Arc<AtomicBool>,
    capture: Capture,
    request: Request,
    path: String,
    automation: bool,
    terminal: Option<Terminal>,
}
impl Drop for Pending {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

fn authority(app: &PhotocraftApp, request: &Request, automation: bool) -> Result<(), String> {
    let params = request.params()?;
    if let Some(gate) = app.session.authorize {
        gate(request.command(), &params).map_err(|e| e.to_string())?;
    }
    if automation {
        if let Some(gate) = &app.services.automation_command {
            gate(request.command(), &params)?;
        }
        if app.session.authorize.is_none()
            && let Some(gate) = app.services.automation_authorize
        {
            gate(request.command(), &params).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Asks where to export `request`, then starts it and hands the job to `started`. Returns
/// `{"pending": true}` once started, or while the save dialog is open; `Err` ("cancelled") when
/// the dialog was cancelled.
pub(super) fn begin(app: &mut PhotocraftApp, request: Request, started: impl FnOnce(&mut PhotocraftApp, Pending) + 'static) -> Result<Value, String> {
    begin_named(app, request, None, started)
}
pub(super) fn begin_named(
    app: &mut PhotocraftApp,
    request: Request,
    suggested_name: Option<&str>,
    started: impl FnOnce(&mut PhotocraftApp, Pending) + 'static,
) -> Result<Value, String> {
    request.preflight(&app.session)?;
    let automation = app.automation_input;
    authority(app, &request, automation)?;
    let capture = request.capture(&app.session)?;
    let name = std::path::Path::new(&capture.snapshot.name).file_stem().and_then(|s| s.to_str()).filter(|s| !s.is_empty()).unwrap_or("drawing");
    let suggested = suggested_name.map(str::to_owned).unwrap_or_else(|| format!("{name}.{}", request.extension()));
    if app.services.file_dialog.is_none() {
        return Err("file save picker service unavailable".into());
    }
    app.pick_save(&suggested, move |app, path| {
        let pending = start(app, request, capture, automation, path)?;
        started(app, pending);
        Ok(serde_json::json!({"pending": true}))
    })
}

/// [`begin_named`] once the path is known: the document must still be as it was when asked.
fn start(app: &mut PhotocraftApp, request: Request, capture: Capture, automation: bool, path: String) -> Result<Pending, String> {
    capture.check(&app.session).map_err(|e| e.to_string())?;
    authority(app, &request, automation)?;
    let stage = (!automation).then(|| app.services.stage_write.clone()).flatten();
    if automation && app.services.automation_write.is_none() {
        return Err("authorized export writer unavailable".into());
    }
    if !automation && stage.is_none() && app.services.write.is_none() {
        return Err("file write service unavailable".into());
    }
    let cancelled = Arc::new(AtomicBool::new(false));
    let worker_path = path.clone();
    let prepare = move |output: ExportedVector, ctx: &lightcraft_sysmem::JobCtl, cancel: &(dyn Fn() -> bool + Sync)| {
        if cancel() {
            return Err(EngineError::Cancelled);
        }
        let payload = if let Some(stage) = stage {
            let progress = |fraction: f32| ctx.report(0.8 + fraction.clamp(0.0, 1.0) * 0.18, "Writing temporary export");
            let interrupt = lightcraft_sysmem::jobs::Interrupt::new(cancel, &progress);
            Payload::Staged(stage(&worker_path, &output.bytes, &interrupt).map_err(|e| if cancel() { EngineError::Cancelled } else { EngineError::Other(e) })?)
        } else {
            Payload::Bytes(output.bytes)
        };
        if cancel() {
            return Err(EngineError::Cancelled);
        }
        ctx.report(0.99, "Ready to publish export");
        Ok(Ready { payload, warnings: output.warnings })
    };
    let result = match &request {
        Request::Svg(r) => svg_export::start(&mut app.session, r, cancelled.clone(), prepare),
        Request::Pdf(r) => pdf_export::start(&mut app.session, r, cancelled.clone(), prepare),
    }
    .map_err(|e| e.to_string())?;
    let job = match result.0 {
        Started::Job(id) => Some(id),
        Started::Done(_) => None,
    };
    Ok(Pending { job, receiver: result.1, cancelled, capture, request, path, automation, terminal: None })
}
impl Pending {
    /// One compact file failure, retaining the full destination and reason in details.
    pub(super) fn report_failure(&self, app: &mut PhotocraftApp, reason: &str) {
        let title = li_i18n::fmt(tl!("Export failed: {name}"), &[("name", &crate::file_open::display_name(&self.path))]);
        app.ui.status = title.clone();
        app.ui.status_error = true;
        crate::notices::post(app, title, vec![self.path.clone(), reason.to_owned()], true, None);
    }
    pub(super) fn on_event(&mut self, event: &photocraft_engine::jobs::JobEvent) -> bool {
        if self.job != Some(event.id) {
            return false;
        }
        match &event.outcome {
            photocraft_engine::jobs::JobOutcome::Cancelled => self.terminal = Some(Terminal::Cancelled),
            photocraft_engine::jobs::JobOutcome::Failed(error) => self.terminal = Some(Terminal::Failed(error.clone())),
            _ => {}
        }
        true
    }
    pub(super) fn check(&self, session: &Session) -> Result<(), String> {
        self.capture.check(session).map_err(|e| e.to_string())
    }
    pub(super) fn cancel(&mut self, session: &mut Session) {
        self.cancelled.store(true, Ordering::Relaxed);
        if let Some(job) = self.job {
            session.cancel_job(job);
        }
    }
    /// Draw BEFORE polling the mailbox: a Cancel click in the ready frame wins.
    pub(super) fn controls(&self, ui: &mut egui::Ui, session: &Session) -> bool {
        ui.heading(if self.request.extension() == "svg" { tl!("Export SVG") } else { tl!("Export PDF") });
        let info = self.job.and_then(|id| session.job(id));
        ui.label(info.as_ref().map_or(tl!("Preparing export"), |job| job.message.as_str()));
        ui.add(egui::ProgressBar::new(info.as_ref().map_or(1.0, |job| job.progress)).show_percentage());
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(30));
        ui.button(tl!("Cancel")).clicked()
    }
    pub(super) fn poll(&mut self, app: &mut PhotocraftApp) -> Result<Poll, String> {
        if self.cancelled.load(Ordering::Relaxed) {
            return Ok(Poll::Cancelled);
        }
        if let Some(terminal) = self.terminal.take() {
            return match terminal {
                Terminal::Cancelled => Ok(Poll::Cancelled),
                Terminal::Failed(error) => Err(error),
            };
        }
        self.check(&app.session)?;
        let output = match self.receiver.try_recv() {
            Ok(output) => output,
            Err(TryRecvError::Empty) => return Ok(Poll::Pending),
            Err(TryRecvError::Disconnected) => {
                let recent = self.job.and_then(|id| app.session.jobs_with_recent().into_iter().find(|job| job.id == id));
                if recent.as_ref().is_some_and(|job| job.state == "cancelled") {
                    return Ok(Poll::Cancelled);
                }
                return Err(recent.and_then(|job| job.error).unwrap_or_else(|| "Export worker stopped before returning output".into()));
            }
        };
        // The original snapshot may no longer be active, but it must still exist unchanged.
        self.check(&app.session)?;
        authority(app, &self.request, self.automation)?;
        if self.cancelled.load(Ordering::Relaxed) {
            return Ok(Poll::Cancelled);
        }
        match output.payload {
            Payload::Staged(prepared) => (prepared.commit)()?,
            Payload::Bytes(bytes) => write(&mut app.services, &self.path, &bytes, self.automation)?,
        }
        Ok(Poll::Complete(Completion { path: self.path.clone(), warnings: output.warnings }))
    }
}
fn write(services: &mut Services, path: &str, bytes: &[u8], automation: bool) -> Result<(), String> {
    // Compatibility for hosts without staging. Native hosts supply stage_write; automation
    // always stays on its capability-scoped writer, never the ambient staging service.
    if automation {
        services.automation_write.as_mut().ok_or("authorized export writer unavailable")?(path, bytes)
    } else {
        services.write.as_mut().ok_or("file write service unavailable")?(path, bytes)
    }
}

#[cfg(test)]
#[path = "vector_export_tests.rs"]
mod tests;
