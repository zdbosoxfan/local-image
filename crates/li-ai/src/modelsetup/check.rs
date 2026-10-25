//! Static checks of a compiled recipe against the connected ComfyUI, before any render or
//! download: version gates (the template index's `minComfyUIVersion` against the server, with
//! `/object_info` deciding), missing node classes and options (core nodes: update ComfyUI; a
//! custom pack: install it), missing companion files, GPU support of the precision, memory, and
//! the licence. Each capability is built once against `/object_info`, exactly as a run builds it.

use std::collections::BTreeMap;

use super::Snapshot;
use super::issue::{Fix, Issue, role_label, size_text};
use crate::builders::{Params, Task};
use crate::companions::{Runtime, Why};
use crate::family::{Commercial, Family};
use crate::identify::Identity;
use crate::template::index::{TemplateEntry, Version};
use crate::template::{TaskGraph, TemplateRecipe};

/// What a model's capability check found.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct CapabilityCheck {
    /// `create`, `edit`, `style`, `refine`, `inpaint`, `upscale`.
    pub name: String,
    pub ok: bool,
    /// References it takes (create with references, edit, style).
    pub references: u32,
    pub detail: String,
}

/// One model to check.
pub struct ModelCheck<'a> {
    pub key: &'a str,
    pub label: &'a str,
    pub file: &'a str,
    pub bytes: Option<u64>,
    pub identity: &'a Identity,
    pub recipe: &'a TemplateRecipe,
    pub family: Option<&'a Family>,
    pub server: &'a Snapshot,
    pub runtime: &'a Runtime,
    /// Local size of an installed file (`folder`, `name`), when the model folder is readable.
    pub local_bytes: &'a dyn Fn(&str, &str) -> Option<u64>,
    /// The person overrode the GPU warning for this model.
    pub try_anyway: bool,
}

/// The outcome: issues and per-capability results.
#[derive(Clone, Debug, Default)]
pub struct Checked {
    pub issues: Vec<Issue>,
    pub capabilities: Vec<CapabilityCheck>,
}

impl Checked {
    pub fn blocked(&self) -> bool {
        self.issues.iter().any(|i| i.severity == super::issue::Severity::Blocker)
    }
}

/// Server-wide findings.
pub fn server_issues(s: &Snapshot) -> Vec<Issue> {
    let mut out = Vec::new();
    if s.comfyui_version.is_none() {
        out.push(Issue::new("VersionUnknown", None, &[]));
    }
    if !s.templates_version.is_empty() && !s.required_templates_version.is_empty() && s.templates_version != s.required_templates_version {
        out.push(Issue::new("TemplatesMismatch", None, &[("installed", s.templates_version.clone()), ("required", s.required_templates_version.clone())]));
    }
    out
}

/// How to update ComfyUI, by how it was installed (shown, never run).
pub fn update_how(s: &Snapshot) -> String {
    if s.deploy_environment == "local-git" && !s.remote {
        "Update ComfyUI: in your ComfyUI folder run `git pull`, then `python -m pip install -r requirements.txt` with ComfyUI's Python, and restart ComfyUI."
            .into()
    } else {
        format!(
            "Update ComfyUI on {} the way it was installed (ComfyUI Desktop: its Update command; portable: update/update_comfyui.bat), then restart it.",
            s.host
        )
    }
}

fn need_clause(need: Option<Version>) -> String {
    need.map(|v| format!(" to {v} or newer")).unwrap_or_default()
}

/// The graph a capability runs (create with references and style run the edit or style graph;
/// refine and fill derive from create).
fn graph_of<'a>(r: &'a TemplateRecipe, capability: &str) -> Option<&'a TaskGraph> {
    match capability {
        "edit" => r.edit.as_ref(),
        "references" => r.edit.as_ref().or(r.style.as_ref()),
        "style" => r.style.as_ref(),
        "create" => r.create.as_ref().or(r.style.as_ref()),
        _ => r.create.as_ref().or(r.edit.as_ref()),
    }
}

/// The required files a graph lacks.
fn missing_files(g: &TaskGraph) -> Vec<&str> {
    g.files.iter().filter(|b| !b.optional && b.used.is_none()).map(|b| b.declared.as_str()).collect()
}

/// Builds each capability's graph against `/object_info` (a capability whose graph lacks a file
/// is off without building).
fn capabilities(r: &TemplateRecipe, s: &Snapshot) -> Vec<CapabilityCheck> {
    let d = &r.defaults;
    let (w, h) = (d.width.max(256), d.height.max(256));
    let base = Params::create("test", w, h, 1, d.steps.max(1), d.cfg);
    let mut out = Vec::new();
    let mut try_build = |name: &str, p: Params, references: u32| {
        let lacking = graph_of(r, name).map(missing_files).unwrap_or_default();
        if !lacking.is_empty() {
            out.push(CapabilityCheck { name: name.into(), ok: false, references, detail: format!("needs {}", lacking.join(", ")) });
            return;
        }
        let res = r.build(&p, &s.info);
        out.push(CapabilityCheck { name: name.into(), ok: res.is_ok(), references, detail: res.err().map(|e| format!("{e:#}")).unwrap_or_default() });
    };
    if r.create.is_some() || r.style.is_some() {
        try_build("create", Params { ..base }, 0);
    }
    let refs = r.reference_capacity() as u32;
    if refs > 0 {
        let name = if r.edit.is_none() && r.style.is_some() { "style" } else { "references" };
        try_build(name, Params { references: 1, ..base }, refs);
    }
    if r.edit.is_some() {
        try_build("edit", Params { task: Task::Edit, ..base }, r.edit.as_ref().map(|g| g.refs.max() as u32).unwrap_or(0).min(r.controls.references));
    }
    if r.enhancer_ready() {
        try_build("prompt_enhancer", Params { enhance: true, ..base }, 0);
    }
    try_build("refine", Params { task: Task::Refine { denoise: d.denoise.clamp(0.05, 1.0) }, ..base }, 0);
    try_build("inpaint", Params { task: Task::Inpaint { denoise: 1.0 }, ..base }, 0);
    if let Some(refine) = out.iter().find(|c| c.name == "refine").cloned() {
        out.push(CapabilityCheck { name: "upscale".into(), detail: if refine.ok { "uses Refine".into() } else { refine.detail }, ..refine });
    }
    out
}

/// GPU memory, in bytes, a model needs: its weights with working overhead, or its largest
/// companion if that is bigger, plus working memory for an image of `megapixels`.
pub fn memory_need(model_bytes: u64, largest_companion: u64, megapixels: f64) -> f64 {
    (model_bytes as f64 * 1.15).max(largest_companion as f64) + 1.5e9 * megapixels
}

/// Checks one template model.
pub fn check(m: &ModelCheck) -> Checked {
    let key = Some(m.key);
    let s = m.server;
    let r = m.recipe;
    let have = s.version_label();
    let mut issues = Vec::new();
    // Companions: one the main graph needs blocks the model; one only another capability's
    // graph needs turns that capability off.
    let main = r.create.as_ref().or(r.edit.as_ref()).or(r.style.as_ref());
    let mut missing = false;
    for b in r.bindings() {
        match (b.used.as_deref(), b.why) {
            (None, _) if b.optional => {
                let size = declared_size(m.runtime, &b.declared);
                let mut i = Issue::new(
                    "UpsamplerAvailable",
                    key,
                    &[("file", b.declared.clone()), ("size", size.map(size_text).unwrap_or_else(|| "size unknown".into()))],
                );
                if !s.remote && !b.url.is_empty() {
                    i = i.fix(Fix::Download { files: vec![b.declared.clone()], folder: b.folder.clone(), bytes: size.unwrap_or(0), url: b.url.clone() });
                }
                issues.push(i);
            }
            (Some(_), _) if b.optional => issues.push(Issue::new("UpsamplerInstalled", key, &[])),
            (None, _) => {
                let in_main = main.is_some_and(|g| g.files.iter().any(|x| x.declared == b.declared && x.role == b.role));
                missing |= in_main;
                let size = declared_size(m.runtime, &b.declared);
                let mut i = Issue::new(
                    "CompanionMissing",
                    key,
                    &[
                        ("model", m.label.to_owned()),
                        ("role_label", role_label(&b.role, false)),
                        ("file", b.declared.clone()),
                        ("size", size.map(size_text).unwrap_or_else(|| "size unknown".into())),
                        ("folder", b.folder.clone()),
                        ("stem", crate::stem::stem_key(&b.declared).display),
                    ],
                );
                if s.remote {
                    i = i.fix(Fix::CopyCommand { label: format!("Put it in {}/ on {}", b.folder, s.host), command: b.url.clone() });
                } else if !b.url.is_empty() {
                    i = i.fix(Fix::Download { files: vec![b.declared.clone()], folder: b.folder.clone(), bytes: size.unwrap_or(0), url: b.url.clone() });
                }
                if !in_main {
                    i.severity = super::issue::Severity::Warning;
                }
                issues.push(i);
            }
            (Some(used), Why::Equivalent) if b.role != "main" => issues.push(Issue::new(
                "UsingEquivalent",
                key,
                &[("model", m.label.to_owned()), ("declared", b.declared.clone()), ("installed", used.to_owned())],
            )),
            (Some(used), Why::WrongFolder) => issues.push(
                Issue::new(
                    "WrongFolder",
                    key,
                    &[
                        ("file", used.to_owned()),
                        ("kind_label", role_label(&b.role, false)),
                        ("kind_label_plural", role_label(&b.role, true)),
                        ("found", b.found_in.clone().unwrap_or_default()),
                        ("expected", b.folder.clone()),
                    ],
                )
                .fix(Fix::Move { from: used.to_owned(), to_folder: b.folder.clone() }),
            ),
            _ => {}
        }
    }
    // Templates this ComfyUI can't run (their capability is off; the model is blocked when none runs).
    let all_skipped = r.create.is_none() && r.edit.is_none() && r.style.is_none();
    let mut static_failed = false;
    for u in &r.skipped {
        static_failed |= !u.missing_classes.is_empty() || !u.not_offered.is_empty();
        let need = u.min_comfyui.as_deref().and_then(Version::parse);
        let mut found: Vec<Issue> = Vec::new();
        for class in &u.missing_classes {
            found.push(match m.runtime.pack_for(class) {
                Some(p) => Issue::new(
                    "CustomNodesMissing",
                    key,
                    &[("file", m.file.to_owned()), ("pack", p.name.clone()), ("manager", p.manager.clone()), ("repo", p.repo.clone())],
                )
                .fix(Fix::OpenUrl { label: "Custom nodes".into(), url: p.repo.clone() }),
                None => Issue::new(
                    "NodeMissing",
                    key,
                    &[("have", have.clone()), ("class", class.clone()), ("model", m.label.to_owned()), ("need_clause", need_clause(need))],
                ),
            });
        }
        for (class, input, value) in &u.not_offered {
            found.push(Issue::new(
                "ValueNotOffered",
                key,
                &[
                    ("have", have.clone()),
                    ("class", class.clone()),
                    ("input", input.clone()),
                    ("value", value.clone()),
                    ("model", m.label.to_owned()),
                    ("need_clause", need_clause(need)),
                ],
            ));
        }
        if found.is_empty() {
            found.push(Issue::new("TemplateUnusable", key, &[("template", u.template.clone()), ("detail", u.detail.clone())]));
        }
        for mut i in found {
            if !all_skipped {
                i.severity = super::issue::Severity::Warning;
            }
            issues.push(i);
        }
    }
    // Capabilities, built as a run builds them (only when every required file is there).
    let caps = if missing || all_skipped { Vec::new() } else { capabilities(r, s) };
    static_failed |= caps.iter().any(|c| !c.ok && c.name == "create");
    // Version gate: /object_info decides; the index's minimum only warns.
    let mins =
        r.sources.iter().map(|x| (x.name.as_str(), x.min_comfyui.as_deref())).chain(r.skipped.iter().map(|u| (u.template.as_str(), u.min_comfyui.as_deref())));
    let (template, need) = mins.filter_map(|(t, v)| Some((t.to_owned(), Version::parse(v?)?))).max_by_key(|(_, v)| *v).unzip();
    if let (Some(have_v), Some(need_v)) = (s.comfyui_version, need)
        && have_v < need_v
    {
        if static_failed {
            issues.push(
                Issue::new("ComfyTooOld", key, &[("model", m.label.to_owned()), ("need", need_v.to_string()), ("have", have.clone()), ("how", update_how(s))])
                    .fix(Fix::CopyCommand { label: "Update ComfyUI".into(), command: update_how(s) }),
            );
        } else {
            issues.push(Issue::new(
                "ComfyMayBeTooOld",
                key,
                &[("template", template.unwrap_or_default()), ("need", need_v.to_string()), ("have", have.clone())],
            ));
        }
    }
    for c in caps.iter().filter(|c| !c.ok && c.name != "upscale" && !c.detail.starts_with("needs ")) {
        let severity = if c.name == "create" && r.edit.is_none() { super::issue::Severity::Blocker } else { super::issue::Severity::Warning };
        let mut i = Issue::new("TemplateUnusable", key, &[("template", template_of(r, &c.name)), ("detail", c.detail.clone())]);
        i.severity = severity;
        issues.push(i);
    }
    // Precision and GPU.
    let gpu = s.gpu.as_ref().map(|g| g.name.clone());
    if let Some(class) = &m.identity.quant.class
        && let Some(rule) = m.runtime.quant_rule(class)
        && let Some(need) = &rule.needs
    {
        let generation = gpu.as_deref().and_then(|g| m.runtime.gpu_generation(g));
        if generation != Some(need.as_str()) {
            let args = [("file", m.file.to_owned()), ("quant", rule.label.clone()), ("gpu", gpu.clone().unwrap_or_else(|| "unknown".into()))];
            if rule.block && !m.try_anyway {
                let mut issue = Issue::new("QuantUnsupported", key, &args);
                // The precision the template itself declares, when this GPU runs it.
                let runs_here = |name: &str| {
                    let q = crate::stem::name_quant(&crate::stem::stem_key(name).quant_tokens);
                    q.and_then(|q| m.runtime.quant_rule(&q)).is_none_or(|r| !r.block || r.needs.as_deref().is_none_or(|n| generation == Some(n)))
                };
                let official = r.bindings().into_iter().find(|b| b.role == "main" && b.declared != m.file && !b.url.is_empty() && runs_here(&b.declared));
                if let Some(b) = official.filter(|_| !s.remote) {
                    let bytes = declared_size(m.runtime, &b.declared).unwrap_or(0);
                    issue = issue.fix(Fix::Download { files: vec![b.declared.clone()], folder: b.folder.clone(), bytes, url: b.url.clone() });
                }
                issues.push(issue.fix(Fix::TryAnyway));
            } else if !rule.block {
                issues.push(Issue::new("QuantSlow", key, &args));
            }
        }
    }
    // Memory: the largest of the model and its companions, plus working memory for the image.
    if let (Some(g), Some(bytes)) = (&s.gpu, m.bytes)
        && g.vram_total > 0
    {
        let companions = r.bindings().into_iter().filter(|b| !b.optional).filter_map(|b| (m.local_bytes)(&b.folder, b.used.as_deref()?)).max().unwrap_or(0);
        let mp = f64::from(r.defaults.width.max(512)) * f64::from(r.defaults.height.max(512)) / 1e6;
        let need = memory_need(bytes, companions, mp);
        if need > 0.95 * g.vram_total as f64 {
            issues.push(Issue::new(
                "VramLow",
                key,
                &[
                    ("model", m.label.to_owned()),
                    ("need", format!("{:.0}", need / 1e9)),
                    ("have", format!("{:.0}", g.vram_total as f64 / 1e9)),
                    ("alt_clause", String::new()),
                ],
            ));
        }
    }
    // Licence.
    match m.family.filter(|f| !f.license.is_empty()) {
        Some(f) => issues.push(Issue::new(
            "LicenceNotice",
            key,
            &[("licence", f.license.clone()), ("summary", f.license_summary.clone().unwrap_or_else(|| commercial_text(f.commercial).into()))],
        )),
        None => issues.push(Issue::new("LicenceUnknown", key, &[])),
    }
    Checked { issues, capabilities: caps }
}

/// Whether this ComfyUI can run the official template `entry` (`body`) before any of its files
/// is downloaded: the node classes and options it uses (its files aside) and its version gate.
/// Blockers mean a download would not make it run.
pub fn check_template(entry: &TemplateEntry, body: &serde_json::Value, s: &Snapshot, runtime: &Runtime) -> Vec<Issue> {
    let have = s.version_label();
    let label = if entry.title.is_empty() { entry.name.clone() } else { entry.title.clone() };
    let unusable = |detail: String| Issue::new("TemplateUnusable", None, &[("template", label.clone()), ("detail", detail)]);
    let mut problems = Vec::new();
    let mut issues = Vec::new();
    match crate::graph::ui::to_api(body, &s.info) {
        Err(e) => issues.push(unusable(format!("{e:#}"))),
        Ok(conv) if !conv.problems.is_empty() => issues.push(unusable(format!("node {}: {}", conv.problems[0].0, conv.problems[0].1))),
        Ok(conv) => {
            let zero: Vec<&str> = runtime.zero_classes.iter().map(String::as_str).collect();
            let stem = crate::template::index::declared_files(&entry.name, body).into_iter().find(|d| d.is_main()).map(|d| d.stem);
            let opts = crate::graph::canon::Options { info: &s.info, io_outputs: &entry.io_outputs, main_stem: stem.as_deref(), zero: &zero };
            match crate::graph::canon::canonicalise(&conv, &opts) {
                Err(why) => issues.push(unusable(why)),
                Ok(modes) => {
                    for c in &modes {
                        problems.extend(
                            crate::graph::validate::validate(&c.graph, &s.info)
                                .into_iter()
                                .filter(|p| !p.is_file() && !(p.class == "LoadImage" && p.input == "image")),
                        );
                    }
                }
            }
        }
    }
    let need = entry.min_comfyui;
    for p in &problems {
        let issue = match &p.kind {
            crate::graph::validate::ProblemKind::MissingClass => match runtime.pack_for(&p.class) {
                Some(pack) => Issue::new(
                    "CustomNodesMissing",
                    None,
                    &[("file", label.clone()), ("pack", pack.name.clone()), ("manager", pack.manager.clone()), ("repo", pack.repo.clone())],
                )
                .fix(Fix::OpenUrl { label: "Custom nodes".into(), url: pack.repo.clone() }),
                None => Issue::new(
                    "NodeMissing",
                    None,
                    &[("have", have.clone()), ("class", p.class.clone()), ("model", label.clone()), ("need_clause", need_clause(need))],
                ),
            },
            crate::graph::validate::ProblemKind::NotOffered(value) => Issue::new(
                "ValueNotOffered",
                None,
                &[
                    ("have", have.clone()),
                    ("class", p.class.clone()),
                    ("input", p.input.clone()),
                    ("value", value.clone()),
                    ("model", label.clone()),
                    ("need_clause", need_clause(need)),
                ],
            ),
            _ => unusable(p.describe()),
        };
        if !issues.iter().any(|i: &Issue| i.code == issue.code && i.args == issue.args) {
            issues.push(issue);
        }
    }
    if let (Some(have_v), Some(need_v)) = (s.comfyui_version, need)
        && have_v < need_v
    {
        if issues.is_empty() {
            issues.push(Issue::new("ComfyMayBeTooOld", None, &[("template", label.clone()), ("need", need_v.to_string()), ("have", have.clone())]));
        } else {
            issues.insert(
                0,
                Issue::new("ComfyTooOld", None, &[("model", label.clone()), ("need", need_v.to_string()), ("have", have.clone()), ("how", update_how(s))])
                    .fix(Fix::CopyCommand { label: "Update ComfyUI".into(), command: update_how(s) }),
            );
        }
    }
    issues
}

fn commercial_text(c: Commercial) -> &'static str {
    match c {
        Commercial::Yes => "Commercial use allowed.",
        Commercial::No => "Not for commercial use.",
        Commercial::Limited => "Commercial use has conditions; read the licence.",
        Commercial::Unknown => "Check the licence before client work.",
    }
}

fn template_of(r: &TemplateRecipe, capability: &str) -> String {
    let g = match capability {
        "edit" => r.edit.as_ref(),
        "style" => r.style.as_ref(),
        _ => r.create.as_ref().or(r.edit.as_ref()),
    };
    g.map(|g| g.template.clone()).unwrap_or_default()
}

/// A declared file's size, when the runtime data knows it (mirrors carry sizes).
fn declared_size(rt: &Runtime, name: &str) -> Option<u64> {
    rt.mirror(name).map(|m| m.bytes)
}

/// Capability name → ok, for the report.
pub fn capability_map(caps: &[CapabilityCheck]) -> BTreeMap<String, bool> {
    caps.iter().map(|c| (c.name.clone(), c.ok)).collect()
}
