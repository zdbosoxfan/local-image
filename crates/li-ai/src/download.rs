//! Verified downloads of model and LoRA files: HTTPS from an allow-list of hosts (checked on every
//! redirect), streamed to a `.part` file with the exact byte count and SHA-256 enforced, then
//! renamed into place. An existing file is accepted only if its size and hash match; it is never
//! overwritten.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};

use crate::catalog::{FileSpec, Preset};
use crate::comfy::{Cancelled, JobControl};

const ALLOWED_HOSTS: &[&str] = &[
    "github.com",
    "release-assets.githubusercontent.com",
    "objects.githubusercontent.com",
    "huggingface.co",
    "cdn-lfs.huggingface.co",
    "cdn-lfs.hf.co",
    "us.aws.cdn.hf.co",
    "civitai.com",
    "civitai.red",
];

/// Host suffixes of the CDNs the allowed hosts redirect to (Hugging Face's Xet and LFS CDNs,
/// Civitai's delivery buckets on Cloudflare R2 and Backblaze B2).
const ALLOWED_SUFFIXES: &[&str] = &[".hf.co", ".huggingface.co", ".r2.cloudflarestorage.com", ".backblazeb2.com"];

fn host_of(url: &str) -> Option<String> {
    let rest = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://"))?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let authority = authority.rsplit('@').next().unwrap_or(authority);
    Some(authority.to_ascii_lowercase())
}

static TEST_HOSTS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// Allows plain-HTTP downloads from one `host:port` (the in-process mock server).
#[doc(hidden)]
pub fn allow_test_host(authority: &str) {
    if let Ok(mut v) = TEST_HOSTS.lock() {
        v.push(authority.to_ascii_lowercase());
    }
}

/// Whether a download may come from `url`: HTTPS to an allow-listed host, or plain HTTP to the
/// exact `host:port` of a mock server (in-process, or named in `LOCAL_IMAGE_TEST_DOWNLOAD_HOST`
/// for demos against `examples/mock_comfy`).
pub fn host_allowed(url: &str) -> bool {
    let Some(authority) = host_of(url) else { return false };
    if url.starts_with("http://") {
        return TEST_HOSTS.lock().is_ok_and(|v| v.contains(&authority))
            || std::env::var("LOCAL_IMAGE_TEST_DOWNLOAD_HOST").is_ok_and(|t| !t.is_empty() && t.eq_ignore_ascii_case(&authority));
    }
    let host = authority.split(':').next().unwrap_or("");
    ALLOWED_HOSTS.contains(&host) || ALLOWED_SUFFIXES.iter().any(|s| host.ends_with(s))
}

fn download_agent(timeout_body: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(20)))
        .timeout_recv_body(Some(timeout_body))
        .max_redirects(0)
        .http_status_as_error(false)
        .user_agent(concat!("LocalImage/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

fn resolve_redirect(url: &str, loc: &str) -> Result<String> {
    if loc.starts_with("https://") || loc.starts_with("http://") {
        Ok(loc.to_owned())
    } else if loc.starts_with('/') {
        let origin: String = url.splitn(4, '/').take(3).collect::<Vec<_>>().join("/");
        Ok(format!("{origin}{loc}"))
    } else {
        bail!("Unexpected redirect: {loc}")
    }
}

/// GETs `url` following redirects (each checked against the allow-list). The token is sent only
/// to the first URL's own host, never to the CDN a redirect points at.
fn open(agent: &ureq::Agent, url: &str, token: Option<&str>, head: bool) -> Result<(String, ureq::http::Response<ureq::Body>)> {
    let first = host_of(url);
    let mut url = url.to_owned();
    for _ in 0..8 {
        if !host_allowed(&url) {
            bail!("Refusing to download from an unexpected host: {url}");
        }
        let auth = token.filter(|t| !t.is_empty() && host_of(&url) == first).map(|t| format!("Bearer {t}"));
        let r = if head {
            let mut req = agent.head(&url);
            if let Some(a) = &auth {
                req = req.header("Authorization", a);
            }
            req.call()?
        } else {
            let mut req = agent.get(&url);
            if let Some(a) = &auth {
                req = req.header("Authorization", a);
            }
            req.call()?
        };
        if r.status().is_redirection() {
            let loc = r.headers().get("location").and_then(|v| v.to_str().ok()).context("redirect without a location")?;
            url = resolve_redirect(&url, loc)?;
            continue;
        }
        if !r.status().is_success() {
            bail!("Download failed: HTTP {} for {url}", r.status().as_u16());
        }
        return Ok((url, r));
    }
    bail!("too many redirects")
}

/// The size of a download (`Content-Length` of the final response after redirects).
pub fn remote_size(url: &str, token: Option<&str>) -> Result<Option<u64>> {
    let (_, r) = open(&download_agent(Duration::from_secs(30)), url, token, true)?;
    Ok(r.headers().get("content-length").and_then(|v| v.to_str().ok()).and_then(|v| v.parse().ok()))
}

/// Progress of a multi-file download.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DownloadProgress {
    pub file: String,
    pub done_bytes: u64,
    pub total_bytes: u64,
}

/// Size and SHA-256 of a file on disk.
pub fn hash_file(path: &Path) -> Result<(u64, String)> {
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    let mut n = 0u64;
    loop {
        let k = f.read(&mut buf)?;
        if k == 0 {
            break;
        }
        h.update(&buf[..k]);
        n += k as u64;
    }
    Ok((n, hex::encode(h.finalize())))
}

/// True when `path` holds an accepted copy of `spec` (checked by size first, then hash).
pub fn verify_existing(path: &Path, spec: &FileSpec) -> Result<bool> {
    let Ok(meta) = std::fs::metadata(path) else { return Ok(false) };
    let accepted: Vec<(u64, &str)> = std::iter::once((spec.bytes, spec.sha256.as_str())).chain(spec.compatible.iter().map(|(b, s)| (*b, s.as_str()))).collect();
    if !accepted.iter().any(|(b, _)| *b == meta.len()) {
        bail!("{} exists but is not the expected file (size differs). Move it aside to download a fresh copy.", path.display());
    }
    let (_, sha) = hash_file(path)?;
    if accepted.iter().any(|(b, s)| *b == meta.len() && s.eq_ignore_ascii_case(&sha)) {
        Ok(true)
    } else {
        bail!("{} exists but its checksum does not match. Move it aside to download a fresh copy.", path.display())
    }
}

pub fn target_path(model_dir: &Path, spec: &FileSpec) -> PathBuf {
    model_dir.join(&spec.folder).join(&spec.name)
}

/// Which files of a preset are missing from `model_dir` (by name only; hashing is done on
/// download).
pub fn missing_files<'a>(model_dir: &Path, preset: &'a Preset) -> Vec<&'a FileSpec> {
    preset.files.iter().filter(|f| !target_path(model_dir, f).exists()).collect()
}

/// Downloads every missing file of `preset` into `model_dir`.
pub fn download_preset(model_dir: &Path, preset: &Preset, ctl: &JobControl, on_progress: &dyn Fn(DownloadProgress)) -> Result<()> {
    let total: u64 = preset.files.iter().map(|f| f.bytes).sum();
    let mut done = 0u64;
    for spec in &preset.files {
        let path = target_path(model_dir, spec);
        if path.exists() {
            ctl.set_message(format!("Checking {}", spec.name));
            verify_existing(&path, spec)?;
            done += spec.bytes;
            on_progress(DownloadProgress { file: spec.name.clone(), done_bytes: done, total_bytes: total });
            continue;
        }
        let base = done;
        download_file(&spec.url, &path, spec.bytes, &spec.sha256, ctl, &|n| {
            on_progress(DownloadProgress { file: spec.name.clone(), done_bytes: base + n, total_bytes: total })
        })
        .map_err(|e| match &preset.access_url {
            Some(url) if e.to_string().contains("401") || e.to_string().contains("403") => {
                anyhow::anyhow!("The publisher requires you to accept its licence before downloading. Request access at {url}, then try again.")
            }
            _ => e,
        })?;
        done += spec.bytes;
    }
    Ok(())
}

/// Downloads one file with size and SHA-256 enforcement.
pub fn download_file(url: &str, dest: &Path, bytes: u64, sha256: &str, ctl: &JobControl, on_bytes: &dyn Fn(u64)) -> Result<()> {
    download_file_with(url, dest, bytes, sha256, None, ctl, on_bytes)
}

/// [`download_file`] with an access token for the first host (Hugging Face, Civitai).
pub fn download_file_with(url: &str, dest: &Path, bytes: u64, sha256: &str, token: Option<&str>, ctl: &JobControl, on_bytes: &dyn Fn(u64)) -> Result<()> {
    if dest.exists() {
        bail!("{} already exists", dest.display());
    }
    let dir = dest.parent().context("destination has no folder")?;
    std::fs::create_dir_all(dir)?;
    if let Some(free) = free_space(dir)
        && free < bytes + (256 << 20)
    {
        bail!("Not enough disk space in {}: {} needed.", dir.display(), human_bytes(bytes + (256 << 20)));
    }
    let (_, mut r) = open(&download_agent(Duration::from_secs(120)), url, token, false)?;
    let part = dir.join(format!(".{}.local-image-{}.part", dest.file_name().and_then(|n| n.to_str()).unwrap_or("file"), uuid::Uuid::new_v4().simple()));
    let result = (|| -> Result<()> {
        let mut out = std::fs::File::create(&part)?;
        let mut reader = r.body_mut().with_config().limit(bytes + 1).reader();
        let mut h = Sha256::new();
        let mut buf = vec![0u8; 1 << 20];
        let mut n = 0u64;
        loop {
            if ctl.is_cancelled() {
                return Err(anyhow::anyhow!(Cancelled));
            }
            let k = reader.read(&mut buf)?;
            if k == 0 {
                break;
            }
            n += k as u64;
            if n > bytes {
                bail!("The download is larger than expected; it was discarded.");
            }
            h.update(&buf[..k]);
            out.write_all(&buf[..k])?;
            on_bytes(n);
        }
        if n != bytes {
            bail!("The download ended early ({} of {}). Try again.", human_bytes(n), human_bytes(bytes));
        }
        let got = hex::encode(h.finalize());
        if !got.eq_ignore_ascii_case(sha256) {
            bail!("The downloaded file's checksum does not match; it was discarded.");
        }
        out.sync_all()?;
        if dest.exists() {
            bail!("{} appeared while downloading", dest.display());
        }
        std::fs::rename(&part, dest)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    result
}

/// Free bytes on the volume holding `dir`, when the platform can tell.
pub fn free_space(dir: &Path) -> Option<u64> {
    #[cfg(unix)]
    {
        let out = std::process::Command::new("df").arg("-Pk").arg(dir).output().ok()?;
        let text = String::from_utf8_lossy(&out.stdout);
        let line = text.lines().nth(1)?;
        let avail: u64 = line.split_whitespace().nth(3)?.parse().ok()?;
        Some(avail * 1024)
    }
    #[cfg(not(unix))]
    {
        let _ = dir;
        None
    }
}

pub fn human_bytes(n: u64) -> String {
    let f = n as f64;
    if f >= 1e9 {
        format!("{:.1} GB", f / 1e9)
    } else if f >= 1e6 {
        format!("{:.0} MB", f / 1e6)
    } else if f >= 1e3 {
        format!("{:.0} KB", f / 1e3)
    } else {
        format!("{n} bytes")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_allow_list() {
        assert!(host_allowed("https://huggingface.co/x/resolve/abc/f.safetensors"));
        assert!(host_allowed("https://cas-bridge.xethub.hf.co/x"));
        assert!(!host_allowed("http://huggingface.co/x"));
        assert!(!host_allowed("https://huggingface.co.evil.com/x"));
        assert!(!host_allowed("https://example.com/x"));
    }

    #[test]
    fn existing_files_are_verified() {
        let dir = std::env::temp_dir().join(format!("li-dl-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("f.bin");
        std::fs::write(&p, b"hello").unwrap();
        let sha = hex::encode(Sha256::digest(b"hello"));
        let spec = FileSpec {
            role: crate::catalog::Role::Vae,
            folder: "vae".into(),
            name: "f.bin".into(),
            bytes: 5,
            sha256: sha,
            url: "https://huggingface.co/f".into(),
            compatible: Vec::new(),
        };
        assert!(verify_existing(&p, &spec).unwrap());
        let wrong = FileSpec { bytes: 6, ..spec.clone() };
        assert!(verify_existing(&p, &wrong).is_err());
        std::fs::remove_dir_all(dir).ok();
        assert_eq!(human_bytes(7_256_783_064), "7.3 GB");
    }
}
