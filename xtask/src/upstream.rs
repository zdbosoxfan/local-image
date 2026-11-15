//! The storytold upstreams (PhotoCraft, LightCraft, VectorCraft) and the tools that bring their
//! commits into Local Image.
//!
//! The commands read the clones under `--upstream-dir` (default `target/upstream/<key>`) and never
//! clone or fetch; in a blob-less clone git downloads the file contents it needs on demand.

use std::collections::{BTreeMap, HashMap};
use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::attributions::{self, plain, table_rows};

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

/// Runs `cmd` with `input` on its stdin and collects its output.
fn output_with_stdin(cmd: &mut Command, input: Vec<u8>) -> Result<Output> {
    let mut child = cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().context("could not run git")?;
    let mut stdin = child.stdin.take().context("no stdin pipe")?;
    // A separate writer keeps a large input from blocking while git fills its stdout.
    let writer = std::thread::spawn(move || stdin.write_all(&input));
    let out = child.wait_with_output()?;
    let written = writer.join().unwrap_or_else(|panic| std::panic::resume_unwind(panic));
    // A command that failed may have stopped reading; its own error matters more than the pipe's.
    if out.status.success() {
        written?;
    }
    Ok(out)
}

/// The full hash of `rev` in the clone `dir`. A commit the clone lacks stays unknown: without
/// GIT_NO_LAZY_FETCH a blob-less clone would download any full hash the remote can serve.
fn resolve_commit(dir: &Path, rev: &str) -> Result<String> {
    let out = git(dir)
        .env("GIT_NO_LAZY_FETCH", "1")
        .args(["rev-parse", "--verify", "--quiet", &format!("{rev}^{{commit}}")])
        .output()
        .context("could not run git")?;
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

/// Why `sha` cannot be taken from the clone: its main branch does not contain it. The head of an
/// unmerged or rejected pull request is such a commit, and nobody upstream accepted it.
fn off_main(clone: &Path, upstream: &Upstream, sha: &str) -> Result<Option<String>> {
    Ok((!is_ancestor(clone, sha, "origin/HEAD")?).then(|| format!("{sha} is not on {}'s main branch (origin/HEAD of {})", upstream.name, clone.display())))
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
    if let Some(why) = off_main(&clone, upstream, &new)? {
        bail!("{why}");
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

// port

/// What `cargo xtask port` does with the commit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PortMode {
    Apply,
    /// Print the mapping, the PORTS.md row and whether the patch applies; write nothing.
    DryRun,
    /// Only append the PORTS.md row, for a port finished by hand.
    Record,
}

/// How `port` ended; `code` is the process exit status (an error exits 1).
#[derive(Debug, PartialEq)]
pub enum PortStatus {
    Applied,
    /// Nothing was written; the reason why.
    Refused(String),
    Conflicts,
}

impl PortStatus {
    pub fn code(&self) -> i32 {
        match self {
            PortStatus::Applied => 0,
            PortStatus::Refused(_) => 2,
            PortStatus::Conflicts => 3,
        }
    }
}

/// Switches of `cargo xtask port`.
#[derive(Debug, Default)]
pub struct PortOptions {
    /// Let the commit delete files of ours that are still the upstream's files.
    pub allow_delete: bool,
}

/// One file of the upstream commit that maps to a path of ours.
struct Change {
    /// git's status letter: A added, M modified, D deleted, T type changed.
    status: char,
    from: String,
    ours: String,
    /// The upstream's blob before the commit; all zeros for an added file.
    pre: String,
}

/// How our committed file compares with the upstream's before the commit.
#[derive(Debug, PartialEq)]
enum Local {
    Missing,
    Pristine,
    Changed,
}

fn local_state(root: &Path, ours: &str, pre: &str) -> Result<Local> {
    let out = git(root).args(["rev-parse", "--verify", "--quiet", &format!("HEAD:{ours}")]).output().context("could not run git")?;
    Ok(if !out.status.success() {
        Local::Missing
    } else if String::from_utf8_lossy(&out.stdout).trim() == pre {
        Local::Pristine
    } else {
        Local::Changed
    })
}

const COMMIT_SECTION: &str = "## Upstream commit ports";

/// `cargo xtask port <repo> <sha>`: applies one upstream commit at our paths with a three-way
/// merge, staged but not committed, and records it in PORTS.md. Says for each file whether ours
/// has local changes. Refuses, writing nothing to the tree or index, on a dirty tree, an unknown,
/// merge or unmerged commit, a commit already recorded, any path the map cannot place, and a
/// deletion of a file of ours that has local changes or that `--allow-delete` did not permit.
/// The upstream blobs a merge needs are copied into our object store (unreachable until used).
pub fn port(root: &Path, key: &str, rev: &str, upstream_dir: &Path, mode: PortMode, options: &PortOptions) -> Result<PortStatus> {
    let upstream = by_key(key).with_context(|| format!("unknown upstream `{key}`"))?;
    let clone = upstream_dir.join(key);
    git_out(&clone, &["rev-parse", "--git-dir"]).with_context(|| format!("no clone of {} in {}", upstream.name, clone.display()))?;
    let refuse = |why: String| Ok(PortStatus::Refused(why));
    let status = git_out(root, &["status", "--porcelain", "--untracked-files=normal"])?;
    if !status.is_empty() {
        return refuse(format!("the working tree has changes; commit or stash them first:\n{status}"));
    }
    let sha = match resolve_commit(&clone, rev) {
        Ok(sha) => sha,
        Err(e) => return refuse(e.to_string()),
    };
    if let Some(why) = off_main(&clone, upstream, &sha)? {
        return refuse(why);
    }
    if git_out(&clone, &["rev-list", "--parents", "-n1", &sha])?.split_whitespace().count() > 2 {
        return refuse(format!("{sha} is a merge commit; port its parents' commits instead"));
    }
    let ports_path = root.join("docs/PORTS.md");
    let md = std::fs::read_to_string(&ports_path).context("docs/PORTS.md")?;
    let in_files = attributions::parse_ports(&md).into_iter().map(|r| (r.commit, "a file table"));
    let in_commits = attributions::parse_commit_ports(&md)?.into_iter().map(|c| (c.commit, "the commit table"));
    if let Some((c, table)) = in_files.chain(in_commits).find(|(c, _)| sha.starts_with(c.as_str())) {
        return refuse(format!("{sha} is already recorded in {table} of docs/PORTS.md (as {c})"));
    }

    let maps = load_path_maps(root)?;
    let map = maps.get(key).with_context(|| format!("{PATH_MAP} has no [{key}] table"))?;
    let touched = git(&clone).args(["diff-tree", "-r", "--root", "--no-commit-id", "--raw", "--no-abbrev", "--no-renames", "-z", &sha]).output()?;
    if !touched.status.success() {
        bail!("git diff-tree {sha}: {}", String::from_utf8_lossy(&touched.stderr).trim());
    }
    let moved_since = |at: &str| is_ancestor(&clone, at, &sha);
    let (mut changes, mut dropped, mut refused) = (Vec::new(), Vec::new(), Vec::new());
    // Each entry is `:<modes> <blob before> <blob after> <status>`, then the path.
    let mut fields = touched.stdout.split(|&b| b == 0).filter(|p| !p.is_empty());
    while let Some(meta) = fields.next() {
        let meta = String::from_utf8_lossy(meta);
        let (Some(raw), [_, _, pre, _, status]) = (fields.next(), &meta.split(' ').collect::<Vec<_>>()[..]) else {
            bail!("unexpected git diff-tree entry {meta}")
        };
        let status = status.chars().next().unwrap_or('?');
        let path = String::from_utf8_lossy(raw).into_owned();
        // Patch headers quote such paths, and the header rewrite matches them literally.
        if raw.iter().any(|&b| !(0x20..0x7f).contains(&b) || b == b'"' || b == b'\\') {
            refused.push(format!("{path}: git quotes this path; port by hand"));
            continue;
        }
        match map.route(&path, &moved_since)? {
            Route::To(ours) => changes.push(Change { status, from: path, ours, pre: pre.to_string() }),
            Route::Skip => dropped.push(path),
            Route::Refuse(why) => refused.push(format!("{path}: {why}")),
        }
    }
    if !refused.is_empty() {
        return refuse(format!("{sha} touches paths that cannot be ported automatically:\n  {}", refused.join("\n  ")));
    }
    if changes.is_empty() {
        return refuse(format!("{sha} only touches skipped paths"));
    }
    for path in &dropped {
        println!("dropped: {path}");
    }
    let subject = git_out(&clone, &["log", "-1", "--format=%s", &sha])?;
    let ours: Vec<&str> = changes.iter().map(|c| c.ours.as_str()).collect();
    let row = commit_row(upstream, &sha, &subject, &ours);
    let recorded = with_commit_row(&md, &row, &sha)?;

    if mode == PortMode::Record {
        for c in &changes {
            println!("{} {} -> {}", c.status, c.from, c.ours);
        }
        std::fs::write(&ports_path, recorded).context("docs/PORTS.md")?;
        println!("recorded in docs/PORTS.md:\n{row}");
        return Ok(PortStatus::Applied);
    }
    let (mut by_hand, mut deletes) = (Vec::new(), Vec::new());
    for c in &changes {
        let note = match (c.status, local_state(root, &c.ours, &c.pre)?) {
            ('A', Local::Missing) => "new file",
            ('A', _) => "we already have a file here: expect a conflict",
            ('D', Local::Pristine) => {
                deletes.push(c.ours.as_str());
                "deletes our file, which is still the upstream's"
            }
            ('D', Local::Changed) => {
                by_hand.push(format!("{}: deleted upstream, but our {} has local changes", c.from, c.ours));
                "deleted upstream; ours has local changes"
            }
            (_, Local::Missing) => {
                by_hand.push(format!("{}: we have no {}", c.from, c.ours));
                "we have no such file"
            }
            (_, Local::Pristine) => "ours is still the upstream's file",
            (_, Local::Changed) => "ours has local changes: merged three-way, review the result",
        };
        println!("{} {} -> {} ({note})", c.status, c.from, c.ours);
    }
    if !by_hand.is_empty() {
        return refuse(format!("{sha} cannot be applied without losing or guessing at our side; port by hand:\n  {}", by_hand.join("\n  ")));
    }
    if !deletes.is_empty() && !options.allow_delete {
        return refuse(format!(
            "{sha} deletes files of ours (an upstream rename is a deletion and an addition):\n  {}\ncheck that nothing of ours still needs them, then rerun with --allow-delete",
            deletes.join("\n  ")
        ));
    }
    let patch = mapped_patch(&clone, &sha, &changes, &dropped)?;
    if mode == PortMode::DryRun {
        println!("{row}");
        let check = output_with_stdin(git(root).args(["apply", "--check", "--whitespace=nowarn"]), patch)?;
        if check.status.success() {
            println!("git apply --check: applies cleanly");
        } else {
            println!("git apply --check: does not apply cleanly (a real run merges three-way):\n{}", String::from_utf8_lossy(&check.stderr).trim());
        }
        return Ok(PortStatus::Applied);
    }
    // Without the upstream preimage blobs, `apply --3way` cannot merge and rejects whole files.
    for pre in changes.iter().map(|c| &c.pre).filter(|pre| !pre.bytes().all(|b| b == b'0')) {
        let blob = git(&clone).args(["cat-file", "blob", pre]).output()?;
        if !blob.status.success() {
            bail!("git cat-file blob {pre}: {}", String::from_utf8_lossy(&blob.stderr).trim());
        }
        let id = output_with_stdin(git(root).args(["hash-object", "-w", "--stdin"]), blob.stdout)?;
        if !id.status.success() || String::from_utf8_lossy(&id.stdout).trim() != pre {
            bail!("could not import upstream blob {pre}: {}", String::from_utf8_lossy(&id.stderr).trim());
        }
    }
    let applied = output_with_stdin(git(root).args(["apply", "--3way", "--whitespace=nowarn"]), patch)?;
    let said = String::from_utf8_lossy(&applied.stderr);
    let unmerged = git_out(root, &["diff", "--name-only", "--diff-filter=U"])?;
    if !unmerged.is_empty() {
        eprintln!("{}", said.trim());
        println!("conflicts left in the tree:\n{unmerged}\nresolve them, add the regression test, commit, then run `cargo xtask port {key} {sha} --record`");
        return Ok(PortStatus::Conflicts);
    }
    if !applied.status.success() {
        bail!("git apply --3way: {}", said.trim());
    }
    // git names here the files it had to merge three-way.
    if !said.trim().is_empty() {
        eprintln!("{}", said.trim());
    }
    std::fs::write(&ports_path, recorded).context("docs/PORTS.md")?;
    println!("{row}\nthe change is staged and the docs/PORTS.md row is not; add the regression test, then `cargo xtask attributions`");
    Ok(PortStatus::Applied)
}

/// The commit's diff with `dropped` paths' file blocks removed and the headers of the others
/// rewritten to our paths. The commit message is left out: an unindented diff in it would otherwise
/// be applied as part of the patch.
fn mapped_patch(clone: &Path, sha: &str, changes: &[Change], dropped: &[String]) -> Result<Vec<u8>> {
    let out = git(clone)
        .args(["diff-tree", "-p", "--root", "--no-commit-id", "--full-index", "--binary", "--no-renames", "--src-prefix=a/", "--dst-prefix=b/", sha])
        .output()?;
    if !out.status.success() {
        bail!("git diff-tree {sha}: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    let mut patch = Vec::new();
    // (upstream path, our path) of the current file block; None while skipping a dropped one.
    let mut block: Option<(&str, &str)> = None;
    let mut in_header = false;
    for line in out.stdout.split_inclusive(|&b| b == b'\n') {
        if line.starts_with(b"diff --git ") {
            let is = |p: &str| line == format!("diff --git a/{p} b/{p}\n").as_bytes();
            in_header = true;
            if dropped.iter().any(|p| is(p)) {
                block = None;
                continue;
            }
            let change = changes.iter().find(|c| is(&c.from)).with_context(|| format!("unexpected patch header {}", String::from_utf8_lossy(line).trim()))?;
            let to = &change.ours;
            block = Some((&change.from, to));
            patch.extend_from_slice(format!("diff --git a/{to} b/{to}\n").as_bytes());
            continue;
        }
        let Some((from, to)) = block else { continue };
        if in_header && (line.starts_with(b"@@") || line == b"GIT binary patch\n") {
            in_header = false;
        }
        if !in_header {
            patch.extend_from_slice(line);
        } else if line == format!("--- a/{from}\n").as_bytes() {
            patch.extend_from_slice(format!("--- a/{to}\n").as_bytes());
        } else if line == format!("+++ b/{from}\n").as_bytes() {
            patch.extend_from_slice(format!("+++ b/{to}\n").as_bytes());
        } else {
            patch.extend_from_slice(line);
        }
    }
    Ok(patch)
}

/// A row of the commit table, in the form most existing rows use.
fn commit_row(upstream: &Upstream, sha: &str, subject: &str, ours: &[&str]) -> String {
    let files: Vec<String> = ours.iter().map(|p| format!("`{p}`")).collect();
    format!("| [`{}`]({}/commit/{sha}) | {} | {} | {} |", &sha[..12], upstream.url, subject.replace('|', "\\|"), files.join("<br>"), upstream.licence)
}

/// `md` with `row` (the port of `sha`) appended to the table under PORTS.md's first
/// `## Upstream commit ports` heading; the section is created at the end if it is missing.
fn with_commit_row(md: &str, row: &str, sha: &str) -> Result<String> {
    let parts: Vec<&str> = md.split_inclusive('\n').collect();
    let mut out = String::new();
    match parts.iter().position(|l| l.trim() == COMMIT_SECTION) {
        None => {
            out.push_str(md);
            if !md.is_empty() && !md.ends_with('\n') {
                out.push('\n');
            }
            out.push_str(&format!("\n{COMMIT_SECTION}\n\n| Upstream commit | Subject | Our files | Licence |\n|---|---|---|---|\n{row}\n"));
        }
        Some(heading) => {
            let is_row = |l: &&str| l.trim_start().starts_with('|');
            let start =
                parts[heading..].iter().position(is_row).map(|i| heading + i).with_context(|| format!("docs/PORTS.md has no table under {COMMIT_SECTION}"))?;
            let end = parts[start..].iter().position(|l| !is_row(l)).map_or(parts.len(), |i| start + i);
            out.push_str(&parts[..end].concat());
            if !out.ends_with('\n') {
                out.push('\n');
            }
            out.push_str(row);
            out.push('\n');
            out.push_str(&parts[end..].concat());
        }
    }
    let rows = attributions::parse_commit_ports(&out).context("the new docs/PORTS.md row does not parse")?;
    if !rows.iter().any(|c| c.commit == sha) {
        bail!("the table under {COMMIT_SECTION} in docs/PORTS.md is not a commit table; the row would not be read back");
    }
    Ok(out)
}

// upstream-drift

/// `cargo xtask upstream-drift`: for each mapped directory, how our committed copy differs from
/// the upstream at `rev` (default: its review cutoff), as a markdown table. Writes nothing.
pub fn upstream_drift(root: &Path, upstream_dir: &Path, repo: Option<&str>, rev: Option<&str>) -> Result<String> {
    if rev.is_some() && repo.is_none() {
        bail!("--rev needs --repo");
    }
    let maps = load_path_maps(root)?;
    let markers = review_markers(&std::fs::read_to_string(root.join("docs/PORTS.md")).context("docs/PORTS.md")?)?;
    let mut upstreams: Vec<&Upstream> = match repo {
        Some(key) => vec![by_key(key).with_context(|| format!("unknown upstream `{key}`"))?],
        None => UPSTREAMS.iter().collect(),
    };
    upstreams.sort_by_key(|u| u.name);
    let mut out = String::from(
        "| Upstream | Upstream dir | Ours | Identical | Changed | Ours only | Upstream only | Lines +ours/-upstream |\n|---|---|---|---|---|---|---|---|\n",
    );
    for upstream in upstreams {
        let Some(map) = maps.get(upstream.key).filter(|m| !m.map.is_empty() || !m.moved.is_empty()) else { continue };
        let clone = upstream_dir.join(upstream.key);
        let rev = match rev {
            Some(rev) => rev,
            None => markers.get(upstream.key).with_context(|| format!("docs/PORTS.md has no review cutoff for {}", upstream.name))?,
        };
        let rev = resolve_commit(&clone, rev)?;
        let mut rows: Vec<(&str, String)> = Vec::new();
        let mut changed: Vec<(&str, String, String)> = Vec::new();
        for (dir, ours) in &map.map {
            let theirs = tree_blobs(&clone, &rev, dir)?;
            let mine = tree_blobs(root, "HEAD", ours)?;
            let identical = theirs.iter().filter(|(p, id)| mine.get(*p) == Some(id)).count();
            let both = theirs.keys().filter(|p| mine.contains_key(*p)).count();
            for (p, id) in &theirs {
                if let Some(our_id) = mine.get(p).filter(|o| *o != id) {
                    changed.push((dir, our_id.clone(), id.clone()));
                }
            }
            let counts = format!("{identical} | {} | {} | {}", both - identical, mine.len() - both, theirs.len() - both);
            rows.push((dir, format!("| {} | {dir} | {ours} | {counts} |", upstream.name)));
        }
        let our_text = cat_blobs(root, changed.iter().map(|(_, o, _)| o.as_str()))?;
        let their_text = cat_blobs(&clone, changed.iter().map(|(_, _, t)| t.as_str()))?;
        let mut lines: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
        for (dir, o, t) in &changed {
            let (plus, minus) = line_drift(&our_text[o], &their_text[t]);
            let sum = lines.entry(dir).or_default();
            *sum = (sum.0 + plus, sum.1 + minus);
        }
        for (dir, row) in &mut rows {
            let (plus, minus) = lines.get(dir).copied().unwrap_or_default();
            row.push_str(&format!(" +{plus}/-{minus} |"));
        }
        for (dir, m) in &map.moved {
            rows.push((dir, format!("| {} | {dir} | {} | moved to {} at {} | | | | |", upstream.name, m.ours, m.to, m.at)));
        }
        rows.sort();
        for (_, row) in rows {
            out.push_str(&row);
            out.push('\n');
        }
    }
    out.push_str("\nLines +ours/-upstream counts the lines each side has that the other lacks (not a diff).\n");
    Ok(out)
}

/// Path under `prefix` → blob id, for the committed tree `rev` of `repo`.
fn tree_blobs(repo: &Path, rev: &str, prefix: &str) -> Result<BTreeMap<String, String>> {
    let out = git(repo).args(["ls-tree", "-r", "-z", rev, "--", prefix]).output()?;
    if !out.status.success() {
        bail!("git ls-tree {rev} {prefix} in {}: {}", repo.display(), String::from_utf8_lossy(&out.stderr).trim());
    }
    let mut blobs = BTreeMap::new();
    for entry in out.stdout.split(|&b| b == 0).filter(|e| !e.is_empty()) {
        let entry = String::from_utf8_lossy(entry);
        let Some((meta, path)) = entry.split_once('\t') else { bail!("unexpected ls-tree entry {entry}") };
        if let [_, "blob", id] = meta.split(' ').collect::<Vec<_>>().as_slice()
            && let Some(rel) = path.strip_prefix(prefix)
        {
            blobs.insert(rel.to_string(), id.to_string());
        }
    }
    Ok(blobs)
}

/// The contents of blobs `ids` of `repo`, read through one `git cat-file --batch`.
fn cat_blobs<'a>(repo: &Path, ids: impl Iterator<Item = &'a str>) -> Result<HashMap<String, Vec<u8>>> {
    let input: String = ids.map(|id| format!("{id}\n")).collect();
    if input.is_empty() {
        return Ok(HashMap::new());
    }
    let out = output_with_stdin(git(repo).args(["cat-file", "--batch"]), input.into_bytes())?;
    if !out.status.success() {
        bail!("git cat-file --batch in {}: {}", repo.display(), String::from_utf8_lossy(&out.stderr).trim());
    }
    let mut blobs = HashMap::new();
    let mut rest = out.stdout.as_slice();
    while !rest.is_empty() {
        let end = rest.iter().position(|&b| b == b'\n').context("truncated cat-file header")?;
        let header = String::from_utf8_lossy(&rest[..end]).into_owned();
        let [id, _, size] = header.split(' ').collect::<Vec<_>>()[..] else { bail!("git cat-file in {}: {header}", repo.display()) };
        let size: usize = size.parse().with_context(|| format!("git cat-file header {header}"))?;
        let body = rest.get(end + 1..end + 1 + size).context("truncated cat-file body")?;
        blobs.insert(id.to_string(), body.to_vec());
        rest = rest.get(end + 2 + size..).unwrap_or_default();
    }
    Ok(blobs)
}

/// (lines of `ours` missing from `theirs`, lines of `theirs` missing from `ours`), each side
/// counted as a multiset of lines.
fn line_drift(ours: &[u8], theirs: &[u8]) -> (usize, usize) {
    let lines = |b: &[u8]| -> Vec<Vec<u8>> {
        let b = b.strip_suffix(b"\n").unwrap_or(b);
        if b.is_empty() { Vec::new() } else { b.split(|&c| c == b'\n').map(<[u8]>::to_vec).collect() }
    };
    let mut counts: HashMap<Vec<u8>, isize> = HashMap::new();
    for l in lines(ours) {
        *counts.entry(l).or_default() += 1;
    }
    for l in lines(theirs) {
        *counts.entry(l).or_default() -= 1;
    }
    let plus = counts.values().filter(|&&c| c > 0).sum::<isize>().unsigned_abs();
    let minus = counts.values().filter(|&&c| c < 0).sum::<isize>().unsigned_abs();
    (plus, minus)
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
    use std::path::PathBuf;

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

    /// An upstream clone (`upstream/photocraft`: a base commit, then `moved`, the commit that moved
    /// crates/cms away) and our repository (`ours`) holding the base's blur.rs at its mapped path.
    struct PortFixture {
        dir: PathBuf,
        clone: PathBuf,
        ours: PathBuf,
        moved: String,
    }

    const PORTS_INTRO: &str = "# Ports\n\nIntro.\n";

    fn blur(changes: &[(usize, &str)]) -> String {
        (1..=20).map(|i| changes.iter().find(|(n, _)| *n == i).map_or(format!("line {i}\n"), |(_, l)| format!("{l}\n"))).collect()
    }

    impl PortFixture {
        fn new(name: &str) -> Self {
            let dir = temp_dir(name);
            let (clone, ours) = (dir.join("upstream/photocraft"), dir.join("ours"));
            let mut fixture = PortFixture { dir, clone, ours, moved: String::new() };
            for repo in [&fixture.clone, &fixture.ours] {
                std::fs::create_dir_all(repo).unwrap();
                git(repo, &["init", "--quiet"]);
            }
            git(&fixture.clone, &["symbolic-ref", "refs/remotes/origin/HEAD", "refs/heads/main"]);
            fixture.write(&fixture.clone, "crates/algo/src/blur.rs", &blur(&[]));
            fixture.write(&fixture.clone, "crates/cms/src/lib.rs", "cms\n");
            fixture.write(&fixture.clone, "crates/raw/src/lib.rs", "raw\n");
            fixture.write(&fixture.clone, "docs/notes.md", "notes\n");
            fixture.commit_upstream("base");
            fixture.write(&fixture.clone, "crates/cms/src/lib.rs", "moved\n");
            fixture.moved = fixture.commit_upstream("Use craft-cms from craft-libs");
            fixture.write(&fixture.ours, "crates/pc-algo/src/blur.rs", &blur(&[]));
            fixture.write(&fixture.ours, "docs/PORTS.md", PORTS_INTRO);
            let map = format!(
                "[photocraft]\nskip = [\"docs/\"]\n[photocraft.map]\n\"crates/algo/\" = \"crates/pc-algo/\"\n\
                 [photocraft.moved]\n\"crates/cms/\" = {{ ours = \"crates/pc-cms/\", to = \"craft-libs craft-cms\", at = \"{}\" }}\n\
                 [photocraft.unmapped]\n\"crates/raw/\" = \"reads RAW elsewhere\"\n",
                fixture.moved
            );
            fixture.write(&fixture.ours, "docs/upstream-paths.toml", &map);
            fixture.commit_ours("base");
            fixture
        }

        fn write(&self, repo: &Path, path: &str, text: &str) {
            let path = repo.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }

        fn commit_upstream(&self, message: &str) -> String {
            git(&self.clone, &["add", "-A"]);
            git(&self.clone, &["commit", "--quiet", "-m", message]);
            git(&self.clone, &["rev-parse", "HEAD"])
        }

        fn commit_ours(&self, message: &str) {
            git(&self.ours, &["add", "-A"]);
            git(&self.ours, &["commit", "--quiet", "-m", message]);
        }

        fn port(&self, sha: &str, mode: PortMode) -> PortStatus {
            self.port_with(sha, mode, &PortOptions::default())
        }

        fn port_with(&self, sha: &str, mode: PortMode, options: &PortOptions) -> PortStatus {
            port(&self.ours, "photocraft", sha, &self.dir.join("upstream"), mode, options).unwrap()
        }

        fn refused(&self, sha: &str, mode: PortMode) -> String {
            match self.port(sha, mode) {
                PortStatus::Refused(why) => why,
                other => panic!("expected a refusal, got {other:?}"),
            }
        }

        fn read(&self, path: &str) -> String {
            std::fs::read_to_string(self.ours.join(path)).unwrap()
        }

        fn row(sha: &str, subject: &str) -> String {
            format!(
                "| [`{}`](https://github.com/storytold/photocraft/commit/{sha}) | {subject} | `crates/pc-algo/src/blur.rs` | MIT OR Apache-2.0 |\n",
                &sha[..12]
            )
        }
    }

    impl Drop for PortFixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn port_applies_and_records_a_clean_commit() {
        let f = PortFixture::new("port-clean");
        f.write(&f.clone, "crates/algo/src/blur.rs", &blur(&[(2, "upstream 2")]));
        f.write(&f.clone, "docs/notes.md", "more notes\n");
        let sha = f.commit_upstream("fix(blur): exact | variance");

        assert_eq!(f.port(&sha[..9], PortMode::DryRun), PortStatus::Applied);
        assert_eq!(git(&f.ours, &["status", "--porcelain", "--untracked-files=all"]), "");
        assert_eq!(f.read("docs/PORTS.md"), PORTS_INTRO);

        assert_eq!(f.port(&sha[..9], PortMode::Apply), PortStatus::Applied);
        assert_eq!(git(&f.ours, &["diff", "--cached", "--name-only"]), "crates/pc-algo/src/blur.rs");
        assert_eq!(git(&f.ours, &["show", ":crates/pc-algo/src/blur.rs"]), blur(&[(2, "upstream 2")]).trim_end());
        assert!(!f.ours.join("docs/notes.md").exists());
        assert_eq!(
            f.read("docs/PORTS.md"),
            format!(
                "{PORTS_INTRO}\n## Upstream commit ports\n\n| Upstream commit | Subject | Our files | Licence |\n|---|---|---|---|\n{}",
                PortFixture::row(&sha, "fix(blur): exact \\| variance")
            )
        );
    }

    #[test]
    fn port_ignores_diffs_quoted_in_the_commit_message() {
        let f = PortFixture::new("port-message-diff");
        f.write(&f.clone, "crates/algo/src/blur.rs", &blur(&[(2, "upstream 2")]));
        let quoted = "diff --git a/crates/algo/src/blur.rs b/crates/algo/src/blur.rs\n--- a/crates/algo/src/blur.rs\n\
                      +++ b/crates/algo/src/blur.rs\n@@ -10 +10 @@\n-line 10\n+not in the diff\n";
        let sha = f.commit_upstream(&format!("fix(blur): quote a diff\n\n{quoted}"));

        assert_eq!(f.port(&sha, PortMode::Apply), PortStatus::Applied);
        assert_eq!(git(&f.ours, &["show", ":crates/pc-algo/src/blur.rs"]), blur(&[(2, "upstream 2")]).trim_end());
    }

    #[test]
    fn port_leaves_three_way_conflicts_unrecorded() {
        let f = PortFixture::new("port-conflict");
        let existing = format!(
            "{PORTS_INTRO}\n## Upstream commit ports\n\n| Upstream commit | Subject | Our files | Licence |\n|---|---|---|---|\n{}",
            PortFixture::row(&f.moved, "earlier")
        );
        f.write(&f.ours, "docs/PORTS.md", &existing);
        f.write(&f.ours, "crates/pc-algo/src/blur.rs", &blur(&[(15, "ours 15")]));
        f.commit_ours("diverge");
        f.write(&f.clone, "crates/algo/src/blur.rs", &blur(&[(2, "upstream 2"), (15, "upstream 15")]));
        let sha = f.commit_upstream("fix(blur): two hunks");

        assert_eq!(f.port(&sha, PortMode::Apply), PortStatus::Conflicts);
        let merged = f.read("crates/pc-algo/src/blur.rs");
        assert!(merged.starts_with("line 1\nupstream 2\nline 3\n"), "{merged}");
        assert_eq!(merged.matches("<<<<<<< ").count(), 1, "{merged}");
        let conflict = &merged[merged.find("<<<<<<< ").unwrap()..merged.find(">>>>>>> ").unwrap()];
        assert!(conflict.contains("ours 15") && conflict.contains("upstream 15") && !conflict.contains("line 14"), "{merged}");
        assert_eq!(f.read("docs/PORTS.md"), existing);

        f.write(&f.ours, "crates/pc-algo/src/blur.rs", &blur(&[(2, "upstream 2"), (15, "upstream 15")]));
        f.commit_ours("port by hand");
        assert_eq!(f.port(&sha, PortMode::Record), PortStatus::Applied);
        assert_eq!(f.read("docs/PORTS.md"), format!("{existing}{}", PortFixture::row(&sha, "fix(blur): two hunks")));
    }

    #[test]
    fn port_merges_around_a_local_change_when_only_the_upstream_has_the_preimage() {
        let f = PortFixture::new("port-three-way");
        f.write(&f.ours, "crates/pc-algo/src/blur.rs", &blur(&[(5, "ours 5")]));
        f.commit_ours("local change");
        f.write(&f.clone, "crates/algo/src/blur.rs", &blur(&[(18, "upstream 18")]));
        f.commit_upstream("not ported");
        f.write(&f.clone, "crates/algo/src/blur.rs", &blur(&[(2, "upstream 2"), (18, "upstream 18")]));
        let sha = f.commit_upstream("fix(blur): line 2");

        assert_eq!(f.port(&sha, PortMode::Apply), PortStatus::Applied);
        assert_eq!(git(&f.ours, &["show", ":crates/pc-algo/src/blur.rs"]), blur(&[(2, "upstream 2"), (5, "ours 5")]).trim_end());
    }

    #[test]
    fn port_keeps_our_file_when_the_upstream_adds_one_at_its_path() {
        let f = PortFixture::new("port-add-add");
        f.write(&f.ours, "crates/pc-algo/src/new.rs", "ours\n");
        f.commit_ours("our own file");
        f.write(&f.clone, "crates/algo/src/new.rs", "upstream\n");
        let sha = f.commit_upstream("feat: new file");

        assert_eq!(f.port(&sha, PortMode::Apply), PortStatus::Conflicts);
        let merged = f.read("crates/pc-algo/src/new.rs");
        assert!(merged.contains("<<<<<<< ") && merged.contains("ours\n") && merged.contains("upstream\n"), "{merged}");
        assert_eq!(f.read("docs/PORTS.md"), PORTS_INTRO);
    }

    #[test]
    fn port_deletes_our_file_only_when_allowed_and_unchanged() {
        let f = PortFixture::new("port-delete");
        let ours = "crates/pc-algo/src/old.rs";
        f.write(&f.clone, "crates/algo/src/old.rs", "old\n");
        f.commit_upstream("add old");
        f.write(&f.ours, ours, "old\n");
        f.commit_ours("take old");
        let pre = git(&f.ours, &["rev-parse", &format!("HEAD:{ours}")]);
        std::fs::remove_file(f.clone.join("crates/algo/src/old.rs")).unwrap();
        f.write(&f.clone, "crates/algo/src/blur.rs", &blur(&[(2, "upstream 2")]));
        let sha = f.commit_upstream("refactor: drop old");
        let allow = PortOptions { allow_delete: true };
        let untouched = |f: &PortFixture| {
            assert_eq!(git(&f.ours, &["status", "--porcelain", "--untracked-files=all"]), "");
            assert_eq!(f.read("docs/PORTS.md"), PORTS_INTRO);
        };

        assert_eq!(local_state(&f.ours, ours, &pre).unwrap(), Local::Pristine);
        assert_eq!(local_state(&f.ours, "crates/pc-algo/src/none.rs", &pre).unwrap(), Local::Missing);
        for mode in [PortMode::DryRun, PortMode::Apply] {
            let why = f.refused(&sha, mode);
            assert!(why.contains(ours) && why.contains("--allow-delete"), "{why}");
            untouched(&f);
        }

        f.write(&f.ours, ours, "old\nours\n");
        f.commit_ours("adapt old");
        assert_eq!(local_state(&f.ours, ours, &pre).unwrap(), Local::Changed);
        match f.port_with(&sha, PortMode::Apply, &allow) {
            PortStatus::Refused(why) => assert!(why.contains("our crates/pc-algo/src/old.rs has local changes"), "{why}"),
            other => panic!("expected a refusal, got {other:?}"),
        }
        untouched(&f);
        assert_eq!(f.read(ours), "old\nours\n");

        f.write(&f.ours, ours, "old\n");
        f.commit_ours("back to the upstream's old");
        assert_eq!(f.port_with(&sha, PortMode::Apply, &allow), PortStatus::Applied);
        assert_eq!(git(&f.ours, &["diff", "--cached", "--name-status"]), format!("M\tcrates/pc-algo/src/blur.rs\nD\t{ours}"));
    }

    #[test]
    fn commit_rows_land_only_in_a_commit_table() {
        let sha = "0123456789abcdef0123456789abcdef01234567";
        let row = commit_row(&UPSTREAMS[0], sha, "subject", &["crates/pc-algo/src/blur.rs"]);
        let other_table = format!("{COMMIT_SECTION}\n\n| Commit | Why |\n|---|---|\n| a | b |\n");
        assert!(format!("{:#}", with_commit_row(&other_table, &row, sha).unwrap_err()).contains("is not a commit table"));
        assert!(format!("{:#}", with_commit_row(&format!("{COMMIT_SECTION}\n\nNo table.\n"), &row, sha).unwrap_err()).contains("has no table"));
    }

    #[test]
    fn port_refuses_dirty_trees_unmapped_and_moved_paths() {
        let f = PortFixture::new("port-refuse");
        let unchanged = |f: &PortFixture| {
            assert_eq!(git(&f.ours, &["status", "--porcelain", "--untracked-files=all"]), "");
            assert_eq!(f.read("docs/PORTS.md"), PORTS_INTRO);
        };
        f.write(&f.clone, "crates/algo/src/blur.rs", &blur(&[(2, "upstream 2")]));
        let clean = f.commit_upstream("clean");

        git(&f.ours, &["config", "status.showUntrackedFiles", "no"]);
        f.write(&f.ours, "scratch.txt", "untracked");
        assert!(f.refused(&clean, PortMode::Apply).contains("the working tree has changes"));
        assert!(f.refused(&clean, PortMode::Record).contains("the working tree has changes"));
        std::fs::remove_file(f.ours.join("scratch.txt")).unwrap();
        unchanged(&f);
        f.write(&f.ours, "crates/pc-algo/src/blur.rs", &blur(&[(9, "ours 9")]));
        assert!(f.refused(&clean, PortMode::Apply).contains("the working tree has changes"));
        git(&f.ours, &["add", "-A"]);
        assert!(f.refused(&clean, PortMode::Apply).contains("the working tree has changes"));
        assert_eq!(f.read("crates/pc-algo/src/blur.rs"), blur(&[(9, "ours 9")]));
        git(&f.ours, &["reset", "--quiet", "--hard"]);
        unchanged(&f);

        f.write(&f.clone, "crates/algo/src/blur.rs", &blur(&[(3, "upstream 3")]));
        f.write(&f.clone, "crates/raw/src/lib.rs", "raw 2\n");
        let unmapped = f.commit_upstream("touches raw");
        assert!(f.refused(&unmapped, PortMode::Apply).contains("crates/raw/src/lib.rs: reads RAW elsewhere"));
        unchanged(&f);

        f.write(&f.clone, "crates/cms/src/lib.rs", "after the move\n");
        let after_move = f.commit_upstream("cms after the move");
        assert!(f.refused(&after_move, PortMode::Apply).contains("crates/cms/src/lib.rs: moved to craft-libs craft-cms"));
        unchanged(&f);

        f.write(&f.clone, "crates/algo/src/theirs.rs", "one\n");
        f.commit_upstream("a file we never took");
        f.write(&f.clone, "crates/algo/src/theirs.rs", "two\n");
        let no_file = f.commit_upstream("change it");
        assert!(f.refused(&no_file, PortMode::Apply).contains("crates/algo/src/theirs.rs: we have no crates/pc-algo/src/theirs.rs"));
        unchanged(&f);

        git(&f.clone, &["checkout", "--quiet", "-b", "side", &clean]);
        f.write(&f.clone, "crates/algo/src/other.rs", "side\n");
        f.commit_upstream("side");
        git(&f.clone, &["checkout", "--quiet", "-"]);
        git(&f.clone, &["merge", "--quiet", "--no-ff", "-m", "merge side", "side"]);
        let merge = git(&f.clone, &["rev-parse", "HEAD"]);
        assert!(f.refused(&merge, PortMode::Apply).contains("is a merge commit"));
        unchanged(&f);

        git(&f.clone, &["checkout", "--quiet", "-b", "pull-request"]);
        f.write(&f.clone, "crates/algo/src/blur.rs", &blur(&[(7, "unmerged 7")]));
        let unmerged = f.commit_upstream("not merged upstream");
        git(&f.clone, &["checkout", "--quiet", "-"]);
        assert!(f.refused(&unmerged, PortMode::Apply).contains("is not on PhotoCraft's main branch"));
        unchanged(&f);

        assert_eq!(f.port(&clean, PortMode::Apply), PortStatus::Applied);
        f.commit_ours("port");
        let recorded = f.read("docs/PORTS.md");
        assert!(f.refused(&clean, PortMode::Apply).contains("already recorded in the commit table"));
        assert!(f.refused(&clean, PortMode::Record).contains("already recorded in the commit table"));
        assert_eq!(f.read("docs/PORTS.md"), recorded);
    }

    #[test]
    fn upstream_drift_counts_files_and_lines() -> Result<()> {
        let f = PortFixture::new("drift");
        f.write(&f.clone, "crates/algo/src/changed.rs", "a\nb\nc\n");
        f.write(&f.clone, "crates/algo/src/upstream_only.rs", "u\n");
        let rev = f.commit_upstream("upstream side");
        f.write(&f.ours, "crates/pc-algo/src/changed.rs", "a\nB\nc\nd\n");
        f.write(&f.ours, "crates/pc-algo/src/ours_only.rs", "o\n");
        f.write(&f.ours, "docs/PORTS.md", &format!("| Upstream | Reviewed up to |\n|---|---|\n| PhotoCraft | `{rev}` |\n"));
        f.commit_ours("our side");

        let table = upstream_drift(&f.ours, &f.dir.join("upstream"), Some("photocraft"), None)?;
        assert_eq!(
            table,
            format!(
                "| Upstream | Upstream dir | Ours | Identical | Changed | Ours only | Upstream only | Lines +ours/-upstream |\n|---|---|---|---|---|---|---|---|\n\
                 | PhotoCraft | crates/algo/ | crates/pc-algo/ | 1 | 1 | 1 | 1 | +2/-1 |\n\
                 | PhotoCraft | crates/cms/ | crates/pc-cms/ | moved to craft-libs craft-cms at {} | | | | |\n\
                 \nLines +ours/-upstream counts the lines each side has that the other lacks (not a diff).\n",
                f.moved
            )
        );
        assert_eq!(git(&f.ours, &["status", "--porcelain"]), "");
        assert!(format!("{:#}", upstream_drift(&f.ours, &f.dir.join("upstream"), Some("photocraft"), Some("0000000")).unwrap_err()).contains("fetch origin"));
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
        git(&clone, &["checkout", "--quiet", "-b", "pull-request", &second]);
        let unmerged = commit("unmerged");
        git(&clone, &["symbolic-ref", "refs/remotes/origin/HEAD", "refs/heads/main"]);
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
        let off_main = upstream_reviewed(&dir.join("ours"), "photocraft", &unmerged, &upstream_dir);
        let after_refusals = std::fs::read_to_string(&ports)?;
        std::fs::remove_dir_all(&dir)?;

        assert_eq!(markers, BTreeMap::from([("photocraft", first.clone()), ("lightcraft", lc.to_string())]));
        forward?;
        assert_eq!(advanced, md.replace(&format!("| PhotoCraft | `{first}` |"), &format!("| PhotoCraft | `{second}` |")));
        assert!(format!("{:#}", backward.expect_err("a side branch is not a descendant")).contains("only moves forward"));
        assert!(format!("{:#}", unknown.expect_err("an unknown commit")).contains("fetch origin"));
        assert!(format!("{:#}", off_main.expect_err("a commit no main branch has")).contains("is not on PhotoCraft's main branch"));
        assert_eq!(after_refusals, advanced);
        let duplicate = format!("| Upstream | Reviewed up to |\n|---|---|\n| PhotoCraft | `{first}` |\n| photocraft | `{second}` |\n");
        assert!(format!("{:#}", review_markers(&duplicate).expect_err("two cutoffs for one upstream")).contains("line 4"));
        Ok(())
    }
}
