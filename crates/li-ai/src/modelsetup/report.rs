//! The setup report: what `ai doctor --json` prints and what the text report (the CLI, and the
//! wizard's Copy report) renders.

use serde::{Deserialize, Serialize};

use super::issue::{Issue, Severity};

pub const REPORT_VERSION: u32 = 1;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GpuReport {
    pub name: String,
    pub vram_gb: f64,
    pub generation: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerReport {
    pub host: String,
    pub reachable: bool,
    pub remote: bool,
    pub comfyui_version: Option<String>,
    pub templates_version: Option<String>,
    pub required_templates_version: Option<String>,
    pub deploy_environment: Option<String>,
    pub gpu: Option<GpuReport>,
    pub model_dir: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CapabilityReport {
    pub name: String,
    /// `verified`, `ready`, `warning`, `blocked`, `unsupported` or `failed`.
    pub state: String,
    pub detail: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BindingReport {
    pub role: String,
    pub used: Option<String>,
    pub declared: String,
    pub why: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LicenceReport {
    pub name: String,
    pub summary: String,
    pub commercial: String,
    pub url: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelReport {
    pub key: String,
    pub file: String,
    pub folder: String,
    pub bytes: Option<u64>,
    pub label: String,
    pub arch: Option<String>,
    pub variant: Option<String>,
    pub quant: Option<String>,
    /// `builtin`, `builder`, `template`, `universal` or `unsupported`.
    pub engine: String,
    pub templates: Vec<String>,
    pub status: String,
    pub capabilities: Vec<CapabilityReport>,
    pub bindings: Vec<BindingReport>,
    pub verified_at: Option<String>,
    pub seconds: Option<f64>,
    pub peak_vram_gb: Option<f64>,
    pub licence: Option<LicenceReport>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LoraReport {
    pub file: String,
    pub arch: Option<String>,
    pub status: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FileReport {
    pub file: String,
    pub kind: String,
    pub used_by: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FixReport {
    pub kind: String,
    pub label: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct IssueReport {
    pub code: String,
    pub severity: Severity,
    pub model: Option<String>,
    pub message: String,
    pub fixes: Vec<FixReport>,
}

impl From<&Issue> for IssueReport {
    fn from(i: &Issue) -> Self {
        Self {
            code: i.code.clone(),
            severity: i.severity,
            model: i.model.clone(),
            message: i.english(),
            fixes: i.fixes.iter().map(|f| FixReport { kind: f.kind().into(), label: f.describe() }).collect(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Summary {
    pub usable: usize,
    pub verified: usize,
    pub not_tested: usize,
    pub needs_attention: usize,
    pub not_usable: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Report {
    pub report_version: u32,
    pub generated_at: String,
    pub server: ServerReport,
    pub models: Vec<ModelReport>,
    pub loras: Vec<LoraReport>,
    pub other_files: Vec<FileReport>,
    pub issues: Vec<IssueReport>,
    pub summary: Summary,
}

impl Report {
    pub fn blockers(&self) -> usize {
        self.issues.iter().filter(|i| i.severity == Severity::Blocker).count()
    }

    pub fn failures(&self) -> usize {
        self.models.iter().filter(|m| m.status == "failed").count()
    }

    /// Counts the models by state.
    pub fn summarise(&mut self) {
        let mut s = Summary::default();
        for m in &self.models {
            match m.status.as_str() {
                "verified" | "builtin" => {
                    s.usable += 1;
                    if m.status == "verified" {
                        s.verified += 1;
                    }
                }
                "ready" | "partially_verified" => {
                    s.usable += 1;
                    if m.status == "ready" {
                        s.not_tested += 1;
                    } else {
                        s.needs_attention += 1;
                    }
                }
                "needs_reverify" => {
                    s.usable += 1;
                    s.needs_attention += 1;
                }
                _ => s.not_usable += 1,
            }
        }
        self.summary = s;
    }
}

fn status_word(status: &str) -> &'static str {
    match status {
        "verified" => "VERIFIED",
        "builtin" => "OK",
        "ready" => "READY",
        "partially_verified" => "PARTIAL",
        "needs_reverify" => "RECHECK",
        "failed" => "FAILED",
        "blocked" => "BLOCKED",
        "unmatched" => "NO MATCH",
        "ignored" => "IGNORED",
        _ => "NEW",
    }
}

fn capability_label(c: &CapabilityReport) -> String {
    let name = match c.name.as_str() {
        "create" => "Create",
        "edit" => "Edit",
        "references" => "References",
        "style" => "Style references",
        "prompt_enhancer" => "Prompt enhancer",
        "refine" => "Refine",
        "inpaint" => "Fill",
        "upscale" => "Upscale",
        other => other,
    };
    let state = if matches!(c.state.as_str(), "ready" | "verified") { String::new() } else { format!(" ({})", c.state) };
    if matches!(c.name.as_str(), "references" | "style" | "edit") && c.detail.chars().all(|x| x.is_ascii_digit()) && !c.detail.is_empty() {
        return format!("{name} {}{state}", c.detail);
    }
    format!("{name}{state}")
}

/// The plain-text report.
pub fn render_text(r: &Report) -> String {
    let mut o = String::new();
    let s = &r.server;
    o.push_str(&format!("Local Image AI doctor - {}\n", r.generated_at));
    if s.reachable {
        let templates = s.templates_version.as_deref().map(|v| format!("  templates {v} from the server")).unwrap_or_default();
        let env = s.deploy_environment.as_deref().map(|e| format!(" ({e})")).unwrap_or_default();
        o.push_str(&format!("ComfyUI  {}  v{}{env}{templates}\n", s.host, s.comfyui_version.as_deref().unwrap_or("unknown")));
    } else {
        o.push_str(&format!("ComfyUI  {}  not reachable\n", s.host));
    }
    if let Some(g) = &s.gpu {
        let generation = g.generation.as_deref().map(|x| format!(" ({}{})", x[..1].to_ascii_uppercase(), &x[1..])).unwrap_or_default();
        o.push_str(&format!("GPU      {}, {:.1} GB{generation}\n", g.name, g.vram_gb));
    }
    if let Some(d) = &s.model_dir {
        o.push_str(&format!("Models   {d}{}\n", if s.remote { " (another computer)" } else { " (this computer)" }));
    }
    o.push_str(&format!("\nMODELS ({})\n", r.models.len()));
    for m in &r.models {
        let engine = match m.engine.as_str() {
            "builtin" => "built-in",
            e => e,
        };
        o.push_str(&format!("  {:<8} {:<38} {:<10} {}/{}\n", status_word(&m.status), m.label, engine, m.folder, m.file));
        if !m.templates.is_empty() {
            o.push_str(&format!("{:>11}{}\n", "", m.templates.join(" + ")));
        }
        let uses: Vec<String> = m
            .bindings
            .iter()
            .filter(|b| b.role != "main")
            .filter_map(|b| {
                let used = b.used.as_deref()?;
                let stem = used.trim_end_matches(".safetensors").trim_end_matches(".gguf");
                let extra = if b.why == "equivalent" { format!(" (stands in for {})", b.declared.trim_end_matches(".safetensors")) } else { String::new() };
                Some(format!("{} {stem}{extra}", super::issue::role_label(&b.role, false)))
            })
            .collect();
        if !uses.is_empty() {
            o.push_str(&format!("{:>11}uses {}\n", "", uses.join(", ")));
        }
        if !m.capabilities.is_empty() {
            let shown: Vec<&CapabilityReport> = m.capabilities.iter().filter(|c| c.state != "unsupported").collect();
            let labels = |caps: &[&CapabilityReport]| caps.iter().map(|c| capability_label(c)).collect::<Vec<_>>().join(", ");
            // A verified model names the tools its test didn't render.
            let (untested, tested): (Vec<&CapabilityReport>, Vec<&CapabilityReport>) = shown.iter().partition(|c| c.state == "ready");
            let line = match m.status.as_str() {
                "ready" => format!("{} - not tested", labels(&shown)),
                _ if tested.is_empty() => labels(&untested),
                status => {
                    let verified = if status == "verified" { " - verified" } else { "" };
                    let rest = if untested.is_empty() { String::new() } else { format!("; not tested: {}", labels(&untested)) };
                    format!("{}{verified}{rest}", labels(&tested))
                }
            };
            o.push_str(&format!("{:>11}{line}\n", ""));
        }
        if let (Some(at), Some(sec)) = (&m.verified_at, m.seconds) {
            let vram = m.peak_vram_gb.map(|v| format!(", GPU memory in use up to {v:.1} GB")).unwrap_or_default();
            o.push_str(&format!("{:>11}tested {at}, create {sec:.1} s{vram}\n", ""));
        }
    }
    if !r.loras.is_empty() {
        o.push_str("\nLORAS\n");
        for l in &r.loras {
            o.push_str(&format!("  {:<6} {:<48} {}\n", l.status, l.file, l.arch.as_deref().unwrap_or("unknown")));
        }
    }
    if !r.other_files.is_empty() {
        o.push_str("\nOTHER FILES\n");
        for f in &r.other_files {
            let used = if f.used_by.is_empty() { String::new() } else { format!(" ({})", f.used_by.join(", ")) };
            o.push_str(&format!("         {:<48} {}{used}\n", f.file, f.kind));
        }
    }
    if !r.issues.is_empty() {
        o.push_str(&format!("\nISSUES ({})\n", r.issues.len()));
        // An issue about a model names it, unless its sentence already does.
        let label = |key: &str| r.models.iter().find(|m| m.key == key || m.file == key).map(|m| m.label.as_str());
        for (n, i) in r.issues.iter().enumerate() {
            let about = i.model.as_deref().and_then(label).filter(|l| !l.is_empty() && !i.message.contains(*l)).map(|l| format!("{l}: ")).unwrap_or_default();
            o.push_str(&format!("  {}. [{}] {about}{}\n", n + 1, i.severity.label(), i.message));
            for f in &i.fixes {
                o.push_str(&format!("     fix: {}\n", f.label));
            }
        }
    }
    let su = &r.summary;
    o.push_str(&format!(
        "\nSummary: {} usable ({} verified, {} not tested, {} need attention), {} not usable, {} blocker{}.\n",
        su.usable,
        su.verified,
        su.not_tested,
        su.needs_attention,
        su.not_usable,
        r.blockers(),
        if r.blockers() == 1 { "" } else { "s" }
    ));
    if su.not_tested > 0 {
        o.push_str(&format!(
            "Run `local-image-cli ai doctor --verify all` to test the {} untested model{} (one at a time).\n",
            su.not_tested,
            if su.not_tested == 1 { "" } else { "s" }
        ));
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_text_report_lists_models_issues_and_a_summary() {
        let mut r = Report {
            report_version: REPORT_VERSION,
            generated_at: "2026-10-24 14:10 -04:00".into(),
            server: ServerReport { host: "127.0.0.1:8189".into(), reachable: true, comfyui_version: Some("0.42.5".into()), ..Default::default() },
            models: vec![ModelReport {
                key: "unet:flux3.safetensors".into(),
                file: "flux3.safetensors".into(),
                folder: "diffusion_models".into(),
                label: "FLUX.3 Klein · distilled · BF16".into(),
                engine: "template".into(),
                status: "ready".into(),
                templates: vec!["image_flux3_klein_distilled_t2i".into()],
                capabilities: vec![
                    CapabilityReport { name: "create".into(), state: "ready".into(), ..Default::default() },
                    CapabilityReport { name: "edit".into(), state: "ready".into(), detail: "10".into() },
                ],
                ..Default::default()
            }],
            issues: vec![IssueReport {
                code: "NotVerified".into(),
                severity: Severity::Info,
                message: "M hasn't been tested on this computer yet.".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        r.summarise();
        let t = render_text(&r);
        assert!(t.contains("READY    FLUX.3 Klein · distilled · BF16"), "{t}");
        assert!(t.contains("Create, Edit 10 - not tested"), "{t}");
        assert!(t.contains("1. [info] M hasn't been tested"), "{t}");
        assert!(t.contains("Summary: 1 usable (0 verified, 1 not tested, 0 need attention), 0 not usable, 0 blockers."), "{t}");
        let back: Report = serde_json::from_str(&serde_json::to_string(&r).unwrap()).unwrap();
        assert_eq!(back, r);
        // A verified model doesn't claim the tools its test didn't render.
        let cap = |name: &str, state: &str, detail: &str| CapabilityReport { name: name.into(), state: state.into(), detail: detail.into() };
        r.models[0].status = "verified".into();
        r.models[0].capabilities =
            vec![cap("create", "verified", ""), cap("style", "ready", "3"), cap("refine", "verified", ""), cap("upscale", "verified", "uses Refine")];
        let t = render_text(&r);
        assert!(t.contains("Create, Refine, Upscale - verified; not tested: Style references 3\n"), "{t}");
        // A partly verified model marks the tool that failed, and the failure names the model.
        r.models[0].status = "partially_verified".into();
        r.models[0].capabilities = vec![cap("create", "verified", ""), cap("inpaint", "failed", ""), cap("style", "blocked", "3")];
        r.issues = vec![IssueReport {
            code: "VerifyFailed:black".into(),
            severity: Severity::Blocker,
            model: Some("unet:flux3.safetensors".into()),
            message: "The Fill test image came out black.".into(),
            ..Default::default()
        }];
        let t = render_text(&r);
        assert!(t.contains("Create, Fill (failed), Style references 3 (blocked)\n"), "{t}");
        assert!(t.contains("1. [blocker] FLUX.3 Klein · distilled · BF16: The Fill test image came out black.\n"), "{t}");
    }
}
