//! The AI operations the editor runs on worker threads. Each one checks the model against the
//! live ComfyUI inventory, prepares pixels, runs the graph and post-processes the result.

use anyhow::{Context, Result, bail};
use image::{GrayImage, Luma, RgbImage, RgbaImage, imageops};

use crate::builders::{self, Params, Slot, Task};
use crate::catalog::{self, Availability, ModelId};
use crate::comfy::{ComfyClient, JobControl, ObjectInfo, Stage};
use crate::family::{Encode, Family, InpaintMethod, RefMethod};
use crate::imaging::{self, Rect};
use crate::workflows::{self, LoraUse};

/// A random 48-bit seed (what 0.7 used, so seeds stay interchangeable).
pub fn new_seed() -> u64 {
    rand::random::<u64>() & ((1 << 48) - 1)
}

/// Which model repairs a selection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RemoveEngine {
    /// FLUX.2 Klein + fal's object-removal LoRA (16 GB).
    Klein,
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
#[derive(Clone, Debug, PartialEq)]
pub struct RefineStep {
    pub model: ModelId,
    pub variant: String,
    /// How much the refine pass may change (0.2–0.6 is typical).
    pub strength: f32,
    pub steps: Option<u32>,
    pub guidance: Option<f32>,
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
    pub transparent: bool,
    /// Strength for Refine, Inpaint, Upscale-refine and starting images (0.05..1).
    pub denoise: f32,
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
            transparent: false,
            denoise: 0.6,
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
        if (self.steps as f32) < info.steps.min || (self.steps as f32) > info.steps.max {
            bail!("{} uses {}–{} steps.", info.label, info.steps.min, info.steps.max);
        }
        if self.loras.len() > f.lora.max.max(3) as usize {
            bail!("Choose at most {} styles (LoRAs).", f.lora.max.max(3));
        }
        if let Some(r) = &self.refine {
            let ri = r.model.try_info().with_context(|| format!("The refine model {} is not available.", r.model.key()))?;
            if ri.resolved.kind != crate::family::FamilyKind::Image {
                bail!("{} cannot refine images.", ri.label);
            }
        }
        Ok(())
    }
}

/// The AI service: a ComfyUI client plus a cached inventory.
#[derive(Clone, Debug)]
pub struct Ai {
    pub client: ComfyClient,
}

impl Ai {
    pub fn new(host: impl Into<String>) -> Self {
        Self { client: ComfyClient::new(host) }
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

    /// Repairs the selected area of `image` (the visible composite). Returns the repaired image;
    /// pixels outside the (cleaned) selection are unchanged.
    pub fn remove_objects(&self, image: &RgbImage, mask: &GrayImage, engine: &RemoveEngine, seed: u64, ctl: &JobControl) -> Result<RgbImage> {
        if image.dimensions() != mask.dimensions() {
            bail!("The selection does not match the image size.");
        }
        let mask = imaging::clean_selection_mask(mask);
        if imaging::bbox(&mask, 0).is_none() {
            bail!("Paint over what you want to remove first.");
        }
        ctl.set_stage(Stage::Preparing);
        let info = self.inventory()?;
        match engine {
            RemoveEngine::Klein => self.remove_klein(image, &mask, seed, &info, ctl),
            RemoveEngine::Qwen { variant } => self.remove_qwen(image, &mask, variant, seed, &info, ctl),
        }
    }

    fn remove_klein(&self, image: &RgbImage, mask: &GrayImage, seed: u64, info: &ObjectInfo, ctl: &JobControl) -> Result<RgbImage> {
        let a = self.resolve(ModelId::KleinRemove, "bf16", info)?;
        let (w, h) = image.dimensions();
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
            let crop = imageops::crop_imm(&current, b.x0, b.y0, b.width(), b.height()).to_image();
            let in_crop = Rect { x0: region.x0 - b.x0, y0: region.y0 - b.y0, x1: region.x1 - b.x0, y1: region.y1 - b.y0 };
            let highlighted = imaging::klein_highlight(&crop, in_crop);
            let built = workflows::klein_remove(&a.files, highlighted.width(), highlighted.height(), seed.wrapping_add(i as u64));
            let png = imaging::encode_png(&highlighted)?;
            let bytes = self.client.run(built.graph, &[(built.image_nodes[0].clone(), png)], ctl)?;
            let out = image::load_from_memory(&bytes)?.to_rgb8();
            if out.dimensions() != highlighted.dimensions() {
                bail!("The removal model returned an unexpected size.");
            }
            let generated = imageops::resize(&out, b.width(), b.height(), imageops::FilterType::Lanczos3);
            let crop_mask = imageops::crop_imm(&region_mask, b.x0, b.y0, b.width(), b.height()).to_image();
            let blended = imaging::poisson_blend(&crop, &generated, &crop_mask);
            for (x, y, p) in blended.enumerate_pixels() {
                if crop_mask.get_pixel(x, y)[0] > 0 {
                    current.put_pixel(b.x0 + x, b.y0 + y, *p);
                }
            }
        }
        Ok(current)
    }

    fn remove_qwen(&self, image: &RgbImage, mask: &GrayImage, variant: &str, seed: u64, info: &ObjectInfo, ctl: &JobControl) -> Result<RgbImage> {
        let instruction = "<image1> is the photograph to edit. <image2> is a black and white selection mask: white marks the unwanted object or area to remove and black marks the area to keep. Remove the selected object completely and reconstruct a natural empty continuation of the surrounding background in its place. Match the surrounding texture, perspective, lighting and color. Do not insert any new objects, people, text or symbols. Do not draw the mask in the result. Keep the original composition and all other areas unchanged. ";
        let rgba = image::DynamicImage::ImageRgb8(image.clone()).to_rgba8();
        let mask_rgba = image::DynamicImage::ImageLuma8(mask.clone()).to_rgba8();
        let edited = self.qwen_edit(&[rgba, mask_rgba], instruction, "", variant, seed, 25, 1.0, &[], info, ctl)?;
        let edited = image::DynamicImage::ImageRgba8(imaging::over_white(&edited)).to_rgb8();
        Ok(RgbImage::from_fn(image.width(), image.height(), |x, y| {
            let m = mask.get_pixel(x, y)[0] as u32;
            let (a, b) = (image.get_pixel(x, y), edited.get_pixel(x, y));
            image::Rgb([0, 1, 2].map(|k| ((a[k] as u32 * (255 - m) + b[k] as u32 * m + 127) / 255) as u8))
        }))
    }

    /// Generative fill: changes only the selected area according to `prompt` (Qwen edit with the
    /// selection as a second reference, composited through the selection).
    pub fn fill(&self, image: &RgbImage, mask: &GrayImage, prompt: &str, variant: &str, seed: u64, ctl: &JobControl) -> Result<RgbImage> {
        if image.dimensions() != mask.dimensions() {
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
        let rgba = image::DynamicImage::ImageRgb8(image.clone()).to_rgba8();
        let mask_rgba = image::DynamicImage::ImageLuma8(mask.clone()).to_rgba8();
        let edited = self.qwen_edit(&[rgba, mask_rgba], &instruction, "", variant, seed, 25, 1.0, &[], &info, ctl)?;
        let edited = image::DynamicImage::ImageRgba8(imaging::over_white(&edited)).to_rgb8();
        Ok(RgbImage::from_fn(image.width(), image.height(), |x, y| if mask.get_pixel(x, y)[0] > 0 { *edited.get_pixel(x, y) } else { *image.get_pixel(x, y) }))
    }

    /// Qwen "edit": the output follows reference 1 and is resized back to its size.
    #[allow(clippy::too_many_arguments)]
    fn qwen_edit(
        &self,
        refs: &[RgbaImage],
        prompt: &str,
        negative: &str,
        variant: &str,
        seed: u64,
        steps: u32,
        cfg: f32,
        loras: &[LoraUse],
        info: &ObjectInfo,
        ctl: &JobControl,
    ) -> Result<RgbaImage> {
        let a = self.resolve(ModelId::Qwen, variant, info)?;
        let loras = Self::validate_lora_names(info, loras, "LoraLoaderModelOnly")?;
        let (ow, oh) = refs[0].dimensions();
        let pngs = refs
            .iter()
            .map(|r| {
                let (w, h) = imaging::qwen_canvas_size(r.width(), r.height());
                imaging::encode_png(&imageops::resize(r, w, h, imageops::FilterType::Lanczos3))
            })
            .collect::<Result<Vec<_>>>()?;
        let built = workflows::qwen(
            &a.files,
            &workflows::QwenParams {
                prompt,
                negative,
                width: 0,
                height: 0,
                seed,
                steps,
                cfg,
                references: refs.len(),
                use_cache: info.has_node("QwenImage21Cache"),
                loras: &loras,
            },
        );
        let images: Vec<(String, Vec<u8>)> = built.image_nodes.iter().cloned().zip(pngs).collect();
        let out = imaging::decode_rgba(&self.client.run(built.graph, &images, ctl)?)?;
        Ok(if out.dimensions() != (ow, oh) { imageops::resize(&out, ow, oh, imageops::FilterType::Lanczos3) } else { out })
    }

    /// Background removal with Qwen: returns an alpha matte the size of `image`. Callers apply it
    /// as a layer mask so the subject keeps its original pixels.
    pub fn cutout(&self, image: &RgbaImage, variant: &str, hint: &str, seed: u64, ctl: &JobControl) -> Result<GrayImage> {
        ctl.set_stage(Stage::Preparing);
        let info = self.inventory()?;
        let prompt = format!(
            "Remove the background from <image1> and preserve the subject exactly. Return an RGBA PNG with a transparent background and a real alpha channel. Keep the original subject position, scale, details and colors. {}",
            hint.trim()
        );
        let out = self.qwen_edit(std::slice::from_ref(image), &prompt, "", variant, seed, 25, 1.0, &[], &info, ctl)?;
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
        self.generate(&req, ctl)
    }

    /// Generates with any model: Create, Edit, Refine, Inpaint or Upscale-refine, then the
    /// optional Draft → Refine pass. Returns the image at the request's (or the source's) size.
    pub fn generate(&self, req: &GenerateRequest, ctl: &JobControl) -> Result<RgbaImage> {
        req.validate()?;
        ctl.set_stage(Stage::Preparing);
        let info = self.inventory()?;
        let out = self.generate_once(req, &info, ctl)?;
        match &req.refine {
            Some(step) => {
                ctl.set_message("Refining the draft");
                self.refine_pass(&out, req, step, &info, ctl)
            }
            None => Ok(out),
        }
    }

    fn generate_once(&self, req: &GenerateRequest, info: &ObjectInfo, ctl: &JobControl) -> Result<RgbaImage> {
        let m = req.model.info();
        let f = &m.resolved;
        let a = self.resolve(req.model, &req.variant, info)?;
        let loras = if m.lora { Self::validate_lora_names(info, &req.loras, &f.lora.loader)? } else { Vec::new() };
        let (prompt, negative) = prompt_rules(f, &req.prompt, &req.negative);
        let (source, refs) = req.source_and_refs();
        if f.pipeline.encode == Encode::QwenImage21 && matches!(req.mode, GenerateMode::Create | GenerateMode::Edit) {
            return self.qwen21_generate(req, &a, &loras, info, ctl);
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
        };
        match req.mode {
            GenerateMode::Create => {
                let (w, h) = imaging::snap_size(req.width, req.height, f.sizes.multiple.max(8));
                let refs = prepare_refs(f, refs, w, h)?;
                let p = Params { width: w, height: h, references: refs.len(), ..base };
                let out = self.run_built(builders::build(f, &a.files, &p), None, None, &refs, ctl)?;
                Ok(imaging::over_white(&out))
            }
            GenerateMode::Edit if f.capabilities.edit => {
                let src = source.context("Open an image to edit.")?;
                let (w, h) = crate::inpaint::work_size(src.width(), src.height(), f.sizes.native, f.sizes.max, f.sizes.multiple);
                let png = imaging::encode_png(&imageops::resize(&imaging::over_white(src), w, h, imageops::FilterType::Lanczos3))?;
                let refs = prepare_refs(f, refs, w, h)?;
                let p = Params { task: Task::Edit, width: w, height: h, references: refs.len(), ..base };
                let out = self.run_built(builders::build(f, &a.files, &p), Some(png), None, &refs, ctl)?;
                Ok(resize_to(&imaging::over_white(&out), src.width(), src.height()))
            }
            GenerateMode::Edit | GenerateMode::Refine => {
                let src = source.context("Open an image first.")?;
                let denoise = if req.mode == GenerateMode::Edit { req.denoise.max(0.3) } else { req.denoise };
                self.refine_image(
                    src,
                    f,
                    &a,
                    Params { task: Task::Refine { denoise: denoise.clamp(0.05, 1.0) }, ..base },
                    prepare_refs(f, refs, 1024, 1024)?,
                    ctl,
                )
            }
            GenerateMode::Inpaint => {
                let src = source.context("Open an image first.")?;
                let mask = req.mask.as_ref().context("Make a selection to fill.")?;
                self.inpaint_image(src, mask, f, &a, base, req.denoise, refs, ctl)
            }
            GenerateMode::UpscaleRefine => {
                let src = source.context("Open an image to enlarge.")?;
                self.upscale_refine(src, req.scale, f, &a, Params { task: Task::Refine { denoise: req.denoise.clamp(0.05, 1.0) }, ..base }, ctl)
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
        imaging::decode_rgba(&self.client.run(built.graph, &images, ctl)?)
    }

    /// img2img of the whole image at the family's working size; the result keeps `src`'s size.
    fn refine_image(&self, src: &RgbaImage, f: &Family, a: &Availability, p: Params, refs: Vec<Vec<u8>>, ctl: &JobControl) -> Result<RgbaImage> {
        let (w, h) = crate::inpaint::work_size(src.width(), src.height(), f.sizes.native, f.sizes.max, f.sizes.multiple);
        let png = imaging::encode_png(&imageops::resize(&imaging::over_white(src), w, h, imageops::FilterType::Lanczos3))?;
        let p = Params { width: w, height: h, references: refs.len(), ..p };
        let out = self.run_built(builders::build(f, &a.files, &p), Some(png), None, &refs, ctl)?;
        Ok(resize_to(&imaging::over_white(&out), src.width(), src.height()))
    }

    /// Inpaints the masked area: context crop, noise mask, pre-fill (or green fill for instruction
    /// models), then composites the result back through a feathered mask.
    #[allow(clippy::too_many_arguments)]
    fn inpaint_image(
        &self,
        src: &RgbaImage,
        mask: &GrayImage,
        f: &Family,
        a: &Availability,
        base: Params,
        denoise: f32,
        refs: &[RgbaImage],
        ctl: &JobControl,
    ) -> Result<RgbaImage> {
        use crate::inpaint;
        let plan = inpaint::plan(mask).context("Make a selection to fill.")?;
        let rgb = image::DynamicImage::ImageRgba8(imaging::over_white(src)).to_rgb8();
        let crop = imageops::crop_imm(&rgb, plan.context.x0, plan.context.y0, plan.context.width(), plan.context.height()).to_image();
        let nmask = inpaint::noise_mask(mask, &plan);
        let (w, h) = inpaint::work_size(crop.width(), crop.height(), f.sizes.native, f.sizes.max, f.sizes.multiple);
        let scaled = |img: &RgbImage| imageops::resize(img, w, h, imageops::FilterType::Lanczos3);
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
                (Task::Inpaint { denoise: denoise.clamp(0.05, 1.0) }, filled, Some(nmask.clone()))
            }
        };
        let source_png = imaging::encode_png(&scaled(&source_img))?;
        let mask_png = mask_png.map(|m| imaging::encode_png(&imageops::resize(&m, w, h, imageops::FilterType::Triangle))).transpose()?;
        let p = Params { task, prompt: &prompt_owned, width: w, height: h, references: refs_png.len(), ..base };
        ctl.set_message("Filling the selection");
        let out = self.run_built(builders::build(f, &a.files, &p), Some(source_png), mask_png, &refs_png, ctl)?;
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
    fn upscale_refine(&self, src: &RgbaImage, scale: f32, f: &Family, a: &Availability, p: Params, ctl: &JobControl) -> Result<RgbaImage> {
        let scale = scale.clamp(1.0, 4.0);
        let (tw, th) = (((src.width() as f32 * scale) as u32).min(8192), ((src.height() as f32 * scale) as u32).min(8192));
        let big = imageops::resize(&imaging::over_white(src), tw, th, imageops::FilterType::Lanczos3);
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
                let png = imaging::encode_png(&imageops::resize(&crop, ww, wh, imageops::FilterType::Lanczos3))?;
                let tp = Params { prompt, width: ww, height: wh, ..p.clone() };
                let out = self.run_built(builders::build(f, &a.files, &tp), Some(png), None, &[], ctl)?;
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
        let alpha = imageops::resize(src, tw, th, imageops::FilterType::Lanczos3);
        Ok(RgbaImage::from_fn(tw, th, |x, y| {
            let a = acc[(y * tw + x) as usize];
            let d = a[3].max(1e-6);
            image::Rgba([(a[0] / d).round() as u8, (a[1] / d).round() as u8, (a[2] / d).round() as u8, alpha.get_pixel(x, y)[3]])
        }))
    }

    /// Draft → Refine: the refine model resamples the draft (enlarged by `step.scale`).
    fn refine_pass(&self, draft: &RgbaImage, req: &GenerateRequest, step: &RefineStep, info: &ObjectInfo, ctl: &JobControl) -> Result<RgbaImage> {
        let m = step.model.info();
        let f = &m.resolved;
        let a = self.resolve(step.model, &step.variant, info)?;
        let (prompt, negative) = prompt_rules(f, &req.prompt, &req.negative);
        let src = if step.scale > 1.01 {
            let (w, h) = ((draft.width() as f32 * step.scale) as u32, (draft.height() as f32 * step.scale) as u32);
            imageops::resize(draft, w, h, imageops::FilterType::Lanczos3)
        } else {
            draft.clone()
        };
        let p = Params {
            task: Task::Refine { denoise: step.strength.clamp(0.05, 1.0) },
            prompt: &prompt,
            negative: &negative,
            width: src.width(),
            height: src.height(),
            seed: req.seed.wrapping_add(1),
            steps: step.steps.unwrap_or(m.steps.default as u32).clamp(m.steps.min as u32, m.steps.max as u32),
            cfg: step.guidance.unwrap_or(m.guidance.default),
            references: 0,
            loras: &[],
            batch: 1,
            sampler: None,
        };
        if f.pipeline.encode == Encode::QwenImage21 {
            // Qwen Image 2.1 refines as an instruction edit of the draft.
            let out = self.qwen_edit(std::slice::from_ref(&src), &prompt, &negative, &step.variant, p.seed, p.steps, p.cfg, &[], info, ctl)?;
            return Ok(imaging::over_white(&out));
        }
        let (w, h) = (src.width().min(f.sizes.max.max(1024) * 2), src.height().min(f.sizes.max.max(1024) * 2));
        if src.width() * src.height() > f.sizes.max * f.sizes.max {
            return self.upscale_refine(&resize_to(&src, w, h), 1.0, f, &a, p, ctl);
        }
        self.refine_image(&src, f, &a, p, Vec::new(), ctl)
    }

    /// Qwen Image 2.1 create / edit, as in 0.7 (transparent output, reference canvas rules).
    fn qwen21_generate(&self, req: &GenerateRequest, a: &Availability, loras: &[LoraUse], info: &ObjectInfo, ctl: &JobControl) -> Result<RgbaImage> {
        let (source, refs) = req.source_and_refs();
        if req.mode == GenerateMode::Edit {
            let mut all = vec![source.context("Open an image to edit.")?.clone()];
            all.extend(refs.iter().cloned());
            let out = self.qwen_edit(&all, &req.prompt, &req.negative, &req.variant, req.seed, req.steps, req.guidance, loras, info, ctl)?;
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
                    imaging::encode_png(&imageops::resize(r, rw, rh, imageops::FilterType::Lanczos3))
                }
            })
            .collect::<Result<Vec<_>>>()?;
        let built = workflows::qwen(
            &a.files,
            &workflows::QwenParams {
                prompt: &prompt,
                negative: &req.negative,
                width: w,
                height: h,
                seed: req.seed,
                steps: req.steps,
                cfg: req.guidance,
                references: pngs.len(),
                use_cache: info.has_node("QwenImage21Cache"),
                loras,
            },
        );
        let images: Vec<_> = built.image_nodes.iter().cloned().zip(pngs).collect();
        let out = imaging::decode_rgba(&self.client.run(built.graph, &images, ctl)?)?;
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
        imaging::decode_rgba(&self.client.run(graph, &uploads, ctl)?)
    }

    /// SeedVR2 enhancement to `width × height` (same aspect, larger, even). Keeps the alpha.
    pub fn upscale(&self, image: &RgbaImage, width: u32, height: u32, seed: u64, ctl: &JobControl) -> Result<RgbaImage> {
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
        let a = self.resolve(ModelId::SeedVr2, "fp16", &info)?;
        let resized = imageops::resize(image, width, height, imageops::FilterType::Lanczos3);
        let png = imaging::encode_png(&imaging::over_white(&resized))?;
        let built = workflows::seedvr2(&a.files, seed);
        let out = image::load_from_memory(&self.client.run(built.graph, &[(built.image_nodes[0].clone(), png)], ctl)?)?.to_rgba8();
        let out = if out.dimensions() != (width, height) { imageops::resize(&out, width, height, imageops::FilterType::Lanczos3) } else { out };
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
    if img.dimensions() == (w, h) { img.clone() } else { imageops::resize(img, w, h, imageops::FilterType::Lanczos3) }
}

/// The family's prompt conventions: Pony's score tags and Illustrious' quality tags are added
/// when the prompt doesn't already start with them; its default negative fills an empty one.
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
                    imageops::resize(
                        &white,
                        ((white.width() as f32 * k) as u32).max(1),
                        ((white.height() as f32 * k) as u32).max(1),
                        imageops::FilterType::Lanczos3,
                    )
                }
                _ => {
                    // About one megapixel on a 32 px grid (0.7's rule for FLUX.2 Klein references).
                    let s = (1_048_576.0 / (white.width() as f64 * white.height() as f64)).sqrt();
                    let (sw, sh) = (((white.width() as f64 * s) as u32).max(32), ((white.height() as f64 * s) as u32).max(32));
                    let (rw, rh) = imaging::qwen_canvas_size(sw, sh);
                    imageops::resize(&white, rw, rh, imageops::FilterType::Lanczos3)
                }
            };
            imaging::encode_png(&img)
        })
        .collect()
}
