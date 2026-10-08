//! The AI operations the editor runs on worker threads. Each one checks the model against the
//! live ComfyUI inventory, prepares pixels, runs the graph and post-processes the result.

use anyhow::{Context, Result, bail};
use image::{GrayImage, Luma, RgbImage, RgbaImage, imageops};

use crate::catalog::{self, Availability, ModelId};
use crate::comfy::{ComfyClient, JobControl, ObjectInfo, Stage};
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GenerateMode {
    /// Text (and optional references) to a new image.
    Create,
    /// Instruction edit of reference 1, keeping its size.
    Edit,
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
    /// Variation strength for Z-Image starting images (0.05..1).
    pub denoise: f32,
    pub references: Vec<RgbaImage>,
    pub loras: Vec<LoraUse>,
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
            references: Vec::new(),
            loras: Vec::new(),
        }
    }

    /// Plain-language problems with the request, before anything is sent.
    pub fn validate(&self) -> Result<()> {
        let info = self.model.info();
        if self.prompt.trim().is_empty() {
            bail!("Describe the image first.");
        }
        if self.prompt.chars().count() > 4000 {
            bail!("The prompt is longer than 4000 characters.");
        }
        if !info.text_to_image {
            bail!("{} does not create images from text.", info.label);
        }
        if self.references.len() > info.max_references {
            bail!("{} takes at most {} image inputs.", info.label, info.max_references);
        }
        if self.mode == GenerateMode::Edit && self.references.is_empty() {
            bail!("Editing needs an image to edit.");
        }
        if self.transparent && !info.transparent {
            bail!("{} cannot make transparent images; choose Qwen Image 2.1.", info.label);
        }
        if (self.steps as f32) < info.steps.min || (self.steps as f32) > info.steps.max {
            bail!("{} uses {}–{} steps.", info.label, info.steps.min, info.steps.max);
        }
        if self.loras.len() > 3 {
            bail!("Choose at most three styles (LoRAs).");
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
        self.client.object_info().map_err(|e| anyhow::anyhow!("Cannot reach ComfyUI at {}. Start the AI engine in Preferences › Local AI. ({e:#})", self.client.host()))
    }

    fn validate_lora_names(info: &ObjectInfo, loras: &[LoraUse]) -> Result<Vec<LoraUse>> {
        if loras.is_empty() {
            return Ok(Vec::new());
        }
        if !info.has_node("LoraLoaderModelOnly") {
            bail!("Update ComfyUI: styles (LoRAs) need the LoraLoaderModelOnly node.");
        }
        let choices = info.choices("LoraLoaderModelOnly", "lora_name");
        loras
            .iter()
            .map(|l| {
                if !(-2.0..=2.0).contains(&l.strength) {
                    bail!("Style strength must be between −2 and 2.");
                }
                let found = choices.iter().find(|c| c.replace('\\', "/") == l.name.replace('\\', "/"));
                found.map(|n| LoraUse { name: n.clone(), strength: l.strength }).with_context(|| format!("The style {} is not installed in ComfyUI. Refresh, or finish its download.", l.name))
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
        let loras = Self::validate_lora_names(info, loras)?;
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

    /// Text-to-image, reference generation or instruction edit, for any generator.
    pub fn generate(&self, req: &GenerateRequest, ctl: &JobControl) -> Result<RgbaImage> {
        req.validate()?;
        ctl.set_stage(Stage::Preparing);
        let info = self.inventory()?;
        let a = self.resolve(req.model, &req.variant, &info)?;
        let loras = if req.model.info().lora { Self::validate_lora_names(&info, &req.loras)? } else { Vec::new() };
        let grid = if req.model == ModelId::Qwen && !req.references.is_empty() { 32 } else { 16 };
        let (w, h) = imaging::snap_size(req.width, req.height, grid);
        match req.model {
            ModelId::Qwen => {
                if req.mode == GenerateMode::Edit {
                    let out = self.qwen_edit(&req.references, &req.prompt, &req.negative, &req.variant, req.seed, req.steps, req.guidance, &loras, &info, ctl)?;
                    return Ok(imaging::over_white(&out));
                }
                let mut prompt = req.prompt.trim().to_owned();
                prompt.push_str(if req.transparent {
                    "\nCreate the subject on a fully transparent background with real RGBA alpha. Do not draw a checkerboard or an opaque backdrop. Preserve fine subject edges."
                } else {
                    "\nReturn a fully opaque image with a complete background."
                });
                let pngs = req
                    .references
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
                        loras: &loras,
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
            ModelId::ZImageTurbo => {
                let init = req.references.first().map(|r| imaging::encode_png(&imaging::over_white(&imaging::fit_cover(r, w, h)))).transpose()?;
                let built = workflows::z_image(
                    &a.files,
                    &workflows::ZImageParams { prompt: &req.prompt, width: w, height: h, seed: req.seed, steps: req.steps, denoise: req.denoise.clamp(0.05, 1.0), init_image: init.is_some(), loras: &loras },
                );
                let images: Vec<_> = built.image_nodes.iter().cloned().zip(init).collect();
                Ok(imaging::over_white(&imaging::decode_rgba(&self.client.run(built.graph, &images, ctl)?)?))
            }
            ModelId::Klein4B | ModelId::Klein9B => {
                let pngs = req
                    .references
                    .iter()
                    .map(|r| {
                        let white = imaging::over_white(r);
                        let s = (1_048_576.0 / (white.width() as f64 * white.height() as f64)).sqrt();
                        let (sw, sh) = (((white.width() as f64 * s) as u32).max(32), ((white.height() as f64 * s) as u32).max(32));
                        let (rw, rh) = imaging::qwen_canvas_size(sw, sh);
                        imaging::encode_png(&imageops::resize(&white, rw, rh, imageops::FilterType::Lanczos3))
                    })
                    .collect::<Result<Vec<_>>>()?;
                let built = workflows::flux2_klein(
                    &a.files,
                    &workflows::Flux2Params { prompt: &req.prompt, width: w, height: h, seed: req.seed, steps: req.steps, references: pngs.len(), loras: &loras },
                );
                let images: Vec<_> = built.image_nodes.iter().cloned().zip(pngs).collect();
                Ok(imaging::over_white(&imaging::decode_rgba(&self.client.run(built.graph, &images, ctl)?)?))
            }
            ModelId::Ernie => {
                let built = workflows::ernie(
                    &a.files,
                    &workflows::ErnieParams { prompt: &req.prompt, negative: &req.negative, width: w, height: h, seed: req.seed, steps: req.steps, cfg: req.guidance },
                );
                Ok(imaging::over_white(&imaging::decode_rgba(&self.client.run(built.graph, &[], ctl)?)?))
            }
            ModelId::SeedVr2 | ModelId::KleinRemove => bail!("{} does not generate images from a prompt.", req.model.info().label),
        }
    }

    /// SeedVR2 enhancement to `width × height` (same aspect, larger, even). Keeps the alpha.
    pub fn upscale(&self, image: &RgbaImage, width: u32, height: u32, seed: u64, ctl: &JobControl) -> Result<RgbaImage> {
        let (sw, sh) = image.dimensions();
        if width % 2 != 0 || height % 2 != 0 || width <= sw || height <= sh {
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
