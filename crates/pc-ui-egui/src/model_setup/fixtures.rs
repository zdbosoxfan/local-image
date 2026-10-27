//! Synthetic model states for interaction tests and the snapshot example.

use li_ai::arch::FileKind;
use li_ai::companions::{Binding, Why};
use li_ai::identify::{Identity, Quant, QuantClass};
use li_ai::modelsetup::check::CapabilityCheck;
use li_ai::modelsetup::state::{Fingerprint, Verified};
use li_ai::modelsetup::{Gpu, Snapshot, TemplateChoice};
use li_ai::template::index::Version;

use super::*;

pub(super) const FLUX3: &str = "unet:flux3-image-klein-bf16.safetensors";
pub(super) const MOSS: &str = "unet:moss_image_sft_bf16.safetensors";
pub(super) const KREA: &str = "unet:krea2_turbo_bf16.safetensors";
pub(super) const TURBO: &str = "unet:qwen_image_2.1_turbo_bf16.safetensors";
pub(super) const KLEIN9B: &str = "unet:flux-2-klein-9b-fp8.safetensors";
pub(super) const HIDREAM: &str = "ckpt:hidream_o1_image_bf16.safetensors";
pub(super) const MING: &str = "unet:ming_image_0.1_design_bf16.safetensors";
pub(super) const QWEN21: &str = "unet:qwen_image_2.1_bf16.safetensors";

pub(super) fn snapshot(version: &str) -> Snapshot {
    Snapshot {
        host: "127.0.0.1:8189".into(),
        comfyui_version: Version::parse(version),
        version_text: version.into(),
        templates_version: "0.11.96".into(),
        gpu: Some(Gpu { name: "NVIDIA GeForce RTX 5090".into(), vram_total: 33_731_248_128 }),
        ..Default::default()
    }
}

fn cap(name: &str, references: u32) -> CapabilityCheck {
    CapabilityCheck { name: name.into(), ok: true, references, detail: String::new() }
}

fn model(key: &str, label: &str, status: Status, arch: &str, variant: Option<&str>) -> ModelState {
    let file = key.trim_start_matches("unet:").trim_start_matches("ckpt:");
    ModelState {
        status,
        file: file.into(),
        folder: if key.starts_with("ckpt:") { "checkpoints".into() } else { "diffusion_models".into() },
        label: label.into(),
        engine: "template".into(),
        catalog_id: key.into(),
        fingerprint: Fingerprint { bytes: 11_097_404_512, ..Default::default() },
        identity: Some(Identity {
            kind: if key.starts_with("ckpt:") { FileKind::Checkpoint } else { FileKind::DiffusionModel },
            arch: Some(arch.to_owned()).filter(|a| !a.is_empty()),
            variant: variant.map(str::to_owned),
            quant: Quant { class: Some(QuantClass::Bf16), ..Default::default() },
            evidence: "content_mode_blocks + single_stream_modulations (FLUX.3)".into(),
            ..Default::default()
        }),
        capabilities: vec![cap("create", 0), cap("edit", 10), cap("refine", 0), cap("inpaint", 0), cap("upscale", 0)],
        ..Default::default()
    }
}

fn issue(code: &str, key: &str, args: &[(&str, &str)]) -> Issue {
    Issue::new(code, Some(key), &args.iter().map(|(k, v)| (*k, (*v).to_owned())).collect::<Vec<_>>())
}

/// One model in each state Set Up Models shows.
pub(super) fn fixture() -> Scan {
    let mut models = std::collections::BTreeMap::new();
    let mut flux3 = model(FLUX3, "FLUX.3 Klein Distilled · BF16", Status::Ready, "flux3-klein", Some("distilled"));
    flux3.templates = [("create".into(), "image_flux3_klein_distilled_t2i".into()), ("edit".into(), "image_flux3_klein_distilled_image_edit".into())].into();
    flux3.bindings = vec![
        Binding {
            role: "clip".into(),
            declared: "qwen3vl_4b_int8_convrot.safetensors".into(),
            used: Some("qwen3vl_4b_bf16.safetensors".into()),
            why: Why::SameStem,
            ..Default::default()
        },
        Binding {
            role: "vae".into(),
            declared: "flux2-vae.safetensors".into(),
            used: Some("flux2-vae.safetensors".into()),
            why: Why::Exact,
            ..Default::default()
        },
    ];
    flux3.issues = vec![
        issue("NotVerified", FLUX3, &[("model", "FLUX.3 Klein Distilled · BF16")]).fix(Fix::Verify { model: "flux3-image-klein-bf16.safetensors".into() }),
        issue("UpsamplerAvailable", FLUX3, &[("file", "qwen3.5_9b_flux3_prompt_upsampler_w4a8.safetensors"), ("size", "8.1 GB")]).fix(Fix::Download {
            files: vec!["qwen3.5_9b_flux3_prompt_upsampler_w4a8.safetensors".into()],
            folder: "text_encoders".into(),
            bytes: 8_100_000_000,
            url: "https://huggingface.co/Comfy-Org/flux3/resolve/main/text_encoders/qwen3.5_9b_flux3_prompt_upsampler_w4a8.safetensors".into(),
        }),
        issue("LicenceNotice", FLUX3, &[("licence", "FLUX Kommunity License v1.0"), ("summary", "Non-commercial.")]),
    ];
    models.insert(FLUX3.to_owned(), flux3);
    let mut moss = model(MOSS, "MOSS-Image SFT · BF16", Status::Blocked, "moss-image", Some("sft"));
    moss.bindings = vec![Binding {
        role: "vae".into(),
        declared: "flux2-vae.safetensors".into(),
        used: Some("flux2-vae.safetensors".into()),
        why: Why::WrongFolder,
        ..Default::default()
    }];
    moss.issues = vec![
        issue(
            "CompanionMissing",
            MOSS,
            &[
                ("model", "MOSS-Image SFT · BF16"),
                ("role_label", "text encoder"),
                ("file", "qwen3.5_9b_bf16.safetensors"),
                ("size", "19.3 GB"),
                ("folder", "text_encoders"),
                ("stem", "qwen3.5_9b"),
            ],
        )
        .fix(Fix::Download {
            files: vec!["qwen3.5_9b_bf16.safetensors".into()],
            folder: "text_encoders".into(),
            bytes: 19_306_312_328,
            url: "https://huggingface.co/Comfy-Org/MOSS-Image/resolve/main/text_encoders/qwen3.5_9b_bf16.safetensors".into(),
        })
        .fix(Fix::UseInstead { declared: "qwen3.5_9b_bf16.safetensors".into(), installed: "qwen3.5_9b_fp8.safetensors".into() }),
        issue(
            "WrongFolder",
            MOSS,
            &[("file", "flux2-vae.safetensors"), ("kind_label", "VAE"), ("kind_label_plural", "VAEs"), ("found", "diffusion_models"), ("expected", "vae")],
        )
        .fix(Fix::Move { from: "flux2-vae.safetensors".into(), to_folder: "vae".into() }),
        issue("ComfyTooOld", MOSS, &[("model", "MOSS-Image"), ("need", "0.44.0"), ("have", "0.43.0"), ("how", "Update ComfyUI.")])
            .fix(Fix::CopyCommand { label: "Update ComfyUI".into(), command: "git pull".into() }),
    ];
    moss.capabilities.clear();
    models.insert(MOSS.to_owned(), moss);
    let mut krea = model(KREA, "Krea 2 Turbo · BF16", Status::Verified, "krea2", Some("turbo"));
    let mut caps = std::collections::BTreeMap::new();
    caps.insert("create".to_owned(), CapabilityResult { pass: true, seconds: 5.5, cold: true, peak_vram_gb: Some(17.9), ..Default::default() });
    caps.insert("refine".to_owned(), CapabilityResult { pass: true, seconds: 3.9, difference: Some(11.6), ..Default::default() });
    caps.insert("upscale".to_owned(), CapabilityResult { pass: true, via: Some("refine".into()), ..Default::default() });
    krea.verified = Some(Verified {
        at: "2026-10-24T14:02:11-04:00".into(),
        seed: 518_877_231,
        comfyui_version: "0.42.5".into(),
        capabilities: caps,
        ..Default::default()
    });
    models.insert(KREA.to_owned(), krea);
    let mut turbo = model(TURBO, "Qwen Image 2.1 Turbo · BF16", Status::Failed, "qwen-image-21", Some("turbo"));
    let mut caps = std::collections::BTreeMap::new();
    caps.insert("create".to_owned(), CapabilityResult { pass: false, seconds: 7.0, failure: Some("VerifyFailed:noise".into()), ..Default::default() });
    turbo.verified = Some(Verified { at: "2026-10-24T14:10:00-04:00".into(), comfyui_version: "0.42.5".into(), capabilities: caps, ..Default::default() });
    turbo.issues = vec![issue("VerifyFailed:noise", TURBO, &[("capability", "Create"), ("vae", "qwen_image_vae.safetensors")])];
    models.insert(TURBO.to_owned(), turbo);
    let mut klein = model(KLEIN9B, "FLUX.2 Klein 9B · FP8", Status::NeedsReverify, "flux2-klein", Some("9b"));
    klein.issues = vec![issue("ReverifyNeeded", KLEIN9B, &[("old", "0.42.5"), ("new", "0.43.0"), ("model", "FLUX.2 Klein 9B · FP8")])];
    models.insert(KLEIN9B.to_owned(), klein);
    let mut hidream = model(HIDREAM, "HiDream O1 · BF16", Status::Blocked, "", None);
    hidream.issues = vec![issue("NodeMissing", HIDREAM, &[("have", "0.43.0"), ("class", "HiDreamO1Sampler"), ("model", "HiDream O1"), ("need_clause", "")])];
    models.insert(HIDREAM.to_owned(), hidream);
    let mut ming = model(MING, "Ming Image · BF16", Status::Unmatched, "", None);
    ming.engine = "unsupported".into();
    ming.issues = vec![issue("NoTemplate", MING, &[("file", "ming_image_0.1_design_bf16.safetensors")]).fix(Fix::CheckOnline).fix(Fix::UpdateProfiles)];
    ming.capabilities.clear();
    models.insert(MING.to_owned(), ming);
    let mut qwen = model(QWEN21, "Qwen Image 2.1 · BF16", Status::Builtin, "qwen-image-21", None);
    qwen.engine = "builtin".into();
    models.insert(QWEN21.to_owned(), qwen);
    models.insert("unet:old_test.safetensors".to_owned(), model("unet:old_test.safetensors", "Old test", Status::Ignored, "", None));
    Scan {
        snapshot: Some(snapshot("0.43.0")),
        server: li_ai::modelsetup::state::ServerState { comfyui_version: "0.43.0".into(), models, ..Default::default() },
        other_files: vec![li_ai::modelsetup::report::FileReport {
            file: "qwen3vl_4b_bf16.safetensors".into(),
            kind: "text encoder".into(),
            used_by: vec!["FLUX.3 Klein Distilled · BF16".into()],
        }],
        templates: vec![
            TemplateChoice { name: "image_flux3_klein_distilled_t2i".into(), title: "FLUX 3 Image Klein Distilled: Text to Image".into() },
            TemplateChoice { name: "image_moss_image_t2i".into(), title: "MOSS-Image: Text to Image".into() },
        ],
        ..Default::default()
    }
}
