//! `cargo xtask attributions`: regenerate `assets/attributions.json`, the list of everything Local
//! Image uses from other projects that Settings / Preferences / About › Attributions shows.
//!
//! Sources (all read from this checkout, nothing from the network):
//!
//! - `assets/attributions-curated.json`: the hand-written part (app, section order and titles,
//!   assets, data, upstream projects, ports with no PORTS.md row);
//! - `docs/PORTS.md`: one entry per ported upstream file. *Used for* is the row's `Used for` column
//!   when the table has one, otherwise the first sentence of our file's module documentation; the
//!   licence files are `licenses/<project>-*` (e.g. `licenses/darktable-NOTICE.md`);
//! - `crates/li-seg/src/lib.rs` (`MODELS`), `crates/lc-engine/src/segment/mod.rs` (SAM 3's licence)
//!   and `crates/li-ai/families/*.json`: the AI models;
//! - `Cargo.lock`: every crate reachable from `local-image` (with the `heif` feature of official
//!   builds) and `local-image-cli` through normal and build dependencies. Workspace members are
//!   followed with Cargo's feature rules (dev-dependencies and optional dependencies no enabled
//!   feature asks for are left out); other crates through everything the lock file records for them,
//!   on every platform. Licence, repository and description come from
//!   `cargo metadata --offline` when it can run, otherwise from the crates' manifests in Cargo's
//!   registry cache, otherwise from the previous `assets/attributions.json`.
//!
//! The `attributions_json_is_up_to_date` test regenerates everything but the crates' metadata and
//! fails when the committed file differs, so a new port, model, licence file or dependency can't be
//! forgotten.

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

/// The generated file, relative to the repository root.
pub const OUTPUT: &str = "assets/attributions.json";
/// The hand-written input.
pub const CURATED: &str = "assets/attributions-curated.json";
/// The binaries whose dependency graph is listed, with the features official builds enable.
const ROOTS: [(&str, &[&str]); 2] = [("local-image", &["heif"]), ("local-image-cli", &[])];

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub used_for: Option<String>,
    /// Upstream file(s) or component the entry is about.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    /// Our file(s) that use or port it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ours: Option<String>,
    pub licence: String,
    /// Licence or notice files from the repository root, built into the app.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub licence_files: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Section {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intro: Option<String>,
    #[serde(default)]
    pub entries: Vec<Entry>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Attributions {
    #[serde(default)]
    pub schema: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generated_by: Option<String>,
    pub app: Entry,
    pub sections: Vec<Section>,
}

// ------------------------------------------------------------------------------ helpers

/// Markdown cell → plain text: links become their text, backticks and emphasis go.
fn plain(md: &str) -> String {
    let mut out = String::new();
    let mut rest = md;
    while let Some(open) = rest.find('[') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match (after.find("]("), after.find(']')) {
            (Some(close), Some(first)) if close == first => {
                out.push_str(&after[..close]);
                let tail = &after[close + 2..];
                rest = tail.find(')').map_or("", |e| &tail[e + 1..]);
            }
            _ => {
                out.push('[');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    let out = out.replace(['`', '*'], "");
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The first `[text](url)` of a markdown cell.
fn first_link(md: &str) -> Option<(String, String)> {
    let open = md.find('[')?;
    let close = md[open..].find("](")? + open;
    let end = md[close + 2..].find(')')? + close + 2;
    Some((plain(&md[open + 1..close]), md[close + 2..end].trim().to_string()))
}

/// The first sentence of a Rust file's `//!` documentation (its first paragraph, up to ". ").
fn module_summary(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut para = Vec::new();
    for line in text.lines() {
        let Some(doc) = line.trim_start().strip_prefix("//!") else {
            if para.is_empty() && line.trim().is_empty() {
                continue;
            }
            break;
        };
        let doc = doc.trim();
        if doc.is_empty() {
            if para.is_empty() {
                continue;
            }
            break;
        }
        para.push(doc.to_string());
    }
    if para.is_empty() {
        return None;
    }
    let joined = plain(&para.join(" "));
    let sentence = match joined.find(". ") {
        Some(i) => &joined[..=i],
        None => joined.as_str(),
    };
    Some(sentence.trim().to_string())
}

/// `https://github.com/o/r/releases/download/…` or `https://raw.githubusercontent.com/o/r/…` → the
/// repository page, so a link never starts a model download in the browser.
fn project_page(url: &str) -> String {
    for (prefix, host) in [("https://github.com/", "https://github.com/"), ("https://raw.githubusercontent.com/", "https://github.com/")] {
        if let Some(rest) = url.strip_prefix(prefix) {
            let parts: Vec<&str> = rest.split('/').take(2).collect();
            if parts.len() == 2 {
                return format!("{host}{}/{}", parts[0], parts[1]);
            }
        }
    }
    if let Some(rest) = url.strip_prefix("https://huggingface.co/") {
        let parts: Vec<&str> = rest.split('/').take(2).collect();
        if parts.len() == 2 {
            return format!("https://huggingface.co/{}/{}", parts[0], parts[1]);
        }
    }
    url.to_string()
}

/// Repository-relative paths of the files directly in `dir` (sorted).
fn files_in(root: &Path, dir: &str) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(root.join(dir))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_file())
        .map(|e| format!("{dir}/{}", e.file_name().to_string_lossy()))
        .collect();
    out.sort();
    out
}

/// `licenses/<slug>-NOTICE.md`, `licenses/<slug>-LICENSE…` for a project name (`darktable` takes
/// `darktable-NOTICE.md`, not `darktable-ai-NOTICE.md`): the rest of the name starts upper-case.
fn project_licence_files(root: &Path, project: &str) -> Vec<String> {
    let slug: String = project.to_ascii_lowercase().split_whitespace().collect::<Vec<_>>().join("-");
    files_in(root, "licenses")
        .into_iter()
        .filter(|f| {
            let name = f.trim_start_matches("licenses/");
            name.strip_prefix(&slug).and_then(|r| r.strip_prefix('-')).is_some_and(|r| r.starts_with(|c: char| c.is_ascii_uppercase()))
        })
        .collect()
}

/// Notice files under licenses/ that mention `needle`.
fn notices_mentioning(root: &Path, needle: &str) -> Vec<String> {
    if needle.trim().len() < 4 {
        return vec![];
    }
    files_in(root, "licenses").into_iter().filter(|f| f.ends_with(".md") && std::fs::read_to_string(root.join(f)).is_ok_and(|t| t.contains(needle))).collect()
}

// ------------------------------------------------------------------------------ PORTS.md

/// One row of docs/PORTS.md, as the attributions need it.
#[derive(Clone, Debug, PartialEq)]
pub struct PortRow {
    pub ours: String,
    pub project: String,
    pub project_url: Option<String>,
    /// Project cell text after its link ("(via the lensfun crate 0.7.0)").
    pub via: String,
    pub path: String,
    pub commit: String,
    /// The commit cell's text after the hash ("tag release-5.6.0").
    pub tag: String,
    pub licence: String,
    pub used_for: Option<String>,
}

/// The rows of every table in PORTS.md whose header has `Our file`, `Upstream project`, `Upstream
/// path` and `commit` columns (found by name, so columns can be added or reordered). Rows without a
/// commit hash are skipped.
pub fn parse_ports(md: &str) -> Vec<PortRow> {
    let mut out = Vec::new();
    let mut cols: Option<HashMap<&'static str, usize>> = None;
    for line in md.lines().map(str::trim) {
        if !line.starts_with('|') {
            cols = None;
            continue;
        }
        let cells: Vec<&str> = line.trim_matches('|').split('|').map(str::trim).collect();
        if cells.iter().all(|c| c.chars().all(|ch| matches!(ch, '-' | ':' | ' '))) {
            continue;
        }
        let Some(c) = &cols else {
            let lower: Vec<String> = cells.iter().map(|c| plain(c).to_ascii_lowercase()).collect();
            let find = |pred: &dyn Fn(&str) -> bool| lower.iter().position(|c| pred(c));
            let mut m = HashMap::new();
            for (key, pos) in [
                ("ours", find(&|c| c.contains("our") || c.contains("local image"))),
                ("project", find(&|c| c.contains("project") || c == "upstream")),
                ("path", find(&|c| c.contains("path"))),
                ("commit", find(&|c| c.contains("commit"))),
                ("licence", find(&|c| c.contains("licen"))),
                ("used", find(&|c| c.contains("used for") || c.contains("purpose"))),
            ] {
                if let Some(p) = pos {
                    m.insert(key, p);
                }
            }
            if ["ours", "project", "path", "commit"].iter().all(|k| m.contains_key(k)) {
                cols = Some(m);
            }
            continue;
        };
        let get = |k: &str| c.get(k).and_then(|&i| cells.get(i)).copied().unwrap_or("");
        let commit_cell = plain(get("commit"));
        // the first hex word of at least 7 digits: a commit, or the pinned tree of a release
        // archive without commit metadata (e.g. "v0.8.0; immutable tree `ae01bcb…`")
        let Some(hash) = commit_cell
            .split_whitespace()
            .map(|w| w.trim_matches(|ch: char| !ch.is_ascii_alphanumeric()))
            .find(|w| w.len() >= 7 && w.chars().all(|ch| ch.is_ascii_hexdigit()))
        else {
            continue;
        };
        // the rest of the cell: after the hash (a tag in parentheses), or before it (a release)
        let at = commit_cell.find(hash).unwrap_or(0);
        let rest = if at == 0 { &commit_cell[hash.len()..] } else { &commit_cell[..at] };
        let tag =
            rest.split_whitespace().collect::<Vec<_>>().join(" ").trim_matches(|ch: char| ch == '(' || ch == ')' || ch == ';' || ch == '`').trim().to_string();
        let project_cell = get("project");
        let (project, project_url, via) = match first_link(project_cell) {
            Some((name, url)) => {
                let close = project_cell.find(&format!("]({url})")).map_or(project_cell.len(), |i| i + url.len() + 3);
                (name, Some(url), plain(&project_cell[close.min(project_cell.len())..]))
            }
            None => (plain(project_cell), None, String::new()),
        };
        let used = plain(get("used"));
        out.push(PortRow {
            ours: plain(get("ours")),
            project,
            project_url,
            via,
            path: plain(get("path")),
            commit: hash.to_string(),
            tag,
            licence: plain(get("licence")),
            used_for: (!used.is_empty()).then_some(used),
        });
    }
    out
}

fn port_entries(root: &Path) -> Result<Vec<Entry>> {
    let md = std::fs::read_to_string(root.join("docs/PORTS.md")).context("docs/PORTS.md")?;
    let rows = parse_ports(&md);
    Ok(rows
        .into_iter()
        .map(|r| {
            let first_path = r.path.split([',', ' ']).next().unwrap_or("").to_string();
            let url = match &r.project_url {
                Some(u) if u.starts_with("https://github.com/") && first_path.contains('/') => {
                    Some(format!("{}/blob/{}/{first_path}", u.trim_end_matches('/'), r.commit))
                }
                other => other.clone(),
            };
            let used_for =
                r.used_for.clone().or_else(|| r.ours.split([',', ' ']).find(|p| p.ends_with(".rs")).and_then(|p| module_summary(&root.join(p.trim()))));
            let upstream = if r.via.is_empty() { r.path.clone() } else { format!("{} {}", r.path, r.via) };
            Entry {
                name: r.project.clone(),
                version: (!r.tag.is_empty()).then(|| r.tag.clone()),
                used_for,
                upstream: Some(upstream),
                commit: Some(r.commit.clone()),
                ours: Some(r.ours.clone()),
                licence: r.licence.clone(),
                licence_files: project_licence_files(root, &r.project),
                url,
                ..Default::default()
            }
        })
        .collect())
}

// ------------------------------------------------------------------------------ AI models

/// `name: "value"` on a line of Rust source.
fn string_field<'a>(line: &'a str, name: &str) -> Option<&'a str> {
    let rest = line.trim().strip_prefix(name)?.trim_start().strip_prefix(':')?.trim();
    let rest = rest.strip_prefix('"')?;
    Some(&rest[..rest.rfind('"')?])
}

/// li-seg's `MODELS` table, read from its source.
fn li_seg_entries(root: &Path) -> Result<Vec<Entry>> {
    let src = std::fs::read_to_string(root.join("crates/li-seg/src/lib.rs")).context("crates/li-seg/src/lib.rs")?;
    let start = src.find("pub const MODELS").context("li-seg: no MODELS table")?;
    let body = &src[start..];
    let body = &body[..body.find("\n];").unwrap_or(body.len())];
    let mut out = Vec::new();
    let mut cur: Option<(String, String, String, String)> = None;
    let flush = |cur: &mut Option<(String, String, String, String)>, out: &mut Vec<Entry>| {
        if let Some((label, url, licence, task)) = cur.take() {
            let name = match label.rfind(" (") {
                Some(i) if label.ends_with(')') => label[..i].to_string(),
                _ => label.clone(),
            };
            let (licence, author) = match licence.find(" (") {
                Some(i) if licence.ends_with(')') => (licence[..i].to_string(), Some(licence[i + 2..licence.len() - 1].to_string())),
                _ => (licence, None),
            };
            let used_for = match task.as_str() {
                "Subject" => "Subject masks: Select Subject and Remove Background.",
                "Sky" => "Sky masks: Select Sky.",
                "Depth" => "Depth masks in Develop.",
                "ImageText" => "Smart Sort: sorting photos into folders by what they show.",
                "FaceDetect" | "FaceEmbed" => "Smart Sort: finding faces and recognising people (opt-in).",
                "Denoise" => "AI Denoise in Develop.",
                _ => "Masks and image analysis.",
            };
            let needle = author.as_deref().and_then(|a| a.split([',', ';']).next()).unwrap_or("").trim().to_string();
            out.push(Entry {
                name,
                author,
                used_for: Some(used_for.into()),
                licence,
                licence_files: notices_mentioning(root, &needle),
                url: Some(project_page(&url)),
                ..Default::default()
            });
        }
    };
    let mut in_companions = false;
    for line in body.lines() {
        if line.trim_start().starts_with("companions: &[") && !line.trim().ends_with("&[],") {
            in_companions = true;
            continue;
        }
        if in_companions {
            if line.trim().ends_with("],") {
                in_companions = false;
            }
            continue;
        }
        if line.trim_start().starts_with("ModelSpec {") {
            flush(&mut cur, &mut out);
            cur = Some(Default::default());
        }
        let Some(c) = cur.as_mut() else { continue };
        if let Some(v) = string_field(line, "label") {
            c.0 = v.to_string();
        } else if let Some(v) = string_field(line, "url") {
            c.1 = v.to_string();
        } else if let Some(v) = string_field(line, "licence") {
            c.2 = v.to_string();
        } else if let Some(rest) = line.trim().strip_prefix("task:") {
            c.3 = rest.trim().trim_start_matches("Task::").split(|ch: char| !ch.is_alphanumeric()).next().unwrap_or("").to_string();
        }
    }
    flush(&mut cur, &mut out);
    if out.is_empty() {
        bail!("li-seg: MODELS has no entries");
    }
    Ok(out)
}

/// SAM 3 (lc-engine's object masks): its licence constants.
fn sam3_entry(root: &Path) -> Result<Entry> {
    let src = std::fs::read_to_string(root.join("crates/lc-engine/src/segment/mod.rs")).context("crates/lc-engine/src/segment/mod.rs")?;
    let konst = |name: &str| {
        src.lines().find_map(|l| {
            let rest = l.trim().strip_prefix(&format!("pub const {name}: &str = \""))?;
            Some(rest[..rest.rfind('"')?].to_string())
        })
    };
    let licence = konst("LICENSE_NAME").context("lc-engine segment: LICENSE_NAME")?;
    let licence_url = konst("LICENSE_URL").context("lc-engine segment: LICENSE_URL")?;
    let url = licence_url.split("/blob/").next().unwrap_or(&licence_url).to_string();
    Ok(Entry {
        name: "SAM 3 (Segment Anything 3)".into(),
        author: Some("Meta AI (facebook/sam3)".into()),
        used_for: Some("Object masks in Develop: click an object to select it.".into()),
        licence,
        url: Some(url),
        ..Default::default()
    })
}

/// The AI model families (crates/li-ai/families/*.json) that state a licence.
fn family_entries(root: &Path) -> Result<Vec<Entry>> {
    let mut out = Vec::new();
    for f in files_in(root, "crates/li-ai/families") {
        if !f.ends_with(".json") || f.ends_with("/index.json") {
            continue;
        }
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(root.join(&f))?).with_context(|| f.clone())?;
        let Some(licence) = v.get("license").and_then(|l| l.as_str()) else { continue };
        let s = |k: &str| v.get(k).and_then(|x| x.as_str()).map(str::to_string);
        let models: Vec<String> =
            v.get("models").and_then(|m| m.as_array()).into_iter().flatten().filter_map(|m| m.get("label")?.as_str().map(str::to_string)).collect();
        out.push(Entry {
            name: s("label").unwrap_or_else(|| f.clone()),
            used_for: s("description"),
            upstream: (!models.is_empty()).then(|| models.join(", ")),
            licence: licence.to_string(),
            licence_files: vec![],
            url: s("license_url"),
            ..Default::default()
        });
    }
    Ok(out)
}

// ------------------------------------------------------------------------------ Rust crates

#[derive(Clone, Debug)]
struct LockPackage {
    name: String,
    version: String,
    member: bool,
    deps: Vec<String>,
}

fn parse_lock(text: &str) -> Result<Vec<LockPackage>> {
    let lock: toml::Table = toml::from_str(text).context("Cargo.lock")?;
    let pkgs = lock.get("package").and_then(|p| p.as_array()).context("Cargo.lock: no [[package]]")?;
    Ok(pkgs
        .iter()
        .filter_map(|p| {
            let s = |k: &str| p.get(k).and_then(|v| v.as_str()).map(str::to_string);
            Some(LockPackage {
                name: s("name")?,
                version: s("version")?,
                member: p.get("source").is_none(),
                deps: p.get("dependencies").and_then(|d| d.as_array()).into_iter().flatten().filter_map(|d| d.as_str().map(str::to_string)).collect(),
            })
        })
        .collect())
}

/// Package name → manifest of every workspace member.
fn member_manifests(root: &Path) -> HashMap<String, PathBuf> {
    let mut out = HashMap::new();
    let mut dirs: Vec<PathBuf> = vec![root.join("xtask")];
    for parent in ["crates", "apps"] {
        dirs.extend(std::fs::read_dir(root.join(parent)).into_iter().flatten().flatten().map(|e| e.path()));
    }
    for d in dirs {
        let manifest = d.join("Cargo.toml");
        let Ok(text) = std::fs::read_to_string(&manifest) else { continue };
        let Ok(t) = toml::from_str::<toml::Table>(&text) else { continue };
        if let Some(name) = t.get("package").and_then(|p| p.get("name")).and_then(|n| n.as_str()) {
            out.insert(name.to_string(), manifest);
        }
    }
    out
}

/// One normal or build dependency of a workspace member (any target).
#[derive(Clone, Debug)]
struct MemberDep {
    /// The name features refer to it by (the key in the manifest).
    key: String,
    package: String,
    optional: bool,
    default_features: bool,
    features: Vec<String>,
}

/// A workspace member's runtime dependencies and `[features]`.
#[derive(Clone, Debug, Default)]
struct Member {
    deps: Vec<MemberDep>,
    features: BTreeMap<String, Vec<String>>,
}

fn strings(v: Option<&toml::Value>) -> Vec<String> {
    v.and_then(|v| v.as_array()).into_iter().flatten().filter_map(|s| s.as_str().map(str::to_string)).collect()
}

fn read_member(manifest: &Path, workspace_deps: &toml::Table) -> Result<Member> {
    let t: toml::Table = toml::from_str(&std::fs::read_to_string(manifest)?).with_context(|| manifest.display().to_string())?;
    let mut tables: Vec<&toml::Table> = Vec::new();
    for key in ["dependencies", "build-dependencies"] {
        if let Some(d) = t.get(key).and_then(|v| v.as_table()) {
            tables.push(d);
        }
    }
    for target in t.get("target").and_then(|v| v.as_table()).into_iter().flat_map(|t| t.values()) {
        for key in ["dependencies", "build-dependencies"] {
            if let Some(d) = target.get(key).and_then(|v| v.as_table()) {
                tables.push(d);
            }
        }
    }
    let mut m = Member::default();
    for table in tables {
        for (key, spec) in table {
            let inherited = spec.get("workspace").and_then(|w| w.as_bool()) == Some(true);
            let ws = if inherited { workspace_deps.get(key) } else { None };
            let package = spec.get("package").or_else(|| ws?.get("package")).and_then(|p| p.as_str()).unwrap_or(key).to_string();
            let flag = |v: Option<&toml::Value>| ["default-features", "default_features"].iter().find_map(|k| v?.get(*k)?.as_bool());
            let default_features = flag(Some(spec)).unwrap_or(true) && flag(ws).unwrap_or(true);
            let mut features = strings(spec.get("features"));
            features.extend(strings(ws.and_then(|w| w.get("features"))));
            let optional = spec.get("optional").and_then(|o| o.as_bool()).unwrap_or(false);
            m.deps.push(MemberDep { key: key.clone(), package, optional, default_features, features });
        }
    }
    for (name, list) in t.get("features").and_then(|f| f.as_table()).into_iter().flatten() {
        m.features.insert(name.clone(), strings(Some(list)));
    }
    Ok(m)
}

/// Close `requested` over a member's `[features]`: (enabled optional dependency keys, extra
/// features per dependency key).
fn member_activation(m: &Member, requested: &BTreeSet<String>) -> (BTreeSet<String>, BTreeMap<String, BTreeSet<String>>) {
    let mut features: BTreeSet<String> = BTreeSet::new();
    let mut deps: BTreeSet<String> = BTreeSet::new();
    let mut weak: Vec<(String, String)> = Vec::new();
    let mut dep_features: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut todo: Vec<String> = requested.iter().cloned().collect();
    while let Some(f) = todo.pop() {
        if let Some(d) = f.strip_prefix("dep:") {
            deps.insert(d.to_string());
        } else if let Some((d, feat)) = f.split_once('/') {
            match d.strip_suffix('?') {
                Some(d) => weak.push((d.to_string(), feat.to_string())),
                None => {
                    deps.insert(d.to_string());
                    dep_features.entry(d.to_string()).or_default().insert(feat.to_string());
                }
            }
        } else if features.insert(f.clone()) {
            match m.features.get(&f) {
                Some(list) => todo.extend(list.iter().cloned()),
                // An optional dependency's implicit feature.
                None => {
                    deps.insert(f);
                }
            }
        }
    }
    for (d, feat) in weak {
        if deps.contains(&d) || m.deps.iter().any(|x| x.key == d && !x.optional) {
            dep_features.entry(d).or_default().insert(feat);
        }
    }
    (deps, dep_features)
}

/// (name, version) of every non-workspace crate the app is built from, sorted.
///
/// Workspace members are followed through their normal and build dependencies with Cargo's
/// feature rules (so an optional dependency counts only when a feature the app builds with enables
/// it); other crates through everything Cargo.lock records for them (all platforms).
pub fn app_crates(root: &Path) -> Result<Vec<(String, String)>> {
    let pkgs = parse_lock(&std::fs::read_to_string(root.join("Cargo.lock")).context("Cargo.lock")?)?;
    let root_toml: toml::Table = toml::from_str(&std::fs::read_to_string(root.join("Cargo.toml"))?)?;
    let workspace_deps = root_toml.get("workspace").and_then(|w| w.get("dependencies")).and_then(|d| d.as_table()).cloned().unwrap_or_default();
    let manifests = member_manifests(root);
    let mut by_name: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, p) in pkgs.iter().enumerate() {
        by_name.entry(p.name.as_str()).or_default().push(i);
    }
    // "name", "name version" or "name version (source)"
    let resolve = |spec: &str| -> Option<usize> {
        let mut w = spec.split_whitespace();
        let name = w.next()?;
        let cands = by_name.get(name)?;
        match w.next() {
            Some(v) => cands.iter().copied().find(|&i| pkgs[i].version == v),
            None => cands.first().copied(),
        }
    };
    let mut members: HashMap<usize, Member> = HashMap::new();
    // Requested features of each member reached so far; a member is visited again when they grow.
    let mut requested: HashMap<usize, BTreeSet<String>> = HashMap::new();
    let mut seen = vec![false; pkgs.len()];
    let mut queue: VecDeque<usize> = VecDeque::new();
    for (r, features) in ROOTS {
        let i = *by_name.get(r).and_then(|v| v.first()).with_context(|| format!("Cargo.lock has no package {r}"))?;
        let req = requested.entry(i).or_default();
        req.insert("default".into());
        req.extend(features.iter().map(|f| f.to_string()));
        seen[i] = true;
        queue.push_back(i);
    }
    let empty = BTreeSet::new();
    while let Some(i) = queue.pop_front() {
        let p = &pkgs[i];
        if !p.member {
            for spec in &p.deps {
                if let Some(j) = resolve(spec)
                    && !seen[j]
                {
                    seen[j] = true;
                    queue.push_back(j);
                }
            }
            continue;
        }
        if let std::collections::hash_map::Entry::Vacant(e) = members.entry(i) {
            let manifest = manifests.get(&p.name).with_context(|| format!("no manifest for workspace member {}", p.name))?;
            e.insert(read_member(manifest, &workspace_deps)?);
        }
        let m = &members[&i];
        let (enabled, dep_features) = member_activation(m, requested.get(&i).unwrap_or(&empty));
        for d in m.deps.iter().filter(|d| !d.optional || enabled.contains(&d.key)) {
            let Some(j) = p.deps.iter().filter_map(|s| resolve(s)).find(|&j| pkgs[j].name == d.package) else { continue };
            let mut grew = false;
            if pkgs[j].member {
                let mut want: BTreeSet<String> = d.features.iter().cloned().collect();
                want.extend(dep_features.get(&d.key).into_iter().flatten().cloned());
                if d.default_features {
                    want.insert("default".into());
                }
                let req = requested.entry(j).or_default();
                let before = req.len();
                req.extend(want);
                grew = req.len() > before;
            }
            if !seen[j] || grew {
                seen[j] = true;
                queue.push_back(j);
            }
        }
    }
    let mut out: Vec<(String, String)> = pkgs.iter().zip(&seen).filter(|(p, s)| **s && !p.member).map(|(p, _)| (p.name.clone(), p.version.clone())).collect();
    out.sort();
    out.dedup();
    Ok(out)
}

/// The licence shown for a crate whose manifest couldn't be read.
const UNKNOWN: &str = "not recorded";

/// What the attributions show for a crate.
#[derive(Clone, Debug, Default)]
struct CrateMeta {
    licence: Option<String>,
    url: Option<String>,
    description: Option<String>,
}

fn tidy(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// From `cargo metadata --offline` (needs every crate downloaded for every platform).
fn metadata_from_cargo(root: &Path) -> Option<HashMap<(String, String), CrateMeta>> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let out = std::process::Command::new(cargo)
        .args(["metadata", "--format-version", "1", "--offline", "--locked"])
        .current_dir(root)
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    let mut map = HashMap::new();
    for p in v.get("packages")?.as_array()? {
        let s = |k: &str| p.get(k).and_then(|x| x.as_str()).map(tidy).filter(|x| !x.is_empty());
        let (Some(name), Some(version)) = (s("name"), s("version")) else { continue };
        let licence = s("license").or_else(|| s("license_file").map(|f| format!("see {f}")));
        map.insert((name, version), CrateMeta { licence, url: s("repository").or_else(|| s("homepage")), description: s("description") });
    }
    Some(map)
}

/// From the crate's manifest in Cargo's registry cache, when it has been downloaded.
fn metadata_from_registry(name: &str, version: &str) -> Option<CrateMeta> {
    let home = std::env::var_os("CARGO_HOME").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cargo")))?;
    for index in std::fs::read_dir(home.join("registry").join("src")).ok()?.flatten() {
        let manifest = index.path().join(format!("{name}-{version}")).join("Cargo.toml");
        let Ok(text) = std::fs::read_to_string(&manifest) else { continue };
        let t: toml::Table = toml::from_str(&text).ok()?;
        let p = t.get("package")?;
        let s = |k: &str| p.get(k).and_then(|x| x.as_str()).map(tidy).filter(|x| !x.is_empty());
        let licence = s("license").or_else(|| s("license-file").map(|f| format!("see {f}")));
        return Some(CrateMeta { licence, url: s("repository").or_else(|| s("homepage")), description: s("description") });
    }
    None
}

fn crate_entries(root: &Path, previous: Option<&Attributions>) -> Result<(Vec<Entry>, usize)> {
    let crates = app_crates(root)?;
    let cargo = metadata_from_cargo(root);
    if cargo.is_none() {
        eprintln!("note: `cargo metadata --offline` could not run (crates for other platforms not downloaded?); using Cargo's registry cache");
    }
    let old: HashMap<(String, String), &Entry> = previous
        .and_then(|p| p.sections.iter().find(|s| s.id == "crates"))
        .into_iter()
        .flat_map(|s| s.entries.iter())
        .filter(|e| !e.licence.starts_with(UNKNOWN))
        .map(|e| ((e.name.clone(), e.version.clone().unwrap_or_default()), e))
        .collect();
    let old_by_name: HashMap<&str, &Entry> = old.values().map(|e| (e.name.as_str(), *e)).collect();
    let mut unknown = 0;
    let entries = crates
        .into_iter()
        .map(|(name, version)| {
            let key = (name.clone(), version.clone());
            let meta = cargo.as_ref().and_then(|m| m.get(&key).cloned()).or_else(|| metadata_from_registry(&name, &version)).or_else(|| {
                // Not downloaded on this machine: keep what the last run found (same version first).
                let e = old.get(&key).copied().or_else(|| old_by_name.get(name.as_str()).copied())?;
                Some(CrateMeta { licence: Some(e.licence.clone()), url: e.url.clone(), description: e.used_for.clone() })
            });
            let meta = meta.unwrap_or_default();
            if meta.licence.is_none() {
                unknown += 1;
            }
            Entry {
                licence: meta.licence.unwrap_or_else(|| format!("{UNKNOWN} (see crates.io)")),
                url: Some(meta.url.unwrap_or_else(|| format!("https://crates.io/crates/{name}"))),
                used_for: meta.description,
                name,
                version: Some(version),
                ..Default::default()
            }
        })
        .collect();
    Ok((entries, unknown))
}

// ------------------------------------------------------------------------------ generate

/// Everything but the crates' metadata: the curated file with the generated entries merged in.
/// `crates` is the crate section's entries.
pub fn assemble(root: &Path, crates: Vec<Entry>) -> Result<Attributions> {
    let mut a: Attributions = serde_json::from_str(&std::fs::read_to_string(root.join(CURATED)).context(CURATED)?).context(CURATED)?;
    a.schema = 1;
    a.generated_by = Some(format!("cargo xtask attributions (from {CURATED}, docs/PORTS.md, li-seg, lc-engine, li-ai families and Cargo.lock)"));
    let mut generated: BTreeMap<&str, Vec<Entry>> = BTreeMap::new();
    generated.insert("ports", port_entries(root)?);
    let mut models = li_seg_entries(root)?;
    models.push(sam3_entry(root)?);
    models.extend(family_entries(root)?);
    generated.insert("models", models);
    generated.insert("crates", crates);
    for (id, entries) in generated {
        let s = a.sections.iter_mut().find(|s| s.id == id).with_context(|| format!("{CURATED} has no section `{id}`"))?;
        let curated = std::mem::take(&mut s.entries);
        s.entries = entries;
        s.entries.extend(curated);
    }
    Ok(a)
}

pub fn to_json(a: &Attributions) -> Result<String> {
    Ok(serde_json::to_string_pretty(a)? + "\n")
}

/// Every licence file the attributions reference.
pub fn referenced_files(a: &Attributions) -> BTreeSet<String> {
    a.app.licence_files.iter().chain(a.sections.iter().flat_map(|s| s.entries.iter()).flat_map(|e| e.licence_files.iter())).cloned().collect()
}

/// Licence and notice files in the repository that must each be referenced by an entry.
pub fn licence_files_to_cover(root: &Path) -> Vec<String> {
    let mut out = files_in(root, "licenses");
    let mut stack = vec![root.join("assets")];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                let name = e.file_name().to_string_lossy().to_string();
                if ["LICENSE", "OFL", "ATTRIBUTION", "NOTICE"].iter().any(|k| name.starts_with(k))
                    && let Ok(rel) = p.strip_prefix(root)
                {
                    out.push(rel.to_string_lossy().replace('\\', "/"));
                }
            }
        }
    }
    out.sort();
    out
}

pub fn check_licence_files(root: &Path, a: &Attributions) -> Result<()> {
    let referenced = referenced_files(a);
    for f in &referenced {
        if !root.join(f).is_file() {
            bail!("{f} is referenced as a licence file but doesn't exist");
        }
    }
    let missing: Vec<String> = licence_files_to_cover(root).into_iter().filter(|f| !referenced.contains(f)).collect();
    if !missing.is_empty() {
        bail!(
            "licence files no attribution refers to: {missing:?} — add them to an entry's licence_files in {CURATED} (a licenses/<project>-NOTICE.md is picked up by the PORTS.md rows of <project>)"
        );
    }
    Ok(())
}

/// `cargo xtask attributions [--fetch]`. `fetch` downloads every crate of Cargo.lock first (the only
/// step that uses the network), so the licences of crates for other platforms can be read too.
pub fn run(root: &Path, fetch: bool) -> Result<()> {
    if fetch {
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
        let ok = std::process::Command::new(cargo).args(["fetch", "--locked"]).current_dir(root).status().context("cargo fetch")?.success();
        if !ok {
            bail!("cargo fetch failed");
        }
    }
    let out = root.join(OUTPUT);
    let previous: Option<Attributions> = std::fs::read_to_string(&out).ok().and_then(|t| serde_json::from_str(&t).ok());
    let (crates, unknown) = crate_entries(root, previous.as_ref())?;
    let n_crates = crates.len();
    let a = assemble(root, crates)?;
    check_licence_files(root, &a)?;
    std::fs::write(&out, to_json(&a)?)?;
    for s in &a.sections {
        println!("{:>5}  {}", s.entries.len(), s.title);
    }
    println!("wrote {OUTPUT} ({n_crates} crates)");
    if unknown > 0 {
        println!(
            "warning: {unknown} crate(s) have no licence information here (not downloaded); run `cargo xtask attributions --fetch` where the network is available"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> PathBuf {
        crate::root()
    }

    fn committed() -> Attributions {
        serde_json::from_str(&std::fs::read_to_string(root().join(OUTPUT)).expect("assets/attributions.json")).expect("valid JSON")
    }

    /// The committed file matches its sources: regenerate with `cargo xtask attributions` when this
    /// fails. Offline and cheap: Cargo.lock and the workspace manifests give the crate list (their
    /// licences aren't re-read), everything else is regenerated as the task does.
    #[test]
    fn attributions_json_is_up_to_date() {
        let have = committed();
        let crates = have.sections.iter().find(|s| s.id == "crates").expect("a crates section").entries.clone();
        let listed: Vec<(String, String)> = crates.iter().map(|e| (e.name.clone(), e.version.clone().unwrap_or_default())).collect();
        let want_crates = app_crates(&root()).unwrap();
        let added: Vec<_> = want_crates.iter().filter(|c| !listed.contains(c)).collect();
        let removed: Vec<_> = listed.iter().filter(|c| !want_crates.contains(c)).collect();
        assert!(
            added.is_empty() && removed.is_empty(),
            "{OUTPUT} is out of date with Cargo.lock — run `cargo xtask attributions`.\nnew crates: {added:?}\ngone: {removed:?}"
        );
        let want = assemble(&root(), crates).unwrap();
        for (w, h) in want.sections.iter().zip(&have.sections) {
            assert_eq!(w, h, "{OUTPUT}: section `{}` is out of date — run `cargo xtask attributions`", w.id);
        }
        assert_eq!(want, have, "{OUTPUT} is out of date — run `cargo xtask attributions`");
        check_licence_files(&root(), &have).unwrap();
    }

    /// Every PORTS.md row is an entry (the up-to-date test covers this too; this names the row).
    #[test]
    fn every_port_is_attributed() {
        let md = std::fs::read_to_string(root().join("docs/PORTS.md")).unwrap();
        let rows = parse_ports(&md);
        assert!(!rows.is_empty());
        let a = committed();
        let ports = &a.sections.iter().find(|s| s.id == "ports").unwrap().entries;
        for r in rows {
            assert!(
                ports.iter().any(|e| e.ours.as_deref() == Some(r.ours.as_str()) && e.commit.as_deref() == Some(r.commit.as_str())),
                "PORTS.md row {} ({}) is missing from {OUTPUT} — run `cargo xtask attributions`",
                r.ours,
                r.project
            );
        }
    }

    #[test]
    fn ports_rows_parse_links_tags_and_extra_columns() {
        let md = "| Our file | Upstream project | Upstream path | Upstream commit | Licence | Used for | Date |\n|---|---|---|---|---|---|---|\n\
| `a.rs` | [darktable-ai](https://github.com/darktable-org/darktable-ai) | `x/demo.py` (`_run`) | `6bcd41c6f296ca692e6f845b25cf7cdb8148305c` (tag `release-5.6.0`) | GPL-3.0-only | Denoise | 2026-10-09 |\n\
| `b.rs` | [LensFun](https://github.com/lensfun/lensfun) (via the [`lensfun`](https://crates.io/crates/lensfun) crate 0.7.0) | `libs/m.cpp` | `43f9e001ab66c4fcdd4f400463f13b72f94b2288` | LGPL-3.0-or-later | | 2026-10-09 |\n\
| `c.rs` | RawTherapee | `rtengine/x.cc` | (pending) | GPL-3.0-or-later | | |\n";
        let rows = parse_ports(md);
        assert_eq!(rows.len(), 2, "{rows:?}");
        assert_eq!(rows[0].project, "darktable-ai");
        assert_eq!(rows[0].project_url.as_deref(), Some("https://github.com/darktable-org/darktable-ai"));
        assert_eq!(rows[0].tag, "tag release-5.6.0");
        assert_eq!(rows[0].path, "x/demo.py (_run)");
        assert_eq!(rows[0].used_for.as_deref(), Some("Denoise"));
        assert_eq!(rows[1].project, "LensFun");
        assert_eq!(rows[1].via, "(via the lensfun crate 0.7.0)");
        assert_eq!(rows[1].used_for, None);
        assert_eq!(rows[1].licence, "LGPL-3.0-or-later");
    }

    #[test]
    fn project_licence_files_match_the_project_exactly() {
        let dt = project_licence_files(&root(), "darktable");
        assert!(dt.contains(&"licenses/darktable-NOTICE.md".to_string()), "{dt:?}");
        assert!(!dt.iter().any(|f| f.contains("darktable-ai")), "{dt:?}");
        assert_eq!(project_licence_files(&root(), "LensFun"), vec!["licenses/lensfun-NOTICE.md".to_string()]);
    }

    #[test]
    fn model_links_open_project_pages_not_downloads() {
        assert_eq!(project_page("https://github.com/danielgatis/rembg/releases/download/v0.0.0/u2netp.onnx"), "https://github.com/danielgatis/rembg");
        assert_eq!(
            project_page("https://raw.githubusercontent.com/kisakutanaka/SkySegmentation/4f17/models/x.onnx"),
            "https://github.com/kisakutanaka/SkySegmentation"
        );
        let a = committed();
        for e in &a.sections.iter().find(|s| s.id == "models").unwrap().entries {
            let url = e.url.as_deref().unwrap_or("");
            assert!(!url.ends_with(".onnx") && !url.ends_with(".dtmodel"), "{}: {url}", e.name);
        }
    }

    #[test]
    fn clip_bundle_is_attributed_once_with_its_tagging_purpose() {
        let entries = li_seg_entries(&root()).unwrap();
        let clip: Vec<_> = entries.iter().filter(|e| e.name == "CLIP ViT-B-32 LAION").collect();
        assert_eq!(clip.len(), 1);
        assert_eq!(clip[0].licence, "MIT");
        assert_eq!(clip[0].used_for.as_deref(), Some("Smart Sort: sorting photos into folders by what they show."));
        assert_eq!(clip[0].url.as_deref(), Some("https://huggingface.co/immich-app/ViT-B-32__laion2b-s34b-b79k"));
    }

    /// The crate graph leaves out dev-only and xtask-only crates and keeps the GUI stack.
    #[test]
    fn app_crates_are_the_shipped_graph() {
        let crates = app_crates(&root()).unwrap();
        let has = |n: &str| crates.iter().any(|(name, _)| name == n);
        for n in ["egui", "wgpu", "lensfun", "tract-onnx", "serde", "heic-rs"] {
            assert!(has(n), "{n} is compiled into the app");
        }
        // candle comes only with lightcraft-engine's `sam` feature, which the app doesn't enable
        for n in ["egui_kittest", "criterion", "proptest", "zip", "candle-core", "xtask", "local-image"] {
            assert!(!has(n), "{n} is not part of the app");
        }
    }
}
