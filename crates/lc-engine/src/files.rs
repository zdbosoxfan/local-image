//! Real files: probing (dimensions + metadata for import) and loading (decode → linear Rec.2020,
//! oriented, downscaled to the requested level). Standard formats go through `lightcraft-codecs`,
//! camera raws through `lightcraft-raw` (demosaic + DNG colour model).

use std::sync::Arc;

use li_color::cct::xy_to_temp_tint;
use li_raster::RgbFrame;
use li_raster::resample::Filter;
use lightcraft_catalog::{MediaKind, Meta};
use lightcraft_geom::Orientation;
use lightcraft_pipeline::SourceInfo;
use lightcraft_preview::Hasher128;

use crate::media::{FileLoader, FileProbe, PreviewLoader, ProbeInfo};

/// Desktop photo imports include 50 MP Sony and 100 MP DJI exports. Keep the codec's
/// conservative generic default, but admit these stills explicitly here. At this ceiling,
/// one full linear RGB buffer is 1.5 GiB, below the RAW decoder's 2 GiB output ceiling.
/// JPEG previews retain DCT scaling before colour conversion; other formats decode fully.
const MAX_STILL_IMAGE_PIXELS: u64 = lightcraft_codecs::DESKTOP_PHOTO_PIXELS;

fn still_image_decode_options(max_edge: usize) -> lightcraft_codecs::DecodeOptions {
    let edge = u32::try_from(max_edge).unwrap_or(u32::MAX);
    lightcraft_codecs::DecodeOptions { max_size: Some((edge, edge)), max_pixels: MAX_STILL_IMAGE_PIXELS }
}

fn meta_of(m: &lightcraft_meta::Metadata) -> (Meta, Option<String>) {
    let shutter = m.exposure_time.map(|t| if t >= 1.0 { format!("{t:.0}") } else { format!("1/{:.0}", 1.0 / t) }).unwrap_or_default();
    let camera = [m.make.clone().unwrap_or_default(), m.model.clone().unwrap_or_default()].join(" ").trim().to_string();
    let meta = Meta {
        serial_number: m.serial_number.clone(),
        sony: m.sony.clone(),
        dji: m.dji.clone(),
        camera,
        lens: m.lens_model.clone().unwrap_or_default(),
        focal_mm: m.focal_length.map(|f| f as f32),
        aperture: m.f_number.map(|f| f as f32),
        shutter,
        iso: m.iso,
        location: m.sublocation.clone().unwrap_or_default(),
        city: m.city.clone().unwrap_or_default(),
        state: m.state.clone().unwrap_or_default(),
        country: m.country.clone().unwrap_or_default(),
        alt_text: m.alt_text.clone().unwrap_or_default(),
        extended_description: m.extended_description.clone().unwrap_or_default(),
        gps: m.gps.as_ref().map(|g| (g.latitude, g.longitude)),
        title: m.title.clone().unwrap_or_default(),
        caption: m.caption.clone().unwrap_or_default(),
        copyright: m.copyright.clone().unwrap_or_default(),
        copyright_status: lightcraft_catalog::CopyrightStatus::from_marked(m.copyright_marked),
        usage_terms: m.usage_terms.clone().unwrap_or_default(),
        copyright_url: m.copyright_url.clone().unwrap_or_default(),
        creator: m.artist.clone().unwrap_or_default(),
        creators: m.creators.clone(),
        keywords: m.keywords.clone(),
        regions: m.regions.clone(),
        person_ids: Vec::new(),
    };
    (meta, m.capture_time.as_ref().map(|d| d.to_iso()))
}

/// Lossless handoff of Sony's raw-IFD sample curves to the shared lens boundary.
pub fn sony_lens(sony: Option<&lightcraft_meta::SonyMetadata>) -> Option<lightcraft_develop::SonyLensData> {
    let lens = sony?.lens_correction.as_ref()?;
    Some(lightcraft_develop::SonyLensData {
        distortion: lens.distortion.clone(),
        ca_red: lens.ca_red.clone(),
        ca_blue: lens.ca_blue.clone(),
        vignetting: lens.vignetting.clone(),
    })
}

fn built_in_dji_dng(format: lightcraft_raw::RawFormat, metadata: &lightcraft_meta::Metadata) -> bool {
    let make = metadata.make.as_deref().unwrap_or_default();
    let model = metadata.model.as_deref().unwrap_or_default();
    format == lightcraft_raw::RawFormat::Dng
        && (make.eq_ignore_ascii_case("DJI")
            || metadata.dji.is_some()
            || (make.eq_ignore_ascii_case("Hasselblad") && model.starts_with('L') && model.contains("D-")))
}

/// Lens corrections embedded in a DNG's `OpcodeList3` (`WarpRectilinear`, `FixVignetteRadial`), re-expressed for
/// the default-cropped, EXIF-oriented image. Vendor metadata uses the same shared model.
pub fn embedded_lens(raw: &lightcraft_raw::RawInfo) -> Option<lightcraft_develop::EmbeddedLens> {
    use lightcraft_develop::{EmbeddedLens, EmbeddedVignette, EmbeddedWarp};
    use lightcraft_geom::Point;
    // A mixed list must execute in its declared order in the raw pipeline.
    // Splitting warp/vignette out would move them past GainMap or other operations.
    // Multiple warps/vignettes likewise cannot be represented by a single profile.
    // DJI's built-in DNG operations define the developed camera image. Apply
    // them in camera RGB in the raw path, even when the editable lens-profile
    // checkbox is off. Splitting them into a later working-RGB correction would
    // change per-plane CA, GainMap order, and profile-table interpolation.
    let dji_dng = built_in_dji_dng(raw.format, &raw.metadata);
    if dji_dng || raw.metadata.lens_corrections_baked {
        return None;
    }
    let warps = raw
        .opcodes
        .list3
        .iter()
        .filter(|op| matches!(op, lightcraft_raw::Opcode::WarpRectilinear { .. } | lightcraft_raw::Opcode::WarpFisheye { .. }))
        .count();
    let vignettes = raw.opcodes.list3.iter().filter(|op| matches!(op, lightcraft_raw::Opcode::FixVignetteRadial { .. })).count();
    let per_plane_warp = raw.opcodes.list3.iter().any(|op| match op {
        lightcraft_raw::Opcode::WarpRectilinear { planes, .. } => planes.first().is_some_and(|first| planes.iter().any(|p| p != first)),
        lightcraft_raw::Opcode::WarpFisheye { planes, .. } => planes.windows(2).any(|p| p[0] != p[1]),
        _ => false,
    });
    let warp_before_vignette = raw
        .opcodes
        .list3
        .iter()
        .position(|op| matches!(op, lightcraft_raw::Opcode::WarpRectilinear { .. } | lightcraft_raw::Opcode::WarpFisheye { .. }))
        .zip(raw.opcodes.list3.iter().position(|op| matches!(op, lightcraft_raw::Opcode::FixVignetteRadial { .. })))
        .is_some_and(|(warp, vignette)| warp < vignette);
    if raw.opcodes.list3.iter().any(|op| !is_lens_opcode(op)) || warps > 1 || vignettes > 1 || warp_before_vignette || per_plane_warp {
        return None;
    }
    let (aw, ah) = (raw.active_area.width as f64, raw.active_area.height as f64);
    if aw < 2.0 || ah < 2.0 {
        return None;
    }
    let c = raw.crop.clipped(raw.active_area.width, raw.active_area.height);
    let (cx0, cy0, cw, ch) =
        if c.width == 0 || c.height == 0 { (0.0, 0.0, aw, ah) } else { (c.x as f64, c.y as f64, c.width as f64, c.height as f64) };
    let long = cw.max(ch);
    // Adobe SDK uses half-open image bounds for both opcodes. Warp evaluates
    // integer indices; vignette evaluates pixel centres. Our geometry uses centres.
    let centre = |rel: [f64; 2], index_offset: f64| -> (Point, f64) {
        let (px, py) = (rel[0] * aw, rel[1] * ah);
        let m = [(0.0, 0.0), (aw, 0.0), (0.0, ah), (aw, ah)].iter().map(|&(x, y)| (x - px).hypot(y - py)).fold(0.0, f64::max).max(1e-9);
        (Point::new((px + index_offset - cx0) / cw, (py + index_offset - cy0) / ch), m / long)
    };
    let mut lens = EmbeddedLens::default();
    for op in &raw.opcodes.list3 {
        match op {
            lightcraft_raw::Opcode::WarpRectilinear { planes, center } if matches!(planes.len(), 1 | 3) && lens.warp.is_none() => {
                let (center, radius) = centre(*center, 0.5);
                let p = |i: usize| planes[i.min(planes.len() - 1)];
                lens.warp = Some(EmbeddedWarp { fisheye: false, planes: [p(0), p(1), p(2)], center, radius });
            }
            lightcraft_raw::Opcode::WarpFisheye { planes, center } if matches!(planes.len(), 1 | 3) && lens.warp.is_none() => {
                let (px, py) = (center[0] * (aw - 1.0), center[1] * (ah - 1.0));
                let m = px.max(aw - 1.0 - px).hypot(py.max(ah - 1.0 - py));
                let center = Point::new((px + 0.5 - cx0) / cw, (py + 0.5 - cy0) / ch);
                let radius = m / long;
                let p = |i: usize| {
                    let c = planes[i.min(planes.len() - 1)];
                    [c[0], c[1], c[2], c[3], 0.0, 0.0]
                };
                lens.warp = Some(EmbeddedWarp { fisheye: true, planes: [p(0), p(1), p(2)], center, radius });
            }
            lightcraft_raw::Opcode::FixVignetteRadial { k, center } if lens.vignette.is_none() => {
                let (center, radius) = centre(*center, 0.0);
                lens.vignette = Some(EmbeddedVignette { k: *k, center, radius });
            }
            _ => {}
        }
    }
    if lens.warp.is_none() && lens.vignette.is_none() {
        lens.radial = raw.vendor_lens.as_ref().and_then(|data| crate::embedded_lens::radial(data, cw, ch));
        lens.radial?;
    }
    Some(lightcraft_pipeline::optics::reorient_lens(&lens, raw.orientation, cw, ch))
}

pub(crate) fn is_lens_opcode(op: &lightcraft_raw::Opcode) -> bool {
    matches!(
        op,
        lightcraft_raw::Opcode::WarpRectilinear { .. }
            | lightcraft_raw::Opcode::WarpFisheye { .. }
            | lightcraft_raw::Opcode::FixVignetteRadial { .. }
    )
}

fn ext_upper(name: &str) -> String {
    std::path::Path::new(name).extension().map(|e| e.to_string_lossy().to_uppercase()).unwrap_or_default()
}

/// Probe a file's bytes: kind, dimensions (oriented), metadata. Raws are described from their
/// headers ([`lightcraft_raw::probe_info`]): no pixel data is decompressed.
pub fn probe_bytes(name: &str, bytes: &[u8]) -> Result<ProbeInfo, String> {
    // content identity: BLAKE3
    describe_bytes(name, bytes, Some(lightcraft_preview::ContentId::of(bytes).to_string()))
}

fn describe_bytes(name: &str, bytes: &[u8], content_hash: Option<String>) -> Result<ProbeInfo, String> {
    let (m, xmp) = lightcraft_meta::extract_with_xmp(bytes);
    let (meta, captured) = meta_of(&m);
    if lightcraft_raw::probe(bytes).is_some() {
        let mut raw = match lightcraft_raw::probe_info(bytes) {
            Ok(r) => r,
            Err(lightcraft_raw::RawError::Unsupported(why)) => {
                // a raw variant we can't decode yet: describe it from its embedded preview
                let (w, h) = embedded_preview_size(bytes).ok_or(format!("unsupported raw ({why}) without an embedded preview"))?;
                return Ok(ProbeInfo {
                    width: w,
                    height: h,
                    format: ext_upper(name),
                    kind: MediaKind::Raw,
                    file_size: bytes.len() as u64,
                    captured,
                    meta,
                    as_shot_wb: None,
                    content_hash,
                    xmp,
                    preview_only: Some(why),
                    ..Default::default()
                });
            }
            Err(e) => return Err(e.to_string()),
        };
        lightcraft_raw::camera_matrices::fill(
            &mut raw.color,
            raw.metadata.make.as_deref().unwrap_or_default(),
            raw.metadata.model.as_deref().unwrap_or_default(),
        );
        let (mut w, mut h) = (raw.crop.width.max(1) as u32, raw.crop.height.max(1) as u32);
        if w <= 1 || h <= 1 {
            (w, h) = (raw.active_area.width as u32, raw.active_area.height as u32);
        }
        if raw.orientation.swaps_axes() {
            std::mem::swap(&mut w, &mut h);
        }
        let (t, tint) = xy_to_temp_tint(lightcraft_raw::color::as_shot_white_xy_of(&raw));
        // Vendor RGB multipliers do not identify an absolute illuminant without camera calibration.
        let relative = crate::camera_preview::file_local_look(raw.format) && !lightcraft_raw::color::has_matrix(&raw.color);
        let as_shot_wb = Some(if relative { (6500.0, 0.0) } else { (t.round(), tint.round()) });
        let embedded_lens = embedded_lens(&raw);
        return Ok(ProbeInfo {
            embedded_lens,
            measured_wb: !relative,
            width: w,
            height: h,
            format: ext_upper(name),
            kind: MediaKind::Raw,
            file_size: bytes.len() as u64,
            captured,
            meta,
            as_shot_wb,
            content_hash,
            xmp,
            preview_only: None,
            modified: None,
        });
    }
    let fmt = lightcraft_codecs::sniff(bytes).ok_or("unrecognized file format")?;
    if !fmt.can_decode() {
        return Err(format!("{fmt:?} files are not supported yet"));
    }
    let d = lightcraft_codecs::decode(bytes, still_image_decode_options(64)).map_err(|e| e.to_string())?;
    let o = Orientation::from_exif(d.orientation);
    let (mut w, mut h) = (d.source_width, d.source_height);
    if o.swaps_axes() {
        std::mem::swap(&mut w, &mut h);
    }
    let format = match ext_upper(name).as_str() {
        "JPG" | "JPEG" => "JPEG".to_string(),
        "" => format!("{fmt:?}").to_uppercase(),
        e => e.to_string(),
    };
    Ok(ProbeInfo {
        width: w,
        height: h,
        format,
        kind: MediaKind::Image,
        file_size: bytes.len() as u64,
        captured,
        meta,
        as_shot_wb: None,
        measured_wb: false,
        content_hash,
        embedded_lens: None,
        xmp: None,
        preview_only: None,
        modified: None,
    })
}

/// Sensor clip level (normalised) for highlight reconstruction.
const HIGHLIGHT_CLIP: f32 = 0.99;

/// Read the unmodified sensor before any reconstruction. Each CFA tile records the
/// fraction of RGB channels whose samples are clipped (green samples averaged).
fn sensor_clip_confidence(raw: &lightcraft_raw::RawImage) -> Option<li_raster::GrayFrame> {
    let n = raw.normalized_samples().ok()?;
    let crop = raw.crop.clipped(n.width, n.height);
    let crop = if crop.width > 1 && crop.height > 1 { crop } else { lightcraft_raw::Rect { x: 0, y: 0, width: n.width, height: n.height } };
    let tile = n.cfa.as_ref().map_or(1, |c| c.width.max(c.height));
    let confidence = |x: usize, y: usize| {
        let mut counts = [0usize; 3];
        let mut clipped = [0usize; 3];
        for dy in 0..tile {
            for dx in 0..tile {
                let xx = (crop.x + x * tile + dx).min(crop.x + crop.width - 1);
                let yy = (crop.y + y * tile + dy).min(crop.y + crop.height - 1);
                if n.cpp == 3 {
                    for c in 0..3 {
                        counts[c] += 1;
                        clipped[c] += usize::from(n.get(xx, yy, c) >= HIGHLIGHT_CLIP);
                    }
                } else if let Some(cfa) = &n.cfa {
                    let c = cfa.color_at(xx, yy).min(2) as usize;
                    counts[c] += 1;
                    clipped[c] += usize::from(n.get(xx, yy, 0) >= HIGHLIGHT_CLIP);
                } else {
                    let v = usize::from(n.get(xx, yy, 0) >= HIGHLIGHT_CLIP);
                    counts = [1; 3];
                    clipped = [v; 3];
                }
            }
        }
        (0..3).map(|c| clipped[c] as f32 / counts[c].max(1) as f32).sum::<f32>() / 3.
    };
    // a pass over the whole sensor on every raw load: rows of tiles are independent
    use rayon::prelude::*;
    let mut p = li_raster::GrayFrame::new(crop.width.div_ceil(tile), crop.height.div_ceil(tile));
    let width = p.width;
    p.data.par_chunks_mut(width.max(1)).enumerate().for_each(|(y, row)| row.iter_mut().enumerate().for_each(|(x, v)| *v = confidence(x, y)));
    Some(p)
}

/// The largest block size to bin a raw's mosaic by for a source of at most `max_edge` pixels:
/// the binned image must keep at least 90 % of `max_edge` (a 16 MP sensor still bins 2× for the
/// 2560 px preview). X-Trans can only bin 3× (its 6×6 pattern), and its full demosaic is ~5× the
/// cost of Bayer's, so 3× is accepted down to 75 % (a 24 MP X-Trans preview is then ~2000 px
/// instead of a ~1.2 s full demosaic; zooming in still uses the full-size source).
/// `None` = demosaic at full size.
pub fn bin_factor(raw: &lightcraft_raw::RawImage, max_edge: usize) -> Option<usize> {
    let c = raw.crop.clipped(raw.active_area.width, raw.active_area.height);
    let long = if c.width > 1 && c.height > 1 { c.width.max(c.height) } else { raw.active_area.width.max(raw.active_area.height) };
    let need = (max_edge.saturating_mul(9) / 10).max(1);
    let need3 = (max_edge.saturating_mul(3) / 4).max(1);
    [8usize, 6, 4, 3, 2].into_iter().find(|&k| raw.can_bin(k) && (long / k >= need || (k == 3 && !raw.can_bin(2) && long / k >= need3)))
}

/// How a raw file is decoded: the photo's Raw Processing settings that change decoding
/// (demosaic, highlight reconstruction, measuring the capture-sharpening radius).
/// Automatic chooses from measured sensor noise; explicit saved methods retain their meaning.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RawOptions {
    pub demosaic: lightcraft_develop::Demosaic,
    /// Dual demosaic threshold 0..1 (0 unless the method is dual).
    pub dual_threshold: f32,
    pub highlights: lightcraft_develop::HighlightMode,
    /// Measure the capture-sharpening radius from the mosaic ([`SourceInfo::capture_radius`]).
    pub capture_radius: bool,
    /// Stable identifier of the selected camera profile (0 = automatic, 1 = matrix).
    pub camera_profile: u64,
    pub profile_generation: u64,
}

impl RawOptions {
    /// The options `s` asks for (the defaults while the Raw section is switched off).
    pub fn of(s: &lightcraft_develop::DevelopSettings) -> RawOptions {
        let r = &s.raw;
        if !s.section_enabled("raw") {
            return RawOptions {
                camera_profile: crate::dcp_profiles::selection_key(&s.camera_profile),
                profile_generation: crate::dcp_profiles::cache_key(),
                ..Default::default()
            };
        }
        RawOptions {
            camera_profile: crate::dcp_profiles::selection_key(&s.camera_profile),
            profile_generation: crate::dcp_profiles::cache_key(),
            demosaic: r.demosaic,
            dual_threshold: if r.demosaic.is_dual() { (r.dual_threshold / 100.0).clamp(0.0, 1.0) as f32 } else { 0.0 },
            highlights: r.highlights,
            capture_radius: r.capture.enabled && r.capture.radius <= 0.0,
        }
    }

    pub fn is_default(&self) -> bool {
        *self == RawOptions::default()
    }

    /// Identifies decoded sources made with these options (0 for the defaults).
    pub fn key(&self) -> u64 {
        if self.is_default() {
            return 0;
        }
        let k = Hasher128::new().str(&format!("{self:?}")).finish();
        (k.0 as u64) | 1
    }

    /// The demosaic method for a source of at most `max_edge` pixels (thumbnails stay bilinear).
    fn method(&self, max_edge: usize) -> lightcraft_raw::Method {
        use lightcraft_develop::Demosaic as D;
        use lightcraft_raw::Method as M;
        if max_edge <= 600 {
            return M::Bilinear;
        }
        match self.demosaic {
            D::Auto => M::Auto,
            D::Lmmse => M::Lmmse,
            D::Dcb => M::Dcb,
            D::Markesteijn3 => M::Markesteijn3,
            D::DualMarkesteijnVng => M::DualMarkesteijnVng,
            D::XtransDirectional => M::XtransDirectional,
            D::Ahd => M::Ahd,
            D::Rcd => M::Rcd,
            D::DualRcd => M::DualRcd,
            D::DualRcdVng => M::DualRcdVng,
            D::Vng4 => M::Vng4,
            D::Amaze => M::Amaze,
            D::DualAmazeVng => M::DualAmazeVng,
            D::Ppg => M::Ppg,
            D::Bilinear => M::Bilinear,
        }
    }
}

/// Decode a file into a linear Rec.2020 image no larger than `max_edge`, oriented.
///
/// Runs on a rayon worker: its many short parallel loops then start on the worker's own queue
/// instead of each one waking the pool from outside and waiting for it (which costs more than the
/// loops themselves when the machine is busy).
pub fn load_bytes(bytes: &[u8], max_edge: usize) -> Result<(RgbFrame, SourceInfo), String> {
    load_bytes_with(bytes, max_edge, &RawOptions::default())
}

/// [`load_bytes`] decoding a raw file with `opts`.
pub fn load_bytes_with(bytes: &[u8], max_edge: usize, opts: &RawOptions) -> Result<(RgbFrame, SourceInfo), String> {
    rayon::scope(|_| load_bytes_now(std::borrow::Cow::Borrowed(bytes), max_edge, opts))
}

/// [`load_bytes`] taking the file's bytes: a raw file's bytes are freed as soon as it is decoded
/// (less memory held while it is developed).
pub fn load_vec(bytes: Vec<u8>, max_edge: usize) -> Result<(RgbFrame, SourceInfo), String> {
    load_vec_with(bytes, max_edge, &RawOptions::default())
}

/// [`load_vec`] decoding a raw file with `opts`.
pub fn load_vec_with(bytes: Vec<u8>, max_edge: usize, opts: &RawOptions) -> Result<(RgbFrame, SourceInfo), String> {
    let opts = *opts;
    rayon::scope(move |_| load_bytes_now(std::borrow::Cow::Owned(bytes), max_edge, &opts))
}

fn load_bytes_now(bytes: std::borrow::Cow<'_, [u8]>, max_edge: usize, opts: &RawOptions) -> Result<(RgbFrame, SourceInfo), String> {
    if lightcraft_raw::probe(&bytes).is_some() {
        let allowance = lightcraft_sysmem::heavy_jobs().headroom().min(usize::MAX as u64) as usize;
        let mut raw = match lightcraft_raw::decode_with_memory_budget(&bytes, allowance) {
            Ok(r) => r,
            Err(lightcraft_raw::RawError::Unsupported(why)) => {
                // show the camera's embedded JPEG (rendered, not raw) until the variant is supported
                return load_embedded_preview(&bytes, max_edge).ok_or(format!("unsupported raw ({why}) without an embedded preview"));
            }
            Err(e) => return Err(e.to_string()),
        };
        crate::dcp_profiles::apply_selected(&mut raw, opts.camera_profile);
        lightcraft_raw::camera_matrices::fill(
            &mut raw.color,
            raw.metadata.make.as_deref().unwrap_or_default(),
            raw.metadata.model.as_deref().unwrap_or_default(),
        );
        let xy = lightcraft_raw::color::as_shot_white_xy(&raw);
        let t = lightcraft_raw::color::camera_transform(&raw, xy);
        // Keep the actual source uncorrected. Only the private camera-look
        // fitting proxy is corrected to match the camera's processed JPEG.
        let lens = embedded_lens(&raw.info());
        let builtin_shading = built_in_dji_dng(raw.format, &raw.metadata)
            && raw
                .opcodes
                .list1
                .iter()
                .chain(&raw.opcodes.list2)
                .chain(&raw.opcodes.list3)
                .any(|op| matches!(op, lightcraft_raw::Opcode::GainMap { .. } | lightcraft_raw::Opcode::FixVignetteRadial { .. }));
        let embedded_lens_baked =
            raw.metadata.lens_corrections_baked || builtin_shading || (lens.is_none() && raw.opcodes.list3.iter().any(is_lens_opcode));
        let embedded_ca_baked = raw.metadata.chromatic_aberration_baked
            || embedded_lens_baked
                && raw.opcodes.list3.iter().any(|op| match op {
                    lightcraft_raw::Opcode::WarpRectilinear { planes, .. } => planes.windows(2).any(|p| p[0] != p[1]),
                    lightcraft_raw::Opcode::WarpFisheye { planes, .. } => planes.windows(2).any(|p| p[0] != p[1]),
                    _ => false,
                });
        if lens.is_some() {
            raw.opcodes.list3.retain(|op| !is_lens_opcode(op));
        }
        let camera_look = crate::camera_preview::fit_preview(&raw, &bytes, &t, lens.as_ref());
        drop(bytes);
        // Previews and thumbnails bin the mosaic straight to (about) the size they need; only
        // larger levels (exports, 1:1) demosaic the whole sensor.
        let t0 = web_time::Instant::now();
        let clip_confidence = sensor_clip_confidence(&raw);
        // capture sharpening: the sensor's blur, measured on the mosaic when asked for
        let capture_radius = if opts.capture_radius { raw.normalized().ok().map(|n| lightcraft_raw::capture::capture_radius(&n)) } else { None };
        let capture_threshold = lightcraft_pipeline::capture::default_threshold(raw.white_at(0), raw.metadata.iso);
        let sensor_long = {
            let c = raw.crop.clipped(raw.active_area.width, raw.active_area.height);
            if c.width > 1 && c.height > 1 { c.width.max(c.height) } else { raw.active_area.width.max(raw.active_area.height) }
        };
        let binned = match bin_factor(&raw, max_edge) {
            Some(k) if opts.highlights == lightcraft_develop::HighlightMode::Segmentation => raw
                .develop_binned_with(k, HIGHLIGHT_CLIP, |n, scale| {
                    lightcraft_raw::highlight::segmentation(
                        n,
                        t.wb,
                        HIGHLIGHT_CLIP,
                        &lightcraft_raw::highlight::SegmentationOptions::default().at_scale(scale),
                    );
                })
                .map_err(|e| e.to_string())?,
            Some(k) => raw.develop_binned(k, HIGHLIGHT_CLIP).map_err(|e| e.to_string())?,
            None => None,
        };
        let mut img = match binned {
            Some(img) => img,
            None => {
                let dopts = lightcraft_raw::DemosaicOptions { dual_threshold: opts.dual_threshold };
                raw.develop_with_cfa(opts.method(max_edge), &dopts, |n| {
                    if opts.highlights == lightcraft_develop::HighlightMode::Segmentation {
                        lightcraft_raw::highlight::segmentation(n, t.wb, HIGHLIGHT_CLIP, &lightcraft_raw::highlight::SegmentationOptions::default());
                    }
                })
                .map_err(|e| e.to_string())?
            }
        };
        // the samples aren't needed any more (the colour model below reads only the tags)
        raw.data = lightcraft_raw::RawData::U16(Vec::new());
        let mut stages = vec![("develop", t0.elapsed())];
        stages.push(("transform", t0.elapsed()));
        match opts.highlights {
            lightcraft_develop::HighlightMode::Reconstruct => drop(lightcraft_raw::highlight::reconstruct(&mut img, t.wb, HIGHLIGHT_CLIP)),
            lightcraft_develop::HighlightMode::Opposed => drop(lightcraft_raw::highlight::opposed(&mut img, t.wb, HIGHLIGHT_CLIP)),
            lightcraft_develop::HighlightMode::Segmentation => {
                // Linear RGB / monochrome raws have no CFA to segment.
                if raw.cpp != 1 || raw.cfa.is_none() {
                    lightcraft_raw::highlight::opposed(&mut img, t.wb, HIGHLIGHT_CLIP);
                }
            }
            lightcraft_develop::HighlightMode::Clip => lightcraft_raw::highlight::clip_neutral(&mut img, t.wb, HIGHLIGHT_CLIP),
        }
        stages.push(("highlights", t0.elapsed()));
        let m = camera_look.as_ref().map(|p| p.matrix.mul(&t.matrix)).unwrap_or(t.matrix).to_f32();
        // Nonlinear colour adjustments must follow camera-plane lens sampling.
        // Lensfun CA can be enabled later even when no embedded profile exists.
        let source_hue_sat = camera_look.as_ref().and_then(|p| p.hue_sat.clone()).map(Arc::new);
        let gain = 2f32.powf(t.baseline_exposure as f32);
        let wb = t.wb;
        // Linear camera conversion precedes deferred nonlinear profile looks.
        img.map_in_place(|p| {
            let c = [p[0] * wb[0], p[1] * wb[1], p[2] * wb[2]];
            let rgb = [
                m[0][0] * c[0] + m[0][1] * c[1] + m[0][2] * c[2],
                m[1][0] * c[0] + m[1][1] * c[1] + m[1][2] * c[2],
                m[2][0] * c[0] + m[2][1] * c[1] + m[2][2] * c[2],
            ];
            rgb.map(|v| v * gain)
        });
        stages.push(("colour", t0.elapsed()));
        let img = img.fit(max_edge, max_edge, Filter::Box);
        let sensor_scale = (sensor_long as f32 / img.width.max(img.height).max(1) as f32).max(1.0);
        stages.push(("fit", t0.elapsed()));
        let clip_confidence = clip_confidence.map(|p| {
            let p = p.resize(img.width, img.height, Filter::Bilinear).into_oriented(raw.orientation);
            Arc::new(lightcraft_pipeline::ClipConfidence { width: p.width, height: p.height, data: p.data })
        });
        let img = img.into_oriented(raw.orientation);
        stages.push(("orient", t0.elapsed()));
        if lightcraft_pipeline::profiling() {
            let mut prev = std::time::Duration::ZERO;
            let parts: Vec<String> = stages
                .iter()
                .map(|(n, t)| {
                    let d = *t - prev;
                    prev = *t;
                    format!("{n} {:.1}", d.as_secs_f64() * 1e3)
                })
                .collect();
            log::debug!("[profile] raw source {}×{} (max {max_edge}, ms after decode): {}", img.width, img.height, parts.join(", "));
        }
        let (temp, tint) = xy_to_temp_tint(xy);
        let relative = crate::camera_preview::file_local_look(raw.format) && t.matrix_is_fallback;
        let camera_tone = camera_look.as_ref().map(|p| p.tone).or_else(|| raw.color.profile.tone_curve.as_ref().and_then(dng_tone_curve));
        let (temp, tint) = if relative { (6500.0, 0.0) } else { (temp.round(), tint.round()) };
        let normalized_color = lightcraft_raw::color::normalized_color(&raw);
        let normalization_ratio = lightcraft_raw::color::normalization_ratios(&raw);
        return Ok((
            img,
            SourceInfo {
                raw: true,
                camera_wb_presets: if !relative {
                    lightcraft_raw::camera_data::wb_presets(
                        raw.metadata.make.as_deref().unwrap_or_default(),
                        raw.metadata.model.as_deref().unwrap_or_default(),
                    )
                    .iter()
                    .filter(|p| p.tuning == 0)
                    .map(|p| {
                        let mut neutral = p.multipliers.map(|v| 1.0 / v as f64);
                        for i in 0..3 {
                            neutral[i] *= normalization_ratio[i];
                        }
                        let xy = lightcraft_raw::color::neutral_to_xy(&normalized_color, neutral);
                        let (temp, tint) = xy_to_temp_tint(xy);
                        lightcraft_pipeline::CameraWbPreset {
                            name: p.name.clone(),
                            temp: temp.clamp(2000.0, 50000.0),
                            tint: tint.clamp(-150.0, 150.0),
                        }
                    })
                    .collect()
                } else {
                    vec![]
                },
                source_orientation: raw.orientation,
                embedded_lens_baked,
                embedded_ca_baked,
                lcp: None,
                clip_confidence,
                raw_clip_level: Some(HIGHLIGHT_CLIP),
                camera_noise: if source_hue_sat.is_none() && raw.color.profile.is_empty() {
                    lightcraft_raw::camera_data::noise_profile(
                        raw.metadata.make.as_deref().unwrap_or_default(),
                        raw.metadata.model.as_deref().unwrap_or_default(),
                        raw.metadata.iso.unwrap_or(0),
                    )
                    .map(|p| lightcraft_pipeline::detail::nr::NoiseCalibration {
                        a: std::array::from_fn(|i| p.a[i] * normalization_ratio[i] as f32),
                        b: std::array::from_fn(|i| p.b[i] * normalization_ratio[i] as f32),
                        camera_to_working: std::array::from_fn(|i| std::array::from_fn(|j| m[i][j] * wb[j] * gain)),
                    })
                } else {
                    None
                },
                as_shot_temp: temp,
                as_shot_tint: tint,
                lens,
                lens_camera_to_working: Some(li_color::Mat3(m.map(|r| r.map(f64::from))).mul(&li_color::Mat3::diag(
                    wb[0] as f64,
                    wb[1] as f64,
                    wb[2] as f64,
                ))),
                source_hue_sat,
                sony_lens: sony_lens(raw.metadata.sony.as_ref()),
                relative_wb: relative,
                camera_tone,
                look_curve: camera_look.as_ref().map(|p| p.tone).or_else(|| {
                    lightcraft_pipeline::basecurves::camera(
                        raw.metadata.make.as_deref().unwrap_or_default(),
                        raw.metadata.model.as_deref().unwrap_or_default(),
                    )
                }),
                profile_curve: raw.color.profile.tone_curve.as_ref().and_then(dng_tone_curve),
                camera_profile: (!raw.color.profile.is_empty()).then(|| Arc::new(raw.color.profile.clone())),
                baseline_gain: gain,
                camera_color: if t.matrix_is_fallback {
                    None
                } else {
                    let mut color = lightcraft_raw::color::camera_color(&raw.color);
                    color.analog_balance = normalized_color.analog_balance;
                    let total = li_color::Mat3(m.map(|r| r.map(f64::from))).mul(&li_color::Mat3::diag(wb[0] as f64, wb[1] as f64, wb[2] as f64));
                    total.inverse().map(|undo| li_color::camera::CameraWhite { color, undo })
                },
                lens_db: None,
                sensor_scale,
                capture_radius,
                capture_threshold,
            },
        ));
    }
    let d = lightcraft_codecs::decode(&bytes, still_image_decode_options(max_edge)).map_err(|e| e.to_string())?;
    drop(bytes);
    let img = d.to_working();
    let img = if img.width.max(img.height) > max_edge { img.fit(max_edge, max_edge, Filter::Mitchell) } else { img };
    Ok((
        img.into_oriented(Orientation::from_exif(d.orientation)),
        SourceInfo { source_orientation: Orientation::from_exif(d.orientation), ..Default::default() },
    ))
}

/// A DNG `ProfileToneCurve` (linear in, linear out, 1.0 = white after exposure compensation) as the
/// finish stage's camera tone curve: 32 knots, log-spaced over 12 stops below white; above white
/// the camera tone's shoulder continues it.
pub(crate) fn dng_tone_curve(curve: &lightcraft_raw::profile::ToneCurve) -> Option<lightcraft_pipeline::tone::CameraTone> {
    let knots: [[f32; 2]; 32] = std::array::from_fn(|i| {
        let x = 2f32.powf(-12.0 + 12.0 * i as f32 / 31.0);
        [x, curve.eval(x).min(0.9995)]
    });
    lightcraft_pipeline::tone::CameraTone::new(knots)
}

/// Orientation for an embedded preview: its own EXIF orientation when it has one, else the raw file's.
fn preview_orientation(raw_bytes: &[u8], jpeg_orientation: u16) -> Orientation {
    if jpeg_orientation > 1 {
        return Orientation::from_exif(jpeg_orientation);
    }
    lightcraft_meta::extract(raw_bytes).orientation.unwrap_or(Orientation::Normal)
}

/// Oriented size of the embedded preview of a raw file.
fn embedded_preview_size(bytes: &[u8]) -> Option<(u32, u32)> {
    let jpeg = lightcraft_raw::embedded_preview(bytes)?;
    let d = lightcraft_codecs::decode(&jpeg, still_image_decode_options(64)).ok()?;
    let (mut w, mut h) = (d.source_width, d.source_height);
    if preview_orientation(bytes, d.orientation).swaps_axes() {
        std::mem::swap(&mut w, &mut h);
    }
    Some((w, h))
}

/// The embedded preview of a raw file as a working-space image no larger than `max_edge`, oriented.
pub fn load_embedded_preview(bytes: &[u8], max_edge: usize) -> Option<(RgbFrame, SourceInfo)> {
    let jpeg = lightcraft_raw::embedded_preview(bytes)?;
    let d = lightcraft_codecs::decode(&jpeg, still_image_decode_options(max_edge)).ok()?;
    let img = d.to_working();
    let img = if img.width.max(img.height) > max_edge { img.fit(max_edge, max_edge, Filter::Mitchell) } else { img };
    Some((img.oriented(preview_orientation(bytes, d.orientation)), SourceInfo::default()))
}

/// The embedded preview of a raw file for display (sRGB, oriented, no larger than `max_edge`): the
/// loupe and grid show it until the raw itself has been developed ([`crate::media::QuickJob`]).
pub fn embedded_preview_srgb(bytes: &[u8], max_edge: usize) -> Option<li_raster::Rgba8> {
    let jpeg = lightcraft_raw::embedded_preview(bytes)?;
    let mut d = lightcraft_codecs::decode(&jpeg, still_image_decode_options(max_edge)).ok()?;
    if d.image.width.max(d.image.height) > max_edge {
        d.image = d.image.fit(max_edge, max_edge, Filter::Box);
        d.alpha = None;
    }
    let o = preview_orientation(bytes, d.orientation);
    Some(d.to_srgb8().oriented(o))
}

/// Filesystem-backed embedded-preview hook (native).
pub fn fs_preview_loader() -> PreviewLoader {
    Arc::new(|path: &str, max_edge: usize| embedded_preview_srgb(&read_photo(path).ok()?, max_edge))
}

fn read_photo(path: &str) -> Result<Vec<u8>, String> {
    li_fs::read::file(std::path::Path::new(path), li_fs::read::MAX_FILE_BYTES, &lightcraft_sysmem::jobs::Interrupt::NONE)
        .map_err(|e| format!("{path}: {e}"))
}

/// The filesystem loader for raw files decoded with non-default options (see [`fs_hooks`]).
pub fn fs_loader_with() -> crate::media::FileLoaderWith {
    Arc::new(|path: &str, max_edge: usize, opts: &RawOptions| {
        let len = std::fs::metadata(path).map(|m| m.len() as usize).unwrap_or(0);
        let weight = len.saturating_mul(if max_edge <= crate::media::SourceLevel::Thumb.max_edge() { 3 } else { 6 });
        let gate = crate::memory::work_gate();
        let _permit = if crate::memory::is_background() { gate.acquire(weight) } else { gate.acquire_urgent(weight) };
        let bytes = read_photo(path)?;
        let r = load_vec_with(bytes, max_edge, opts);
        crate::memory::release();
        r
    })
}

/// Filesystem-backed hooks (native). On the web the host installs bytes-based hooks instead.
///
/// Their working memory is bounded by [`crate::memory::work_gate`]: background loads (grid
/// thumbnails, neighbour prefetch: [`crate::memory::in_background`]) and import probes wait while
/// too much is in flight, each counting its file's size times the expansion of decoding it;
/// interactive loads (the loupe, exports) never wait — they are counted, so background work
/// yields to them.
pub fn fs_hooks() -> (FileLoader, FileProbe) {
    let loader: FileLoader = Arc::new(|path: &str, max_edge: usize| {
        let len = std::fs::metadata(path).map(|m| m.len() as usize).unwrap_or(0);
        let weight = len.saturating_mul(if max_edge <= crate::media::SourceLevel::Thumb.max_edge() { 3 } else { 6 });
        let gate = crate::memory::work_gate();
        let _permit = if crate::memory::is_background() { gate.acquire(weight) } else { gate.acquire_urgent(weight) };
        let bytes = read_photo(path)?;
        let r = load_vec(bytes, max_edge);
        // the file, the samples and the intermediate images are gone: give their pages back
        crate::memory::release();
        r
    });
    let probe: FileProbe = Arc::new(|path: &str| {
        let len = std::fs::metadata(path).map(|m| m.len() as usize).unwrap_or(0);
        let _permit = crate::memory::work_gate().acquire(len);
        let bytes = read_photo(path)?;
        probe_bytes(path, &bytes)
    });
    (loader, probe)
}

impl crate::Session {
    /// Install the filesystem file hooks (desktop, CLI, MCP headless).
    pub fn with_fs(mut self) -> Self {
        let (l, p) = fs_hooks();
        self.media.file_loader = Some(l);
        self.media.file_loader_with = Some(fs_loader_with());
        self.media.file_probe = Some(p);
        self.media.preview_loader = Some(fs_preview_loader());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lightcraft_codecs::{ChromaSubsampling, EncodeImage, EncodeMeta, Samples, encode_jpeg};

    #[test]
    fn high_resolution_jpeg_import_and_scaled_reference_loading() {
        // A real ~95 MP finished-export size, above the generic 32 MP codec limit.
        let jpeg = li_testkit::constant_gray_jpeg(11_928, 7_952);
        assert!(matches!(
            lightcraft_codecs::decode(&jpeg, lightcraft_codecs::DecodeOptions::fit(64, 64)),
            Err(lightcraft_codecs::Error::TooLarge(_, _))
        ));
        let probe = probe_bytes("high-resolution-final.jpg", &jpeg).unwrap();
        assert_eq!((probe.width, probe.height, probe.kind), (11_928, 7_952, MediaKind::Image));
        let (image, source) = load_vec(jpeg, 512).unwrap();
        assert_eq!((image.width, image.height), (512, 341));
        assert!(!source.raw);
        assert!(image.data.iter().flatten().all(|&v| v.is_finite() && (v - 0.21586).abs() < 0.002));
    }

    #[test]
    fn still_image_import_keeps_a_source_pixel_ceiling() {
        let jpeg = li_testkit::constant_gray_jpeg(16_384, 8_193); // just beyond the explicit still-image budget
        const { assert!((16_384u64 * 8_193) > MAX_STILL_IMAGE_PIXELS) };
        assert!(probe_bytes("over-budget.jpg", &jpeg).unwrap_err().contains("image too large"));
        assert!(load_vec(jpeg, 64).unwrap_err().contains("image too large"));
    }

    #[test]
    fn ds_finding_sensor_confidence_matches_normalized_active_samples() {
        let bytes = crate::tests_xmp::synthetic_dng_with(None, Default::default());
        let mut raw = lightcraft_raw::decode(&bytes).unwrap();
        raw.black.delta_h = vec![1.0; raw.active_area.width];
        let n = raw.normalized().unwrap();
        let samples = raw.normalized_samples().unwrap();
        for y in 0..n.height {
            for x in 0..n.width {
                assert_eq!(samples.get(x, y, 0), n.data[y * n.width + x]);
            }
        }
        let confidence = sensor_clip_confidence(&raw).unwrap();
        assert!(confidence.data.iter().all(|v| (0.0..=1.0).contains(v)));
        // Exercise the real decode path used by render workers, including clipping metadata.
        let (_, info) = load_bytes(&bytes, 16).unwrap();
        assert!(info.clip_confidence.is_some());
    }

    /// Each tile's confidence is the mean over channels of its clipped-sample fraction: per CFA
    /// colour for a mosaic, per channel for linear RGB, and all-or-nothing for monochrome.
    #[test]
    fn sensor_confidence_is_the_mean_clipped_fraction_per_channel() {
        let mut raw = lightcraft_raw::decode(&crate::tests_xmp::synthetic_dng_with(None, Default::default())).unwrap();
        raw.width = 4;
        raw.height = 4;
        raw.active_area = lightcraft_raw::Rect::new(0, 0, 4, 4);
        raw.crop = raw.active_area;
        raw.black = lightcraft_raw::BlackLevel::uniform(0.0);
        raw.white = vec![1000.0];
        raw.cfa = Some(lightcraft_raw::Cfa::bayer("RGGB").unwrap());
        let mut data = vec![200; 16];
        // the first 2×2 tile: red, one of its two greens and blue clipped
        data[0] = 1000;
        data[1] = 1000;
        data[5] = 1000;
        raw.data = lightcraft_raw::RawData::U16(data);
        let p = sensor_clip_confidence(&raw).unwrap();
        assert_eq!((p.width, p.height), (2, 2));
        assert_eq!(p.data, vec![5.0 / 6.0, 0.0, 0.0, 0.0]);
        raw.cfa = None;
        let p = sensor_clip_confidence(&raw).unwrap();
        assert_eq!((p.width, p.height), (4, 4));
        assert_eq!(p.data.iter().filter(|&&v| v == 1.0).count(), 3);
        assert!(p.data.iter().all(|&v| v == 0.0 || v == 1.0));
        raw.width = 2;
        raw.height = 1;
        raw.cpp = 3;
        raw.active_area = lightcraft_raw::Rect::new(0, 0, 2, 1);
        raw.crop = raw.active_area;
        raw.data = lightcraft_raw::RawData::U16(vec![1000, 200, 1000, 200, 200, 200]);
        assert_eq!(sensor_clip_confidence(&raw).unwrap().data, vec![2.0 / 3.0, 0.0]);
    }

    /// A CR3-shaped file (not decodable yet) whose only content is a `PRVW` preview box.
    fn cr3_with_preview(w: u32, h: u32) -> Vec<u8> {
        let px: Vec<u8> = (0..w * h).flat_map(|i| [(i % 251) as u8, 128, 200]).collect();
        let jpeg = encode_jpeg(&EncodeImage::new(w, h, 3, Samples::U8(&px)), 90, ChromaSubsampling::S444, &EncodeMeta::default()).unwrap();
        let mut b = b"\0\0\0\x18ftypcrx \0\0\0\x01crx isom".to_vec();
        b.extend_from_slice(&((24 + jpeg.len()) as u32).to_be_bytes());
        b.extend_from_slice(b"PRVW\0\0\0\0\0\x01");
        b.extend_from_slice(&(w as u16).to_be_bytes());
        b.extend_from_slice(&(h as u16).to_be_bytes());
        b.extend_from_slice(b"\0\x01");
        b.extend_from_slice(&(jpeg.len() as u32).to_be_bytes());
        b.extend_from_slice(&jpeg);
        b
    }

    /// Issue #138: a DNG's own profile look (hue/saturation map, look table, tone curve) is
    /// applied when it's developed — Lightroom-converted DNGs rendered flat and muted without it.
    #[test]
    fn dng_profile_look_is_applied() {
        use lightcraft_raw::profile::{HsvTable, ProfileLook, ToneCurve};
        let plain = crate::tests_xmp::synthetic_dng_with(None, Default::default());
        let mut raw = lightcraft_raw::decode(&plain).unwrap();
        let sat = |img: &RgbFrame| {
            img.data.iter().map(|p| (p[0].max(p[1]).max(p[2]) - p[0].min(p[1]).min(p[2])) / p[0].max(p[1]).max(p[2]).max(1e-6)).sum::<f32>()
                / img.data.len() as f32
        };
        let (before, info) = load_bytes(&plain, 64).unwrap();
        assert!(sat(&before) > 0.05, "the synthetic scene is coloured");
        assert!(info.camera_tone.is_none());
        // a map that removes all saturation, and a tone curve
        let grey = HsvTable { hue_divisions: 4, sat_divisions: 2, val_divisions: 1, data: vec![[0.0, 0.0, 1.0]; 8], srgb_value: false };
        raw.color.profile = ProfileLook {
            hue_sat_map: [Some(grey), None],
            look_table: None,
            tone_curve: ToneCurve::from_tag(&[0.0, 0.0, 0.18, 0.3, 1.0, 1.0]),
            ..Default::default()
        };
        let with = lightcraft_raw::write_dng(&raw, &Default::default()).unwrap();
        let (mut after, info) = load_bytes(&with, 64).unwrap();
        lightcraft_pipeline::local::white_balance(&mut after, &info, &Default::default());
        assert!(sat(&after) < 1e-3, "saturation {} → {}", sat(&before), sat(&after));
        // a saturation-only map keeps brightness roughly (HSV value is kept in ProPhoto RGB)
        let mean = |img: &RgbFrame| img.data.iter().map(|p| p[0].max(p[1]).max(p[2])).sum::<f32>() / img.data.len() as f32;
        assert!((0.8..1.1).contains(&(mean(&after) / mean(&before))), "{} vs {}", mean(&after), mean(&before));
        let tone = info.camera_tone.expect("the DNG tone curve becomes the camera tone");
        assert!((tone.apply(0.18) - 0.3).abs() < 0.01, "{}", tone.apply(0.18));
        // a look table alone changes the render too (applied after exposure)
        raw.color.profile = ProfileLook {
            look_table: Some(HsvTable { hue_divisions: 1, sat_divisions: 2, val_divisions: 1, data: vec![[0.0, 1.0, 0.5]; 2], srgb_value: false }),
            ..Default::default()
        };
        let (mut dim, info) = load_bytes(&lightcraft_raw::write_dng(&raw, &Default::default()).unwrap(), 64).unwrap();
        lightcraft_pipeline::local::white_balance(&mut dim, &info, &Default::default());
        assert!((mean(&dim) / mean(&before) - 0.5).abs() < 0.02, "{} vs {}", mean(&dim), mean(&before));
    }

    #[test]
    fn unsupported_raw_falls_back_to_embedded_preview() {
        let b = cr3_with_preview(48, 32);
        let p = probe_bytes("x.cr3", &b).unwrap();
        assert_eq!((p.width, p.height, p.kind, p.format.as_str()), (48, 32, MediaKind::Raw, "CR3"));
        let (img, src) = load_bytes(&b, 24).unwrap();
        assert_eq!((img.width, img.height), (24, 16));
        assert!(!src.raw);
        // no preview at all: a clear error, not a panic
        assert!(load_bytes(b"\0\0\0\x18ftypcrx \0\0\0\x01", 24).is_err());
    }

    /// Issue #10: a raw variant we can't decode imports as "preview only" with the decoder's
    /// reason, is rendered as the rendered JPEG it is (not as raw), says so to agents, survives a
    /// library reopen, and Reload / Relink clear it once the file decodes (undoably).
    #[test]
    fn unsupported_raw_imports_as_preview_only_and_survives_reopen() {
        use serde_json::json;
        let dir = std::env::temp_dir().join(format!("lc-preview-only-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (lib, files) = (dir.join("lib"), dir.join("files"));
        std::fs::create_dir_all(&files).unwrap();
        let path = files.join("DSC_0001.cr3");
        std::fs::write(&path, cr3_with_preview(96, 64)).unwrap();
        let mut s = crate::Session::new().with_fs();
        s.open_library(&lib, false).unwrap();
        // the import review already says so
        let scan = s.execute("library.importPreview", &json!({"paths": [path.to_string_lossy()]})).unwrap();
        assert!(scan["candidates"][0]["previewOnly"].as_str().is_some_and(|w| !w.is_empty()), "{scan}");
        let r = s.execute("library.import", &json!({"paths": [path.to_string_lossy()]})).unwrap();
        let id = lightcraft_catalog::PhotoId(r["imported"][0].as_u64().unwrap());
        let p = s.catalog.photo(id).unwrap().clone();
        let why = p.preview_only.clone().expect("marked preview only");
        assert!(why.to_uppercase().contains("CR3"), "the decoder's reason: {why}");
        assert_eq!(p.kind, MediaKind::Raw, "still a raw file (filters, Convert to DNG…)");
        assert!(!p.develops_raw());
        // rendered like the JPEG it is: relative white balance, display tone curve, no raw defaults
        assert_eq!(crate::media::source_info(&p), SourceInfo::default());
        assert_eq!(*p.develop, lightcraft_develop::DevelopSettings::default());
        assert!(s.render_job(id, 48, 48, false, true).unwrap().run().rendered.is_ok());
        // agents see it
        let q = s.execute("catalog.query", &json!({"filter": {}})).unwrap();
        assert_eq!(q["photos"][0]["previewOnly"], json!(why), "{q}");
        let i = s.execute("photo.inspect", &json!({"id": id.0})).unwrap();
        assert_eq!(i["preview_only"], json!(why), "{i}");
        // a library reopen keeps it
        s.persist().unwrap();
        drop(s);
        let mut s = crate::Session::new().with_fs();
        s.open_library(&lib, false).unwrap();
        assert_eq!(s.catalog.photo(id).unwrap().preview_only.as_deref(), Some(why.as_str()));
        // the file decodes now (here: replaced by a DNG): Reload clears it, undo brings it back
        std::fs::write(&path, crate::tests_xmp::synthetic_dng_with(None, lightcraft_meta::Metadata::default())).unwrap();
        let r = s.execute("photo.reload", &json!({"ids": [id.0]})).unwrap();
        assert_eq!(r["reloaded"], json!([id.0]), "{r}");
        assert!(s.catalog.photo(id).unwrap().develops_raw());
        assert!(crate::media::source_info(s.catalog.photo(id).unwrap()).raw);
        s.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(s.catalog.photo(id).unwrap().preview_only.as_deref(), Some(why.as_str()));
        // relinking to a file that decodes clears it too
        let dng = files.join("DSC_0001.dng");
        std::fs::write(&dng, crate::tests_xmp::synthetic_dng_with(None, lightcraft_meta::Metadata::default())).unwrap();
        s.execute("photo.relink", &json!({"id": id.0, "path": dng.to_string_lossy()})).unwrap();
        assert_eq!(s.catalog.photo(id).unwrap().preview_only, None);
        drop(s);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn quick_jobs_show_embedded_previews_then_cached_renders() {
        use crate::media::QuickSource;
        let dir = std::env::temp_dir().join(format!("lc-quick-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.cr3");
        std::fs::write(&path, cr3_with_preview(96, 64)).unwrap();
        let mut s = crate::Session::new().with_fs();
        let r = s.execute("library.import", &serde_json::json!({"paths": [path.to_string_lossy()]})).unwrap();
        let id = lightcraft_catalog::PhotoId(r["imported"][0].as_u64().unwrap());
        // loupe: an unedited raw opens on its embedded preview…
        let q = s.quick_view_job(id, 1600, true).unwrap().run();
        assert_eq!(q.quick, Some(QuickSource::Embedded));
        let img = q.rendered.unwrap().image;
        assert_eq!((img.width, img.height), (96, 64));
        // …and once the loupe has rendered it, on that render (any size)
        let full = s.loupe_job(id, 48, 32, true).unwrap().run();
        assert!(full.rendered.is_ok() && full.quick.is_none());
        let q = s.quick_view_job(id, 1600, true).unwrap().run();
        assert_eq!(q.quick, Some(QuickSource::Cached));
        assert_eq!(q.rendered.unwrap().image.width, 48);
        // grid: embedded first, then the real thumbnail (final, with the thumbnail job's key)
        let job = s.thumb_job(id, 128).unwrap();
        let q = s.quick_thumb_job(&job).unwrap().run();
        assert_eq!((q.quick, q.key), (Some(QuickSource::Embedded), job.key));
        drop(job.clone().run());
        assert_eq!(s.quick_thumb_job(&job).unwrap().run().quick, Some(QuickSource::Cached));
        // an edited raw: no embedded stand-in (it wouldn't show the edit)
        s.execute("library.select", &serde_json::json!({"ids": [id.0]})).unwrap();
        s.execute("develop.set", &serde_json::json!({"control": "light.exposure", "value": 1.0})).unwrap();
        let job = s.thumb_job(id, 128).unwrap();
        assert!(s.quick_thumb_job(&job).is_none());
        assert_eq!(s.quick_view_job(id, 1600, true).unwrap().run().quick, Some(QuickSource::Small));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod camdata_tests {
    #[test]
    #[ignore = "fetch CC0 rawdecode and licensed RawNIND reference samples under target/"]
    fn camdata_real_raw_sources_use_profiles_and_camera_presets() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
        for path in [root.join("rawdecode/2414.ARW"), root.join("rawnind/Bayer_7D-1_ISO6400_sha1=733cb095be69607aa14eca55b1f9f471bd0f00a3.cr2")] {
            let (image, info) = super::load_bytes(&std::fs::read(&path).unwrap(), 512).unwrap();
            assert!(info.raw && image.width > 0 && image.height > 0);
            assert!(info.camera_noise.is_some(), "{}: profiled NR must reach the app path", path.display());
            assert!(!info.camera_wb_presets.is_empty(), "{}: camera WB presets", path.display());
            assert!(info.camera_color.is_some());
        }
    }
}

#[cfg(test)]
mod sony_dji_opcode_tests {
    use super::*;
    use lightcraft_raw::{Opcode, OpcodeLists, RawFormat, RawInfo, Rect};

    fn raw_info(list3: Vec<Opcode>) -> RawInfo {
        RawInfo {
            format: RawFormat::Dng,
            width: 8,
            height: 6,
            cpp: 1,
            cfa: None,
            bits: 16,
            active_area: Rect::new(0, 0, 8, 6),
            crop: Rect::new(1, 1, 6, 4),
            orientation: Orientation::Normal,
            color: Default::default(),
            wb_multipliers: None,
            vendor_lens: None,
            opcodes: OpcodeLists { list3, ..Default::default() },
            metadata: Default::default(),
        }
    }

    #[test]
    fn lenscorr_merge_dji_loader_keeps_builtins_and_suppresses_profile_fallbacks() {
        let raw = lightcraft_raw::RawImage {
            format: RawFormat::Dng,
            width: 4,
            height: 3,
            cpp: 3,
            cfa: None,
            bits: 32,
            data: lightcraft_raw::RawData::F32(vec![0.25; 36]),
            black: lightcraft_raw::BlackLevel::uniform(0.0),
            white: vec![1.0; 3],
            active_area: Rect::new(0, 0, 4, 3),
            crop: Rect::new(0, 0, 4, 3),
            orientation: Orientation::Normal,
            color: lightcraft_raw::ColorData { color_matrix: [Some(li_color::Mat3::IDENTITY), None], illuminant: [21, 0], ..Default::default() },
            wb_multipliers: None,
            linearized: false,
            vendor_lens: None,
            opcodes: OpcodeLists {
                list3: vec![
                    Opcode::WarpRectilinear { planes: vec![[0.9, 0.0, 0.0, 0.0, 0.0, 0.0]], center: [0.5, 0.5] },
                    Opcode::GainMap {
                        area: lightcraft_raw::opcodes::Area { bottom: 3, right: 4, planes: 3, row_pitch: 1, col_pitch: 1, ..Default::default() },
                        points_v: 1,
                        points_h: 1,
                        spacing: [1.0; 2],
                        origin: [0.0; 2],
                        map_planes: 1,
                        gains: vec![2.0],
                    },
                ],
                ..Default::default()
            },
            metadata: lightcraft_meta::Metadata { make: Some("DJI".into()), model: Some("L3D-100c".into()), ..Default::default() },
        };
        let bytes = lightcraft_raw::write_dng(
            &raw,
            &lightcraft_raw::DngWriteOptions { compression: lightcraft_raw::DngCompression::Uncompressed, ..Default::default() },
        )
        .unwrap();
        let (image, mut source) = load_bytes(&bytes, 8).unwrap();
        assert!(source.embedded_lens_baked && !source.embedded_ca_baked);
        assert!(source.lens.is_none());
        assert_eq!(source.source_orientation, Orientation::Normal);
        source.lcp = Some(lightcraft_pipeline::lcp::LcpCorrection {
            vignette: Some(lightcraft_pipeline::lcp::LcpModel {
                focal: [1.0; 2],
                center: [0.5; 2],
                radial: [-0.2, 0.0, 0.0],
                tangential: [0.0; 2],
                scale: 1.0,
            }),
            ..Default::default()
        });
        let mut settings = lightcraft_develop::DevelopSettings::default();
        settings.optics.lens_profile = true;
        assert!(lightcraft_pipeline::frame_for(&image, &source, &settings, false).warp.is_none());
        let mut only_gain = raw.clone();
        only_gain.opcodes.list3.remove(0);
        let gain_bytes = lightcraft_raw::write_dng(
            &only_gain,
            &lightcraft_raw::DngWriteOptions { compression: lightcraft_raw::DngCompression::Uncompressed, ..Default::default() },
        )
        .unwrap();
        let (_, gain_source) = load_bytes(&gain_bytes, 8).unwrap();
        assert!(gain_source.embedded_lens_baked, "built-in DJI shading alone must suppress another vignette profile");
        let mut list2_vignette = raw.clone();
        list2_vignette.opcodes.list3.clear();
        list2_vignette.opcodes.list2 = vec![Opcode::FixVignetteRadial { k: [0.2, 0.0, 0.0, 0.0, 0.0], center: [0.5, 0.5] }];
        let vignette_bytes = lightcraft_raw::write_dng(
            &list2_vignette,
            &lightcraft_raw::DngWriteOptions { compression: lightcraft_raw::DngCompression::Uncompressed, ..Default::default() },
        )
        .unwrap();
        assert!(load_bytes(&vignette_bytes, 8).unwrap().1.embedded_lens_baked, "list-2 DJI shading must not receive another vignette profile");
        // The ordered GainMap did run: compare against an independent constant
        // camera RGB value, transformed only after the built-in operations.
        let decoded = lightcraft_raw::decode(&bytes).unwrap();
        let t = lightcraft_raw::color::camera_transform(&decoded, lightcraft_raw::color::as_shot_white_xy(&decoded));
        let expected = t.matrix.apply_f32(std::array::from_fn(|ch| 0.5 * t.wb[ch]));
        for p in image.data {
            for ch in 0..3 {
                assert!((p[ch] - expected[ch]).abs() < 1e-5);
            }
        }
    }

    #[test]
    fn sony_dji_lens_handoff_preserves_opcode_order_and_sdk_origin() {
        let warp = Opcode::WarpRectilinear { planes: vec![[0.5, 0.0, 0.0, 0.0, 0.0, 0.0]], center: [0.25, 0.75] };
        let vignette = Opcode::FixVignetteRadial { k: [1.0, 0.0, 0.0, 0.0, 0.0], center: [0.25, 0.75] };
        let info = raw_info(vec![vignette.clone(), warp.clone()]);
        let lens = embedded_lens(&info).unwrap();
        let w = lens.warp.unwrap();
        let v = lens.vignette.unwrap();
        assert_eq!(w.center, lightcraft_geom::Point::new(1.5 / 6.0, 4.0 / 4.0));
        assert_eq!(v.center, lightcraft_geom::Point::new(1.0 / 6.0, 3.5 / 4.0));
        assert_eq!(w.radius, 7.5 / 6.0);
        let mut drone = info.clone();
        drone.metadata.make = Some("DJI".into());
        assert!(embedded_lens(&drone).is_none(), "built-in DJI operations must run with the profile checkbox off");
        let mut multi = raw_info(vec![warp.clone()]);
        multi.opcodes.list3 =
            vec![Opcode::WarpRectilinear { planes: vec![[0.5, 0.0, 0.0, 0.0, 0.0, 0.0], [0.6, 0.0, 0.0, 0.0, 0.0, 0.0]], center: [0.25, 0.75] }];
        assert!(embedded_lens(&multi).is_none(), "camera-plane CA cannot move after a colour matrix");
        // Source-space vignette-before-warp is representable. The reverse is not.
        assert!(embedded_lens(&raw_info(vec![warp.clone(), vignette.clone()])).is_none());
        // Duplicate warps and mixed lists must stay ordered in raw development.
        assert!(embedded_lens(&raw_info(vec![warp.clone(), warp.clone()])).is_none());
        assert!(
            embedded_lens(&raw_info(vec![
                warp,
                Opcode::GainMap {
                    area: Default::default(),
                    points_v: 1,
                    points_h: 1,
                    spacing: [1.0; 2],
                    origin: [0.0; 2],
                    map_planes: 1,
                    gains: vec![1.25]
                },
                vignette
            ]))
            .is_none()
        );
    }
}

#[cfg(test)]
mod sony_dji_app_tests {
    use super::*;
    #[test]
    #[ignore = "small real DJI source-path validation; fetch scripts/sony_dji_samples.py --supplemental"]
    fn sony_dji_real_sources_keep_embedded_color_and_apply_builtin_operations() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/sony-dji");
        for id in ["3214", "7823", "mini4pro"] {
            let bytes = std::fs::read(dir.join(format!("{id}.DNG"))).unwrap();
            let raw = lightcraft_raw::probe_info(&bytes).unwrap();
            assert!(raw.color.color_matrix.iter().all(Option::is_some), "{id}: preserve dual embedded colour");
            assert!(raw.color.as_shot_neutral.is_some(), "{id}: embedded as-shot WB");
            assert!(!raw.opcodes.list3.is_empty() && raw.opcodes.list3.iter().all(lightcraft_raw::Opcode::is_applied));
            assert!(embedded_lens(&raw).is_none(), "{id}: built-in opcodes stay before colour conversion");
            let probe = probe_bytes(&format!("{id}.DNG"), &bytes).unwrap();
            assert!(probe.preview_only.is_none(), "{id}: normal raw import");
            let (image, source) = load_bytes(&bytes, 32).unwrap();
            assert!(source.raw && source.camera_color.is_some(), "{id}: app source uses embedded calibration");
            assert!(source.lens.is_none(), "{id}: do not apply built-in corrections twice");
            assert!(image.width > 0 && image.height > 0 && image.width.max(image.height) <= 32);
            assert!(image.data.iter().flatten().all(|v| v.is_finite()), "{id}: finite developed image");
        }
    }
}

#[cfg(test)]
mod baked_merge_lens_regression {
    use super::*;

    #[test]
    fn merged_dng_reopen_suppresses_automatic_lens_correction() {
        let image = RgbFrame::filled(16, 12, [0.2; 3]);
        let color =
            lightcraft_merge::FrameColor::Linear { to_xyz_d50: li_color::bradford(li_color::D65, li_color::D50).mul(&li_color::REC2020.to_xyz()) };
        let metadata = lightcraft_meta::Metadata {
            lens_corrections_baked: true,
            make: Some("Test camera".into()),
            lens_model: Some("Original lens".into()),
            ..Default::default()
        };
        let bytes =
            lightcraft_merge::write_linear_dng(&image, &color, Orientation::Normal, &metadata, 0., lightcraft_merge::DngSamples::Float).unwrap();
        let raw = lightcraft_raw::decode(&bytes).unwrap();
        assert!(raw.metadata.lens_corrections_baked);
        assert!(embedded_lens(&raw.info()).is_none());
        let (_, info) = load_bytes(&bytes, 16).unwrap();
        assert!(info.embedded_lens_baked);
        let mut s = lightcraft_develop::DevelopSettings::default();
        s.optics.lens_profile = true;
        // A baked source must keep manual optics usable, but never resolve a
        // second automatic correction from the retained camera/lens metadata.
        assert!(lightcraft_pipeline::frame_for(&image, &info, &s, false).warp.is_none());
    }
}
