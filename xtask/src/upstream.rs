//! The storytold upstreams (PhotoCraft, LightCraft, VectorCraft) and the tools that bring their
//! commits into Local Image.
//!
//! The commands read the clones under `--upstream-dir` (default `target/upstream/<key>`) and never
//! clone or fetch; in a blob-less clone git downloads the file contents it needs on demand.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::attributions::{plain, table_rows};

/// A storytold upstream: `key` is its repository name and the folder of its clone.
#[derive(Debug, PartialEq)]
pub struct Upstream {
    pub key: &'static str,
    pub name: &'static str,
    pub url: &'static str,
    pub licence: &'static str,
}

pub const UPSTREAMS: [Upstream; 3] = [
    Upstream { key: "photocraft", name: "PhotoCraft", url: "https://github.com/storytold/photocraft", licence: "MIT OR Apache-2.0" },
    Upstream { key: "lightcraft", name: "LightCraft", url: "https://github.com/storytold/lightcraft", licence: "MIT OR Apache-2.0" },
    Upstream { key: "vectorcraft", name: "VectorCraft", url: "https://github.com/storytold/vectorcraft", licence: "MIT OR Apache-2.0" },
];

pub fn by_key(key: &str) -> Option<&'static Upstream> {
    UPSTREAMS.iter().find(|u| u.key == key)
}

pub fn is_full_sha(s: &str) -> bool {
    s.len() == 40 && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn git(dir: &Path) -> Command {
    let mut c = Command::new("git");
    c.arg("-C").arg(dir);
    c
}

/// Runs git in `dir` and returns its trimmed stdout; failure is an error carrying git's message.
fn git_out(dir: &Path, args: &[&str]) -> Result<String> {
    let out = git(dir).args(args).output().context("could not run git")?;
    if !out.status.success() {
        bail!("git -C {} {}: {}", dir.display(), args.join(" "), String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// The full hash of `rev` in the clone `dir`.
fn resolve_commit(dir: &Path, rev: &str) -> Result<String> {
    let out = git(dir).args(["rev-parse", "--verify", "--quiet", &format!("{rev}^{{commit}}")]).output().context("could not run git")?;
    if !out.status.success() {
        bail!("{rev} is not a commit in {}: run `git -C {} fetch origin`", dir.display(), dir.display());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Whether `ancestor` is `commit` or one of its ancestors.
fn is_ancestor(dir: &Path, ancestor: &str, commit: &str) -> Result<bool> {
    let out = git(dir).args(["merge-base", "--is-ancestor", ancestor, commit]).output().context("could not run git")?;
    match out.status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => bail!("git merge-base {ancestor} {commit} in {}: {}", dir.display(), String::from_utf8_lossy(&out.stderr).trim()),
    }
}

/// `md` with its 0-based line `index` replaced by `line`, keeping the line ending.
fn replace_line(md: &str, index: usize, line: &str) -> String {
    md.split_inclusive('\n')
        .enumerate()
        .map(|(i, l)| if i == index { format!("{line}{}", &l[l.trim_end_matches(['\r', '\n']).len()..]) } else { l.to_string() })
        .collect()
}

// Review cutoffs

const MARKER_TABLE: [&str; 2] = ["upstream", "reviewed up to"];

/// A row of PORTS.md's review cutoffs: every commit of `upstream` up to and including `sha` has
/// been triaged.
struct Marker {
    upstream: &'static Upstream,
    sha: String,
    line: usize,
}

fn marker_rows(md: &str) -> Result<Vec<Marker>> {
    let mut out: Vec<Marker> = Vec::new();
    for (i, line, cells) in table_rows(md, &MARKER_TABLE) {
        let at = || format!("docs/PORTS.md line {}: {line}", i + 1);
        let [name, sha] = cells.as_slice() else { bail!("{}: expected 2 cells", at()) };
        let name = plain(name);
        let upstream = UPSTREAMS.iter().find(|u| u.name.eq_ignore_ascii_case(&name)).with_context(|| format!("{}: unknown upstream `{name}`", at()))?;
        let sha = plain(sha);
        if !is_full_sha(&sha) {
            bail!("{}: `{sha}` is not a full commit hash", at());
        }
        if out.iter().any(|m| m.upstream == upstream) {
            bail!("{}: a second cutoff for {}", at(), upstream.name);
        }
        out.push(Marker { upstream, sha, line: i });
    }
    Ok(out)
}

/// Upstream key → the commit it has been reviewed up to, from PORTS.md.
pub fn review_markers(md: &str) -> Result<BTreeMap<&'static str, String>> {
    Ok(marker_rows(md)?.into_iter().map(|m| (m.upstream.key, m.sha)).collect())
}

/// `cargo xtask upstream-reviewed <repo> <sha>`: moves the repo's cutoff forward to `rev`.
pub fn upstream_reviewed(root: &Path, key: &str, rev: &str, upstream_dir: &Path) -> Result<()> {
    let upstream = by_key(key).with_context(|| format!("unknown upstream `{key}`"))?;
    let clone = upstream_dir.join(key);
    let new = resolve_commit(&clone, rev)?;
    let path = root.join("docs/PORTS.md");
    let md = std::fs::read_to_string(&path).context("docs/PORTS.md")?;
    let markers = marker_rows(&md)?;
    let marker = markers.iter().find(|m| m.upstream == upstream).with_context(|| format!("docs/PORTS.md has no review cutoff for {}", upstream.name))?;
    if marker.sha == new {
        println!("{} is already reviewed up to {new}", upstream.name);
        return Ok(());
    }
    if !is_ancestor(&clone, &marker.sha, &new)? {
        bail!("{new} does not descend from the current cutoff {}: a cutoff only moves forward", marker.sha);
    }
    std::fs::write(&path, replace_line(&md, marker.line, &format!("| {} | `{new}` |", upstream.name)))?;
    println!("{} reviewed up to {new} (was {})", upstream.name, marker.sha);
    Ok(())
}

/// One line per review cutoff: how many commits `origin/HEAD` of the cached clone has past it.
pub fn commits_since_markers(md: &str, cache: &Path) -> Result<Vec<String>> {
    Ok(review_markers(md)?
        .into_iter()
        .map(|(key, sha)| {
            let name = by_key(key).map_or(key, |u| u.name);
            let short = &sha[..12];
            match git_out(&cache.join(key), &["rev-list", "--count", &format!("{sha}..origin/HEAD")]) {
                Ok(n) => format!("{name}: {n} commit(s) since reviewed-up-to {short}"),
                Err(e) => format!("! {name}: cannot count commits since reviewed-up-to {short}: {e}"),
            }
        })
        .collect())
}

// Path map

pub const PATH_MAP: &str = "docs/upstream-paths.toml";

/// One upstream's table in docs/upstream-paths.toml (its header comment explains the entries).
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct PathMap {
    #[serde(default)]
    map: BTreeMap<String, String>,
    #[serde(default)]
    moved: BTreeMap<String, Moved>,
    #[serde(default)]
    unmapped: BTreeMap<String, String>,
    #[serde(default)]
    skip: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Moved {
    ours: String,
    to: String,
    at: String,
}

/// What happens to an upstream path.
#[derive(Debug, PartialEq)]
enum Route {
    To(String),
    Skip,
    Refuse(String),
}

enum Rule<'a> {
    Map(&'a str),
    Moved(&'a Moved),
    Unmapped(&'a str),
    Skip,
}

impl PathMap {
    fn rules(&self) -> impl Iterator<Item = (&str, Rule<'_>)> {
        let map = self.map.iter().map(|(p, ours)| (p.as_str(), Rule::Map(ours)));
        let moved = self.moved.iter().map(|(p, m)| (p.as_str(), Rule::Moved(m)));
        let unmapped = self.unmapped.iter().map(|(p, why)| (p.as_str(), Rule::Unmapped(why)));
        map.chain(moved).chain(unmapped).chain(self.skip.iter().map(|p| (p.as_str(), Rule::Skip)))
    }

    /// The longest prefix rule for `path`. `moved_since(at)` says whether the commit being ported
    /// is `at` or a descendant of it.
    fn route(&self, path: &str, moved_since: &dyn Fn(&str) -> Result<bool>) -> Result<Route> {
        let matches = |prefix: &str| path == prefix || (prefix.ends_with('/') && path.starts_with(prefix));
        let best = self.rules().filter(|(p, _)| matches(p)).max_by_key(|(p, _)| p.len());
        Ok(match best {
            None => Route::Refuse(format!("not in {PATH_MAP}; add it there")),
            Some((p, Rule::Map(ours))) => Route::To(format!("{ours}{}", &path[p.len()..])),
            Some((p, Rule::Moved(m))) => {
                if moved_since(&m.at)? {
                    Route::Refuse(format!("moved to {} in {}; port by hand into {}", m.to, m.at, m.ours))
                } else {
                    Route::To(format!("{}{}", m.ours, &path[p.len()..]))
                }
            }
            Some((_, Rule::Unmapped(why))) => Route::Refuse(why.to_string()),
            Some((_, Rule::Skip)) => Route::Skip,
        })
    }
}

/// Upstream key → its path map.
fn load_path_maps(root: &Path) -> Result<BTreeMap<String, PathMap>> {
    let maps: BTreeMap<String, PathMap> = toml::from_str(&std::fs::read_to_string(root.join(PATH_MAP)).context(PATH_MAP)?).context(PATH_MAP)?;
    for (key, map) in &maps {
        if by_key(key).is_none() {
            bail!("{PATH_MAP}: unknown upstream `{key}`");
        }
        let mut prefixes: Vec<&str> = map.rules().map(|(p, _)| p).collect();
        prefixes.sort_unstable();
        if let Some(w) = prefixes.windows(2).find(|w| w[0] == w[1]) {
            bail!("{PATH_MAP}: [{key}] lists `{}` twice", w[0]);
        }
    }
    Ok(maps)
}

#[cfg(test)]
pub(crate) mod test_support {
    use std::path::{Path, PathBuf};
    use std::process::Command;

    /// A fresh directory under the system temp dir.
    pub fn temp_dir(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("li-{name}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Runs git in `dir` as a fixed fixture identity and returns its trimmed stdout.
    pub fn git(dir: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid", "-c", "commit.gpgsign=false", "-c", "init.defaultBranch=main"])
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8(out.stdout).unwrap().trim().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::{git, temp_dir};
    use super::*;

    #[test]
    fn path_map_resolves_longest_prefix() -> Result<()> {
        let map: PathMap = toml::from_str(
            r#"
skip = ["docs/"]
[map]
"crates/" = "ours/"
"crates/algo/" = "crates/pc-algo/"
[moved]
"crates/cms/" = { ours = "crates/pc-cms/", to = "craft-libs craft-cms", at = "abc1234" }
[unmapped]
"crates/algo/vendor/" = "vendored"
"Cargo.toml" = "workspace dependencies"
"#,
        )?;
        let before = |at: &str| -> Result<bool> {
            assert_eq!(at, "abc1234");
            Ok(false)
        };
        let after = |_: &str| -> Result<bool> { Ok(true) };
        let route = |path: &str| map.route(path, &before).unwrap();
        assert_eq!(route("crates/algo/src/blur.rs"), Route::To("crates/pc-algo/src/blur.rs".into()));
        assert_eq!(route("crates/other/a.rs"), Route::To("ours/other/a.rs".into()));
        assert_eq!(route("crates/algo/vendor/x.c"), Route::Refuse("vendored".into()));
        assert_eq!(route("Cargo.toml"), Route::Refuse("workspace dependencies".into()));
        assert_eq!(route("docs/a.md"), Route::Skip);
        assert_eq!(route("Cargo.toml.orig"), Route::Refuse(format!("not in {PATH_MAP}; add it there")));
        assert_eq!(route("crates/cms/src/a.rs"), Route::To("crates/pc-cms/src/a.rs".into()));
        assert_eq!(
            map.route("crates/cms/src/a.rs", &after)?,
            Route::Refuse("moved to craft-libs craft-cms in abc1234; port by hand into crates/pc-cms/".into())
        );
        Ok(())
    }

    #[test]
    fn committed_path_map_targets_exist() -> Result<()> {
        let root = crate::root();
        let maps = load_path_maps(&root)?;
        assert_eq!(maps.keys().map(String::as_str).collect::<Vec<_>>(), ["lightcraft", "photocraft", "vectorcraft"]);
        for (key, map) in &maps {
            for ours in map.map.values().chain(map.moved.values().map(|m| &m.ours)) {
                assert!(root.join(ours).is_dir(), "{PATH_MAP} [{key}]: {ours} does not exist");
            }
        }
        Ok(())
    }

    #[test]
    fn review_markers_read_and_advance() -> Result<()> {
        let dir = temp_dir("review-markers");
        let clone = dir.join("upstream/photocraft");
        std::fs::create_dir_all(&clone)?;
        std::fs::create_dir_all(dir.join("ours/docs"))?;
        let commit = |message: &str| {
            std::fs::write(clone.join("a.rs"), message).unwrap();
            git(&clone, &["add", "."]);
            git(&clone, &["commit", "--quiet", "-m", message]);
            git(&clone, &["rev-parse", "HEAD"])
        };
        git(&clone, &["init", "--quiet"]);
        let first = commit("first");
        let second = commit("second");
        git(&clone, &["checkout", "--quiet", "-b", "side", &first]);
        let side = commit("side");
        let lc = "6b2b7e6d1117e683a1701568e56f979d061e4699";
        let md = format!("# Ports\n\n| Upstream | Reviewed up to |\n|---|---|\n| PhotoCraft | `{first}` |\n| LightCraft | `{lc}` |\n\nText.\n");
        let ports = dir.join("ours/docs/PORTS.md");
        std::fs::write(&ports, &md)?;
        let markers = review_markers(&md)?;
        let upstream_dir = dir.join("upstream");
        let forward = upstream_reviewed(&dir.join("ours"), "photocraft", &second[..10], &upstream_dir);
        let advanced = std::fs::read_to_string(&ports)?;
        let backward = upstream_reviewed(&dir.join("ours"), "photocraft", &side, &upstream_dir);
        let unknown = upstream_reviewed(&dir.join("ours"), "photocraft", "0123456789abcdef0123456789abcdef01234567", &upstream_dir);
        let after_refusals = std::fs::read_to_string(&ports)?;
        std::fs::remove_dir_all(&dir)?;

        assert_eq!(markers, BTreeMap::from([("photocraft", first.clone()), ("lightcraft", lc.to_string())]));
        forward?;
        assert_eq!(advanced, md.replace(&format!("| PhotoCraft | `{first}` |"), &format!("| PhotoCraft | `{second}` |")));
        assert!(format!("{:#}", backward.expect_err("a side branch is not a descendant")).contains("only moves forward"));
        assert!(format!("{:#}", unknown.expect_err("an unknown commit")).contains("fetch origin"));
        assert_eq!(after_refusals, advanced);
        let duplicate = format!("| Upstream | Reviewed up to |\n|---|---|\n| PhotoCraft | `{first}` |\n| photocraft | `{second}` |\n");
        assert!(format!("{:#}", review_markers(&duplicate).expect_err("two cutoffs for one upstream")).contains("line 4"));
        Ok(())
    }
}
