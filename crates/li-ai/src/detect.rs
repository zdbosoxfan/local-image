//! Detection as data: every profile may carry a `detect` block (tensor names, logical shapes,
//! metadata, GGUF architecture, name patterns, variants, LoRA rules). Blocks with weight or
//! metadata conditions are evaluated before the built-in rules in [`crate::arch`], ordered by
//! `priority`, then by how many conditions they state, then by id. A block with no such
//! condition may only re-label a built-in result equal to its `refines` (and supply variants),
//! so a name pattern can never override what the weights say.
//!
//! [`generation`] is the veto the built-in name and metadata maps share: a name or metadata value
//! that names a model generation no rule describes (`flux3`, `hidream_o1`, `qwen_image_3`) maps to
//! nothing instead of the nearest older family.

use crate::arch::{self, Header};
use crate::family::{Detect, Family, Registry, ShapeRule, VariantRule};

/// A model generation named in a file name or metadata value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Gen {
    /// A generation a built-in family covers.
    Known(&'static str),
    /// A generation nothing built in covers ("FLUX generation 3").
    Unknown(String),
    None,
}

fn after_sep(s: &str) -> &str {
    s.strip_prefix(['-', '_', '.', ' ']).unwrap_or(s)
}

fn flux_generation(n: &str) -> Gen {
    for (i, _) in n.match_indices("flux") {
        let rest = after_sep(&n[i + 4..]);
        let digits: String = rest.chars().take_while(char::is_ascii_digit).take(3).collect();
        if digits.is_empty() || digits.len() > 2 || rest[digits.len()..].starts_with(|c: char| c.is_ascii_alphabetic()) {
            continue;
        }
        return match digits.as_str() {
            "1" => Gen::Known("flux1"),
            "2" => Gen::Known("flux2"),
            d => Gen::Unknown(format!("FLUX generation {d}")),
        };
    }
    Gen::None
}

fn hidream_generation(n: &str) -> Gen {
    for (i, _) in n.match_indices("hidream") {
        let rest = after_sep(&n[i + 7..]).as_bytes();
        if rest.len() >= 2 && rest[0].is_ascii_alphabetic() && rest[1].is_ascii_digit() && !rest.get(2).is_some_and(u8::is_ascii_digit) {
            let tag = std::str::from_utf8(&rest[..2]).unwrap_or("");
            return match tag {
                "i1" | "e1" => Gen::Known("hidream"),
                t => Gen::Unknown(format!("HiDream {}", t.to_ascii_uppercase())),
            };
        }
    }
    Gen::None
}

fn qwen_image_generation(n: &str) -> Gen {
    for prefix in ["qwen_image", "qwen-image", "qwenimage", "qwen image"] {
        for (i, _) in n.match_indices(prefix) {
            let mut rest = after_sep(&n[i + prefix.len()..]);
            if let Some(r) = rest.strip_prefix("edit") {
                rest = after_sep(r);
            }
            let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
            if digits.is_empty() {
                continue;
            }
            let tail = &rest[digits.len()..];
            let minor: String = tail.strip_prefix(['.', '_']).map(|t| t.chars().take_while(char::is_ascii_digit).collect()).unwrap_or_default();
            return match (digits.as_str(), minor.as_str()) {
                ("2", "1") | ("21", "") => Gen::Known("qwen-image-21"),
                (d, _) if d.len() == 4 => Gen::Known("qwen-image"),
                (d, "") => Gen::Unknown(format!("Qwen Image {d}")),
                (d, m) => Gen::Unknown(format!("Qwen Image {d}.{m}")),
            };
        }
    }
    Gen::None
}

/// The model generation `text` (a file name or a metadata value) names, if any.
pub fn generation(text: &str) -> Gen {
    let n = text.to_ascii_lowercase();
    for g in [flux_generation(&n), hidream_generation(&n), qwen_image_generation(&n)] {
        if g != Gen::None {
            return g;
        }
    }
    Gen::None
}

/// A detection result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Match {
    pub id: String,
    pub variant: Option<String>,
    pub evidence: String,
}

impl Match {
    pub fn plain(id: impl Into<String>, evidence: impl Into<String>) -> Self {
        Self { id: id.into(), variant: None, evidence: evidence.into() }
    }
}

/// `pattern` against a file name: `*`-separated parts occur in order (case-insensitive); a
/// leading `!` means the token must not occur.
pub fn name_matches(pattern: &str, name: &str) -> bool {
    let n = name.to_ascii_lowercase().replace('\\', "/");
    let n = n.rsplit('/').next().unwrap_or(&n);
    if let Some(neg) = pattern.strip_prefix('!') {
        return !n.contains(&neg.to_ascii_lowercase());
    }
    let mut at = 0;
    for part in pattern.to_ascii_lowercase().split('*').filter(|p| !p.is_empty()) {
        match n[at..].find(part) {
            Some(i) => at += i + part.len(),
            None => return false,
        }
    }
    true
}

/// At least one positive pattern matches and every `!` pattern holds.
fn names_match(patterns: &[String], name: &str) -> bool {
    let (neg, pos): (Vec<&String>, Vec<&String>) = patterns.iter().partition(|p| p.starts_with('!'));
    pos.iter().any(|p| name_matches(p, name)) && neg.iter().all(|p| name_matches(p, name))
}

fn has_suffix(h: &Header, suffix: &str) -> bool {
    h.tensors.iter().any(|t| t.name.ends_with(suffix))
}

fn shape_ok(h: &Header, r: &ShapeRule) -> bool {
    h.tensors.iter().filter(|t| t.name.ends_with(&r.suffix)).any(|t| {
        let shape = crate::identify::logical_shape(h, t);
        let dim = match &r.axis {
            serde_json::Value::String(a) if a == "out" => shape.first().copied(),
            serde_json::Value::String(a) if a == "in" => shape.get(1).copied(),
            serde_json::Value::Number(n) => n.as_u64().and_then(|i| shape.get(i as usize).copied()),
            _ => None,
        };
        dim.is_some_and(|d| r.one_of.contains(&d))
    })
}

fn tensors_ok(h: &Header, all: &[String], any: &[String], none: &[String], shapes: &[ShapeRule]) -> bool {
    all.iter().all(|s| has_suffix(h, s))
        && (any.is_empty() || any.iter().any(|s| h.any_contains(s)))
        && none.iter().all(|s| !has_suffix(h, s))
        && shapes.iter().all(|r| shape_ok(h, r))
}

fn metadata_ok(h: &Header, d: &Detect) -> bool {
    d.metadata.iter().all(|m| {
        let Some(raw) = h.meta(&m.key) else { return false };
        let value = match &m.pointer {
            Some(p) => match serde_json::from_str::<serde_json::Value>(raw).ok().and_then(|v| v.pointer(p).cloned()) {
                Some(serde_json::Value::String(s)) => s,
                Some(v) => v.to_string(),
                None => return false,
            },
            None => raw.to_owned(),
        };
        let v = value.to_ascii_lowercase();
        m.equals.as_ref().is_none_or(|e| v == e.to_ascii_lowercase())
            && m.prefix.as_ref().is_none_or(|p| v.starts_with(&p.to_ascii_lowercase()))
            && m.contains.as_ref().is_none_or(|c| v.contains(&c.to_ascii_lowercase()))
    })
}

fn gguf_ok(h: &Header, d: &Detect) -> bool {
    d.gguf_arch.is_empty() || !h.gguf || h.meta("general.architecture").is_some_and(|a| d.gguf_arch.iter().any(|g| g.eq_ignore_ascii_case(a)))
}

fn prefixed(value: Option<&str>, prefixes: &[String]) -> bool {
    let norm = |s: &str| s.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_lowercase();
    value.is_some_and(|v| {
        let v = norm(v);
        prefixes.iter().any(|p| !p.is_empty() && v.starts_with(&norm(p)))
    })
}

/// How many weight or metadata conditions a block states (the specificity tie-break).
pub fn conditions(d: &Detect) -> usize {
    d.tensors_all.len() + usize::from(!d.tensors_any.is_empty()) + d.tensors_none.len() + d.shapes.len() + d.metadata.len()
}

/// Whether a block can decide from a header on its own (weight or metadata conditions, or
/// metadata prefixes).
pub fn has_header_conditions(d: &Detect) -> bool {
    conditions(d) > 0 || !d.modelspec_prefixes.is_empty() || !d.kohya_prefixes.is_empty()
}

fn header_matches(d: &Detect, h: &Header) -> bool {
    if !gguf_ok(h, d) {
        return false;
    }
    if conditions(d) > 0 && tensors_ok(h, &d.tensors_all, &d.tensors_any, &d.tensors_none, &d.shapes) && metadata_ok(h, d) {
        return true;
    }
    prefixed(h.meta("modelspec.architecture"), &d.modelspec_prefixes) || prefixed(h.meta("ss_base_model_version"), &d.kohya_prefixes)
}

fn variant_ok(v: &VariantRule, h: Option<&Header>, name: &str) -> bool {
    let tensors = v.tensors_all.is_empty() && v.tensors_any.is_empty() && v.tensors_none.is_empty() && v.shapes.is_empty()
        || h.is_some_and(|h| tensors_ok(h, &v.tensors_all, &v.tensors_any, &v.tensors_none, &v.shapes));
    tensors && (v.names.is_empty() || names_match(&v.names, name))
}

/// The variant a block's rules give (first matching rule wins).
pub fn variant_of(d: &Detect, h: Option<&Header>, name: &str) -> Option<String> {
    d.variants.iter().find(|v| variant_ok(v, h, name)).map(|v| v.id.clone())
}

/// The profiles' blocks, best first.
fn blocks(reg: &Registry) -> Vec<(&Family, &Detect)> {
    let mut v: Vec<(&Family, &Detect)> = reg.families.iter().filter_map(|f| f.detect.as_ref().map(|d| (f, d))).collect();
    v.sort_by(|(fa, a), (fb, b)| b.priority.cmp(&a.priority).then(conditions(b).cmp(&conditions(a))).then(fa.id.cmp(&fb.id)));
    v
}

/// Architecture of a main-model header: profile blocks, then the built-in rules (which
/// `refines` blocks may re-label and give variants).
pub fn by_header(reg: &Registry, h: &Header, name: &str) -> Option<Match> {
    let all = blocks(reg);
    for (f, d) in &all {
        if d.refines.is_none() && has_header_conditions(d) && header_matches(d, h) {
            return Some(Match { id: f.id.clone(), variant: variant_of(d, Some(h), name), evidence: evidence(d, &f.id) });
        }
    }
    let builtin = arch::classify(h, name);
    let fam = builtin.family?;
    for (f, d) in &all {
        if d.refines.as_deref() == Some(fam.as_str()) && (conditions(d) == 0 || header_matches(d, h)) {
            let variant = variant_of(d, Some(h), name).or(builtin.variant.clone());
            return Some(Match { id: f.id.clone(), variant, evidence: builtin.evidence });
        }
    }
    Some(Match { id: fam, variant: builtin.variant, evidence: builtin.evidence })
}

fn evidence(d: &Detect, id: &str) -> String {
    let mut parts: Vec<String> = d.tensors_all.iter().chain(&d.tensors_any).map(|t| t.split('.').next().unwrap_or(t).to_owned()).collect();
    parts.dedup();
    if parts.is_empty() { format!("metadata ({id} profile)") } else { format!("{} ({id} profile)", parts.join(" + ")) }
}

/// Architecture from the name alone (remote or unreadable files): profile `names` rules.
pub fn by_name(reg: &Registry, name: &str) -> Option<Match> {
    blocks(reg).into_iter().find(|(_, d)| d.refines.is_none() && names_match(&d.names, name)).map(|(f, d)| Match {
        id: f.id.clone(),
        variant: variant_of(d, None, name),
        evidence: "file name".into(),
    })
}

/// A variant the family's name rules give (MOSS SFT/Base, Krea-2 Turbo/Raw, Qwen 2.1 Turbo).
pub fn name_variant(reg: &Registry, id: &str, name: &str) -> Option<String> {
    let mut sources: Vec<&Detect> = reg.family(id).and_then(|f| f.detect.as_ref()).into_iter().collect();
    sources.extend(reg.families.iter().filter_map(|f| f.detect.as_ref()).filter(|d| d.refines.as_deref() == Some(id)));
    sources.into_iter().find_map(|d| d.variants.iter().find(|v| !v.names.is_empty() && variant_ok(v, None, name)).map(|v| v.id.clone()))
}

const BASE_KEYS: &[&str] = &["ss_base_model_version", "modelspec.architecture", "base_model"];

/// Architecture of a LoRA: profile `lora` rules, then the built-in kohya/modelspec and tensor
/// rules; a LoRA whose metadata names a base nothing knows gets `other:<base>`.
pub fn lora(reg: &Registry, h: &Header, name: &str) -> Option<Match> {
    for (f, d) in blocks(reg) {
        let Some(l) = &d.lora else { continue };
        let tensors = !l.tensors_any.is_empty() && l.tensors_any.iter().any(|t| h.any_contains(t));
        let kohya = prefixed(h.meta("ss_base_model_version"), &l.kohya_prefixes) || prefixed(h.meta("base_model").map(base_model_name), &l.kohya_prefixes);
        let spec = prefixed(h.meta("modelspec.architecture"), &l.modelspec_prefixes);
        if tensors || kohya || spec {
            let why = if tensors { "LoRA modules" } else { "LoRA metadata" };
            return Some(Match::plain(f.id.clone(), format!("{why} ({} profile)", f.id)));
        }
    }
    let builtin = arch::classify(h, name);
    if let Some(fam) = builtin.family {
        return Some(Match { id: fam, variant: builtin.variant, evidence: builtin.evidence });
    }
    let base = BASE_KEYS.iter().find_map(|k| h.meta(k).filter(|v| !v.trim().is_empty()))?;
    if let Gen::Known(fam) = generation(base) {
        return Some(Match::plain(fam, format!("base model {base}")));
    }
    Some(Match::plain(format!("other:{}", base_model_name(base).to_ascii_lowercase()), format!("base model {base}")))
}

/// The model name in a `base_model` value (`fal/krea-2` → `krea-2`, `Comfy-Org/YuE2 yue2_3b` →
/// `YuE2`).
fn base_model_name(v: &str) -> &str {
    let token = v.split_whitespace().next().unwrap_or(v);
    token.rsplit('/').next().unwrap_or(token)
}

/// A LoRA's architecture from its name alone.
pub fn lora_by_name(reg: &Registry, name: &str) -> Option<Match> {
    for (f, d) in blocks(reg) {
        if d.lora.as_ref().is_some_and(|l| l.kohya_prefixes.iter().any(|p| p.len() >= 4 && name_matches(p, name))) {
            return Some(Match::plain(f.id.clone(), "file name"));
        }
    }
    arch::guess_from_name(name, arch::FileKind::Lora).map(|f| Match::plain(f, "file name"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generations_in_names_and_metadata() {
        assert_eq!(generation("flux3-image-klein-bf16.safetensors"), Gen::Unknown("FLUX generation 3".into()));
        assert_eq!(generation("flux-3-image-klein-Q4_K_M.gguf"), Gen::Unknown("FLUX generation 3".into()));
        assert_eq!(generation("flux1-dev.safetensors"), Gen::Known("flux1"));
        assert_eq!(generation("flux-2-klein-4b"), Gen::Known("flux2"));
        assert_eq!(generation("flux.2_dev"), Gen::Known("flux2"));
        assert_eq!(generation("flux_3d_style_lora.safetensors"), Gen::None);
        assert_eq!(generation("flux_kontext"), Gen::None);
        assert_eq!(generation("hidream_i1_full_fp8"), Gen::Known("hidream"));
        assert_eq!(generation("hidream_e1_1"), Gen::Known("hidream"));
        assert_eq!(generation("hidream_o1_image_bf16"), Gen::Unknown("HiDream O1".into()));
        assert_eq!(generation("qwen_image_2.1_bf16"), Gen::Known("qwen-image-21"));
        assert_eq!(generation("qwen-image-2_1-turbo"), Gen::Known("qwen-image-21"));
        assert_eq!(generation("qwen_image_edit_2511_bf16"), Gen::Known("qwen-image"));
        assert_eq!(generation("qwen_image_2512"), Gen::Known("qwen-image"));
        assert_eq!(generation("qwen_image_2"), Gen::Unknown("Qwen Image 2".into()));
        assert_eq!(generation("qwen_image_fp8_e4m3fn"), Gen::None);
        assert_eq!(generation("juggernautXL_v9"), Gen::None);
    }

    #[test]
    fn name_patterns_are_ordered_parts_with_negatives() {
        assert!(name_matches("flux3*klein", "flux3-image-klein-bf16.safetensors"));
        assert!(!name_matches("flux3*klein", "klein-flux3.safetensors"));
        assert!(name_matches("*moss*base*", "sub/moss_image_base_bf16.safetensors"));
        assert!(name_matches("!turbo", "krea2_raw.safetensors"));
        assert!(!name_matches("!turbo", "krea2_turbo.safetensors"));
        assert!(names_match(&["krea2*".into(), "!lora".into()], "krea2_turbo_bf16.safetensors"));
        assert!(!names_match(&["krea2*".into(), "!lora".into()], "krea2_lora.safetensors"));
        assert!(!names_match(&[], "anything"));
    }
}
