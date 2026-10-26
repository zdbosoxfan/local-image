//! Bound background template requests when the server cannot provide a disk-cache key.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::{Options, Scan, Snapshot, Templates};
use crate::family::Registry;

struct Cached {
    host: String,
    version: String,
    checked_at: Instant,
    templates: Result<Templates, String>,
}

// Keep only the last server's package, including failures. File and node checks still run
// on every snapshot; an explicit scan can retry immediately after setup changes.
static CACHE: Mutex<Option<Cached>> = Mutex::new(None);

pub(super) fn scan(opts: &Options, snapshot: Snapshot) -> Scan {
    let reg = crate::family::registry();
    if opts.templates_dir.is_none() && !Templates::cacheable_version(&snapshot.templates_version) {
        let cache = CACHE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(cached) =
            cache.as_ref().filter(|c| c.host == opts.host && c.version == snapshot.templates_version && c.checked_at.elapsed() < Duration::from_secs(300))
        {
            return super::scan_templates_with(opts, snapshot, reg, cached.templates.as_ref().map_err(|e| anyhow::anyhow!("{e}")));
        }
    }
    super::scan_snapshot_with(opts, snapshot, reg)
}

pub(super) fn scan_fetched(opts: &Options, snapshot: Snapshot, reg: &Registry, templates: anyhow::Result<Templates>) -> Scan {
    let mut cache = CACHE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let cached = cache.insert(Cached {
        host: opts.host.clone(),
        version: snapshot.templates_version.clone(),
        checked_at: Instant::now(),
        templates: templates.map_err(|e| format!("{e:#}")),
    });
    super::scan_templates_with(opts, snapshot, reg, cached.templates.as_ref().map_err(|e| anyhow::anyhow!("{e}")))
}
