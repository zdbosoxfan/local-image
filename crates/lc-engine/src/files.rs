//! Real files: probing (dimensions + metadata for import) and loading (decode → linear Rec.2020,
//! oriented, downscaled to the requested level). Standard formats go through `lightcraft-codecs`,
//! camera raws through `lightcraft-raw` (demosaic + DNG colour model).

use std::sync::Arc;

use lightcraft_catalog::{MediaKind, Meta};
use lightcraft_color::cct::xy_to_temp_tint;
use lightcraft_geom::Orientation;
use lightcraft_pipeline::SourceInfo;
use lightcraft_preview::Hasher128;
use lightcraft_raster::Rgb32f;
use lightcraft_raster::resample::{Filter, fit};

use crate::media::{FileLoader, FileProbe, PreviewLoader, ProbeInfo};

fn meta_of(m: &lightcraft_meta::Metadata) -> (Meta, Option<String>) {
    let shutter = m.exposure_time.map(|t| if t >= 1.0 { format!("{t:.0}") } else { format!("1/{:.0}", 1.0 / t) }).unwrap_or_default();
    let camera = [m.make.clone().unwrap_or_default(), m.model.clone().unwrap_or_default()].join(" ").trim().to_string();
    let meta = Meta {
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
        keywords: m.keywords.clone(),
        regions: m.regions.clone(),
    };
    (meta, m.capture_time.as_ref().map(|d| d.to_iso()))
}

/// Lens corrections embedded in a DNG's `OpcodeList3` (`WarpRectilinear`, `FixVignetteRadial`), re-expressed for
/// the default-cropped, EXIF-oriented image. These are the only "profile" corrections LightCraft applies.
pub fn embedded_lens(raw: &lightcraft_raw::RawInfo) -> Option<lightcraft_develop::EmbeddedLens> {
    use lightcraft_develop::{EmbeddedLens, EmbeddedVignette, EmbeddedWarp};
    use lightcraft_geom::Point;
    let (aw, ah) = (raw.active_area.width as f64, raw.active_area.height as f64);
    if aw < 2.0 || ah < 2.0 {
        return None;
    }
    let c = raw.crop.clipped(raw.active_area.width, raw.active_area.height);
    let (cx0, cy0, cw, ch) =
        if c.width == 0 || c.height == 0 { (0.0, 0.0, aw, ah) } else { (c.x as f64, c.y as f64, c.width as f64, c.height as f64) };
    let long = cw.max(ch);
    // opcode centres are relative to the (uncropped) active area, in pixel-index units
    let centre = |rel: [f64; 2]| -> (Point, f64) {
        let (px, py) = (rel[0] * (aw - 1.0), rel[1] * (ah - 1.0));
        let m = [(0.0, 0.0), (aw - 1.0, 0.0), (0.0, ah - 1.0), (aw - 1.0, ah - 1.0)]
            .iter()
            .map(|&(x, y)| (x - px).hypot(y - py))
            .fold(0.0, f64::max)
            .max(1e-9);
        (Point::new((px + 0.5 - cx0) / cw, (py + 0.5 - cy0) / ch), m / long)
    };
    let mut lens = EmbeddedLens::default();
    for op in &raw.opcodes.list3 {
        match op {
            lightcraft_raw::Opcode::WarpRectilinear { planes, center } if !planes.is_empty() && lens.warp.is_none() => {
                let (center, radius) = centre(*center);
                let p = |i: usize| planes[i.min(planes.len() - 1)];
                lens.warp = Some(EmbeddedWarp { planes: [p(0), p(1), p(2)], center, radius });
            }
            lightcraft_raw::Opcode::FixVignetteRadial { k, center } if lens.vignette.is_none() => {
                let (center, radius) = centre(*center);
                lens.vignette = Some(EmbeddedVignette { k: *k, center, radius });
            }
            _ => {}
        }
    }
    if lens.warp.is_none() && lens.vignette.is_none() {
        return None;
    }
    Some(lightcraft_pipeline::optics::reorient_lens(&lens, raw.orientation, cw, ch))
}

fn is_lens_opcode(op: &lightcraft_raw::Opcode) -> bool {
    matches!(op, lightcraft_raw::Opcode::WarpRectilinear { .. } | lightcraft_raw::Opcode::FixVignetteRadial { .. })
}

fn ext_upper(name: &str) -> String {
    std::path::Path::new(name).extension().map(|e| e.to_string_lossy().to_uppercase()).unwrap_or_default()
}

/// Probe a file's bytes: kind, dimensions (oriented), metadata. Raws are described from their
/// headers ([`lightcraft_raw::probe_info`]): no pixel data is decompressed.
pub fn probe_bytes(name: &str, bytes: &[u8]) -> Result<ProbeInfo, String> {
    let content_hash = Some(lightcraft_preview::hash_bytes(bytes).to_string());
    let (m, xmp) = lightcraft_raw::metadata_with_xmp(bytes);
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
        });
    }
    let fmt = lightcraft_codecs::sniff(bytes).ok_or("unrecognized file format")?;
    if !fmt.can_decode() {
        return Err(format!("{fmt:?} files are not supported yet"));
    }
    let d = lightcraft_codecs::decode(bytes, lightcraft_codecs::DecodeOptions::fit(64, 64)).map_err(|e| e.to_string())?;
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
    })
}

/// Sensor clip level (normalised) for highlight reconstruction.
const HIGHLIGHT_CLIP: f32 = 0.99;

/// Read the unmodified sensor before any reconstruction. Each CFA tile records the
/// fraction of RGB channels whose samples are clipped (green samples averaged).
fn sensor_clip_confidence(raw: &lightcraft_raw::RawImage) -> Option<lightcraft_raster::Plane> {
    let n = raw.normalized().ok()?;
    let crop = raw.crop.clipped(n.width, n.height);
    let crop = if crop.width > 1 && crop.height > 1 { crop } else { lightcraft_raw::Rect { x: 0, y: 0, width: n.width, height: n.height } };
    let tile = n.cfa.as_ref().map_or(1, |c| c.width.max(c.height));
    // Independent sensor tiles keep their arithmetic order, but no longer serialize the
    // full-sensor clipping pass on one worker while developing a small thumbnail.
    use rayon::prelude::*;
    let width = crop.width.div_ceil(tile);
    let mut p = lightcraft_raster::Plane::new(width, crop.height.div_ceil(tile));
    p.data.par_iter_mut().enumerate().for_each(|(i, value)| {
        let (x, y) = (i % width, i / width);
        let mut counts = [0usize; 3];
        let mut clipped = [0usize; 3];
        for dy in 0..tile {
            for dx in 0..tile {
                let xx = (crop.x + x * tile + dx).min(crop.x + crop.width - 1);
                let yy = (crop.y + y * tile + dy).min(crop.y + crop.height - 1);
                if n.cpp == 3 {
                    for c in 0..3 {
                        counts[c] += 1;
                        clipped[c] += usize::from(n.data[(yy * n.width + xx) * 3 + c] >= HIGHLIGHT_CLIP);
                    }
                } else if let Some(cfa) = &n.cfa {
                    let c = cfa.color_at(xx, yy).min(2) as usize;
                    counts[c] += 1;
                    clipped[c] += usize::from(n.data[yy * n.width + xx] >= HIGHLIGHT_CLIP);
                } else {
                    let v = usize::from(n.data[yy * n.width + xx] >= HIGHLIGHT_CLIP);
                    counts = [1; 3];
                    clipped = [v; 3];
                }
            }
        }
        *value = (0..3).map(|c| clipped[c] as f32 / counts[c].max(1) as f32).sum::<f32>() / 3.;
    });
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
/// (demosaic, highlight reconstruction, measuring the capture-sharpening radius). The default
/// decodes exactly as before those settings existed.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RawOptions {
    pub demosaic: lightcraft_develop::Demosaic,
    /// Dual demosaic threshold 0..1 (0 unless the method is dual).
    pub dual_threshold: f32,
    pub highlights: lightcraft_develop::HighlightMode,
    /// Measure the capture-sharpening radius from the mosaic ([`SourceInfo::capture_radius`]).
    pub capture_radius: bool,
}

impl RawOptions {
    /// The options `s` asks for (the defaults while the Raw section is switched off).
    pub fn of(s: &lightcraft_develop::DevelopSettings) -> RawOptions {
        let r = &s.raw;
        if !s.section_enabled("raw") {
            return RawOptions::default();
        }
        RawOptions {
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
            D::Auto | D::Ahd => M::Ahd,
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
pub fn load_bytes(bytes: &[u8], max_edge: usize) -> Result<(Rgb32f, SourceInfo), String> {
    load_bytes_with(bytes, max_edge, &RawOptions::default())
}

/// [`load_bytes`] decoding a raw file with `opts`.
pub fn load_bytes_with(bytes: &[u8], max_edge: usize, opts: &RawOptions) -> Result<(Rgb32f, SourceInfo), String> {
    rayon::scope(|_| load_bytes_now(std::borrow::Cow::Borrowed(bytes), max_edge, opts))
}

/// [`load_bytes`] taking the file's bytes: a raw file's bytes are freed as soon as it is decoded
/// (less memory held while it is developed).
pub fn load_vec(bytes: Vec<u8>, max_edge: usize) -> Result<(Rgb32f, SourceInfo), String> {
    load_vec_with(bytes, max_edge, &RawOptions::default())
}

/// [`load_vec`] decoding a raw file with `opts`.
pub fn load_vec_with(bytes: Vec<u8>, max_edge: usize, opts: &RawOptions) -> Result<(Rgb32f, SourceInfo), String> {
    let opts = *opts;
    rayon::scope(move |_| load_bytes_now(std::borrow::Cow::Owned(bytes), max_edge, &opts))
}

fn load_bytes_now(bytes: std::borrow::Cow<'_, [u8]>, max_edge: usize, opts: &RawOptions) -> Result<(Rgb32f, SourceInfo), String> {
    if lightcraft_raw::probe(&bytes).is_some() {
        let mut raw = match lightcraft_raw::decode(&bytes) {
            Ok(r) => r,
            Err(lightcraft_raw::RawError::Unsupported(why)) => {
                // show the camera's embedded JPEG (rendered, not raw) until the variant is supported
                return load_embedded_preview(&bytes, max_edge).ok_or(format!("unsupported raw ({why}) without an embedded preview"));
            }
            Err(e) => return Err(e.to_string()),
        };
        crate::dcp_profiles::apply(&mut raw);
        lightcraft_raw::camera_matrices::fill(
            &mut raw.color,
            raw.metadata.make.as_deref().unwrap_or_default(),
            raw.metadata.model.as_deref().unwrap_or_default(),
        );
        let xy = lightcraft_raw::color::as_shot_white_xy(&raw);
        let t = lightcraft_raw::color::camera_transform(&raw, xy);
        let camera_look = crate::camera_preview::fit_preview(&raw, &bytes, &t);
        drop(bytes);
        // Embedded lens corrections are applied by the pipeline ("Enable Profile Corrections"), not baked in.
        let lens = embedded_lens(&raw.info());
        raw.opcodes.list3.retain(|op| !is_lens_opcode(op));
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
        let hue_sat = camera_look.as_ref().and_then(|p| p.hue_sat.as_ref()).and_then(crate::camera_preview::HueSat::new);
        let gain = 2f32.powf(t.baseline_exposure as f32);
        let wb = t.wb;
        // A DNG's own profile look (hue/saturation map, look table), DNG spec chapter 6.
        img.map_in_place(|p| {
            let c = [p[0] * wb[0], p[1] * wb[1], p[2] * wb[2]];
            let rgb = [
                m[0][0] * c[0] + m[0][1] * c[1] + m[0][2] * c[2],
                m[1][0] * c[0] + m[1][1] * c[1] + m[1][2] * c[2],
                m[2][0] * c[0] + m[2][1] * c[1] + m[2][2] * c[2],
            ];
            let rgb = rgb.map(|v| v * gain);
            // after the baseline exposure, as when it was fitted
            hue_sat.as_ref().map_or(rgb, |h| h.apply(rgb))
        });
        stages.push(("colour", t0.elapsed()));
        let img = fit(&img, max_edge, max_edge, Filter::Box);
        let sensor_scale = (sensor_long as f32 / img.width.max(img.height).max(1) as f32).max(1.0);
        stages.push(("fit", t0.elapsed()));
        let clip_confidence = clip_confidence.map(|p| {
            let p = lightcraft_raster::resample::resize(&p, img.width, img.height, Filter::Bilinear).into_oriented(raw.orientation);
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
            eprintln!("[profile] raw source {}×{} (max {max_edge}, ms after decode): {}", img.width, img.height, parts.join(", "));
        }
        let (temp, tint) = xy_to_temp_tint(xy);
        let relative = crate::camera_preview::file_local_look(raw.format) && t.matrix_is_fallback;
        let camera_tone = camera_look.as_ref().map(|p| p.tone).or_else(|| raw.color.profile.tone_curve.as_ref().and_then(dng_tone_curve));
        let (temp, tint) = if relative { (6500.0, 0.0) } else { (temp.round(), tint.round()) };
        return Ok((
            img,
            SourceInfo {
                raw: true,
                clip_confidence,
                raw_clip_level: Some(HIGHLIGHT_CLIP),
                as_shot_temp: temp,
                as_shot_tint: tint,
                lens,
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
                    let color = lightcraft_color::camera::CameraColor {
                        illuminant: raw.color.illuminant,
                        color_matrix: raw.color.color_matrix,
                        forward_matrix: raw.color.forward_matrix,
                        camera_calibration: raw.color.camera_calibration,
                        analog_balance: raw.color.analog_balance,
                    };
                    let total = lightcraft_color::Mat3(m.map(|r| r.map(f64::from))).mul(&lightcraft_color::Mat3::diag(
                        wb[0] as f64,
                        wb[1] as f64,
                        wb[2] as f64,
                    ));
                    total.inverse().map(|undo| lightcraft_color::camera::CameraWhite { color, undo })
                },
                lens_db: None,
                sensor_scale,
                capture_radius,
                capture_threshold,
            },
        ));
    }
    let d = lightcraft_codecs::decode(&bytes, lightcraft_codecs::DecodeOptions::fit(max_edge as u32, max_edge as u32)).map_err(|e| e.to_string())?;
    drop(bytes);
    let img = d.to_working();
    let img = if img.width.max(img.height) > max_edge { fit(&img, max_edge, max_edge, Filter::Mitchell) } else { img };
    Ok((img.into_oriented(Orientation::from_exif(d.orientation)), SourceInfo::default()))
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
    let d = lightcraft_codecs::decode(&jpeg, lightcraft_codecs::DecodeOptions::fit(64, 64)).ok()?;
    let (mut w, mut h) = (d.source_width, d.source_height);
    if preview_orientation(bytes, d.orientation).swaps_axes() {
        std::mem::swap(&mut w, &mut h);
    }
    Some((w, h))
}

/// The embedded preview of a raw file as a working-space image no larger than `max_edge`, oriented.
pub fn load_embedded_preview(bytes: &[u8], max_edge: usize) -> Option<(Rgb32f, SourceInfo)> {
    let jpeg = lightcraft_raw::embedded_preview(bytes)?;
    let d = lightcraft_codecs::decode(&jpeg, lightcraft_codecs::DecodeOptions::fit(max_edge as u32, max_edge as u32)).ok()?;
    let img = d.to_working();
    let img = if img.width.max(img.height) > max_edge { fit(&img, max_edge, max_edge, Filter::Mitchell) } else { img };
    Some((img.oriented(preview_orientation(bytes, d.orientation)), SourceInfo::default()))
}

/// The embedded preview of a raw file for display (sRGB, oriented, no larger than `max_edge`): the
/// loupe and grid show it until the raw itself has been developed ([`crate::media::QuickJob`]).
pub fn embedded_preview_srgb(bytes: &[u8], max_edge: usize) -> Option<lightcraft_raster::Rgba8> {
    let jpeg = lightcraft_raw::embedded_preview(bytes)?;
    preview_srgb(&jpeg, lightcraft_meta::extract(bytes).orientation.unwrap_or_default(), max_edge)
}

fn preview_srgb(jpeg: &[u8], raw_orientation: Orientation, max_edge: usize) -> Option<lightcraft_raster::Rgba8> {
    let max_edge = max_edge.max(1);
    let mut d = lightcraft_codecs::decode(jpeg, lightcraft_codecs::DecodeOptions::fit(max_edge as u32, max_edge as u32)).ok()?;
    if d.image.width.max(d.image.height) > max_edge {
        d.image = fit(&d.image, max_edge, max_edge, Filter::Box);
        d.alpha = None;
    }
    let o = if d.orientation > 1 { Orientation::from_exif(d.orientation) } else { raw_orientation };
    Some(d.to_srgb8().oriented(o))
}

/// Filesystem-backed embedded-preview hook (native).
pub fn fs_preview_loader() -> PreviewLoader {
    Arc::new(|path: &str, max_edge: usize| {
        let mut file = std::fs::File::open(path).ok()?;
        let len = file.metadata().ok()?.len() as usize;
        let edge = max_edge.clamp(1, crate::media::SourceLevel::Preview.max_edge());
        let weight = len.min(64 << 20).saturating_add(edge.saturating_mul(edge).saturating_mul(24));
        let gate = crate::memory::work_gate();
        let permit = if crate::memory::is_background() { gate.acquire(weight) } else { gate.acquire_urgent(weight) };
        if let Some(preview) = lightcraft_raw::embedded_preview_reader(&mut file) {
            return preview_srgb(&preview.jpeg, preview.orientation, max_edge);
        }
        drop(permit);
        let weight = len.saturating_mul(3).saturating_add(edge.saturating_mul(edge).saturating_mul(24));
        let _permit = if crate::memory::is_background() { gate.acquire(weight) } else { gate.acquire_urgent(weight) };
        embedded_preview_srgb(&std::fs::read(path).ok()?, max_edge)
    })
}

/// The filesystem loader for raw files decoded with non-default options (see [`fs_hooks`]).
pub fn fs_loader_with() -> crate::media::FileLoaderWith {
    Arc::new(|path: &str, max_edge: usize, opts: &RawOptions| {
        let len = std::fs::metadata(path).map(|m| m.len() as usize).unwrap_or(0);
        let weight = len * if max_edge <= crate::media::SourceLevel::Thumb.max_edge() { 3 } else { 6 };
        let gate = crate::memory::work_gate();
        let _permit = if crate::memory::is_background() { gate.acquire(weight) } else { gate.acquire_urgent(weight) };
        let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
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
        let weight = len * if max_edge <= crate::media::SourceLevel::Thumb.max_edge() { 3 } else { 6 };
        let gate = crate::memory::work_gate();
        let _permit = if crate::memory::is_background() { gate.acquire(weight) } else { gate.acquire_urgent(weight) };
        let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
        let r = load_vec(bytes, max_edge);
        // the file, the samples and the intermediate images are gone: give their pages back
        crate::memory::release();
        r
    });
    let probe: FileProbe = Arc::new(|path: &str| {
        let len = std::fs::metadata(path).map(|m| m.len() as usize).unwrap_or(0);
        let _permit = crate::memory::work_gate().acquire(len);
        let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
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

    #[test]
    fn library_scale_sensor_confidence_keeps_channel_fractions_for_cfa_rgb_and_mono() {
        let mut raw = lightcraft_raw::decode(&crate::tests_xmp::synthetic_dng_with(None, Default::default())).unwrap();
        raw.width = 4;
        raw.height = 4;
        raw.active_area = lightcraft_raw::Rect::new(0, 0, 4, 4);
        raw.crop = raw.active_area;
        raw.black = lightcraft_raw::BlackLevel::uniform(0.0);
        raw.white = vec![1000.0];
        raw.cfa = Some(lightcraft_raw::Cfa::bayer("RGGB").unwrap());
        let mut data = vec![200; 16];
        data[0] = 1000; // clipped red in the first tile
        data[1] = 1000; // one of its two greens
        data[5] = 1000; // its blue
        raw.data = lightcraft_raw::RawData::U16(data);
        let p = sensor_clip_confidence(&raw).unwrap();
        assert_eq!((p.width, p.height), (2, 2));
        assert_eq!(p.data, vec![5.0 / 6.0, 0.0, 0.0, 0.0]);
        raw.cfa = None;
        let p = sensor_clip_confidence(&raw).unwrap();
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

    #[test]
    fn library_scale_seek_preview_loader_matches_buffer_pixels_and_orientation_including_fallback() {
        let dir = crate::tests_xmp::temp_dir("seek-preview");
        let loader = fs_preview_loader();
        for orientation in [1, 8] {
            let px: Vec<u8> = (0..64 * 48).flat_map(|i| [(i % 251) as u8, 128, 200]).collect();
            let exif = lightcraft_meta::write_exif(&lightcraft_meta::Metadata {
                orientation: Some(Orientation::from_exif(orientation)),
                ..Default::default()
            });
            let meta = EncodeMeta { exif: Some(&exif), ..Default::default() };
            let jpeg = encode_jpeg(&EncodeImage::new(64, 48, 3, Samples::U8(&px)), 90, ChromaSubsampling::S444, &meta).unwrap();
            let at = 2 << 20;
            let mut bytes = b"II\x2a\0\x08\0\0\0".to_vec();
            bytes.extend(3u16.to_le_bytes());
            for (tag, ty, value) in [(274u16, 3u16, 6u32), (513, 4, at as u32), (514, 4, jpeg.len() as u32)] {
                bytes.extend(tag.to_le_bytes());
                bytes.extend(ty.to_le_bytes());
                bytes.extend(1u32.to_le_bytes());
                bytes.extend(value.to_le_bytes());
            }
            bytes.extend(0u32.to_le_bytes());
            bytes.resize(at, 0);
            bytes.extend(jpeg);
            let path = dir.join(format!("orientation-{orientation}.dng"));
            std::fs::write(&path, &bytes).unwrap();
            let expected = embedded_preview_srgb(&bytes, 32).unwrap();
            let actual = loader(path.to_str().unwrap(), 32).unwrap();
            assert_eq!(actual, expected);
        }
        let bytes = cr3_with_preview(64, 48);
        let path = dir.join("fallback.cr3");
        std::fs::write(&path, &bytes).unwrap();
        assert_eq!(loader(path.to_str().unwrap(), 32), embedded_preview_srgb(&bytes, 32));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Issue #138: a DNG's own profile look (hue/saturation map, look table, tone curve) is
    /// applied when it's developed — Lightroom-converted DNGs rendered flat and muted without it.
    #[test]
    fn dng_profile_look_is_applied() {
        use lightcraft_raw::profile::{HsvTable, ProfileLook, ToneCurve};
        let plain = crate::tests_xmp::synthetic_dng_with(None, Default::default());
        let mut raw = lightcraft_raw::decode(&plain).unwrap();
        let sat = |img: &Rgb32f| {
            img.data.iter().map(|p| (p[0].max(p[1]).max(p[2]) - p[0].min(p[1]).min(p[2])) / p[0].max(p[1]).max(p[2]).max(1e-6)).sum::<f32>()
                / img.data.len() as f32
        };
        let (before, info) = load_bytes(&plain, 64).unwrap();
        assert!(sat(&before) > 0.05, "the synthetic scene is coloured");
        assert!(info.camera_tone.is_none());
        // a map that removes all saturation, and a tone curve
        let grey = HsvTable { hue_divisions: 4, sat_divisions: 2, val_divisions: 1, data: vec![[0.0, 0.0, 1.0]; 8], srgb_value: false };
        raw.color.profile =
            ProfileLook { hue_sat_map: [Some(grey), None], look_table: None, tone_curve: ToneCurve::from_tag(&[0.0, 0.0, 0.18, 0.3, 1.0, 1.0]) };
        let with = lightcraft_raw::write_dng(&raw, &Default::default()).unwrap();
        let (mut after, info) = load_bytes(&with, 64).unwrap();
        lightcraft_pipeline::local::white_balance(&mut after, &info, &Default::default());
        assert!(sat(&after) < 1e-3, "saturation {} → {}", sat(&before), sat(&after));
        // a saturation-only map keeps brightness roughly (HSV value is kept in ProPhoto RGB)
        let mean = |img: &Rgb32f| img.data.iter().map(|p| p[0].max(p[1]).max(p[2])).sum::<f32>() / img.data.len() as f32;
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
