//! Photo Merge (HDR, Panorama, HDR Panorama) in the library.
//!
//! A merge is planned from the session ([`crate::Session::plan_merge`] → [`MergeJob`], which is
//! `Send` and self-contained), run anywhere (a worker thread in the app, inline for the CLI/MCP:
//! [`MergeJob::run`], with progress and cancellation), and finished on the session
//! ([`crate::Session::finish_merge`]): the DNG is written next to the first source photo
//! (`<name>-HDR.dng`, `-Pano.dng`, `-HDR-Pano.dng`), imported, selected, and given Auto Settings /
//! the auto-crop rectangle when asked. A *preview* returns a ≤ 1024 px rendered image
//! (with the deghost overlay when asked) instead of a file; raw HDR fusion still uses
//! the full sensor lattice before resizing.

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

/// A shoot-wide HDR request. Detection and fusion run outside the session so callers
/// can move this value to the same background worker used for individual merges.
#[derive(Clone)]
pub struct BatchMergeJob {
    pub source: MergeJob,
    /// Catalog capture times supply a fallback when a file has no EXIF timestamp.
    pub captured: Vec<Option<String>>,
    pub brackets: lightcraft_merge::brackets::BracketOptions,
}

#[derive(Clone, Debug, Default)]
pub struct BatchMergeSummary {
    pub groups: usize,
    pub completed: usize,
    pub skipped: Vec<PhotoId>,
    pub failed: Vec<(Vec<PhotoId>, String)>,
}

impl BatchMergeJob {
    /// Detect exposure brackets using EV, capture time and scene similarity. Only small
    /// fingerprints survive this pass; full-resolution brackets are loaded one set at a time.
    pub fn detect(&self, progress: &ProgressFn) -> std::result::Result<Vec<MergeJob>, String> {
        let mut probes = Vec::with_capacity(self.source.sources.len());
        let n = self.source.sources.len();
        for (i, (_, path)) in self.source.sources.iter().enumerate() {
            if !progress(i as f32 / n.max(1) as f32, "Detecting HDR brackets") {
                return Err("cancelled".into());
            }
            let bytes = (self.source.read)(path)?;
            let metadata = lightcraft_meta::extract(&bytes);
            let raw_info = if lightcraft_raw::probe(&bytes).is_some() {
                Some(lightcraft_raw::probe_info(&bytes).map_err(|e| format!("{path}: {e}"))?)
            } else {
                None
            };
            let exposure = lightcraft_merge::frame::exif_exposure(&metadata)
                .or_else(|| raw_info.as_ref().and_then(|info| lightcraft_merge::frame::exif_exposure(&info.metadata)));
            let capture = metadata
                .capture_time
                .or_else(|| raw_info.as_ref().and_then(|info| info.metadata.capture_time))
                .or_else(|| self.captured.get(i).and_then(|time| time.as_deref()).and_then(lightcraft_meta::DateTime::parse_iso));
            // A full sensor decode cannot establish a missing EV or capture time; leave
            // those sources ungrouped without spending the demosaic memory/time.
            if exposure.is_none() || capture.is_none() {
                probes.push(lightcraft_merge::brackets::BracketProbe {
                    exposure,
                    capture_seconds: capture.map(|time| time.unix_seconds() as f64 + time.millis as f64 / 1000.0),
                    width: 0,
                    height: 0,
                    camera: (metadata.make, metadata.model),
                    scene: [0.0; 64],
                });
                continue;
            }
            let frame = lightcraft_merge::load_frame(&bytes, Some(128), true).map_err(|e| format!("{path}: {e}"))?;
            let mut probe = lightcraft_merge::brackets::BracketProbe::from_frame(&frame);
            probe.exposure = exposure;
            probe.capture_seconds = capture.map(|time| time.unix_seconds() as f64 + time.millis as f64 / 1000.0);
            if let Some(info) = raw_info {
                let (w, h) = info.developed_size();
                (probe.width, probe.height) = if info.orientation.swaps_axes() { (h, w) } else { (w, h) };
            }
            probes.push(probe);
        }
        if !progress(1.0, "Detected HDR brackets") {
            return Err("cancelled".into());
        }
        Ok(lightcraft_merge::brackets::detect_groups(&probes, &self.brackets)
            .into_iter()
            .map(|group| {
                let mut job = self.source.clone();
                job.sources = group.into_iter().map(|i| self.source.sources[i].clone()).collect();
                job
            })
            .collect())
    }

    /// Merge many sets on a worker with overall progress. Consume each output before the
    /// next set starts (for example through a bounded channel to `Session::finish_merge`),
    /// rather than holding a shoot's full DNGs in memory. Failed sets are reported and the
    /// remaining sets continue; cancellation aborts immediately.
    pub fn run(
        &self,
        progress: &ProgressFn,
        finished: &mut dyn FnMut(&MergeJob, MergeOutput) -> std::result::Result<(), String>,
    ) -> std::result::Result<BatchMergeSummary, String> {
        let jobs = self.detect(&|f, stage| progress(0.1 * f, stage))?;
        let used: std::collections::HashSet<_> = jobs.iter().flat_map(|job| job.sources.iter().map(|(id, _)| *id)).collect();
        let mut summary = BatchMergeSummary {
            groups: jobs.len(),
            skipped: self.source.sources.iter().map(|(id, _)| *id).filter(|id| !used.contains(id)).collect(),
            ..Default::default()
        };
        for (i, job) in jobs.iter().enumerate() {
            let group = |f: f32, stage: &str| {
                let label = format!("HDR set {} of {}: {stage}", i + 1, jobs.len());
                progress(0.1 + 0.9 * (i as f32 + f) / jobs.len() as f32, &label)
            };
            match job.run(&|fraction, stage| group(0.9 * fraction, stage)) {
                Ok(output) => {
                    if !group(0.9, "Preparing DNG") {
                        return Err("cancelled".into());
                    }
                    finished(job, output)?;
                    summary.completed += 1;
                    if !group(1.0, "Delivered DNG") {
                        return Err("cancelled".into());
                    }
                }
                Err(error) if error == "cancelled" => return Err(error),
                Err(error) => summary.failed.push((job.sources.iter().map(|(id, _)| *id).collect(), error)),
            }
        }
        if !progress(1.0, "Batch HDR complete") {
            return Err("cancelled".into());
        }
        Ok(summary)
    }
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

/// A worker-written, synced DNG ready for catalog attachment. Dropping an unaccepted
/// result removes only this newly-created file; an accepted catalog import owns it.
/// The path is private so cancellation can never be redirected to a source photo.
pub struct PreparedMerge {
    path: String,
    info: Value,
    crop: Option<[f64; 4]>,
    auto: Option<DevelopSettings>,
    bytes: usize,
    preserve_file: bool,
    identity: Option<std::fs::Metadata>,
    probe: Option<crate::media::ProbeInfo>,
}

impl PreparedMerge {
    pub fn path(&self) -> &str {
        &self.path
    }
}

impl Drop for PreparedMerge {
    fn drop(&mut self) {
        if self.preserve_file {
            return;
        }
        // A user may replace a result before it is accepted. Do not remove that new
        // file or a symlink, even though the original output path belongs to this job.
        let Some(original) = &self.identity else {
            return;
        };
        let Ok(current) = std::fs::symlink_metadata(&self.path) else {
            return;
        };
        if !current.file_type().is_file() || current.len() != original.len() || current.modified().ok() != original.modified().ok() {
            return;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if current.dev() != original.dev() || current.ino() != original.ino() {
                return;
            }
        }
        let _ = std::fs::remove_file(&self.path);
    }
}

pub type ProgressFn<'a> = dyn Fn(f32, &str) -> bool + Sync + 'a;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

impl MergeJob {
    fn load(&self, orient: bool, progress: &ProgressFn, span: (f32, f32)) -> std::result::Result<Vec<Frame>, String> {
        // Sensor decoders allocate full-size scratch even for small previews. Keep their
        // peak bounded by one decoder rather than the machine's logical CPU count.
        let n = self.sources.len();
        self.sources
            .iter()
            .enumerate()
            .map(|(i, (_, path))| {
                if !progress(span.0 + (span.1 - span.0) * i as f32 / n as f32, "Reading photos") {
                    return Err("cancelled".into());
                }
                let bytes = (self.read)(path)?;
                let frame = lightcraft_merge::load_frame(&bytes, self.preview, orient).map_err(|e| format!("{path}: {e}"))?;
                if !progress(span.0 + (span.1 - span.0) * (i + 1) as f32 / n as f32, "Reading photos") {
                    return Err("cancelled".into());
                }
                Ok(frame)
            })
            .collect()
    }

    /// Keep sensor samples on the raw path until exposure fusion is complete. Mixed
    /// raw/rendered sets and handheld non-Bayer CFA lattices merge through the
    /// scene-linear demosaic path.
    fn hdr_result(
        &self,
        options: &HdrOptions,
        progress: &ProgressFn,
    ) -> std::result::Result<(lightcraft_merge::HdrResult, Option<lightcraft_sysmem::Permit<'static>>, bool), String> {
        let mut raws = Vec::with_capacity(self.sources.len());
        let mut permit = None;
        let mut allowance = usize::MAX;
        let mut geometry = None;
        for (i, (_, path)) in self.sources.iter().enumerate() {
            if !progress(0.3 * i as f32 / self.sources.len() as f32, "Reading raw brackets") {
                return Err("cancelled".into());
            }
            let bytes = (self.read)(path)?;
            if lightcraft_raw::probe(&bytes).is_none() {
                drop(raws);
                drop(permit);
                let frames = self.load(false, progress, (0.0, 0.3))?;
                return lightcraft_merge::merge_hdr(frames, options, &|f, stage| progress(0.3 + 0.7 * f, stage))
                    .map(|r| (r, None, false))
                    .map_err(err);
            }
            let info = lightcraft_raw::probe_info(&bytes).map_err(|e| format!("{path}: {e}"))?;
            let extent = (info.width, info.height, info.cpp);
            if geometry.is_some_and(|first| first != extent) {
                return Err("HDR brackets have different sensor dimensions or sample layouts".into());
            }
            if i == 0 {
                geometry = Some(extent);
                let cost = hdr_memory_bytes(info.width, info.height, self.sources.len(), info.cpp)?;
                let jobs = lightcraft_sysmem::heavy_jobs();
                if cost > jobs.headroom() {
                    return Err(format!(
                        "HDR needs approximately {:.1} GB of working memory; {:.1} GB is currently available within the application budget. Close other work and retry.",
                        cost as f64 / 1_073_741_824.0,
                        jobs.headroom() as f64 / 1_073_741_824.0
                    ));
                }
                allowance = cost.min(usize::MAX as u64) as usize;
                permit = Some(jobs.admit(cost, &|| !progress(0.0, "Waiting for HDR memory")).ok_or("cancelled")?);
            }
            if options.align && info.cfa.as_ref().is_some_and(|cfa| !cfa.is_bayer()) {
                // Handheld X-Trans and other non-Bayer CFA layouts would need a dedicated
                // raw resampler to align, so they merge through the shared scene-linear
                // demosaic path instead.
                drop(raws);
                let frames = self.load(false, progress, (0.0, 0.3))?;
                return lightcraft_merge::merge_hdr(frames, options, &|f, stage| progress(0.3 + 0.7 * f, stage))
                    .map(|result| (result, permit, false))
                    .map_err(err);
            }
            let mut raw = lightcraft_raw::decode_with_memory_budget(&bytes, allowance).map_err(|e| format!("{path}: {e}"))?;
            if raw.metadata == lightcraft_meta::Metadata::default() {
                raw.metadata = lightcraft_meta::extract(&bytes);
            }
            raws.push(raw);
        }
        let mut result = lightcraft_merge::merge_raw_hdr(&raws, options, &|f, stage| progress(0.3 + 0.7 * f, stage)).map_err(err)?;
        if let Some(edge) = self.preview {
            // Alignment/CFA fusion always sees the original sensor sampling lattice.
            result.radiance = lightcraft_raster::resample::fit(&result.radiance, edge, edge, li_raster::resample::Filter::Box);
            if !result.ghost.data.is_empty() {
                result.ghost = lightcraft_raster::resample::resize(
                    &result.ghost,
                    result.radiance.width,
                    result.radiance.height,
                    li_raster::resample::Filter::Box,
                );
            }
        }
        Ok((result, permit, true))
    }

    /// Complete file I/O and optional Auto Settings on a background worker. Catalog
    /// mutations stay in `Session::finish_prepared_merge`; dropping this output before
    /// attachment cleans up the generated file (including cancelled/stale UI requests).
    pub fn prepare_output(&self, out: MergeOutput) -> std::result::Result<PreparedMerge, String> {
        let first = &self.sources.first().ok_or("nothing merged")?.1;
        let path = li_fs::write_new_unique(&mut output_names(first, self.kind.suffix()), &out.dng)
            .map_err(|error| format!("could not write the merged DNG next to {first}: {error}"))?;
        let identity = std::fs::symlink_metadata(&path).ok();
        let mut prepared = PreparedMerge {
            path: path.to_string_lossy().into_owned(),
            info: out.info,
            crop: out.crop,
            auto: None,
            bytes: out.dng.len(),
            preserve_file: false,
            identity,
            probe: None,
        };
        // Seed import's existing verified probe cache from the worker's bytes, avoiding
        // a second full-file read/hash on the UI thread. Invalid externally constructed
        // outputs still fail through the normal import path.
        prepared.probe = crate::files::probe_bytes(prepared.path(), &out.dng).ok().map(|mut probe| {
            probe.modified = prepared.identity.as_ref().and_then(|metadata| metadata.modified().ok());
            probe
        });
        if self.finish.auto_settings {
            // The owned byte buffer is released immediately after sensor decode, before
            // development/Auto scratch is allocated. No raw decode runs on the UI thread.
            let (source, info) = crate::files::load_vec(out.dng, PREVIEW_EDGE)?;
            let mut settings = DevelopSettings::for_raw(info.as_shot_temp, info.as_shot_tint);
            apply_auto(&mut settings, &source, &info);
            prepared.auto = Some(settings);
        }
        Ok(prepared)
    }

    /// Run the merge. `progress(fraction, stage)` returns false to cancel.
    pub fn run(&self, progress: &ProgressFn) -> std::result::Result<MergeOutput, String> {
        // Fallback adapters may restart their local stage. Expose monotonic progress
        // to callers while preserving every cancellation request.
        let fraction = std::sync::atomic::AtomicU32::new(0);
        let report = |value: f32, stage: &str| {
            let value = value.clamp(0.0, 1.0);
            let previous = fraction.fetch_max(value.to_bits(), std::sync::atomic::Ordering::Relaxed);
            progress(value.max(f32::from_bits(previous)), stage)
        };
        let progress: &ProgressFn = &report;
        if self.sources.len() < self.kind.min_photos() {
            return Err(format!("select at least {} photos", self.kind.min_photos()));
        }
        if !progress(0.0, "Reading photos") {
            return Err("cancelled".into());
        }
        let sub = |a: f32, b: f32| move |f: f32, s: &str| progress(a + (b - a) * f, s);
        let mut pixelshift_permit = None;
        let mut hdr_permit = None;
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
                    image = lightcraft_raster::resample::fit(&image, edge, edge, li_raster::resample::Filter::Box);
                }
                let info = json!({"shots": r.shots, "motionPixels": motion_pixels, "motionCorrection": self.sources.len() > 1,
                    "motionFallback": if self.sources.len() > 1 { "firstShotAuto" } else { "alreadyComposed" }});
                let ghost = (self.preview.is_some() && self.finish.show_overlay && motion_pixels > 0).then(|| r.motion.oriented(r.orientation));
                (image, r.color, r.orientation, r.metadata, r.baseline_exposure, lightcraft_merge::DngSamples::Float, None, info, ghost)
            }
            MergeKind::Hdr { hdr } => {
                let (r, permit, raw_domain) = self.hdr_result(hdr, &sub(0.0, 0.9))?;
                hdr_permit = permit;
                let info = json!({
                    "reference": self.sources[r.reference].0.0,
                    "ev": r.ev, "evExif": r.ev_exif,
                    "rawDomain": raw_domain, "sampleType": "float32",
                    "alignment": r.alignments.iter().map(|a| json!({"model": a.model, "inliers": a.inliers, "rmsPx": a.rms})).collect::<Vec<_>>(),
                });
                let ghost = (!r.ghost.data.is_empty()).then(|| r.ghost.oriented(r.orientation));
                (r.radiance, r.color, r.orientation, r.metadata, r.baseline_exposure, lightcraft_merge::DngSamples::Float, None, info, ghost)
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
                let b = if *bracket >= 2 {
                    *bracket
                } else {
                    let mut detection = self.clone();
                    detection.preview = Some(128);
                    let frames = detection.load(false, progress, (0.0, 0.2))?;
                    detect_bracket(&frames).ok_or("could not detect the bracket size from EXIF; set it")?
                };
                if !self.sources.len().is_multiple_of(b) || self.sources.len() / b < 2 {
                    return Err(format!("{} photos don't form brackets of {b} for at least two positions", self.sources.len()));
                }
                let groups = self.sources.len() / b;
                let mut merged = Vec::new();
                let mut raw_domains = Vec::with_capacity(groups);
                for g in 0..groups {
                    let mut set = self.clone();
                    set.sources = self.sources[g * b..(g + 1) * b].to_vec();
                    let p = sub(0.2 + 0.4 * g as f32 / groups as f32, 0.2 + 0.4 * (g + 1) as f32 / groups as f32);
                    let (r, permit, raw_domain) = set.hdr_result(hdr, &p)?;
                    raw_domains.push(raw_domain);
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
                    drop(permit);
                }
                let p = sub(0.6, 0.9);
                let r = lightcraft_merge::stitch(merged, pano, &p).map_err(err)?;
                let mut info = pano_info(&r, &self.sources);
                info["bracket"] = json!(b);
                info["rawDomain"] = json!(raw_domains.iter().all(|raw| *raw));
                info["rawDomainBrackets"] = json!(raw_domains);
                info["sampleType"] = json!("float32");
                (
                    r.image,
                    r.color,
                    lightcraft_geom::Orientation::Normal,
                    r.metadata,
                    r.baseline_exposure,
                    lightcraft_merge::DngSamples::Float,
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
        if !progress(1.0, "Done") {
            return Err("cancelled".into());
        }
        drop(pixelshift_permit);
        drop(hdr_permit);
        Ok(MergeOutput { dng, preview, crop, info })
    }
}

/// Raw bracket storage, decoder scratch, fused CFA, single demosaic, mask and DNG staging.
/// This conservative estimate uses the sensor extent even when the requested result is a preview.
fn hdr_memory_bytes(width: usize, height: usize, shots: usize, cpp: usize) -> std::result::Result<u64, String> {
    if width < 2 || height < 2 || shots < 2 || !matches!(cpp, 1 | 3) {
        return Err("HDR needs at least two nonempty sensor frames".into());
    }
    let per_pixel = (shots as u64).checked_mul(4 * cpp as u64).and_then(|n| n.checked_add(112)).ok_or("HDR memory estimate overflowed")?;
    (width as u64)
        .checked_mul(height as u64)
        .and_then(|n| n.checked_mul(per_pixel))
        .and_then(|n| n.checked_add(8 << 20))
        .ok_or_else(|| "HDR memory estimate overflowed".into())
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

    /// Plan a detached batch from the selected shoot. EV/time/scene detection happens
    /// inside the worker, so planning never decodes sensor pixels on the UI thread.
    pub fn plan_batch_hdr(&self, hdr: HdrOptions, finish: MergeFinish, ids: &[PhotoId]) -> Result<BatchMergeJob> {
        let source = self.plan_merge(MergeKind::Hdr { hdr }, finish, ids, false)?;
        let captured = source.sources.iter().map(|(id, _)| self.catalog.photo(*id).and_then(|photo| photo.captured.clone())).collect();
        Ok(BatchMergeJob { source, captured, brackets: lightcraft_merge::brackets::BracketOptions::default() })
    }

    /// Write a finished (non-preview) merge next to its first source, import it, select it and
    /// apply Auto Settings / the auto crop. Returns `{id, path, …info}`.
    pub fn finish_merge(&mut self, job: &MergeJob, out: MergeOutput) -> Result<Value> {
        let prepared = job.prepare_output(out).map_err(EngineError::Other)?;
        self.finish_prepared_merge(job, prepared)
    }

    /// Attach a worker-prepared output with catalog/settings/selection changes only.
    /// The generated file becomes permanent as soon as catalog import succeeds.
    pub fn finish_prepared_merge(&mut self, job: &MergeJob, mut prepared: PreparedMerge) -> Result<Value> {
        let path = prepared.path.clone();
        if let Some(probe) = prepared.probe.take() {
            self.import_probes.insert(path.clone(), probe);
        }
        let report = crate::import::import(self, std::slice::from_ref(&path), crate::import::ImportMode::Add)?;
        let id = report
            .imported
            .first()
            .copied()
            .map(PhotoId)
            .ok_or_else(|| EngineError::Other(format!("the merged file {path} could not be imported: {:?}", report.failed)))?;
        prepared.preserve_file = true;
        if job.finish.stack {
            let sources: Vec<PhotoId> = job.sources.iter().map(|(p, _)| *p).collect();
            if let Some(op) = self.catalog.stack_with_ops(id, &sources) {
                self.commit("Stack with Sources", op)?;
            }
        }
        self.selection = crate::Selection::single(id);
        if let Some(auto) = prepared.auto.take()
            && let Some(current) = self.develop_of(id)
        {
            let mut settings = (*current).clone();
            settings.light = auto.light;
            settings.color.vibrance = auto.color.vibrance;
            settings.color.saturation = auto.color.saturation;
            self.set_develop(id, settings, "Auto Settings")?;
        }
        if let Some(crop) = prepared.crop
            && let Some(settings) = self.develop_of(id)
        {
            let mut settings = (*settings).clone();
            settings.crop.geometry.rect = lightcraft_geom::Rect::from_xywh(crop[0], crop[1], crop[2], crop[3]);
            self.set_develop(id, settings, "Auto Crop")?;
        }
        let mut info = std::mem::take(&mut prepared.info);
        info["id"] = json!(id.0);
        info["path"] = json!(path);
        info["bytes"] = json!(prepared.bytes);
        Ok(info)
    }

    /// Plan, run and finish a merge synchronously (CLI, MCP, control channel).
    pub fn merge_now(&mut self, kind: MergeKind, finish: MergeFinish, ids: &[PhotoId]) -> Result<Value> {
        let job = self.plan_merge(kind, finish, ids, false)?;
        let out = job.run(&|_, _| true).map_err(EngineError::Other)?;
        self.finish_merge(&job, out)
    }
}

#[cfg(test)]
mod j19_tests {
    use super::*;

    fn batch_fixture() -> BatchMergeJob {
        let evs = [-2.0, 0.0, 2.0, -2.0, 0.0, 2.0, 0.0];
        let mut bytes = std::collections::HashMap::new();
        for (i, ev) in evs.into_iter().enumerate() {
            let image = Rgb32f::from_fn(48, 32, |x, y| {
                let signal = (0.04 + 0.002 * x as f32 + 0.001 * y as f32) * 2f32.powf(ev);
                [signal, signal * 0.9, signal * 0.8]
            });
            let color = Frame::from_linear_rec2020(image.clone(), None).color;
            let metadata = lightcraft_meta::Metadata {
                exposure_time: Some(2f64.powf(ev as f64) / 125.0),
                f_number: Some(8.0),
                iso: Some(100),
                ..Default::default()
            };
            let dng = lightcraft_merge::write_linear_dng(
                &image,
                &color,
                lightcraft_geom::Orientation::Normal,
                &metadata,
                0.0,
                lightcraft_merge::DngSamples::Float,
            )
            .unwrap();
            bytes.insert(format!("frame{i}.dng"), dng);
        }
        BatchMergeJob {
            source: MergeJob {
                kind: MergeKind::Hdr { hdr: HdrOptions { align: false, deghost: Deghost::None } },
                finish: MergeFinish::default(),
                sources: (0..7).map(|i| (PhotoId(i + 1), format!("frame{i}.dng"))).collect(),
                preview: None,
                read: Arc::new(move |path| bytes.get(path).cloned().ok_or_else(|| format!("missing {path}"))),
            },
            captured: [0, 1, 2, 20, 21, 22, 50].into_iter().map(|s| Some(format!("2026-10-14T12:00:{s:02}"))).collect(),
            brackets: lightcraft_merge::brackets::BracketOptions::default(),
        }
    }

    #[test]
    fn j19_batch_detects_and_streams_float_dngs() {
        let batch = batch_fixture();
        let mut delivered = Vec::new();
        let progress = std::sync::Mutex::new(Vec::new());
        let summary = batch
            .run(
                &|fraction, _| {
                    progress.lock().unwrap().push(fraction);
                    true
                },
                &mut |job, output| {
                    let raw = lightcraft_raw::probe_info(&output.dng).map_err(err)?;
                    assert_eq!(raw.bits, 32, "library HDR output must use full float samples");
                    assert_eq!(raw.developed_size(), (48, 32));
                    delivered.push(job.sources.iter().map(|(id, _)| *id).collect::<Vec<_>>());
                    Ok(())
                },
            )
            .unwrap();
        assert_eq!((summary.groups, summary.completed), (2, 2));
        assert_eq!(summary.skipped, vec![PhotoId(7)]);
        assert!(summary.failed.is_empty());
        assert_eq!(delivered, vec![vec![PhotoId(1), PhotoId(2), PhotoId(3)], vec![PhotoId(4), PhotoId(5), PhotoId(6)]]);
        let fractions = progress.lock().unwrap();
        assert!(fractions.windows(2).all(|pair| pair[0] <= pair[1]), "{fractions:?}");
        assert_eq!(fractions.last(), Some(&1.0));
    }

    #[test]
    fn j19_batch_cancels_before_delivering_and_checks_memory_overflow() {
        let batch = batch_fixture();
        let mut delivered = false;
        let result = batch.run(&|fraction, _| fraction < 0.2, &mut |_, _| {
            delivered = true;
            Ok(())
        });
        assert!(matches!(result, Err(error) if error == "cancelled"));
        assert!(!delivered);
        assert!(hdr_memory_bytes(usize::MAX, usize::MAX, 3, 1).is_err());
        assert!(hdr_memory_bytes(48, 32, 1, 1).is_err());
        assert!(hdr_memory_bytes(48, 32, 3, 1).unwrap() > (48 * 32 * 3 * 4) as u64);
    }

    #[test]
    fn j19_library_hdr_command_roundtrip_float_precision_and_timing() {
        let fixture = batch_fixture();
        let dir = std::env::current_dir().unwrap().join("target").join(format!("j19-library-command-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let paths: Vec<_> = fixture
            .source
            .sources
            .iter()
            .take(3)
            .map(|(_, name)| {
                let path = dir.join(name);
                std::fs::write(&path, (fixture.source.read)(name).unwrap()).unwrap();
                path.to_string_lossy().into_owned()
            })
            .collect();
        let mut session = Session::new().with_fs();
        let imported = session.execute("library.import", &json!({"paths": paths})).unwrap();
        let ids = imported["imported"].as_array().unwrap().clone();
        let start = std::time::Instant::now();
        let result = session.execute("merge.hdr", &json!({"ids": ids, "align": false, "autoSettings": false})).unwrap();
        eprintln!(
            "j19_library_hdr_command_cpu_ms={:.3} (48x32, three sources; command decode+merge+float DNG write+catalog import)",
            start.elapsed().as_secs_f64() * 1000.0
        );
        let path = result["path"].as_str().unwrap();
        assert_eq!(lightcraft_raw::probe_info(&std::fs::read(path).unwrap()).unwrap().bits, 32);
        assert_eq!(session.catalog.photos().count(), 4);
        assert!(std::path::Path::new(path).starts_with(&dir));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn j19_prepared_output_cleanup_and_catalog_attachment() {
        let fixture = batch_fixture();
        let dir = std::env::current_dir().unwrap().join("target").join(format!("j19-prepared-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut job = fixture.source;
        job.sources.truncate(3);
        // Source reads remain in memory; only new generated outputs use target/.
        job.sources[0].1 = dir.join("source.dng").to_string_lossy().into_owned();
        let image = Rgb32f::from_fn(48, 32, |x, y| [0.03 + x as f32 * 0.004, 0.03 + y as f32 * 0.003, 0.07]);
        let color = Frame::from_linear_rec2020(image.clone(), None).color;
        let dng = lightcraft_merge::write_linear_dng(
            &image,
            &color,
            lightcraft_geom::Orientation::Normal,
            &lightcraft_meta::Metadata::default(),
            0.0,
            lightcraft_merge::DngSamples::Float,
        )
        .unwrap();
        let output = || MergeOutput { dng: dng.clone(), preview: None, crop: None, info: json!({}) };
        let abandoned = job.prepare_output(output()).unwrap();
        let abandoned_path = abandoned.path().to_string();
        assert!(std::path::Path::new(&abandoned_path).exists());
        drop(abandoned);
        assert!(!std::path::Path::new(&abandoned_path).exists(), "cancelled/stale result should be removed");
        let replaced = job.prepare_output(output()).unwrap();
        let replaced_path = replaced.path().to_string();
        std::fs::write(&replaced_path, b"user replacement").unwrap();
        drop(replaced);
        assert_eq!(std::fs::read(&replaced_path).unwrap(), b"user replacement", "cleanup must preserve an output replaced by the owner");
        job.finish.auto_settings = true;
        let start = std::time::Instant::now();
        let accepted = job.prepare_output(output()).unwrap();
        eprintln!("j19_prepare_output_cpu_ms={:.3} (48x32 float DNG write+probe+Auto)", start.elapsed().as_secs_f64() * 1000.0);
        assert!(accepted.auto.is_some());
        assert!(accepted.probe.is_some());
        let mut session = Session::new().with_fs();
        let result = session.finish_prepared_merge(&job, accepted).unwrap();
        assert_eq!(session.catalog.photos().count(), 1);
        let path = result["path"].as_str().unwrap();
        assert!(std::path::Path::new(path).exists(), "accepted output must survive ownership guard drop");
        assert!(session.develop_of(PhotoId(result["id"].as_u64().unwrap())).is_some());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn j19_raw_preview_fuses_full_sensor_then_resizes() {
        let mut job = batch_fixture().source;
        job.sources.truncate(3);
        job.preview = Some(24);
        let result = job.run(&|_, _| true).unwrap();
        assert_eq!(lightcraft_raw::probe_info(&result.dng).unwrap().bits, 32);
        assert_eq!((result.info["width"].as_u64(), result.info["height"].as_u64()), (Some(24), Some(16)));
        assert!(result.preview.is_some());
    }
}
