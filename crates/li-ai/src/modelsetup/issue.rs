//! What model setup found, in plain English: every finding is an [`Issue`] with a stable code,
//! a severity, the arguments of its sentence and the fixes on offer. The sentences are the
//! English source strings of the message catalogue; the UI translates [`Issue::template`] and
//! fills it with [`fill`], the CLI prints [`Issue::english`].

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Blocker,
    Warning,
    #[default]
    Info,
}

impl Severity {
    pub fn label(self) -> &'static str {
        match self {
            Self::Blocker => "blocker",
            Self::Warning => "warn",
            Self::Info => "info",
        }
    }
}

/// A way to resolve an issue. The app performs the actions; the CLI prints them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum Fix {
    /// Download the files (verified by their published SHA-256 or size).
    Download {
        files: Vec<String>,
        folder: String,
        bytes: u64,
        url: String,
    },
    UseInstead {
        declared: String,
        installed: String,
    },
    Move {
        from: String,
        to_folder: String,
    },
    ChooseTemplate {
        candidates: Vec<String>,
    },
    OpenUrl {
        label: String,
        url: String,
    },
    /// A command shown for the person to run; never executed.
    CopyCommand {
        label: String,
        command: String,
    },
    UpdateProfiles,
    CheckOnline,
    Recheck,
    Verify {
        model: String,
    },
    TryAnyway,
    Ignore,
}

impl Fix {
    /// One line for the CLI and the report.
    pub fn describe(&self) -> String {
        match self {
            Self::Download { files, folder, bytes, .. } => {
                let size = if *bytes > 0 { size_text(*bytes) } else { "size unknown".into() };
                format!("download {} ({size}) into {folder}/", files.join(", "))
            }
            Self::UseInstead { declared, installed } => format!("use {installed} instead of {declared}"),
            Self::Move { from, to_folder } => format!("move {from} to {to_folder}/"),
            Self::ChooseTemplate { candidates } => format!("choose a template: {}", candidates.join(", ")),
            Self::OpenUrl { label, url } => format!("{label}: {url}"),
            Self::CopyCommand { label, command } => format!("{label}: {command}"),
            Self::UpdateProfiles => "update Model Profiles".into(),
            Self::CheckOnline => "check online for a newer ComfyUI (local-image-cli ai doctor --online)".into(),
            Self::Recheck => "press Re-check (or run local-image-cli ai doctor again)".into(),
            Self::Verify { model } => format!("local-image-cli ai doctor --verify {model}   (or Generate › Set Up Models)"),
            Self::TryAnyway => "try anyway".into(),
            Self::Ignore => "ignore this file".into(),
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Self::Download { .. } => "download",
            Self::UseInstead { .. } => "use_instead",
            Self::Move { .. } => "move",
            Self::ChooseTemplate { .. } => "choose_template",
            Self::OpenUrl { .. } => "open_url",
            Self::CopyCommand { .. } => "copy_command",
            Self::UpdateProfiles => "update_profiles",
            Self::CheckOnline => "check_online",
            Self::Recheck => "recheck",
            Self::Verify { .. } => "verify",
            Self::TryAnyway => "try_anyway",
            Self::Ignore => "ignore",
        }
    }
}

/// `7.8 GB`, `512 MB`.
pub fn size_text(bytes: u64) -> String {
    if bytes >= 1_000_000_000 { format!("{:.1} GB", bytes as f64 / 1e9) } else { format!("{} MB", (bytes as f64 / 1e6).round().max(1.0)) }
}

/// The message catalogue: code, severity, English source string.
pub const CATALOGUE: &[(&str, Severity, &str)] = &[
    ("Unavailable", Severity::Blocker, "{reason}"),
    ("ServerUnreachable", Severity::Blocker, "ComfyUI is not reachable at {host}. Start it (Local AI › Start), then press Re-check."),
    ("VersionUnknown", Severity::Info, "This ComfyUI doesn't report its version, so version requirements can't be checked."),
    (
        "TemplatesMismatch",
        Severity::Info,
        "ComfyUI's template package ({installed}) differs from the one this ComfyUI expects ({required}). Update ComfyUI so its nodes match its templates.",
    ),
    (
        "TemplateIndexUnavailable",
        Severity::Warning,
        "This ComfyUI doesn't serve its templates (/templates), so models without a profile can't be matched. Update ComfyUI, or press Check online.",
    ),
    ("WrongFolder", Severity::Blocker, "{file} is a {kind_label}, but it is in {found}/. ComfyUI loads {kind_label_plural} from {expected}/."),
    (
        "NoTemplate",
        Severity::Blocker,
        "Local Image couldn't match {file} to an official ComfyUI template on this server. Choose a template, check online for a newer ComfyUI, or update Model Profiles.",
    ),
    (
        "UnsupportedGeneration",
        Severity::Blocker,
        "{file} looks like {generation}, which no profile or template on this server describes yet. Check online for a newer ComfyUI, or update Model Profiles.",
    ),
    (
        "TemplateUnusable",
        Severity::Blocker,
        "The official template {template} uses something Local Image can't run yet ({detail}). Update Local Image, or choose another template.",
    ),
    ("CompanionMissing", Severity::Blocker, "{model} needs the {role_label} {file} ({size}) in {folder}/. Any precision of {stem} works."),
    ("UsingEquivalent", Severity::Info, "{model} expects {declared}. Using {installed}, which decodes the same format."),
    (
        "UnverifiableDownload",
        Severity::Blocker,
        "{file} has neither a published checksum nor a known size, so Local Image won't download it. Put it in {folder}/ yourself, then press Re-check.",
    ),
    ("ComfyTooOld", Severity::Blocker, "{model} needs ComfyUI {need} or newer. This server runs {have}. {how}"),
    (
        "ComfyMayBeTooOld",
        Severity::Warning,
        "{template} was made for ComfyUI {need} or newer; this server runs {have}. It may still work: verify it to find out, or update ComfyUI.",
    ),
    ("NodeMissing", Severity::Blocker, "This ComfyUI ({have}) doesn't have the node {class} that {model} uses. Update ComfyUI{need_clause}."),
    (
        "ValueNotOffered",
        Severity::Blocker,
        "This ComfyUI ({have}) doesn't offer {class} {input} \"{value}\", which {model} needs. Update ComfyUI{need_clause}.",
    ),
    (
        "CustomNodesMissing",
        Severity::Blocker,
        "{file} needs the {pack} custom nodes, which this ComfyUI doesn't have. Install them with ComfyUI-Manager (search \"{manager}\") or clone {repo} into ComfyUI/custom_nodes, restart ComfyUI, then press Re-check.",
    ),
    ("QuantUnsupported", Severity::Blocker, "{file} uses {quant}, which needs an NVIDIA Blackwell (RTX 50-series) GPU. This GPU is {gpu}."),
    ("QuantSlow", Severity::Warning, "{file} uses {quant}. It runs fastest on Blackwell GPUs and may be slower on {gpu}."),
    ("VramLow", Severity::Warning, "{model} needs about {need} GB of GPU memory; this GPU has {have} GB. It may run slowly.{alt_clause}"),
    ("LicenceNotice", Severity::Info, "Licence: {licence}. {summary}"),
    ("LicenceUnknown", Severity::Info, "Licence unknown - check before client work."),
    ("RemoteServer", Severity::Info, "This ComfyUI runs on another computer ({host}). Install the files there: {list}."),
    ("UpsamplerAvailable", Severity::Info, "This model can use an optional prompt enhancer ({file}, {size}). Turn on Enhance prompt in Generate to use it."),
    ("UpsamplerInstalled", Severity::Info, "Prompt enhancer: installed (off by default)."),
    ("NotVerified", Severity::Info, "{model} hasn't been tested on this computer yet."),
    ("Busy", Severity::Info, "ComfyUI was busy for {minutes} minutes, so nothing was tested. Verify again when its queue is empty."),
    ("Stalled", Severity::Blocker, "ComfyUI stopped processing prompts: a test waited over a minute while nothing ran. Restart ComfyUI, then verify again."),
    ("VerifyFailed:wrong_size", Severity::Blocker, "The {capability} test image came back {got} instead of {want}."),
    (
        "VerifyFailed:black",
        Severity::Blocker,
        "The {capability} test image came out black. This usually means the model and its text encoder or VAE don't match, or this GPU can't run {quant}.",
    ),
    ("VerifyFailed:white", Severity::Blocker, "The {capability} test image came out blank white. The VAE ({vae}) probably doesn't match {model}."),
    (
        "VerifyFailed:flat",
        Severity::Blocker,
        "The {capability} test image is a single flat colour, not a picture. Check the text encoder ({te}) and the VAE ({vae}).",
    ),
    ("VerifyFailed:noise", Severity::Blocker, "The {capability} test image is noise, not a picture. The VAE ({vae}) probably doesn't match this model."),
    (
        "VerifyFailed:refine_drift",
        Severity::Blocker,
        "Refine changed the test image too much (difference {difference}; at most 35 is allowed). Refine tests the model again when you choose it and won't run it if it drifts again.",
    ),
    ("VerifyFailed:edit_nochange", Severity::Blocker, "The Edit test returned the picture unchanged."),
    ("VerifyFailed:inpaint_nochange", Severity::Blocker, "Fill didn't change the selected area in the test."),
    (
        "VerifyFailed:inpaint_outside",
        Severity::Blocker,
        "Fill changed the picture outside the selected area in the test (difference {outside}; at most 8 is allowed).",
    ),
    ("VerifyFailed:error", Severity::Blocker, "The {capability} test failed: {message}"),
    ("VerifyTurnedOff", Severity::Info, "{capability} is turned off for this model until it passes its test."),
    ("VerifyTimeout", Severity::Blocker, "The {capability} test took longer than 10 minutes and was stopped. Try again when the GPU is free."),
    (
        "VerifyWarning:prompt",
        Severity::Warning,
        "The test image doesn't show what was asked (a red ball on white). The text encoder ({te}) may not match this model; check its results.",
    ),
    ("VerifyWarning:edit", Severity::Warning, "The Edit test didn't make the requested change. Edit may not follow instructions well with this model."),
    ("VerifyWarning:fill", Severity::Warning, "The Fill test didn't paint what was asked (a red ball). Fill may not follow the prompt well with this model."),
    ("ReverifyNeeded", Severity::Warning, "ComfyUI was updated from {old} to {new}. {model} needs testing again."),
    ("StillValid", Severity::Info, "Still valid on ComfyUI {new}."),
    ("UsingLastGood", Severity::Warning, "Using the recipe verified on ComfyUI {old}; the new template could not be read: {reason}."),
    ("FileChanged", Severity::Warning, "{file} changed since it was tested on {date}. Verify it again."),
    ("RecipeChanged", Severity::Warning, "Its settings changed since the test (a model profile or Local Image update). Verify it again."),
    ("OtherGpu", Severity::Info, "Tested on {old_gpu}; now running on {new_gpu}."),
    ("ForeignLora", Severity::Info, "{file} is a LoRA for {base}, which Local Image doesn't run. It is hidden from the style lists."),
    (
        "ProfileNeedsNewerApp",
        Severity::Warning,
        "The updated {family} profile needs a newer Local Image (profile format {schema}). The built-in profile is used until you update.",
    ),
    ("ProfileRejected", Severity::Warning, "The {family} profile update was rejected: {reason}. The previous copy is still in use."),
];

/// One finding.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Issue {
    pub code: String,
    pub severity: Severity,
    /// The model key it is about.
    pub model: Option<String>,
    pub args: BTreeMap<String, String>,
    pub fixes: Vec<Fix>,
}

impl Issue {
    /// An issue of a catalogue code (the severity comes from the catalogue).
    pub fn new(code: &str, model: Option<&str>, args: &[(&str, String)]) -> Self {
        let severity = CATALOGUE.iter().find(|(c, _, _)| *c == code).map(|(_, s, _)| *s).unwrap_or(Severity::Blocker);
        Self {
            code: code.to_owned(),
            severity,
            model: model.map(str::to_owned),
            args: args.iter().map(|(k, v)| ((*k).to_owned(), v.clone())).collect(),
            fixes: Vec::new(),
        }
    }

    pub fn fix(mut self, fix: Fix) -> Self {
        self.fixes.push(fix);
        self
    }

    /// The English source string.
    pub fn template(&self) -> &'static str {
        CATALOGUE.iter().find(|(c, _, _)| *c == self.code).map(|(_, _, t)| *t).unwrap_or("{message}")
    }

    /// The filled English sentence.
    pub fn english(&self) -> String {
        fill(self.template(), &self.args)
    }
}

/// Replaces `{name}` placeholders by `args` (a missing argument is left empty).
pub fn fill(template: &str, args: &BTreeMap<String, String>) -> String {
    let mut out = String::with_capacity(template.len() + 32);
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let Some(close) = rest[open..].find('}') else {
            out.push_str(&rest[open..]);
            return out;
        };
        let key = &rest[open + 1..open + close];
        out.push_str(args.get(key).map(String::as_str).unwrap_or(""));
        rest = &rest[open + close + 1..];
    }
    out.push_str(rest);
    out
}

/// "text encoder", "VAE" (singular or plural).
pub fn role_label(role: &str, plural: bool) -> String {
    let base = match role {
        "clip" | "text_encoders" => "text encoder",
        "vae" => "VAE",
        "lora" | "loras" => "LoRA",
        "upsampler" => "prompt enhancer",
        "main" | "diffusion_models" | "unet" => "diffusion model",
        "checkpoints" => "checkpoint",
        "style_model" => "style model",
        "model_patches" => "model patch",
        other => other,
    };
    if plural { format!("{base}s") } else { base.to_owned() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn busy_display_says_nothing_was_tested_and_how_to_retry() {
        let minutes = crate::modelsetup::verify::BUSY_WAIT.as_secs() / 60;
        let message = crate::modelsetup::Busy.to_string();
        assert_eq!(message, format!("ComfyUI was busy for {minutes} minutes, so nothing was tested. Verify again when its queue is empty."));
        assert!(!message.contains('{'));
    }

    #[test]
    fn issues_fill_their_catalogue_sentence() {
        let i = Issue::new(
            "CompanionMissing",
            Some("unet:x"),
            &[
                ("model", "FLUX.3 Klein".into()),
                ("role_label", role_label("clip", false)),
                ("file", "qwen3vl_4b_int8_convrot.safetensors".into()),
                ("size", size_text(4_400_000_000)),
                ("folder", "text_encoders".into()),
                ("stem", "qwen3vl_4b".into()),
            ],
        );
        assert_eq!(i.severity, Severity::Blocker);
        assert_eq!(
            i.english(),
            "FLUX.3 Klein needs the text encoder qwen3vl_4b_int8_convrot.safetensors (4.4 GB) in text_encoders/. Any precision of qwen3vl_4b works."
        );
        assert_eq!(Issue::new("NotVerified", None, &[("model", "M".into())]).severity, Severity::Info);
        assert_eq!(fill("a {x} b {missing}.", &[("x".to_owned(), "1".to_owned())].into()), "a 1 b .");
        assert_eq!(size_text(512_000_000), "512 MB");
        let download = |bytes| Fix::Download { files: vec!["x.safetensors".into()], folder: "text_encoders".into(), bytes, url: String::new() }.describe();
        assert_eq!(download(0), "download x.safetensors (size unknown) into text_encoders/");
        assert_eq!(download(9_350_798_360), "download x.safetensors (9.4 GB) into text_encoders/");
    }

    #[test]
    fn every_catalogue_code_is_unique_and_its_placeholders_close() {
        let mut seen = std::collections::BTreeSet::new();
        for (code, _, text) in CATALOGUE {
            assert!(seen.insert(*code), "duplicate {code}");
            assert_eq!(text.matches('{').count(), text.matches('}').count(), "{code}");
        }
    }
}
