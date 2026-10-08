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
}

impl Detected {
    fn new(kind: FileKind, family: Option<&str>, evidence: impl Into<String>) -> Self {
        Self { kind, family: family.map(str::to_owned), evidence: evidence.into() }
    }
}

/// The family a `modelspec.architecture` value names (SAI model spec; also written by kohya and
/// most trainers), e.g. `stable-diffusion-xl-v1-base`, `flux-1-dev`, `qwen-image`.
pub fn family_from_modelspec(arch: &str) -> Option<&'static str> {
    let a = arch.to_ascii_lowercase();
    let a = a.split('/').next().unwrap_or("");
    Some(match a {
        _ if a.starts_with("stable-diffusion-xl") || a.starts_with("sdxl") => "sdxl",
        _ if a.starts_with("stable-diffusion-v1") || a == "sd1" || a.starts_with("sd-1") => "sd15",
        _ if a.starts_with("stable-diffusion-v3") || a.starts_with("sd3") => "sd35",
        _ if a.starts_with("flux-2") || a.starts_with("flux.2") || a.starts_with("flux2") => "flux2",
        _ if a.starts_with("flux") => "flux1",
        _ if a.starts_with("qwen-image-edit") => "qwen-edit",
        _ if a.starts_with("qwen-image") || a.starts_with("qwen_image") => "qwen-image",
        _ if a.starts_with("hidream") => "hidream",
        _ if a.starts_with("z-image") || a.starts_with("zimage") => "z-image",
        _ if a.starts_with("ernie") => "ernie",
        _ if a.starts_with("pony") => "pony",
        _ if a.starts_with("illustrious") => "illustrious",
        _ => return None,
    })
}

/// kohya's `ss_base_model_version` (`sd_v1`, `sdxl_base_v1-0`, `flux1`, `sd3`…).
fn family_from_kohya(v: &str) -> Option<&'static str> {
    let v = v.to_ascii_lowercase();
    Some(match v.as_str() {
        _ if v.starts_with("sdxl") => "sdxl",
        _ if v.starts_with("sd_v1") || v.starts_with("sd1") => "sd15",
        _ if v.starts_with("sd3") => "sd35",
        _ if v.starts_with("flux2") || v.starts_with("flux_2") => "flux2",
        _ if v.starts_with("flux") => "flux1",
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

/// Name hints for what the weights alone cannot tell apart.
fn refine_by_name(family: &str, file_name: &str) -> String {
    let n = file_name.to_ascii_lowercase();
    match family {
        "qwen-image" if n.contains("edit") => "qwen-edit".into(),
        "qwen-image" if n.contains("2.1") || n.contains("2_1") || n.contains("21") && n.contains("qwen_image_2") => "qwen-image-21".into(),
        "flux2" if n.contains("klein") => "flux2-klein".into(),
        "flux1" if n.contains("kontext") => "flux1-kontext".into(),
        "flux1" if n.contains("schnell") => "flux1".into(),
        f => f.to_owned(),
    }
}

/// Identifies a file from its header (and name, for the cases weights cannot decide).
pub fn classify(h: &Header, file_name: &str) -> Detected {
    // Explicit metadata wins.
    if let Some(a) = h.meta("modelspec.architecture") {
        let kind = if a.to_ascii_lowercase().contains("lora") || is_lora(h) { FileKind::Lora } else { kind_of(h) };
        if let Some(fam) = family_from_modelspec(a) {
            let fam = if fam == "sdxl" { sdxl_flavour(h, file_name) } else { fam };
            return Detected::new(kind, Some(&refine_by_name(fam, file_name)), format!("modelspec.architecture = {a}"));
        }
    }
    if h.gguf {
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
            "chroma" => Some("chroma"),
            _ => None,
        };
        if let Some(f) = fam {
            return Detected::new(FileKind::DiffusionModel, Some(&refine_by_name(f, file_name)), format!("GGUF general.architecture = {arch}"));
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
        n.contains("lora_down") || n.contains("lora_up") || n.contains("lora_A.") || n.contains("lora_B.") || n.ends_with(".lokr_w1") || n.contains(".hada_w1")
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
    // FLUX.2 (modulation shared across blocks). Its size (SwarmUI's rule): the modulation's
    // output width is 36864 for dev, 24576 for Klein 9B, 18432 for Klein 4B.
    if h.any_contains("double_stream_modulation_img") || h.any_contains("single_stream_modulation") {
        let width = h.find_suffix("double_stream_modulation_img.lin.weight").map(|t| t.shape.iter().copied().max().unwrap_or(0)).unwrap_or(0);
        let fam = match width {
            18432 | 24576 => "flux2-klein",
            36864 => "flux2",
            // Unknown width: the text input (3 × the text encoder's width) tells Klein from dev.
            _ => match h.find_suffix("txt_in.weight").and_then(|t| t.shape.get(1).copied()) {
                Some(7680) | Some(12288) => "flux2-klein",
                _ => "flux2",
            },
        };
        return Detected::new(kind, Some(&refine_by_name(fam, file_name)), format!("FLUX.2 modulation {width}"));
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
        return Detected::new(kind, Some(&refine_by_name(fam, file_name)), format!("double_blocks, img_in {img_in} (FLUX.1 {what})"));
    }
    // Qwen Image (MMDiT with img_mod/txt_mod). Edit 2511 carries a marker key; plain Edit and
    // 2509 match the text-to-image weights, so the name decides.
    if h.any_contains("transformer_blocks.0.img_mod.") && h.any_contains("txt_norm") {
        if h.has("__index_timestep_zero__") {
            return Detected::new(kind, Some("qwen-edit"), "__index_timestep_zero__ (Qwen Image Edit 2511)");
        }
        return Detected::new(kind, Some(&refine_by_name("qwen-image", file_name)), "img_mod + txt_norm (Qwen Image)");
    }
    // Z-Image and Lumina 2 (NextDiT): the caption embedder's width is 3840 for Z-Image, 2304 for
    // Lumina 2.
    if h.any_contains("cap_embedder.") && h.any_contains("noise_refiner.") {
        let cap = h.find_suffix("cap_embedder.1.weight").and_then(|t| t.shape.first().copied()).unwrap_or(0);
        let fam = if cap == 2304 { "lumina2" } else { "z-image" };
        return Detected::new(kind, Some(fam), format!("cap_embedder {cap} (NextDiT)"));
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

fn classify_lora(h: &Header, file_name: &str) -> Detected {
    let lora = |fam: &str, why: &str| Detected::new(FileKind::Lora, Some(fam), why.to_owned());
    if let Some(v) = h.meta("ss_base_model_version")
        && let Some(f) = family_from_kohya(v)
    {
        let f = if f == "sdxl" { sdxl_flavour(h, file_name) } else { f };
        return lora(&refine_by_name(f, file_name), &format!("ss_base_model_version = {v}"));
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
    if names("img_mod") && names("txt_mlp") || names("transformer_blocks.0.img_mlp") {
        return lora(&refine_by_name("qwen-image", file_name), "img_mod/img_mlp (Qwen Image)");
    }
    if names("noise_refiner") || names("context_refiner") || names("layers.0.attention.qkv") && names("adaLN") {
        return lora("z-image", "NextDiT layers (Z-Image)");
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
        return lora(&refine_by_name(fam, file_name), &format!("{double} double / {single} single blocks"));
    }
    Detected::new(FileKind::Lora, None, "LoRA of an unknown family")
}

/// Best guess from the name alone (for remote ComfyUI installs where the file can't be read).
pub fn guess_from_name(file_name: &str, folder_kind: FileKind) -> Option<String> {
    let n = file_name.to_ascii_lowercase();
    let pick = [
        ("qwen_image_2.1", "qwen-image-21"),
        ("qwen-image-2.1", "qwen-image-21"),
        ("qwen_image_edit", "qwen-edit"),
        ("qwen-image-edit", "qwen-edit"),
        ("qwen_image", "qwen-image"),
        ("qwen-image", "qwen-image"),
        ("flux-2-klein", "flux2-klein"),
        ("flux2-klein", "flux2-klein"),
        ("flux2_klein", "flux2-klein"),
        ("klein", "flux2-klein"),
        ("flux2", "flux2"),
        ("flux-2", "flux2"),
        ("flux.2", "flux2"),
        ("kontext", "flux1-kontext"),
        ("flux1-fill", "flux1-fill"),
        ("flux", "flux1"),
        ("hidream", "hidream"),
        ("z_image", "z-image"),
        ("z-image", "z-image"),
        ("zimage", "z-image"),
        ("ernie", "ernie"),
        ("sd3.5", "sd35"),
        ("sd3_5", "sd35"),
        ("sd35", "sd35"),
        ("pony", "pony"),
        ("illustrious", "illustrious"),
        ("noobai", "illustrious"),
        ("sdxl", "sdxl"),
        ("xl", "sdxl"),
        ("v1-5", "sd15"),
        ("sd15", "sd15"),
        ("sd_1.5", "sd15"),
        ("seedvr2", "seedvr2"),
    ];
    let _ = folder_kind;
    pick.iter().find(|(k, _)| n.contains(k)).map(|(_, f)| (*f).to_owned())
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
        let z = header(&[("cap_embedder.1.weight", &[3840, 2560]), ("noise_refiner.0.attention.qkv.weight", &[11520, 3840])], Value::Null);
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
}
