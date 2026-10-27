//! Precision-independent model names. Official templates name one precision of a model
//! (`moss_image_sft_int8_convrot.safetensors`) while the user may have another
//! (`moss_image_sft_bf16.safetensors`, a GGUF, an NVFP4 file). [`stem_key`] strips the trailing run
//! of precision and quantisation tokens, so two files are the same model at another precision
//! exactly when their keys are equal. There is never a prefix or substring test: `flux2-vae` and
//! `flux2-vae_small`, `qwen_image_2.1` and `qwen_image_2.1_turbo` stay different.

use crate::identify::QuantClass;

/// A file name split into the model part and its precision suffix.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StemKey {
    /// The original text of the model part (`flux3-image-klein`).
    pub display: String,
    /// `display` lower-cased with only ASCII letters and digits (`flux3imageklein`).
    pub key: String,
    /// The stripped precision tokens, lower-cased (`["int8", "convrot"]`).
    pub quant_tokens: Vec<String>,
    /// The extension without the dot (`safetensors`), empty when there was none.
    pub ext: String,
}

const EXTENSIONS: &[&str] = &["safetensors", "gguf", "sft", "ckpt", "pt", "pth", "bin"];

/// Tokens that name a precision or quantisation format.
const CORE: &[&str] = &[
    "bf16",
    "fp16",
    "fp32",
    "f16",
    "f32",
    "fp8",
    "fp8mixed",
    "fp8scaled",
    "e4m3fn",
    "e5m2",
    "e4m3",
    "e4m3fnuz",
    "int8",
    "int4",
    "nvfp4",
    "mxfp8",
    "mxfp4",
    "fp4",
    "fp4mixed",
    "w4a8",
    "w4a16",
    "w8a8",
];

/// Words that qualify a precision (`fp8_scaled`, `int8_convrot`, `w4a8_gptq`).
const QUAL: &[&str] = &["scaled", "mixed", "convrot", "gptq", "awq", "quant", "quantized", "svdq"];

/// GGUF type suffixes (`Q4_K_M`, `Q8_0`, `IQ4_XS`); they only count right after a `q`/`iq` token.
const GGUF_SUFFIX: &[&str] = &["k", "s", "m", "l", "xs", "xxs", "nl", "0", "1"];

fn is_gguf_type(t: &str) -> bool {
    let digits = t.strip_prefix("iq").or_else(|| t.strip_prefix('q'));
    digits.is_some_and(|d| d.len() == 1 && matches!(d.as_bytes()[0], b'1'..=b'8'))
}

fn is_core(t: &str) -> bool {
    CORE.contains(&t) || is_gguf_type(t)
}

/// Splits a basename (extension removed) into `(token, start, end)` byte ranges.
fn tokens(s: &str) -> Vec<(String, usize, usize)> {
    let mut out = Vec::new();
    let mut start = 0;
    for (i, c) in s.char_indices() {
        if matches!(c, '_' | '-' | '.' | ' ') {
            if i > start {
                out.push((s[start..i].to_ascii_lowercase(), start, i));
            }
            start = i + c.len_utf8();
        }
    }
    if start < s.len() {
        out.push((s[start..].to_ascii_lowercase(), start, s.len()));
    }
    out
}

fn alnum_key(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_alphanumeric()).map(|c| c.to_ascii_lowercase()).collect()
}

/// The precision-independent key of a file name (a path is reduced to its basename).
pub fn stem_key(file_name: &str) -> StemKey {
    let path = file_name.replace('\\', "/");
    let base = path.rsplit('/').next().unwrap_or(&path);
    let (stem, ext) = match base.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() && EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()) => (s, e.to_ascii_lowercase()),
        _ => (base, String::new()),
    };
    let toks = tokens(stem);
    // Walk back from the end over precision tokens; GGUF suffixes need a q/iq token to their left.
    let mut first = toks.len();
    while first > 0 {
        let t = toks[first - 1].0.as_str();
        let accepted = if is_core(t) || QUAL.contains(&t) {
            true
        } else if GGUF_SUFFIX.contains(&t) {
            let mut j = first - 1;
            while j > 0 && GGUF_SUFFIX.contains(&toks[j - 1].0.as_str()) {
                j -= 1;
            }
            j > 0 && is_gguf_type(&toks[j - 1].0)
        } else {
            false
        };
        if !accepted {
            break;
        }
        first -= 1;
    }
    let run = &toks[first..];
    let strip = run.iter().any(|(t, _, _)| is_core(t)) && first > 0;
    let display = if strip { stem[..toks[first - 1].2].to_owned() } else { stem.to_owned() };
    if !strip || alnum_key(&display).len() < 3 {
        return StemKey { key: alnum_key(stem), display: stem.to_owned(), quant_tokens: Vec::new(), ext };
    }
    StemKey { key: alnum_key(&display), display, quant_tokens: run.iter().map(|(t, _, _)| t.clone()).collect(), ext }
}

/// The precision a file name states, from its stripped tokens (most specific first).
pub fn name_quant(tokens: &[String]) -> Option<QuantClass> {
    let has = |t: &str| tokens.iter().any(|x| x == t);
    if let Some(i) = tokens.iter().position(|t| is_gguf_type(t)) {
        return Some(QuantClass::Gguf(tokens[i..].iter().map(|t| t.to_ascii_uppercase()).collect::<Vec<_>>().join("_")));
    }
    Some(match () {
        _ if has("nvfp4") => QuantClass::Nvfp4,
        _ if has("mxfp8") => QuantClass::Mxfp8,
        _ if has("fp4mixed") || has("fp4") && has("mixed") => QuantClass::Fp4Mixed,
        _ if has("w4a8") => QuantClass::W4a8,
        _ if has("int8") && has("convrot") => QuantClass::Int8Convrot,
        _ if has("int8") => QuantClass::Int8,
        _ if has("fp8mixed") || has("fp8") && has("mixed") => QuantClass::Fp8Mixed,
        _ if has("fp8scaled") || (has("fp8") || has("e4m3fn")) && has("scaled") => QuantClass::Fp8Scaled,
        _ if has("e5m2") => QuantClass::Fp8E5m2,
        _ if has("fp8") || has("e4m3fn") || has("e4m3") => QuantClass::Fp8E4m3fn,
        _ if has("bf16") => QuantClass::Bf16,
        _ if has("fp16") || has("f16") => QuantClass::Fp16,
        _ if has("fp32") || has("f32") => QuantClass::Fp32,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn precision_suffixes_strip_to_one_key_per_model() {
        let rows: &[(&[&str], &str)] = &[
            (&["flux3-image-klein-bf16.safetensors", "flux3-image-klein-int8-convrot.safetensors", "flux-3-image-klein-Q4_K_M.gguf"], "flux3imageklein"),
            (&["flux3-image-klein-base-bf16"], "flux3imagekleinbase"),
            (&["flux3_image_klein_sd-Q8_0.gguf"], "flux3imagekleinsd"),
            (&["moss_image_sft_bf16.safetensors", "moss_image_sft_int8_convrot.safetensors"], "mossimagesft"),
            (
                &[
                    "krea2_turbo_bf16.safetensors",
                    "krea2_turbo_fp8_scaled.safetensors",
                    "krea2_turbo_mxfp8.safetensors",
                    "krea2_turbo_nvfp4.safetensors",
                    "krea2_turbo_int8_convrot.safetensors",
                ],
                "krea2turbo",
            ),
            (&["krea2_raw_bf16.safetensors"], "krea2raw"),
            (&["qwen3vl_4b_bf16.safetensors", "qwen3vl_4b_int8_convrot.safetensors", "qwen3vl_4b_fp8_scaled.safetensors"], "qwen3vl4b"),
            (&["qwen3.5_9b_w4a8_gptq.safetensors", "qwen3.5_9b_bf16.safetensors"], "qwen359b"),
            (&["qwen3.5_9b_flux3_prompt_upsampler_w4a8.safetensors"], "qwen359bflux3promptupsampler"),
            (&["qwen3.5_9b_qwen_image_2.1_pe_t2i.int8_convrot.safetensors"], "qwen359bqwenimage21pet2i"),
            (&["qwen_image_2.1_turbo_int8_convrot.safetensors"], "qwenimage21turbo"),
            (&["qwen_image_2.1_bf16.safetensors"], "qwenimage21"),
            (&["flux-2-klein-9b-fp8.safetensors"], "flux2klein9b"),
            (&["flux-2-klein-4b.safetensors"], "flux2klein4b"),
            (&["qwen_3_8b_fp8mixed.safetensors"], "qwen38b"),
            (&["t5xxl_fp8_e4m3fn_scaled.safetensors"], "t5xxl"),
            (&["mistral_3_small_flux2_fp4_mixed.safetensors"], "mistral3smallflux2"),
            (&["flux2-vae_small.safetensors"], "flux2vaesmall"),
            (&["flux2-vae.safetensors"], "flux2vae"),
            (&["model_mixed.safetensors"], "modelmixed"),
            (&["model_scaled.safetensors"], "modelscaled"),
            (&["RealVisXL_V5.0_fp16.safetensors"], "realvisxlv50"),
            (&["ae.safetensors"], "ae"),
            (&["sub/dir/flux2-vae.safetensors"], "flux2vae"),
        ];
        for (inputs, key) in rows {
            for input in *inputs {
                assert_eq!(stem_key(input).key, *key, "{input}");
            }
        }
        assert_ne!(stem_key("qwen_image_2.1_turbo_bf16").key, stem_key("qwen_image_2.1_bf16").key);
        assert_ne!(stem_key("ae.safetensors").key, stem_key("flux2-vae.safetensors").key);
        assert_eq!(stem_key("flux3-image-klein-int8-convrot.safetensors").display, "flux3-image-klein");
        assert_eq!(stem_key("flux-3-image-klein-Q4_K_M.gguf").ext, "gguf");
    }

    #[test]
    fn never_strips_below_three_characters() {
        for name in ["fp16.safetensors", "x_bf16.safetensors", "ab-fp8.safetensors"] {
            let k = stem_key(name);
            assert!(k.quant_tokens.is_empty(), "{name}: {k:?}");
            assert!(k.key.len() >= 3, "{name}");
        }
        assert_eq!(stem_key("abc_bf16.safetensors").key, "abc");
    }

    #[test]
    fn names_state_their_precision() {
        let q = |n: &str| name_quant(&stem_key(n).quant_tokens);
        assert_eq!(q("krea2_turbo_nvfp4.safetensors"), Some(QuantClass::Nvfp4));
        assert_eq!(q("krea2_turbo_mxfp8.safetensors"), Some(QuantClass::Mxfp8));
        assert_eq!(q("krea2_raw_fp8_scaled.safetensors"), Some(QuantClass::Fp8Scaled));
        assert_eq!(q("qwen_3_8b_fp8mixed.safetensors"), Some(QuantClass::Fp8Mixed));
        assert_eq!(q("mistral_3_small_flux2_fp4_mixed.safetensors"), Some(QuantClass::Fp4Mixed));
        assert_eq!(q("qwen3vl_8b_w4a8.safetensors"), Some(QuantClass::W4a8));
        assert_eq!(q("moss_image_sft_int8_convrot.safetensors"), Some(QuantClass::Int8Convrot));
        assert_eq!(q("t5xxl_fp8_e4m3fn.safetensors"), Some(QuantClass::Fp8E4m3fn));
        assert_eq!(q("x_model_bf16.safetensors"), Some(QuantClass::Bf16));
        assert_eq!(q("RealVisXL_V5.0_fp16.safetensors"), Some(QuantClass::Fp16));
        assert_eq!(q("flux-3-image-klein-Q4_K_M.gguf"), Some(QuantClass::Gguf("Q4_K_M".into())));
        assert_eq!(q("flux3_image_klein_sd-Q8_0.gguf"), Some(QuantClass::Gguf("Q8_0".into())));
        assert_eq!(q("flux-2-klein-4b.safetensors"), None);
        for class in [QuantClass::Bf16, QuantClass::Int8Convrot, QuantClass::Nvfp4, QuantClass::Gguf("Q4_K_M".into())] {
            assert_eq!(QuantClass::parse(&class.id()), Some(class.clone()), "{class:?}");
        }
    }
}
