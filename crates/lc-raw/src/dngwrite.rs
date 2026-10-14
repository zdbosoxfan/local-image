//! Writing DNG files from a [`RawImage`] (uncompressed or lossless-JPEG tiles), used for DNG export and for
//! bit-exact decoder round-trip tests. The raw image is stored in IFD0 (allowed by DNG 1.7 §"File Structure").

use crate::{RawData, RawError, RawImage, Result, ljpeg, opcodes};
use lightcraft_color::Mat3;
use lightcraft_tiff::tags::{self as t, compression, photometric};
use lightcraft_tiff::writer::{rational, srational};
use lightcraft_tiff::{ByteOrder, IfdBuilder, ImageData, TiffWriter, Value};
use rayon::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DngCompression {
    Uncompressed,
    /// Lossless JPEG tiles of `tile × tile` pixels (`tile` even). CFA tiles are encoded as two interleaved
    /// components of half the tile width (the common DNG layout), linear data with one component per sample.
    Lj92 {
        tile: u32,
    },
    /// Deflate (zlib) tiles of `tile × tile` pixels. Floating-point data is stored as 16-bit (`half`) or
    /// 32-bit IEEE floats with the floating-point predictor (TIFF predictor 3, Adobe TIFF Technical Note 3);
    /// integer data with horizontal differencing (predictor 2). 16-bit float is what HDR merges usually use.
    Deflate {
        tile: u32,
        half: bool,
    },
}

/// IEEE 754 binary32 → binary16 (round to nearest even; overflow → ±inf, NaN kept).
pub fn f32_to_f16(v: f32) -> u16 {
    let b = v.to_bits();
    let sign = ((b >> 16) & 0x8000) as u16;
    let exp = ((b >> 23) & 0xff) as i32;
    let man = b & 0x7f_ffff;
    if exp == 0xff {
        return sign | 0x7c00 | if man != 0 { 0x200 } else { 0 };
    }
    let e = exp - 127 + 15;
    if e >= 0x1f {
        return sign | 0x7c00;
    }
    if e <= 0 {
        if e < -10 {
            return sign;
        }
        // subnormal half
        let m = man | 0x80_0000;
        let shift = (14 - e) as u32;
        let half = m >> shift;
        let rem = m & ((1 << shift) - 1);
        let mid = 1 << (shift - 1);
        let r = if rem > mid || (rem == mid && half & 1 == 1) { half + 1 } else { half };
        return sign | r as u16;
    }
    let half = ((e as u32) << 10) | (man >> 13);
    let rem = man & 0x1fff;
    let r = if rem > 0x1000 || (rem == 0x1000 && half & 1 == 1) { half + 1 } else { half };
    sign | r as u16
}

/// Apply the floating-point predictor (inverse of `unpack::undo_float_predictor`) to one row of
/// `n` big-endian samples of `bytes_per` bytes: split into byte planes (most significant first), then
/// byte-difference with `stride` samples.
fn float_predict(row: &[u8], n: usize, bytes_per: usize, stride: usize) -> Vec<u8> {
    let mut planes = vec![0u8; row.len()];
    for i in 0..n {
        for b in 0..bytes_per {
            planes[b * n + i] = row[i * bytes_per + b];
        }
    }
    for i in (stride..planes.len()).rev() {
        planes[i] = planes[i].wrapping_sub(planes[i - stride]);
    }
    planes
}

#[derive(Clone, Debug, PartialEq)]
pub struct DngWriteOptions {
    pub compression: DngCompression,
    pub order: ByteOrder,
    /// XMP packet to embed (tag 700); defaults to one generated from the image metadata.
    pub xmp: Option<String>,
}

impl Default for DngWriteOptions {
    fn default() -> Self {
        Self { compression: DngCompression::Lj92 { tile: 256 }, order: ByteOrder::Little, xmp: None }
    }
}

fn srat_mat(m: &Mat3) -> Value {
    Value::SRational(m.0.iter().flatten().map(|&v| srational(v)).collect())
}

fn rat_vec(v: &[f64]) -> Value {
    Value::Rational(v.iter().map(|&x| rational(x)).collect())
}

/// Serialise `raw` as a DNG.
pub fn write_dng(raw: &RawImage, opts: &DngWriteOptions) -> Result<Vec<u8>> {
    raw.validate()?;
    // DNG WhiteLevel is per sample, not per CFA colour. Truncating an unequal
    // R/G1/B/G2 table to its red entry would silently change clipping and colour.
    if raw.format != crate::RawFormat::Dng
        && raw.cpp == 1
        && raw.cfa.is_some()
        && raw.white.len() == 4
        && raw.white.iter().any(|w| *w != raw.white[0])
    {
        return Err(RawError::Unsupported("per-channel vendor whites require linearized DNG export".into()));
    }
    let (w, h, cpp) = (raw.width, raw.height, raw.cpp);
    let mut ifd = IfdBuilder::new();
    ifd.set(t::NEW_SUBFILE_TYPE, Value::Long(vec![0]));
    ifd.set(t::IMAGE_WIDTH, Value::Long(vec![w as u32]));
    ifd.set(t::IMAGE_LENGTH, Value::Long(vec![h as u32]));
    ifd.set(t::SAMPLES_PER_PIXEL, Value::Short(vec![cpp as u16]));
    ifd.set(t::PLANAR_CONFIGURATION, Value::Short(vec![1]));
    ifd.set(t::PHOTOMETRIC, Value::Short(vec![if raw.cfa.is_some() { photometric::CFA } else { photometric::LINEAR_RAW }]));
    let extended_color = raw.color.color_matrix_3.is_some()
        || raw.color.illuminant_3 != 0
        || raw.color.illuminant.contains(&255)
        || raw.color.profile.hue_sat_map_3.is_some();
    ifd.set(t::DNG_VERSION, Value::Byte(vec![1, if extended_color { 6 } else { 4 }, 0, 0]));
    ifd.set(t::DNG_BACKWARD_VERSION, Value::Byte(vec![1, 3, 0, 0]));
    let make = raw.metadata.make.clone().unwrap_or_default();
    let model = raw.metadata.model.clone().unwrap_or_default();
    if !make.is_empty() {
        ifd.set(t::MAKE, Value::Ascii(make.clone()));
    }
    if !model.is_empty() {
        ifd.set(t::MODEL, Value::Ascii(model.clone()));
    }
    let unique = format!("{make} {model}").trim().to_string();
    ifd.set(t::UNIQUE_CAMERA_MODEL, Value::Ascii(if unique.is_empty() { "LightCraft".into() } else { unique }));
    ifd.set(t::ORIENTATION, Value::Short(vec![raw.orientation.to_exif()]));
    ifd.set(t::SOFTWARE, Value::Ascii("Local Image".into()));
    // Sony sample curves travel whole through DNG conversion/Pixel Shift output.
    // Darktable's DNG round trip uses these original numeric tags in IFD0.
    // They are vendor data, never fitted into a different polynomial model.
    if let Some(lens) = raw.metadata.sony.as_ref().and_then(|s| s.lens_correction.as_ref()) {
        let n = lens.distortion.len();
        if (2..=16).contains(&n) && lens.ca_red.len() == n && lens.ca_blue.len() == n && lens.vignetting.len() == n {
            let prefixed = |count: usize, values: &[i16]| {
                let mut out = Vec::with_capacity(values.len() + 1);
                out.push(count as i16);
                out.extend_from_slice(values);
                Value::SShort(out)
            };
            ifd.set(0x7037, prefixed(n, &lens.distortion));
            ifd.set(0x7032, prefixed(n, &lens.vignetting));
            let mut ca = lens.ca_red.clone();
            ca.extend_from_slice(&lens.ca_blue);
            ifd.set(0x7035, prefixed(2 * n, &ca));
        }
    }
    let xmp = opts.xmp.clone().unwrap_or_else(|| lightcraft_meta::write_xmp(&raw.metadata, None));
    ifd.set(t::XMP, Value::Byte(xmp.into_bytes()));

    if let Some(cfa) = &raw.cfa {
        ifd.set(t::CFA_REPEAT_PATTERN_DIM, Value::Short(vec![cfa.height as u16, cfa.width as u16]));
        ifd.set(t::CFA_PATTERN_EP, Value::Byte(cfa.pattern.clone()));
        ifd.set(t::CFA_PLANE_COLOR, Value::Byte(vec![0, 1, 2]));
        ifd.set(t::CFA_LAYOUT, Value::Short(vec![1]));
    }
    let b = &raw.black;
    ifd.set(t::BLACK_LEVEL_REPEAT_DIM, Value::Short(vec![b.repeat_rows as u16, b.repeat_cols as u16]));
    let mut bl: Vec<f64> = b.values.iter().map(|&v| v as f64).collect();
    if bl.len() == 1 && cpp > 1 {
        bl = vec![bl[0]; cpp];
    }
    ifd.set(t::BLACK_LEVEL, rat_vec(&bl));
    if !b.delta_h.is_empty() {
        ifd.set(t::BLACK_LEVEL_DELTA_H, Value::SRational(b.delta_h.iter().map(|&v| srational(v as f64)).collect()));
    }
    if !b.delta_v.is_empty() {
        ifd.set(t::BLACK_LEVEL_DELTA_V, Value::SRational(b.delta_v.iter().map(|&v| srational(v as f64)).collect()));
    }
    let white: Vec<u32> = (0..cpp).map(|s| raw.white_at(s).round().clamp(0.0, u32::MAX as f32) as u32).collect();
    if matches!(raw.data, RawData::F32(_)) {
        ifd.set(t::WHITE_LEVEL, rat_vec(&(0..cpp).map(|s| raw.white_at(s) as f64).collect::<Vec<_>>()));
    } else {
        ifd.set(t::WHITE_LEVEL, Value::Long(white));
    }
    let a = raw.active_area;
    ifd.set(t::ACTIVE_AREA, Value::Long(vec![a.y as u32, a.x as u32, (a.y + a.height) as u32, (a.x + a.width) as u32]));
    ifd.set(t::DEFAULT_CROP_ORIGIN, Value::Long(vec![raw.crop.x as u32, raw.crop.y as u32]));
    ifd.set(t::DEFAULT_CROP_SIZE, Value::Long(vec![raw.crop.width as u32, raw.crop.height as u32]));

    let c = &raw.color;
    for (signature, tag) in
        [(&c.camera_calibration_signature, t::CAMERA_CALIBRATION_SIGNATURE), (&c.profile_calibration_signature, t::PROFILE_CALIBRATION_SIGNATURE)]
    {
        if !signature.is_empty() {
            ifd.set(tag, Value::Ascii(signature.clone()));
        }
    }
    for (i, tag) in [t::CALIBRATION_ILLUMINANT_1, t::CALIBRATION_ILLUMINANT_2].into_iter().enumerate() {
        if c.color_matrix[i].is_some() {
            ifd.set(tag, Value::Short(vec![c.illuminant[i]]));
        }
    }
    if c.color_matrix_3.is_some() {
        ifd.set(t::CALIBRATION_ILLUMINANT_3, Value::Short(vec![c.illuminant_3]));
    }
    for (matrix, tag) in
        [(c.color_matrix_3, t::COLOR_MATRIX_3), (c.forward_matrix_3, t::FORWARD_MATRIX_3), (c.camera_calibration_3, t::CAMERA_CALIBRATION_3)]
    {
        if let Some(matrix) = matrix {
            ifd.set(tag, srat_mat(&matrix));
        }
    }
    for (i, tag) in [t::ILLUMINANT_DATA_1, t::ILLUMINANT_DATA_2, t::ILLUMINANT_DATA_3].into_iter().enumerate() {
        if [c.illuminant[0], c.illuminant[1], c.illuminant_3][i] == 255
            && let Some(xy) = c.illuminant_white_xy[i]
        {
            let mut bytes = Vec::with_capacity(18);
            opts.order.put_u16(&mut bytes, 0);
            for value in [xy.x, xy.y] {
                let (numerator, denominator) = rational(value);
                opts.order.put_u32(&mut bytes, numerator);
                opts.order.put_u32(&mut bytes, denominator);
            }
            ifd.set(tag, Value::Undefined(bytes));
        }
    }
    if c.illuminant.contains(&255) {
        ifd.set(t::DNG_BACKWARD_VERSION, Value::Byte(vec![1, 6, 0, 0]));
    }
    for (pair, tags) in [
        (&c.color_matrix, [t::COLOR_MATRIX_1, t::COLOR_MATRIX_2]),
        (&c.forward_matrix, [t::FORWARD_MATRIX_1, t::FORWARD_MATRIX_2]),
        (&c.camera_calibration, [t::CAMERA_CALIBRATION_1, t::CAMERA_CALIBRATION_2]),
    ] {
        for i in 0..2 {
            if let Some(m) = &pair[i] {
                ifd.set(tags[i], srat_mat(m));
            }
        }
    }
    if let Some(v) = c.analog_balance {
        ifd.set(t::ANALOG_BALANCE, rat_vec(&v));
    }
    if let Some(v) = c.as_shot_neutral {
        ifd.set(t::AS_SHOT_NEUTRAL, rat_vec(&v));
    } else if let Some(xy) = c.as_shot_white_xy {
        ifd.set(t::AS_SHOT_WHITE_XY, rat_vec(&[xy.x, xy.y]));
    } else if let Some(m) = raw.wb_multipliers.filter(|m| m.iter().all(|v| *v > 0.0 && v.is_finite())) {
        // vendor raws (NEF, ARW…) carry white balance as multipliers: the DNG neutral is their reciprocal
        ifd.set(t::AS_SHOT_NEUTRAL, rat_vec(&m.map(|v| 1.0 / v as f64)));
    }
    ifd.set(t::BASELINE_EXPOSURE, Value::SRational(vec![srational(c.baseline_exposure)]));
    // the source's own profile look travels with its data (conversions, smart previews, merges)
    let p = &c.profile;
    let table_tags = |ifd: &mut IfdBuilder, table: &crate::profile::HsvTable, dims: u16, enc: u16| {
        ifd.set(dims, Value::Long(vec![table.hue_divisions as u32, table.sat_divisions as u32, table.val_divisions as u32]));
        if table.srgb_value {
            ifd.set(enc, Value::Long(vec![1]));
        }
    };
    let flat = |table: &crate::profile::HsvTable| Value::Float(table.data.iter().flatten().copied().collect());
    match &p.hue_sat_map {
        [Some(a), b] => {
            table_tags(&mut ifd, a, t::PROFILE_HUE_SAT_MAP_DIMS, t::PROFILE_HUE_SAT_MAP_ENCODING);
            ifd.set(t::PROFILE_HUE_SAT_MAP_DATA_1, flat(a));
            if let Some(b) =
                b.as_ref().filter(|b| (b.hue_divisions, b.sat_divisions, b.val_divisions) == (a.hue_divisions, a.sat_divisions, a.val_divisions))
            {
                ifd.set(t::PROFILE_HUE_SAT_MAP_DATA_2, flat(b));
            }
        }
        [None, Some(b)] => {
            table_tags(&mut ifd, b, t::PROFILE_HUE_SAT_MAP_DIMS, t::PROFILE_HUE_SAT_MAP_ENCODING);
            ifd.set(t::PROFILE_HUE_SAT_MAP_DATA_1, flat(b));
        }
        [None, None] => {}
    }
    if let Some(third) = &p.hue_sat_map_3
        && let Some(first) = &p.hue_sat_map[0]
        && (third.hue_divisions, third.sat_divisions, third.val_divisions, third.srgb_value)
            == (first.hue_divisions, first.sat_divisions, first.val_divisions, first.srgb_value)
    {
        ifd.set(52537, flat(third));
    }
    if let Some(l) = &p.look_table {
        table_tags(&mut ifd, l, t::PROFILE_LOOK_TABLE_DIMS, t::PROFILE_LOOK_TABLE_ENCODING);
        ifd.set(t::PROFILE_LOOK_TABLE_DATA, flat(l));
    }
    if let Some(curve) = &p.tone_curve {
        ifd.set(t::PROFILE_TONE_CURVE, Value::Float(curve.points.iter().flatten().copied().collect()));
    }
    for (list, tag) in [(&raw.opcodes.list1, t::OPCODE_LIST_1), (&raw.opcodes.list2, t::OPCODE_LIST_2), (&raw.opcodes.list3, t::OPCODE_LIST_3)] {
        if !list.is_empty() {
            ifd.set(tag, Value::Undefined(opcodes::write_list(list)));
        }
    }

    match (&raw.data, opts.compression) {
        (RawData::F32(v), DngCompression::Deflate { tile, half }) => {
            let tile = (tile.max(16) & !15) as usize;
            let bp = if half { 2 } else { 4 };
            ifd.set(t::BITS_PER_SAMPLE, Value::Short(vec![(bp * 8) as u16; cpp]));
            ifd.set(t::SAMPLE_FORMAT, Value::Short(vec![3; cpp]));
            ifd.set(t::COMPRESSION, Value::Short(vec![compression::ADOBE_DEFLATE]));
            ifd.set(t::PREDICTOR, Value::Short(vec![3]));
            let (ta, td) = (w.div_ceil(tile), h.div_ceil(tile));
            let tiles: Vec<Vec<u8>> = (0..ta * td)
                .into_par_iter()
                .map(|i| {
                    let (tx, ty) = (i % ta * tile, i / ta * tile);
                    let mut buf = Vec::with_capacity(tile * tile * cpp * bp);
                    let mut row = Vec::with_capacity(tile * cpp * bp);
                    for y in 0..tile {
                        let sy = (ty + y).min(h - 1);
                        row.clear();
                        for x in 0..tile {
                            let sx = (tx + x).min(w - 1);
                            for s in 0..cpp {
                                let f = v[(sy * w + sx) * cpp + s];
                                if half {
                                    row.extend_from_slice(&f32_to_f16(f).to_be_bytes());
                                } else {
                                    row.extend_from_slice(&f.to_bits().to_be_bytes());
                                }
                            }
                        }
                        buf.extend_from_slice(&float_predict(&row, tile * cpp, bp, cpp));
                    }
                    miniz_oxide::deflate::compress_to_vec_zlib(&buf, 6)
                })
                .collect();
            ifd.set_image(ImageData::Tiles { tile_width: tile as u32, tile_height: tile as u32, tiles });
        }
        (RawData::F32(v), _) => {
            ifd.set(t::BITS_PER_SAMPLE, Value::Short(vec![32; cpp]));
            ifd.set(t::SAMPLE_FORMAT, Value::Short(vec![3; cpp]));
            ifd.set(t::COMPRESSION, Value::Short(vec![compression::NONE]));
            let mut bytes = Vec::with_capacity(v.len() * 4);
            for &x in v {
                let mut tmp = Vec::with_capacity(4);
                opts.order.put_u32(&mut tmp, x.to_bits());
                bytes.extend_from_slice(&tmp);
            }
            ifd.set_image(ImageData::Strips { rows_per_strip: h as u32, strips: vec![bytes] });
        }
        (RawData::U16(v), _) => {
            let maxv = v.iter().copied().max().unwrap_or(0);
            let bits = (16 - maxv.leading_zeros()).max(raw.bits.min(16)).max(2) as u8;
            ifd.set(
                t::BITS_PER_SAMPLE,
                Value::Short(vec![if matches!(opts.compression, DngCompression::Uncompressed) { 16 } else { bits as u16 }; cpp]),
            );
            match opts.compression {
                DngCompression::Uncompressed => {
                    ifd.set(t::COMPRESSION, Value::Short(vec![compression::NONE]));
                    let rows = (65536 / (w * cpp * 2)).clamp(1, h);
                    let strips = v
                        .chunks(rows * w * cpp)
                        .map(|s| {
                            let mut b = Vec::with_capacity(s.len() * 2);
                            s.iter().for_each(|&x| opts.order.put_u16(&mut b, x));
                            b
                        })
                        .collect();
                    ifd.set_image(ImageData::Strips { rows_per_strip: rows as u32, strips });
                }
                DngCompression::Deflate { tile, .. } => {
                    let tile = (tile.max(16) & !15) as usize;
                    ifd.set(t::BITS_PER_SAMPLE, Value::Short(vec![16; cpp]));
                    ifd.set(t::COMPRESSION, Value::Short(vec![compression::ADOBE_DEFLATE]));
                    ifd.set(t::PREDICTOR, Value::Short(vec![2]));
                    let (ta, td) = (w.div_ceil(tile), h.div_ceil(tile));
                    let tiles: Vec<Vec<u8>> = (0..ta * td)
                        .into_par_iter()
                        .map(|i| {
                            let (tx, ty) = (i % ta * tile, i / ta * tile);
                            let mut buf = Vec::with_capacity(tile * tile * cpp * 2);
                            let mut row = vec![0u16; tile * cpp];
                            for y in 0..tile {
                                let sy = (ty + y).min(h - 1);
                                for x in 0..tile {
                                    let sx = (tx + x).min(w - 1);
                                    for s in 0..cpp {
                                        row[x * cpp + s] = v[(sy * w + sx) * cpp + s];
                                    }
                                }
                                for i in (cpp..row.len()).rev() {
                                    row[i] = row[i].wrapping_sub(row[i - cpp]);
                                }
                                row.iter().for_each(|&x| opts.order.put_u16(&mut buf, x));
                            }
                            miniz_oxide::deflate::compress_to_vec_zlib(&buf, 6)
                        })
                        .collect();
                    ifd.set_image(ImageData::Tiles { tile_width: tile as u32, tile_height: tile as u32, tiles });
                }
                DngCompression::Lj92 { tile } => {
                    let tile = (tile.max(2) & !1) as usize;
                    if tile > 65534 {
                        return Err(RawError::Limit("tile too large"));
                    }
                    ifd.set(t::COMPRESSION, Value::Short(vec![compression::JPEG]));
                    let (ta, td) = (w.div_ceil(tile), h.div_ceil(tile));
                    let tiles: Vec<Vec<u8>> = (0..ta * td)
                        .into_par_iter()
                        .map(|i| {
                            let (tx, ty) = (i % ta * tile, i / ta * tile);
                            let mut buf = Vec::with_capacity(tile * tile * cpp);
                            for y in 0..tile {
                                let sy = (ty + y).min(h - 1);
                                for x in 0..tile {
                                    let sx = (tx + x).min(w - 1);
                                    for s in 0..cpp {
                                        buf.push(v[(sy * w + sx) * cpp + s]);
                                    }
                                }
                            }
                            if cpp == 1 {
                                ljpeg::encode(&buf, tile / 2, tile, 2, bits, 1, 0)
                            } else {
                                ljpeg::encode(&buf, tile, tile, cpp, bits, 1, 0)
                            }
                        })
                        .collect();
                    ifd.set_image(ImageData::Tiles { tile_width: tile as u32, tile_height: tile as u32, tiles });
                }
            }
        }
    }
    Ok(TiffWriter::new(opts.order, false).write(&[ifd])?)
}

#[cfg(test)]
mod sony_dji_roundtrip_tests {
    use super::*;
    #[test]
    fn sony_dji_linear_dng_retains_vendor_curves_and_drone_xmp() {
        let lens = lightcraft_meta::SonyLensCorrection {
            distortion: vec![-100, 0, 417],
            ca_red: vec![128, 256, 640],
            ca_blue: vec![512, 0, -256],
            vignetting: vec![0, 32, 6144],
        };
        let mut metadata = lightcraft_meta::Metadata {
            make: Some("SONY".into()),
            model: Some("ILCE-7RM3".into()),
            sony: Some(lightcraft_meta::SonyMetadata { lens_correction: Some(lens.clone()), ..Default::default() }),
            dji: Some(lightcraft_meta::DjiMetadata { relative_altitude: Some(12.5), gimbal_yaw: Some(-44.0), ..Default::default() }),
            ..Default::default()
        };
        // Both kinds of metadata use the same writer; this synthetic metadata is
        // intentionally camera-independent and contains no owner image values.
        metadata.dji.as_mut().unwrap().properties.insert("CalibrationTag".into(), "1.2340".into());
        let raw = RawImage {
            vendor_lens: None,
            format: crate::RawFormat::Dng,
            width: 4,
            height: 3,
            cpp: 3,
            data: RawData::F32(vec![0.25; 36]),
            cfa: None,
            bits: 32,
            black: crate::BlackLevel::uniform(0.0),
            white: vec![1.0],
            active_area: crate::Rect::new(0, 0, 4, 3),
            crop: crate::Rect::new(0, 0, 4, 3),
            orientation: crate::Orientation::Normal,
            color: Default::default(),
            wb_multipliers: None,
            linearized: false,
            opcodes: Default::default(),
            metadata,
        };
        let bytes =
            write_dng(&raw, &DngWriteOptions { compression: DngCompression::Deflate { tile: 16, half: false }, ..Default::default() }).unwrap();
        let back = crate::decode(&bytes).unwrap();
        assert_eq!(back.data, raw.data);
        assert_eq!(back.metadata.sony.unwrap().lens_correction, Some(lens));
        let dji = back.metadata.dji.unwrap();
        assert_eq!(dji.relative_altitude, Some(12.5));
        assert_eq!(dji.gimbal_yaw, Some(-44.0));
        assert_eq!(dji.properties.get("CalibrationTag").map(String::as_str), Some("1.2340"));
    }
}

#[cfg(test)]
mod triple_illuminant_tests {
    use super::*;
    use crate::{ColorData, RawFormat, Rect};
    use lightcraft_color::{D50, D65, Xy};

    fn triple_raw() -> RawImage {
        let table = |shift| crate::profile::HsvTable::from_tags(&[1, 2, 1], &[shift, 1.0, 1.0, shift, 1.0, 1.0], 0).unwrap();
        RawImage {
            vendor_lens: None,
            format: RawFormat::Dng,
            width: 2,
            height: 2,
            cpp: 3,
            data: RawData::U16(vec![1000; 12]),
            cfa: None,
            bits: 16,
            black: Default::default(),
            white: vec![65535.0; 3],
            active_area: Rect::new(0, 0, 2, 2),
            crop: Rect::new(0, 0, 2, 2),
            orientation: Default::default(),
            color: ColorData {
                illuminant: [17, 21],
                illuminant_3: 255,
                illuminant_white_xy: [None, None, Some(Xy::new(0.3457, 0.39))],
                color_matrix: [Some(Mat3::diag(1.0, 2.0, 3.0)), Some(Mat3::diag(2.0, 3.0, 4.0))],
                color_matrix_3: Some(Mat3::diag(4.0, 5.0, 6.0)),
                forward_matrix: [Some(Mat3::diag(D50.to_xyz()[0], 1.0, D50.to_xyz()[2])); 2],
                forward_matrix_3: Some(Mat3::diag(D50.to_xyz()[0], 1.0, D50.to_xyz()[2])),
                camera_calibration_3: Some(Mat3::diag(1.1, 0.9, 1.0)),
                camera_calibration_signature: "calibration-body".into(),
                profile_calibration_signature: "calibration-body".into(),
                as_shot_white_xy: Some(D65),
                profile: crate::profile::ProfileLook {
                    hue_sat_map: [Some(table(10.0)), Some(table(-10.0))],
                    hue_sat_map_3: Some(table(30.0)),
                    ..Default::default()
                },
                ..Default::default()
            },
            wb_multipliers: None,
            linearized: false,
            opcodes: Default::default(),
            metadata: Default::default(),
        }
    }

    #[test]
    fn triple_illuminant_dng_export_and_dcp_roundtrip() {
        for order in [ByteOrder::Little, ByteOrder::Big] {
            let raw = triple_raw();
            let mut bytes = write_dng(&raw, &DngWriteOptions { compression: DngCompression::Uncompressed, order, xmp: None }).unwrap();
            let parsed = lightcraft_tiff::Tiff::parse(&bytes).unwrap();
            assert_eq!(parsed.ifds[0].bytes(t::DNG_VERSION), Some(&[1, 6, 0, 0][..]));
            assert_eq!(parsed.ifds[0].bytes(t::DNG_BACKWARD_VERSION), Some(&[1, 3, 0, 0][..]));
            let decoded = crate::decode(&bytes).unwrap();
            assert_eq!(decoded.color.illuminant_3, 255);
            assert_eq!(decoded.color.illuminant_white_xy, raw.color.illuminant_white_xy);
            assert_eq!(decoded.color.color_matrix_3, raw.color.color_matrix_3);
            assert_eq!(decoded.color.camera_calibration_3, raw.color.camera_calibration_3);
            assert_eq!(decoded.color.profile, raw.color.profile);
            assert_eq!(decoded.color.camera_calibration_signature, "calibration-body");
            assert_eq!(decoded.color.profile_calibration_signature, "calibration-body");
            assert_eq!(crate::color::camera_color(&decoded.color).camera_calibration_3, decoded.color.camera_calibration_3);
            let mut mismatched = decoded.color.clone();
            mismatched.profile_calibration_signature = "different-profile-space".into();
            let calibration = crate::color::camera_color(&mismatched);
            assert_eq!(calibration.camera_calibration, [None; 2]);
            assert_eq!(calibration.camera_calibration_3, None);
            let xy = Xy::new(0.38, 0.36);
            let expected = crate::color::camera_to_xyz_d50(&raw.color, xy);
            let actual = crate::color::camera_to_xyz_d50(&decoded.color, xy);
            for (a, b) in actual.0.iter().flatten().zip(expected.0.iter().flatten()) {
                assert!((a - b).abs() < 1e-6);
            }
            bytes[2..4].copy_from_slice(if order == ByteOrder::Little { b"RC" } else { b"CR" });
            let dcp = crate::dcp::Dcp::parse(&bytes).unwrap();
            assert_eq!(dcp.color.color_matrix_3, raw.color.color_matrix_3);
            let mut malformed = triple_raw();
            malformed.color.forward_matrix_3 = None;
            let mut bytes = write_dng(&malformed, &DngWriteOptions { compression: DngCompression::Uncompressed, order, xmp: None }).unwrap();
            bytes[2..4].copy_from_slice(if order == ByteOrder::Little { b"RC" } else { b"CR" });
            assert!(crate::dcp::Dcp::parse(&bytes).is_err());
        }
    }
}
