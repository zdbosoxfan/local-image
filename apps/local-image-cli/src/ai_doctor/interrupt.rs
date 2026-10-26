//! Bridge Ctrl-C to verification, including its follow-up scan and report.

use li_ai::JobControl;
use tokio::sync::oneshot;

pub(super) struct Interrupt {
    stop: Option<oneshot::Sender<()>>,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl Interrupt {
    pub(super) fn start(ctl: JobControl) -> Result<Self, String> {
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| e.to_string())?;
        let (stop, stopped) = oneshot::channel();
        let (ready, registered) = std::sync::mpsc::sync_channel(1);
        let worker = std::thread::Builder::new()
            .name("doctor-interrupt".into())
            .spawn(move || {
                runtime.block_on(async move {
                    #[cfg(unix)]
                    let signal = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt());
                    #[cfg(windows)]
                    let signal = tokio::signal::windows::ctrl_c();
                    let mut signal = match signal {
                        Ok(signal) => signal,
                        Err(e) => {
                            let _ = ready.send(Err(e.to_string()));
                            return;
                        }
                    };
                    let _ = ready.send(Ok(()));
                    tokio::select! {
                        _ = signal.recv() => ctl.cancel(),
                        _ = stopped => {}
                    }
                });
            })
            .map_err(|e| e.to_string())?;
        registered.recv().map_err(|e| e.to_string())??;
        Ok(Self { stop: Some(stop), worker: Some(worker) })
    }
}

impl Drop for Interrupt {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
