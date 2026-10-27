//! Which official templates fit an installed file. Templates name one precision of a model; a
//! file matches a template's main model when its name is the same (exact), the same model at
//! another precision (equal stem keys), a stem the architecture's profile lists for its variant
//! (renamed files, community quantisations), the same network as a file already verified with
//! that template (equal header signatures), or the user's choice.

use serde::{Deserialize, Serialize};

use super::index::{DeclaredFile, TemplateEntry, Version};
use crate::arch::FileKind;
use crate::comfy::normalized_name;
use crate::family::TemplatePolish;
use crate::identify::Identity;

/// What a template is used for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    Create,
    Edit,
    Style,
}

impl Capability {
    pub fn id(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Edit => "edit",
            Self::Style => "style",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "create" => Self::Create,
            "edit" => Self::Edit,
            "style" => Self::Style,
            _ => return None,
        })
    }
}

/// How strongly a template matches.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    Exact,
    Stem,
    Variant,
    Signature,
    Manual,
}

/// A template's capability from its index tags.
pub fn capability(entry: &TemplateEntry) -> Option<Capability> {
    let has = |t: &str| entry.tags.iter().any(|x| x.eq_ignore_ascii_case(t));
    if has("Style Reference") {
        Some(Capability::Style)
    } else if has("Image Edit") || has("Edit") || has("Image to Image") {
        Some(Capability::Edit)
    } else if has("Text to Image") {
        Some(Capability::Create)
    } else {
        None
    }
}

/// A template on offer with the files it declares.
pub struct Candidate<'a> {
    pub entry: &'a TemplateEntry,
    pub declared: &'a [DeclaredFile],
}

/// One template that fits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Match {
    pub template: String,
    pub capability: Capability,
    pub tier: Tier,
    pub declared: DeclaredFile,
}

/// What the file is and what the user chose.
pub struct Query<'a> {
    pub file: &'a str,
    pub identity: &'a Identity,
    /// The template polish of the file's architecture.
    pub polish: Option<&'a TemplatePolish>,
    /// Other families' pinned templates (a pin by another family means the weights contradict
    /// the template).
    pub foreign_pins: &'a [String],
    /// Templates already verified with a file of the same network signature.
    pub twins: &'a [String],
    pub manual: Option<&'a str>,
    pub server: Option<Version>,
}

fn folder_fits(kind: FileKind, folder: &str) -> bool {
    match kind {
        FileKind::Checkpoint => folder == "checkpoints",
        _ => matches!(folder, "diffusion_models" | "unet" | "unet_gguf"),
    }
}

fn pinned(polish: Option<&TemplatePolish>, name: &str) -> Option<Capability> {
    polish?.templates.iter().find(|(_, names)| names.iter().any(|n| n == name)).and_then(|(cap, _)| Capability::parse(cap))
}

/// Every fitting template, best first within each capability.
pub fn find(q: &Query, candidates: &[Candidate]) -> Vec<Match> {
    let key = crate::stem::stem_key(q.file).key;
    let variant_stems: Vec<String> = match (q.polish, &q.identity.variant) {
        (Some(p), Some(v)) => p.stems.get(v).cloned().unwrap_or_default(),
        _ => Vec::new(),
    };
    let mut out: Vec<(Match, (bool, bool, bool, bool, std::cmp::Reverse<String>, String))> = Vec::new();
    for c in candidates {
        if q.foreign_pins.contains(&c.entry.name) {
            continue;
        }
        let Some(cap) = pinned(q.polish, &c.entry.name).or_else(|| capability(c.entry)) else { continue };
        let best = c
            .declared
            .iter()
            .filter(|d| d.is_main() && folder_fits(q.identity.kind, &d.folder))
            .filter_map(|d| {
                let tier = if normalized_name(&d.name) == normalized_name(q.file) {
                    Tier::Exact
                } else if d.stem == key {
                    Tier::Stem
                } else if variant_stems.contains(&d.stem) {
                    Tier::Variant
                } else if q.twins.contains(&c.entry.name) {
                    Tier::Signature
                } else if q.manual == Some(c.entry.name.as_str()) {
                    Tier::Manual
                } else {
                    return None;
                };
                Some((tier, d))
            })
            .min_by_key(|(t, _)| *t);
        let Some((tier, declared)) = best else { continue };
        let declared_quant = crate::stem::name_quant(&crate::stem::stem_key(&declared.name).quant_tokens);
        let rank = (
            pinned(q.polish, &c.entry.name).is_none(),
            declared_quant != q.identity.quant.class,
            c.entry.name.contains("bbox"),
            match (q.server, c.entry.min_comfyui) {
                (Some(have), Some(need)) => have < need,
                _ => false,
            },
            std::cmp::Reverse(c.entry.date.clone()),
            c.entry.name.clone(),
        );
        out.push((Match { template: c.entry.name.clone(), capability: cap, tier, declared: declared.clone() }, rank));
    }
    out.sort_by(|(a, ra), (b, rb)| a.capability.cmp(&b.capability).then(a.tier.cmp(&b.tier)).then(ra.cmp(rb)));
    out.into_iter().map(|(m, _)| m).collect()
}

/// The best match per capability.
pub fn best(matches: &[Match]) -> Vec<&Match> {
    let mut out: Vec<&Match> = Vec::new();
    for m in matches {
        if !out.iter().any(|x| x.capability == m.capability) {
            out.push(m);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identify::{Quant, QuantClass, QuantSource};

    fn entry(name: &str, tags: &[&str], date: &str) -> TemplateEntry {
        TemplateEntry { name: name.into(), tags: tags.iter().map(|t| (*t).to_owned()).collect(), date: date.into(), ..Default::default() }
    }

    fn declared(template: &str, name: &str) -> DeclaredFile {
        DeclaredFile {
            template: template.into(),
            folder: "diffusion_models".into(),
            name: name.into(),
            stem: crate::stem::stem_key(name).key,
            ..Default::default()
        }
    }

    #[test]
    fn templates_match_by_stem_and_rank_within_capability() {
        let entries = [
            entry("image_krea2_turbo_t2i", &["Text to Image"], "2026-10-01"),
            entry("image_krea2_turbo_t2i_int8", &["Text to Image"], "2026-10-02"),
            entry("image_krea2_turbo_int8_image_style_reference", &["Style Reference"], "2026-10-02"),
            entry("image_flux2_klein_4b", &["Text to Image"], "2026-01-01"),
        ];
        let decl = [
            vec![declared("a", "krea2_turbo_fp8_scaled.safetensors")],
            vec![declared("b", "krea2_turbo_int8_convrot.safetensors")],
            vec![declared("c", "krea2_turbo_int8_convrot.safetensors")],
            vec![declared("d", "flux-2-klein-4b.safetensors")],
        ];
        let candidates: Vec<Candidate> = entries.iter().zip(&decl).map(|(e, d)| Candidate { entry: e, declared: d }).collect();
        let id = Identity { kind: FileKind::DiffusionModel, quant: Quant { class: Some(QuantClass::Bf16), source: QuantSource::Header }, ..Default::default() };
        let q = Query { file: "krea2_turbo_bf16.safetensors", identity: &id, polish: None, foreign_pins: &[], twins: &[], manual: None, server: None };
        let m = find(&q, &candidates);
        let b = best(&m);
        assert_eq!(
            b.iter().map(|m| (m.capability, m.template.as_str(), m.tier)).collect::<Vec<_>>(),
            [(Capability::Create, "image_krea2_turbo_t2i_int8", Tier::Stem), (Capability::Style, "image_krea2_turbo_int8_image_style_reference", Tier::Stem),]
        );
        assert!(!m.iter().any(|m| m.template == "image_flux2_klein_4b"));
        // A 9B file never matches a 4B template, and a checkpoint never matches a UNet template.
        let q9 = Query { file: "flux-2-klein-9b-fp8.safetensors", ..q };
        assert!(find(&q9, &candidates).is_empty());
        let ck = Identity { kind: FileKind::Checkpoint, ..id.clone() };
        assert!(find(&Query { identity: &ck, ..q9 }, &candidates).iter().all(|m| m.tier == Tier::Manual));
        // Renamed files match through the profile's variant stems; the user may pick any.
        let polish = TemplatePolish { stems: [("turbo".to_owned(), vec!["krea2turbo".to_owned()])].into(), ..Default::default() };
        let renamed = Identity { variant: Some("turbo".into()), ..id.clone() };
        let qr = Query { file: "my_krea.safetensors", identity: &renamed, polish: Some(&polish), ..q };
        assert_eq!(best(&find(&qr, &candidates)).first().map(|m| m.tier), Some(Tier::Variant));
        let qm = Query { file: "mystery.safetensors", manual: Some("image_flux2_klein_4b"), ..q };
        assert_eq!(find(&qm, &candidates).first().map(|m| (m.template.as_str(), m.tier)), Some(("image_flux2_klein_4b", Tier::Manual)));
    }
}
