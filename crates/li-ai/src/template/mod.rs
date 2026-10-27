//! Official ComfyUI templates as the recipe for models without a builder. The connected server's
//! templates are matched to installed files ([`matching`]), converted and canonicalised
//! ([`crate::graph`]), analysed ([`roles`]) and compiled into a [`TemplateRecipe`]
//! ([`compile`]); every tool (create, create with references, edit, refine, fill, LoRA styles)
//! is derived from the template's own graph by model-agnostic transforms ([`derive`]).
//! A stored recipe needs no network to run.

pub mod compile;
pub mod derive;
pub mod index;
pub mod matching;
pub mod roles;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::builders::{Params, Task};
use crate::comfy::ObjectInfo;
use crate::companions::Binding;
use crate::family::Range;
use crate::workflows::Built;
use roles::{NegativeSlot, Slot};

/// Bumped when canonicalisation, role detection or the transforms change what a recipe holds.
pub const RECIPE_VERSION: u32 = 1;

/// The main model file of a recipe.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FileRef {
    pub name: String,
    pub loader: String,
}

/// A template a recipe was compiled from.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TemplateSource {
    pub name: String,
    pub capability: String,
    pub sha256: String,
    pub templates_version: String,
    pub tags: Vec<String>,
    pub min_comfyui: Option<String>,
    /// The declared main file the installed one matched, and how.
    pub declared_main: String,
    pub tier: String,
}

/// Where the sampled latent comes from.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum LatentSpec {
    /// An empty latent node with width and height (`EmptyLatentImage`, `EmptyFlux2LatentImage`).
    Empty { node: String, class: String },
    /// Another node's output (an edit encoder's latent sized to the source).
    FromNode { node: String, output: u64 },
    #[default]
    None,
}

/// What a loaded image is for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "index")]
pub enum ImageRole {
    /// The image being edited (or refined).
    #[default]
    Source,
    Reference(usize),
    Mask,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ImageSlot {
    pub node: String,
    pub role: ImageRole,
}

/// How many reference images a graph can take, and where more go.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum RefCapacity {
    /// A `COMFY_AUTOGROW_V3` input: entries `<input>.<prefix><i>` or `<input>.<names[i]>`.
    Autogrow { node: String, input: String, prefix: Option<String>, names: Vec<String>, max: usize },
    /// Fixed optional IMAGE inputs (`image1..image3`).
    Fixed { node: String, inputs: Vec<String> },
    #[default]
    None,
}

impl RefCapacity {
    pub fn max(&self) -> usize {
        match self {
            Self::Autogrow { max, .. } => *max,
            Self::Fixed { inputs, .. } => inputs.len(),
            Self::None => 0,
        }
    }
}

/// The inputs a run sets.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Slots {
    pub prompt: Vec<Slot>,
    pub negative: NegativeSlot,
    pub seeds: Vec<Slot>,
    pub steps: Vec<Slot>,
    pub cfg: Vec<Slot>,
    pub sampler_name: Vec<Slot>,
    pub scheduler: Vec<Slot>,
    pub denoise: Vec<Slot>,
    pub width: Vec<Slot>,
    pub height: Vec<Slot>,
}

/// One canonical graph with its roles.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TaskGraph {
    /// `<template>` or `<template>#<mode>`.
    pub template: String,
    pub graph: Map<String, Value>,
    pub output: String,
    /// Sampler nodes, final pass last.
    pub samplers: Vec<String>,
    pub slots: Slots,
    pub latent: LatentSpec,
    /// The decoder's `vae` link.
    pub vae: Value,
    /// The main loader node; its output 0 is the MODEL (output 1 the CLIP of a checkpoint).
    pub loader: String,
    pub images: Vec<ImageSlot>,
    pub refs: RefCapacity,
    /// Image-only nodes between a reference loader and its consumer (copied per added reference).
    pub ref_transforms: Vec<String>,
    /// Steps fixed by the template's own sigmas (`ManualSigmas`).
    pub fixed_steps: Option<u32>,
    /// The size multiple the template's resolution node rounds to (0 when it has none).
    pub grid: u32,
    /// Loader inputs and the files they load.
    pub files: Vec<Binding>,
    /// The same graph with the template's LLM prompt enhancer (opt-in).
    pub enhanced: Option<Box<TaskGraph>>,
}

/// The template's own values.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TemplateDefaults {
    pub steps: u32,
    pub cfg: f32,
    pub sampler: String,
    pub scheduler: String,
    pub width: u32,
    pub height: u32,
    pub grid: u32,
    pub negative: String,
    pub denoise: f32,
}

/// Which controls are live for this model (hidden ones keep the template's value).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Controls {
    pub negative: bool,
    pub guidance: Option<Range>,
    pub steps: Option<Range>,
    pub sampler: bool,
    pub size_grid: u32,
    pub size_native: u32,
    pub size_min: u32,
    pub size_max: u32,
    pub references: u32,
}

/// The template's optional LLM prompt enhancer.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UpsamplerInfo {
    pub template: String,
    pub declared_file: String,
    pub url: String,
    /// The installed file it would use, when there is one.
    pub installed: Option<String>,
}

/// How a template model fills a selection.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InpaintWay {
    #[default]
    NoiseMask,
    Instruction,
}

/// A matched template this ComfyUI cannot run, and why.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Unusable {
    pub template: String,
    pub capability: String,
    pub detail: String,
    /// Node classes this ComfyUI lacks.
    pub missing_classes: Vec<String>,
    /// `(class, input, value)` options this ComfyUI doesn't offer.
    pub not_offered: Vec<(String, String, String)>,
    /// The template's `minComfyUIVersion`.
    pub min_comfyui: Option<String>,
}

/// Everything needed to run an installed model through official templates.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TemplateRecipe {
    pub model_key: String,
    pub main: FileRef,
    pub sources: Vec<TemplateSource>,
    pub create: Option<TaskGraph>,
    pub edit: Option<TaskGraph>,
    pub style: Option<TaskGraph>,
    pub defaults: TemplateDefaults,
    pub controls: Controls,
    pub inpaint: InpaintWay,
    pub upsampler: Option<UpsamplerInfo>,
    /// The family id whose `template` polish was applied.
    pub polish: Option<String>,
    pub recipe_version: u32,
    /// Matched templates this ComfyUI cannot run (their capability is off).
    pub skipped: Vec<Unusable>,
}

impl TemplateRecipe {
    /// Every loader binding of every graph (one per declared file and role).
    pub fn bindings(&self) -> Vec<&Binding> {
        let mut out: Vec<&Binding> = Vec::new();
        for g in [&self.create, &self.edit, &self.style].into_iter().flatten() {
            for b in g.files.iter().chain(g.enhanced.iter().flat_map(|e| e.files.iter())) {
                if !out.iter().any(|x| x.declared == b.declared && x.role == b.role) {
                    out.push(b);
                }
            }
        }
        out
    }

    /// Reference images Create can take (through the edit or style graph).
    pub fn reference_capacity(&self) -> usize {
        let edit = self.edit.as_ref().map(|g| g.refs.max()).unwrap_or(0);
        let style = self.style.as_ref().map(|g| g.refs.max()).unwrap_or(0);
        edit.max(style).min(self.controls.references as usize)
    }

    /// Whether the optional prompt enhancer can run (its graph exists and its file is installed).
    pub fn enhancer_ready(&self) -> bool {
        self.upsampler.as_ref().is_some_and(|u| u.installed.is_some()) && self.create.as_ref().is_some_and(|g| g.enhanced.is_some())
    }

    /// The API graph for one run: the template graph for the task with the transforms applied,
    /// the run's values set, and checked against `info`.
    pub fn build(&self, p: &Params, info: &ObjectInfo) -> Result<Built> {
        if p.references > self.controls.references as usize {
            bail!("This model takes at most {} reference images.", self.controls.references);
        }
        let variant = |g: &TaskGraph| -> TaskGraph {
            match (&g.enhanced, p.enhance) {
                (Some(e), true) => (**e).clone(),
                _ => g.clone(),
            }
        };
        let mut tg = match p.task {
            Task::Create if p.references > 0 => match (&self.style, &self.edit, &self.create) {
                (Some(style), None, _) => variant(style),
                (_, Some(edit), Some(create)) => derive::create_with_refs(&variant(edit), create)?,
                (Some(style), _, _) => variant(style),
                _ => bail!("This model takes no reference images."),
            },
            Task::Create => variant(self.create.as_ref().ok_or_else(|| anyhow::anyhow!("This model has no text-to-image template."))?),
            Task::Edit => variant(self.edit.as_ref().ok_or_else(|| anyhow::anyhow!("This model has no edit template."))?),
            Task::Refine { denoise } => match (&self.create, &self.edit) {
                (Some(create), _) => derive::refine(create, denoise, info)?,
                (None, Some(edit)) => edit.clone(),
                _ => bail!("This model has no template to refine with."),
            },
            Task::Inpaint { denoise } => match &self.create {
                Some(create) => derive::inpaint(create, denoise, info)?,
                None => bail!("This model has no template to fill with."),
            },
        };
        derive::apply_files(&mut tg, info)?;
        derive::grow_references(&mut tg, p.references, info)?;
        derive::chain_loras(&mut tg, p.loras, info)?;
        derive::set_values(&mut tg, self, p);
        let problems: Vec<String> = crate::graph::validate::validate(&tg.graph, info)
            .into_iter()
            .filter(|pr| !(pr.class == "LoadImage" && pr.input == "image"))
            .map(|pr| pr.describe())
            .collect();
        if !problems.is_empty() {
            bail!("The {} recipe doesn't match this ComfyUI ({}). Open Set Up Models and press Re-check.", tg.template, problems.join("; "));
        }
        Ok(derive::finish(tg))
    }
}
