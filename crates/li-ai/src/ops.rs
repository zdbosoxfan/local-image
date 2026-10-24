//! The AI operations the editor runs on worker threads. Each one checks the model against the
//! live ComfyUI inventory, prepares pixels, runs the graph and post-processes the result.

use anyhow::{Context, Result, bail};
use image::{GrayImage, Luma, RgbImage, RgbaImage, imageops};
use li_raster::RgbFrame;

use crate::builders::{self, Params, Slot, Task};
use crate::catalog::{self, Availability, ModelId};
use crate::comfy::{ComfyClient, JobControl, ObjectInfo, Stage};
use crate::family::{Encode, Family, InpaintMethod, RefMethod, Role};
use crate::imaging::{self, Rect};
use crate::workflows::{self, LoraUse};

#[cfg(test)]
#[path = "ops_review_tests.rs"]
mod review_tests;

/// What turns a request into a graph: a family's builder, or an official-template recipe.
pub enum Engine<'a> {
    Builder { family: &'a Family, files: &'a workflows::Files },
    Template { recipe: &'a crate::template::TemplateRecipe, family: &'a Family },
}

impl Engine<'_> {
    /// Sizes, inpaint method and prompt conventions.
    pub fn family(&self) -> &Family {
        match self {
            Engine::Builder { family, .. } | Engine::Template { family, .. } => family,
        }
    }

    pub fn build(&self, p: &Params, info: &ObjectInfo) -> Result<workflows::Built> {
        match self {
            Engine::Builder { family, files } => Ok(builders::build(family, files, p)),
            Engine::Template { recipe, .. } => recipe.build(p, info),
        }
    }
}

/// A ComfyUI error of a template-model run, explained with the files its template declares.
pub(crate) fn explain_with_recipe(e: anyhow::Error, recipe: Option<&crate::template::TemplateRecipe>) -> anyhow::Error {
    match (recipe, crate::comfy::friendly_of(&e)) {
        (Some(r), Some(f)) => {
            let declared =
                r.bindings().into_iter().filter(|b| matches!(b.role.as_str(), "clip" | "vae")).map(|b| (b.role.clone(), b.declared.clone())).collect();
            let template = r.sources.first().map(|s| s.name.clone());
            anyhow::anyhow!(f.with_recipe(declared, template))
        }
        _ => e,
    }
}

/// A random 48-bit seed (what 0.7 used, so seeds stay interchangeable).
pub fn new_seed() -> u64 {
    rand::random::<u64>() & ((1 << 48) - 1)
}

/// Which model repairs a selection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RemoveEngine {
    /// FLUX.2 Klein + fal's object-removal LoRA (16 GB).
    Klein,
    /// On-device FP32 big-LaMa, independent of ComfyUI.
    Lama { models_dir: std::path::PathBuf },
    /// Klein when connected; on-device LaMa when ComfyUI is unreachable.
    Auto { models_dir: std::path::PathBuf },
    /// Qwen Image 2.1 instruction edit, composited through the selection.
    Qwen { variant: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum GenerateMode {
    /// Text (and optional references) to a new image.
    #[default]
    Create,
    /// Instruction edit of the source image (native edit models), else a refine of it.
    Edit,
    /// Re-noise the source by `denoise` and resample (img2img).
    Refine,
    /// Regenerate the masked part of the source.
    Inpaint,
    /// Enlarge the source by `scale`, then refine it tile by tile at `denoise`.
    UpscaleRefine,
}

/// The second step of Draft → Refine: any refine model resamples the draft.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RefineStep {
    pub model: ModelId,
    pub variant: String,
    /// How much the refine pass may change (0.2–0.6 is typical).
    pub strength: f32,
    /// Use the verified recipe's strength; otherwise `strength` is explicit.
    #[serde(default)]
    pub default_strength: bool,
    pub steps: Option<u32>,
    pub guidance: Option<f32>,
    #[serde(default)]
    pub loras: Vec<LoraUse>,
    /// Enlarge the draft first (1 = same size).
    pub scale: f32,
}

#[derive(Clone, Debug)]
pub struct GenerateRequest {
    pub model: ModelId,
    pub variant: String,
    pub mode: GenerateMode,
    pub prompt: String,
    pub negative: String,
    pub width: u32,
    pub height: u32,
    pub seed: u64,
    pub steps: u32,
    pub guidance: f32,
    /// Seeded conditioning variety for Z-Image Turbo Create (0..100; 0 = Off).
    pub variety: f32,
    pub transparent: bool,
    /// Strength for Refine, Inpaint, Upscale-refine and starting images (0.05..1).
    pub denoise: f32,
    /// Resolve [steps, guidance, denoise] from the verified refine recipe. Numeric values
    /// otherwise remain explicit, including values equal to nominal model defaults.
    pub refine_defaults: [bool; 3],
    /// The image being edited, refined, inpainted or enlarged.
    pub source: Option<RgbaImage>,
    /// Inpaint: white = regenerate (the size of `source`).
    pub mask: Option<GrayImage>,
    pub references: Vec<RgbaImage>,
    pub loras: Vec<LoraUse>,
    /// Sampler and scheduler instead of the family's.
    pub sampler: Option<(String, String)>,
    pub refine: Option<RefineStep>,
    /// Upscale-refine factor.
    pub scale: f32,
}

impl GenerateRequest {
    pub fn new(model: ModelId, prompt: impl Into<String>) -> Self {
        let info = model.info();
        Self {
            model,
            variant: catalog::default_variant(model).to_owned(),
            mode: GenerateMode::Create,
            prompt: prompt.into(),
            negative: String::new(),
            width: 1024,
            height: 1024,
            seed: new_seed(),
            steps: info.steps.default as u32,
            guidance: info.guidance.default,
            variety: 0.0,
            transparent: false,
            denoise: 0.6,
            refine_defaults: [false; 3],
            source: None,
            mask: None,
            references: Vec::new(),
            loras: Vec::new(),
            sampler: None,
            refine: None,
            scale: 2.0,
        }
    }

    /// The source image: `source`, or (0.7 callers) the first reference in Edit mode.
    fn source_and_refs(&self) -> (Option<&RgbaImage>, &[RgbaImage]) {
        match (&self.source, self.mode) {
            (Some(s), _) => (Some(s), &self.references),
            (None, GenerateMode::Create) => (None, &self.references),
            (None, _) => (self.references.first(), self.references.get(1..).unwrap_or(&[])),
        }
    }

    /// Plain-language problems with the request, before anything is sent.
    pub fn validate(&self) -> Result<()> {
        if !(0.0..=100.0).contains(&self.variety) {
            bail!("Variety must be between 0 and 100.");
        }
        let info = self.model.try_info().with_context(|| format!("The model {} is not installed or known.", self.model.key()))?;
        let f = &info.resolved;
        let (source, refs) = self.source_and_refs();
        let needs_prompt = !matches!(self.mode, GenerateMode::UpscaleRefine | GenerateMode::Refine);
        if needs_prompt && self.prompt.trim().is_empty() {
            bail!("Describe the image first.");
        }
        if self.prompt.chars().count() > 4000 {
            bail!("The prompt is longer than 4000 characters.");
        }
        match self.mode {
            GenerateMode::Create if !info.text_to_image => bail!("{} does not create images from text.", info.label),
            GenerateMode::Create => {}
            GenerateMode::Inpaint if self.mask.as_ref().and_then(|m| imaging::bbox(m, 0)).is_none() => bail!("Make a selection to fill."),
            _ if source.is_none() => bail!("Open an image first."),
            _ => {}
        }
        if let (Some(s), Some(m)) = (source, &self.mask)
            && s.dimensions() != m.dimensions()
        {
            bail!("The selection does not match the image size.");
        }
        let adapter = matches!(f.pipeline.reference, RefMethod::IpAdapter | RefMethod::Redux);
        let max_refs = if adapter { 4 } else { info.max_references.saturating_sub(usize::from(self.mode == GenerateMode::Edit && f.capabilities.edit)) };
        if refs.len() > max_refs {
            bail!("{} takes at most {} reference image{}.", info.label, max_refs, if max_refs == 1 { "" } else { "s" });
        }
        if self.transparent && !info.transparent {
            bail!("{} cannot make transparent images; choose Qwen Image 2.1.", info.label);
        }
        // No built-in step range: the model's numbers are recommendations, and ComfyUI checks its own limits.
        if self.steps == 0 {
            bail!("Steps must be at least 1.");
        }
        if self.loras.len() > f.lora.max.max(3) as usize {
            bail!("Choose at most {} styles (LoRAs).", f.lora.max.max(3));
        }
        if !self.denoise.is_finite() || !(0.0..=1.0).contains(&self.denoise) {
            bail!("Denoise must be a finite value between 0 and 1.");
        }
        if let Some(r) = &self.refine {
            if !r.scale.is_finite() || !(1.0..=4.0).contains(&r.scale) {
                bail!("Refine scale must be a finite value between 1 and 4.");
            }
            if !r.strength.is_finite() || !(0.0..=1.0).contains(&r.strength) {
                bail!("Refine strength must be a finite value between 0 and 1.");
            }
            let ri = r.model.try_info().with_context(|| format!("The refine model {} is not available.", r.model.key()))?;
            if ri.resolved.kind != crate::family::FamilyKind::Image {
                bail!("{} cannot refine images.", ri.label);
            }
            if r.steps == Some(0) {
                bail!("Steps must be at least 1.");
            }
            if r.loras.len() > ri.resolved.lora.max.max(3) as usize {
                bail!("Choose at most {} styles (LoRAs).", ri.resolved.lora.max.max(3));
            }
        }
        Ok(())
    }
}

/// Effective sampling metadata is returned with the pixels so history describes the submitted job.
#[derive(Clone, Debug)]
pub struct Generated {
    pub image: RgbaImage,
    pub parameters: serde_json::Value,
}

/// The AI service: a ComfyUI client plus a cached inventory.
#[derive(Clone, Debug)]
pub struct Ai {
    pub client: ComfyClient,
    /// Submit only while ComfyUI's queue is empty (background verification never queues
    /// behind, or ahead of, someone's work).
    idle_only: bool,
}

impl Ai {
    pub fn new(host: impl Into<String>) -> Self {
        Self { client: ComfyClient::new(host), idle_only: false }
    }

    /// The same service for background checks: every prompt waits for an idle queue
    /// ([`ComfyClient::run_when_idle`]).
    pub fn idle_only(host: impl Into<String>) -> Self {
        Self { client: ComfyClient::new(host), idle_only: true }
    }

    fn run(&self, graph: serde_json::Value, images: &[(String, Vec<u8>)], ctl: &JobControl) -> Result<Vec<u8>> {
        if self.idle_only { self.client.run_when_idle(graph, images, ctl) } else { self.client.run(graph, images, ctl) }
    }

    fn resolve(&self, model: ModelId, variant: &str, info: &ObjectInfo) -> Result<Availability> {
        let preset = catalog::preset(model, variant).with_context(|| format!("{} has no {variant} preset", model.info().label))?;
        let a = catalog::availability(preset, info);
        if !a.available {
            bail!("{}", a.reason);
        }
        Ok(a)
    }

    fn inventory(&self) -> Result<ObjectInfo> {
        self.client
            .object_info()
            .map_err(|e| anyhow::anyhow!("Cannot reach ComfyUI at {}. Start the AI engine in Preferences › Local AI. ({e:#})", self.client.host()))
    }

    fn validate_lora_names(info: &ObjectInfo, loras: &[LoraUse], loader: &str) -> Result<Vec<LoraUse>> {
        if loras.is_empty() {
            return Ok(Vec::new());
        }
        let loader = if loader.is_empty() { "LoraLoaderModelOnly" } else { loader };
        if !info.has_node(loader) {
            bail!("Update ComfyUI: styles (LoRAs) need the {loader} node.");
        }
        let choices = info.choices(loader, "lora_name");
        loras
            .iter()
            .map(|l| {
                if !(-2.0..=2.0).contains(&l.strength) {
                    bail!("Style strength must be between −2 and 2.");
                }
                let found = choices.iter().find(|c| c.replace('\\', "/") == l.name.replace('\\', "/"));
                found
                    .map(|n| LoraUse { name: n.clone(), strength: l.strength })
                    .with_context(|| format!("The style {} is not installed in ComfyUI. Refresh, or finish its download.", l.name))
            })
            .collect()
    }

    /// Both passes resolve installed names and check known families through the same path.
    /// Unknown families remain usable, as in the UI's explicitly marked list.
    fn validate_loras(info: &ObjectInfo, loras: &[LoraUse], family: &Family) -> Result<Vec<LoraUse>> {
        if !loras.is_empty() && family.lora.loader.is_empty() {
            bail!("{} does not support styles (LoRAs).", family.label);
        }
        let installed = crate::inventory::loras();
        for lora in loras {
            let name = lora.name.replace('\\', "/");
            let identified = installed
                .iter()
                .find(|l| l.name.replace('\\', "/") == name)
                .and_then(|l| l.family.clone())
                .or_else(|| crate::inventory::family_of(&name, crate::arch::FileKind::Lora, None, None).map(|(f, _)| f));
            if let Some(f) = identified
                && !family.lora_fits(&f)
            {
                bail!("The style {} is for {f}, not {}.", lora.name, family.label);
            }
        }
        Self::validate_lora_names(info, loras, &family.lora.loader)
    }

    /// Repairs the selected area of `image` (the visible composite). Returns the repaired image;
    /// pixels outside the (cleaned) selection are unchanged. 8-bit adapter of
    /// [`Self::remove_objects_frame`].
    pub fn remove_objects(&self, image: &RgbImage, mask: &GrayImage, engine: &RemoveEngine, seed: u64, ctl: &JobControl) -> Result<RgbImage> {
        Ok(self.remove_output(&RgbFrame::from_rgb8(image), mask, engine, seed, ctl, false)?.to_rgb8())
    }

    /// Patch pixels for the existing document/develop compositor. Local soft coverage is
    /// applied by that compositor exactly once; standalone callers use remove_objects.
    /// 8-bit adapter of [`Self::remove_patch_frame`].
    pub fn remove_patch(&self, image: &RgbImage, mask: &GrayImage, engine: &RemoveEngine, seed: u64, ctl: &JobControl) -> Result<RgbImage> {
        Ok(self.remove_output(&RgbFrame::from_rgb8(image), mask, engine, seed, ctl, true)?.to_rgb8())
    }

    /// [`Self::remove_objects`] on normalized encoded RGB (no 8-bit step outside the ComfyUI wire).
    pub fn remove_objects_frame(&self, image: &RgbFrame, mask: &GrayImage, engine: &RemoveEngine, seed: u64, ctl: &JobControl) -> Result<RgbFrame> {
        self.remove_output(image, mask, engine, seed, ctl, false)
    }

    /// [`Self::remove_patch`] on normalized encoded RGB: the on-device engine stays in float end
    /// to end; ComfyUI engines quantize only the PNG they send and blend against `image`.
    pub fn remove_patch_frame(&self, image: &RgbFrame, mask: &GrayImage, engine: &RemoveEngine, seed: u64, ctl: &JobControl) -> Result<RgbFrame> {
        self.remove_output(image, mask, engine, seed, ctl, true)
    }

    fn remove_output(&self, image: &RgbFrame, mask: &GrayImage, engine: &RemoveEngine, seed: u64, ctl: &JobControl, patch: bool) -> Result<RgbFrame> {
        ctl.check()?;
        if image.dimensions() != (mask.width() as usize, mask.height() as usize) {
            bail!("The selection does not match the image size.");
        }
        let mask = imaging::clean_selection_mask(mask);
        if imaging::bbox(&mask, 0).is_none() {
            bail!("Paint over what you want to remove first.");
        }
        ctl.set_stage(Stage::Preparing);
        let local = |models_dir: &std::path::Path| {
            ctl.set_message("Loading on-device AI Remove");
            let model = li_seg::lama::shared(catalog::on_device_default(li_seg::Group::Remove), models_dir, &|| ctl.check())?;
            ctl.check()?;
            ctl.set_message("Removing on this computer");
            let progress = |done: usize, total: usize| ctl.set_step(done as u32, total as u32);
            if patch { model.run_patch_frame(image, &mask, &|| ctl.check(), &progress) } else { model.run_frame(image, &mask, &|| ctl.check(), &progress) }
        };
        if let RemoveEngine::Lama { models_dir } = engine {
            return local(models_dir);
        }
        let info = match self.inventory() {
            Ok(info) => info,
            Err(_) if matches!(engine, RemoveEngine::Auto { .. }) => {
                let RemoveEngine::Auto { models_dir } = engine else { unreachable!() };
                return local(models_dir);
            }
            Err(error) => return Err(error),
        };
        match engine {
            RemoveEngine::Lama { .. } => unreachable!(),
            RemoveEngine::Auto { .. } | RemoveEngine::Klein => self.remove_klein(image, &mask, seed, &info, ctl),
            RemoveEngine::Qwen { variant } => self.remove_qwen(image, &mask, variant, seed, &info, ctl, patch),
        }
    }

    fn remove_klein(&self, image: &RgbFrame, mask: &GrayImage, seed: u64, info: &ObjectInfo, ctl: &JobControl) -> Result<RgbFrame> {
        let a = self.resolve(ModelId::KleinRemove, "bf16", info)?;
        let (w, h) = mask.dimensions();
        let regions = imaging::plan_regions(mask, 64);
        let mut current = image.clone();
        for (i, region) in regions.iter().enumerate() {
            ctl.check()?;
            ctl.set_message(if regions.len() > 1 { format!("Area {} of {}", i + 1, regions.len()) } else { String::new() });
            let region_mask = GrayImage::from_fn(w, h, |x, y| {
                let inside = x >= region.x0 && x < region.x1 && y >= region.y0 && y < region.y1;
                Luma([if inside { mask.get_pixel(x, y)[0] } else { 0 }])
            });
            let b = imaging::klein_crop_box(w, h, *region);
            let (bx, by, bw, bh) = (b.x0 as usize, b.y0 as usize, b.width() as usize, b.height() as usize);
            let crop = current.crop(bx, by, bw, bh);
            let in_crop = Rect { x0: region.x0 - b.x0, y0: region.y0 - b.y0, x1: region.x1 - b.x0, y1: region.y1 - b.y0 };
            // The wire is 8-bit PNG; everything around it stays float.
            let highlighted = imaging::klein_highlight(&crop.to_rgb8(), in_crop);
            let built = workflows::klein_remove(&a.files, highlighted.width(), highlighted.height(), seed.wrapping_add(i as u64));
            let png = imaging::encode_png(&highlighted)?;
            let bytes = self.run(built.graph, &[(built.image_nodes[0].clone(), png)], ctl)?;
            let out = imaging::decode_frame(&bytes)?.rgb();
            if (out.width as u32, out.height as u32) != highlighted.dimensions() {
                bail!("The removal model returned an unexpected size.");
            }
            let mut generated = out.resize(bw, bh, li_raster::Filter::Lanczos3);
            generated.clamp01();
            let crop_mask = imageops::crop_imm(&region_mask, b.x0, b.y0, b.width(), b.height()).to_image();
            let blended = imaging::poisson_blend_frame(&crop, &generated, &crop_mask);
            for (x, y, m) in crop_mask.enumerate_pixels() {
                if m[0] > 0 {
                    current.set(bx + x as usize, by + y as usize, blended.get(x as usize, y as usize));
                }
            }
        }
        Ok(current)
    }

    #[allow(clippy::too_many_arguments)]
    fn remove_qwen(&self, image: &RgbFrame, mask: &GrayImage, variant: &str, seed: u64, info: &ObjectInfo, ctl: &JobControl, patch: bool) -> Result<RgbFrame> {
        let instruction = "<image1> is the photograph to edit. <image2> is a black and white selection mask: white marks the unwanted object or area to remove and black marks the area to keep. Remove the selected object completely and reconstruct a natural empty continuation of the surrounding background in its place. Match the surrounding texture, perspective, lighting and color. Do not insert any new objects, people, text or symbols. Do not draw the mask in the result. Keep the original composition and all other areas unchanged. ";
        let rgba = image::DynamicImage::ImageRgb8(image.to_rgb8()).to_rgba8();
        let mask_rgba = image::DynamicImage::ImageLuma8(mask.clone()).to_rgba8();
        let a = self.resolve(ModelId::Qwen, variant, info)?;
        let edited = self.qwen_edit(&[rgba, mask_rgba], instruction, "", (&a, &ModelId::Qwen.info().resolved), seed, 25, 1.0, 1.0, &[], info, ctl, None)?;
        let edited = RgbFrame::from_rgb8(&image::DynamicImage::ImageRgba8(imaging::over_white(&edited)).to_rgb8());
        // Match the surrounding colour while preserving generated gradients, as Klein does.
        let blended = imaging::poisson_blend_frame(image, &edited, mask);
        if patch {
            return Ok(blended);
        }
        Ok(RgbFrame::from_fn(image.width, image.height, |x, y| {
            let m = mask.get_pixel(x as u32, y as u32)[0] as f32 / 255.0;
            let (a, b) = (image.get(x, y), blended.get(x, y));
            [0, 1, 2].map(|k| a[k] * (1.0 - m) + b[k] * m)
        }))
    }

    /// Generative fill: changes only the selected area according to `prompt` (Qwen edit with the
    /// selection as a second reference, composited through the selection). 8-bit adapter of
    /// [`Self::fill_frame`].
    pub fn fill(&self, image: &RgbImage, mask: &GrayImage, prompt: &str, variant: &str, seed: u64, ctl: &JobControl) -> Result<RgbImage> {
        Ok(self.fill_frame(&RgbFrame::from_rgb8(image), mask, prompt, variant, seed, ctl)?.to_rgb8())
    }

    /// [`Self::fill`] on normalized encoded RGB; unselected pixels keep their full precision.
    pub fn fill_frame(&self, image: &RgbFrame, mask: &GrayImage, prompt: &str, variant: &str, seed: u64, ctl: &JobControl) -> Result<RgbFrame> {
        if image.dimensions() != (mask.width() as usize, mask.height() as usize) {
            bail!("The selection does not match the image size.");
        }
        if prompt.trim().is_empty() {
            bail!("Describe what should appear in the selection.");
        }
        if imaging::bbox(mask, 0).is_none() {
            bail!("Make a selection first.");
        }
        ctl.set_stage(Stage::Preparing);
        let info = self.inventory()?;
        let instruction = format!(
            "<image1> is the photograph to edit. <image2> is a black and white selection mask: white marks the area to change and black marks the area to keep. In the white area only: {}. Blend it naturally with the surrounding light, perspective and colour. Do not draw the mask in the result. Keep all other areas unchanged.",
            prompt.trim()
        );
        let rgba = image::DynamicImage::ImageRgb8(image.to_rgb8()).to_rgba8();
        let mask_rgba = image::DynamicImage::ImageLuma8(mask.clone()).to_rgba8();
        let a = self.resolve(ModelId::Qwen, variant, &info)?;
        let edited = self.qwen_edit(&[rgba, mask_rgba], &instruction, "", (&a, &ModelId::Qwen.info().resolved), seed, 25, 1.0, 1.0, &[], &info, ctl, None)?;
        let edited = image::DynamicImage::ImageRgba8(imaging::over_white(&edited)).to_rgb8();
        Ok(RgbFrame::from_fn(image.width, image.height, |x, y| {
            let p = edited.get_pixel(x as u32, y as u32);
            if mask.get_pixel(x as u32, y as u32)[0] > 0 { p.0.map(|v| v as f32 / 255.0) } else { image.get(x, y) }
        }))
    }

    /// Qwen "edit" with the resolved files `a` of model `f` (a curated preset or an installed Qwen
    /// Image 2.1 file): the output follows reference 1 and is resized back to its size.
    #[allow(clippy::too_many_arguments)]
    fn qwen_edit(
        &self,
        refs: &[RgbaImage],
        prompt: &str,
        negative: &str,
        (a, f): (&Availability, &Family),
        seed: u64,
        steps: u32,
        cfg: f32,
        denoise: f32,
        loras: &[LoraUse],
        info: &ObjectInfo,
        ctl: &JobControl,
        sampler: Option<&(String, String)>,
    ) -> Result<RgbaImage> {
        let loras = Self::validate_lora_names(info, loras, "LoraLoaderModelOnly")?;
        let qwen = &f.pipeline;
        let (ow, oh) = refs[0].dimensions();
        let pngs = refs
            .iter()
            .map(|r| {
                let (w, h) = imaging::qwen_canvas_size(r.width(), r.height());
                imaging::encode_png(&li_raster::resize_rgba8(r, w, h, li_raster::Filter::Lanczos3))
            })
            .collect::<Result<Vec<_>>>()?;
        let mut built = workflows::qwen(
            &a.files,
            &workflows::QwenParams {
                sampler_name: &qwen.sampler_name,
                scheduler: &qwen.scheduler,
                prompt,
                negative,
                width: 0,
                height: 0,
                seed,
                steps,
                cfg,
                denoise,
                references: refs.len(),
                use_cache: info.has_node("QwenImage21Cache"),
                loras: &loras,
            },
        );
        set_qwen_sampler(&mut built, sampler);
        let images: Vec<(String, Vec<u8>)> = built.image_nodes.iter().cloned().zip(pngs).collect();
        let out = imaging::decode_rgba(&self.run(built.graph, &images, ctl)?)?;
        Ok(if out.dimensions() != (ow, oh) { li_raster::resize_rgba8(&out, ow, oh, li_raster::Filter::Lanczos3) } else { out })
    }

    /// Background removal with Qwen: returns an alpha matte the size of `image`. Callers apply it
    /// as a layer mask so the subject keeps its original pixels.
    pub fn cutout(&self, image: &RgbaImage, variant: &str, hint: &str, seed: u64, ctl: &JobControl) -> Result<GrayImage> {
        ctl.check()?;
        if matches!(variant, "fast" | "quick" | "better") {
            let dir = crate::AiSettings::cached_model_dir();
            return cpu_cutout(image, variant == "better", &dir, ctl);
        }
        ctl.set_stage(Stage::Preparing);
        let info = self.inventory()?;
        let prompt = format!(
            "Remove the background from <image1> and preserve the subject exactly. Return an RGBA PNG with a transparent background and a real alpha channel. Keep the original subject position, scale, details and colors. {}",
            hint.trim()
        );
        let a = self.resolve(ModelId::Qwen, variant, &info)?;
        let out = self.qwen_edit(std::slice::from_ref(image), &prompt, "", (&a, &ModelId::Qwen.info().resolved), seed, 25, 1.0, 1.0, &[], &info, ctl, None)?;
        imaging::finish_cutout(&out)
    }

    /// An empty background plate for compositing behind a cutout.
    pub fn generate_background(&self, description: &str, width: u32, height: u32, variant: &str, seed: u64, ctl: &JobControl) -> Result<RgbaImage> {
        let prompt = format!(
            "Create an empty photographic background plate for compositing. Leave the central foreground and supporting surface clear, with generous empty space for a subject to be placed later. Show only the environment, lighting and surfaces. Do not include people, animals, products, foreground subjects, text, logos or watermarks. Return a fully opaque image. Background description: {}",
            if description.trim().is_empty() { "A simple studio cyclorama with soft natural light." } else { description.trim() }
        );
        let mut req = GenerateRequest::new(ModelId::Qwen, prompt);
        req.variant = variant.to_owned();
        req.negative = "people, person, human, animal, product, foreground object, subject, furniture in the foreground, text, logo, watermark".into();
        req.width = width;
        req.height = height;
        req.seed = seed;
        req.steps = 25;
        req.validate()?;
        ctl.check()?;
        ctl.set_stage(Stage::Preparing);
        let info = self.inventory()?;
        let availability = self.resolve(req.model, &req.variant, &info)?;
        self.qwen21_generate(&req, &availability, &[], &info, ctl, true)
    }

    /// Generates with any model: Create, Edit, Refine, Inpaint or Upscale-refine, then the
    /// optional Draft → Refine pass. Returns the image at the request's (or the source's) size.
    pub fn generate(&self, req: &GenerateRequest, ctl: &JobControl) -> Result<RgbaImage> {
        Ok(self.generate_detailed(req, ctl)?.image)
    }

    pub fn generate_detailed(&self, requested: &GenerateRequest, ctl: &JobControl) -> Result<Generated> {
        requested.validate()?;
        ctl.set_stage(Stage::Preparing);
        let info = self.inventory()?;
        let mut req = requested.clone();
        let recipe = if req.mode == GenerateMode::Refine {
            let recipe = crate::refine::ensure(&self.client, req.model, &req.variant, ctl)?;
            req.loras = Self::check_recipe_loras(&recipe, &req.loras, &info)?;
            let defaults = recipe.defaults();
            if req.refine_defaults[0] {
                req.steps = defaults.steps;
            }
            if req.refine_defaults[1] {
                req.guidance = defaults.cfg;
            }
            if req.refine_defaults[2] {
                req.denoise = defaults.denoise;
            }
            if req.sampler.is_none() {
                req.sampler = Some((defaults.sampler, defaults.scheduler));
            }
            Some(recipe)
        } else {
            None
        };
        // Validate the second pass before submitting the draft, so a selected style is never lost.
        let second_recipe = if let Some(step) = &mut req.refine {
            let recipe = crate::refine::ensure(&self.client, step.model, &step.variant, ctl)?;
            step.loras = Self::check_recipe_loras(&recipe, &step.loras, &info)?;
            let defaults = recipe.defaults();
            if step.default_strength {
                step.strength = defaults.denoise;
            }
            step.steps = Some(step.steps.unwrap_or(defaults.steps));
            step.guidance = Some(step.guidance.unwrap_or(defaults.cfg));
            Some(recipe)
        } else {
            None
        };
        let out = self.generate_once(&req, &info, recipe.as_ref(), ctl).map_err(|e| explain_with_recipe(e, req.model.info().recipe.as_deref()))?;
        let image = match (&req.refine, &second_recipe) {
            (Some(step), Some(recipe)) => {
                ctl.set_message("Refining the draft");
                self.refine_pass(&out, &req, step, recipe, &info, ctl)?
            }
            _ => out,
        };
        let family = &req.model.info().resolved;
        let (sampler, scheduler) = req.sampler.clone().unwrap_or_else(|| (family.pipeline.sampler_name.clone(), family.pipeline.scheduler.clone()));
        // Report the actual latent strength, including full-noise text/reference creation.
        let denoise = match req.mode {
            GenerateMode::Create if family.pipeline.reference == RefMethod::InitImage && !req.references.is_empty() => req.denoise,
            GenerateMode::Create => 1.0,
            GenerateMode::Edit if family.capabilities.edit => 1.0,
            _ => req.denoise,
        };
        let mut parameters = serde_json::json!({"steps":req.steps,"guidance":req.guidance,"denoise":req.denoise,
            "sampler":sampler,"scheduler":scheduler,"width":req.width,"height":req.height,"output_width":image.width(),"output_height":image.height(),"refine":req.refine});
        parameters["denoise"] = serde_json::json!(denoise);
        if let Some(recipe) = recipe {
            parameters["recipe"] = serde_json::json!(recipe.label());
        }
        if let Some(recipe) = second_recipe {
            let defaults = recipe.defaults();
            parameters["refine"]["default_strength"] = serde_json::json!(false);
            parameters["refine"]["recipe"] = serde_json::json!(recipe.label());
            parameters["refine"]["sampler"] = serde_json::json!(defaults.sampler);
            parameters["refine"]["scheduler"] = serde_json::json!(defaults.scheduler);
        }
        Ok(Generated { image, parameters })
    }

    fn check_recipe_loras(recipe: &crate::refine::Recipe, loras: &[LoraUse], info: &ObjectInfo) -> Result<Vec<LoraUse>> {
        recipe.validate_loras(loras)?;
        match recipe.lora_profile() {
            Some(profile) => Self::validate_loras(info, loras, profile),
            None => Ok(Vec::new()),
        }
    }

    fn generate_once(&self, req: &GenerateRequest, info: &ObjectInfo, recipe: Option<&crate::refine::Recipe>, ctl: &JobControl) -> Result<RgbaImage> {
        if req.mode == GenerateMode::Refine {
            let source = req.source_and_refs().0.context("Open an image first.")?;
            let recipe = recipe.context("Refinement recipe was not resolved.")?;
            let loras = req.loras.as_slice();
            let denoise = req.denoise;
            let steps = req.steps;
            let cfg = req.guidance;
            let (prompt, negative) = match recipe {
                crate::refine::Recipe::Family { profile, .. } => prompt_rules(profile, &req.prompt, &req.negative),
                _ => (req.prompt.clone(), req.negative.clone()),
            };
            let p = Params {
                task: Task::Refine { denoise },
                negative: &negative,
                loras,
                sampler: req.sampler.as_ref().map(|(s, c)| (s.as_str(), c.as_str())),
                ..Params::create(&prompt, source.width(), source.height(), req.seed, steps, cfg)
            };
            return recipe.run(&self.client, source, &p, ctl);
        }
        let m = req.model.info();
        let f = &m.resolved;
        let a = self.resolve(req.model, &req.variant, info)?;
        // Resolved before the Qwen 2.1 branch: a template-engine Qwen file never takes it.
        let engine = match &m.recipe {
            Some(recipe) => Engine::Template { recipe, family: f },
            None => Engine::Builder { family: f, files: &a.files },
        };
        let loras = Self::validate_loras(info, &req.loras, f)?;
        let (prompt, negative) = prompt_rules(f, &req.prompt, &req.negative);
        let (source, refs) = req.source_and_refs();
        if m.recipe.is_none() && f.pipeline.encode == Encode::QwenImage21 && matches!(req.mode, GenerateMode::Create | GenerateMode::Edit) {
            return self.qwen21_generate(req, &a, &loras, info, ctl, false);
        }
        let sampler = req.sampler.as_ref().map(|(a, b)| (a.as_str(), b.as_str()));
        let base = Params {
            task: Task::Create,
            prompt: &prompt,
            negative: &negative,
            width: req.width,
            height: req.height,
            seed: req.seed,
            steps: req.steps,
            cfg: req.guidance,
            references: 0,
            loras: &loras,
            batch: 1,
            sampler,
            variety: req.variety,
            init_denoise: req.denoise,
            enhance: false,
        };
        match req.mode {
            GenerateMode::Create => {
                let (w, h) = imaging::snap_size(req.width, req.height, f.sizes.multiple.max(8));
                let refs = prepare_refs(f, refs, w, h)?;
                let p = Params { width: w, height: h, references: refs.len(), ..base };
                let out = self.run_built(engine.build(&p, info)?, None, None, &refs, ctl)?;
                Ok(imaging::over_white(&out))
            }
            GenerateMode::Edit if f.capabilities.edit => {
                let src = source.context("Open an image to edit.")?;
                let (w, h) = crate::inpaint::work_size(src.width(), src.height(), f.sizes.native, f.sizes.max, f.sizes.multiple);
                let png = imaging::encode_png(&li_raster::resize_rgba8(&imaging::over_white(src), w, h, li_raster::Filter::Lanczos3))?;
                let refs = prepare_refs(f, refs, w, h)?;
                let p = Params { task: Task::Edit, width: w, height: h, references: refs.len(), ..base };
                let out = self.run_built(engine.build(&p, info)?, Some(png), None, &refs, ctl)?;
                Ok(resize_to(&imaging::over_white(&out), src.width(), src.height()))
            }
            GenerateMode::Edit | GenerateMode::Refine => {
                let src = source.context("Open an image first.")?;
                let denoise = req.denoise;
                self.refine_image(src, &engine, info, Params { task: Task::Refine { denoise }, ..base }, prepare_refs(f, refs, 1024, 1024)?, ctl)
            }
            GenerateMode::Inpaint => {
                let src = source.context("Open an image first.")?;
                let mask = req.mask.as_ref().context("Make a selection to fill.")?;
                self.inpaint_image(src, mask, &engine, info, base, req.denoise, refs, ctl)
            }
            GenerateMode::UpscaleRefine => {
                let src = source.context("Open an image to enlarge.")?;
                self.upscale_refine(src, req.scale, &engine, info, Params { task: Task::Refine { denoise: req.denoise }, ..base }, ctl)
            }
        }
    }

    /// Runs a built graph, uploading each `LoadImage` input by its slot.
    fn run_built(
        &self,
        built: crate::workflows::Built,
        source: Option<Vec<u8>>,
        mask: Option<Vec<u8>>,
        refs: &[Vec<u8>],
        ctl: &JobControl,
    ) -> Result<RgbaImage> {
        let mut images = Vec::new();
        for (id, slot) in built.image_nodes.iter().zip(&built.slots) {
            let png = match slot {
                Slot::Source => source.clone(),
                Slot::Mask => mask.clone(),
                Slot::Reference(i) => refs.get(*i).cloned(),
            };
            images.push((id.clone(), png.context("an image input of the graph has no picture")?));
        }
        imaging::decode_rgba(&self.run(built.graph, &images, ctl)?)
    }

    /// img2img of the whole image at the family's working size; the result keeps `src`'s size.
    fn refine_image(&self, src: &RgbaImage, engine: &Engine, info: &ObjectInfo, p: Params, refs: Vec<Vec<u8>>, ctl: &JobControl) -> Result<RgbaImage> {
        let f = engine.family();
        let (w, h) = crate::inpaint::work_size(src.width(), src.height(), f.sizes.native, f.sizes.max, f.sizes.multiple);
        let png = imaging::encode_png(&li_raster::resize_rgba8(&imaging::over_white(src), w, h, li_raster::Filter::Lanczos3))?;
        let p = Params { width: w, height: h, references: refs.len(), ..p };
        let out = self.run_built(engine.build(&p, info)?, Some(png), None, &refs, ctl)?;
        Ok(resize_to(&imaging::over_white(&out), src.width(), src.height()))
    }

    /// Inpaints the masked area: context crop, noise mask, pre-fill (or green fill for instruction
    /// models), then composites the result back through a feathered mask.
    #[allow(clippy::too_many_arguments)]
    fn inpaint_image(
        &self,
        src: &RgbaImage,
        mask: &GrayImage,
        engine: &Engine,
        info: &ObjectInfo,
        base: Params,
        denoise: f32,
        refs: &[RgbaImage],
        ctl: &JobControl,
    ) -> Result<RgbaImage> {
        use crate::inpaint;
        let f = engine.family();
        let plan = inpaint::plan(mask).context("Make a selection to fill.")?;
        let rgb = image::DynamicImage::ImageRgba8(imaging::over_white(src)).to_rgb8();
        let crop = imageops::crop_imm(&rgb, plan.context.x0, plan.context.y0, plan.context.width(), plan.context.height()).to_image();
        let nmask = inpaint::noise_mask(mask, &plan);
        let (w, h) = inpaint::work_size(crop.width(), crop.height(), f.sizes.native, f.sizes.max, f.sizes.multiple);
        let scaled = |img: &RgbImage| li_raster::resize_rgb8(img, w, h, li_raster::Filter::Lanczos3);
        let refs_png = prepare_refs(f, refs, w, h)?;
        let prompt_owned;
        let (task, source_img, mask_png) = match f.pipeline.inpaint {
            InpaintMethod::Instruction => {
                prompt_owned = format!("{} {}", inpaint::GREEN_INSTRUCTION, base.prompt);
                (Task::Edit, inpaint::green_fill(&crop, &nmask), None)
            }
            InpaintMethod::ModelConditioning => {
                prompt_owned = base.prompt.to_owned();
                (Task::Inpaint { denoise: 1.0 }, crop.clone(), Some(nmask.clone()))
            }
            InpaintMethod::NoiseMask | InpaintMethod::None => {
                prompt_owned = base.prompt.to_owned();
                let filled = if denoise >= 0.99 {
                    inpaint::blur_fill(&crop, &nmask, (plan.context.width().max(plan.context.height()) as f32 / 16.0).clamp(8.0, 64.0))
                } else {
                    crop.clone()
                };
                (Task::Inpaint { denoise }, filled, Some(nmask.clone()))
            }
        };
        let source_png = imaging::encode_png(&scaled(&source_img))?;
        let mask_png = mask_png.map(|m| imaging::encode_png(&li_raster::resize_gray8(&m, w, h, li_raster::Filter::Triangle))).transpose()?;
        let p = Params { task, prompt: &prompt_owned, width: w, height: h, references: refs_png.len(), ..base };
        ctl.set_message("Filling the selection");
        let out = self.run_built(engine.build(&p, info)?, Some(source_png), mask_png, &refs_png, ctl)?;
        let out_rgb = image::DynamicImage::ImageRgba8(imaging::over_white(&out)).to_rgb8();
        let comp = inpaint::compositing_mask(mask, &plan);
        let merged = inpaint::composite(&rgb, &out_rgb, &plan, &comp);
        Ok(RgbaImage::from_fn(src.width(), src.height(), |x, y| {
            let p = merged.get_pixel(x, y);
            let alpha = src.get_pixel(x, y)[3].max(comp.get_pixel(x, y)[0]);
            image::Rgba([p[0], p[1], p[2], alpha])
        }))
    }

    /// Enlarges by `scale` (Lanczos), then refines in overlapping tiles at the family's native
    /// size, blended with feathered weights.
    fn upscale_refine(&self, src: &RgbaImage, scale: f32, engine: &Engine, info: &ObjectInfo, p: Params, ctl: &JobControl) -> Result<RgbaImage> {
        let f = engine.family();
        let scale = scale.clamp(1.0, 4.0);
        let (tw, th) = (((src.width() as f32 * scale) as u32).min(8192), ((src.height() as f32 * scale) as u32).min(8192));
        let big = li_raster::resize_rgba8(&imaging::over_white(src), tw, th, li_raster::Filter::Lanczos3);
        let tile = f.sizes.native.clamp(512, 1536);
        let overlap = tile / 8;
        let starts = |len: u32| -> Vec<u32> {
            if len <= tile {
                return vec![0];
            }
            let n = (len - overlap).div_ceil(tile - overlap);
            (0..n).map(|i| ((len - tile) as u64 * i as u64 / (n - 1).max(1) as u64) as u32).collect()
        };
        let (xs, ys) = (starts(tw), starts(th));
        let mut acc = vec![[0f32; 4]; (tw * th) as usize];
        let total = xs.len() * ys.len();
        let prompt = if p.prompt.trim().is_empty() { "high quality, sharp details" } else { p.prompt };
        let mut k = 0;
        for &y0 in &ys {
            for &x0 in &xs {
                k += 1;
                ctl.check()?;
                ctl.set_message(if total > 1 { format!("Tile {k} of {total}") } else { String::new() });
                let (w, h) = (tile.min(tw), tile.min(th));
                let crop = imageops::crop_imm(&big, x0, y0, w, h).to_image();
                let (ww, wh) = imaging::snap_size(w, h, f.sizes.multiple.max(8));
                let png = imaging::encode_png(&li_raster::resize_rgba8(&crop, ww, wh, li_raster::Filter::Lanczos3))?;
                let tp = Params { prompt, width: ww, height: wh, ..p.clone() };
                let out = self.run_built(engine.build(&tp, info)?, Some(png), None, &[], ctl)?;
                let out = resize_to(&out, w, h);
                for yy in 0..h {
                    for xx in 0..w {
                        // Weight falls off over the overlap at inner edges.
                        let edge = |v: u32, len: u32, at_start: bool, at_end: bool| -> f32 {
                            let a = if at_start { 1.0 } else { ((v + 1) as f32 / overlap.max(1) as f32).min(1.0) };
                            let b = if at_end { 1.0 } else { ((len - v) as f32 / overlap.max(1) as f32).min(1.0) };
                            a.min(b).max(1e-3)
                        };
                        let wgt = edge(xx, w, x0 == 0, x0 + w >= tw) * edge(yy, h, y0 == 0, y0 + h >= th);
                        let px = out.get_pixel(xx, yy);
                        let a = &mut acc[((y0 + yy) * tw + x0 + xx) as usize];
                        for c in 0..3 {
                            a[c] += px[c] as f32 * wgt;
                        }
                        a[3] += wgt;
                    }
                }
            }
        }
        let alpha = li_raster::resize_rgba8(src, tw, th, li_raster::Filter::Lanczos3);
        Ok(RgbaImage::from_fn(tw, th, |x, y| {
            let a = acc[(y * tw + x) as usize];
            let d = a[3].max(1e-6);
            image::Rgba([(a[0] / d).round() as u8, (a[1] / d).round() as u8, (a[2] / d).round() as u8, alpha.get_pixel(x, y)[3]])
        }))
    }

    /// Draft → Refine: the refine model resamples the draft (enlarged by `step.scale`).
    fn refine_pass(
        &self,
        draft: &RgbaImage,
        req: &GenerateRequest,
        step: &RefineStep,
        recipe: &crate::refine::Recipe,
        info: &ObjectInfo,
        ctl: &JobControl,
    ) -> Result<RgbaImage> {
        let defaults = recipe.defaults();
        let src = if step.scale > 1.01 {
            let (w, h) = ((draft.width() as f32 * step.scale) as u32, (draft.height() as f32 * step.scale) as u32);
            li_raster::resize_rgba8(draft, w.min(8192), h.min(8192), li_raster::Filter::Lanczos3)
        } else {
            draft.clone()
        };
        Self::check_recipe_loras(recipe, &step.loras, info)?;
        let loras = step.loras.as_slice();
        let (prompt, negative) = match recipe {
            crate::refine::Recipe::Family { profile, .. } => prompt_rules(profile, &req.prompt, &req.negative),
            _ => (req.prompt.clone(), req.negative.clone()),
        };
        let p = Params {
            task: Task::Refine { denoise: step.strength },
            negative: &negative,
            loras,
            ..Params::create(
                &prompt,
                src.width(),
                src.height(),
                req.seed.wrapping_add(1),
                step.steps.unwrap_or(defaults.steps),
                step.guidance.unwrap_or(defaults.cfg),
            )
        };
        if let crate::refine::Recipe::Family { profile, files, .. } = &recipe
            && u64::from(src.width()) * u64::from(src.height()) > u64::from(profile.sizes.max).pow(2)
        {
            return self.upscale_refine(&src, 1.0, &Engine::Builder { family: profile, files }, info, p, ctl);
        }
        recipe.run(&self.client, &src, &p, ctl)
    }

    /// Qwen Image 2.1 create / edit, as in 0.7 (transparent output, reference canvas rules).
    fn qwen21_generate(
        &self,
        req: &GenerateRequest,
        a: &Availability,
        loras: &[LoraUse],
        info: &ObjectInfo,
        ctl: &JobControl,
        background: bool,
    ) -> Result<RgbaImage> {
        let (source, refs) = req.source_and_refs();
        if req.mode == GenerateMode::Edit {
            let mut all = vec![source.context("Open an image to edit.")?.clone()];
            all.extend(refs.iter().cloned());
            let out = self.qwen_edit(
                &all,
                &req.prompt,
                &req.negative,
                (a, &req.model.info().resolved),
                req.seed,
                req.steps,
                req.guidance,
                1.0,
                loras,
                info,
                ctl,
                req.sampler.as_ref(),
            )?;
            return Ok(imaging::over_white(&out));
        }
        let grid = if refs.is_empty() { 16 } else { 32 };
        let (w, h) = imaging::snap_size(req.width, req.height, grid);
        let mut prompt = req.prompt.trim().to_owned();
        prompt.push_str(if req.transparent {
            "\nCreate the subject on a fully transparent background with real RGBA alpha. Do not draw a checkerboard or an opaque backdrop. Preserve fine subject edges."
        } else {
            "\nReturn a fully opaque image with a complete background."
        });
        let pngs = refs
            .iter()
            .enumerate()
            .map(|(i, r)| {
                if i == 0 {
                    imaging::encode_png(&imaging::pad_to(r, w, h))
                } else {
                    let (rw, rh) = imaging::qwen_canvas_size(r.width(), r.height());
                    imaging::encode_png(&li_raster::resize_rgba8(r, rw, rh, li_raster::Filter::Lanczos3))
                }
            })
            .collect::<Result<Vec<_>>>()?;
        let pipeline = &req.model.info().resolved.pipeline;
        let mut built = workflows::qwen(
            &a.files,
            &workflows::QwenParams {
                sampler_name: &pipeline.sampler_name,
                scheduler: &pipeline.scheduler,
                prompt: &prompt,
                negative: &req.negative,
                width: w,
                height: h,
                seed: req.seed,
                steps: req.steps,
                cfg: req.guidance,
                denoise: 1.0,
                references: pngs.len(),
                use_cache: info.has_node("QwenImage21Cache"),
                loras,
            },
        );
        set_qwen_sampler(&mut built, req.sampler.as_ref());
        let images: Vec<_> = built.image_nodes.iter().cloned().zip(pngs).collect();
        let out = imaging::decode_rgba(&self.run(built.graph, &images, ctl)?)?;
        if background {
            return imaging::finish_background(&out);
        }
        if req.transparent {
            let alpha = imaging::finish_cutout(&out)?;
            let mut out = out;
            for (x, y, p) in out.enumerate_pixels_mut() {
                p[3] = alpha.get_pixel(x, y)[0];
            }
            Ok(out)
        } else {
            Ok(imaging::over_white(&out))
        }
    }

    /// Runs an imported custom workflow: its marked fields get `inputs`; `images` fills its
    /// `li:image…` inputs in order, `mask` its `li:mask`.
    pub fn run_custom(
        &self,
        w: &crate::custom::CustomWorkflow,
        inputs: &crate::custom::Inputs,
        images: &[RgbaImage],
        mask: Option<&GrayImage>,
        ctl: &JobControl,
    ) -> Result<RgbaImage> {
        use crate::custom::FieldKind;
        ctl.set_stage(Stage::Preparing);
        let info = self.inventory()?;
        let missing: Vec<String> = w
            .graph
            .as_object()
            .into_iter()
            .flat_map(|o| o.values())
            .filter_map(|n| n["class_type"].as_str())
            .filter(|c| !info.has_node(c))
            .map(str::to_owned)
            .collect();
        if !missing.is_empty() {
            bail!("This workflow needs nodes ComfyUI doesn't have: {}. Install them with ComfyUI-Manager.", missing.join(", "));
        }
        if w.has(&FieldKind::Prompt) && inputs.prompt.trim().is_empty() {
            bail!("Describe the image first.");
        }
        let (graph, slots) = crate::custom::prepare(w, inputs);
        let mut uploads = Vec::new();
        for (kind, node) in slots {
            let png = match kind {
                FieldKind::Image(i) => imaging::encode_png(&imaging::over_white(
                    images.get(i as usize).with_context(|| format!("This workflow takes image {} — add it first.", i + 1))?,
                ))?,
                FieldKind::Mask => imaging::encode_png(&image::DynamicImage::ImageLuma8(mask.context("This workflow needs a selection.")?.clone()).to_rgb8())?,
                _ => continue,
            };
            uploads.push((node, png));
        }
        imaging::decode_rgba(&self.run(graph, &uploads, ctl)?)
    }

    /// SeedVR2 enhancement to `width × height` (same aspect, larger, even). Keeps the alpha.
    pub fn upscale(&self, image: &RgbaImage, width: u32, height: u32, seed: u64, ctl: &JobControl) -> Result<RgbaImage> {
        self.upscale_with_files(image, width, height, seed, ctl, None)
    }

    pub(crate) fn upscale_with_files(
        &self,
        image: &RgbaImage,
        width: u32,
        height: u32,
        seed: u64,
        ctl: &JobControl,
        custom: Option<&workflows::Files>,
    ) -> Result<RgbaImage> {
        let (sw, sh) = image.dimensions();
        if !width.is_multiple_of(2) || !height.is_multiple_of(2) || width <= sw || height <= sh {
            bail!("Choose a larger, even size to enhance to.");
        }
        let ratio_err = (width as f64 / height as f64 - sw as f64 / sh as f64).abs() * height as f64;
        if ratio_err > 2.0 {
            bail!("Keep the image's proportions when enhancing.");
        }
        ctl.set_stage(Stage::Preparing);
        let info = self.inventory()?;
        let files = if let Some(files) = custom {
            let preset = catalog::preset(ModelId::SeedVr2, "fp16").context("SeedVR2 profile missing")?;
            for node in &preset.required_nodes {
                if !info.has_node(node) {
                    bail!("SeedVR2 needs the ComfyUI core node {node}");
                }
            }
            for (role, node, input) in [(Role::Unet, "UNETLoader", "unet_name"), (Role::Vae, "VAELoader", "vae_name")] {
                let name = files.get(&role).context("Custom SeedVR2 file missing")?;
                if !info.choices(node, input).contains(name) {
                    bail!("Custom SeedVR2 file is not installed: {name}");
                }
            }
            files.clone()
        } else {
            self.resolve(ModelId::SeedVr2, "fp16", &info)?.files
        };
        let resized = li_raster::resize_rgba8(image, width, height, li_raster::Filter::Lanczos3);
        let png = imaging::encode_png(&imaging::over_white(&resized))?;
        let built = workflows::seedvr2(&files, seed);
        let out = image::load_from_memory(&self.run(built.graph, &[(built.image_nodes[0].clone(), png)], ctl)?)?.to_rgba8();
        let out = if out.dimensions() != (width, height) { li_raster::resize_rgba8(&out, width, height, li_raster::Filter::Lanczos3) } else { out };
        // Restore transparency: blend by alpha⁴ so soft edges keep the source colour.
        Ok(RgbaImage::from_fn(width, height, |x, y| {
            let s = resized.get_pixel(x, y);
            let o = out.get_pixel(x, y);
            let a = (s[3] as f32 / 255.0).powi(4);
            let c = |k: usize| (o[k] as f32 * a + s[k] as f32 * (1.0 - a)).round() as u8;
            image::Rgba([c(0), c(1), c(2), s[3]])
        }))
    }
}

fn resize_to(img: &RgbaImage, w: u32, h: u32) -> RgbaImage {
    if img.dimensions() == (w, h) { img.clone() } else { li_raster::resize_rgba8(img, w, h, li_raster::Filter::Lanczos3) }
}

/// The family's prompt conventions: Pony's score tags and Illustrious' quality tags are added
/// when the prompt doesn't already start with them; its default negative fills an empty one.
fn set_qwen_sampler(built: &mut workflows::Built, sampler: Option<&(String, String)>) {
    if let Some((sampler, scheduler)) = sampler {
        built.graph["6"]["inputs"]["sampler_name"] = serde_json::json!(sampler);
        built.graph["6"]["inputs"]["scheduler"] = serde_json::json!(scheduler);
    }
}

pub fn prompt_rules(f: &Family, prompt: &str, negative: &str) -> (String, String) {
    let prefix = f.prompt.prefix.trim();
    let first = prefix.split(',').next().unwrap_or("").trim();
    let p = if prefix.is_empty() || (!first.is_empty() && prompt.contains(first)) {
        prompt.trim().to_owned()
    } else {
        format!("{} {}", f.prompt.prefix.trim_end(), prompt.trim())
    };
    let n = if negative.trim().is_empty() && f.capabilities.negative_prompt { f.prompt.negative.clone() } else { negative.to_owned() };
    (p, n)
}

/// Reference images as PNGs, sized for how the family takes them.
fn prepare_refs(f: &Family, refs: &[RgbaImage], w: u32, h: u32) -> Result<Vec<Vec<u8>>> {
    refs.iter()
        .map(|r| {
            let white = imaging::over_white(r);
            let img = match f.pipeline.reference {
                RefMethod::InitImage => imaging::over_white(&imaging::fit_cover(r, w, h)),
                RefMethod::IpAdapter | RefMethod::Redux => {
                    let k = (512.0 / white.width().max(white.height()) as f32).min(1.0);
                    li_raster::resize_rgba8(
                        &white,
                        ((white.width() as f32 * k) as u32).max(1),
                        ((white.height() as f32 * k) as u32).max(1),
                        li_raster::Filter::Lanczos3,
                    )
                }
                _ => {
                    // About one megapixel on a 32 px grid (0.7's rule for FLUX.2 Klein references).
                    let s = (1_048_576.0 / (white.width() as f64 * white.height() as f64)).sqrt();
                    let (sw, sh) = (((white.width() as f64 * s) as u32).max(32), ((white.height() as f64 * s) as u32).max(32));
                    let (rw, rh) = imaging::qwen_canvas_size(sw, sh);
                    li_raster::resize_rgba8(&white, rw, rh, li_raster::Filter::Lanczos3)
                }
            };
            imaging::encode_png(&img)
        })
        .collect()
}

#[cfg(test)]
#[test]
fn ds2_refinement_request_rejects_nonfinite_and_out_of_range_parameters() {
    let mut r = GenerateRequest::new(ModelId::Klein4B, "a test");
    assert!(r.validate().is_ok());
    for value in [-0.1, 1.1, f32::NAN, f32::INFINITY] {
        r.denoise = value;
        assert!(r.validate().is_err());
    }
    r.denoise = 0.35;
    r.refine = Some(RefineStep {
        model: ModelId::Klein4B,
        variant: String::new(),
        strength: 0.4,
        default_strength: false,
        steps: None,
        guidance: None,
        loras: vec![],
        scale: 1.0,
    });
    assert!(r.validate().is_ok());
    for value in [0.0, 1e6, f32::NAN, f32::INFINITY] {
        r.refine.as_mut().unwrap().scale = value;
        assert!(r.validate().is_err());
    }
    r.refine.as_mut().unwrap().scale = 4.0;
    for value in [-0.1, 2.0, f32::NAN, f32::INFINITY] {
        r.refine.as_mut().unwrap().strength = value;
        assert!(r.validate().is_err());
    }
    r.refine.as_mut().unwrap().strength = 1.0;
    assert!(r.validate().is_ok());
}

#[cfg(test)]
#[test]
fn final_review_qwen_remove_blends_the_boundary_and_preserves_unmasked_pixels() {
    let server = crate::mock::MockComfy::start().unwrap();
    server.delay_ms.store(1, std::sync::atomic::Ordering::SeqCst);
    let ai = Ai::new(server.host());
    let info = ai.inventory().unwrap();
    let src = RgbImage::from_pixel(64, 48, image::Rgb([105, 166, 205]));
    let mask = GrayImage::from_fn(64, 48, |x, y| Luma([if (16..48).contains(&x) && (12..36).contains(&y) { 255 } else { 0 }]));
    let out = ai.remove_qwen(&RgbFrame::from_rgb8(&src), &mask, "int8", 42, &info, &JobControl::new(), false).unwrap().to_rgb8();
    let boundary_jump = out.get_pixel(16, 20).0.into_iter().zip(src.get_pixel(15, 20).0).map(|(a, b)| a.abs_diff(b)).max().unwrap();
    assert!(boundary_jump < 12, "Qwen repair must meet the surrounding colour: jump {boundary_jump}");
    for (x, y, p) in out.enumerate_pixels() {
        if mask.get_pixel(x, y)[0] == 0 {
            assert_eq!(p, src.get_pixel(x, y));
        }
    }
    assert_eq!(out.dimensions(), src.dimensions());
}

/// CPU cutout on the real service path. No ComfyUI inventory or GPU is involved.
pub fn cpu_cutout(image: &RgbaImage, better: bool, dir: &std::path::Path, ctl: &JobControl) -> Result<GrayImage> {
    ctl.check()?;
    ctl.set_stage(Stage::Preparing);
    let start = std::time::Instant::now();
    let seg = li_seg::shared_tier(dir, better).context("Install the selected background-removal model in Local AI")?;
    ctl.check()?;
    let p = seg.predict(image.as_raw(), image.width() as usize, image.height() as usize)?.context("No subject found")?;
    ctl.check()?;
    let elapsed = start.elapsed().as_secs_f32();
    CUTOUT_SECONDS[usize::from(better)].store(elapsed.to_bits(), std::sync::atomic::Ordering::Relaxed);
    Ok(GrayImage::from_fn(image.width(), image.height(), |x, y| Luma([(p[(y * image.width() + x) as usize] * 255.0).round() as u8])))
}

static CUTOUT_SECONDS: [std::sync::atomic::AtomicU32; 2] = [std::sync::atomic::AtomicU32::new(0), std::sync::atomic::AtomicU32::new(0)];
pub fn cutout_seconds(better: bool) -> Option<f32> {
    let v = f32::from_bits(CUTOUT_SECONDS[usize::from(better)].load(std::sync::atomic::Ordering::Relaxed));
    (v > 0.0).then_some(v)
}

#[cfg(test)]
mod cutout_tier_tests {
    use super::*;
    #[test]
    fn bg_cutout_cancelled_tiers_never_contact_comfy_or_load_models() {
        let ai = Ai::new("127.0.0.1:1");
        let ctl = JobControl::new();
        ctl.cancel();
        for variant in ["fast", "quick", "better", "int8", "bf16"] {
            let error = ai.cutout(&RgbaImage::new(4, 4), variant, "", 1, &ctl).unwrap_err();
            assert!(error.is::<lightcraft_sysmem::jobs::Cancelled>());
        }
    }
    #[test]
    #[ignore = "opt-in real NVIDIA GPU and idle ComfyUI, BGREMOVE_PHOTO"]
    fn bg_cutout_real_qwen_gpu() {
        let ai = crate::service();
        let stats = ai.client.system_stats().expect("live ComfyUI");
        assert!(stats["devices"].as_array().unwrap().iter().any(|d| d["type"] == "cuda" && d["name"].as_str().is_some_and(|n| n.contains("NVIDIA"))));
        let queue = ai.client.queue_state().unwrap();
        assert!(queue["queue_running"].as_array().unwrap().is_empty() && queue["queue_pending"].as_array().unwrap().is_empty(), "shared queue must be idle");
        let image = image::open(std::env::var_os("BGREMOVE_PHOTO").expect("CC0 photo path")).unwrap().to_rgba8();
        // Use a small, grid-aligned photograph at the model's usable spatial scale.
        // A 128-pixel probe produced an almost-zero matte (alpha 0..3) on the real GPU.
        let image = li_raster::resize_rgba8(&image, 512, 320, li_raster::Filter::Lanczos3);
        let alpha = ai.cutout(&image, "int8", "", 1, &JobControl::new()).unwrap();
        assert_eq!(alpha.dimensions(), image.dimensions());
        assert!(alpha.pixels().any(|p| p[0] < 128) && alpha.pixels().any(|p| p[0] > 128));
    }
    #[test]
    fn bg_cutout_cpu_missing_model_is_local_error() {
        let dir = std::path::Path::new("/not-a-local-image-model-directory");
        for better in [false, true] {
            let error = cpu_cutout(&RgbaImage::new(4, 4), better, dir, &JobControl::new()).unwrap_err();
            assert!(error.to_string().contains("Install the selected"));
        }
    }
}

#[cfg(test)]
#[test]
fn lama_local_validation_and_cancellation_never_need_comfy() {
    let ai = Ai::new("http://127.0.0.1:1");
    let engine = RemoveEngine::Lama { models_dir: std::path::PathBuf::from("/nonexistent-lama") };
    let rgb = RgbImage::new(8, 8);
    let mask = GrayImage::from_pixel(8, 8, Luma([255]));
    let ctl = JobControl::new();
    assert!(ai.remove_objects(&rgb, &mask, &engine, 0, &ctl).unwrap_err().to_string().contains("Download AI Remove"));
    ctl.cancel();
    assert!(ai.remove_patch(&rgb, &mask, &engine, 0, &ctl).unwrap_err().is::<lightcraft_sysmem::jobs::Cancelled>());
}
