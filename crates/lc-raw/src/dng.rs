//! DNG (Adobe Digital Negative Specification 1.7): raw IFD selection, pixel data (via [`crate::tiffraw`]),
//! linearization, black/white levels, active area, default crop, CFA description, colour tags, opcode lists.

use crate::profile::{HsvTable, ProfileLook, ToneCurve};
use crate::tiffraw::{Packing, read_image_in};
use crate::{BlackLevel, Cfa, ColorData, Mat3, Mode, RawData, RawError, RawFormat, RawImage, Rect, Result, opcodes};
use li_color::Xy;
use lightcraft_geom::Orientation;
use lightcraft_tiff::tags::{self as t, photometric};
use lightcraft_tiff::{ByteOrder, Ifd, Tiff};

/// The main raw IFD: full-resolution (NewSubfileType 0) CFA or LinearRaw image with the most pixels.
pub(crate) fn raw_ifd(tiff: &Tiff) -> Option<&Ifd> {
    tiff.all_ifds()
        .into_iter()
        .filter(|i| i.u32(t::NEW_SUBFILE_TYPE).unwrap_or(0) == 0)
        .filter(|i| matches!(i.u16(t::PHOTOMETRIC), Some(photometric::CFA) | Some(photometric::LINEAR_RAW)))
        .max_by_key(|i| i.u64(t::IMAGE_WIDTH).unwrap_or(0).saturating_mul(i.u64(t::IMAGE_LENGTH).unwrap_or(0)))
}

fn mat3(v: Option<Vec<f64>>) -> Option<Mat3> {
    let v = v?;
    if v.len() != 9 || v.iter().any(|x| !x.is_finite()) {
        return None;
    }
    let m = Mat3([[v[0], v[1], v[2]], [v[3], v[4], v[5]], [v[6], v[7], v[8]]]);
    (m.determinant().abs() > 1e-12).then_some(m)
}

fn vec3(v: Option<Vec<f64>>) -> Option<[f64; 3]> {
    let v = v?;
    (v.len() == 3 && v.iter().all(|x| x.is_finite() && *x > 0.0)).then(|| [v[0], v[1], v[2]])
}

/// Decode DNG 1.6 IlluminantData's xy or spectral payload in the enclosing TIFF's byte order.
fn illuminant_data(bytes: &[u8], order: ByteOrder) -> Option<Xy> {
    let rational = |offset| {
        let n = f64::from(order.read_u32(bytes, offset)?);
        let d = f64::from(order.read_u32(bytes, offset + 4)?);
        (d > 0.0).then_some(n / d)
    };
    let xy = match order.read_u16(bytes, 0)? {
        0 => Xy::new(rational(2)?, rational(10)?),
        1 => {
            let count = usize::try_from(order.read_u32(bytes, 2)?).ok()?;
            if !(2..=1000).contains(&count) || bytes.len() < 22 + count * 8 {
                return None;
            }
            let start = rational(6)?;
            let spacing = rational(14)?;
            if start <= 0.0 || spacing <= 0.0 {
                return None;
            }
            let samples: Vec<f64> = (0..count).map(|i| rational(22 + 8 * i)).collect::<Option<_>>()?;
            let observer = cie_observer();
            let mut sum = [0.0; 3];
            let mut observer_sum = [0.0; 3];
            for (i, xyz) in observer.iter().enumerate() {
                let pos = ((360.0 + i as f64 - start) / spacing).clamp(0.0, (count - 1) as f64);
                let lower = (pos as usize).min(count - 2);
                let fraction = pos - lower as f64;
                let light = samples[lower] + fraction * (samples[lower + 1] - samples[lower]);
                for j in 0..3 {
                    sum[j] += xyz[j] * light;
                    observer_sum[j] += xyz[j];
                }
            }
            sum = std::array::from_fn(|j| sum[j] / observer_sum[j]);
            if sum.iter().any(|v| !v.is_finite() || *v <= 0.0) {
                return None;
            }
            Xy::from_xyz(sum)
        }
        _ => return None,
    };
    (xy.x.is_finite() && xy.y.is_finite() && xy.x > 0.0 && xy.y > 0.0 && xy.x + xy.y < 1.0).then_some(xy)
}

fn cie_observer() -> &'static [[f64; 3]] {
    static DATA: std::sync::OnceLock<Vec<[f64; 3]>> = std::sync::OnceLock::new();
    DATA.get_or_init(|| {
        include_str!("../data/color/CIE_xyz_1931_2deg.csv")
            .lines()
            .filter_map(|line| {
                let mut fields = line.split(',').skip(1);
                Some([fields.next()?.parse().ok()?, fields.next()?.parse().ok()?, fields.next()?.parse().ok()?])
            })
            .collect()
    })
}

/// Colour tags: DNG puts them in IFD0, but some writers use the raw IFD; prefer the raw IFD.
pub(crate) fn color_data(ifd0: &Ifd, raw: &Ifd, order: ByteOrder) -> ColorData {
    let get = |tag: u16| raw.f64s(tag).or_else(|| ifd0.f64s(tag));
    let geti = |tag: u16| raw.u16(tag).or_else(|| ifd0.u16(tag));
    let gets = |tag: u16| {
        let ifd = if raw.contains(tag) { raw } else { ifd0 };
        ifd.string(tag).unwrap_or_default()
    };
    ColorData {
        illuminant: [geti(t::CALIBRATION_ILLUMINANT_1).unwrap_or(0), geti(t::CALIBRATION_ILLUMINANT_2).unwrap_or(0)],
        illuminant_3: geti(t::CALIBRATION_ILLUMINANT_3).unwrap_or(0),
        illuminant_white_xy: [t::ILLUMINANT_DATA_1, t::ILLUMINANT_DATA_2, t::ILLUMINANT_DATA_3]
            .map(|tag| raw.bytes(tag).or_else(|| ifd0.bytes(tag)).and_then(|bytes| illuminant_data(bytes, order))),
        color_matrix: [mat3(get(t::COLOR_MATRIX_1)), mat3(get(t::COLOR_MATRIX_2))],
        color_matrix_3: mat3(get(t::COLOR_MATRIX_3)),
        forward_matrix_3: mat3(get(t::FORWARD_MATRIX_3)),
        camera_calibration_3: mat3(get(t::CAMERA_CALIBRATION_3)),
        forward_matrix: [mat3(get(t::FORWARD_MATRIX_1)), mat3(get(t::FORWARD_MATRIX_2))],
        camera_calibration: [mat3(get(t::CAMERA_CALIBRATION_1)), mat3(get(t::CAMERA_CALIBRATION_2))],
        camera_calibration_signature: gets(t::CAMERA_CALIBRATION_SIGNATURE),
        profile_calibration_signature: gets(t::PROFILE_CALIBRATION_SIGNATURE),
        analog_balance: vec3(get(t::ANALOG_BALANCE)),
        as_shot_neutral: vec3(get(t::AS_SHOT_NEUTRAL)),
        as_shot_white_xy: get(t::AS_SHOT_WHITE_XY).filter(|v| v.len() == 2 && v[0] > 0.0 && v[1] > 0.0).map(|v| Xy::new(v[0], v[1])),
        baseline_exposure: get(t::BASELINE_EXPOSURE).and_then(|v| v.first().copied()).filter(|v| v.is_finite()).unwrap_or(0.0)
            + get(t::BASELINE_EXPOSURE_OFFSET).and_then(|v| v.first().copied()).filter(|v| v.is_finite()).unwrap_or(0.0),
        profile: profile_look(ifd0, raw),
    }
}

/// The profile look tags (`ProfileHueSatMap*`, `ProfileLookTable*`, `ProfileToneCurve`); malformed
/// ones are ignored. Like the colour tags, read from the raw IFD first, else IFD 0.
pub(crate) fn profile_look(ifd0: &Ifd, raw: &Ifd) -> ProfileLook {
    let pick = |tag: u16| if raw.contains(tag) { raw } else { ifd0 };
    let table = |dims: u16, data: u16, enc: u16| {
        let ifd = pick(dims);
        let dims = ifd.u64s(dims)?;
        let data = ifd.f64s(data)?;
        HsvTable::from_tags(&dims, &data, ifd.u64s(enc).and_then(|v| v.first().copied()).unwrap_or(0))
    };
    ProfileLook {
        hue_sat_map: [
            table(t::PROFILE_HUE_SAT_MAP_DIMS, t::PROFILE_HUE_SAT_MAP_DATA_1, t::PROFILE_HUE_SAT_MAP_ENCODING),
            table(t::PROFILE_HUE_SAT_MAP_DIMS, t::PROFILE_HUE_SAT_MAP_DATA_2, t::PROFILE_HUE_SAT_MAP_ENCODING),
        ],
        hue_sat_map_3: table(t::PROFILE_HUE_SAT_MAP_DIMS, 52537, t::PROFILE_HUE_SAT_MAP_ENCODING),
        look_table: table(t::PROFILE_LOOK_TABLE_DIMS, t::PROFILE_LOOK_TABLE_DATA, t::PROFILE_LOOK_TABLE_ENCODING),
        tone_curve: pick(t::PROFILE_TONE_CURVE).f64s(t::PROFILE_TONE_CURVE).and_then(|v| ToneCurve::from_tag(&v)),
    }
}

/// Some camera DNGs underreport a terminal uncompressed StripByteCounts while
/// the complete declared image is present at EOF (CC0 Mavic 3, 5140). Recover
/// only that unambiguous layout; never consume trailing metadata as pixels or
/// manufacture missing samples. LibRaw's packed DNG loader reads these full rows.
fn uncompressed_layout(bytes: &[u8], info: &mut lightcraft_tiff::image::ImageInfo, bits: u32) -> Result<()> {
    use lightcraft_tiff::image::{Layout, chunk_bytes};
    if info.compression != t::compression::NONE {
        return Ok(());
    }
    let cpp = info.samples_per_pixel as usize;
    let row_bytes = (info.width as usize)
        .checked_mul(cpp)
        .and_then(|n| n.checked_mul(bits as usize))
        .and_then(|n| n.checked_add(7))
        .map(|n| n / 8)
        .ok_or(RawError::Limit("DNG row size"))?;
    if matches!(info.layout, Layout::Strips { rows_per_strip } if rows_per_strip >= info.height)
        && info.offsets.len() == 1
        && info.byte_counts.len() == 1
    {
        let expected = row_bytes.checked_mul(info.height as usize).ok_or(RawError::Limit("DNG strip size"))?;
        let available = usize::try_from(info.offsets[0]).ok().and_then(|at| bytes.len().checked_sub(at));
        if info.byte_counts[0] < expected as u64 && available == Some(expected) {
            info.byte_counts[0] = expected as u64;
        }
    }
    // Edge padding outside the image may be omitted, but every covered pixel
    // must exist. Short uncompressed input is corruption, not zero-filled detail.
    let ccpp = if info.planar == 2 { 1 } else { cpp };
    for c in info.chunks(bytes.len() as u64) {
        if c.x >= info.width || c.y >= info.height {
            continue;
        }
        let rows = c.height.min(info.height - c.y) as usize;
        let cols = c.width.min(info.width - c.x) as usize;
        let pitch = (c.width as usize)
            .checked_mul(ccpp)
            .and_then(|n| n.checked_mul(bits as usize))
            .and_then(|n| n.checked_add(7))
            .map(|n| n / 8)
            .ok_or(RawError::Limit("DNG chunk row"))?;
        let tail = cols
            .checked_mul(ccpp)
            .and_then(|n| n.checked_mul(bits as usize))
            .and_then(|n| n.checked_add(7))
            .map(|n| n / 8)
            .ok_or(RawError::Limit("DNG chunk tail"))?;
        let needed = rows.saturating_sub(1).checked_mul(pitch).and_then(|n| n.checked_add(tail)).ok_or(RawError::Limit("DNG chunk size"))?;
        if chunk_bytes(bytes, &c).is_none_or(|src| src.len() < needed) {
            return Err(RawError::Corrupt("truncated uncompressed DNG samples".into()));
        }
    }
    Ok(())
}

pub(crate) fn decode(bytes: &[u8], mode: Mode) -> Result<RawImage> {
    let tiff = Tiff::parse(bytes)?;
    let ifd0 = &tiff.ifds[0];
    let raw = raw_ifd(&tiff).ok_or_else(|| RawError::Corrupt("DNG without a raw image IFD".into()))?;
    let mut info = raw.image()?;
    let storage_bits = info.bits() as u32;
    uncompressed_layout(bytes, &mut info, storage_bits)?;
    let (w, h, cpp) = (info.width as usize, info.height as usize, info.samples_per_pixel as usize);
    // one sample (CFA or monochrome), RGB or four colours; two are neither and can't be developed
    if !matches!(cpp, 1 | 3 | 4) {
        return Err(RawError::Unsupported(format!("{cpp} samples per pixel")));
    }
    if info.compression == t::compression::JPEG_XL {
        return Err(RawError::Unsupported("JPEG XL DNG".into()));
    }
    let mut data = read_image_in(mode, bytes, &info, tiff.order, Packing::Msb)?;
    let bits = info.bits() as u32;

    // linearization table (integer data only)
    let mut linearized = false;
    if let (RawData::U16(v), Some(table)) = (&mut data, raw.u64s(t::LINEARIZATION_TABLE).filter(|t| !t.is_empty())) {
        let table: Vec<u16> = table.iter().map(|&x| x.min(65535) as u16).collect();
        let last = table.len() - 1;
        v.iter_mut().for_each(|s| *s = table[(*s as usize).min(last)]);
        linearized = true;
    }

    let float = matches!(data, RawData::F32(_));
    let default_white = if float { 1.0 } else { ((1u64 << bits.min(16)) - 1) as f32 };
    let mut white: Vec<f32> = raw.f64s(t::WHITE_LEVEL).unwrap_or_default().into_iter().map(|v| v as f32).filter(|v| *v > 0.0).collect();
    if white.is_empty() {
        white = vec![default_white];
    }
    // No sensor records NaN or infinity, so such a float sample is damage. Left in, it turns into NaN in demosaic
    // arithmetic and spreads; NaN and −∞ read as black (0), +∞ as the white level (a clipped highlight). Finite
    // samples, negative or above white included, are kept as stored.
    if let RawData::F32(v) = &mut data {
        for (i, s) in v.iter_mut().enumerate() {
            if !s.is_finite() {
                *s = if *s > 0.0 { white.get(i % cpp).or(white.first()).copied().unwrap_or(default_white) } else { 0.0 };
            }
        }
    }

    let dim = raw.u64s(t::BLACK_LEVEL_REPEAT_DIM).unwrap_or_default();
    let (br, bc) = match dim.as_slice() {
        [r, c] if (1..=16).contains(r) && (1..=16).contains(c) => (*r as usize, *c as usize),
        _ => (1, 1),
    };
    // (a FLOAT / DOUBLE tag can hold NaN or infinity: such a level reads as 0, in its place)
    let level = |v: f64| Some(v as f32).filter(|v| v.is_finite()).unwrap_or(0.0);
    let mut values: Vec<f32> = raw.f64s(t::BLACK_LEVEL).unwrap_or_default().into_iter().map(level).collect();
    if values.len() != br * bc * cpp {
        values = vec![values.first().copied().unwrap_or(0.0)];
    }
    let (br, bc) = if values.len() == 1 { (1, 1) } else { (br, bc) };
    let floats = |tag| raw.f64s(tag).unwrap_or_default().into_iter().map(level).collect::<Vec<f32>>();
    let black =
        BlackLevel { repeat_rows: br, repeat_cols: bc, values, delta_h: floats(t::BLACK_LEVEL_DELTA_H), delta_v: floats(t::BLACK_LEVEL_DELTA_V) };

    let active_area = match raw.u64s(t::ACTIVE_AREA).as_deref() {
        Some([top, left, bottom, right]) if bottom > top && right > left && (*bottom as usize) <= h && (*right as usize) <= w => {
            Rect::new(*left as usize, *top as usize, (*right - *left) as usize, (*bottom - *top) as usize)
        }
        _ => Rect::new(0, 0, w, h),
    };
    let crop = match (raw.f64s(t::DEFAULT_CROP_ORIGIN).as_deref(), raw.f64s(t::DEFAULT_CROP_SIZE).as_deref()) {
        (Some([x, y]), Some([cw, ch])) if *cw >= 1.0 && *ch >= 1.0 && *x >= 0.0 && *y >= 0.0 => {
            Rect::new(x.round() as usize, y.round() as usize, cw.round() as usize, ch.round() as usize).clipped(active_area.width, active_area.height)
        }
        _ => Rect::new(0, 0, active_area.width, active_area.height),
    };

    let cfa = if info.photometric == photometric::CFA {
        if raw.u16(t::CFA_LAYOUT).unwrap_or(1) != 1 {
            return Err(RawError::Unsupported("non-rectangular CFA layout".into()));
        }
        let dim = raw.u64s(t::CFA_REPEAT_PATTERN_DIM).unwrap_or_else(|| vec![2, 2]);
        let (rows, cols) = match dim.as_slice() {
            [r, c] if (1..=16).contains(r) && (1..=16).contains(c) => (*r as usize, *c as usize),
            _ => return Err(RawError::Corrupt("bad CFARepeatPatternDim".into())),
        };
        let pat = raw.bytes(t::CFA_PATTERN_EP).ok_or_else(|| RawError::Corrupt("missing CFAPattern".into()))?;
        if pat.len() != rows * cols {
            return Err(RawError::Corrupt("CFAPattern size mismatch".into()));
        }
        let planes = raw.bytes(t::CFA_PLANE_COLOR).map(|b| b.to_vec()).unwrap_or_else(|| vec![0, 1, 2]);
        let pattern: Vec<u8> = pat
            .iter()
            .map(|c| planes.iter().position(|p| p == c).map(|i| i as u8).filter(|&i| i < 3))
            .collect::<Option<_>>()
            .ok_or_else(|| RawError::Unsupported("CFA with more than three colours".into()))?;
        if cpp != 1 {
            return Err(RawError::Unsupported("CFA data with several samples per pixel".into()));
        }
        Some(Cfa { width: cols, height: rows, pattern })
    } else {
        None
    };

    let opcode_list = |tag| raw.bytes(tag).map(opcodes::parse_list).unwrap_or_default();
    let opcodes =
        crate::OpcodeLists { list1: opcode_list(t::OPCODE_LIST_1), list2: opcode_list(t::OPCODE_LIST_2), list3: opcode_list(t::OPCODE_LIST_3) };

    let mut metadata = lightcraft_meta::from_tiff(&tiff);
    metadata.width = Some(crop.width as u32);
    metadata.height = Some(crop.height as u32);
    let img = RawImage {
        format: RawFormat::Dng,
        width: w,
        height: h,
        cpp,
        data,
        cfa,
        bits,
        black,
        white,
        active_area,
        crop,
        orientation: Orientation::from_exif(ifd0.u16(t::ORIENTATION).unwrap_or(1)),
        color: color_data(ifd0, raw, tiff.order),
        wb_multipliers: None,
        linearized,
        opcodes,
        vendor_lens: None,
        metadata,
    };
    img.validate_for(mode)?;
    Ok(img)
}

#[cfg(test)]
mod sony_dji_strip_tests {
    use super::*;
    use crate::{BlackLevel, DngCompression, DngWriteOptions, RawFormat, write_dng};

    #[test]
    fn sony_dji_terminal_strip_count_recovery_requires_complete_eof_pixels() {
        let raw = RawImage {
            vendor_lens: None,
            format: RawFormat::Dng,
            width: 4,
            height: 3,
            cpp: 1,
            data: RawData::U16((1000..1012).collect()),
            cfa: Cfa::bayer("RGGB"),
            bits: 16,
            black: BlackLevel::uniform(0.0),
            white: vec![65535.0],
            active_area: Rect::new(0, 0, 4, 3),
            crop: Rect::new(0, 0, 4, 3),
            orientation: Orientation::Normal,
            color: Default::default(),
            wb_multipliers: None,
            linearized: false,
            opcodes: Default::default(),
            metadata: Default::default(),
        };
        let mut bytes = write_dng(&raw, &DngWriteOptions { compression: DngCompression::Uncompressed, ..Default::default() }).unwrap();
        let tiff = Tiff::parse(&bytes).unwrap();
        let ifd = raw_ifd(&tiff).unwrap();
        let count_at = ifd.get(t::STRIP_BYTE_COUNTS).unwrap().offset as usize;
        let data_at = ifd.u64(t::STRIP_OFFSETS).unwrap() as usize;
        let offset_at = ifd.get(t::STRIP_OFFSETS).unwrap().offset as usize;
        // TiffWriter places pixels before the directory. Relocate this complete
        // strip to EOF to reproduce the independently observed camera layout.
        let pixels = bytes[data_at..data_at + 24].to_vec();
        let terminal_at = bytes.len();
        bytes.extend(pixels);
        bytes[offset_at..offset_at + 4].copy_from_slice(&(terminal_at as u32).to_le_bytes());
        assert_eq!(bytes.len() - terminal_at, 24);
        bytes[count_at..count_at + 4].copy_from_slice(&20u32.to_le_bytes());
        let full = decode(&bytes, Mode::Full).unwrap();
        assert_eq!(full.data, raw.data);
        assert_eq!(decode(&bytes, Mode::Header).unwrap().info(), full.info());
        let mut ambiguous = bytes.clone();
        ambiguous.extend([1, 2, 3, 4]);
        assert!(matches!(decode(&ambiguous, Mode::Full), Err(RawError::Corrupt(_))));
        bytes.truncate(bytes.len() - 2);
        assert!(matches!(decode(&bytes, Mode::Full), Err(RawError::Corrupt(_))));
        assert!(matches!(decode(&bytes, Mode::Header), Err(RawError::Corrupt(_))));
    }
}

#[cfg(test)]
mod triple_illuminant_tests {
    use super::*;

    #[test]
    fn triple_illuminant_custom_xy_and_spectrum_endian_and_malformed() {
        assert_eq!(cie_observer().len(), 471);
        for order in [ByteOrder::Little, ByteOrder::Big] {
            let mut xy = Vec::new();
            order.put_u16(&mut xy, 0);
            for (n, d) in [(3457, 10000), (39, 100)] {
                order.put_u32(&mut xy, n);
                order.put_u32(&mut xy, d);
            }
            assert_eq!(illuminant_data(&xy, order), Some(Xy::new(0.3457, 0.39)));
            assert!(illuminant_data(&xy[..17], order).is_none());
            xy[6..10].fill(0);
            assert!(illuminant_data(&xy, order).is_none());
            let mut spectrum = Vec::new();
            order.put_u16(&mut spectrum, 1);
            order.put_u32(&mut spectrum, 2);
            for value in [400, 300, 0, 1] {
                order.put_u32(&mut spectrum, value);
                order.put_u32(&mut spectrum, 1);
            }
            let white = illuminant_data(&spectrum, order).unwrap();
            // Independent CIE 1 nm observer integration with endpoint extension.
            assert!((white.x - 0.4448893988140621).abs() < 1e-13);
            assert!((white.y - 0.41495127408339716).abs() < 1e-13);
            assert!(illuminant_data(&spectrum[..37], order).is_none());
            spectrum[30..34].fill(0);
            assert!(illuminant_data(&spectrum, order).is_none());
        }
    }
}
