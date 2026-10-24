//! Local segmentation on the CPU: BRIA RMBG-1.4 (Fast) or InSPyReNet (Better Quality) for the subject,
//! PP-MobileSeg for the sky, Depth Anything V2 Small for depth, run with `tract` (pure Rust, no
//! GPU, no external runtime), or a custom `.onnx` file the user picks for a function. Powers
//! Select › Subject and Sky, Remove Background (Fast / Better Quality), the Object Selection tool's click mode,
//! and the Library's Subject / Sky / Background / Depth masks — small, quick models that need no
//! AI server. Smart Sort uses a paired CLIP image/text bundle through [`Clip`].
//!
//! Pre-processing uses premultiplied square resize and the model's RGB mean/std. The new
//! subject tiers use x/255 and clamped probabilities; custom IS-Net and U²-Net style
//! models use peak scaling and min–max output, after a sigmoid when they output logits (found
//! when the model is added). Maps are bilinearly resized to the photo.

pub mod bpe;
pub mod clip;
pub use clip::{Clip, Crops, shared_clip};

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, Result, bail};
use tract_onnx::prelude::*;
use tract_onnx::tract_hir::infer::Factoid;

pub mod denoise;
pub mod enhance;
pub mod estimate;
mod external_data;
pub mod faces;
pub mod lama;
pub mod restoration;
pub mod restoration_graph;

/// Another file required by a multi-file model (weights or tokenizer data).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Companion {
    pub file: &'static str,
    pub url: &'static str,
    pub bytes: u64,
    pub sha256: &'static str,
}

/// A downloadable model: (file name, bytes, sha256, url, input size, label).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModelSpec {
    pub id: &'static str,
    pub label: &'static str,
    pub file: &'static str,
    pub companions: &'static [Companion],
    pub bytes: u64,
    pub sha256: &'static str,
    pub url: &'static str,
    pub size: usize,
    pub isnet: bool,
    pub licence: &'static str,
    /// What the model finds.
    pub task: Task,
    /// The function Settings lists it under; `None`: listed on its own.
    pub group: Option<Group>,
    /// Plain-language description: what it does, the speed / quality trade-off.
    pub about: &'static str,
}

/// A function the on-device models serve (Settings › Local AI lists one row per function).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Group {
    /// The salient subject: Select Subject, Remove Background (Fast / Better Quality), Object Selection.
    Subject,
    Sky,
    Depth,
    Tagging,
    Faces,
    RawDenoise,
    RgbDenoise,
    Remove,
}

impl Group {
    pub const ALL: [Group; 8] = [Group::Subject, Group::Sky, Group::Depth, Group::Tagging, Group::Faces, Group::RawDenoise, Group::RgbDenoise, Group::Remove];

    pub fn label(self) -> &'static str {
        match self {
            Group::Subject => "Subject & Background",
            Group::Sky => "Sky",
            Group::Depth => "Depth",
            Group::Tagging => "Smart Sort (scenes)",
            Group::Faces => "Faces",
            Group::RawDenoise => "Raw Denoise",
            Group::RgbDenoise => "RGB Denoise",
            Group::Remove => "AI Remove",
        }
    }

    /// A short stable key (custom-model file names, the custom-model list).
    pub fn key(self) -> &'static str {
        match self {
            Group::Subject => "subject",
            Group::Sky => "sky",
            Group::Depth => "depth",
            Group::Tagging => "tagging",
            Group::Faces => "faces",
            Group::RawDenoise => "raw-denoise",
            Group::RgbDenoise => "rgb-denoise",
            Group::Remove => "remove",
        }
    }

    /// Mask, depth and restoration functions support user-picked single ONNX files.
    pub fn supports_custom(self) -> bool {
        matches!(self, Group::Subject | Group::Sky | Group::Depth | Group::RawDenoise | Group::RgbDenoise)
    }

    /// What the function does and where the app uses it.
    pub fn about(self) -> &'static str {
        match self {
            Group::Subject => {
                "Finds the main subject of a photo. Used by Select › Subject, Remove Background (Fast / Better Quality), the Object Selection tool's click mode, and the Library's Subject and Background masks."
            }
            Group::Sky => "Finds the sky. Used by Select › Sky and the Library's sky masks.",
            Group::Tagging => {
                "Recognises what a photo shows (speakers, audience, details…) so Smart Sort can put it in the right folder. Runs on the CPU; photos never leave this computer."
            }
            Group::RawDenoise => {
                "Removes noise from raw photos locally, using RawNIND UtNet2. Includes Bayer demosaic and linear Rec.2020 models. Requires a GPU."
            }
            Group::Remove => "Repairs brushed areas with LaMa on this computer. No AI server needed; photos stay on this device.",
            Group::RgbDenoise => "Removes noise from JPEG, TIFF and edited RGB images locally with NIND UNet.",
            Group::Faces => {
                "Finds faces and recognises the same person across photos. Off until you turn it on in Smart Sort. Face data stays in your library."
            }
            Group::Depth => {
                "Estimates how far away each part of a photo is. Used by the Library's depth masks (e.g. to darken or blur the background, or pick the foreground by distance)."
            }
        }
    }

    /// The official model of this function.
    pub fn official(self) -> &'static ModelSpec {
        let id = match self {
            Group::Subject => "rmbg-1.4",
            Group::Sky => "sky-mobileseg",
            Group::Depth => "depth-anything-v2-small",
            Group::Tagging => "clip-b32-laion",
            Group::Faces => "sface",
            Group::RawDenoise => denoise::DENOISE_ID,
            Group::RgbDenoise => "denoise-nind",
            Group::Remove => lama::MODEL_ID,
        };
        MODELS.iter().find(|m| m.id == id).expect("the official model is in MODELS")
    }

    /// The model the app uses for this function: the custom one when set, else the official one
    /// when it is installed.
    pub fn in_use(self, models_dir: &Path) -> Option<InUse> {
        active(self, models_dir).map(|a| a.in_use)
    }
}

/// Which model serves a function right now.
#[derive(Clone, Debug, PartialEq)]
pub enum InUse {
    Official(&'static ModelSpec),
    Custom(Custom),
}
impl InUse {
    /// The model's name for messages.
    pub fn label(&self) -> String {
        match self {
            Self::Official(spec) => spec.label.to_owned(),
            Self::Custom(custom) => format!("custom “{}”", custom.name),
        }
    }

    pub fn cpu_estimate(&self) -> estimate::Estimate {
        match self {
            Self::Official(spec) => estimate::estimate(spec.id, (spec.size * spec.size) as u64),
            Self::Custom(custom) => estimate::estimate(&format!("custom:{}", custom.file), (custom.size as u64).saturating_mul(custom.size as u64)),
        }
    }
}

/// How a model wants its input normalised.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Norm {
    /// IS-Net style: mean 0.5, std 1.
    IsNet,
    /// U²-Net / ImageNet style: ImageNet mean and std (also Depth Anything, MiDaS, SegFormer).
    ImageNet,
    /// DPT style: mean 0.5, std 0.5, the input in -1..1 (DPT, Depth Pro, PaddleSeg exports).
    Half,
}

impl Norm {
    pub const ALL: [Norm; 3] = [Norm::IsNet, Norm::ImageNet, Norm::Half];

    pub fn label(self) -> &'static str {
        match self {
            Norm::IsNet => "IS-Net style",
            Norm::ImageNet => "U²-Net / ImageNet style",
            Norm::Half => "DPT style (-1 to 1)",
        }
    }

    /// What a new custom model of `group` most likely expects: IS-Net style for subject models,
    /// ImageNet style for sky and depth models (as the official ones).
    pub fn default_for(group: Group) -> Norm {
        if group == Group::Subject { Norm::IsNet } else { Norm::ImageNet }
    }

    fn key(self) -> &'static str {
        match self {
            Norm::IsNet => "isnet",
            Norm::ImageNet => "imagenet",
            Norm::Half => "half",
        }
    }

    fn from_key(k: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|n| n.key() == k)
    }

    fn mean_std(self) -> ([f32; 3], [f32; 3]) {
        match self {
            Norm::IsNet => ([0.5; 3], [1.0; 3]),
            Norm::ImageNet => ([0.485, 0.456, 0.406], [0.229, 0.224, 0.225]),
            Norm::Half => ([0.5; 3], [0.5; 3]),
        }
    }
}

/// What a custom model's output values are, found when it is added ([`add_custom`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Activation {
    /// Probabilities: the graph ends in a sigmoid or softmax (values 0–1).
    Probability,
    /// Logits: the activation is left to the caller (BiRefNet, RMBG-2.0, class-logit scene
    /// parsers).
    Logits,
}

impl Activation {
    fn key(self) -> &'static str {
        match self {
            Activation::Probability => "probability",
            Activation::Logits => "logits",
        }
    }

    fn from_key(k: &str) -> Option<Self> {
        [Activation::Probability, Activation::Logits].into_iter().find(|a| a.key() == k)
    }

    /// What custom-model lines written before outputs were checked meant: subject maps were used
    /// as they came, sky maps as logits.
    fn legacy(group: Group) -> Self {
        if group == Group::Sky { Activation::Logits } else { Activation::Probability }
    }

    /// Logits when any output value is clearly outside 0–1.
    fn of_outputs(values: impl IntoIterator<Item = f32>) -> Self {
        if values.into_iter().any(|v| v.is_finite() && !(-0.01..=1.01).contains(&v)) { Activation::Logits } else { Activation::Probability }
    }
}

/// What a model finds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Task {
    /// The salient subject: one foreground map.
    Subject,
    /// The sky. `classes` = 1: one map; otherwise ADE20K-style class maps where `class` is the
    /// sky and `margin` how far its logit must lead every other class's.
    Sky {
        classes: usize,
        class: usize,
        margin: f32,
    },
    /// Monocular depth: one map of relative inverse depth (larger = nearer; Depth Anything V2,
    /// ImageNet-normalised input), or of metric depth (larger = farther) for a custom model set so.
    Depth,
    /// Paired image/text embedding models; kept separate from mask inference.
    ImageText,
    FaceDetect,
    FaceEmbed,
    /// RGB restoration and enlargement, through the dedicated enhance API.
    Upscale,
    /// Masked RGB reconstruction, through the dedicated LaMa API.
    Inpaint,
    /// A verified ONNX inside an official darktable-ai ZIP package.
    Denoise {
        companion_inners: &'static [&'static str],
        inner: &'static str,
        inner_bytes: u64,
        inner_sha256: &'static str,
    },
}

pub const MODELS: &[ModelSpec] = &[
    lama::MODEL,
    ModelSpec {
        id: "rmbg-1.4",
        label: "Fast · BRIA RMBG-1.4",
        file: "rmbg-1.4.onnx",
        companions: &[],
        bytes: 176153355,
        sha256: "8cafcf770b06757c4eaced21b1a88e57fd2b66de01b8045f35f01535ba742e0f",
        url: "https://huggingface.co/briaai/RMBG-1.4/resolve/2ceba5a5efaec153162aedea169f76caf9b46cf8/onnx/model.onnx",
        size: 1024,
        isnet: true,
        licence: "BRIA RMBG-1.4 licence · non-commercial (BRIA AI; commercial use requires a BRIA agreement)",
        task: Task::Subject,
        group: Some(Group::Subject),
        about: "Fast subject masks, with clean hair, fur and glass. About 3 s per photo on CPU; no GPU or AI server needed.",
    },
    ModelSpec {
        id: "inspyrenet",
        label: "Better Quality · InSPyReNet SwinB Plus-Ultra",
        file: "inspyrenet-swinb-plus-ultra.onnx",
        companions: &[],
        bytes: 395316574,
        sha256: "c108dd92b3ddfe3a2d9e9ac2b74730cc5acbb2ddc7ea863330c43f56ae832aa3",
        url: "https://huggingface.co/OS-Software/InSPyReNet-SwinB-Plus-Ultra-ONNX/resolve/fe628d2d1465f775c707368a80248a2d5b5ad71f/onnx/model.onnx",
        size: 1024,
        isnet: false,
        licence: "MIT (InSPyReNet, plemeri; community ONNX conversion by OS-Software)",
        task: Task::Subject,
        group: Some(Group::Subject),
        about: "Fine hair, fur and branch detail. About 15 s per photo on CPU, including model load; no GPU or AI server needed.",
    },
    // Sky: PaddleSeg's PP-MobileSeg-Base trained on ADE20K (sky = class 2), ONNX opset 13.
    ModelSpec {
        id: "sky-mobileseg",
        label: "PP-MobileSeg",
        file: "pp_mobileseg_base_ade20k_512.onnx",
        companions: &[],
        bytes: 23711565,
        sha256: "63c15451d3907472410de9417cabf2f121b62006d50b980fb2f774ceddaeec7a",
        url: "https://raw.githubusercontent.com/kisakutanaka/SkySegmentation/4f1715a9517e065d2867a724b4dc6aad914f0aca/models/pp_mobileseg_base_ade20k_512.onnx",
        size: 512,
        isnet: false,
        licence: "Apache-2.0 (PaddleSeg PP-MobileSeg; trained on ADE20K)",
        task: Task::Sky { classes: 150, class: 2, margin: 2.0 },
        group: Some(Group::Sky),
        about: "Finds the sky precisely, including around trees, buildings and the horizon. A scene-parsing model that knows 150 kinds of things, so tricky skies work well. Takes a second or two per photo.",
    },
    // Depth: Depth Anything V2 Small (Yang et al. 2024; the Small weights are Apache-2.0, the
    // larger ones are not), the TorchScript ONNX export of fabio-sim/Depth-Anything-ONNX
    // (Apache-2.0) release v2.0.0 — plain opset-17 operators that tract runs (that release's
    // other export uses ONNX local functions, which it doesn't).
    ModelSpec {
        id: "depth-anything-v2-small",
        label: "Depth Anything V2 Small",
        file: "depth_anything_v2_vits_dynamic.onnx",
        companions: &[],
        bytes: 99092268,
        sha256: "46c4e8eeda3a27f34701831b6a2ec7753d7b38779b215acb5633424703deed8f",
        url: "https://github.com/fabio-sim/Depth-Anything-ONNX/releases/download/v2.0.0/depth_anything_v2_vits_dynamic.onnx",
        size: 518,
        isnet: false,
        licence: "Apache-2.0 (Depth Anything V2 Small, Yang et al. 2024)",
        task: Task::Depth,
        group: Some(Group::Depth),
        about: "Clean edges between near and far, good on most photos. Takes a few seconds per photo.",
    },
    ModelSpec {
        id: "clip-b32-laion",
        label: "CLIP ViT-B-32 LAION",
        file: "clip-vit-b32-laion2b-visual.onnx",
        url: "https://huggingface.co/immich-app/ViT-B-32__laion2b-s34b-b79k/resolve/19a057a0fb927cf239541300e74237cd56c7ffe2/visual/model.onnx",
        companions: &[
            Companion {
                file: "clip-vit-b32-laion2b-textual.onnx",
                url: "https://huggingface.co/immich-app/ViT-B-32__laion2b-s34b-b79k/resolve/19a057a0fb927cf239541300e74237cd56c7ffe2/textual/model.onnx",
                bytes: 254193396,
                sha256: "48c25be37b352398f2533c9426dab6f9340535e14e46f884cc894236a5060722",
            },
            Companion {
                file: "clip-vocab.json",
                url: "https://huggingface.co/immich-app/ViT-B-32__laion2b-s34b-b79k/resolve/19a057a0fb927cf239541300e74237cd56c7ffe2/textual/vocab.json",
                bytes: 862328,
                sha256: "5047b556ce86ccaf6aa22b3ffccfc52d391ea4accdab9c2f2407da5b742d4363",
            },
            Companion {
                file: "clip-merges.txt",
                url: "https://huggingface.co/immich-app/ViT-B-32__laion2b-s34b-b79k/resolve/19a057a0fb927cf239541300e74237cd56c7ffe2/textual/merges.txt",
                bytes: 524619,
                sha256: "9fd691f7c8039210e0fced15865466c65820d09b63988b0174bfe25de299051a",
            },
        ],
        bytes: 351613724,
        sha256: "b9ce24b91a8c62ef8d40ea786051709c93140104cbf2b6b4a2cc270df3838f9c",
        size: 224,
        isnet: false,
        licence: "MIT (LAION CLIP weights; Immich ONNX export; OpenAI tokenizer)",
        task: Task::ImageText,
        group: Some(Group::Tagging),
        about: "Recognises scenes from your descriptions for Smart Sort. A fast local model for speakers, audiences, details and other categories you choose. Downloads include its text model and tokenizer.",
    },
    ModelSpec {
        id: "yunet",
        label: "YuNet",
        file: "face_detection_yunet_2023mar.onnx",
        companions: &[],
        bytes: 232589,
        sha256: "8f2383e4dd3cfbb4553ea8718107fc0423210dc964f9f4280604804ed2552fa4",
        url: "https://huggingface.co/opencv/face_detection_yunet/resolve/3cc26e7f1014a5ee5d74a42acee58bafc9d0a310/face_detection_yunet_2023mar.onnx",
        size: 640,
        isnet: false,
        licence: "MIT (YuNet, Shiqi Yu / OpenCV Zoo)",
        task: Task::FaceDetect,
        group: Some(Group::Faces),
        about: "Finds faces and their landmarks, ready to recognise the same person in other photos. Runs locally on the CPU.",
    },
    ModelSpec {
        id: "sface",
        label: "YuNet + SFace",
        file: "face_recognition_sface_2021dec.onnx",
        companions: &[Companion {
            file: "face_detection_yunet_2023mar.onnx",
            url: "https://huggingface.co/opencv/face_detection_yunet/resolve/3cc26e7f1014a5ee5d74a42acee58bafc9d0a310/face_detection_yunet_2023mar.onnx",
            bytes: 232589,
            sha256: "8f2383e4dd3cfbb4553ea8718107fc0423210dc964f9f4280604804ed2552fa4",
        }],
        bytes: 38696353,
        sha256: "0ba9fbfa01b5270c96627c4ef784da859931e02f04419c829e83484087c34e79",
        url: "https://huggingface.co/opencv/face_recognition_sface/resolve/3d7082438a6e4551e840c9b2bb60b71e8da4b524/face_recognition_sface_2021dec.onnx",
        size: 112,
        isnet: false,
        licence: "Apache-2.0 (SFace, OpenCV Zoo)",
        task: Task::FaceEmbed,
        group: Some(Group::Faces),
        about: "Finds faces and groups photos of the same person for Smart Sort. Includes both face models; runs on the CPU with face data kept in your library.",
    },
    ModelSpec {
        id: "realplksr-2x",
        label: "Faithful · RealPLKSR 2×",
        file: "realplksr-2x.onnx",
        companions: &[],
        bytes: 29627920,
        sha256: "d7abb65092f3808d3aa255ffdab42b1d672883915d90adbdd321204168b9f293",
        url: "https://huggingface.co/darktable-org/upscale-realplksr-onnx/resolve/0bb636eeeed5fffd256571c3cc440d9fdf62467e/onnx/model_x2.onnx",
        size: 512,
        isnet: false,
        licence: "MIT (RealPLKSR, Dongheon Lee et al.; darktable ONNX export)",
        task: Task::Upscale,
        group: None,
        about: "Enlarges clean photos while preserving their detail. Fixed tiles with overlap blending; runs locally on the CPU.",
    },
    ModelSpec {
        id: "realplksr-4x",
        label: "Faithful · RealPLKSR 4×",
        file: "realplksr-4x.onnx",
        companions: &[],
        bytes: 29711010,
        sha256: "8ce692652494e2e5c31e1f7c7ab61eb084e9a450cde727a374f5d6746cfe054b",
        url: "https://huggingface.co/darktable-org/upscale-realplksr-onnx/resolve/0bb636eeeed5fffd256571c3cc440d9fdf62467e/onnx/model_x4.onnx",
        size: 256,
        isnet: false,
        licence: "MIT (RealPLKSR, Dongheon Lee et al.; darktable ONNX export)",
        task: Task::Upscale,
        group: None,
        about: "Enlarges clean photos while preserving their detail. Fixed tiles with overlap blending; runs locally on the CPU.",
    },
    ModelSpec {
        id: "bsrgan-2x",
        label: "Rescue · BSRGAN 2×",
        file: "upscale-bsrgan.dtmodel",
        companions: &[],
        bytes: 124072128,
        sha256: "e90621e12fea12098ad6abd701bf67f50cea6c633bad98a19a4ccebba94404a6",
        url: "https://github.com/darktable-org/darktable-ai/releases/download/release-5.6.0/upscale-bsrgan.dtmodel",
        size: 512,
        isnet: false,
        licence: "Apache-2.0 (BSRGAN, Kai Zhang et al.; darktable ONNX export)",
        task: Task::Upscale,
        group: None,
        about: "Enlarges photos and cleans noise, JPEG artefacts and mild blur. The download includes both scales; runs locally on the CPU.",
    },
    ModelSpec {
        id: "bsrgan-4x",
        label: "Rescue · BSRGAN 4×",
        file: "upscale-bsrgan.dtmodel",
        companions: &[],
        bytes: 124072128,
        sha256: "e90621e12fea12098ad6abd701bf67f50cea6c633bad98a19a4ccebba94404a6",
        url: "https://github.com/darktable-org/darktable-ai/releases/download/release-5.6.0/upscale-bsrgan.dtmodel",
        size: 256,
        isnet: false,
        licence: "Apache-2.0 (BSRGAN, Kai Zhang et al.; darktable ONNX export)",
        task: Task::Upscale,
        group: None,
        about: "Enlarges photos and cleans noise, JPEG artefacts and mild blur. The download includes both scales; runs locally on the CPU.",
    },
    ModelSpec {
        id: denoise::DENOISE_ID,
        label: "RawNIND UtNet2",
        file: "rawdenoise-nind-linear.onnx",
        companions: &[Companion {
            file: "rawdenoise-nind-bayer.onnx",
            url: "https://github.com/darktable-org/darktable-ai/releases/download/release-5.6.0/rawdenoise-nind.dtmodel",
            bytes: 31056425,
            sha256: "da27509dab6a2915da67e988acd86cf71f9d5bbc8d1aa0ed32933578a887b901",
        }],
        bytes: 57700134,
        sha256: "d71b5f1e727c85a359e6f74dca9e2016c9d8fc3e2f7ac3e9b347d80ceca969af",
        url: "https://github.com/darktable-org/darktable-ai/releases/download/release-5.6.0/rawdenoise-nind.dtmodel",
        size: 512,
        isnet: false,
        licence: "GPL-3.0 (RawNIND UtNet2; darktable-ai ONNX export)",
        task: Task::Denoise {
            companion_inners: &["rawdenoise-nind/model_bayer.onnx"],
            inner: "rawdenoise-nind/model_linear.onnx",
            inner_bytes: 31053823,
            inner_sha256: "df957efadcc152c007d5d3b0917bdff9e41c0d4a0efe56584ef30b36393cd181",
        },
        group: Some(Group::RawDenoise),
        about: "Raw denoising and Bayer demosaic require a GPU. Linear Rec.2020 variant for other sensors.",
    },
    ModelSpec {
        id: "denoise-nind",
        label: "NIND UNet",
        file: "denoise-nind.onnx",
        companions: &[],
        bytes: 55008210,
        sha256: "825b3657cbb5193a67432a2f0b44ab86531cc7337f89e8e6c17d93db9665708a",
        url: "https://github.com/darktable-org/darktable-ai/releases/download/release-5.6.0/denoise-nind.dtmodel",
        size: 768,
        isnet: false,
        licence: "GPL-3.0 (NIND UNet; darktable-ai ONNX export)",
        task: Task::Denoise {
            companion_inners: &[],
            inner: "denoise-nind/model.onnx",
            inner_bytes: 59138823,
            inner_sha256: "5d615c5026a3c579455b9082f72b91e109adb54f4125b5b5cde45983bd6776b2",
        },
        group: Some(Group::RgbDenoise),
        about: "Local denoising for JPEG, TIFF and edited RGB images. Requires a GPU.",
    },
];

pub fn spec(id: &str) -> Option<&'static ModelSpec> {
    MODELS.iter().find(|m| m.id == id)
}

/// Runs the model; `stop()` is asked before every graph node and `true` ends the run with
/// [`Cancelled`].
type Runner = dyn Fn(Tensor, &dyn Fn() -> bool) -> TractResult<TVec<TValue>> + Send + Sync;

// A run stopped through its cancel flag ends with the shared job `Cancelled` (see `is_cancelled`).
// The `&AtomicBool` cancel arguments below take a job's `JobCtl::cancel_flag()`.
use lightcraft_sysmem::jobs::Cancelled;

/// Did `e` come from a cancelled run?
pub fn is_cancelled(e: &anyhow::Error) -> bool {
    e.chain().any(|c| c.is::<Cancelled>())
}

/// Run `plan`, asking `stop()` before each node (model stage): a cancel stops the work within one
/// stage instead of after the whole network. API: tract-core 0.23.8 `SimpleState::run_plan_with_eval`.
fn run_checked(plan: &Arc<TypedRunnableModel>, inputs: TVec<TValue>, stop: &dyn Fn() -> bool) -> TractResult<TVec<TValue>> {
    let mut state = plan.spawn()?;
    cpu_compute(|| {
        state.run_plan_with_eval(inputs, |ctx, op_state, node, input| {
            if stop() {
                return Err(anyhow::Error::new(Cancelled));
            }
            tract_onnx::tract_core::plan::eval(ctx, op_state, node, input)
        })
    })
}

/// `Err(Cancelled)` once `stop()` says so.
fn check(stop: &dyn Fn() -> bool) -> Result<()> {
    if stop() { Err(anyhow::Error::new(Cancelled)) } else { Ok(()) }
}

fn never() -> bool {
    false
}

/// Subject output contract. Custom IS-Net/U²-Net style models use min–max;
/// the two background-removal tiers already output probabilities.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Post {
    MinMax,
    Probability,
}

/// How to run a model: official or custom.
#[derive(Clone, Debug, PartialEq)]
struct Config {
    id: String,
    size: usize,
    norm: Norm,
    task: Task,
    post: Post,
    activation: Activation,
    /// Depth models: the output is metric depth (larger = farther), not inverse depth.
    metric: bool,
    /// Depth Anything V2: a flexible-input export sees the photo with its own aspect ratio.
    keeps_aspect: bool,
}

impl Config {
    fn of_spec(spec: &ModelSpec) -> Self {
        let tier = matches!(spec.id, "rmbg-1.4" | "inspyrenet");
        Self {
            id: spec.id.to_owned(),
            size: spec.size,
            norm: if spec.isnet { Norm::IsNet } else { Norm::ImageNet },
            task: spec.task,
            post: if tier { Post::Probability } else { Post::MinMax },
            activation: if matches!(spec.task, Task::Sky { .. }) { Activation::Logits } else { Activation::Probability },
            metric: false,
            keeps_aspect: spec.task == Task::Depth,
        }
    }
}

fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

/// `model` computing in f32: half-precision exports (`model_fp16.onnx`) then take and return f32
/// like every other model. Models without f16 tensors are left as they are.
fn in_f32(model: TypedModel) -> Result<TypedModel> {
    use tract_onnx::tract_core::transform::{NodeFilter, build_float_translator};
    let half = model.nodes.iter().any(|n| n.outputs.iter().any(|o| o.fact.datum_type == DatumType::F16));
    if !half {
        return Ok(model);
    }
    build_float_translator(DatumType::F16, DatumType::F32, NodeFilter::default()).transform_into(model)
}

/// A loaded model, ready to run (cheap to clone).
#[derive(Clone)]
pub struct Segmenter {
    cfg: Config,
    run: Arc<Runner>,
    /// The model takes any input size and keeps the photo's aspect ratio ([`depth_input`]): its
    /// plans by input size.
    flexible: Option<Arc<Plans>>,
}

/// Depth Anything V2's patch: its flexible export takes sides that are multiples of it.
const DEPTH_PATCH: usize = 14;

/// The input (height, width) a flexible depth model gets for a `w × h` photo, as the model's
/// reference preprocessing sizes it: the short side at `s`, the long side keeping the aspect ratio
/// (a multiple of the patch, at most twice the short side).
fn depth_input(s: usize, w: usize, h: usize) -> (usize, usize) {
    let aspect = (w.max(h) as f64 / w.min(h).max(1) as f64).min(2.0);
    let long = (((s as f64 * aspect) / DEPTH_PATCH as f64).round() as usize * DEPTH_PATCH).max(s);
    if w >= h { (s, long) } else { (long, s) }
}

/// `model` planned for one `th × tw` input of type `input`.
fn plan_at(model: InferenceModel, input: DatumType, th: usize, tw: usize) -> Result<Arc<TypedRunnableModel>> {
    in_f32(model.with_input_fact(0, InferenceFact::dt_shape(input, tvec!(1, 3, th, tw)))?.into_typed()?)?.into_optimized()?.into_runnable()
}

/// A flexible-input model's plans, built for an input size when it is first needed (the most
/// recently used first, a few at most).
struct Plans {
    model: InferenceModel,
    input: DatumType,
    built: std::sync::Mutex<Vec<((usize, usize), Arc<TypedRunnableModel>)>>,
}

impl Plans {
    /// The plan for a `(height, width)` input. Planning takes seconds and runs outside the lock,
    /// so a run at a size already planned is not held up.
    fn at(&self, size: (usize, usize)) -> Result<Arc<TypedRunnableModel>> {
        let built = || self.built.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        {
            let mut plans = built();
            if let Some(i) = plans.iter().position(|(at, _)| *at == size) {
                let hit = plans.remove(i);
                plans.insert(0, hit.clone());
                return Ok(hit.1);
            }
        }
        let plan = plan_at(self.model.clone(), self.input, size.0, size.1)?;
        let mut plans = built();
        plans.retain(|(at, _)| *at != size);
        plans.insert(0, (size, plan.clone()));
        plans.truncate(3);
        Ok(plan)
    }
}

impl std::fmt::Debug for Segmenter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Segmenter").field("model", &self.cfg.id).finish()
    }
}

/// The map's height and width when it is a single map: `[n, 1, h, w]`, `[n, h, w]` or `[h, w]`.
fn single_map(shape: &[usize]) -> Option<(usize, usize)> {
    match shape {
        [_, 1, h, w] | [_, h, w] | [h, w] => Some((*h, *w)),
        _ => None,
    }
}

impl Segmenter {
    pub fn load(spec: &ModelSpec, path: &Path) -> Result<Self> {
        Self::load_cfg(Config::of_spec(spec), path)
    }

    fn load_cfg(cfg: Config, path: &Path) -> Result<Self> {
        if matches!(cfg.task, Task::ImageText | Task::FaceDetect | Task::FaceEmbed | Task::Upscale | Task::Inpaint | Task::Denoise { .. }) {
            bail!("use the dedicated embedding or face inference API");
        }
        estimate::estimate(&cfg.id, (cfg.size as u64).saturating_mul(cfg.size as u64)).check()?;
        init_executor();
        let s = cfg.size;
        // the dynamic-shape depth export declares symbolic intermediate shapes that conflict
        // with the fixed input: let tract infer them
        let depth = cfg.task == Task::Depth;
        let model = tract_onnx::onnx()
            .with_ignore_value_info(depth)
            .with_ignore_output_shapes(depth || cfg.task == Task::Subject)
            .model_for_path(path)
            .with_context(|| format!("Could not read the segmentation model {}", path.display()))?;
        let fact = model.input_fact(0)?;
        let input = fact.datum_type.concretize().unwrap_or(DatumType::F32);
        let flexible = cfg.keeps_aspect && [2, 3].iter().all(|i| fact.shape.dim(*i).and_then(|d| d.concretize()).and_then(|d| d.to_i64().ok()).is_none());
        if !flexible {
            let plan = plan_at(model, input, s, s)?;
            return Ok(Self { cfg, run: Arc::new(move |t: Tensor, stop: &dyn Fn() -> bool| run_checked(&plan, tvec!(t.into()), stop)), flexible: None });
        }
        let plans = Arc::new(Plans { model, input, built: Default::default() });
        let at = plans.clone();
        let run = move |t: Tensor, stop: &dyn Fn() -> bool| {
            let size = (t.shape().get(2).copied().unwrap_or(s), t.shape().get(3).copied().unwrap_or(s));
            run_checked(&at.at(size)?, tvec!(t.into()), stop)
        };
        Ok(Self { cfg, run: Arc::new(run), flexible: Some(plans) })
    }

    /// The input (height, width) this model gets for a `w × h` photo.
    fn input_size(&self, w: usize, h: usize) -> (usize, usize) {
        if self.flexible.is_some() { depth_input(self.cfg.size, w, h) } else { (self.cfg.size, self.cfg.size) }
    }

    /// The model's id (an official id, or `custom:<file>`).
    pub fn id(&self) -> &str {
        &self.cfg.id
    }

    /// The model input for an RGBA8 image: premultiplied resize to the model's square size,
    /// normalised.
    fn input(&self, rgba: &[u8], w: usize, h: usize, peak_scaled: bool) -> Result<tract_ndarray::Array4<f32>> {
        self.input_at(rgba, w, h, peak_scaled, (self.cfg.size, self.cfg.size))
    }

    /// [`Segmenter::input`] at `th × tw`.
    fn input_at(&self, rgba: &[u8], w: usize, h: usize, peak_scaled: bool, (th, tw): (usize, usize)) -> Result<tract_ndarray::Array4<f32>> {
        if rgba.len() != w * h * 4 || w == 0 || h == 0 {
            bail!("image buffer size mismatch");
        }
        let small = resize_premultiplied(rgba, w, h, tw, th);
        let peak = if peak_scaled { small.iter().flat_map(|p| [p[0], p[1], p[2]]).fold(1e-6f32, f32::max) } else { 1.0 };
        let (mean, std) = self.cfg.norm.mean_std();
        Ok(tract_ndarray::Array4::from_shape_fn((1, 3, th, tw), |(_, c, y, x)| (small[y * tw + x][c] / peak - mean[c]) / std[c]))
    }

    /// Foreground probability (0–1) for an RGBA8 image, at the image's size. `None` when the model
    /// sees no clear subject.
    pub fn predict(&self, rgba: &[u8], w: usize, h: usize) -> Result<Option<Vec<f32>>> {
        self.predict_checked(rgba, w, h, &never)
    }

    /// [`Segmenter::predict`] that stops within one model stage once `cancel` is set, with an
    /// error for which [`is_cancelled`] is true.
    pub fn predict_with_cancel(&self, rgba: &[u8], w: usize, h: usize, cancel: &AtomicBool) -> Result<Option<Vec<f32>>> {
        self.predict_checked(rgba, w, h, &|| cancel.load(Ordering::Relaxed))
    }

    fn predict_checked(&self, rgba: &[u8], w: usize, h: usize, stop: &dyn Fn() -> bool) -> Result<Option<Vec<f32>>> {
        check(stop)?;
        let input = self.input(rgba, w, h, self.cfg.post == Post::MinMax)?;
        let out = estimate::run(&self.cfg.id, (self.cfg.size * self.cfg.size) as u64, || (self.run)(input.into(), stop))?;
        check(stop)?;
        let map = out[0].to_plain_array_view::<f32>()?;
        let Some((oh, ow)) = single_map(map.shape()) else { bail!("The model's output {:?} isn't a single map", map.shape()) };
        let mut vals: Vec<f32> = map.iter().copied().collect();
        if vals.len() < oh * ow || oh * ow == 0 {
            bail!("unexpected model output");
        }
        vals.truncate(oh * ow);
        if self.cfg.activation == Activation::Logits {
            vals.iter_mut().for_each(|v| *v = sigmoid(*v));
        }
        let (lo, hi) = vals.iter().fold((f32::MAX, f32::MIN), |(a, b), v| (a.min(*v), b.max(*v)));
        if hi < 0.5 {
            return Ok(None);
        }
        let span = (hi - lo).max(1e-6);
        anyhow::ensure!(vals.iter().all(|v| v.is_finite()), "non-finite subject probability");
        let norm: Vec<f32> = vals
            .iter()
            .map(|v| match self.cfg.post {
                Post::MinMax => (v - lo) / span,
                Post::Probability => v.clamp(0.0, 1.0),
            })
            .collect();
        Ok(Some(upscale(&norm, ow, oh, w, h)))
    }

    /// Sky probability (0–1) for an RGBA8 image, at the image's size (a sky model). Raw
    /// probabilities, not stretched: a photo without sky stays near zero.
    pub fn predict_sky(&self, rgba: &[u8], w: usize, h: usize) -> Result<Vec<f32>> {
        self.predict_sky_checked(rgba, w, h, &never)
    }

    /// [`Segmenter::predict_sky`] that stops within one model stage once `cancel` is set (see
    /// [`Segmenter::predict_with_cancel`]).
    pub fn predict_sky_with_cancel(&self, rgba: &[u8], w: usize, h: usize, cancel: &AtomicBool) -> Result<Vec<f32>> {
        self.predict_sky_checked(rgba, w, h, &|| cancel.load(Ordering::Relaxed))
    }

    fn predict_sky_checked(&self, rgba: &[u8], w: usize, h: usize, stop: &dyn Fn() -> bool) -> Result<Vec<f32>> {
        let Task::Sky { classes, class, margin } = self.cfg.task else { bail!("not a sky model") };
        anyhow::ensure!(classes > 0 && class < classes, "sky class is outside the model output");
        check(stop)?;
        let input = self.input(rgba, w, h, false)?;
        let out = estimate::run(&self.cfg.id, (self.cfg.size * self.cfg.size) as u64, || (self.run)(input.into(), stop))?;
        check(stop)?;
        let map = out[0].to_plain_array_view::<f32>()?;
        let shape = map.shape().to_vec();
        let (oh, ow) = match shape.as_slice() {
            [_, c, oh, ow] if *c == classes => (*oh, *ow),
            other => bail!("unexpected sky model output {other:?}"),
        };
        let vals: Vec<f32> = map.iter().copied().collect();
        let plane = oh * ow;
        let logits = self.cfg.activation == Activation::Logits;
        let prob: Vec<f32> = if classes == 1 {
            vals[..plane].iter().map(|v| if logits { sigmoid(*v) } else { v.clamp(0.0, 1.0) }).collect()
        } else {
            // class probabilities lead each other by their log ratio, as logits do by their difference
            let logit = |v: f32| if logits { v } else { v.max(1e-6).ln() };
            (0..plane)
                .map(|i| {
                    let sky = logit(vals[class * plane + i]);
                    let other = (0..classes).filter(|c| *c != class).map(|c| logit(vals[c * plane + i])).fold(f32::MIN, f32::max);
                    sigmoid(sky - other - margin)
                })
                .collect()
        };
        Ok(upscale(&prob, ow, oh, w, h))
    }

    /// [`Segmenter::predict_depth`] that stops within one model stage once `cancel` is set (see
    /// [`Segmenter::predict_with_cancel`]).
    pub fn predict_depth_with_cancel(&self, rgba: &[u8], w: usize, h: usize, cancel: &AtomicBool) -> Result<Vec<f32>> {
        self.predict_depth_checked(rgba, w, h, &|| cancel.load(Ordering::Relaxed))
    }

    /// Relative nearness (0 = farthest, 1 = nearest in the photo) for an RGBA8 image, at the
    /// image's size (a depth model). A flexible-input model sees the photo in its own aspect ratio,
    /// a fixed one squeezed to its square input; its relative inverse depth (a metric model's
    /// depth inverted first; no depth reads as farthest) is min–max stretched (it has no absolute
    /// scale).
    pub fn predict_depth(&self, rgba: &[u8], w: usize, h: usize) -> Result<Vec<f32>> {
        self.predict_depth_checked(rgba, w, h, &never)
    }

    fn predict_depth_checked(&self, rgba: &[u8], w: usize, h: usize, stop: &dyn Fn() -> bool) -> Result<Vec<f32>> {
        if self.cfg.task != Task::Depth {
            bail!("not a depth model");
        }
        check(stop)?;
        let size = self.input_size(w, h);
        let units = (size.0 * size.1) as u64;
        // planned before the run: the CPU estimate calibrates on inference alone
        if let Some(plans) = &self.flexible {
            estimate::estimate(&self.cfg.id, units).check()?;
            plans.at(size)?;
            check(stop)?;
        }
        let input = self.input_at(rgba, w, h, false, size)?;
        let out = estimate::run(&self.cfg.id, units, || (self.run)(input.into(), stop))?;
        check(stop)?;
        let map = out[0].to_plain_array_view::<f32>()?;
        let Some((oh, ow)) = single_map(map.shape()) else { bail!("unexpected depth model output {:?}", map.shape()) };
        let mut vals: Vec<f32> = map.iter().copied().collect();
        if vals.len() < oh * ow || oh * ow == 0 {
            bail!("unexpected depth model output");
        }
        vals.truncate(oh * ow);
        if self.cfg.metric {
            vals.iter_mut().for_each(|v| *v = if v.is_finite() && *v > 0.0 { 1.0 / v.max(1e-6) } else { 0.0 });
        }
        let (lo, hi) = vals.iter().filter(|v| v.is_finite()).fold((f32::MAX, f32::MIN), |(a, b), v| (a.min(*v), b.max(*v)));
        if hi <= lo {
            return Ok(vec![0.5; w * h]);
        }
        let norm: Vec<f32> = vals.iter().map(|v| if v.is_finite() { (v - lo) / (hi - lo) } else { 0.0 }).collect();
        Ok(upscale(&norm, ow, oh, w, h))
    }
}

/// Area-filtered, premultiplied (transparent reads as black) resize to `tw × th`, values 0–1.
fn resize_premultiplied(rgba: &[u8], w: usize, h: usize, tw: usize, th: usize) -> Vec<[f32; 3]> {
    let mut out = vec![[0.0f32; 3]; tw * th];
    for ty in 0..th {
        let (y0, y1) = (ty * h / th, (((ty + 1) * h).div_ceil(th)).min(h).max(ty * h / th + 1));
        for tx in 0..tw {
            let (x0, x1) = (tx * w / tw, (((tx + 1) * w).div_ceil(tw)).min(w).max(tx * w / tw + 1));
            let mut acc = [0.0f32; 3];
            let mut n = 0.0f32;
            for y in y0..y1 {
                for x in x0..x1 {
                    let i = (y * w + x) * 4;
                    let a = rgba[i + 3] as f32 / 255.0;
                    for c in 0..3 {
                        acc[c] += rgba[i + c] as f32 / 255.0 * a;
                    }
                    n += 1.0;
                }
            }
            out[ty * tw + tx] = acc.map(|v| v / n.max(1.0));
        }
    }
    out
}

/// Bilinear upscale of a `sw × sh` map to `w × h`.
pub fn upscale(src: &[f32], sw: usize, sh: usize, w: usize, h: usize) -> Vec<f32> {
    let mut out = vec![0.0f32; w * h];
    for y in 0..h {
        let fy = ((y as f32 + 0.5) * sh as f32 / h as f32 - 0.5).clamp(0.0, (sh - 1) as f32);
        let (y0, ty) = (fy.floor() as usize, fy.fract());
        let y1 = (y0 + 1).min(sh - 1);
        for x in 0..w {
            let fx = ((x as f32 + 0.5) * sw as f32 / w as f32 - 0.5).clamp(0.0, (sw - 1) as f32);
            let (x0, tx) = (fx.floor() as usize, fx.fract());
            let x1 = (x0 + 1).min(sw - 1);
            let a = src[y0 * sw + x0] * (1.0 - tx) + src[y0 * sw + x1] * tx;
            let b = src[y1 * sw + x0] * (1.0 - tx) + src[y1 * sw + x1] * tx;
            out[y * w + x] = a * (1.0 - ty) + b * ty;
        }
    }
    out
}

/// The object under a click: the 8-connected region of the probability map (≥ 0.5) containing
/// `(cx, cy)`; `None` when the click is on background.
pub fn object_at(prob: &[f32], w: usize, h: usize, cx: usize, cy: usize) -> Option<Vec<f32>> {
    if cx >= w || cy >= h || prob[cy * w + cx] < 0.5 {
        return None;
    }
    let mut keep = vec![false; w * h];
    let mut stack = vec![(cx, cy)];
    keep[cy * w + cx] = true;
    while let Some((x, y)) = stack.pop() {
        for dy in -1i64..=1 {
            for dx in -1i64..=1 {
                let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 {
                    continue;
                }
                let j = ny as usize * w + nx as usize;
                if !keep[j] && prob[j] >= 0.5 {
                    keep[j] = true;
                    stack.push((nx as usize, ny as usize));
                }
            }
        }
    }
    // Keep the soft edge of the region: soft values touching it stay, the rest go.
    Some((0..w * h).map(|i| if keep[i] { prob[i] } else { 0.0 }).collect())
}

/// Where segmentation models live: `<models>/segmentation/<file>`.
pub fn model_path(models_dir: &Path, spec: &ModelSpec) -> std::path::PathBuf {
    companion_path(models_dir, spec.file)
}

pub fn companion_path(models_dir: &Path, file: &str) -> std::path::PathBuf {
    models_dir.join("segmentation").join(file)
}

impl ModelSpec {
    /// Main weights first, then all companion files, in download order.
    pub fn files(&self) -> impl Iterator<Item = Companion> + '_ {
        let (bytes, sha256) = match self.task {
            Task::Denoise { inner_bytes, inner_sha256, .. } => (inner_bytes, inner_sha256),
            _ => (self.bytes, self.sha256),
        };
        std::iter::once(Companion { file: self.file, url: self.url, bytes, sha256 }).chain(self.companions.iter().copied())
    }

    pub fn download_bytes(&self) -> u64 {
        if matches!(self.task, Task::Denoise { .. }) { self.bytes } else { self.files().map(|f| f.bytes).sum() }
    }
}

// custom models

/// A model the user picked for a function (an `.onnx` file copied into
/// `<models>/segmentation/custom/`), used instead of the official one.
#[derive(Clone, Debug, PartialEq)]
pub struct Custom {
    pub group: Group,
    /// The copy's file name inside the custom folder.
    pub file: String,
    /// The picked file's name, for display.
    pub name: String,
    /// The square input size the model runs at.
    pub size: usize,
    /// How the input is normalised.
    pub norm: Norm,
    /// Sky output channels; restoration models persist three RGB channels here.
    pub classes: usize,
    /// Sky class index; depth output: 0 = inverse depth, 1 = metric depth; restoration
    /// contract: 0 = display sRGB, 1 = linear Rec.2020.
    pub class: usize,
    /// Subject and sky maps: what the output values are.
    pub activation: Activation,
}

impl Custom {
    /// Where the copy lives.
    pub fn path(&self, models_dir: &Path) -> PathBuf {
        custom_dir(models_dir).join(&self.file)
    }

    fn config(&self) -> Config {
        let task = match self.group {
            Group::Subject => Task::Subject,
            Group::Sky => Task::Sky { classes: self.classes, class: self.class, margin: if self.classes == 1 { 0.0 } else { 2.0 } },
            Group::Depth => Task::Depth,
            Group::Tagging => Task::ImageText,
            Group::Faces => Task::FaceEmbed,
            Group::RawDenoise | Group::RgbDenoise | Group::Remove => self.group.official().task,
        };
        Config {
            id: format!("custom:{}", self.file),
            size: self.size,
            norm: self.norm,
            task,
            post: Post::MinMax,
            activation: self.activation,
            metric: self.group == Group::Depth && self.class == 1,
            keeps_aspect: false,
        }
    }
}

/// What the user chose besides the file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CustomOptions {
    /// RGB restoration: use linear Rec.2020 instead of display sRGB. Raw always uses linear.
    pub denoise_linear: bool,
    /// The input normalisation (subject, sky and depth models).
    pub norm: Norm,
    /// Sky models with class logits: the sky's class index (ADE20K: 2).
    pub sky_class: usize,
    /// Depth models: the output is metric depth (larger = farther) instead of inverse depth.
    pub depth_metric: bool,
}

impl Default for CustomOptions {
    fn default() -> Self {
        Self { norm: Norm::IsNet, sky_class: 2, denoise_linear: false, depth_metric: false }
    }
}

impl CustomOptions {
    /// The options a new custom model of `group` starts from.
    pub fn for_group(group: Group) -> Self {
        Self { norm: Norm::default_for(group), ..Self::default() }
    }
}

/// `<models>/segmentation/custom/`.
pub fn custom_dir(models_dir: &Path) -> PathBuf {
    models_dir.join("segmentation").join("custom")
}

fn custom_list(models_dir: &Path) -> PathBuf {
    custom_dir(models_dir).join("custom.txt")
}

/// The custom model set for `group`, when its file is still there.
pub fn custom(models_dir: &Path, group: Group) -> Option<Custom> {
    if !group.supports_custom() {
        return None;
    }
    read_customs(models_dir).into_iter().find(|c| c.group == group).filter(|c| c.path(models_dir).is_file())
}

/// One line per function: `key TAB file TAB name TAB size TAB norm TAB classes TAB class TAB
/// activation` (lines without the activation are read as [`Activation::legacy`]).
fn read_customs(models_dir: &Path) -> Vec<Custom> {
    let Ok(text) = std::fs::read_to_string(custom_list(models_dir)) else { return Vec::new() };
    text.lines()
        .filter_map(|line| {
            let f: Vec<&str> = line.split('\t').collect();
            let ([key, file, name, size, norm, classes, class], activation) = (f.get(..7)?, f.get(7)) else { return None };
            if f.len() > 8 {
                return None;
            }
            let group = Group::ALL.into_iter().find(|g| g.supports_custom() && g.key() == *key)?;
            let custom = Custom {
                group,
                file: (*file).to_owned(),
                name: (*name).to_owned(),
                size: size.parse().ok().filter(|s| *s > 0)?,
                norm: Norm::from_key(norm)?,
                classes: classes.parse().ok().filter(|c| *c > 0)?,
                class: class.parse().ok()?,
                activation: match activation {
                    Some(a) => Activation::from_key(a)?,
                    None => Activation::legacy(group),
                },
            };
            (custom.group != Group::Sky || custom.class < custom.classes).then_some(custom)
        })
        .collect()
}

fn write_customs(models_dir: &Path, customs: &[Custom]) -> Result<()> {
    let text: String = customs
        .iter()
        .map(|c| format!("{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n", c.group.key(), c.file, c.name, c.size, c.norm.key(), c.classes, c.class, c.activation.key()))
        .collect();
    let path = custom_list(models_dir);
    if text.is_empty() {
        let _ = std::fs::remove_file(&path);
        return Ok(());
    }
    std::fs::create_dir_all(custom_dir(models_dir))?;
    li_fs::write_atomic(&path, text.as_bytes()).with_context(|| format!("Could not save {}", path.display()))
}

/// A file name that is safe inside the custom folder.
fn safe_name(name: &str) -> String {
    let s: String = name.chars().map(|c| if c.is_alphanumeric() || matches!(c, '.' | '-' | '_' | ' ') { c } else { '_' }).collect();
    s.trim_matches('.').to_owned()
}

/// Why a user-picked custom model cannot be used.
#[derive(Debug, Clone, PartialEq)]
pub enum CustomModelError {
    /// This function requires a bundle and has no custom-model flow.
    UnsupportedFunction,
    /// The path isn't a regular file.
    NotAFile { path: PathBuf },
    /// The file name doesn't end in `.onnx`.
    NotOnnx,
    /// The model has no input.
    NoInput,
    /// The model's input is not a 4D image tensor.
    NotAnImageInput,
    /// The model's input has a channel count other than 3.
    WrongChannels { channels: i64 },
    /// The model's input is not square.
    NonSquareInput { height: i64, width: i64 },
    /// The sky model's first output shape isn't a class map.
    NotSkyModelOutput { shape: Vec<usize> },
    /// The requested sky class index is out of range.
    SkyClassOutOfRange { class: usize, classes: usize },
    /// The subject model doesn't output a single foreground map.
    NotSubjectModel,
    /// The depth model doesn't output a single depth map.
    NotDepthModel,
    /// The GPU denoiser can't run this restoration model; why.
    UnsupportedRestoration { reason: String },
    /// A tract load/optimise failure.
    Other(String),
}

impl fmt::Display for CustomModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CustomModelError::UnsupportedFunction => f.write_str("Custom models are not supported for this function"),
            CustomModelError::NotAFile { path } => write!(f, "{} isn't a file", path.display()),
            CustomModelError::NotOnnx => f.write_str("Choose an ONNX model (a file ending in .onnx)"),
            CustomModelError::NoInput => f.write_str("The model has no input"),
            CustomModelError::NotAnImageInput => f.write_str("The model's input must be an image (batch × 3 × height × width)"),
            CustomModelError::WrongChannels { channels } => write!(f, "The model's input has {channels} channels; an RGB image (3) is needed"),
            CustomModelError::NonSquareInput { height, width } => {
                write!(f, "The model's input is {height}×{width}; a square input (or a flexible size) is needed")
            }
            CustomModelError::NotSkyModelOutput { shape } => write!(f, "Not a sky model: its output {shape:?} should be 1 map or one map per class"),
            CustomModelError::SkyClassOutOfRange { class, classes } => {
                write!(f, "The sky class {class} is out of range: this model has {classes} classes (0 to {})", classes - 1)
            }
            CustomModelError::NotSubjectModel => f.write_str("Not a subject model: it must output one foreground map"),
            CustomModelError::NotDepthModel => f.write_str("Not a depth model: it must output one depth map"),
            CustomModelError::UnsupportedRestoration { reason } => write!(f, "{RESTORATION_UNSUPPORTED} ({reason})"),
            CustomModelError::Other(msg) => f.write_str(msg),
        }
    }
}

impl std::error::Error for CustomModelError {}

/// The model's square input size when the file fixes it (`None` when it is dynamic).
pub fn input_size(path: &Path) -> Result<Option<usize>, CustomModelError> {
    let model = tract_onnx::onnx().model_for_path(path).map_err(|_| CustomModelError::Other(format!("Could not read {} as an ONNX model", path.display())))?;
    let fact = model.input_fact(0).map_err(|_| CustomModelError::NoInput)?;
    let dim = |i: usize| fact.shape.dim(i).and_then(|d| d.concretize()).and_then(|d| d.to_i64().ok());
    if fact.shape.rank().concretize().is_some_and(|r| r != 4) {
        return Err(CustomModelError::NotAnImageInput);
    }
    if let Some(c) = dim(1)
        && c != 3
    {
        return Err(CustomModelError::WrongChannels { channels: c });
    }
    match (dim(2), dim(3)) {
        (Some(h), Some(w)) if h == w && h > 0 => Ok(Some(h as usize)),
        (Some(h), Some(w)) => Err(CustomModelError::NonSquareInput { height: h, width: w }),
        _ => Ok(None),
    }
}

/// A small synthetic photo to test a model on: a sky-blue gradient over a green ground, with a
/// red block in the middle.
fn synthetic(w: usize, h: usize) -> Vec<u8> {
    let mut img = vec![255u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) * 4;
            let c = if x > w / 3 && x < 2 * w / 3 && y > h / 2 && y < h * 9 / 10 {
                [200, 40, 40]
            } else if y < h / 2 {
                [90, 150, 230]
            } else {
                [70, 120, 60]
            };
            img[i..i + 3].copy_from_slice(&c);
        }
    }
    img
}

/// Why a restoration model can't be used, before the specific reason.
pub const RESTORATION_UNSUPPORTED: &str = "Local Image's GPU denoiser can't run this model. It runs convolutions (not grouped or dilated), ReLU, LeakyReLU, sigmoid, max pooling, two-input channel concat and DepthToSpace, with float32 weights";

fn tract_error(e: anyhow::Error) -> CustomModelError {
    CustomModelError::Other(format!("{e:#}"))
}

/// The first output of a mask model run on the synthetic photo and on its inverse: its shape, and
/// what its values are.
fn probe_outputs(cfg: Config, path: &Path, peak_scaled: bool) -> Result<(Vec<usize>, Activation), CustomModelError> {
    let seg = Segmenter::load_cfg(cfg, path).map_err(tract_error)?;
    let s = seg.cfg.size;
    let photo = synthetic(s, s);
    let inverse: Vec<u8> = photo.as_chunks::<4>().0.iter().flat_map(|p| [255 - p[0], 255 - p[1], 255 - p[2], p[3]]).collect();
    let (mut shape, mut values) = (Vec::new(), Vec::new());
    for img in [photo, inverse] {
        let input = seg.input(&img, s, s, peak_scaled).map_err(tract_error)?;
        let out = (seg.run)(input.into(), &never).map_err(tract_error)?;
        let map = out.first().ok_or_else(|| CustomModelError::Other("The model has no output".into()))?.to_plain_array_view::<f32>().map_err(tract_error)?;
        shape = map.shape().to_vec();
        values.extend(map.iter().copied());
    }
    Ok((shape, Activation::of_outputs(values)))
}

/// Checks that `path` can serve `custom.group` as `opts` configure it: a mask model runs on the
/// synthetic photo (its sky classes and output activation are set in `custom`), a restoration
/// model on a flat tile. Errors say what doesn't fit.
fn validate(custom: &mut Custom, path: &Path, opts: &CustomOptions) -> Result<(), CustomModelError> {
    if matches!(custom.group, Group::RawDenoise | Group::RgbDenoise) {
        let kind = if custom.class == 1 { denoise::Variant::Linear } else { denoise::Variant::Rgb };
        // the GPU graph is what runs the model; tract checks its numbers on the CPU
        restoration_graph::Graph::load(path, restoration_graph::Shape { c: kind.channels(), h: custom.size, w: custom.size })
            .map_err(|e| CustomModelError::UnsupportedRestoration { reason: format!("{e:#}") })?;
        let model = denoise::Denoiser::load_at(path, kind, custom.size).map_err(tract_error)?;
        let out = model.run_tile(&vec![0.1; 3 * custom.size * custom.size]).map_err(tract_error)?;
        if out.len() != 3 * custom.size * custom.size || !out.iter().all(|v| v.is_finite()) {
            return Err(CustomModelError::Other("Restoration output must be three finite channels at the input size".into()));
        }
        return Ok(());
    }
    if matches!(custom.group, Group::Tagging | Group::Faces | Group::Remove) {
        return Err(CustomModelError::UnsupportedFunction);
    }
    let cfg =
        Config { task: if custom.group == Group::Sky { Task::Sky { classes: 1, class: 0, margin: 0.0 } } else { custom.config().task }, ..custom.config() };
    let (shape, activation) = probe_outputs(cfg, path, custom.group == Group::Subject)?;
    match custom.group {
        Group::Sky => {
            let classes = match shape.as_slice() {
                [_, ch, _, _] if *ch >= 1 => *ch,
                other => return Err(CustomModelError::NotSkyModelOutput { shape: other.to_vec() }),
            };
            if classes > 1 && opts.sky_class >= classes {
                return Err(CustomModelError::SkyClassOutOfRange { class: opts.sky_class, classes });
            }
            custom.classes = classes;
            custom.class = if classes > 1 { opts.sky_class } else { 0 };
        }
        Group::Subject if single_map(&shape).is_none() => return Err(CustomModelError::NotSubjectModel),
        Group::Depth if single_map(&shape).is_none() => return Err(CustomModelError::NotDepthModel),
        _ => {}
    }
    custom.activation = activation;
    Ok(())
}

/// Adds the `.onnx` file at `src` as the custom model of `group`: reads its input size (the
/// official model's size when it is flexible), tests it on a synthetic image, copies it into the
/// custom folder and remembers it. A custom model already set is replaced; `dispose` puts its old
/// file away (the Trash). Nothing changes when the model doesn't fit.
pub fn add_custom(models_dir: &Path, group: Group, src: &Path, opts: CustomOptions, dispose: &dyn Fn(&Path) -> std::io::Result<()>) -> Result<Custom> {
    add_custom_with(models_dir, group, src, opts, dispose, &write_customs)
}

fn identical_files(a: &Path, b: &Path) -> std::io::Result<bool> {
    use std::io::Read;
    if std::fs::metadata(a)?.len() != std::fs::metadata(b)?.len() {
        return Ok(false);
    }
    let (mut a, mut b) = (std::io::BufReader::new(std::fs::File::open(a)?), std::io::BufReader::new(std::fs::File::open(b)?));
    let (mut aa, mut bb) = ([0u8; 8192], [0u8; 8192]);
    loop {
        let n = a.read(&mut aa)?;
        b.read_exact(&mut bb[..n])?;
        if aa[..n] != bb[..n] {
            return Ok(false);
        }
        if n == 0 {
            return Ok(true);
        }
    }
}

fn add_custom_with(
    models_dir: &Path,
    group: Group,
    src: &Path,
    opts: CustomOptions,
    dispose: &dyn Fn(&Path) -> std::io::Result<()>,
    publish: &dyn Fn(&Path, &[Custom]) -> Result<()>,
) -> Result<Custom> {
    if !group.supports_custom() {
        return Err(CustomModelError::UnsupportedFunction.into());
    }
    if !src.is_file() {
        return Err(CustomModelError::NotAFile { path: src.to_path_buf() }.into());
    }
    let name = src.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if !name.to_lowercase().ends_with(".onnx") {
        return Err(CustomModelError::NotOnnx.into());
    }
    let size = input_size(src)?.unwrap_or(group.official().size);
    let mut c = Custom {
        group,
        file: format!("{}-{}", group.key(), safe_name(&name)),
        name,
        size,
        norm: opts.norm,
        classes: 1,
        class: 0,
        activation: Activation::Probability,
    };
    if matches!(group, Group::RawDenoise | Group::RgbDenoise) {
        c.classes = 3;
        c.class = usize::from(group == Group::RawDenoise || opts.denoise_linear);
    }
    if group == Group::Depth {
        c.class = usize::from(opts.depth_metric);
    }
    let inlined = external_data::inlined(src).map_err(tract_error)?;
    std::fs::create_dir_all(custom_dir(models_dir))?;
    let mut all = read_customs(models_dir);
    let old = all.iter().find(|o| o.group == group).cloned();
    let original_dest = c.path(models_dir);
    // A replacement with the same basename must not overwrite the registered file
    // before its new metadata has been published.
    let same_copy = inlined.is_none()
        && src != original_dest
        && original_dest.is_file()
        && old.as_ref().is_some_and(|old| old.path(models_dir) == original_dest)
        && identical_files(src, &original_dest)?;
    if src != original_dest && original_dest.exists() && !same_copy {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        loop {
            c.file = format!("{}-{}-{}-{}", group.key(), std::process::id(), NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed), safe_name(&c.name));
            if !c.path(models_dir).exists() {
                break;
            }
        }
    }
    let dest = c.path(models_dir);
    let copied = src != dest && !same_copy;
    if copied {
        match &inlined {
            Some(bytes) => li_fs::write_atomic(&dest, bytes).map(drop),
            None => std::fs::copy(src, &dest).map(drop),
        }
        .with_context(|| format!("Could not copy the model into {}", dest.display()))?;
    }
    // the copy is what runs, so it is what gets tested
    if let Err(error) = validate(&mut c, &dest, &opts) {
        if copied {
            let _ = std::fs::remove_file(&dest);
        }
        return Err(error.into());
    }
    all.retain(|o| o.group != group);
    all.push(c.clone());
    if let Err(error) = publish(models_dir, &all) {
        if copied {
            let _ = std::fs::remove_file(&dest);
        }
        return Err(error);
    }
    if let Some(old) = old
        && old.file != c.file
    {
        let _ = dispose(&old.path(models_dir));
    }
    forget_group(group);
    Ok(c)
}

/// Changes how the custom model of `group` is run (its input normalisation, a sky model's class,
/// a depth model's output) without importing it again.
pub fn set_custom_options(models_dir: &Path, group: Group, opts: CustomOptions) -> Result<Custom> {
    let mut all = read_customs(models_dir);
    let c = all.iter_mut().find(|c| c.group == group).context("This function has no custom model")?;
    c.norm = opts.norm;
    match group {
        Group::Sky if c.classes > 1 => {
            if opts.sky_class >= c.classes {
                return Err(CustomModelError::SkyClassOutOfRange { class: opts.sky_class, classes: c.classes }.into());
            }
            c.class = opts.sky_class;
        }
        Group::Depth => c.class = usize::from(opts.depth_metric),
        _ => {}
    }
    let changed = c.clone();
    write_customs(models_dir, &all)?;
    forget_group(group);
    Ok(changed)
}

/// Goes back to the official model of `group`: forgets the custom one and `dispose`s of its copy
/// (the Trash). Returns the bytes of the file that was put away (0 when there was none).
pub fn clear_custom(models_dir: &Path, group: Group, dispose: &dyn Fn(&Path) -> std::io::Result<()>) -> Result<u64> {
    let mut all = read_customs(models_dir);
    let Some(old) = all.iter().find(|o| o.group == group).cloned() else { return Ok(0) };
    let path = old.path(models_dir);
    let bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    if path.is_file() {
        dispose(&path).with_context(|| format!("Could not remove {}", path.display()))?;
    }
    all.retain(|o| o.group != group);
    write_customs(models_dir, &all)?;
    forget_group(group);
    Ok(bytes)
}

// which model runs

#[derive(Clone, Debug, PartialEq, Eq)]
struct ArtifactIdentity {
    path: PathBuf,
    len: u64,
    modified: Option<std::time::SystemTime>,
    #[cfg(unix)]
    change: (u64, u64, i64, i64),
}

impl ArtifactIdentity {
    fn read(path: &Path) -> std::io::Result<Self> {
        let path = std::fs::canonicalize(path)?;
        let meta = std::fs::metadata(&path)?;
        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt;
        Ok(Self {
            path,
            len: meta.len(),
            modified: meta.modified().ok(),
            #[cfg(unix)]
            change: (meta.dev(), meta.ino(), meta.ctime(), meta.ctime_nsec()),
        })
    }
}

/// The exact artifact and settings captured when an operation starts. Changing the global
/// model folder cannot redirect an in-flight request or reuse another artifact's probability map.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelSnapshot {
    /// Cache key: changes when the model or its settings do.
    key: String,
    path: PathBuf,
    cfg: Config,
    in_use: InUse,
    artifact: ArtifactIdentity,
}

impl ModelSnapshot {
    fn new(key: String, path: PathBuf, cfg: Config, in_use: InUse) -> Option<Self> {
        let artifact = ArtifactIdentity::read(&path).ok()?;
        Some(Self { key, path: artifact.path.clone(), cfg, in_use, artifact })
    }

    pub fn in_use(&self) -> &InUse {
        &self.in_use
    }

    pub fn is_current(&self) -> bool {
        ArtifactIdentity::read(&self.path).is_ok_and(|identity| identity == self.artifact)
    }

    /// The loaded model. `Ok(None)`: the file changed since the snapshot; `Err`: it failed to
    /// load or would take too long on this CPU.
    pub fn segmenter(&self) -> Result<Option<Segmenter>> {
        let slot = match self.cfg.task {
            Task::Subject if self.cfg.id == "inspyrenet" => &BETTER,
            Task::Subject => &SUBJECT,
            Task::Sky { .. } => &SKY,
            Task::Depth => &DEPTH,
            _ => return Ok(None),
        };
        cached(self.clone(), slot)
    }
}

pub fn snapshot(group: Group, models_dir: &Path) -> Option<ModelSnapshot> {
    active(group, models_dir)
}

fn active(group: Group, models_dir: &Path) -> Option<ModelSnapshot> {
    if let Some(c) = custom(models_dir, group) {
        let len = std::fs::metadata(c.path(models_dir)).map(|m| m.len()).unwrap_or(0);
        let key = format!("custom:{}:{}:{}:{}:{}:{}:{len}", c.file, c.size, c.norm.key(), c.classes, c.class, c.activation.key());
        return ModelSnapshot::new(key, c.path(models_dir), c.config(), InUse::Custom(c));
    }
    let spec = if group == Group::Subject {
        // the chosen tier, else the other one when only it is installed
        let tiers = if subject_quality(models_dir) { ["inspyrenet", "rmbg-1.4"] } else { ["rmbg-1.4", "inspyrenet"] };
        tiers.into_iter().filter_map(spec).find(|s| installed_bytes(models_dir, s).is_some())?
    } else {
        group.official()
    };
    let path = model_path(models_dir, spec);
    installed_bytes(models_dir, spec)?;
    ModelSnapshot::new(spec.id.to_owned(), path, Config::of_spec(spec), InUse::Official(spec))
}

/// Whether a model is ready for `group` (a custom one, or the installed official one).
pub fn available(group: Group, models_dir: &Path) -> bool {
    active(group, models_dir).is_some()
}

/// A process-wide cache of the loaded depth model.
/// `Ok(None)`: no depth model is installed; `Err`: the one set failed to load.
pub fn shared_depth(models_dir: &Path) -> Result<Option<Segmenter>> {
    active(Group::Depth, models_dir).map_or(Ok(None), |a| cached(a, &DEPTH))
}

/// The last CPU tier chosen in the shared cutout widget also serves Select Subject,
/// Object Selection and Library subject masks. It is kept in the models folder
/// (`segmentation/subject-tier.txt`); Fast until one is chosen.
static SUBJECT_QUALITY: std::sync::Mutex<Vec<(PathBuf, bool)>> = std::sync::Mutex::new(Vec::new());

fn subject_tier_file(models_dir: &Path) -> PathBuf {
    models_dir.join("segmentation").join("subject-tier.txt")
}

pub fn set_subject_quality(models_dir: &Path, better: bool) {
    let mut choices = SUBJECT_QUALITY.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if choices.iter().any(|(dir, choice)| dir == models_dir && *choice == better) {
        return;
    }
    choices.retain(|(dir, _)| dir != models_dir);
    choices.push((models_dir.to_owned(), better));
    let file = subject_tier_file(models_dir);
    if file.parent().is_some_and(Path::is_dir)
        && let Err(e) = li_fs::write_atomic(&file, if better { b"better" } else { b"fast" })
    {
        log::warn!("Could not save the subject model tier in {}: {e}", file.display());
    }
}

/// The subject tier chosen for `models_dir` (the saved one after a restart): `true` = Better Quality.
pub fn subject_quality(models_dir: &Path) -> bool {
    let mut choices = SUBJECT_QUALITY.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some((_, better)) = choices.iter().find(|(dir, _)| dir == models_dir) {
        return *better;
    }
    let better = std::fs::read_to_string(subject_tier_file(models_dir)).is_ok_and(|t| t.trim() == "better");
    choices.push((models_dir.to_owned(), better));
    better
}

/// A process-wide cache of the loaded subject model (loading takes a moment).
/// `Ok(None)`: no subject model is installed; `Err`: the one set failed to load.
pub fn shared(models_dir: &Path) -> Result<Option<Segmenter>> {
    let Some(active) = active(Group::Subject, models_dir) else { return Ok(None) };
    let slot = if active.cfg.id == "inspyrenet" { &BETTER } else { &SUBJECT };
    cached(active, slot)
}

/// A process-wide cache of the loaded sky model. `Ok(None)`: none is installed; `Err`: the one
/// set failed to load.
pub fn shared_sky(models_dir: &Path) -> Result<Option<Segmenter>> {
    active(Group::Sky, models_dir).map_or(Ok(None), |a| cached(a, &SKY))
}

type FileStamp = (std::path::PathBuf, u64, Option<std::time::SystemTime>, &'static str);
static VERIFIED_FILES: std::sync::Mutex<Vec<(FileStamp, bool)>> = std::sync::Mutex::new(Vec::new());

fn verified_file(stamp: FileStamp) -> bool {
    use sha2::{Digest, Sha256};
    let mut cache = VERIFIED_FILES.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some((_, valid)) = cache.iter().find(|(s, _)| *s == stamp) {
        return *valid;
    }
    let valid = if stamp.3.is_empty() {
        true
    } else {
        std::fs::File::open(&stamp.0).is_ok_and(|mut f| {
            let mut digest = Sha256::new();
            std::io::copy(&mut f, &mut digest).is_ok() && format!("{:x}", digest.finalize()) == stamp.3
        })
    };
    // Only keep the current metadata of each file, avoiding growth after repeated reinstalls.
    cache.retain(|(s, _)| s.0 != stamp.0);
    cache.push((stamp, valid));
    valid
}

/// Verified files from an interrupted bundle download can be retained on retry. Metadata
/// invalidates the checksum cache when a file changes, so Settings need not hash weights per frame.
pub fn file_installed(models_dir: &Path, file: Companion) -> bool {
    let path = companion_path(models_dir, file.file);
    let Ok(m) = std::fs::metadata(&path) else {
        return false;
    };
    m.is_file() && m.len() == file.bytes && verified_file((path, m.len(), m.modified().ok(), file.sha256))
}

/// Size of the installed copy of `spec`, when it is installed.
pub fn installed_bytes(models_dir: &Path, spec: &ModelSpec) -> Option<u64> {
    bundle_bytes(models_dir, spec, !matches!(spec.task, Task::Denoise { .. } | Task::Inpaint))
}
/// Restoration hashes run on the inference/download worker, never the Settings paint thread.
pub fn verified_denoise_bytes(models_dir: &Path, spec: &ModelSpec) -> Option<u64> {
    bundle_bytes(models_dir, spec, true)
}
fn bundle_bytes(models_dir: &Path, spec: &ModelSpec, verify: bool) -> Option<u64> {
    let mut total = 0;
    let mut stamp = Vec::new();
    for file in spec.files() {
        let path = companion_path(models_dir, file.file);
        let m = std::fs::metadata(&path).ok()?;
        if !m.is_file() || ((!spec.companions.is_empty() || matches!(spec.task, Task::Upscale | Task::Inpaint | Task::Denoise { .. })) && m.len() != file.bytes)
        {
            return None;
        }
        total += m.len();
        stamp.push((path, m.len(), m.modified().ok(), file.sha256));
    }
    if verify
        && (!spec.companions.is_empty() || matches!(spec.task, Task::Upscale | Task::Inpaint | Task::Denoise { .. }))
        && !stamp.into_iter().all(verified_file)
    {
        return None;
    }
    Some(total)
}

/// Removes an installed model, deleting its file. See [`remove_with`].
pub fn remove(models_dir: &Path, spec: &ModelSpec) -> Result<u64> {
    remove_with(models_dir, spec, &|p| std::fs::remove_file(p))
}

/// Removes an installed model: `dispose` gets its file (to move it to the Trash, or delete it),
/// and the loaded copy is forgotten so nothing keeps using it. Returns the bytes freed (0 when it
/// wasn't installed).
pub fn remove_with(models_dir: &Path, spec: &ModelSpec, dispose: &dyn Fn(&Path) -> std::io::Result<()>) -> Result<u64> {
    let shared = shared_models(spec);
    if shared.iter().any(|other| {
        other.id != spec.id && other.files().any(|file| spec.files().any(|own| own.file == file.file) && companion_path(models_dir, file.file).is_file())
    }) {
        anyhow::bail!("This model shares files with another installed model; remove the shared bundle instead.");
    }
    remove_bundle_with(models_dir, spec, dispose)
}

/// Independent models backed by the same physical files (for example both BSRGAN scales).
/// Components of one grouped function, such as SFace/YuNet, are already a single bundle.
pub fn shared_models(spec: &ModelSpec) -> Vec<&'static ModelSpec> {
    MODELS
        .iter()
        .filter(|other| {
            (other.id == spec.id || spec.group.is_none() || spec.group != other.group) && other.files().any(|a| spec.files().any(|b| a.file == b.file))
        })
        .collect()
}

/// Explicitly remove the whole shared bundle, after showing all affected models to the user.
pub fn remove_bundle_with(models_dir: &Path, spec: &ModelSpec, dispose: &dyn Fn(&Path) -> std::io::Result<()>) -> Result<u64> {
    for other in shared_models(spec) {
        forget(other);
    }
    forget(spec);
    let mut bytes = 0;
    // Also remove a partially installed bundle, so failed downloads can be cleaned up.
    for file in spec.files() {
        let path = companion_path(models_dir, file.file);
        match std::fs::metadata(&path) {
            Ok(m) if m.is_file() => {
                dispose(&path).with_context(|| format!("Could not remove {}", path.display()))?;
                bytes += m.len();
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(bytes)
}

/// Drops the process-wide loaded copy of `spec` (after it is removed).
fn forget(spec: &ModelSpec) {
    if spec.task == Task::Inpaint {
        lama::forget();
    }
    if matches!(spec.task, Task::ImageText) {
        clip::forget();
    }
    for slot in [&SUBJECT, &BETTER, &SKY, &DEPTH] {
        let mut g = slot.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if g.as_ref().is_some_and(|(key, _)| key.model == spec.id) {
            *g = None;
        }
    }
}

/// Drops the loaded copy of whatever serves `group`.
fn forget_group(group: Group) {
    if group == Group::Tagging {
        clip::forget();
        return;
    }
    let Some(slot) = slot(group) else { return };
    let mut g = slot.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    *g = None;
}

/// Configure tract once, before the first model runs (every loader calls this; the app also calls
/// it at start-up). tract's matrix kernels run on rayon: on whichever pool the calling thread belongs
/// to, i.e. the app's one sized compute pool (`lightcraft_sysmem::init`), or the background pool for
/// indexing. Tile-level `par_iter`s and tract's own parallelism then share one set of threads
/// instead of multiplying (never `Executor::multithread`, which adds a second
/// pool). `LOCAL_IMAGE_SEG_THREADS` is for measurement only: `1` runs tract single-threaded, `n > 1`
/// on a private `n`-thread pool.
/// API: tract-linalg 0.23.8 src/multithread.rs (Executor and set_default_executor).
pub fn init_executor() {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        #[cfg(not(target_arch = "wasm32"))]
        {
            use tract_linalg::multithread::{Executor, set_default_executor};
            let measured = std::env::var("LOCAL_IMAGE_SEG_THREADS").ok().and_then(|v| v.trim().parse::<usize>().ok());
            let executor = match measured {
                Some(1) => Executor::SingleThread,
                Some(n) if n > 1 => Executor::multithread(n.min(256)),
                _ => Executor::RayonGlobal,
            };
            log::info!(
                "tract executor: {}",
                match measured {
                    Some(1) => "single thread (LOCAL_IMAGE_SEG_THREADS=1)".to_string(),
                    Some(n) if n > 1 => format!("private {n}-thread pool (LOCAL_IMAGE_SEG_THREADS, measurement only)"),
                    _ => format!("shared rayon pool ({} threads)", rayon::current_num_threads()),
                }
            );
            set_default_executor(executor);
        }
    });
}

/// Verified, cached tier model, also used by the single and batch cutout paths.
pub fn shared_tier(models_dir: &Path, better: bool) -> Option<Segmenter> {
    let spec = spec(if better { "inspyrenet" } else { "rmbg-1.4" })?;
    if !spec.files().all(|file| file_installed(models_dir, file)) {
        return None;
    }
    let snapshot = ModelSnapshot::new(spec.id.into(), model_path(models_dir, spec), Config::of_spec(spec), InUse::Official(spec))?;
    cached(snapshot, if better { &BETTER } else { &SUBJECT }).unwrap_or_else(|e| {
        log::warn!("The {} subject model failed to load: {e:#}", spec.label);
        None
    })
}
static BETTER: Slot = std::sync::Mutex::new(None);

#[derive(Clone, PartialEq, Eq)]
struct CacheKey {
    model: String,
    artifact: ArtifactIdentity,
}
type Slot = std::sync::Mutex<Option<(CacheKey, Segmenter)>>;
static SUBJECT: Slot = std::sync::Mutex::new(None);
static SKY: Slot = std::sync::Mutex::new(None);
static DEPTH: Slot = std::sync::Mutex::new(None);

fn slot(group: Group) -> Option<&'static Slot> {
    match group {
        Group::Subject => Some(&SUBJECT),
        Group::Sky => Some(&SKY),
        Group::Depth => Some(&DEPTH),
        Group::Tagging | Group::Faces | Group::RawDenoise | Group::RgbDenoise | Group::Remove => None,
    }
}

fn cached(a: ModelSnapshot, slot: &Slot) -> Result<Option<Segmenter>> {
    if !a.is_current() {
        return Ok(None);
    }
    let identity = CacheKey { model: a.key.clone(), artifact: a.artifact.clone() };
    let mut g = slot.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some((key, s)) = g.as_ref()
        && *key == identity
    {
        return Ok(Some(s.clone()));
    }
    let s = Segmenter::load_cfg(a.cfg.clone(), &a.path).with_context(|| format!("The {} model failed to load", a.in_use.label()))?;
    if !a.is_current() {
        return Ok(None);
    }
    *g = Some((identity, s.clone()));
    Ok(Some(s))
}

#[cfg(test)]
mod tests;

#[cfg(test)]
#[test]
fn ds2_shared_model_removal_requires_explicit_bundle() {
    let dir = std::env::temp_dir().join(format!("ds2-shared-model-{}", std::process::id()));
    let spec = spec("bsrgan-2x").unwrap();
    let path = model_path(&dir, spec);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"shared weights").unwrap();
    assert!(remove(&dir, spec).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"shared weights");
    assert_eq!(shared_models(spec).iter().map(|s| s.id).collect::<Vec<_>>(), ["bsrgan-2x", "bsrgan-4x"]);
    assert_eq!(remove_bundle_with(&dir, spec, &|p| std::fs::remove_file(p)).unwrap(), 14);
    assert!(!path.exists());
    std::fs::remove_dir_all(dir).unwrap();
}

/// Lower the tract matmul budget on battery without changing its process-wide executor.
/// The override is scoped to this inference; foreground/global settings stay intact.
pub(crate) fn cpu_compute<R>(f: impl FnOnce() -> R) -> R {
    cpu_compute_policy(lightcraft_sysmem::power::conserving(), f)
}
fn cpu_compute_policy<R>(saving: bool, f: impl FnOnce() -> R) -> R {
    if !saving {
        return f();
    }
    // tract's scoped executor restores TLS only on normal return. Catch within the scope,
    // restore it, then propagate a panic to the existing job guard; never leave a shared
    // worker permanently in battery mode after one failed model.
    let result = tract_linalg::multithread::multithread_tract_scope(tract_linalg::multithread::Executor::SingleThread, || {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(f))
    });
    match result {
        Ok(value) => value,
        Err(panic) => std::panic::resume_unwind(panic),
    }
}

#[cfg(test)]
mod battery_executor_tests {
    #[test]
    fn battery_executor_override_restores_after_model_panic() {
        use tract_linalg::multithread::{Executor, current_tract_executor, multithread_tract_scope};
        multithread_tract_scope(Executor::RayonGlobal, || {
            let result = std::panic::catch_unwind(|| {
                super::cpu_compute_policy(true, || {
                    assert!(matches!(current_tract_executor(), Executor::SingleThread));
                    panic!("injected model panic");
                })
            });
            assert!(result.is_err());
            assert!(matches!(current_tract_executor(), Executor::RayonGlobal));
        });
    }
}

#[cfg(test)]
#[test]
fn custom_inventory_parity_write_fault_and_cleanup() {
    let dir = std::env::temp_dir().join(format!("custom-inventory-{}", std::process::id()));
    let model = Custom {
        group: Group::Subject,
        file: "fixture.onnx".into(),
        name: "Fixture".into(),
        size: 32,
        norm: Norm::IsNet,
        classes: 1,
        class: 0,
        activation: Activation::Logits,
    };
    write_customs(&dir, &[model]).unwrap();
    let path = custom_list(&dir);
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(bytes, b"subject\tfixture.onnx\tFixture\t32\tisnet\t1\t0\tlogits\n");
    let changed = Custom { file: "new.onnx".into(), name: "New".into(), size: 64, norm: Norm::ImageNet, ..read_customs(&dir)[0].clone() };
    {
        let _fault = li_fs::fail_writes_after(2);
        assert!(write_customs(&dir, &[changed]).is_err());
    }
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert_eq!(std::fs::read_dir(custom_dir(&dir)).unwrap().count(), 1);
    assert_eq!(read_customs(&dir)[0].file, "fixture.onnx");
    write_customs(&dir, &[]).unwrap();
    assert!(!path.exists());
    std::fs::remove_dir_all(dir).unwrap();
}
