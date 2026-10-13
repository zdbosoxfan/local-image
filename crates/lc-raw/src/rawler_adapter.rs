//! The sole boundary to the unmodified crates.io rawler engine (see docs/RAW-DECODERS.md).
//!
//! Called only after a native decoder reports `Unsupported`. Container preflight bounds allocations
//! before rawler parses untrusted counts; rawler's public decode API contains codec panics. Our sensor
//! origin stays unchanged: CFA is sensor-relative, black levels and crop are active-area-relative.
use crate::{BlackLevel, Cfa, ColorData, Mat3, Mode, OpcodeLists, RawData, RawError, RawFormat, RawImage, Rect, Result};
use lightcraft_tiff::{ByteOrder, tags};
use rawler::{decoders::RawDecodeParams, imgop::xyz::Illuminant, rawimage::RawPhotometricInterpretation, rawsource::RawSource};
use std::collections::HashSet;

// Covers 100 MP stills and 80 MP OM high-resolution files; prevents multi-GB fabricated images.
const MAX_PIXELS: usize = 128_000_000;
const MAX_FILE: usize = 512 * 1024 * 1024;
const MAX_METADATA: usize = 32 * 1024 * 1024;
const MAX_VALUES: usize = 1_048_576;

fn corrupt(message: &str) -> RawError {
    RawError::Corrupt(format!("rawler preflight: {message}"))
}
fn geometry(w: usize, h: usize) -> Result<usize> {
    w.checked_mul(h).filter(|&n| w > 0 && h > 0 && w <= 65535 && h <= 65535 && n <= MAX_PIXELS).ok_or(RawError::Limit("raw image geometry"))
}

pub(crate) fn decode(bytes: &[u8], format: RawFormat, mode: Mode) -> Result<RawImage> {
    preflight(bytes, format)?;
    let source = RawSource::new_from_slice(bytes);
    // Public rawler decode/dummy contain panic::catch_unwind, including parallel codec panics.
    let mut raw = match mode {
        Mode::Full => rawler::decode(&source, &RawDecodeParams::default()),
        Mode::Header => rawler::decode_dummy(&source),
    }
    .map_err(|err| match err {
        rawler::RawlerError::Unsupported { .. } => RawError::Unsupported(format!("rawler: {err}")),
        rawler::RawlerError::DecoderFailed(_) => RawError::Corrupt(format!("rawler: {err}")),
    })?;
    // rawler 0.8.0 applies the RAF compressed camera mode only during full decoding.
    // Apply the same public camera data to dummy headers without decompressing the sensor.
    if mode == Mode::Header
        && format == RawFormat::Raf
        && raf_compressed(bytes)?
        && let Some(camera) = rawler::global_loader().get_cameras().get(&(raw.make.clone(), raw.model.clone(), "compressed".into()))
    {
        raw.active_area = camera.active_area.map(|[l, t, r, b]| {
            rawler::imgop::Rect::new(
                rawler::imgop::Point::new(l, t),
                rawler::imgop::Dim2::new(raw.width.saturating_sub(l + r), raw.height.saturating_sub(t + b)),
            )
        });
        raw.color_matrix = camera.color_matrix.clone();
        raw.bps = camera.real_bps;
        if let Some(level) = camera.make_whitelevel(1) {
            raw.whitelevel = level;
        }
        if let Some(level) = camera.make_blacklevel(1) {
            raw.blacklevel = level;
        }
    }
    map_image(raw, bytes, format, mode)
}

fn raf_compressed(bytes: &[u8]) -> Result<bool> {
    let Some(block) = crate::vendor::raf::header(bytes)?.raw else { return Ok(false) };
    let Ok(tiff) = lightcraft_tiff::Tiff::parse(block) else { return Ok(block.starts_with(b"IS")) };
    let Some(offset) = tiff.ifds.first().and_then(|i| i.u64(0xf000)) else { return Ok(false) };
    let (ifd, _) = lightcraft_tiff::parse_ifd_at(block, offset, tiff.order, 0, false, &Default::default())?;
    Ok(ifd.u64(0xf007).and_then(|off| block.get(off as usize..(off as usize).saturating_add(2))) == Some(b"IS"))
}

fn map_image(raw: rawler::RawImage, bytes: &[u8], format: RawFormat, mode: Mode) -> Result<RawImage> {
    let n = geometry(raw.width, raw.height)?;
    if raw.cpp != 1 || raw.bps > 16 || raw.fuji_rotation_width.is_some() {
        return Err(RawError::Unsupported("rawler output requires unsupported sensor layout".into()));
    }
    let rect = |r: rawler::imgop::Rect| Rect::new(r.p.x, r.p.y, r.d.w, r.d.h);
    let active = raw.active_area.map(rect).unwrap_or(Rect::new(0, 0, raw.width, raw.height));
    let crop = raw.crop_area.map(rect).unwrap_or(active);
    let fits = |r: Rect| {
        r.width > 0
            && r.height > 0
            && r.x.checked_add(r.width).is_some_and(|end| end <= raw.width)
            && r.y.checked_add(r.height).is_some_and(|end| end <= raw.height)
    };
    if !fits(active)
        || !fits(crop)
        || crop.x < active.x
        || crop.y < active.y
        || crop.x + crop.width > active.x + active.width
        || crop.y + crop.height > active.y + active.height
    {
        return Err(corrupt("invalid active area or crop"));
    }
    let cfa = match &raw.photometric {
        RawPhotometricInterpretation::Cfa(config) => {
            let (w, h) = (config.cfa.width, config.cfa.height);
            if w == 0 || h == 0 || w > 12 || h > 12 {
                return Err(corrupt("invalid CFA"));
            }
            let pattern = (0..h).flat_map(|y| (0..w).map(move |x| config.cfa.color_at(x, y) as u8)).collect();
            Some(Cfa { width: w, height: h, pattern })
        }
        RawPhotometricInterpretation::BlackIsZero => None,
        RawPhotometricInterpretation::LinearRaw => return Err(RawError::Unsupported("rawler linear RGB sensor".into())),
    };
    let b = &raw.blacklevel;
    if b.width == 0 || b.height == 0 || b.width > 12 || b.height > 12 || b.cpp != 1 || b.levels.len() != b.width * b.height {
        return Err(corrupt("invalid black-level layout"));
    }
    // rawler stores blacks at the sensor origin. Normalization in lc-raw starts at active (0,0).
    let levels = b.as_vec();
    let values: Vec<_> = (0..b.height)
        .flat_map(|y| {
            let levels = &levels;
            (0..b.width).map(move |x| levels[((y + active.y) % b.height) * b.width + (x + active.x) % b.width])
        })
        .collect();
    let black = BlackLevel { repeat_rows: b.height, repeat_cols: b.width, values, ..Default::default() };
    let white = raw.whitelevel.as_vec();
    if white.len() != 1 || !white[0].is_finite() || white[0] <= black.mean() || black.values.iter().any(|v| !v.is_finite() || *v < 0.0) {
        return Err(corrupt("invalid sensor levels"));
    }
    let wb = [raw.wb_coeffs[0], raw.wb_coeffs[1], raw.wb_coeffs[2]];
    let wb = wb.iter().all(|v| v.is_finite() && *v > 0.0).then(|| wb.map(|v| v / wb[1]));
    let mut color = ColorData::default();
    fill_color(&mut color, &raw.make, &raw.model);
    if color.color_matrix.iter().all(Option::is_none) {
        fill_matrices(&mut color, &raw.color_matrix);
    }
    let metadata_bytes = if format == RawFormat::Raf { crate::vendor::raf::header(bytes)?.jpeg.unwrap_or(bytes) } else { bytes };
    let mut metadata = lightcraft_meta::extract(metadata_bytes);
    metadata.make.get_or_insert(raw.make);
    metadata.model.get_or_insert(raw.model);
    let orientation = metadata.orientation.unwrap_or_else(|| crate::Orientation::from_exif(raw.orientation.to_u16()));
    metadata.orientation = Some(orientation);
    metadata.width = Some(crop.width as u32);
    metadata.height = Some(crop.height as u32);
    let data = match (mode, raw.data) {
        (Mode::Header, _) => RawData::U16(Vec::new()),
        (Mode::Full, rawler::RawImageData::Integer(data)) if data.len() == n => RawData::U16(data),
        _ => return Err(corrupt("unexpected sensor samples")),
    };
    let image = RawImage {
        format,
        width: raw.width,
        height: raw.height,
        cpp: 1,
        data,
        cfa,
        bits: raw.bps as u32,
        black,
        white,
        active_area: active,
        crop: Rect::new(crop.x - active.x, crop.y - active.y, crop.width, crop.height),
        orientation,
        color,
        wb_multipliers: wb,
        linearized: false,
        opcodes: OpcodeLists::default(),
        metadata,
    };
    image.validate_for(mode)?;
    Ok(image)
}

/// Native decoders retain their samples and file colour data. Missing calibration uses the existing
/// table first, then rawler's public camera database (including aliases), without decoding a file.
pub(crate) fn fill_color(color: &mut ColorData, make: &str, model: &str) {
    crate::camera_matrices::fill(color, make, model);
    if color.color_matrix.iter().any(Option::is_some) {
        return;
    }
    if let Some(calibration) = runtime_color(make, model) {
        color.color_matrix = calibration.color_matrix;
        color.illuminant = calibration.illuminant;
    }
}
fn runtime_color(make: &str, model: &str) -> Option<&'static ColorData> {
    static CAMERAS: std::sync::OnceLock<std::collections::BTreeMap<String, ColorData>> = std::sync::OnceLock::new();
    let cameras = CAMERAS.get_or_init(|| {
        let mut entries: Vec<_> = rawler::global_loader().get_cameras().iter().collect();
        entries.sort_by_key(|(key, _)| *key);
        let mut out = std::collections::BTreeMap::new();
        for ((make, model, _mode), camera) in entries {
            let mut color = ColorData::default();
            fill_matrices(&mut color, &camera.color_matrix);
            for key in [format!("{make}{model}"), format!("{}{}", camera.clean_make, camera.clean_model)] {
                out.entry(crate::camera_matrices::normalized(&key)).or_insert_with(|| color.clone());
            }
        }
        out
    });
    cameras.get(&crate::camera_matrices::normalized(&format!("{make}{model}")))
}

fn fill_matrices(color: &mut ColorData, data: &std::collections::HashMap<Illuminant, Vec<f32>>) {
    let mut matrices: Vec<_> = data.iter().filter(|(_, v)| v.len() == 9 && v.iter().all(|v| v.is_finite())).collect();
    matrices.sort_by_key(|(i, _)| match i {
        Illuminant::A => (0, 0),
        Illuminant::D65 => (1, 0),
        other => (2, u16::from(**other)),
    });
    for (index, (illuminant, values)) in matrices.into_iter().take(2).enumerate() {
        color.color_matrix[index] = Some(Mat3(std::array::from_fn(|y| std::array::from_fn(|x| f64::from(values[y * 3 + x])))));
        color.illuminant[index] = u16::from(*illuminant);
    }
}

fn be32(bytes: &[u8], at: usize) -> Result<usize> {
    let b = bytes.get(at..at.saturating_add(4)).ok_or_else(|| corrupt("short integer"))?;
    Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize)
}
fn preflight(bytes: &[u8], format: RawFormat) -> Result<()> {
    if bytes.len() > MAX_FILE {
        return Err(RawError::Limit("raw file size"));
    }
    match format {
        RawFormat::Cr3 => {
            let mut budget = 4096;
            boxes(bytes, 0, bytes.len(), 0, &mut budget)?;
            let cr3 = lightcraft_meta::cr3::parse_cr3(bytes).ok_or_else(|| corrupt("invalid CR3"))?;
            let mut raw_tracks = 0;
            for track in cr3.tracks {
                let (off, len) = track.data.ok_or_else(|| corrupt("missing track sample"))?;
                let sample = bytes.get(off..off.saturating_add(len)).ok_or_else(|| corrupt("track outside file"))?;
                match track.kind {
                    lightcraft_meta::cr3::Cr3TrackKind::Raw { cmp1: Some((at, len)), .. } => {
                        let cmp = bytes.get(at..at.saturating_add(len)).ok_or_else(|| corrupt("CMP1 outside file"))?;
                        crx(cmp, sample)?;
                        raw_tracks += 1;
                    }
                    lightcraft_meta::cr3::Cr3TrackKind::Other(kind) if &kind == b"CTMD" => ctmd(sample)?,
                    _ => {}
                }
            }
            if raw_tracks == 0 {
                return Err(RawError::Unsupported("CR3 without a supported raw track".into()));
            }
        }
        RawFormat::Raf => {
            // Validate every structure rawler's RAF parser reads, including JPEG Exif and FujiIFD.
            let jpeg = be32(bytes, 84)?;
            let raw = be32(bytes, 100)?;
            tiff(bytes, jpeg.checked_add(12).ok_or_else(|| corrupt("RAF JPEG offset"))?)?;
            if matches!(bytes.get(raw..raw.saturating_add(2)), Some(b"II" | b"MM")) {
                tiff(bytes, raw)?;
            }
            let dir = be32(bytes, 92)?;
            let count = be32(bytes, dir)?;
            if count > 4096 {
                return Err(RawError::Limit("RAF records"));
            }
            let mut at = dir + 4;
            for _ in 0..count {
                let b = bytes.get(at..at.saturating_add(4)).ok_or_else(|| corrupt("short RAF record"))?;
                let size = usize::from(u16::from_be_bytes([b[2], b[3]]));
                if u16::from_be_bytes([b[0], b[1]]) == 0x0100 && size >= 4 {
                    let dimensions = bytes.get(at + 4..at + 8).ok_or_else(|| corrupt("short RAF dimensions"))?;
                    geometry(
                        usize::from(u16::from_be_bytes([dimensions[2], dimensions[3]])),
                        usize::from(u16::from_be_bytes([dimensions[0], dimensions[1]])),
                    )?;
                }
                at = at.checked_add(4 + size).filter(|&end| end <= bytes.len()).ok_or_else(|| corrupt("RAF record outside file"))?;
            }
        }
        _ => tiff(bytes, 0)?,
    }
    Ok(())
}

// Validate classic TIFF entries without allocating their values. This includes maker-note and vendor
// child IFDs that a general TIFF parser would not visit. The budget covers the complete metadata graph.
fn tiff(bytes: &[u8], base: usize) -> Result<()> {
    tiff_mode(bytes, base, true)
}
fn tiff_mode(bytes: &[u8], base: usize, chain: bool) -> Result<()> {
    let order = match bytes.get(base..base.saturating_add(2)) {
        Some(b"II") => ByteOrder::Little,
        Some(b"MM") => ByteOrder::Big,
        _ => return Err(corrupt("invalid TIFF byte order")),
    };
    let offset = order.read_u32(bytes, base.saturating_add(4)).ok_or_else(|| corrupt("short TIFF header"))? as usize;
    let mut guard = TiffGuard { bytes, visited: HashSet::new(), values: 0, olympus: None };
    guard.ifd(base.saturating_add(offset), base, order, 0, chain)
}
struct TiffGuard<'a> {
    bytes: &'a [u8],
    visited: HashSet<(usize, usize)>,
    values: usize,
    olympus: Option<usize>,
}
impl TiffGuard<'_> {
    fn ifd(&mut self, at: usize, base: usize, order: ByteOrder, depth: usize, chain: bool) -> Result<()> {
        if depth > 10 || self.visited.len() >= 256 {
            return Err(RawError::Limit("raw metadata nesting"));
        }
        if !self.visited.insert((at, base)) {
            return Err(corrupt("cyclic TIFF IFD"));
        }
        let entries = order.read_u16(self.bytes, at).ok_or_else(|| corrupt(&format!("short IFD at {at} (base {base})")))? as usize;
        let end = at.checked_add(2 + entries * 12).filter(|&end| end <= self.bytes.len()).ok_or_else(|| corrupt("short IFD entries"))?;
        let mut width = None;
        let mut height = None;
        for p in (at + 2..end).step_by(12) {
            let tag = order.read_u16(self.bytes, p).ok_or_else(|| corrupt("short tag"))?;
            let typ = order.read_u16(self.bytes, p + 2).ok_or_else(|| corrupt("short type"))?;
            let count = order.read_u32(self.bytes, p + 4).ok_or_else(|| corrupt("short count"))? as usize;
            let size = lightcraft_tiff::FieldType::from_u16(typ).map_or(8, |t| t.size());
            if count > MAX_VALUES && (size != 1 || count > MAX_METADATA) {
                return Err(corrupt(&format!("metadata value count {count} at {p}, tag {tag:04x}, type {typ}")));
            }
            let len = count * size;
            self.values = self.values.checked_add(len).filter(|&n| n <= MAX_METADATA).ok_or(RawError::Limit("raw metadata bytes"))?;
            let value = if len <= 4 {
                p + 8
            } else {
                base.saturating_add(order.read_u32(self.bytes, p + 8).ok_or_else(|| corrupt("short offset"))? as usize)
            };
            let data = self.bytes.get(value..value.saturating_add(len)).ok_or_else(|| corrupt("TIFF value outside file"))?;
            let scalar = match typ {
                3 => order.read_u16(data, 0).map(usize::from),
                4 | 13 => order.read_u32(data, 0).map(|v| v as usize),
                _ => None,
            };
            match tag {
                tags::IMAGE_WIDTH | 0xf001 => width = scalar,
                tags::IMAGE_LENGTH | 0xf002 => height = scalar,
                tags::SAMPLES_PER_PIXEL if scalar.is_some_and(|v| v == 0 || v > 4) => return Err(RawError::Limit("raw samples per pixel")),
                tags::TILE_WIDTH | tags::TILE_LENGTH if scalar.is_some_and(|v| v == 0 || v > 65535) => {
                    return Err(RawError::Limit("raw tile dimensions"));
                }
                tags::SUB_IFDS | tags::EXIF_IFD | tags::GPS_IFD | tags::INTEROP_IFD | 0xf000 if matches!(typ, 4 | 13) => {
                    for v in data.as_chunks::<4>().0 {
                        let offset = order.read_u32(v, 0).unwrap_or(0) as usize;
                        if offset != 0 {
                            self.ifd(base.saturating_add(offset), base, order, depth + 1, false)?;
                        }
                    }
                }
                0x2010 | 0x2040 | 0x3000 if self.olympus.is_some() => {
                    let maker_base = self.olympus.unwrap_or(0);
                    let offset = if tag == 0x3000 && typ == 7 {
                        value
                    } else {
                        maker_base.saturating_add(order.read_u32(self.bytes, p + 8).unwrap_or(0) as usize)
                    };
                    self.ifd(offset, maker_base, order, depth + 1, false)?;
                }
                tags::MAKER_NOTE if len >= 12 => self.maker(value, base, order, depth + 1)?,
                _ => {}
            }
        }
        if let (Some(w), Some(h)) = (width, height) {
            geometry(w, h)?;
        }
        if chain && let Some(next) = order.read_u32(self.bytes, end).filter(|&v| v != 0) {
            self.ifd(base.saturating_add(next as usize), base, order, depth + 1, true)?;
        }
        Ok(())
    }
    fn maker(&mut self, at: usize, base: usize, order: ByteOrder, depth: usize) -> Result<()> {
        let note = self.bytes.get(at..).ok_or_else(|| corrupt("maker note offset"))?;
        let (offset, base, order) = if note.starts_with(b"FUJIFILM") {
            (at.saturating_add(ByteOrder::Little.read_u32(note, 8).ok_or_else(|| corrupt("Fuji maker note"))? as usize), at, ByteOrder::Little)
        } else if note.starts_with(b"OLYMPUS\0") {
            self.olympus = Some(at);
            (at + 12, base, order)
        } else if note.starts_with(b"OM SYSTEM\0") {
            self.olympus = Some(at);
            (at + 16, base, order)
        } else if note.starts_with(b"OLYMP\0") {
            (at + 8, base, order)
        } else if note.starts_with(b"Nikon\0\x02") {
            let hdr = at + 10;
            let o = if self.bytes.get(hdr..hdr + 2) == Some(b"II") { ByteOrder::Little } else { ByteOrder::Big };
            (hdr.saturating_add(o.read_u32(self.bytes, hdr + 4).ok_or_else(|| corrupt("Nikon maker note"))? as usize), hdr, o)
        } else if note.starts_with(b"SONY") {
            (at + 12, base, order)
        } else {
            (at, base, order)
        };
        self.ifd(offset, base, order, depth, false)
    }
}

// ISO-BMFF bounds, including allocation-driving sample table counts. Unknown leaf boxes remain
// skippable, so a rawler version bump can add cameras without a model list here.
fn boxes(bytes: &[u8], mut at: usize, end: usize, depth: usize, budget: &mut usize) -> Result<()> {
    if depth > 12 {
        return Err(RawError::Limit("CR3 box nesting"));
    }
    while at < end {
        if *budget == 0 {
            return Err(RawError::Limit("CR3 boxes"));
        }
        *budget -= 1;
        let size = be32(bytes, at)?;
        let kind = bytes.get(at + 4..at + 8).ok_or_else(|| corrupt("short box"))?;
        let (size, header) = match size {
            0 => (end - at, 8),
            1 => {
                let b = bytes.get(at + 8..at + 16).ok_or_else(|| corrupt("short extended box"))?;
                (
                    usize::try_from(u64::from_be_bytes(b.try_into().map_err(|_| corrupt("box size"))?))
                        .map_err(|_| RawError::Limit("CR3 box size"))?,
                    16,
                )
            }
            n => (n, 8),
        };
        let next = at.checked_add(size).filter(|&next| size >= header && next <= end).ok_or_else(|| corrupt("box outside parent"))?;
        let body = at + header;
        let payload = &bytes[body..next];
        match kind {
            b"moov" | b"trak" | b"mdia" | b"minf" | b"stbl" | b"dinf" => boxes(bytes, body, next, depth + 1, budget)?,
            b"uuid" if payload.starts_with(&[0x85, 0xc0, 0xb6, 0x87, 0x82, 0x0f, 0x11, 0xe0, 0x81, 0x11, 0xf4, 0xce, 0x46, 0x2b, 0x6a, 0x48]) => {
                boxes(bytes, body + 16, next, depth + 1, budget)?
            }
            b"stsd" | b"dref" => {
                if payload.len() < 8 {
                    return Err(corrupt("short full box"));
                }
                boxes(bytes, body + 8, next, depth + 1, budget)?;
            }
            b"CRAW" => {
                if payload.len() < 82 {
                    return Err(corrupt("short CRAW"));
                }
                boxes(bytes, body + 82, next, depth + 1, budget)?;
            }
            b"CMT1" | b"CMT2" | b"CMT3" | b"CMT4" => tiff(payload, 0).map_err(|e| corrupt(&format!("CMT {:?}: {e}", kind)))?,
            b"stsz" => {
                table(payload, 8, 12, if be32(payload, 4)? == 0 { 4 } else { 0 })?;
            }
            b"co64" => table(payload, 4, 8, 8)?,
            b"stco" | b"stss" => table(payload, 4, 8, 4)?,
            b"stsc" => table(payload, 4, 8, 12)?,
            b"stts" | b"ctts" => table(payload, 4, 8, 8)?,
            b"CCTP" => {
                if payload.len() < 12 {
                    return Err(corrupt("short CCTP"));
                }
                boxes(bytes, body + 12, next, depth + 1, budget)?;
            }
            b"CDI1" => {
                if payload.len() < 4 {
                    return Err(corrupt("short CDI1"));
                }
                boxes(bytes, body + 4, next, depth + 1, budget)?;
            }
            b"CTBO" => table(payload, 0, 4, 20)?,
            _ => {}
        }
        at = next;
    }
    Ok(())
}
fn table(data: &[u8], count_at: usize, start: usize, entry: usize) -> Result<()> {
    let count = be32(data, count_at)?;
    if count > 65536 {
        return Err(RawError::Limit("CR3 sample table"));
    }
    if start + count * entry > data.len() {
        return Err(corrupt("short sample table"));
    }
    Ok(())
}
fn ctmd(mut data: &[u8]) -> Result<()> {
    while data.len() >= 12 {
        let size = u32::from_le_bytes(data[..4].try_into().map_err(|_| corrupt("CTMD record"))?) as usize;
        if size < 12 || size > data.len() {
            return Err(corrupt("CTMD size"));
        }
        let kind = u16::from_le_bytes([data[4], data[5]]);
        if (7..=12).contains(&kind) {
            let mut blocks = &data[12..size];
            while blocks.len() >= 8 {
                let len = u32::from_le_bytes(blocks[..4].try_into().map_err(|_| corrupt("CTMD block"))?) as usize;
                if len < 8 || len > blocks.len() {
                    return Err(corrupt("CTMD block size"));
                }
                // Exif and maker-note blocks contain a complete little/big-endian TIFF.
                if matches!(blocks[8..len].get(..2), Some(b"II" | b"MM")) {
                    tiff_mode(&blocks[8..len], 0, false).map_err(|e| corrupt(&format!("CTMD type {kind}: {e}")))?;
                }
                blocks = &blocks[len..];
            }
        }
        data = &data[size..];
    }
    Ok(())
}
fn crx(cmp: &[u8], data: &[u8]) -> Result<()> {
    if cmp.len() < 36 {
        return Err(corrupt("short CMP1"));
    }
    let (w, h) = (be32(cmp, 8)?, be32(cmp, 12)?);
    geometry(w, h)?;
    let (tw, th) = (be32(cmp, 16)?, be32(cmp, 20)?);
    if tw == 0
        || th == 0
        || tw > w
        || th > h
        || w % 2 != 0
        || h % 2 != 0
        || tw % 2 != 0
        || th % 2 != 0
        || w.div_ceil(tw) > 255
        || h.div_ceil(th) > 255
        || !(8..=16).contains(&cmp[24])
        || cmp[25] >> 4 != 4
        || cmp[26] >> 4 > 3
        || cmp[26] & 15 > 3
    {
        return Err(corrupt("invalid CRX parameters"));
    }
    let header = be32(cmp, 28)?;
    if header > data.len() || header > MAX_METADATA {
        return Err(corrupt("CRX header outside sample"));
    }
    // Validate the allocation-driving tile/plane/band graph before rawler constructs it.
    let hdr = &data[..header];
    let mut at = 0;
    let mut total = 0;
    let read16 = |at: usize| -> Result<usize> {
        let b = hdr.get(at..at.saturating_add(2)).ok_or_else(|| corrupt("short CRX descriptor"))?;
        Ok(usize::from(u16::from_be_bytes([b[0], b[1]])))
    };
    let levels = usize::from(cmp[26] & 15);
    for _ in 0..w.div_ceil(tw) * h.div_ceil(th) {
        let marker = read16(at)?;
        let size = read16(at + 2)?;
        if !matches!((marker, size), (0xff01, 8) | (0xff11, 16)) {
            return Err(corrupt("CRX tile descriptor"));
        }
        let tile_size = be32(hdr, at + 4)?;
        let extra = if size == 16 { be32(hdr, at + 12)? + read16(at + 16)? } else { 0 };
        at += 4 + size;
        let mut planes = 0;
        for _ in 0..4 {
            let marker = read16(at)?;
            if !matches!(marker, 0xff02 | 0xff12) || read16(at + 2)? != 8 {
                return Err(corrupt("CRX plane descriptor"));
            }
            let plane_size = be32(hdr, at + 4)?;
            at += 12;
            let mut bands = 0;
            for _ in 0..3 * levels + 1 {
                let marker = read16(at)?;
                let size = read16(at + 2)?;
                if !matches!((marker, size), (0xff03, 8) | (0xff13, 16)) {
                    return Err(corrupt("CRX subband descriptor"));
                }
                let band_size = be32(hdr, at + 4)?;
                let unused = if size == 8 { be32(hdr, at + 8)? & 0x7ffff } else { read16(at + 16)? };
                if band_size == 0 || unused > band_size || band_size > data.len() {
                    return Err(corrupt("CRX subband size"));
                }
                bands += band_size;
                at += 4 + size;
            }
            if bands != plane_size {
                return Err(corrupt("CRX plane size"));
            }
            planes += plane_size;
        }
        if planes + extra != tile_size {
            return Err(corrupt("CRX tile size"));
        }
        total += tile_size;
        if total > data.len() - header {
            return Err(corrupt("CRX tile outside sample"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lightcraft_tiff::{IfdBuilder, TiffWriter, Value};
    use rawler::imgop::{Dim2, Point};
    use rawler::rawimage::{CFAConfig, WhiteLevel};

    fn sensor() -> rawler::RawImage {
        let camera = rawler::global_loader().get_cameras().values().find(|c| c.make == "Canon" && c.model == "Canon EOS R6").unwrap().clone();
        rawler::RawImage {
            make: camera.make.clone(),
            model: camera.model.clone(),
            clean_make: camera.clean_make.clone(),
            clean_model: camera.clean_model.clone(),
            width: 8,
            height: 8,
            cpp: 1,
            bps: 14,
            wb_coeffs: [2.0, 1.0, 1.5, f32::NAN],
            whitelevel: WhiteLevel(vec![16383]),
            blacklevel: rawler::rawimage::BlackLevel::new(&[10u16, 20, 30, 40], 2, 2, 1),
            xyz_to_cam: [[0.0; 3]; 4],
            photometric: RawPhotometricInterpretation::Cfa(CFAConfig::new_from_camera(&camera)),
            active_area: Some(rawler::imgop::Rect::new(Point::new(1, 3), Dim2::new(6, 4))),
            crop_area: Some(rawler::imgop::Rect::new(Point::new(2, 4), Dim2::new(4, 2))),
            blackareas: Vec::new(),
            orientation: rawler::Orientation::Normal,
            data: rawler::RawImageData::Integer(vec![1024; 64]),
            color_matrix: camera.color_matrix.clone(),
            dng_tags: Default::default(),
            fuji_rotation_width: None,
            camera,
        }
    }
    fn metadata() -> Vec<u8> {
        let mut ifd = IfdBuilder::new();
        ifd.set(tags::ORIENTATION, Value::Short(vec![6]));
        TiffWriter::default().write(&[ifd]).unwrap()
    }
    #[test]
    fn rawler_contract_maps_sensor_origin_crop_wb_levels_and_file_orientation() {
        let bytes = metadata();
        let image = map_image(sensor(), &bytes, RawFormat::Cr3, Mode::Full).unwrap();
        assert_eq!(image.black.values, [40.0, 30.0, 20.0, 10.0]);
        assert_eq!(image.active_area, Rect::new(1, 3, 6, 4));
        assert_eq!(image.crop, Rect::new(1, 1, 4, 2));
        assert_eq!(image.cfa.unwrap().name(), "RGGB");
        assert_eq!(image.white, [16383.0]);
        assert_eq!(image.wb_multipliers, Some([2.0, 1.0, 1.5]));
        assert_eq!(image.orientation, crate::Orientation::Rotate90);
        let header = map_image(sensor(), &bytes, RawFormat::Cr3, Mode::Header).unwrap();
        assert!(header.data.is_empty());
        assert_eq!(header.bits, 14);
        assert_eq!(header.metadata.width, Some(4));
    }
    #[test]
    fn rawler_contract_prefers_existing_calibration_and_maps_runtime_illuminants() {
        let raw = sensor();
        let mut color = ColorData::default();
        fill_color(&mut color, &raw.make, &raw.model);
        assert_eq!(color.color_matrix, crate::camera_matrices::get(&raw.make, &raw.model).unwrap().0);
        let saved = color.clone();
        fill_color(&mut color, "Unknown", "Camera");
        assert_eq!(color, saved);
        let mut runtime = ColorData::default();
        fill_matrices(&mut runtime, &raw.color_matrix);
        assert_eq!(runtime.illuminant, [17, 21]);
        assert_eq!(runtime_color(&raw.make, &raw.model).unwrap(), &runtime);
        assert!(runtime_color("Unknown", "Camera").is_none());
        for (i, code) in [(0, Illuminant::A), (1, Illuminant::D65)] {
            let values = &raw.color_matrix[&code];
            for y in 0..3 {
                for x in 0..3 {
                    assert_eq!(runtime.color_matrix[i].unwrap().0[y][x], f64::from(values[y * 3 + x]));
                }
            }
        }
    }
    #[test]
    fn rawler_contract_rejects_invalid_mapping() {
        for kind in 0..6 {
            let mut raw = sensor();
            match kind {
                0 => raw.width = usize::MAX,
                1 => raw.crop_area.as_mut().unwrap().p.x = usize::MAX,
                2 => raw.data = rawler::RawImageData::Integer(vec![0]),
                3 => raw.blacklevel.levels.clear(),
                4 => raw.whitelevel.0 = vec![0],
                _ => raw.cpp = 3,
            }
            assert!(map_image(raw, &metadata(), RawFormat::Cr3, Mode::Full).is_err());
        }
    }
    #[test]
    fn rawler_contract_bounds_hostile_metadata_and_box_tables() {
        let mut ifd = IfdBuilder::new();
        ifd.set(tags::IMAGE_WIDTH, Value::Long(vec![60000]));
        ifd.set(tags::IMAGE_LENGTH, Value::Long(vec![60000]));
        assert!(matches!(tiff(&TiffWriter::default().write(&[ifd]).unwrap(), 0), Err(RawError::Limit(_))));
        let mut table_data = vec![0; 12];
        table_data[8..12].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(matches!(table(&table_data, 8, 12, 4), Err(RawError::Limit(_))));
        let mut cyclic = b"II*\0\x08\0\0\0\0\0\x08\0\0\0".to_vec();
        assert!(tiff(&cyclic, 0).is_err());
        cyclic[8..10].copy_from_slice(&1u16.to_le_bytes());
        cyclic.resize(26, 0);
        cyclic[10..12].copy_from_slice(&1u16.to_le_bytes());
        cyclic[12..14].copy_from_slice(&7u16.to_le_bytes());
        cyclic[14..18].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(tiff(&cyclic, 0).is_err());
        assert!(boxes(&[0; 8], 0, 8, 13, &mut 100).is_err());
        let mut ifd = IfdBuilder::new();
        ifd.set(tags::SAMPLES_PER_PIXEL, Value::Long(vec![u32::MAX]));
        assert!(matches!(tiff(&TiffWriter::default().write(&[ifd]).unwrap(), 0), Err(RawError::Limit(_))));
        let mut raf = vec![0; 128];
        raf[84..88].copy_from_slice(&108u32.to_be_bytes());
        raf[92..96].copy_from_slice(&108u32.to_be_bytes());
        raf[100..104].copy_from_slice(&128u32.to_be_bytes());
        // The JPEG Exif TIFF at 120 is empty; the records at 108 claim a 65535-square sensor.
        raf[120..128].copy_from_slice(b"II*\0\x08\0\0\0");
        raf.extend_from_slice(&[0; 6]);
        raf[108..112].copy_from_slice(&1u32.to_be_bytes());
        raf[112..116].copy_from_slice(&[1, 0, 0, 4]);
        raf[116..120].fill(0xff);
        assert!(matches!(preflight(&raf, RawFormat::Raf), Err(RawError::Limit(_))));
    }
}
