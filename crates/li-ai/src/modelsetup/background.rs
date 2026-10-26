//! Bound background template requests when the server cannot provide a disk-cache key.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::{Options, Scan, Snapshot, Templates};
use crate::comfy::ComfyClient;

struct Cached {
    host: String,
    version: String,
    checked_at: Instant,
    templates: Result<Templates, String>,
}

pub(super) fn scan(opts: &Options, snapshot: Snapshot) -> Scan {
    let reg = crate::family::registry();
    if opts.templates_dir.is_some() || Templates::cacheable_version(&snapshot.templates_version) {
        return super::scan_snapshot_with(opts, snapshot, reg);
    }
    // Keep only the last server's package, including failures. File and node checks still run
    // on every snapshot; an explicit scan can retry immediately after setup changes.
    static CACHE: Mutex<Option<Cached>> = Mutex::new(None);
    let mut cache = CACHE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if cache.as_ref().is_none_or(|c| c.host != opts.host || c.version != snapshot.templates_version || c.checked_at.elapsed() >= Duration::from_secs(300)) {
        let templates = Templates::fetch(&ComfyClient::new(&opts.host), &snapshot.templates_version, opts.cache_dir.as_deref()).map_err(|e| format!("{e:#}"));
        *cache = Some(Cached { host: opts.host.clone(), version: snapshot.templates_version.clone(), checked_at: Instant::now(), templates });
    }
    let cached = cache.as_ref().expect("the background template result was fetched for this server and version");
    super::scan_templates_with(opts, snapshot, reg, cached.templates.as_ref().map_err(|e| anyhow::anyhow!("{e}")))
}
