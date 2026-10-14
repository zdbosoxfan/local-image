//! Photo Merge (HDR, Panorama, HDR Panorama) in the library.
//!
//! A merge is planned from the session ([`crate::Session::plan_merge`] → [`MergeJob`], which is
//! `Send` and self-contained), run anywhere (a worker thread in the app, inline for the CLI/MCP:
//! [`MergeJob::run`], with progress and cancellation), and finished on the session
//! ([`crate::Session::finish_merge`]): the DNG is written next to the first source photo
//! (`<name>-HDR.dng`, `-Pano.dng`, `-HDR-Pano.dng`), imported, selected, and given Auto Settings /
//! the auto-crop rectangle when asked. A *preview* run works on ≤ 1024 px frames and returns a
//! rendered image (with the deghost overlay when asked) instead of a file.

use std::sync::Arc;

use lightcraft_catalog::{PhotoId, Source};
use lightcraft_develop::DevelopSettings;
use lightcraft_merge::{Deghost, Frame, HdrOptions, PanoOptions, Projection};
use lightcraft_pipeline::{Quality, RenderRequest, SourceInfo};
use lightcraft_raster::{Rgb32f, Rgba8};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{EngineError, Result, Session};

/// Long edge of the frames of a preview merge.
pub const PREVIEW_EDGE: usize = 1024;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum MergeKind {
    /// Sony complete 4/16-source capture, or one composed ARQ.
    PixelShift,
    Hdr {
        hdr: HdrOptions,
    },
    Panorama {
        pano: PanoOptions,
    },
    /// Merge each bracket (`bracket` consecutive photos; 0 = detect from EXIF), then stitch.
    HdrPanorama {
        hdr: HdrOptions,
        pano: PanoOptions,
        bracket: usize,
    },
}

impl MergeKind {
    fn suffix(&self) -> &'static str {
        match self {
            MergeKind::PixelShift => "PixelShift",
            MergeKind::Hdr { .. } => "HDR",
            MergeKind::Panorama { .. } => "Pano",
            MergeKind::HdrPanorama { .. } => "HDR-Pano",
        }
    }
    fn min_photos(&self) -> usize {
        match self {
            MergeKind::PixelShift => 1,
            MergeKind::HdrPanorama { .. } => 4,
            _ => 2,
        }
    }
}

/// Options that apply after the merge.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MergeFinish {
    /// Run Auto Settings on the result.
    pub auto_settings: bool,
    /// Stack the result on top of its sources (collapsed); the result's caption also lists them.
    pub stack: bool,
    /// Preview only: tint deghosted areas red.
    pub show_overlay: bool,
}

/// Reads a source photo's bytes (by catalog path).
pub type ByteReader = Arc<dyn Fn(&str) -> std::result::Result<Vec<u8>, String> + Send + Sync>;

/// A planned merge, detached from the session.
#[derive(Clone)]
pub struct MergeJob {
    pub kind: MergeKind,
    pub finish: MergeFinish,
    pub sources: Vec<(PhotoId, String)>,
    /// `Some(edge)`: preview at that frame size; `None`: the full-resolution merge.
    pub preview: Option<usize>,
    pub read: ByteReader,
}

/// What a merge produced.
pub struct MergeOutput {
    /// The DNG (also for previews, at preview size).
    pub dng: Vec<u8>,
    /// Rendered preview (previews only).
    pub preview: Option<Rgba8>,
    /// Auto-crop rectangle (normalised x, y, w, h) for panoramas.
    pub crop: Option<[f64; 4]>,
    /// Statistics for the caller (exposures, alignment, projection, …).
    pub info: Value,
}

pub type ProgressFn<'a> = dyn Fn(f32, &str) -> bool + Sync + 'a;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

impl MergeJob {
    fn load(&self, orient: bool, progress: &ProgressFn, span: (f32, f32)) -> std::result::Result<Vec<Frame>, String> {
        use rayon::prelude::*;
        let done = std::sync::atomic::AtomicUsize::new(0);
        let n = self.sources.len();
        let frames: std::result::Result<Vec<Frame>, String> = self
            .sources
            .par_iter()
            .map(|(_, path)| {
                let bytes = (self.read)(path)?;
                let f = lightcraft_merge::load_frame(&bytes, self.preview, orient).map_err(|e| format!("{path}: {e}"));
                let k = done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                progress(span.0 + (span.1 - span.0) * k as f32 / n as f32, "Reading photos");
                f
            })
            .collect();
        frames
    }

    /// Run the merge. `progress(fraction, stage)` returns false to cancel.
    pub fn run(&self, progress: &ProgressFn) -> std::result::Result<MergeOutput, String> {
        if self.sources.len() < self.kind.min_photos() {
            return Err(format!("select at least {} photos", self.kind.min_photos()));
        }
        if !progress(0.0, "Reading photos") {
            return Err("cancelled".into());
        }
        let sub = |a: f32, b: f32| move |f: f32, s: &str| progress(a + (b - a) * f, s);
        let mut pixelshift_permit = None;
        let (image, color, orientation, metadata, baseline, samples, crop, mut info, ghost) = match &self.kind {
            MergeKind::PixelShift => {
                let mut raws = Vec::with_capacity(self.sources.len());
                let mut source_size = None;
                let mut decode_allowance = 0usize;
                // Keep decode sequential: sixteen large sensors must not multiply decoder
                // scratch allocations by the machine's logical CPU count.
                for (i, (_, path)) in self.sources.iter().enumerate() {
                    if !progress(0.3 * i as f32 / self.sources.len() as f32, "Reading Pixel Shift shots") {
                        return Err("cancelled".into());
                    }
                    let bytes = (self.read)(path)?;
                    let info = lightcraft_raw::probe_info(&bytes).map_err(|e| format!("{path}: {e}"))?;
                    if info.metadata.sony.as_ref().and_then(|s| s.pixel_shift.as_ref()).is_none()
                        && lightcraft_meta::extract(&bytes).sony.as_ref().and_then(|s| s.pixel_shift.as_ref()).is_none()
                    {
                        return Err(
                            "Sony Pixel Shift capture information is missing; select the original Pixel Shift ARWs or one composed ARQ".into()
                        );
                    }
                    if source_size.is_some_and(|size| size != (info.width, info.height)) {
                        return Err("Pixel Shift sources have different sensor dimensions".into());
                    }
                    if i == 0 {
                        source_size = Some((info.width, info.height));
                        let cost = pixelshift_memory_bytes(info.width, info.height, self.sources.len())?;
                        let jobs = lightcraft_sysmem::heavy_jobs();
                        if cost > jobs.headroom() {
                            return Err(format!(
                                "Pixel Shift needs approximately {:.1} GB of working memory; {:.1} GB is currently available within the application budget. Close other work and retry.",
                                cost as f64 / 1_073_741_824.0,
                                jobs.headroom() as f64 / 1_073_741_824.0
                            ));
                        }
                        decode_allowance = cost.min(usize::MAX as u64) as usize;
                        pixelshift_permit = Some(jobs.admit(cost, &|| !progress(0.0, "Waiting for Pixel Shift memory")).ok_or("cancelled")?);
                    }
                    let mut raw = lightcraft_raw::decode_for_pixel_shift(&bytes, decode_allowance).map_err(|e| format!("{path}: {e}"))?;
                    if raw.metadata.sony.as_ref().and_then(|s| s.pixel_shift.as_ref()).is_none() {
                        raw.metadata.sony = lightcraft_meta::extract(&bytes).sony;
                    }
                    raws.push(raw);
                }
                let r = lightcraft_merge::merge_pixel_shift(raws, &sub(0.3, 0.88)).map_err(err)?;
                let motion_pixels = r.motion.data.iter().filter(|v| **v > 0.0).count();
                let mut image = r.image;
                // Reconstruction must see the original CFA sampling lattice even for
                // previews. Resize the complete reconstruction, never its source mosaics.
                if let Some(edge) = self.preview {
                    image = lightcraft_raster::resample::fit(&image, edge, edge, lightcraft_raster::resample::Filter::Box);
                }
                let info = json!({"shots": r.shots, "motionPixels": motion_pixels, "motionCorrection": self.sources.len() > 1,
                    "motionFallback": if self.sources.len() > 1 { "firstShotAuto" } else { "alreadyComposed" }});
                let ghost = (self.preview.is_some() && self.finish.show_overlay && motion_pixels > 0).then(|| r.motion.oriented(r.orientation));
                (image, r.color, r.orientation, r.metadata, r.baseline_exposure, lightcraft_merge::DngSamples::Float, None, info, ghost)
            }
            MergeKind::Hdr { hdr } => {
                let frames = self.load(false, progress, (0.0, 0.3))?;
                let p = sub(0.3, 0.9);
                let r = lightcraft_merge::merge_hdr(frames, hdr, &p).map_err(err)?;
                let info = json!({
                    "reference": self.sources[r.reference].0.0,
                    "ev": r.ev, "evExif": r.ev_exif,
                    "alignment": r.alignments.iter().map(|a| json!({"model": a.model, "inliers": a.inliers, "rmsPx": a.rms})).collect::<Vec<_>>(),
                });
                let ghost = (!r.ghost.data.is_empty()).then(|| r.ghost.oriented(r.orientation));
                (r.radiance, r.color, r.orientation, r.metadata, r.baseline_exposure, lightcraft_merge::DngSamples::Half, None, info, ghost)
            }
            MergeKind::Panorama { pano } => {
                let frames = self.load(true, progress, (0.0, 0.25))?;
                let p = sub(0.25, 0.9);
                let r = lightcraft_merge::stitch(frames, pano, &p).map_err(err)?;
                let info = pano_info(&r, &self.sources);
                (
                    r.image,
                    r.color,
                    lightcraft_geom::Orientation::Normal,
                    r.metadata,
                    r.baseline_exposure,
                    lightcraft_merge::DngSamples::U16,
                    r.crop,
                    info,
                    None,
                )
            }
            MergeKind::HdrPanorama { hdr, pano, bracket } => {
                let frames = self.load(false, progress, (0.0, 0.2))?;
                let b =
                    if *bracket >= 2 { *bracket } else { detect_bracket(&frames).ok_or("could not detect the bracket size from EXIF; set it")? };
                if !frames.len().is_multiple_of(b) || frames.len() / b < 2 {
                    return Err(format!("{} photos don't form brackets of {b} for at least two positions", frames.len()));
                }
                let groups = frames.len() / b;
                let mut merged = Vec::new();
                let mut it = frames.into_iter();
                for g in 0..groups {
                    let set: Vec<Frame> = it.by_ref().take(b).collect();
                    let p = sub(0.2 + 0.4 * g as f32 / groups as f32, 0.2 + 0.4 * (g + 1) as f32 / groups as f32);
                    let r = lightcraft_merge::merge_hdr(set, hdr, &p).map_err(err)?;
                    let mut f = Frame {
                        image: r.radiance,
                        clip: f32::INFINITY,
                        exposure: None,
                        raw: r.raw,
                        color: r.color,
                        orientation: r.orientation,
                        metadata: r.metadata,
                        baseline_exposure: r.baseline_exposure,
                    };
                    f.orient();
                    merged.push(f);
                }
                let p = sub(0.6, 0.9);
                let r = lightcraft_merge::stitch(merged, pano, &p).map_err(err)?;
                let mut info = pano_info(&r, &self.sources);
                info["bracket"] = json!(b);
                (
                    r.image,
                    r.color,
                    lightcraft_geom::Orientation::Normal,
                    r.metadata,
                    r.baseline_exposure,
                    lightcraft_merge::DngSamples::Half,
                    r.crop,
                    info,
                    None,
                )
            }
        };
        if !progress(0.9, "Writing DNG") {
            return Err("cancelled".into());
        }
        let mut metadata = metadata;
        if self.finish.stack {
            let names: Vec<&str> =
                self.sources.iter().map(|(_, p)| std::path::Path::new(p).file_name().and_then(|n| n.to_str()).unwrap_or(p)).collect();
            metadata.caption = Some(format!("Merged from {}", names.join(", ")));
        }
        metadata.software = Some("Local Image Photo Merge".into());
        let dng = lightcraft_merge::write_linear_dng(&image, &color, orientation, &metadata, baseline, samples).map_err(err)?;
        info["width"] = json!(image.width);
        info["height"] = json!(image.height);
        let preview = match self.preview {
            Some(edge) => Some(render_preview(&dng, edge, self.finish.auto_settings, crop, ghost.as_ref().filter(|_| self.finish.show_overlay))?),
            None => None,
        };
        progress(1.0, "Done");
        drop(pixelshift_permit);
        Ok(MergeOutput { dng, preview, crop, info })
    }
}

/// Conservative peak: decoded sensor storage, first-shot develop scratch, full RGB/mask,
/// float DNG staging and compressed output. Use full sensor dimensions before any allocation;
/// previews need the same CFA reconstruction as final merges.
pub(crate) fn pixelshift_memory_bytes(width: usize, height: usize, shots: usize) -> std::result::Result<u64, String> {
    if width < 2 || height < 2 || !matches!(shots, 1 | 4 | 16) {
        return Err("select one composed ARQ or one complete 4-shot or 16-shot capture".into());
    }
    let factor_squared = if shots == 16 { 4 } else { 1 };
    let pixels = (width as u64)
        .checked_mul(height as u64)
        .filter(|p| *p <= isize::MAX as u64 / 12)
        .ok_or("Pixel Shift dimensions exceed the supported address space")?;
    pixels
        .checked_mul(shots as u64 * 4 + factor_squared * 48 + 32)
        .and_then(|bytes| bytes.checked_add(8 << 20))
        .ok_or_else(|| "Pixel Shift working memory estimate overflowed".into())
}

fn pano_info(r: &lightcraft_merge::PanoResult, sources: &[(PhotoId, String)]) -> Value {
    json!({
        "projection": r.projection,
        "used": r.used.iter().filter_map(|&i| sources.get(i).map(|s| s.0.0)).collect::<Vec<_>>(),
        "fov": [r.fov.0, r.fov.1],
        "rmsPx": r.rms,
        "gains": r.gains,
        "crop": r.crop,
    })
}

/// Bracket size from EXIF: the smallest period of the exposure sequence (≥ 2).
pub fn detect_bracket(frames: &[Frame]) -> Option<usize> {
    let ev: Vec<f64> = frames.iter().map(|f| f.exposure).collect::<Option<_>>()?;
    let n = ev.len();
    (2..=n / 2).find(|&b| n.is_multiple_of(b) && (0..n).all(|k| (ev[k] - ev[k % b]).abs() < 0.2) && (1..b).any(|k| (ev[k] - ev[0]).abs() > 0.5))
}

/// Render a merged DNG like the library will show it (default raw settings, optionally Auto
/// Settings and the auto crop), with the deghost overlay tinted red.
fn render_preview(
    dng: &[u8],
    edge: usize,
    auto: bool,
    crop: Option<[f64; 4]>,
    ghost: Option<&lightcraft_raster::Plane>,
) -> std::result::Result<Rgba8, String> {
    let (src, info) = crate::files::load_bytes(dng, edge)?;
    let mut s = DevelopSettings::for_raw(info.as_shot_temp, info.as_shot_tint);
    if auto {
        apply_auto(&mut s, &src, &info);
    }
    if let Some(c) = crop {
        s.crop.geometry.rect = lightcraft_geom::Rect::from_xywh(c[0], c[1], c[2], c[3]);
    }
    let req = RenderRequest { quality: Quality::Full, apply_crop: false, ..RenderRequest::fit(edge, edge) };
    let mut img = lightcraft_pipeline::render(&src, &info, &s, &req).image;
    if let Some(g) = ghost {
        let (w, h) = (img.width, img.height);
        for y in 0..h {
            for x in 0..w {
                let gx = ((x as f32 + 0.5) / w as f32 * g.width as f32) as usize;
                let gy = ((y as f32 + 0.5) / h as f32 * g.height as f32) as usize;
                let a = g.get(gx.min(g.width - 1), gy.min(g.height - 1)).clamp(0.0, 1.0) * 0.6;
                if a > 0.01 {
                    let p = &mut img.data[y * w + x];
                    p[0] = (p[0] as f32 * (1.0 - a) + 255.0 * a) as u8;
                    p[1] = (p[1] as f32 * (1.0 - a)) as u8;
                    p[2] = (p[2] as f32 * (1.0 - a)) as u8;
                }
            }
        }
    }
    if let Some(c) = crop {
        // draw the auto-crop frame
        let (w, h) = (img.width as f64, img.height as f64);
        let (x0, y0, x1, y1) = ((c[0] * w) as usize, (c[1] * h) as usize, ((c[0] + c[2]) * w) as usize, ((c[1] + c[3]) * h) as usize);
        for y in y0..y1.min(img.height) {
            for x in x0..x1.min(img.width) {
                if y == y0 || y + 1 == y1 || x == x0 || x + 1 == x1 {
                    img.data[y * img.width + x] = [255, 255, 255, 255];
                }
            }
        }
    }
    Ok(img)
}

fn apply_auto(s: &mut DevelopSettings, src: &Rgb32f, info: &SourceInfo) {
    let a = lightcraft_pipeline::auto::auto_tone(src, info, s);
    s.light.exposure = a.exposure;
    s.light.contrast = a.contrast;
    s.light.highlights = a.highlights;
    s.light.shadows = a.shadows;
    s.light.whites = a.whites;
    s.light.blacks = a.blacks;
    s.color.vibrance = a.vibrance;
    s.color.saturation = a.saturation;
}

/// `dir/<stem>-<suffix>.dng`, then `-2`, `-3`… (the candidates for a merge's output).
fn output_names(first: &str, suffix: &str) -> impl Iterator<Item = std::path::PathBuf> {
    let p = std::path::Path::new(first);
    let dir = p.parent().map(|d| d.to_path_buf()).unwrap_or_default();
    let stem = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Merge".into());
    let suffix = suffix.to_string();
    (1u64..).map(move |k| dir.join(if k == 1 { format!("{stem}-{suffix}.dng") } else { format!("{stem}-{suffix}-{k}.dng") }))
}

/// Parse `merge.*` command parameters into a kind and finishing options.
pub fn parse(id: &str, p: &Value) -> Result<(MergeKind, MergeFinish)> {
    let bad = |m: String| EngineError::BadParams { cmd: id.into(), msg: m };
    let deghost: Deghost = serde_json::from_value(p.get("deghost").cloned().unwrap_or(json!("none"))).map_err(|e| bad(format!("deghost: {e}")))?;
    let hdr = HdrOptions { align: p.get("align").and_then(Value::as_bool).unwrap_or(true), deghost };
    let projection: Projection =
        serde_json::from_value(p.get("projection").cloned().unwrap_or(json!("auto"))).map_err(|e| bad(format!("projection: {e}")))?;
    let pano = PanoOptions {
        projection,
        boundary_warp: p.get("boundaryWarp").and_then(Value::as_f64).unwrap_or(0.0).clamp(0.0, 100.0),
        auto_crop: p.get("autoCrop").and_then(Value::as_bool).unwrap_or(false),
        fill_edges: p.get("fillEdges").and_then(Value::as_bool).unwrap_or(false),
        max_megapixels: p.get("maxMegapixels").and_then(Value::as_f64).unwrap_or(40.0),
    };
    let kind = match id {
        "merge.pixelShift" => MergeKind::PixelShift,
        "merge.hdr" => MergeKind::Hdr { hdr },
        "merge.panorama" => MergeKind::Panorama { pano },
        _ => MergeKind::HdrPanorama { hdr, pano, bracket: p.get("bracket").and_then(Value::as_u64).unwrap_or(0) as usize },
    };
    let finish = MergeFinish {
        auto_settings: p.get("autoSettings").and_then(Value::as_bool).unwrap_or(true),
        stack: p.get("stack").and_then(Value::as_bool).unwrap_or(false),
        show_overlay: p.get("showOverlay").and_then(Value::as_bool).unwrap_or(false),
    };
    Ok((kind, finish))
}

impl Session {
    /// Plan a merge of `ids` (catalog order of the given ids is kept).
    pub fn plan_merge(&self, kind: MergeKind, finish: MergeFinish, ids: &[PhotoId], preview: bool) -> Result<MergeJob> {
        let mut sources = Vec::new();
        for id in ids {
            let p = self.catalog.photo(*id).ok_or(lightcraft_catalog::CatalogError::NoPhoto(*id))?;
            match &p.source {
                Source::File { path } => sources.push((*id, path.clone())),
                Source::Demo { .. } => return Err(EngineError::Other(format!("{} is a demo photo; Photo Merge needs photo files", p.file_name))),
            }
        }
        if sources.len() < kind.min_photos() {
            return Err(EngineError::Other(format!("select at least {} photos to merge", kind.min_photos())));
        }
        let read: ByteReader = match &self.media.file_bytes {
            Some(r) => r.clone(),
            None => Arc::new(|path: &str| std::fs::read(path).map_err(|e| format!("{path}: {e}"))),
        };
        Ok(MergeJob { kind, finish, sources, preview: preview.then_some(PREVIEW_EDGE), read })
    }

    /// Write a finished (non-preview) merge next to its first source, import it, select it and
    /// apply Auto Settings / the auto crop. Returns `{id, path, …info}`.
    pub fn finish_merge(&mut self, job: &MergeJob, out: MergeOutput) -> Result<Value> {
        let first = &job.sources.first().ok_or_else(|| EngineError::Other("nothing merged".into()))?.1;
        // a new file under a free name (never replacing one), complete and synced before it appears
        let path = lightcraft_catalog::safe_file::write_new_unique(&mut output_names(first, job.kind.suffix()), &out.dng)
            .map_err(|e| EngineError::Other(format!("could not write the merged DNG next to {first}: {e}")))?
            .to_string_lossy()
            .to_string();
        let report = crate::import::import(self, std::slice::from_ref(&path), crate::import::ImportMode::Add)?;
        let id = report
            .imported
            .first()
            .copied()
            .map(PhotoId)
            .ok_or_else(|| EngineError::Other(format!("the merged file {path} could not be imported: {:?}", report.failed)))?;
        if job.finish.stack {
            let sources: Vec<PhotoId> = job.sources.iter().map(|(p, _)| *p).collect();
            if let Some(op) = self.catalog.stack_with_ops(id, &sources) {
                self.commit("Stack with Sources", op)?;
            }
        }
        self.selection = crate::Selection::single(id);
        if job.finish.auto_settings {
            self.execute("develop.auto", &json!({}))?;
        }
        if let Some(c) = out.crop
            && let Some(d) = self.develop_of(id)
        {
            let mut d = (*d).clone();
            d.crop.geometry.rect = lightcraft_geom::Rect::from_xywh(c[0], c[1], c[2], c[3]);
            self.set_develop(id, d, "Auto Crop")?;
        }
        let mut info = out.info;
        info["id"] = json!(id.0);
        info["path"] = json!(path);
        info["bytes"] = json!(out.dng.len());
        Ok(info)
    }

    /// Plan, run and finish a merge synchronously (CLI, MCP, control channel).
    pub fn merge_now(&mut self, kind: MergeKind, finish: MergeFinish, ids: &[PhotoId]) -> Result<Value> {
        let job = self.plan_merge(kind, finish, ids, false)?;
        let out = job.run(&|_, _| true).map_err(EngineError::Other)?;
        self.finish_merge(&job, out)
    }
}
