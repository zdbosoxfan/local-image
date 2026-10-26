//! Read-only release discovery. No photos, identifiers, credentials or asset downloads.
use std::cmp::Ordering;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

const API: &str = "https://api.github.com/repos/zdbosoxfan/local-image/releases?per_page=100";
const RELEASE_PREFIX: &str = "https://github.com/zdbosoxfan/local-image/releases/tag/";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Update {
    pub version: String,
    pub url: String,
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
}

/// Match numeric SemVer tags only; release names and publication order are not versions.
/// Stable builds stay on stable releases; alpha/beta builds may see either channel.
#[derive(Debug, PartialEq, Eq)]
struct Version {
    core: [u64; 3],
    pre: Vec<String>,
}
impl Version {
    fn parse(input: &str) -> Option<Self> {
        let input = input.strip_prefix('v').unwrap_or(input);
        let (base, build) = input.split_once('+').map_or((input, None), |(a, b)| (a, Some(b)));
        let valid = |s: &str| !s.is_empty() && s.split('.').all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'));
        if build.is_some_and(|s| !valid(s)) {
            return None;
        }
        let (core, pre) = base.split_once('-').map_or((base, ""), |(a, b)| (a, b));
        if base.contains('-') && !valid(pre) {
            return None;
        }
        let number = |s: &str| {
            if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) || (s.len() > 1 && s.starts_with('0')) { None } else { s.parse().ok() }
        };
        let core: Vec<_> = core.split('.').map(number).collect::<Option<_>>()?;
        let core: [u64; 3] = core.try_into().ok()?;
        let pre: Vec<String> = if pre.is_empty() { Vec::new() } else { pre.split('.').map(str::to_owned).collect() };
        if pre.iter().any(|p| p.bytes().all(|b| b.is_ascii_digit()) && p.len() > 1 && p.starts_with('0')) {
            return None;
        }
        Some(Self { core, pre })
    }
    fn cmp(&self, other: &Self) -> Ordering {
        let core = self.core.cmp(&other.core);
        if core != Ordering::Equal {
            return core;
        }
        match (self.pre.is_empty(), other.pre.is_empty()) {
            (true, false) => return Ordering::Greater,
            (false, true) => return Ordering::Less,
            _ => {}
        }
        for (a, b) in self.pre.iter().zip(&other.pre) {
            let numeric = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
            let order = match (numeric(a), numeric(b)) {
                (true, true) => a.len().cmp(&b.len()).then_with(|| a.cmp(b)),
                (true, false) => Ordering::Less,
                (false, true) => Ordering::Greater,
                _ => a.cmp(b),
            };
            if order != Ordering::Equal {
                return order;
            }
        }
        self.pre.len().cmp(&other.pre.len())
    }
}

pub fn from_json(current: &str, json: &str) -> Result<Option<Update>> {
    let current = Version::parse(current).context("This build has no comparable release version")?;
    let releases: Vec<Release> = serde_json::from_str(json).context("Invalid GitHub releases response")?;
    let mut newest: Option<(Version, Update)> = None;
    for release in releases {
        let Some(version) = Version::parse(&release.tag_name) else { continue };
        if release.draft || (current.pre.is_empty() && (release.prerelease || !version.pre.is_empty())) || version.cmp(&current) != Ordering::Greater {
            continue;
        }
        // Only the project's release page is ever offered to the browser. Reject injected
        // destinations and URL syntax; use GitHub's canonical URL only when it matches the tag.
        if release.html_url != format!("{RELEASE_PREFIX}{}", release.tag_name) {
            continue;
        }
        if newest.as_ref().is_none_or(|(best, _)| version.cmp(best) == Ordering::Greater) {
            newest = Some((version, Update { version: release.tag_name, url: release.html_url }));
        }
    }
    Ok(newest.map(|(_, update)| update))
}

/// Blocking: invoke only from a worker. Uses ring under the 2026-10-25 TLS security exception
/// for C/assembly crypto and the bundled roots.
pub fn check(current: &str) -> Result<Option<Update>> {
    let agent: ureq::Agent =
        ureq::Agent::config_builder().tls_config(crate::tls::config()).timeout_global(Some(Duration::from_secs(15))).max_redirects(0).build().into();
    fetch(&agent, current, API)
}

fn fetch(agent: &ureq::Agent, current: &str, endpoint: &str) -> Result<Option<Update>> {
    let mut response = agent
        .get(endpoint)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("User-Agent", "Local-Image-update-check")
        .call()
        .context("Could not reach GitHub releases")?;
    if response.status() != 200 {
        bail!("GitHub returned {}", response.status());
    }
    let body = response.body_mut().with_config().limit(2 * 1024 * 1024).read_to_string()?;
    from_json(current, &body)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn release(tag: &str, pre: bool, draft: bool) -> serde_json::Value {
        serde_json::json!({"tag_name": tag, "html_url": format!("{RELEASE_PREFIX}{tag}"), "prerelease": pre, "draft": draft})
    }
    #[test]
    fn release_updates_select_versions_and_channels_without_downgrades() {
        let json = serde_json::json!([
            release("v0.7.0", false, false),
            release("v2.0.0-alpha.10", true, false),
            release("v2.0.0-alpha.2", true, false),
            release("v9.0.0", false, true)
        ])
        .to_string();
        assert_eq!(from_json("2.0.0-alpha.1", &json).unwrap().unwrap().version, "v2.0.0-alpha.10");
        assert!(from_json("2.0.0", &json).unwrap().is_none());
        assert!(from_json("2.0.0-alpha.10+build", &json).unwrap().is_none());
        assert_eq!(from_json("1.0.0", &serde_json::json!([release("v2.0.0", false, false)]).to_string()).unwrap().unwrap().version, "v2.0.0");
    }
    #[test]
    fn release_updates_reject_invalid_json_tags_and_foreign_urls() {
        assert!(from_json("2.0.0", "{}").is_err());
        assert!(from_json("2.0.0", "rate limited").is_err());
        let mut wrong = release("v3.0.0", false, false);
        wrong["html_url"] = serde_json::json!("https://evil.example/file.exe");
        assert!(from_json("2.0.0", &serde_json::json!([wrong, release("nightly", false, false)]).to_string()).unwrap().is_none());
        for invalid in ["2.0", "2.0.0-", "2.0.0-a..1", "2.0.0-01", "02.0.0", "2.0.0+", "2.0.0-a/b"] {
            assert!(Version::parse(invalid).is_none(), "{invalid}");
        }
    }
    #[test]
    fn release_updates_follow_semver_precedence() {
        let tags = ["1.0.0-alpha", "1.0.0-alpha.1", "1.0.0-alpha.beta", "1.0.0-beta", "1.0.0-beta.2", "1.0.0-beta.11", "1.0.0-rc.1", "1.0.0", "1.0.1"];
        for pair in tags.windows(2) {
            assert_eq!(Version::parse(pair[0]).unwrap().cmp(&Version::parse(pair[1]).unwrap()), Ordering::Less);
        }
    }
    #[test]
    fn release_updates_http_contract_and_error_responses_are_bounded() {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let url = format!("http://{}/releases", server.server_addr());
        let server_thread = std::thread::spawn(move || {
            for (status, body) in [(200, "[]".to_string()), (429, "rate limit".into()), (200, "not JSON".into()), (200, "x".repeat(2 * 1024 * 1024 + 1))] {
                let request = server.recv_timeout(Duration::from_secs(10)).unwrap().unwrap();
                assert_eq!(request.method(), &tiny_http::Method::Get);
                assert_eq!(request.url(), "/releases");
                assert!(request.headers().iter().any(|h| h.field.equiv("User-Agent") && h.value.as_str() == "Local-Image-update-check"));
                assert!(!request.headers().iter().any(|h| h.field.equiv("Authorization")));
                request.respond(tiny_http::Response::from_string(body).with_status_code(status)).unwrap();
            }
        });
        let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(5))).build().into();
        assert!(fetch(&agent, "2.0.0", &url).unwrap().is_none());
        for _ in 0..3 {
            assert!(fetch(&agent, "2.0.0", &url).is_err());
        }
        server_thread.join().unwrap();
    }
}
