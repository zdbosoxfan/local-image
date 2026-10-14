//! Downloading a model's files from an ordered list of mirrors, only when the user asked for it,
//! through the app's one verified downloader.
//!
//! The downloader is [`li_ai::download`]; what this module adds is only what `li-ai` does not do itself: the list of mirrors (the next one
//! when a mirror fails, see [`mirrors`]), tries per mirror, progress over several files, and an
//! `AtomicBool` cancel flag. Every byte goes through `li-ai`: HTTPS to its host allow-list
//! (checked on every redirect), the exact size and SHA-256 **required**, a `.part` file that is
//! removed on any failure, the disk-space check, and a verified file linked into place without
//! replacing anything.
//!
//! In practice:
//! - a file without a pinned size and SHA-256 is never downloaded ([`DownloadError::Unpinned`]):
//!   the user puts it in the model folder;
//! - a mirror must be on `li-ai`'s allow-list (Hugging Face, GitHub, …);
//! - an interrupted download starts over (no resume) and a stalled connection ends at `li-ai`'s
//!   whole-body budget; cancelling stops it at once.
//!
//! Runs on whatever thread calls [`download`] (the engine uses a background thread). Native only
//! (the web build downloads no models).

use std::cell::RefCell;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use li_ai::JobControl;
use lightcraft_sysmem::jobs::Cancelled;

/// One file of the model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileSpec {
    pub name: &'static str,
    /// Exact size in bytes, when pinned (required to download it).
    pub size: Option<u64>,
    /// SHA-256 (lower-case hex), when pinned (required to download it).
    pub sha256: Option<&'static str>,
    /// Largest size accepted for a file the user put in place when `size` is not pinned.
    pub max: u64,
}

/// Most mirrors used (a hostile list can't make a download loop forever).
const MAX_MIRRORS: usize = 16;

/// Whether `s` can be a mirror's base URL: `http(s)://host…`, no spaces, no user name.
fn usable_base(s: &str) -> bool {
    let Some(rest) = s.strip_prefix("https://").or_else(|| s.strip_prefix("http://")) else { return false };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    s.len() <= 16 * 1024 && !authority.is_empty() && !authority.contains('@') && !s.chars().any(|c| c.is_control() || c.is_whitespace())
}

/// The mirrors to try, in order: `env`'s (base URLs separated by commas, spaces or newlines), then
/// the mirrors file's (one base URL per line, `#` comments), then `defaults`; duplicates and
/// unusable URLs dropped, at most 16.
pub fn mirrors(env: Option<&str>, file: Option<&Path>, defaults: &[&str]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut add = |s: &str| {
        let s = s.trim().trim_end_matches('/');
        if usable_base(s) && !out.iter().any(|m| m == s) && out.len() < MAX_MIRRORS {
            out.push(s.to_string());
        }
    };
    if let Some(env) = env {
        env.split([',', ' ', '\n', '\t']).for_each(&mut add);
    }
    // a small text file; anything larger isn't a mirror list
    if let Some(text) = file.and_then(|f| std::fs::metadata(f).ok().filter(|m| m.len() < 64 * 1024).and_then(|_| std::fs::read_to_string(f).ok())) {
        text.lines().map(|l| l.split('#').next().unwrap_or_default()).for_each(&mut add);
    }
    defaults.iter().for_each(|m| add(m));
    out
}

/// Download settings (connect and read timeouts are `li-ai`'s).
#[derive(Clone, Debug)]
pub struct Options {
    /// Tries per mirror and file for transient failures (each starts the file over).
    pub attempts: u32,
    /// The environment variable holding a bearer token for the first mirror's host (sent only over
    /// https, never to a host a redirect leads to). `None`: nothing is ever sent, so one model's
    /// token cannot leak to another's server (upstream LightCraft 4b57d5d).
    pub token_env: Option<&'static str>,
}

impl Default for Options {
    fn default() -> Self {
        Options { attempts: 3, token_env: None }
    }
}

/// Where a download is.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Progress {
    pub file: String,
    /// Bytes of all files on disk so far, and the total expected (pinned sizes).
    pub done: u64,
    pub total: u64,
    /// The mirror in use (without query).
    pub mirror: String,
}

/// Why a download failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownloadError {
    /// No mirror is configured.
    NoMirrors,
    Cancelled,
    /// A local file problem (folder not writable, disk full…).
    Disk(String),
    /// Every mirror failed for `file`; the reason for each.
    AllMirrorsFailed {
        file: String,
        errors: Vec<String>,
    },
    /// `file` is missing and has no pinned size and SHA-256, so it is never downloaded.
    Unpinned(String),
}

impl std::fmt::Display for DownloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DownloadError::NoMirrors => write!(f, "no download location is configured for this model in this build"),
            DownloadError::Cancelled => write!(f, "the download was cancelled"),
            DownloadError::Disk(e) => write!(f, "{e}"),
            DownloadError::AllMirrorsFailed { file, errors } => write!(f, "could not download {file}: {}", errors.join("; ")),
            DownloadError::Unpinned(file) => {
                write!(f, "{file} has no pinned size and SHA-256, so Local Image does not download it: put it in the model folder yourself")
            }
        }
    }
}

/// The bearer token to send for `url`: only a non-empty one, only over https. (`li-ai` sends it
/// only to the first URL's host, never to the host a redirect leads to.)
fn bearer<'a>(token: Option<&'a str>, url: &str) -> Option<&'a str> {
    token.map(str::trim).filter(|t| !t.is_empty() && url.starts_with("https://"))
}

/// A mirror for messages (no query string: it may hold a token).
fn short(m: &str) -> String {
    m.split(['?', '#']).next().unwrap_or_default().to_string()
}

/// Whether `path` is already the right file. A pinned file with the wrong size or hash is deleted
/// (never kept, never mistaken for the model); an unpinned one is accepted when non-empty and
/// within `max`.
fn verified(f: &FileSpec, path: &Path) -> Result<bool, DownloadError> {
    let Ok(meta) = std::fs::metadata(path) else { return Ok(false) };
    if !meta.is_file() {
        return Ok(false);
    }
    let ok = match (f.size, f.sha256) {
        (Some(size), Some(sha)) => {
            meta.len() == size
                && li_ai::download::hash_file(path).map_err(|e| DownloadError::Disk(format!("{}: {e}", path.display())))?.1.eq_ignore_ascii_case(sha)
        }
        _ => meta.len() > 0 && meta.len() <= f.max && f.size.is_none_or(|s| s == meta.len()),
    };
    if !ok && f.size.is_some() && f.sha256.is_some() {
        std::fs::remove_file(path).map_err(|e| DownloadError::Disk(format!("{}: {e}", path.display())))?;
    }
    Ok(ok)
}

/// How one failed try ends.
enum Fail {
    Retry(String),
    NextMirror(String),
    Disk(String),
}

/// Sorts a `li-ai` download error: problems that another try at the same mirror cannot fix move
/// on to the next mirror; a full disk stops the download.
fn classify(msg: String) -> Fail {
    if msg.contains("Not enough disk space") {
        Fail::Disk(msg)
    } else if ["Refusing to download", "HTTP 4", "checksum", "larger than expected", "user-installed only", "already exists", "redirect"]
        .iter()
        .any(|k| msg.contains(k))
    {
        Fail::NextMirror(msg)
    } else {
        Fail::Retry(msg)
    }
}

/// Download `files` into `dir` from `mirrors` (in order; see [`mirrors`]). Files already there
/// and verified are kept. `progress` is called as bytes arrive (often: throttle in it).
pub fn download(
    files: &[FileSpec],
    mirrors: &[String],
    dir: &Path,
    opts: &Options,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(&Progress),
) -> Result<(), DownloadError> {
    std::fs::create_dir_all(dir).map_err(|e| DownloadError::Disk(format!("can't create {}: {e}", dir.display())))?;
    // what's left to fetch
    let mut todo = Vec::new();
    for f in files {
        if !verified(f, &dir.join(f.name))? {
            todo.push(f);
        }
    }
    if todo.is_empty() {
        return Ok(());
    }
    if mirrors.is_empty() {
        return Err(DownloadError::NoMirrors);
    }
    if let Some(f) = todo.iter().find(|f| f.size.is_none() || f.sha256.is_none()) {
        return Err(DownloadError::Unpinned(f.name.to_string()));
    }
    let total: u64 = files.iter().filter_map(|f| f.size).sum();
    let mut before: u64 = files.iter().filter(|f| !todo.contains(f)).filter_map(|f| f.size).sum();
    let token = opts.token_env.and_then(|name| std::env::var(name).ok());
    let ctl = JobControl::new();
    let finished = AtomicBool::new(false);
    let progress = RefCell::new(progress);
    std::thread::scope(|scope| {
        // the caller's flag → `li-ai`'s cancel-aware transport
        scope.spawn(|| {
            while !finished.load(Ordering::SeqCst) {
                if cancel.load(Ordering::SeqCst) {
                    ctl.cancel();
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        });
        let r = (|| {
            for f in todo {
                let (Some(size), Some(sha)) = (f.size, f.sha256) else { return Err(DownloadError::Unpinned(f.name.to_string())) };
                let dest = dir.join(f.name);
                let mut errors = Vec::new();
                let mut ok = false;
                'mirrors: for m in mirrors {
                    let url = format!("{m}/{}", f.name);
                    let mirror = short(m);
                    for _ in 0..opts.attempts.clamp(1, 16) {
                        if cancel.load(Ordering::SeqCst) {
                            return Err(DownloadError::Cancelled);
                        }
                        let report = |n: u64| {
                            let p = Progress { file: f.name.to_string(), done: before.saturating_add(n), total, mirror: mirror.clone() };
                            if let Ok(mut cb) = progress.try_borrow_mut() {
                                cb(&p);
                            }
                        };
                        report(0);
                        match li_ai::download::download_file_with(&url, &dest, size, sha, bearer(token.as_deref(), &url), &ctl, &report) {
                            Ok(()) => {
                                ok = true;
                                break 'mirrors;
                            }
                            Err(e) if e.is::<Cancelled>() || ctl.is_cancelled() || cancel.load(Ordering::SeqCst) => {
                                return Err(DownloadError::Cancelled);
                            }
                            Err(e) => match classify(format!("{e:#}")) {
                                Fail::Disk(e) => return Err(DownloadError::Disk(e)),
                                Fail::Retry(e) => {
                                    log::warn!("download of {}/{}: {e} (retrying)", mirror, f.name);
                                    errors.push(format!("{mirror}: {e}"));
                                }
                                Fail::NextMirror(e) => {
                                    log::warn!("download of {}/{}: {e}", mirror, f.name);
                                    errors.push(format!("{mirror}: {e}"));
                                    continue 'mirrors;
                                }
                            },
                        }
                    }
                }
                if !ok {
                    return Err(DownloadError::AllMirrorsFailed { file: f.name.to_string(), errors });
                }
                before = before.saturating_add(size);
            }
            Ok(())
        })();
        finished.store(true, Ordering::SeqCst);
        r
    })
}

#[cfg(test)]
mod tests;
