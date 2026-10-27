//! Model family detection from a weights file's header, as ComfyUI (`comfy/model_detection.py`)
//! and SwarmUI (`T2IModelClassSorter`) do it: the tensor names and shapes say which network a file
//! holds, and the metadata (`modelspec.architecture`, kohya's `ss_base_model_version`) says more
//! when it is there. Only the header is read, never the weights.
//!
//! - **safetensors**: an 8-byte little-endian header length, then a JSON object mapping each
//!   tensor name to `{dtype, shape, data_offsets}`, plus an optional `__metadata__` string map.
//! - **GGUF** (ComfyUI-GGUF quantised UNets): the `GGUF` magic, version, tensor and key/value
//!   counts, the key/values (`general.architecture` names the family), then the tensor infos.
//!
//! [`classify`] returns what the file is (checkpoint, diffusion model, LoRA, text encoder, VAE)
//! and the family id used by the family registry ([`crate::family`]); the file name breaks ties
//! the weights cannot (Qwen Image vs Qwen Image Edit, Flux.1 dev vs Kontext, base vs distilled).

use std::collections::BTreeMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde_json::Value;

/// Largest header read (real headers are a few MB at most).
const MAX_HEADER: u64 = 64 << 20;

/// One tensor's name, shape and dtype.
#[derive(Clone, Debug, PartialEq)]
pub struct Tensor {
    pub name: String,
    pub shape: Vec<u64>,
    pub dtype: String,
}

/// What a weights file's header says.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Header {
    pub tensors: Vec<Tensor>,
    pub metadata: BTreeMap<String, String>,
    pub gguf: bool,
}

impl Header {
    pub fn has(&self, name: &str) -> bool {
        self.tensors.iter().any(|t| t.name == name)
    }
    pub fn shape(&self, name: &str) -> Option<&[u64]> {
        self.tensors.iter().find(|t| t.name == name).map(|t| t.shape.as_slice())
    }
    /// The first tensor whose name ends with `suffix` (any prefix such as `model.diffusion_model.`).
    pub fn find_suffix(&self, suffix: &str) -> Option<&Tensor> {
        self.tensors.iter().find(|t| t.name.ends_with(suffix))
    }
    pub fn any_contains(&self, part: &str) -> bool {
        self.tensors.iter().any(|t| t.name.contains(part))
    }
    pub fn any_prefix(&self, prefix: &str) -> bool {
        self.tensors.iter().any(|t| t.name.starts_with(prefix))
    }
    /// The highest block index `n` in names containing `{part}{n}.`, plus one.
    pub fn block_count(&self, part: &str) -> usize {
        self.tensors
            .iter()
            .filter_map(|t| {
                let i = t.name.find(part)? + part.len();
                t.name[i..].split('.').next()?.parse::<usize>().ok()
            })
            .max()
            .map_or(0, |n| n + 1)
    }
    pub fn meta(&self, key: &str) -> Option<&str> {
        self.metadata.get(key).map(String::as_str)
    }
}

/// Reads the header of a `.safetensors` or `.gguf` file.
pub fn read_header(path: &Path) -> Result<Header> {
    let mut f = std::fs::File::open(path).with_context(|| format!("Could not open {}", path.display()))?;
    let mut magic = [0u8; 4];
    f.read_exact(&mut magic)?;
    f.seek(SeekFrom::Start(0))?;
    if &magic == b"GGUF" {
        return read_gguf(&mut f);
    }
    read_safetensors(&mut f)
}

/// Parses a safetensors header from a reader positioned at the start of the file.
pub fn read_safetensors(r: &mut impl Read) -> Result<Header> {
    let mut len = [0u8; 8];
    r.read_exact(&mut len)?;
    let len = u64::from_le_bytes(len);
    if len == 0 || len > MAX_HEADER {
        bail!("not a safetensors file (header length {len})");
    }
    let mut buf = vec![0u8; len as usize];
    r.read_exact(&mut buf)?;
    parse_safetensors_json(&buf)
}

pub fn parse_safetensors_json(bytes: &[u8]) -> Result<Header> {
    let v: Value = serde_json::from_slice(bytes).context("the safetensors header is not JSON")?;
    let obj = v.as_object().context("the safetensors header is not an object")?;
    let mut h = Header::default();
    for (k, t) in obj {
        if k == "__metadata__" {
            if let Some(m) = t.as_object() {
                for (mk, mv) in m {
                    h.metadata.insert(mk.clone(), mv.as_str().map(str::to_owned).unwrap_or_else(|| mv.to_string()));
                }
            }
            continue;
        }
        let shape = t.get("shape").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_u64).collect()).unwrap_or_default();
        let dtype = t.get("dtype").and_then(Value::as_str).unwrap_or("").to_owned();
        h.tensors.push(Tensor { name: k.clone(), shape, dtype });
    }
    h.tensors.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(h)
}

fn read_gguf(r: &mut impl Read) -> Result<Header> {
    fn u32_(r: &mut impl Read) -> Result<u32> {
        let mut b = [0u8; 4];
        r.read_exact(&mut b)?;
        Ok(u32::from_le_bytes(b))
    }
    fn u64_(r: &mut impl Read) -> Result<u64> {
        let mut b = [0u8; 8];
        r.read_exact(&mut b)?;
        Ok(u64::from_le_bytes(b))
    }
    fn string(r: &mut impl Read) -> Result<String> {
        let n = u64_(r)?;
        if n > 1 << 20 {
            bail!("GGUF string too long");
        }
        let mut b = vec![0u8; n as usize];
        r.read_exact(&mut b)?;
        Ok(String::from_utf8_lossy(&b).into_owned())
    }
    /// Reads a value of GGUF type `ty`; scalars and strings are kept as text, arrays skipped.
    fn value(r: &mut impl Read, ty: u32, depth: u32) -> Result<Option<String>> {
        let mut skip = |n: usize| -> Result<()> {
            let mut b = vec![0u8; n];
            r.read_exact(&mut b)?;
            Ok(())
        };
        Ok(match ty {
            0 | 1 | 7 => {
                let mut b = [0u8; 1];
                r.read_exact(&mut b)?;
                Some(b[0].to_string())
            }
            2 | 3 => {
                skip(2)?;
                None
            }
            4 => Some(u32_(r)?.to_string()),
            5 => Some((u32_(r)? as i32).to_string()),
            6 => Some(f32::from_bits(u32_(r)?).to_string()),
            8 => Some(string(r)?),
            9 => {
                if depth > 2 {
                    bail!("GGUF arrays nested too deep");
                }
                let inner = u32_(r)?;
                let n = u64_(r)?;
                if n > 1 << 24 {
                    bail!("GGUF array too long");
                }
                for _ in 0..n {
                    value(r, inner, depth + 1)?;
                }
                None
            }
            10 => Some(u64_(r)?.to_string()),
            11 => Some((u64_(r)? as i64).to_string()),
            12 => Some(f64::from_bits(u64_(r)?).to_string()),
            _ => bail!("unknown GGUF value type {ty}"),
        })
    }
    let mut magic = [0u8; 4];
    r.read_exact(&mut magic)?;
    let version = u32_(r)?;
    if version < 2 {
        bail!("GGUF version {version} is not supported");
    }
    let tensors = u64_(r)?;
    let kvs = u64_(r)?;
    if tensors > 1 << 20 || kvs > 1 << 16 {
        bail!("GGUF header too large");
    }
    let mut h = Header { gguf: true, ..Default::default() };
    for _ in 0..kvs {
        let key = string(r)?;
        let ty = u32_(r)?;
        if let Some(v) = value(r, ty, 0)? {
            h.metadata.insert(key, v);
        }
    }
    for _ in 0..tensors {
        let name = string(r)?;
        let dims = u32_(r)?;
        if dims > 8 {
            bail!("GGUF tensor has {dims} dimensions");
        }
        // GGUF stores dimensions innermost first; reverse to the PyTorch order.
        let mut shape: Vec<u64> = (0..dims).map(|_| u64_(r)).collect::<Result<_>>()?;
        shape.reverse();
        let ty = u32_(r)?;
        let _offset = u64_(r)?;
        h.tensors.push(Tensor { name, shape, dtype: format!("gguf:{ty}") });
    }
    h.tensors.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(h)
}

/// What kind of file it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileKind {
    /// A full checkpoint (UNet + text encoder + VAE in one file).
    Checkpoint,
    /// A diffusion model / UNet alone (`diffusion_models/`, `unet/`).
    DiffusionModel,
    Lora,
    TextEncoder,
    Vae,
    Unknown,
}

/// The result of [`classify`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Detected {
    pub kind: FileKind,
    /// A family id from the registry (`sd15`, `sdxl`, `flux1`, …), if recognised.
    pub family: Option<String>,
    /// Why: the deciding tensor, shape or metadata key (shown in the model manager).
    pub evidence: String,
    /// What the weights say beyond the family (`4b`, `9b` for FLUX.2 Klein).
    pub variant: Option<String>,
}

impl Detected {
    fn new(kind: FileKind, family: Option<&str>, evidence: impl Into<String>) -> Self {
        Self { kind, family: family.map(str::to_owned), evidence: evidence.into(), variant: None }
    }
    fn with_variant(mut self, variant: Option<&str>) -> Self {
        self.variant = variant.map(str::to_owned);
        self
    }
}

/// The family a `modelspec.architecture` value names (SAI model spec; also written by kohya and
/// most trainers), e.g. `stable-diffusion-xl-v1-base`, `flux-1-dev`, `qwen-image`. A value naming a
/// generation nothing describes (`flux-3-…`, `hidream-o1`) maps to nothing.
pub fn family_from_modelspec(arch: &str) -> Option<&'static str> {
    let a = arch.to_ascii_lowercase();
    let a = a.split('/').next().unwrap_or("");
    let generation = crate::detect::generation(a);
    if let crate::detect::Gen::Unknown(_) = generation {
        return None;
    }
    Some(match a {
        _ if a.starts_with("stable-diffusion-xl") || a.starts_with("sdxl") => "sdxl",
        _ if a.starts_with("stable-diffusion-v1") || a == "sd1" || a.starts_with("sd-1") => "sd15",
        _ if a.starts_with("stable-diffusion-v3") || a.starts_with("sd3") => "sd35",
        _ if generation == crate::detect::Gen::Known("flux2") && a.contains("klein") => "flux2-klein",
        _ if generation == crate::detect::Gen::Known("flux2") => "flux2",
        _ if a.starts_with("flux") => "flux1",
        _ if a.starts_with("qwen-image-edit") => "qwen-edit",
        _ if generation == crate::detect::Gen::Known("qwen-image-21") => "qwen-image-21",
        _ if a.starts_with("qwen-image") || a.starts_with("qwen_image") => "qwen-image",
        _ if a.starts_with("hidream") => "hidream",
        _ if a.starts_with("z-image") || a.starts_with("zimage") => "z-image",
        _ if a.starts_with("ernie") => "ernie",
        _ if a.starts_with("pony") => "pony",
        _ if a.starts_with("illustrious") => "illustrious",
        _ => return None,
    })
}

/// kohya's `ss_base_model_version` (`sd_v1`, `sdxl_base_v1-0`, `flux1`, `flux2_klein_4b`, `sd3`…),
/// with the same generation veto as [`family_from_modelspec`].
fn family_from_kohya(v: &str) -> Option<&'static str> {
    let v = v.to_ascii_lowercase();
    let generation = crate::detect::generation(&v);
    if let crate::detect::Gen::Unknown(_) = generation {
        return None;
    }
    Some(match v.as_str() {
        _ if v.starts_with("sdxl") => "sdxl",
        _ if v.starts_with("sd_v1") || v.starts_with("sd1") => "sd15",
        _ if v.starts_with("sd3") => "sd35",
        _ if generation == crate::detect::Gen::Known("flux2") && v.contains("klein") => "flux2-klein",
        _ if generation == crate::detect::Gen::Known("flux2") => "flux2",
        _ if v.starts_with("flux") => "flux1",
        _ if generation == crate::detect::Gen::Known("qwen-image-21") => "qwen-image-21",
        _ if v.contains("qwen") => "qwen-image",
        _ if v.contains("hidream") => "hidream",
        _ if v.contains("lumina") || v.contains("z_image") || v.contains("zimage") => "z-image",
        _ => return None,
    })
}

/// SDXL fine-tunes keep SDXL's weights; Pony and Illustrious are told apart by metadata or name.
fn sdxl_flavour(h: &Header, file_name: &str) -> &'static str {
    let n = file_name.to_ascii_lowercase();
    let meta = |k: &str| h.meta(k).unwrap_or("").to_ascii_lowercase();
    let text = format!("{n} {} {} {}", meta("modelspec.title"), meta("modelspec.architecture"), meta("ss_sd_model_name"));
    if text.contains("pony") {
        "pony"
    } else if text.contains("illustrious") || text.contains("noobai") || text.contains("noob") {
        "illustrious"
    } else {
        "sdxl"
    }
}

/// Name hints for what the weights alone cannot tell apart. `None` when the name names a
/// generation nothing describes; callers whose weights already decided keep their family.
fn refine_by_name(family: &str, file_name: &str) -> Option<String> {
    let n = file_name.to_ascii_lowercase();
    let generation = crate::detect::generation(&n);
    if let crate::detect::Gen::Unknown(_) = generation {
        return None;
    }
    Some(match family {
        "qwen-image" if n.contains("edit") => "qwen-edit".into(),
        "qwen-image" if generation == crate::detect::Gen::Known("qwen-image-21") => "qwen-image-21".into(),
        "flux2" if n.contains("klein") => "flux2-klein".into(),
        "flux1" if n.contains("kontext") => "flux1-kontext".into(),
        f => f.to_owned(),
    })
}

/// [`refine_by_name`] where the weights decided: a contradicting name keeps the weights' family.
fn refined(family: &str, file_name: &str) -> String {
    refine_by_name(family, file_name).unwrap_or_else(|| family.to_owned())
}

/// Identifies a file from its header (and name, for the cases weights cannot decide).
pub fn classify(h: &Header, file_name: &str) -> Detected {
    // Explicit metadata wins.
    if let Some(a) = h.meta("modelspec.architecture") {
        let kind = if a.to_ascii_lowercase().contains("lora") || is_lora(h) { FileKind::Lora } else { kind_of(h) };
        if let Some(fam) = family_from_modelspec(a) {
            let fam = if fam == "sdxl" { sdxl_flavour(h, file_name) } else { fam };
            return Detected::new(kind, Some(&refined(fam, file_name)), format!("modelspec.architecture = {a}"));
        }
    }
    if let Some(v) = h.meta("ss_base_model_version")
        && let Some(fam) = family_from_kohya(v)
    {
        let fam = if fam == "sdxl" { sdxl_flavour(h, file_name) } else { fam };
        let kind = if is_lora(h) { FileKind::Lora } else { kind_of(h) };
        return Detected::new(kind, Some(&refined(fam, file_name)), format!("ss_base_model_version = {v}"));
    }
    if h.gguf {
        // GGUF tools tag FLUX.1, FLUX.2, Klein and Chroma all as `flux` (and Z-Image and Lumina 2
        // as `lumina2`), so the tensors decide first and the tag only fills in.
        let w = classify_weights(h, file_name);
        if w.family.is_some() && w.evidence != "file name" {
            return Detected { kind: FileKind::DiffusionModel, evidence: format!("GGUF {}", w.evidence), ..w };
        }
        let arch = h.meta("general.architecture").unwrap_or("");
        let fam = match arch {
            "flux" if h.any_contains("double_stream_modulation") => Some("flux2"),
            "flux" => Some("flux1"),
            "sd1" => Some("sd15"),
            "sdxl" => Some("sdxl"),
            "sd3" => Some("sd35"),
            "hidream" => Some("hidream"),
            "lumina2" => Some("z-image"),
            "qwen_image" => Some("qwen-image"),
            _ => None,
        };
        if let Some(f) = fam {
            return Detected::new(FileKind::DiffusionModel, refine_by_name(f, file_name).as_deref(), format!("GGUF general.architecture = {arch}"));
        }
    }
    if is_lora(h) {
        return classify_lora(h, file_name);
    }
    classify_weights(h, file_name)
}

fn is_lora(h: &Header) -> bool {
    h.tensors.iter().any(|t| {
        let n = &t.name;
        n.contains("lora_down")
            || n.contains("lora_up")
            || n.contains("lora_A.")
            || n.contains("lora_B.")
            || n.ends_with(".lora_A")
            || n.ends_with(".lora_B")
            || n.contains(".lora.down.")
            || n.contains(".lora.up.")
            || n.ends_with(".lokr_w1")
            || n.contains(".hada_w1")
    })
}

fn kind_of(h: &Header) -> FileKind {
    let unet =
        h.any_contains("diffusion_model.") || h.any_contains("double_blocks.") || h.any_contains("transformer_blocks.") || h.any_contains("joint_blocks.");
    let te = h.any_prefix("cond_stage_model.") || h.any_prefix("conditioner.") || h.any_prefix("text_encoders.");
    let vae = h.any_prefix("first_stage_model.") || h.any_prefix("vae.");
    match (unet, te || vae) {
        (true, true) => FileKind::Checkpoint,
        (true, false) => FileKind::DiffusionModel,
        _ if h.any_contains("decoder.conv_in.weight") || h.any_contains("decoder.up_blocks") || h.any_contains("decoder.up.") => FileKind::Vae,
        _ if h.any_contains("text_model.encoder.layers") || h.any_contains("encoder.block.") || h.any_contains("model.layers.") => FileKind::TextEncoder,
        _ => FileKind::Unknown,
    }
}

/// What a file listed by a loader of kind `listed` is. A LoRA is a LoRA wherever it sits; a
/// unified multimodal checkpoint (HiDream-O1: a language model plus diffusion heads) is a
/// checkpoint although its layers look like a text encoder's; a layout the kind rules don't
/// know keeps the loader's kind.
pub fn kind_for(h: &Header, listed: FileKind) -> FileKind {
    if is_lora(h) || h.meta("modelspec.architecture").is_some_and(|a| a.to_ascii_lowercase().contains("lora")) {
        return FileKind::Lora;
    }
    if h.any_prefix("model.language_model.") && (h.any_contains("t_embedder") || h.any_contains("final_layer")) {
        return FileKind::Checkpoint;
    }
    match kind_of(h) {
        FileKind::Unknown => listed,
        k => k,
    }
}

fn classify_weights(h: &Header, file_name: &str) -> Detected {
    let kind = kind_of(h);
    // Stable Diffusion UNets (`input_blocks`).
    if let Some(t) = h.find_suffix("diffusion_model.input_blocks.0.0.weight").or_else(|| h.find_suffix("input_blocks.0.0.weight")) {
        let in_ch = t.shape.get(1).copied().unwrap_or(4);
        let inpaint = if in_ch == 9 { " (inpainting)" } else { "" };
        if h.find_suffix("label_emb.0.0.weight").is_some() {
            let refiner = h.find_suffix("label_emb.0.0.weight").and_then(|t| t.shape.get(1).copied()) == Some(2560);
            if refiner {
                return Detected::new(kind, Some("sdxl-refiner"), "label_emb 2560 (SDXL refiner)");
            }
            return Detected::new(kind, Some(sdxl_flavour(h, file_name)), format!("label_emb.0.0 (SDXL){inpaint}"));
        }
        let ctx = h.find_suffix("input_blocks.1.1.transformer_blocks.0.attn2.to_k.weight").and_then(|t| t.shape.get(1).copied());
        return match ctx {
            Some(1024) => Detected::new(kind, Some("sd2"), "cross-attention 1024 (SD 2.x)"),
            _ => Detected::new(kind, Some("sd15"), format!("cross-attention {} (SD 1.x){inpaint}", ctx.unwrap_or(768))),
        };
    }
    // SD3 / SD3.5 (MMDiT joint blocks).
    if h.any_contains("joint_blocks.0.context_block") {
        let depth = h.block_count("joint_blocks.");
        return Detected::new(kind, Some("sd35"), format!("{depth} joint blocks (SD3)"));
    }
    // HiDream (MoE dual/single stream).
    if h.any_contains("double_stream_blocks.0.block.") || h.any_contains("caption_projection.0.linear") {
        return Detected::new(kind, Some("hidream"), "double_stream_blocks (HiDream)");
    }
    // FLUX.2 (modulation shared across blocks), by its exact tensor names: FLUX.3's plural
    // `single_stream_modulations.*` is another network. Its size (SwarmUI's rule): the
    // modulation's output width is 36864 for dev, 24576 for Klein 9B, 18432 for Klein 4B.
    let flux2_modulation = h.find_suffix("double_stream_modulation_img.lin.weight");
    if flux2_modulation.is_some() || h.find_suffix("single_stream_modulation.lin.weight").is_some() {
        let width = flux2_modulation.map(|t| crate::identify::logical_shape(h, t).into_iter().max().unwrap_or(0)).unwrap_or(0);
        // Unknown width: the text input (3 × the text encoder's width) tells Klein from dev.
        let text_in = h.find_suffix("txt_in.weight").and_then(|t| crate::identify::logical_shape(h, t).get(1).copied());
        let (fam, variant) = match (width, text_in) {
            (24576, _) | (0, Some(12288)) => ("flux2-klein", Some("9b")),
            (18432, _) | (0, Some(7680)) => ("flux2-klein", Some("4b")),
            (36864, _) => ("flux2", None),
            (_, Some(12288)) => ("flux2-klein", Some("9b")),
            (_, Some(7680)) => ("flux2-klein", Some("4b")),
            _ => ("flux2", None),
        };
        return Detected::new(kind, Some(&refined(fam, file_name)), format!("FLUX.2 modulation {width}")).with_variant(variant);
    }
    // Qwen Image 2.1 (its own text norm and modulation).
    if h.any_contains("txt_in.text_norm.weight") && h.any_contains("modulation.1.weight") {
        return Detected::new(kind, Some("qwen-image-21"), "txt_in.text_norm + modulation (Qwen Image 2.1)");
    }
    // ERNIE-Image.
    if h.any_contains("layers.0.mlp.linear_fc2.weight") {
        return Detected::new(kind, Some("ernie"), "layers.N.mlp.linear_fc2 (ERNIE-Image)");
    }
    // Chroma (FLUX.1 with a distilled guidance layer).
    if h.any_contains("distilled_guidance_layer.") {
        return Detected::new(kind, Some("chroma"), "distilled_guidance_layer (Chroma)");
    }
    // FLUX.1 (double/single blocks).
    if h.any_contains("double_blocks.0.img_attn") {
        let img_in = h.find_suffix("img_in.weight").and_then(|t| t.shape.get(1).copied()).unwrap_or(64);
        let fam = match img_in {
            384 => "flux1-fill",
            128 => "flux1-control",
            _ => "flux1",
        };
        let guidance = h.any_contains("guidance_in.");
        let what = if guidance { "dev" } else { "schnell" };
        return Detected::new(kind, Some(&refined(fam, file_name)), format!("double_blocks, img_in {img_in} (FLUX.1 {what})"));
    }
    // Qwen Image (MMDiT with img_mod/txt_mod). Edit 2511 carries a marker key; plain Edit and
    // 2509 match the text-to-image weights, so the name decides.
    if h.any_contains("transformer_blocks.0.img_mod.") && h.any_contains("txt_norm") {
        if h.has("__index_timestep_zero__") {
            return Detected::new(kind, Some("qwen-edit"), "__index_timestep_zero__ (Qwen Image Edit 2511)");
        }
        return Detected::new(kind, Some(&refined("qwen-image", file_name)), "img_mod + txt_norm (Qwen Image)");
    }
    // Z-Image and Lumina 2 (NextDiT): the caption embedder's width is 3840 for Z-Image, 2304 for
    // Lumina 2. Z-Image has a plain `x_embedder` and fused attention `qkv`; Ming-Image shares the
    // caption width but has neither.
    if h.any_contains("cap_embedder.") && h.any_contains("noise_refiner.") {
        let cap = h.find_suffix("cap_embedder.1.weight").and_then(|t| t.shape.first().copied()).unwrap_or(0);
        if cap == 2304 {
            return Detected::new(kind, Some("lumina2"), format!("cap_embedder {cap} (NextDiT)"));
        }
        let plain_embedder = h.tensors.iter().any(|t| t.name == "x_embedder.weight" || t.name.ends_with(".x_embedder.weight"));
        if plain_embedder && h.find_suffix("layers.0.attention.qkv.weight").is_some() {
            return Detected::new(kind, Some("z-image"), format!("cap_embedder {cap} (NextDiT)"));
        }
    }
    let n = file_name.to_ascii_lowercase();
    if n.contains("ernie") {
        return Detected::new(kind, Some("ernie"), "file name");
    }
    if n.contains("seedvr2") {
        return Detected::new(kind, Some("seedvr2"), "file name");
    }
    Detected::new(kind, None, "no known layout")
}

/// The width (in-features) of a LoRA's first matching down/A projection.
fn lora_width(h: &Header, module: &str) -> Option<u64> {
    h.tensors
        .iter()
        .find(|t| t.name.contains(module) && (t.name.contains("lora_down") || t.name.contains("lora_A") || t.name.contains(".lora.down")))
        .and_then(|t| t.shape.get(1).copied())
}

fn classify_lora(h: &Header, file_name: &str) -> Detected {
    let lora = |fam: &str, why: &str| Detected::new(FileKind::Lora, Some(fam), why.to_owned());
    if let Some(v) = h.meta("ss_base_model_version")
        && let Some(f) = family_from_kohya(v)
    {
        let f = if f == "sdxl" { sdxl_flavour(h, file_name) } else { f };
        return lora(&refined(f, file_name), &format!("ss_base_model_version = {v}"));
    }
    let names = |part: &str| h.any_contains(part);
    if names("lora_te2_") || names("lora_unet_label_emb") || names("input_blocks_4_1_transformer_blocks_9") {
        return lora(sdxl_flavour(h, file_name), "two text encoders / deep SDXL blocks");
    }
    if names("lora_unet_input_blocks") || names("lora_unet_down_blocks") || names("lora_te_text_model") {
        return lora("sd15", "kohya SD keys");
    }
    if names("joint_blocks") {
        return lora("sd35", "joint_blocks (SD3)");
    }
    if names("double_stream_blocks") {
        return lora("hidream", "double_stream_blocks (HiDream)");
    }
    // Qwen Image is 3072 wide; Qwen Image 2.1 is 4096 wide.
    if names("img_mod") && names("txt_mlp") || names("transformer_blocks.0.img_mlp") {
        if lora_width(h, "transformer_blocks.0.attn.to_q") == Some(4096) || lora_width(h, "transformer_blocks.0.img_mlp") == Some(4096) {
            return lora("qwen-image-21", "img_mlp 4096 wide (Qwen Image 2.1)");
        }
        return lora(&refined("qwen-image", file_name), "img_mod/img_mlp (Qwen Image)");
    }
    // NextDiT: Z-Image is 3840 wide (fused qkv out 11520); Fibo is 3584 wide with cross-attention.
    if (names("noise_refiner") || names("context_refiner") || names("layers.0.attention.qkv") && names("adaLN")) && !names("cross_attention") {
        let qkv_out = h
            .tensors
            .iter()
            .find(|t| t.name.contains("layers.0.attention.qkv") && (t.name.contains("lora_up") || t.name.contains("lora_B")))
            .and_then(|t| t.shape.first().copied());
        if qkv_out.is_none_or(|w| w == 11520) {
            return lora("z-image", "NextDiT layers (Z-Image)");
        }
    }
    if names("double_blocks") || names("single_blocks") || names("single_transformer_blocks") {
        // FLUX.1 has 19 double / 38 single blocks; FLUX.2 dev 48 single blocks, Klein 9B 24,
        // Klein 4B 20 (SwarmUI's rule: the highest single-block index).
        let double = h.block_count("double_blocks.").max(h.block_count("double_blocks_")).max(h.block_count("transformer_blocks."));
        let single = h.block_count("single_blocks.").max(h.block_count("single_blocks_")).max(h.block_count("single_transformer_blocks."));
        let fam = match (double, single) {
            (19, _) | (_, 38) => "flux1",
            (_, 48) => "flux2",
            (_, 20) | (_, 24) => "flux2-klein",
            (d, s) if d > 0 && d < 19 && s < 38 => "flux2-klein",
            _ => "flux1",
        };
        return lora(&refined(fam, file_name), &format!("{double} double / {single} single blocks"));
    }
    // Diffusers attention-only LoRAs (`transformer_blocks.N.attn.to_q`): width and depth tell
    // the networks apart (Qwen 2.1 32 x 4096, Klein 9B 8 x 4096, Klein 4B 5 x 3072, Qwen 60 x 3072).
    if let Some(width) = lora_width(h, "transformer_blocks.0.attn.to_q") {
        let blocks = h.block_count("transformer_blocks.");
        let fam = match (width, blocks) {
            (4096, b) if b > 8 => Some("qwen-image-21"),
            (4096, _) => Some("flux2-klein"),
            (3072, b) if b > 19 => Some("qwen-image"),
            (3072, 19) => Some("flux1"),
            (3072, b) if b <= 5 => Some("flux2-klein"),
            (6144, b) if b <= 8 => Some("flux2"),
            _ => None,
        };
        if let Some(fam) = fam {
            return lora(fam, &format!("{blocks} transformer blocks x {width}"));
        }
    }
    Detected::new(FileKind::Lora, None, "LoRA of an unknown family")
}

/// Whether a file name (maybe with subfolders) says it is a LoRA or another adapter
/// (`flux_realism_lora.safetensors`, `add-detail-XL-LoRA`, `myStyleLoRA_v2`, `loras/x`,
/// LyCORIS / LoCon / LoHa / LoKr). Words only: `floral_dream` is not a LoRA.
pub fn name_looks_like_lora(file_name: &str) -> bool {
    let path = file_name.replace('\\', "/");
    let stem = path.rsplit_once('.').filter(|(_, ext)| !ext.contains('/')).map_or(path.as_str(), |(s, _)| s);
    let words = ["lora", "loras", "lycoris", "locon", "loha", "lokr"];
    let lower = stem.to_ascii_lowercase();
    let word = lower.split(|c: char| !c.is_ascii_alphanumeric()).any(|w| {
        words.contains(&w) || w.strip_prefix("lora").is_some_and(|rest| !rest.is_empty() && rest.trim_start_matches('v').chars().all(|c| c.is_ascii_digit()))
    });
    // camelCase: "styleLoRA", "detailLora" (a lowercase letter or digit right before it).
    let camel = ["LoRA", "Lora", "LORA"]
        .iter()
        .any(|k| stem.match_indices(k).any(|(i, _)| stem[..i].chars().next_back().is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())));
    word || camel
}

/// Best guess from the name alone (for remote ComfyUI installs where the file can't be read).
/// A name that says LoRA is no main model (`folder_kind` checkpoint or diffusion model). A name
/// that names a generation nothing describes (`flux3`, `hidream_o1`) is no guess at all.
pub fn guess_from_name(file_name: &str, folder_kind: FileKind) -> Option<String> {
    use crate::detect::Gen;
    if matches!(folder_kind, FileKind::Checkpoint | FileKind::DiffusionModel) && name_looks_like_lora(file_name) {
        return None;
    }
    let n = file_name.to_ascii_lowercase();
    let generation = crate::detect::generation(&n);
    match generation {
        Gen::Unknown(_) => return None,
        Gen::Known("qwen-image-21") => return Some("qwen-image-21".into()),
        _ => {}
    }
    let flux1_or_none = matches!(generation, Gen::Known("flux1") | Gen::None);
    let flux2_or_none = matches!(generation, Gen::Known("flux2") | Gen::None);
    let pick = [
        ("qwen_image_edit", "qwen-edit", true),
        ("qwen-image-edit", "qwen-edit", true),
        ("qwen_image", "qwen-image", true),
        ("qwen-image", "qwen-image", true),
        ("flux-2-klein", "flux2-klein", true),
        ("flux2-klein", "flux2-klein", true),
        ("flux2_klein", "flux2-klein", true),
        ("klein", "flux2-klein", flux2_or_none),
        ("flux2", "flux2", true),
        ("flux-2", "flux2", true),
        ("flux.2", "flux2", true),
        ("kontext", "flux1-kontext", flux1_or_none),
        ("flux1-fill", "flux1-fill", true),
        ("flux", "flux1", flux1_or_none),
        ("hidream", "hidream", true),
        ("z_image", "z-image", true),
        ("z-image", "z-image", true),
        ("zimage", "z-image", true),
        ("ernie", "ernie", true),
        ("sd3.5", "sd35", true),
        ("sd3_5", "sd35", true),
        ("sd35", "sd35", true),
        ("pony", "pony", true),
        ("illustrious", "illustrious", true),
        ("noobai", "illustrious", true),
        ("sdxl", "sdxl", true),
        ("xl", "sdxl", true),
        ("v1-5", "sd15", true),
        ("sd15", "sd15", true),
        ("sd_1.5", "sd15", true),
        ("seedvr2", "seedvr2", true),
    ];
    pick.iter().find(|(k, _, allowed)| *allowed && n.contains(k)).map(|(_, f, _)| (*f).to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn header(tensors: &[(&str, &[u64])], meta: Value) -> Header {
        let mut o = serde_json::Map::new();
        for (n, s) in tensors {
            o.insert((*n).to_owned(), json!({"dtype": "F16", "shape": s, "data_offsets": [0, 0]}));
        }
        if !meta.is_null() {
            o.insert("__metadata__".into(), meta);
        }
        let bytes = serde_json::to_vec(&Value::Object(o)).unwrap();
        let mut file = (bytes.len() as u64).to_le_bytes().to_vec();
        file.extend(bytes);
        read_safetensors(&mut file.as_slice()).unwrap()
    }

    fn fam(h: &Header, name: &str) -> (FileKind, Option<String>) {
        let d = classify(h, name);
        (d.kind, d.family)
    }

    #[test]
    fn stable_diffusion_checkpoints() {
        let sd15 = header(
            &[
                ("model.diffusion_model.input_blocks.0.0.weight", &[320, 4, 3, 3]),
                ("model.diffusion_model.input_blocks.1.1.transformer_blocks.0.attn2.to_k.weight", &[320, 768]),
                ("cond_stage_model.transformer.text_model.embeddings.position_ids", &[1, 77]),
                ("first_stage_model.decoder.conv_in.weight", &[512, 4, 3, 3]),
            ],
            Value::Null,
        );
        assert_eq!(fam(&sd15, "dreamshaper_8.safetensors"), (FileKind::Checkpoint, Some("sd15".into())));
        let xl = header(
            &[
                ("model.diffusion_model.input_blocks.0.0.weight", &[320, 4, 3, 3]),
                ("model.diffusion_model.label_emb.0.0.weight", &[1280, 2816]),
                ("conditioner.embedders.1.model.ln_final.weight", &[1280]),
            ],
            Value::Null,
        );
        assert_eq!(fam(&xl, "juggernautXL_v9.safetensors").1.as_deref(), Some("sdxl"));
        assert_eq!(fam(&xl, "ponyDiffusionV6XL.safetensors").1.as_deref(), Some("pony"));
        assert_eq!(fam(&xl, "waiNSFWIllustrious_v14.safetensors").1.as_deref(), Some("illustrious"));
    }

    #[test]
    fn transformer_families() {
        let flux = header(
            &[("double_blocks.0.img_attn.norm.key_norm.scale", &[128]), ("img_in.weight", &[3072, 64]), ("guidance_in.in_layer.weight", &[3072, 256])],
            Value::Null,
        );
        assert_eq!(fam(&flux, "flux1-dev.safetensors"), (FileKind::DiffusionModel, Some("flux1".into())));
        assert_eq!(fam(&flux, "flux1-kontext-dev.safetensors").1.as_deref(), Some("flux1-kontext"));
        let fill = header(&[("double_blocks.0.img_attn.qkv.weight", &[9216, 3072]), ("img_in.weight", &[3072, 384])], Value::Null);
        assert_eq!(fam(&fill, "x.safetensors").1.as_deref(), Some("flux1-fill"));
        let qwen = header(&[("transformer_blocks.0.img_mod.1.weight", &[18432, 3072]), ("txt_norm.weight", &[3584])], Value::Null);
        assert_eq!(fam(&qwen, "qwen_image_fp8_e4m3fn.safetensors").1.as_deref(), Some("qwen-image"));
        assert_eq!(fam(&qwen, "qwen_image_edit_2509_fp8.safetensors").1.as_deref(), Some("qwen-edit"));
        let sd3 = header(&[("joint_blocks.0.context_block.attn.qkv.weight", &[4608, 1536]), ("joint_blocks.23.x_block.mlp.fc1.weight", &[1, 1])], Value::Null);
        assert_eq!(fam(&sd3, "sd3.5_medium.safetensors").1.as_deref(), Some("sd35"));
        let z = header(
            &[
                ("cap_embedder.1.weight", &[3840, 2560]),
                ("noise_refiner.0.attention.qkv.weight", &[11520, 3840]),
                ("x_embedder.weight", &[3840, 64]),
                ("layers.0.attention.qkv.weight", &[11520, 3840]),
            ],
            Value::Null,
        );
        let lumina = header(&[("cap_embedder.1.weight", &[2304, 2304]), ("noise_refiner.0.attention.qkv.weight", &[1, 1])], Value::Null);
        assert_eq!(fam(&lumina, "lumina_2.safetensors").1.as_deref(), Some("lumina2"));
        let edit = header(&[("transformer_blocks.0.img_mod.1.weight", &[1, 1]), ("txt_norm.weight", &[3584]), ("__index_timestep_zero__", &[1])], Value::Null);
        assert_eq!(fam(&edit, "some_qwen.safetensors").1.as_deref(), Some("qwen-edit"));
        let q21 = header(&[("txt_in.text_norm.weight", &[1]), ("modulation.1.weight", &[1, 1])], Value::Null);
        assert_eq!(fam(&q21, "x.safetensors").1.as_deref(), Some("qwen-image-21"));
        let ernie = header(&[("layers.0.mlp.linear_fc2.weight", &[1, 1])], Value::Null);
        assert_eq!(fam(&ernie, "x.safetensors").1.as_deref(), Some("ernie"));
        assert_eq!(fam(&z, "z_image_turbo_bf16.safetensors").1.as_deref(), Some("z-image"));
        let hd = header(&[("double_stream_blocks.0.block.ff_i.shared_experts.w1.weight", &[1, 1])], Value::Null);
        assert_eq!(fam(&hd, "hidream_i1_full.safetensors").1.as_deref(), Some("hidream"));
        let f2 = header(&[("double_stream_modulation_img.lin.weight", &[18432, 3072]), ("img_in.weight", &[3072, 128])], Value::Null);
        assert_eq!(fam(&f2, "some_name.safetensors").1.as_deref(), Some("flux2-klein"));
        let dev = header(&[("double_stream_modulation_img.lin.weight", &[36864, 6144])], Value::Null);
        assert_eq!(fam(&dev, "flux2_dev_fp8mixed.safetensors").1.as_deref(), Some("flux2"));
    }

    #[test]
    fn metadata_and_loras() {
        let spec = header(&[("lora_unet_x.lora_down.weight", &[16, 64])], json!({"modelspec.architecture": "flux-1-dev/lora"}));
        assert_eq!(fam(&spec, "style.safetensors"), (FileKind::Lora, Some("flux1".into())));
        let kohya = header(&[("lora_unet_down_blocks_0.lora_down.weight", &[8, 320])], json!({"ss_base_model_version": "sdxl_base_v1-0"}));
        assert_eq!(fam(&kohya, "detail.safetensors").1.as_deref(), Some("sdxl"));
        let qwen_lora = header(&[("diffusion_model.transformer_blocks.0.img_mlp.net.0.proj.lora_A.weight", &[16, 3072])], Value::Null);
        assert_eq!(fam(&qwen_lora, "lightning.safetensors").1.as_deref(), Some("qwen-image"));
        let flux_lora = header(
            &[
                ("diffusion_model.double_blocks.18.img_attn.qkv.lora_A.weight", &[16, 3072]),
                ("diffusion_model.single_blocks.37.linear1.lora_A.weight", &[16, 3072]),
            ],
            Value::Null,
        );
        assert_eq!(fam(&flux_lora, "a.safetensors").1.as_deref(), Some("flux1"));
    }

    #[test]
    fn gguf_headers() {
        // GGUF v3 with general.architecture = flux and one tensor.
        let mut b = b"GGUF".to_vec();
        b.extend(3u32.to_le_bytes());
        b.extend(1u64.to_le_bytes());
        b.extend(1u64.to_le_bytes());
        let s = |b: &mut Vec<u8>, s: &str| {
            b.extend((s.len() as u64).to_le_bytes());
            b.extend(s.as_bytes());
        };
        s(&mut b, "general.architecture");
        b.extend(8u32.to_le_bytes());
        s(&mut b, "flux");
        s(&mut b, "double_blocks.0.img_attn.qkv.weight");
        b.extend(2u32.to_le_bytes());
        b.extend(3072u64.to_le_bytes());
        b.extend(9216u64.to_le_bytes());
        b.extend(8u32.to_le_bytes());
        b.extend(0u64.to_le_bytes());
        let h = read_gguf(&mut b.as_slice()).unwrap();
        assert_eq!(h.tensors[0].shape, vec![9216, 3072]);
        assert_eq!(fam(&h, "flux1-dev-Q4_K_S.gguf"), (FileKind::DiffusionModel, Some("flux1".into())));
    }

    #[test]
    fn names_guess_families() {
        assert_eq!(guess_from_name("qwen_image_edit_2511_bf16.safetensors", FileKind::DiffusionModel).as_deref(), Some("qwen-edit"));
        assert_eq!(guess_from_name("ponyDiffusionV6XL.safetensors", FileKind::Checkpoint).as_deref(), Some("pony"));
        assert_eq!(guess_from_name("random.safetensors", FileKind::Checkpoint), None);
    }

    /// LoRA files that end up in a model folder are not models; in the LoRA folder they keep
    /// their family.
    #[test]
    fn lora_names_are_not_models() {
        for n in [
            "flux_realism_lora.safetensors",
            "add-detail-XL-LoRA.safetensors",
            "myStyleLoRA_v2.safetensors",
            "pixel-art-xl-lora2.safetensors",
            "sub/loras/thing.safetensors",
            "lycoris_ink.safetensors",
            "qwen_image_lora_v1.safetensors",
        ] {
            assert!(name_looks_like_lora(n), "{n}");
            assert_eq!(guess_from_name(n, FileKind::DiffusionModel), None, "{n}");
            assert_eq!(guess_from_name(n, FileKind::Checkpoint), None, "{n}");
        }
        for n in ["floral_dream_xl.safetensors", "FLORA.safetensors", "Flora_v1.safetensors", "juggernautXL_v9.safetensors", "colorful_xl.safetensors"] {
            assert!(!name_looks_like_lora(n), "{n}");
        }
        assert_eq!(guess_from_name("flux_realism_lora.safetensors", FileKind::Lora).as_deref(), Some("flux1"));
        assert_eq!(guess_from_name("qwen_image_lora_v1.safetensors", FileKind::Lora).as_deref(), Some("qwen-image"));
    }
}
