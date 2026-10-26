use super::*;

/// One photo's export, set up from the session ([`prepare_export`]). [`PreparedExport::run`] does
/// the heavy part (read, decode, render, encode) without the session, so it can run on another
/// thread.
pub struct PreparedExport {
    pub photo: lightcraft_catalog::PhotoId,
    pub file_name: String,
    /// Relative folder within a batch destination (Smart Sort). Validated before rendering.
    pub subfolder: Option<String>,
    pub(super) work: Work,
    /// The library's originals, which [`run_batch`] never writes over (shared by a batch).
    pub(super) guard: std::sync::Arc<crate::originals::OriginalGuard>,
    /// The folder of the photo's original (`None` for a generated demo photo): the destination
    /// with [`ExportOptions::same_folder`].
    pub(super) source_dir: Option<String>,
}

pub(super) struct RenderWork {
    job: crate::media::RenderJob,
    upscale: Option<(std::sync::Arc<dyn crate::enhance::AiHost>, (usize, usize))>,
    meta: Option<Metadata>,
    opts: ExportOptions,
    /// Resize the full-size render to this size with this kernel ([`ExportOptions::resample`]).
    resample: Option<(ResampleChoice, (usize, usize))>,
    /// Estimated working memory ([`render_cost`]).
    cost: u64,
}

pub(super) enum Work {
    Render(Box<RenderWork>),
    /// [`ExportFormat::Original`] (the file's bytes + an XMP sidecar) or [`ExportFormat::Dng`] (the
    /// raw data re-encoded as a lossless DNG with the edits in its XMP).
    File {
        path: String,
        read: Option<crate::merge::ByteReader>,
        packet: String,
        dng: Option<DngCompression>,
        /// "Remove location info": the original must not carry a location (upstream LightCraft db7e378).
        remove_location: bool,
        label: String,
        size: (usize, usize),
    },
}

/// Set up the export of photo `id` at 1-based position `seq` of a batch. (For a whole batch use
/// [`prepare_batch`]: it looks at the library's originals once.)
pub fn prepare_export(
    session: &mut crate::Session,
    id: lightcraft_catalog::PhotoId,
    o: &ExportOptions,
    seq: usize,
) -> Result<PreparedExport, String> {
    let guard = std::sync::Arc::new(session.original_guard());
    prepare_guarded(session, id, o, seq, guard)
}

/// [`prepare_export`] for each of `ids` in order (`{seq}` = position, from 1).
pub fn prepare_batch(session: &mut crate::Session, ids: &[lightcraft_catalog::PhotoId], o: &ExportOptions) -> Result<Vec<PreparedExport>, String> {
    let guard = std::sync::Arc::new(session.original_guard());
    ids.iter().enumerate().map(|(i, id)| prepare_guarded(session, *id, o, i + 1, guard.clone())).collect()
}

pub(crate) fn prepare_guarded(
    session: &mut crate::Session,
    id: lightcraft_catalog::PhotoId,
    o: &ExportOptions,
    seq: usize,
    guard: std::sync::Arc<crate::originals::OriginalGuard>,
) -> Result<PreparedExport, String> {
    let p = session.catalog.photo(id).ok_or("no such photo")?;
    let file_name = o.file_name_for(p, seq);
    let source_dir = match &p.source {
        lightcraft_catalog::Source::File { path } => std::path::Path::new(path).parent().map(|d| d.to_string_lossy().to_string()),
        lightcraft_catalog::Source::Demo { .. } => None,
    };
    let work = if o.format.is_rendered() {
        let (w, h) = output_size(p, o);
        let mut meta = export_metadata(p, o);
        if let (Some(meta), Some(original)) = (&mut meta, &o.source_metadata) {
            meta.fill_missing(original);
            if let (Some(gps), Some(old)) = (&mut meta.gps, &original.gps)
                && gps.latitude == old.latitude
                && gps.longitude == old.longitude
            {
                gps.altitude = old.altitude;
            }
            // Orientation and dimensions describe the new, upright rendered pixels.
            meta.orientation = Some(lightcraft_meta::Orientation::Normal);
            meta.width = Some(w as u32);
            meta.height = Some(h as u32);
        }
        let (space, depth) =
            if o.source_encoding.is_some() { (OutputSpace::Rec2020, OutputDepth::F32Linear) } else { (o.effective_space(), o.effective_depth()) };
        let (nw, nh) = lightcraft_pipeline::native_output_size(p.width.max(1) as usize, p.height.max(1) as usize, &p.develop);
        let native = (nw.round().max(1.0) as usize, nh.round().max(1.0) as usize);
        let upscale = if o.enhance.is_some() && w > native.0 && h > native.1 {
            Some((session.enhance.host.clone().ok_or("AI enlargement is not available in this host")?, (w, h)))
        } else {
            None
        };
        // A chosen kernel develops at full size (8-bit outputs at 16 bits, so the resize reads
        // undithered samples) and resizes afterwards; otherwise the renderer scales.
        let resample = o.resample.filter(|_| upscale.is_none() && (w, h) != native).map(|k| (k, (w, h)));
        let render_depth = if resample.is_some() && depth == OutputDepth::U8 { OutputDepth::U16 } else { depth };
        let cost = match resample {
            Some(_) => render_cost((p.width as usize, p.height as usize), native, render_depth) + pixels((w, h)) * 12,
            None => render_cost((p.width as usize, p.height as usize), (w, h), depth),
        };
        if let Some(why) = too_big_for_memory((w, h), native.0.saturating_mul(native.1), cost, crate::memory::total_ram()) {
            return Err(format!("{}: {why}", p.file_name));
        }
        let job = if upscale.is_some() {
            session.export_job(id, native.0, native.1, OutputSpace::Srgb, OutputDepth::U8)?
        } else if resample.is_some() {
            session.export_job(id, native.0, native.1, space, render_depth)?
        } else {
            session.export_job(id, w, h, space, depth)?
        };
        Work::Render(Box::new(RenderWork { job, upscale, meta, opts: o.clone(), resample, cost }))
    } else {
        let lightcraft_catalog::Source::File { path } = &p.source else {
            return Err(format!("{} is a generated demo photo: it has no original file to export", p.file_name));
        };
        Work::File {
            path: path.clone(),
            read: session.media.file_bytes.clone(),
            packet: { crate::sidecar::sidecar_packet(&photo_for_sidecar(p, o.include_people_names, o.remove_location), &session.catalog) },
            dng: (o.format == ExportFormat::Dng).then_some(o.dng_compression),
            remove_location: o.remove_location,
            label: p.file_name.clone(),
            size: (p.width as usize, p.height as usize),
        }
    };
    Ok(PreparedExport { photo: id, file_name, subfolder: None, work, guard, source_dir })
}

/// Why an export of `out` pixels, from a photo of `native_px` pixels at its own size, can't be made
/// on a computer with `ram` bytes, given its working memory `cost`: an export enlarged past the
/// photo's size that needs more memory than the computer has would fail to allocate, which ends
/// the process rather than the export. Full-size and smaller exports, and computers whose memory
/// isn't known, are never refused.
pub(super) fn too_big_for_memory(out: (usize, usize), native_px: usize, cost: u64, ram: Option<usize>) -> Option<String> {
    let ram = ram? as u64;
    if out.0.saturating_mul(out.1) <= native_px || cost <= ram {
        return None;
    }
    let gb = |b: u64| b as f64 / (1u64 << 30) as f64;
    Some(format!(
        "exporting at {} × {} needs about {:.0} GB of memory, more than this computer has ({:.0} GB); choose a smaller size",
        out.0,
        out.1,
        gb(cost).ceil(),
        gb(ram)
    ))
}

/// One export after its render stage ([`PreparedExport::develop`]), before encoding.
pub(crate) enum Developed {
    /// Rendered pixels, to encode in the requested format.
    Image(Box<RenderedExport>),
    /// The raw data of a DNG export, to write as a DNG.
    Raw { raw: Box<lightcraft_raw::RawImage>, packet: String, compression: lightcraft_raw::DngCompression },
    /// Nothing left to encode (an `Original` export: the file's own bytes).
    Done(Exported),
}

/// A rendered export waiting for its encoder.
pub(crate) struct RenderedExport {
    r: lightcraft_pipeline::Rendered,
    meta: Option<Metadata>,
    opts: ExportOptions,
}

impl Developed {
    /// Bytes it holds while it waits for the encoder, plus the encoder's working copy (what a
    /// pipeline keeps reserved for it after the render, [`lightcraft_sysmem::Permit::finish_heavy`]).
    pub(crate) fn reserved_bytes(&self) -> u64 {
        let held = match self {
            Developed::Image(img) => {
                let r = &img.r;
                let deep = r.deep.as_ref().map_or(0, |d| match &d.samples {
                    DeepSamples::U16(v) => v.len() * 2,
                    DeepSamples::F32(v) => v.len() * 4,
                });
                r.image.data.len() * 4 + deep
            }
            Developed::Raw { raw, .. } => raw.width * raw.height * raw.cpp.max(1) * 2,
            Developed::Done(e) => e.bytes.len() + e.sidecars.iter().map(|(_, b)| b.len()).sum::<usize>(),
        } as u64;
        match self {
            Developed::Done(_) => held,
            _ => held * 2,
        }
    }
}

/// Pixels assumed for a photo whose size the catalog doesn't know (a 24 MP camera).
pub(super) const UNKNOWN_PIXELS: u64 = 24_000_000;

fn pixels((w, h): (usize, usize)) -> u64 {
    if w == 0 || h == 0 { UNKNOWN_PIXELS } else { w as u64 * h as u64 }
}

/// Working memory of one rendered export, from its pixel counts: the decoded
/// source in f32 RGB (12 B/px) plus about two working copies in the develop pipeline (36 B/px of
/// the source), and at the output size the RGBA8 result, the encoder's copy and the encoded file
/// (about 12 B/px) plus the high-bit-depth samples when the format keeps them.
pub(crate) fn render_cost(source: (usize, usize), out: (usize, usize), depth: OutputDepth) -> u64 {
    let deep = match depth {
        OutputDepth::U8 => 0,
        OutputDepth::U16 => 6,
        OutputDepth::F32Linear => 12,
    };
    pixels(source) * 36 + pixels(out) * (12 + deep)
}

/// Working memory of an `Original` (the file's bytes, about 2 B/px for a raw) or `Dng` export
/// (the file's bytes, the decoded 16-bit raw data and the DNG written from it: about 8 B/px).
fn file_cost(size: (usize, usize), dng: bool) -> u64 {
    pixels(size) * if dng { 8 } else { 2 }
}

impl PreparedExport {
    pub fn run(self) -> Result<Exported, String> {
        self.run_with_ctl(&lightcraft_sysmem::JobCtl::new())
    }

    pub fn run_with_ctl(self, ctl: &lightcraft_sysmem::JobCtl) -> Result<Exported, String> {
        let (file_name, developed) = self.develop(ctl)?;
        encode_developed(file_name, developed, ctl)
    }

    /// The working memory this export is expected to need (for its heavy-job permit).
    pub fn estimated_bytes(&self) -> u64 {
        match &self.work {
            Work::Render(w) => w.cost,
            Work::File { size, dng, .. } => file_cost(*size, dng.is_some()),
        }
    }

    /// The render stage: read and decode the original, develop it (the GPU one batch item at a
    /// time, [`pipeline::run_render_job`]), enlarge it with AI if asked; or read the original /
    /// decode the raw data of an `Original` / `Dng` export. Encoding is [`encode_developed`].
    pub(crate) fn develop(self, ctl: &lightcraft_sysmem::JobCtl) -> Result<(String, Developed), String> {
        if ctl.is_cancelled() {
            return Err(crate::enhance::CANCELLED.to_owned());
        }
        let file_name = self.file_name;
        match self.work {
            Work::Render(w) => {
                let RenderWork { job, upscale, meta, opts, resample, cost: _ } = *w;
                let mut r = pipeline::run_render_job(job).rendered?;
                if let Some((kernel, size)) = resample {
                    if ctl.is_cancelled() {
                        return Err(crate::enhance::CANCELLED.to_owned());
                    }
                    let wants_deep = opts.source_encoding.is_some() || opts.effective_depth() != OutputDepth::U8;
                    resample_rendered(&mut r, kernel, size, wants_deep);
                }
                if let Some((host, target)) = upscale {
                    let request = crate::enhance::UpscaleRequest {
                        width: r.image.width,
                        height: r.image.height,
                        rgba: r.image.data.clone(),
                        target,
                        method: opts.enhance.ok_or("missing enhancement method")?.key().into(),
                    };
                    // AI enlargement (CPU inference or the ComfyUI GPU queue): one at a time
                    let pixels = {
                        let _one = pipeline::AI_GATE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                        host.upscale(&request, ctl)?
                    };
                    if pixels.len() != target.0 * target.1 {
                        return Err("AI enlargement returned an unexpected size".into());
                    }
                    r.image = Rgba8 { width: target.0, height: target.1, data: pixels };
                    r.deep = enhanced_samples(&mut r.image, &opts);
                }
                Ok((file_name, Developed::Image(Box::new(RenderedExport { r, meta, opts }))))
            }
            Work::File { path, read, packet, dng, remove_location, label, size } => {
                let bytes = match &read {
                    Some(r) => r(&path)?,
                    None => std::fs::read(&path).map_err(|e| format!("{path}: {e}"))?,
                };
                let Some(dng) = dng else {
                    // the file is copied byte for byte: refuse rather than leak a location it embeds
                    if remove_location {
                        let embeds_location = original_embeds_location(&bytes)
                            .map_err(|e| format!("{label}: can't verify location metadata in this original file: {e}"))?;
                        if embeds_location {
                            return Err(format!(
                                "{label}: can't remove the location from an original file that embeds location info; export as JPEG, TIFF or DNG instead"
                            ));
                        }
                    }
                    let e =
                        Exported { file_name: file_name.clone(), bytes, width: size.0, height: size.1, sidecars: vec![("xmp", packet.into_bytes())] };
                    return Ok((file_name, Developed::Done(e)));
                };
                if lightcraft_raw::probe(&bytes).is_none() {
                    return Err(format!("{label}: DNG export needs a raw photo"));
                }
                let raw = lightcraft_raw::decode(&bytes).map_err(|e| format!("{label}: {e}"))?;
                drop(bytes);
                let compression = match dng {
                    DngCompression::Lossless => lightcraft_raw::DngCompression::Lj92 { tile: 256 },
                    DngCompression::Deflate => lightcraft_raw::DngCompression::Deflate { tile: 256, half: false },
                    DngCompression::Uncompressed => lightcraft_raw::DngCompression::Uncompressed,
                };
                Ok((file_name, Developed::Raw { raw: Box::new(raw), packet, compression }))
            }
        }
    }
}

/// The encode stage of one export: the file's bytes from its [`Developed`] result.
pub(crate) fn encode_developed(file_name: String, developed: Developed, ctl: &lightcraft_sysmem::JobCtl) -> Result<Exported, String> {
    match developed {
        Developed::Image(img) => {
            let RenderedExport { r, meta, opts } = *img;
            if ctl.is_cancelled() {
                return Err(crate::enhance::CANCELLED.to_owned());
            }
            let bytes = encode_rendered(&r, &opts, meta.as_ref())?;
            Ok(Exported { file_name, bytes, width: r.image.width, height: r.image.height, sidecars: Vec::new() })
        }
        Developed::Raw { raw, packet, compression } => {
            let dng = lightcraft_raw::write_dng(&raw, &lightcraft_raw::DngWriteOptions { xmp: Some(packet), compression, ..Default::default() })
                .map_err(|e| e.to_string())?;
            Ok(Exported { file_name, bytes: dng, width: raw.width, height: raw.height, sidecars: Vec::new() })
        }
        Developed::Done(e) => Ok(e),
    }
}
