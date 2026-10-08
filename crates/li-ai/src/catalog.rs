//! The local model catalog: what each model is good at, its inputs and sampling ranges, the exact
//! files each precision preset needs (pinned publisher revisions, sizes and SHA-256), and whether a
//! running ComfyUI can use it. Data carried over unchanged from Local Image 0.7.

use crate::comfy::ObjectInfo;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub enum ModelId {
    Qwen,
    ZImageTurbo,
    Klein4B,
    Klein9B,
    Ernie,
    SeedVr2,
    /// FLUX.2 Klein base 4B + fal's object-removal LoRA (AI Remove).
    KleinRemove,
}

impl ModelId {
    pub const GENERATORS: [ModelId; 5] = [ModelId::Qwen, ModelId::ZImageTurbo, ModelId::Klein4B, ModelId::Klein9B, ModelId::Ernie];
    pub const ALL: [ModelId; 7] =
        [ModelId::Qwen, ModelId::ZImageTurbo, ModelId::Klein4B, ModelId::Klein9B, ModelId::Ernie, ModelId::SeedVr2, ModelId::KleinRemove];

    /// The id used in 0.7 settings, the generation library and LoRA registries.
    pub fn key(self) -> &'static str {
        match self {
            ModelId::Qwen => "qwen",
            ModelId::ZImageTurbo => "z-image-turbo",
            ModelId::Klein4B => "flux2-klein-4b",
            ModelId::Klein9B => "flux2-klein-9b",
            ModelId::Ernie => "ernie-image",
            ModelId::SeedVr2 => "seedvr2",
            ModelId::KleinRemove => "klein-remove",
        }
    }
    pub fn from_key(k: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.key() == k)
    }
    pub fn info(self) -> &'static ModelInfo {
        MODELS.iter().find(|m| m.id == self).expect("every model has info")
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Range {
    pub default: f32,
    pub min: f32,
    pub max: f32,
}

impl Range {
    pub fn fixed(self) -> bool {
        self.min == self.max
    }
}

#[derive(Debug)]
pub struct ModelInfo {
    pub id: ModelId,
    pub label: &'static str,
    pub short: &'static str,
    pub best_for: &'static str,
    pub description: &'static str,
    pub max_references: usize,
    /// The one image input is a starting image for variations (Z-Image), not a semantic reference.
    pub init_image: bool,
    pub text_to_image: bool,
    pub transparent: bool,
    pub negative_prompt: bool,
    pub steps: Range,
    pub guidance: Range,
    pub lora: bool,
    pub vram_gb: u32,
    pub license: &'static str,
    pub license_url: &'static str,
    pub notes: &'static str,
}

pub static MODELS: &[ModelInfo] = &[
    ModelInfo {
        id: ModelId::Qwen,
        label: "Qwen Image 2.1",
        short: "Qwen",
        best_for: "Edits, background removal and transparent assets",
        description: "A 7B image model that follows instructions closely. Edits with up to ten reference images, removes backgrounds with a real alpha channel and generates transparent PNGs.",
        max_references: 10,
        init_image: false,
        text_to_image: true,
        transparent: true,
        negative_prompt: true,
        steps: Range { default: 40.0, min: 1.0, max: 100.0 },
        guidance: Range { default: 1.0, min: 1.0, max: 10.0 },
        lora: true,
        vram_gb: 24,
        license: "Qwen Research License",
        license_url: "https://github.com/QwenLM/Qwen-Image-2.1/blob/main/LICENSE",
        notes: "Compact INT8 needs about 24 GB of GPU memory, Full BF16 about 32 GB. Negative prompts only take effect with guidance above 1.",
    },
    ModelInfo {
        id: ModelId::ZImageTurbo,
        label: "Z-Image Turbo",
        short: "Z-Image",
        best_for: "Fast photographic generation",
        description: "A fast 8-step generator with strong photographic detail. Takes one starting image for variations.",
        max_references: 1,
        init_image: true,
        text_to_image: true,
        transparent: false,
        negative_prompt: false,
        steps: Range { default: 8.0, min: 1.0, max: 50.0 },
        guidance: Range { default: 1.0, min: 1.0, max: 1.0 },
        lora: true,
        vram_gb: 16,
        license: "Apache 2.0",
        license_url: "https://huggingface.co/Tongyi-MAI/Z-Image-Turbo",
        notes: "Variation strength sets how far a result moves from the starting image.",
    },
    ModelInfo {
        id: ModelId::Klein4B,
        label: "FLUX.2 Klein 4B",
        short: "Klein 4B",
        best_for: "Fast drafts and reference editing",
        description: "A distilled 4-step FLUX.2 model. Edits from up to four reference images; ideal for quick drafts.",
        max_references: 4,
        init_image: false,
        text_to_image: true,
        transparent: false,
        negative_prompt: false,
        steps: Range { default: 4.0, min: 4.0, max: 4.0 },
        guidance: Range { default: 1.0, min: 1.0, max: 1.0 },
        lora: true,
        vram_gb: 16,
        license: "Apache 2.0",
        license_url: "https://huggingface.co/black-forest-labs/FLUX.2-klein-4B",
        notes: "",
    },
    ModelInfo {
        id: ModelId::Klein9B,
        label: "FLUX.2 Klein 9B",
        short: "Klein 9B",
        best_for: "Higher-quality reference editing",
        description: "The larger distilled FLUX.2 Klein model, with up to four reference images.",
        max_references: 4,
        init_image: false,
        text_to_image: true,
        transparent: false,
        negative_prompt: false,
        steps: Range { default: 4.0, min: 4.0, max: 4.0 },
        guidance: Range { default: 1.0, min: 1.0, max: 1.0 },
        lora: true,
        vram_gb: 24,
        license: "FLUX non-commercial licence",
        license_url: "https://huggingface.co/black-forest-labs/FLUX.2-klein-9B/blob/main/LICENSE.md",
        notes: "The publisher requires access approval on Hugging Face before the weights can be downloaded.",
    },
    ModelInfo {
        id: ModelId::Ernie,
        label: "ERNIE-Image Base",
        short: "ERNIE",
        best_for: "Posters, typography and graphic layouts",
        description: "A text-to-image model tuned for legible lettering and layout.",
        max_references: 0,
        init_image: false,
        text_to_image: true,
        transparent: false,
        negative_prompt: true,
        steps: Range { default: 50.0, min: 1.0, max: 100.0 },
        guidance: Range { default: 4.0, min: 1.0, max: 10.0 },
        lora: false,
        vram_gb: 24,
        license: "Apache 2.0",
        license_url: "https://huggingface.co/baidu/ERNIE-Image",
        notes: "No model guarantees exact text; check lettering before using a result.",
    },
    ModelInfo {
        id: ModelId::SeedVr2,
        label: "SeedVR2 7B",
        short: "SeedVR2",
        best_for: "Photo enhancement and upscaling up to 4K",
        description: "One-step restoration upscaler. Keeps the source's alpha channel.",
        max_references: 1,
        init_image: true,
        text_to_image: false,
        transparent: false,
        negative_prompt: false,
        steps: Range { default: 1.0, min: 1.0, max: 1.0 },
        guidance: Range { default: 1.0, min: 1.0, max: 1.0 },
        lora: false,
        vram_gb: 32,
        license: "Apache 2.0",
        license_url: "https://huggingface.co/ByteDance-Seed/SeedVR2-7B",
        notes: "Very large targets are slow; 4K needs about 32 GB of GPU memory.",
    },
    ModelInfo {
        id: ModelId::KleinRemove,
        label: "FLUX.2 Klein AI Remove",
        short: "AI Remove",
        best_for: "Removing objects and people",
        description: "FLUX.2 Klein base 4B with fal's object-removal LoRA. Powers the AI Remove brush.",
        max_references: 0,
        init_image: true,
        text_to_image: false,
        transparent: false,
        negative_prompt: false,
        steps: Range { default: 28.0, min: 28.0, max: 28.0 },
        guidance: Range { default: 4.0, min: 4.0, max: 4.0 },
        lora: false,
        vram_gb: 16,
        license: "Apache 2.0",
        license_url: "https://huggingface.co/fal/flux-2-klein-4B-object-remove-lora",
        notes: "",
    },
];

/// Which loader node reads a file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Role {
    Unet,
    Clip,
    Vae,
    Lora,
}

impl Role {
    pub fn loader(self) -> (&'static str, &'static str) {
        match self {
            Role::Unet => ("UNETLoader", "unet_name"),
            Role::Clip => ("CLIPLoader", "clip_name"),
            Role::Vae => ("VAELoader", "vae_name"),
            Role::Lora => ("LoraLoaderModelOnly", "lora_name"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileSpec {
    pub role: Role,
    /// Folder under the model directory.
    pub folder: &'static str,
    pub name: &'static str,
    pub bytes: u64,
    pub sha256: &'static str,
    pub url: &'static str,
    /// Other accepted `(bytes, sha256)` for a file that is already present.
    pub compatible: &'static [(u64, &'static str)],
}

#[derive(Debug)]
pub struct Preset {
    pub model: ModelId,
    pub variant: &'static str,
    pub label: &'static str,
    pub files: &'static [FileSpec],
    /// Set when the publisher gates the weights (approval on Hugging Face).
    pub access_url: Option<&'static str>,
    pub required_nodes: &'static [&'static str],
    /// `(class, input, value)` combo values that must exist.
    pub required_choices: &'static [(&'static str, &'static str, &'static str)],
}

impl Preset {
    pub fn total_bytes(&self) -> u64 {
        self.files.iter().map(|f| f.bytes).sum()
    }
    pub fn id(&self) -> String {
        format!("{}:{}", self.model.key(), self.variant)
    }
}

const QWEN_BASE: &str = "https://huggingface.co/Comfy-Org/Qwen-Image-2.1/resolve/cb504a4090723e43f17ad01cec0359490e2de613";
const FLUX_VAE_COMPAT: &[(u64, &str)] = &[
    (336213556, "d64f3a68e1cc4f9f4e29b6e0da38a0204fe9a49f2d4053f0ec1fa1ca02f9c4b5"),
    (336211292, "868fe7b343cc8f3a19dbcfcafbc3d5f888802be3f89bd81b65b3621a066ce8f3"),
];

const QWEN_VAE: FileSpec = FileSpec {
    role: Role::Vae,
    folder: "vae",
    name: "qwen_image_2.1_vae_bf16.safetensors",
    bytes: 675509688,
    sha256: "bb21f7473051e1ac368515dd3f2e15cd44d7a11748ee8823e1ddca3e4876b7c9",
    url: concat!(
        "https://huggingface.co/Comfy-Org/Qwen-Image-2.1/resolve/cb504a4090723e43f17ad01cec0359490e2de613",
        "/vae/qwen_image_2.1_vae_bf16.safetensors"
    ),
    compatible: &[],
};
const QWEN_3_4B: FileSpec = FileSpec {
    role: Role::Clip,
    folder: "text_encoders",
    name: "qwen_3_4b.safetensors",
    bytes: 8044982048,
    sha256: "6c671498573ac2f7a5501502ccce8d2b08ea6ca2f661c458e708f36b36edfc5a",
    url: "https://huggingface.co/Comfy-Org/z_image_turbo/resolve/6fc90a3b1b653e935a0d175e260736de25b84df5/split_files/text_encoders/qwen_3_4b.safetensors",
    compatible: &[],
};
const FLUX2_VAE: FileSpec = FileSpec {
    role: Role::Vae,
    folder: "vae",
    name: "flux2-vae.safetensors",
    bytes: 336213556,
    sha256: "d64f3a68e1cc4f9f4e29b6e0da38a0204fe9a49f2d4053f0ec1fa1ca02f9c4b5",
    url: "https://huggingface.co/Comfy-Org/flux2-dev/resolve/ed33133cd56476eac818c0943b6f9419b3e4a3a1/split_files/vae/flux2-vae.safetensors",
    compatible: FLUX_VAE_COMPAT,
};

const FLUX_NODES: &[&str] = &[
    "UNETLoader",
    "CLIPLoader",
    "VAELoader",
    "CLIPTextEncode",
    "EmptyFlux2LatentImage",
    "Flux2Scheduler",
    "RandomNoise",
    "KSamplerSelect",
    "SamplerCustomAdvanced",
    "CFGGuider",
    "ReferenceLatent",
    "VAEEncode",
    "VAEDecode",
    "LoadImage",
];

pub static PRESETS: &[Preset] = &[
    Preset {
        model: ModelId::Qwen,
        variant: "int8",
        label: "Compact · INT8",
        files: &[
            FileSpec {
                role: Role::Unet,
                folder: "diffusion_models",
                name: "qwen_image_2.1_int8_convrot.safetensors",
                bytes: 7256783064,
                sha256: "cb74113cb03faecd79611b01fd7fd642f0aa60d6f0b95086abee214d75eaa57d",
                url: concat!(
                    "https://huggingface.co/Comfy-Org/Qwen-Image-2.1/resolve/cb504a4090723e43f17ad01cec0359490e2de613",
                    "/diffusion_models/qwen_image_2.1_int8_convrot.safetensors"
                ),
                compatible: &[],
            },
            FileSpec {
                role: Role::Clip,
                folder: "text_encoders",
                name: "qwen3vl_8b_int8_convrot.safetensors",
                bytes: 9350798360,
                sha256: "8bfd0f6e12abf2d2d697ecc888e5e90b0d6741d6708f05799f53afa560452e8f",
                url: concat!(
                    "https://huggingface.co/Comfy-Org/Qwen-Image-2.1/resolve/cb504a4090723e43f17ad01cec0359490e2de613",
                    "/text_encoders/qwen3vl_8b_int8_convrot.safetensors"
                ),
                compatible: &[],
            },
            QWEN_VAE,
        ],
        access_url: None,
        required_nodes: &[
            "UNETLoader",
            "CLIPLoader",
            "VAELoader",
            "TextEncodeQwenImage21",
            "KSampler",
            "VAEDecode",
            "SaveImage",
            "LoadImage",
            "JoinImageWithAlpha",
            "EmptyLatentImage",
        ],
        required_choices: &[("CLIPLoader", "type", "qwen_image")],
    },
    Preset {
        model: ModelId::Qwen,
        variant: "bf16",
        label: "Full precision · BF16",
        files: &[
            FileSpec {
                role: Role::Unet,
                folder: "diffusion_models",
                name: "qwen_image_2.1_bf16.safetensors",
                bytes: 14230280616,
                sha256: "89f4158d066cc33906a199fca85634f766892dd78f49b6698dabf187ac86c4bc",
                url: concat!(
                    "https://huggingface.co/Comfy-Org/Qwen-Image-2.1/resolve/cb504a4090723e43f17ad01cec0359490e2de613",
                    "/diffusion_models/qwen_image_2.1_bf16.safetensors"
                ),
                compatible: &[],
            },
            FileSpec {
                role: Role::Clip,
                folder: "text_encoders",
                name: "qwen3vl_8b_bf16.safetensors",
                bytes: 17534334616,
                sha256: "68bdc82bc1b66851162ae656225e7e2068166b603db19bd5d5a3b90eb12669a9",
                url: concat!(
                    "https://huggingface.co/Comfy-Org/Qwen-Image-2.1/resolve/cb504a4090723e43f17ad01cec0359490e2de613",
                    "/text_encoders/qwen3vl_8b_bf16.safetensors"
                ),
                compatible: &[],
            },
            QWEN_VAE,
        ],
        access_url: None,
        required_nodes: &[
            "UNETLoader",
            "CLIPLoader",
            "VAELoader",
            "TextEncodeQwenImage21",
            "KSampler",
            "VAEDecode",
            "SaveImage",
            "LoadImage",
            "JoinImageWithAlpha",
            "EmptyLatentImage",
        ],
        required_choices: &[("CLIPLoader", "type", "qwen_image")],
    },
    Preset {
        model: ModelId::ZImageTurbo,
        variant: "bf16",
        label: "BF16",
        files: &[
            FileSpec {
                role: Role::Unet,
                folder: "diffusion_models",
                name: "z_image_turbo_bf16.safetensors",
                bytes: 12309866400,
                sha256: "2407613050b809ffdff18a4ac99af83ea6b95443ecebdf80e064a79c825574a6",
                url: "https://huggingface.co/Comfy-Org/z_image_turbo/resolve/6fc90a3b1b653e935a0d175e260736de25b84df5/split_files/diffusion_models/z_image_turbo_bf16.safetensors",
                compatible: &[],
            },
            QWEN_3_4B,
            FileSpec {
                role: Role::Vae,
                folder: "vae",
                name: "ae.safetensors",
                bytes: 335304388,
                sha256: "afc8e28272cd15db3919bacdb6918ce9c1ed22e96cb12c4d5ed0fba823529e38",
                url: "https://huggingface.co/Comfy-Org/z_image_turbo/resolve/6fc90a3b1b653e935a0d175e260736de25b84df5/split_files/vae/ae.safetensors",
                compatible: &[],
            },
        ],
        access_url: None,
        required_nodes: &[
            "UNETLoader",
            "CLIPLoader",
            "VAELoader",
            "CLIPTextEncode",
            "ConditioningZeroOut",
            "ModelSamplingAuraFlow",
            "KSampler",
            "VAEDecode",
            "VAEEncode",
            "EmptySD3LatentImage",
            "LoadImage",
        ],
        required_choices: &[("CLIPLoader", "type", "lumina2"), ("KSampler", "sampler_name", "res_multistep")],
    },
    Preset {
        model: ModelId::Klein4B,
        variant: "bf16",
        label: "BF16",
        files: &[
            FileSpec {
                role: Role::Unet,
                folder: "diffusion_models",
                name: "flux-2-klein-4b.safetensors",
                bytes: 7751105712,
                sha256: "ec3d4e733a771f61c052fb4856c48b336c55eaf2c65487c2a1faeb9bbda7a343",
                url: "https://huggingface.co/black-forest-labs/FLUX.2-klein-4B/resolve/e7b7dc27f91deacad38e78976d1f2b499d76a294/flux-2-klein-4b.safetensors",
                compatible: &[],
            },
            QWEN_3_4B,
            FLUX2_VAE,
        ],
        access_url: None,
        required_nodes: FLUX_NODES,
        required_choices: &[("CLIPLoader", "type", "flux2"), ("KSamplerSelect", "sampler_name", "euler")],
    },
    Preset {
        model: ModelId::Klein9B,
        variant: "fp8",
        label: "FP8",
        files: &[
            FileSpec {
                role: Role::Unet,
                folder: "diffusion_models",
                name: "flux-2-klein-9b-fp8.safetensors",
                bytes: 9433061528,
                sha256: "865ba09f5b4c3cbd3468a4bd3acb9fcb2f8740c54317482f0bcd4ed1d3655cee",
                url: "https://huggingface.co/black-forest-labs/FLUX.2-klein-9b-fp8/resolve/902d9d510b51533e07729f19211414a3648b77d2/flux-2-klein-9b-fp8.safetensors",
                compatible: &[],
            },
            FileSpec {
                role: Role::Clip,
                folder: "text_encoders",
                name: "qwen_3_8b_fp8mixed.safetensors",
                bytes: 8664848742,
                sha256: "abad16806e0cbabc54e0325d6565847443fe396d5f0be38bb3cd3fe75a1201d6",
                url: "https://huggingface.co/Comfy-Org/flux2-klein-9B/resolve/3f62d9d8ae1fec33c6e91453d5c712855b096b55/split_files/text_encoders/qwen_3_8b_fp8mixed.safetensors",
                compatible: &[],
            },
            FLUX2_VAE,
        ],
        access_url: Some("https://huggingface.co/black-forest-labs/FLUX.2-klein-9b-fp8"),
        required_nodes: FLUX_NODES,
        required_choices: &[("CLIPLoader", "type", "flux2"), ("KSamplerSelect", "sampler_name", "euler")],
    },
    Preset {
        model: ModelId::Ernie,
        variant: "bf16",
        label: "Base BF16",
        files: &[
            FileSpec {
                role: Role::Unet,
                folder: "diffusion_models",
                name: "ernie-image.safetensors",
                bytes: 16067025480,
                sha256: "94a35abaa0899cccc34d2e37310abf74a0a714256526117bba782c7eb4eb91c7",
                url: "https://huggingface.co/Comfy-Org/ERNIE-Image/resolve/82fe29a5cd056f8b1deebc50570f125bcd4f4bea/split_files/diffusion_models/ernie-image.safetensors",
                compatible: &[],
            },
            FileSpec {
                role: Role::Clip,
                folder: "text_encoders",
                name: "ministral-3-3b.safetensors",
                bytes: 7717637511,
                sha256: "49a750a128863854eac7d85e1a277a7b44bf6ec3646405b84686dfeeca3708ca",
                url: "https://huggingface.co/Comfy-Org/ERNIE-Image/resolve/82fe29a5cd056f8b1deebc50570f125bcd4f4bea/split_files/text_encoders/ministral-3-3b.safetensors",
                compatible: &[],
            },
            FileSpec {
                role: Role::Vae,
                folder: "vae",
                name: "flux2-vae.safetensors",
                bytes: 336213556,
                sha256: "d64f3a68e1cc4f9f4e29b6e0da38a0204fe9a49f2d4053f0ec1fa1ca02f9c4b5",
                url: "https://huggingface.co/Comfy-Org/ERNIE-Image/resolve/82fe29a5cd056f8b1deebc50570f125bcd4f4bea/split_files/vae/flux2-vae.safetensors",
                compatible: FLUX_VAE_COMPAT,
            },
        ],
        access_url: None,
        required_nodes: &["UNETLoader", "CLIPLoader", "VAELoader", "CLIPTextEncode", "EmptyFlux2LatentImage", "KSampler", "VAEDecode"],
        required_choices: &[("CLIPLoader", "type", "flux2")],
    },
    Preset {
        model: ModelId::SeedVr2,
        variant: "fp16",
        label: "7B FP16",
        files: &[
            FileSpec {
                role: Role::Unet,
                folder: "diffusion_models",
                name: "seedvr2_7b_fp16.safetensors",
                bytes: 16480583960,
                sha256: "2742ca6fee63bc5cc1773f426dd4b07b78cad27f51c9ea5cd42b035e6b592252",
                url: "https://huggingface.co/Comfy-Org/SeedVR2/resolve/df48879708206a403d2a61acd55578c2e80fd233/split_files/diffusion_models/seedvr2_7b_fp16.safetensors",
                compatible: &[],
            },
            FileSpec {
                role: Role::Vae,
                folder: "vae",
                name: "seedvr2_ema_vae_fp16.safetensors",
                bytes: 501324814,
                sha256: "20678548f420d98d26f11442d3528f8b8c94e57ee046ef93dbb7633da8612ca1",
                url: "https://huggingface.co/Comfy-Org/SeedVR2/resolve/df48879708206a403d2a61acd55578c2e80fd233/split_files/vae/seedvr2_ema_vae_fp16.safetensors",
                compatible: &[],
            },
        ],
        access_url: None,
        required_nodes: &[
            "UNETLoader",
            "VAELoader",
            "LoadImage",
            "SeedVR2Preprocess",
            "VAEEncodeTiled",
            "SeedVR2Conditioning",
            "KSampler",
            "VAEDecodeTiled",
            "SeedVR2PostProcessing",
            "SaveImage",
        ],
        required_choices: &[],
    },
    Preset {
        model: ModelId::KleinRemove,
        variant: "bf16",
        label: "Base 4B + removal LoRA",
        files: &[
            FileSpec {
                role: Role::Unet,
                folder: "diffusion_models",
                name: "flux-2-klein-base-4b.safetensors",
                bytes: 7751105712,
                sha256: "9c5fed22b76baea749d88fc2abe3ad53245e7b21a0d353a762665eea00043b92",
                url: "https://huggingface.co/black-forest-labs/FLUX.2-klein-base-4B/resolve/a3b4f4849157f664bdbc776fd7453c2783562f4d/flux-2-klein-base-4b.safetensors",
                compatible: &[],
            },
            FileSpec {
                role: Role::Clip,
                folder: "text_encoders",
                name: "qwen_3_4b.safetensors",
                bytes: 8044982048,
                sha256: "6c671498573ac2f7a5501502ccce8d2b08ea6ca2f661c458e708f36b36edfc5a",
                url: "https://huggingface.co/Comfy-Org/vae-text-encorder-for-flux-klein-4b/resolve/5f526678002e43af5551dadb73ce2e8c91b43afe/split_files/text_encoders/qwen_3_4b.safetensors",
                compatible: &[],
            },
            FileSpec {
                role: Role::Vae,
                folder: "vae",
                name: "flux2-vae.safetensors",
                bytes: 336211292,
                sha256: "868fe7b343cc8f3a19dbcfcafbc3d5f888802be3f89bd81b65b3621a066ce8f3",
                url: "https://huggingface.co/Comfy-Org/vae-text-encorder-for-flux-klein-4b/resolve/5f526678002e43af5551dadb73ce2e8c91b43afe/split_files/vae/flux2-vae.safetensors",
                compatible: FLUX_VAE_COMPAT,
            },
            FileSpec {
                role: Role::Lora,
                folder: "loras",
                name: "flux-2-klein-object-remove.safetensors",
                bytes: 76038936,
                sha256: "dc197de62e174863f83fc4052465603f4389de7c3a78e17f3d41a1f6f11488ec",
                url: "https://huggingface.co/fal/flux-2-klein-4B-object-remove-lora/resolve/0e3f58790356bf1319b263fc56b333c294b42ff7/kDEkt5q7tDLKOpQJIVMPx_pytorch_lora_weights_comfy_converted.safetensors",
                compatible: &[],
            },
        ],
        access_url: None,
        required_nodes: FLUX_NODES,
        required_choices: &[("CLIPLoader", "type", "flux2"), ("KSamplerSelect", "sampler_name", "euler")],
    },
];

#[allow(dead_code)]
const _QWEN_BASE_USED: &str = QWEN_BASE;

pub fn presets_for(model: ModelId) -> impl Iterator<Item = &'static Preset> {
    PRESETS.iter().filter(move |p| p.model == model)
}

pub fn preset(model: ModelId, variant: &str) -> Option<&'static Preset> {
    presets_for(model).find(|p| p.variant == variant)
}

pub fn default_variant(model: ModelId) -> &'static str {
    presets_for(model).next().map(|p| p.variant).unwrap_or("bf16")
}

/// Whether a preset can run on the connected ComfyUI, and the loader names to use.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Availability {
    pub available: bool,
    pub reason: String,
    pub files: BTreeMap<Role, String>,
}

pub fn availability(preset: &Preset, info: &ObjectInfo) -> Availability {
    let missing_nodes: Vec<&str> = preset.required_nodes.iter().copied().filter(|n| !info.has_node(n)).collect();
    if !missing_nodes.is_empty() {
        return Availability { reason: format!("Update ComfyUI: it is missing the nodes {}.", missing_nodes.join(", ")), ..Default::default() };
    }
    for (class, input, value) in preset.required_choices {
        if !info.choices(class, input).iter().any(|c| c == value) {
            return Availability { reason: format!("Update ComfyUI: {class} does not offer “{value}”."), ..Default::default() };
        }
    }
    let mut files = BTreeMap::new();
    let mut missing = Vec::new();
    for f in preset.files {
        let (class, input) = f.role.loader();
        match info.find_file(class, input, f.name) {
            Some(found) => {
                files.insert(f.role, found);
            }
            None => missing.push(f.name),
        }
    }
    if !missing.is_empty() {
        return Availability {
            reason: format!("Download the {} {} model files ({}), then refresh.", preset.model.info().label, preset.label, missing.join(", ")),
            files,
            ..Default::default()
        };
    }
    Availability { available: true, reason: String::new(), files }
}

/// Planning recommendation shown in the hardware guide.
pub const VRAM_GUIDE: &[(&str, u32)] = &[
    ("Z-Image Turbo, FLUX.2 Klein 4B and AI Remove", 16),
    ("Qwen Compact INT8, Klein 9B and ERNIE-Image", 24),
    ("Qwen Full BF16 and SeedVR2 4K enhancement", 32),
];

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn every_model_has_a_preset_and_hashes_are_hex() {
        for m in ModelId::ALL {
            assert!(presets_for(m).next().is_some(), "{m:?}");
            assert_eq!(ModelId::from_key(m.key()), Some(m));
        }
        for p in PRESETS {
            for f in p.files {
                assert_eq!(f.sha256.len(), 64);
                assert!(f.sha256.bytes().all(|b| b.is_ascii_hexdigit()));
                assert!(f.url.starts_with("https://huggingface.co/"));
                assert!(f.url.ends_with(".safetensors"));
            }
        }
    }

    #[test]
    fn availability_reports_missing_nodes_then_files() {
        let p = preset(ModelId::Ernie, "bf16").unwrap();
        let empty = ObjectInfo(json!({}));
        assert!(availability(p, &empty).reason.contains("Update ComfyUI"));
        let mut info = json!({});
        for n in p.required_nodes {
            info[*n] = json!({"input": {"required": {}}});
        }
        info["CLIPLoader"] = json!({"input": {"required": {"type": [["flux2"]], "clip_name": [["sub/Ministral-3-3B.safetensors"]]}}});
        info["UNETLoader"] = json!({"input": {"required": {"unet_name": [["ernie-image.safetensors"]]}}});
        let a = availability(p, &ObjectInfo(info.clone()));
        assert!(!a.available && a.reason.contains("flux2-vae"), "{a:?}");
        info["VAELoader"] = json!({"input": {"required": {"vae_name": [["flux2-vae.safetensors"]]}}});
        let a = availability(p, &ObjectInfo(info));
        assert!(a.available, "{a:?}");
        assert_eq!(a.files[&Role::Clip], "sub/Ministral-3-3B.safetensors");
    }
}
