//! What an installed weights file is: its kind (diffusion model, checkpoint, LoRA, text encoder,
//! VAE), its architecture and variant, its precision and a fingerprint of its network shape.
//! The header decides whenever it can be read; the file name only breaks ties the weights
//! cannot (MOSS SFT vs Base, Krea-2 Turbo vs Raw) or stands in for a remote file.
//!
//! Architecture comes first from the profiles' `detect` blocks ([`crate::detect`]), then from the
//! built-in rules in [`crate::arch`]. A header that neither recognises stays unknown: a file name
//! never overrides what the weights say.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::arch::{self, FileKind, Header, Tensor};
use crate::family::Registry;

/// A storage precision or quantisation format.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum QuantClass {
    Bf16,
    Fp16,
    Fp32,
    Fp8Scaled,
    Fp8Mixed,
    Fp8E4m3fn,
    Fp8E5m2,
    Int8Convrot,
    Int8,
    Nvfp4,
    Mxfp8,
    Fp4Mixed,
    W4a8,
    /// A GGUF type such as `Q4_K_M` or `Q8_0`.
    Gguf(String),
    /// A class this build does not know (read from a newer state file).
    Other(String),
}

impl QuantClass {
    /// The stable id used in state files and `runtime.json` (`bf16`, `int8_convrot`, `gguf:Q4_K_M`).
    pub fn id(&self) -> String {
        match self {
            Self::Bf16 => "bf16".into(),
            Self::Fp16 => "fp16".into(),
            Self::Fp32 => "fp32".into(),
            Self::Fp8Scaled => "fp8_scaled".into(),
            Self::Fp8Mixed => "fp8mixed".into(),
            Self::Fp8E4m3fn => "fp8_e4m3fn".into(),
            Self::Fp8E5m2 => "fp8_e5m2".into(),
            Self::Int8Convrot => "int8_convrot".into(),
            Self::Int8 => "int8".into(),
            Self::Nvfp4 => "nvfp4".into(),
            Self::Mxfp8 => "mxfp8".into(),
            Self::Fp4Mixed => "fp4mixed".into(),
            Self::W4a8 => "w4a8".into(),
            Self::Gguf(t) => format!("gguf:{t}"),
            Self::Other(s) => s.clone(),
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "bf16" => Self::Bf16,
            "fp16" => Self::Fp16,
            "fp32" => Self::Fp32,
            "fp8_scaled" => Self::Fp8Scaled,
            "fp8mixed" => Self::Fp8Mixed,
            "fp8_e4m3fn" => Self::Fp8E4m3fn,
            "fp8_e5m2" => Self::Fp8E5m2,
            "int8_convrot" => Self::Int8Convrot,
            "int8" => Self::Int8,
            "nvfp4" => Self::Nvfp4,
            "mxfp8" => Self::Mxfp8,
            "fp4mixed" => Self::Fp4Mixed,
            "w4a8" => Self::W4a8,
            "" => return None,
            s => match s.strip_prefix("gguf:") {
                Some(t) => Self::Gguf(t.to_owned()),
                None => Self::Other(s.to_owned()),
            },
        })
    }

    /// The `runtime.json` `quant` class this precision falls under (`gguf` for every GGUF type).
    pub fn group(&self) -> String {
        match self {
            Self::Gguf(_) => "gguf".into(),
            other => other.id(),
        }
    }

    /// A short label for lists ("BF16", "FP8", "INT8", "GGUF Q4_K_M").
    pub fn label(&self) -> String {
        match self {
            Self::Bf16 => "BF16".into(),
            Self::Fp16 => "FP16".into(),
            Self::Fp32 => "FP32".into(),
            Self::Fp8Scaled | Self::Fp8Mixed | Self::Fp8E4m3fn | Self::Fp8E5m2 => "FP8".into(),
            Self::Int8Convrot | Self::Int8 => "INT8".into(),
            Self::Nvfp4 => "NVFP4".into(),
            Self::Mxfp8 => "MXFP8".into(),
            Self::Fp4Mixed => "FP4".into(),
            Self::W4a8 => "W4A8".into(),
            Self::Gguf(t) => format!("GGUF {t}"),
            Self::Other(s) => s.to_ascii_uppercase(),
        }
    }
}

impl Serialize for QuantClass {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.id())
    }
}

impl<'de> Deserialize<'de> for QuantClass {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Ok(Self::parse(&s).unwrap_or(Self::Other(s)))
    }
}

/// Where the precision came from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuantSource {
    Header,
    #[default]
    Name,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Quant {
    pub class: Option<QuantClass>,
    pub source: QuantSource,
}

/// What a listed weights file is.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Identity {
    pub kind: FileKind,
    /// The architecture: a profile id (`flux3-klein`, `sdxl`) or a built-in id no profile has
    /// (`lumina2`); `None` when nothing recognised it.
    pub arch: Option<String>,
    /// The variant within the architecture (`distilled`, `base`, `turbo`, `9b`).
    pub variant: Option<String>,
    /// The registry family with a builder, when the architecture has one.
    pub family: Option<String>,
    pub quant: Quant,
    /// The precision-independent name key ([`crate::stem::stem_key`]).
    pub stem: String,
    /// [`arch_signature`] of the header, when it was read.
    pub signature: Option<String>,
    /// Why: the deciding tensors, metadata or name.
    pub evidence: String,
    /// Set when the name names a model generation nothing describes ("FLUX generation 3").
    pub vetoed: Option<String>,
    pub header_read: bool,
}

impl Default for Identity {
    fn default() -> Self {
        Self {
            kind: FileKind::Unknown,
            arch: None,
            variant: None,
            family: None,
            quant: Quant::default(),
            stem: String::new(),
            signature: None,
            evidence: String::new(),
            vetoed: None,
            header_read: false,
        }
    }
}

/// Quantisation sidecar tensors (not part of the network's shape).
const SIDECARS: &[&str] =
    &[".weight_scale", ".weight_scale_2", ".input_scale", ".comfy_quant", ".weight_codebook", ".weight_s_channel", ".weight_s_rel", ".scale_weight"];

fn is_sidecar(name: &str) -> bool {
    SIDECARS.iter().any(|s| name.ends_with(s))
}

/// llama.cpp `general.file_type` values.
fn gguf_file_type(v: u32) -> Option<&'static str> {
    Some(match v {
        0 => "F32",
        1 => "F16",
        2 => "Q4_0",
        3 => "Q4_1",
        7 => "Q8_0",
        8 => "Q5_0",
        9 => "Q5_1",
        10 => "Q2_K",
        11 => "Q3_K_S",
        12 => "Q3_K_M",
        13 => "Q3_K_L",
        14 => "Q4_K_S",
        15 => "Q4_K_M",
        16 => "Q5_K_S",
        17 => "Q5_K_M",
        18 => "Q6_K",
        32 => "BF16",
        _ => return None,
    })
}

/// ggml tensor type ids.
fn ggml_type(v: u32) -> Option<&'static str> {
    Some(match v {
        0 => "F32",
        1 => "F16",
        2 => "Q4_0",
        3 => "Q4_1",
        6 => "Q5_0",
        7 => "Q5_1",
        8 => "Q8_0",
        10 => "Q2_K",
        11 => "Q3_K",
        12 => "Q4_K",
        13 => "Q5_K",
        14 => "Q6_K",
        30 => "BF16",
        _ => return None,
    })
}

fn dominant<'a>(it: impl Iterator<Item = &'a str>) -> Option<&'a str> {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for d in it {
        *counts.entry(d).or_default() += 1;
    }
    counts.into_iter().max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(a.0))).map(|(d, _)| d)
}

/// The precision a header shows. `name` breaks the INT8 tie (`convrot` in the name).
pub fn quant_of(h: &Header, name: &str) -> Option<QuantClass> {
    if h.gguf {
        if let Some(t) = h.meta("general.file_type").and_then(|v| v.parse::<u32>().ok()).and_then(gguf_file_type) {
            return Some(QuantClass::Gguf(t.into()));
        }
        let t = dominant(h.tensors.iter().filter(|t| t.shape.len() >= 2).filter_map(|t| t.dtype.strip_prefix("gguf:")))?;
        return Some(QuantClass::Gguf(t.parse::<u32>().ok().and_then(ggml_type).unwrap_or(t).into()));
    }
    let has_suffix = |s: &str| h.tensors.iter().any(|t| t.name.ends_with(s));
    if let Some(qm) = h.meta("_quantization_metadata").and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok()) {
        let formats = qm.get("layers").and_then(|l| l.as_object()).map(|l| l.values().filter_map(|v| v.get("format")?.as_str()).collect::<Vec<_>>());
        match formats.as_deref().and_then(|f| dominant(f.iter().copied())) {
            Some("nvfp4") => return Some(QuantClass::Nvfp4),
            Some("mxfp8") => return Some(QuantClass::Mxfp8),
            Some("float8_e4m3fn" | "float8_e5m2") => return Some(if has_suffix(".input_scale") { QuantClass::Fp8Mixed } else { QuantClass::Fp8Scaled }),
            _ => {}
        }
    }
    let weights: Vec<&Tensor> = h.tensors.iter().filter(|t| t.name.ends_with(".weight") && t.shape.len() >= 2).collect();
    let count = |dtype: &str| weights.iter().filter(|t| t.dtype == dtype).count();
    let (i8, u8_, f8) = (count("I8"), count("U8"), count("F8_E4M3") + count("F8_E5M2"));
    if i8 > 0 && has_suffix(".weight_codebook") {
        return Some(QuantClass::W4a8);
    }
    if i8 > 0 && has_suffix(".weight_scale") {
        let convrot = crate::stem::stem_key(name).quant_tokens.iter().any(|t| t == "convrot") || has_suffix(".comfy_quant");
        return Some(if convrot { QuantClass::Int8Convrot } else { QuantClass::Int8 });
    }
    if u8_ > 0 && has_suffix(".weight_scale_2") {
        return Some(match (f8, u8_ >= f8) {
            (0, _) => QuantClass::Nvfp4,
            (_, true) => QuantClass::Fp4Mixed,
            (_, false) => QuantClass::Fp8Mixed,
        });
    }
    if f8 > 0 {
        if has_suffix(".input_scale") {
            return Some(QuantClass::Fp8Mixed);
        }
        if has_suffix(".weight_scale") || has_suffix(".scale_weight") {
            return Some(QuantClass::Fp8Scaled);
        }
        return Some(if count("F8_E5M2") > count("F8_E4M3") { QuantClass::Fp8E5m2 } else { QuantClass::Fp8E4m3fn });
    }
    match dominant(weights.iter().map(|t| t.dtype.as_str()))? {
        "BF16" => Some(QuantClass::Bf16),
        "F16" => Some(QuantClass::Fp16),
        "F32" => Some(QuantClass::Fp32),
        _ => None,
    }
}

/// A tensor's shape as the unquantised network has it: NVFP4 (U8 with `.weight_scale_2`) and
/// W4A8 (I8 with `.weight_codebook`) pack two values per byte along the last axis.
pub fn logical_shape(h: &Header, t: &Tensor) -> Vec<u64> {
    let mut shape = t.shape.clone();
    let packed = match t.dtype.as_str() {
        "U8" => h.has(&format!("{}_scale_2", t.name)),
        "I8" => h.has(&format!("{}_codebook", t.name)),
        _ => false,
    };
    if packed && let Some(last) = shape.last_mut() {
        *last *= 2;
    }
    shape
}

const SIGNATURE_PREFIXES: &[&str] = &["model.diffusion_model.", "diffusion_model.", "model.", "transformer."];

/// SHA-256 over the sorted `(name, logical shape)` list, quantisation sidecars excluded and
/// common prefixes stripped: equal signatures are the same network at another precision.
pub fn arch_signature(h: &Header) -> String {
    let mut rows: Vec<String> = h
        .tensors
        .iter()
        .filter(|t| !is_sidecar(&t.name) && !t.name.ends_with("rope.bands") && !t.name.starts_with("__"))
        .map(|t| {
            let name = SIGNATURE_PREFIXES.iter().find_map(|p| t.name.strip_prefix(p)).unwrap_or(&t.name);
            format!("{name}:{:?}", logical_shape(h, t))
        })
        .collect();
    rows.sort();
    hex::encode(Sha256::digest(rows.join("\n").as_bytes()))
}

/// Identifies a file a ComfyUI loader lists. `listed` is what the loader implies (checkpoint,
/// diffusion model, LoRA, text encoder, VAE); `header` is the file's header when it is local.
pub fn identify(name: &str, listed: FileKind, header: Option<&Header>, reg: &Registry) -> Identity {
    let stem = crate::stem::stem_key(name);
    let mut id = Identity { stem: stem.key.clone(), ..Default::default() };
    let gen_label = match crate::detect::generation(name) {
        crate::detect::Gen::Unknown(label) => Some(label),
        _ => None,
    };
    let Some(h) = header else {
        id.kind = listed;
        id.quant = Quant { class: crate::stem::name_quant(&stem.quant_tokens), source: QuantSource::Name };
        let found = if listed == FileKind::Lora {
            crate::detect::lora_by_name(reg, name)
        } else {
            crate::detect::by_name(reg, name).or_else(|| arch::guess_from_name(name, listed).map(|f| crate::detect::Match::plain(f, "file name")))
        };
        apply(&mut id, found, reg, name);
        if id.arch.is_none() {
            id.vetoed = gen_label;
        }
        return id;
    };
    id.header_read = true;
    id.signature = Some(arch_signature(h));
    id.quant = match quant_of(h, name) {
        Some(c) => Quant { class: Some(c), source: QuantSource::Header },
        None => Quant { class: crate::stem::name_quant(&stem.quant_tokens), source: QuantSource::Name },
    };
    id.kind = arch::kind_for(h, listed);
    let found = if id.kind == FileKind::Lora { crate::detect::lora(reg, h, name) } else { crate::detect::by_header(reg, h, name) };
    apply(&mut id, found, reg, name);
    if id.arch.is_none() {
        id.vetoed = gen_label;
    }
    id
}

fn apply(id: &mut Identity, found: Option<crate::detect::Match>, reg: &Registry, name: &str) {
    let Some(m) = found else {
        id.evidence = if id.header_read { "no known layout".into() } else { "file name not recognised".into() };
        return;
    };
    id.variant = m.variant.or_else(|| (id.kind != FileKind::Lora).then(|| crate::detect::name_variant(reg, &m.id, name)).flatten());
    id.family = reg.family(&m.id).filter(|f| f.buildable()).map(|f| f.id.clone());
    id.evidence = m.evidence;
    id.arch = Some(m.id);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn header(tensors: &[(&str, &str, &[u64])], meta: serde_json::Value) -> Header {
        let mut o = serde_json::Map::new();
        for (n, d, s) in tensors {
            o.insert((*n).to_owned(), json!({"dtype": d, "shape": s, "data_offsets": [0, 0]}));
        }
        if !meta.is_null() {
            o.insert("__metadata__".into(), meta);
        }
        arch::parse_safetensors_json(&serde_json::to_vec(&o).unwrap()).unwrap()
    }

    #[test]
    fn quantisation_formats_are_read_from_the_header() {
        let int8 = header(&[("a.weight", "I8", &[8, 8]), ("a.weight_scale", "F32", &[8, 1]), ("a.comfy_quant", "U8", &[4])], json!(null));
        assert_eq!(quant_of(&int8, "x_int8_convrot.safetensors"), Some(QuantClass::Int8Convrot));
        let w4a8 = header(&[("a.weight", "I8", &[8, 4]), ("a.weight_codebook", "F32", &[16]), ("a.weight_s_channel", "F32", &[8])], json!(null));
        assert_eq!(quant_of(&w4a8, "x.safetensors"), Some(QuantClass::W4a8));
        assert_eq!(logical_shape(&w4a8, &w4a8.tensors[0]), vec![8, 8]);
        let nvfp4 = header(&[("a.weight", "U8", &[8, 4]), ("a.weight_scale", "F8_E4M3", &[8, 1]), ("a.weight_scale_2", "F32", &[])], json!(null));
        assert_eq!(quant_of(&nvfp4, "x.safetensors"), Some(QuantClass::Nvfp4));
        assert_eq!(logical_shape(&nvfp4, &nvfp4.tensors[0]), vec![8, 8]);
        let fp4mixed = header(
            &[
                ("a.weight", "U8", &[8, 4]),
                ("a.weight_scale_2", "F32", &[]),
                ("b.weight", "U8", &[8, 4]),
                ("b.weight_scale_2", "F32", &[]),
                ("c.weight", "F8_E4M3", &[8, 8]),
            ],
            json!(null),
        );
        assert_eq!(quant_of(&fp4mixed, "x.safetensors"), Some(QuantClass::Fp4Mixed));
        let scaled = header(&[("a.weight", "F8_E4M3", &[8, 8]), ("a.weight_scale", "F32", &[])], json!(null));
        assert_eq!(quant_of(&scaled, "x.safetensors"), Some(QuantClass::Fp8Scaled));
        let mixed = header(&[("a.weight", "F8_E4M3", &[8, 8]), ("a.weight_scale", "F32", &[]), ("a.input_scale", "F32", &[])], json!(null));
        assert_eq!(quant_of(&mixed, "x.safetensors"), Some(QuantClass::Fp8Mixed));
        let raw = header(&[("a.weight", "F8_E4M3", &[8, 8])], json!(null));
        assert_eq!(quant_of(&raw, "x.safetensors"), Some(QuantClass::Fp8E4m3fn));
        let mx = header(
            &[("a.weight", "F8_E4M3", &[8, 8]), ("a.weight_scale", "U8", &[8, 1])],
            json!({"_quantization_metadata": "{\"layers\": {\"a\": {\"format\": \"mxfp8\"}}}"}),
        );
        assert_eq!(quant_of(&mx, "x.safetensors"), Some(QuantClass::Mxfp8));
        let bf16 = header(&[("a.weight", "BF16", &[8, 8]), ("a.bias", "F32", &[8]), ("n.weight", "F32", &[8])], json!(null));
        assert_eq!(quant_of(&bf16, "x.safetensors"), Some(QuantClass::Bf16));
    }

    #[test]
    fn signatures_ignore_precision_and_prefixes() {
        let bf16 = header(&[("blocks.0.attn.wq.weight", "BF16", &[64, 64]), ("blocks.0.norm.scale", "F32", &[64])], json!(null));
        let nvfp4 = header(
            &[
                ("model.diffusion_model.blocks.0.attn.wq.weight", "U8", &[64, 32]),
                ("model.diffusion_model.blocks.0.attn.wq.weight_scale", "F8_E4M3", &[64, 4]),
                ("model.diffusion_model.blocks.0.attn.wq.weight_scale_2", "F32", &[]),
                ("model.diffusion_model.blocks.0.norm.scale", "BF16", &[64]),
            ],
            json!(null),
        );
        assert_eq!(arch_signature(&bf16), arch_signature(&nvfp4));
        let other = header(
            &[("blocks.0.attn.wq.weight", "BF16", &[64, 64]), ("blocks.0.norm.scale", "F32", &[64]), ("ref_time_in.in_layer.weight", "BF16", &[8, 8])],
            json!(null),
        );
        assert_ne!(arch_signature(&bf16), arch_signature(&other));
    }

    #[test]
    fn quant_class_ids_round_trip_through_serde() {
        for c in [QuantClass::Fp8Scaled, QuantClass::Gguf("Q8_0".into()), QuantClass::Other("future".into())] {
            let s = serde_json::to_string(&c).unwrap();
            assert_eq!(serde_json::from_str::<QuantClass>(&s).unwrap(), c);
        }
    }
}
