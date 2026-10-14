//! Sony maker notes: ExifTool 13.40 layouts/cipher, plus 13.59 A7 V/A7R VI field shifts.
//! Unknown bodies/versions are not interpreted with another body's offsets.
use lightcraft_tiff::{ByteOrder, Ifd, Tiff, tags};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SonyPixelShift {
    pub group_id: u32,
    /// Zero denotes an already-composed ARQ, otherwise 1..=shot_count.
    pub shot_number: u8,
    pub shot_count: u8,
}

/// Sony raw-SubIFD radial curves, with the count prefixes validated and removed.
/// Distortion uses 2^-14 units, CA 2^-21; vignetting remains in vendor encoding.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SonyLensCorrection {
    pub distortion: Vec<i16>,
    pub ca_red: Vec<i16>,
    pub ca_blue: Vec<i16>,
    pub vignetting: Vec<i16>,
}

/// Only the raw-SubIFD curves are lens-correction inputs; maker-note settings are not.
pub(crate) fn lens_correction(tiff: &Tiff) -> Option<SonyLensCorrection> {
    for ifd in tiff.all_ifds() {
        let values = |tag| -> Option<Vec<i16>> {
            let v = ifd.value(tag)?;
            if v.count() > 33 {
                return None;
            }
            (0..v.count()).map(|i| i16::try_from(v.get_i64(i)?).ok()).collect()
        };
        let (Some(d), Some(c), Some(v)) = (values(0x7037), values(0x7035), values(0x7032)) else {
            continue;
        };
        let Some(n) = d.first().and_then(|n| usize::try_from(*n).ok()).filter(|n| (2..=16).contains(n)) else {
            continue;
        };
        if d.len() != n + 1 || c.len() != 2 * n + 1 || v.len() != n + 1 || c[0] != (2 * n) as i16 || v[0] != n as i16 {
            continue;
        }
        return Some(SonyLensCorrection {
            distortion: d[1..].to_vec(),
            ca_red: c[1..n + 1].to_vec(),
            ca_blue: c[n + 1..].to_vec(),
            vignetting: v[1..].to_vec(),
        });
    }
    None
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SonyMetadata {
    /// Documented numeric LensType/LensType2 ID; no model name is invented for an ID.
    pub lens_id: Option<u32>,
    pub shutter_count: Option<u32>,
    /// Internal maker-note identifier, which is not necessarily the printed body serial.
    pub internal_serial_number: Option<String>,
    pub creative_style: Option<String>,
    /// The recorded gamma/profile label; PP slots can be customized, so no slot is inferred.
    pub picture_profile: Option<String>,
    pub focus_mode: Option<String>,
    pub focus_area: Option<String>,
    /// Sony's 640 × 480 AF coordinate space (not full-resolution pixels).
    pub flexible_spot: Option<[u16; 2]>,
    pub sequence_number: Option<u16>,
    pub pixel_shift: Option<SonyPixelShift>,
    pub lens_correction: Option<SonyLensCorrection>,
}

/// Read maker notes with their original absolute TIFF offset base.
/// `from_tiff` alone cannot resolve these offsets without the original file bytes.
pub(crate) fn read(bytes: &[u8], tiff: &Tiff, metadata: &mut crate::Metadata) {
    if !metadata.make.as_deref().is_some_and(|s| s.to_ascii_uppercase().contains("SONY")) {
        return;
    }
    let Some(entry) = tiff.exif().and_then(|e| e.get(tags::MAKER_NOTE)) else {
        return;
    };
    let Some(note) = lightcraft_tiff::makernote::parse_makernote(bytes, entry.offset, entry.count() as u64, tiff.order, "SONY") else {
        return;
    };
    let mut sony = from_ifd(&note.ifd, note.order, metadata.model.as_deref().unwrap_or_default());
    sony.lens_correction = lens_correction(tiff);
    if sony != SonyMetadata::default() {
        metadata.sony = Some(sony);
    }
}

fn text(ifd: &Ifd, tag: u16) -> Option<String> {
    ifd.string(tag).map(|v| v.trim_matches(['\0', ' ']).to_string()).filter(|v| !v.is_empty())
}

fn decipher(bytes: &[u8]) -> Vec<u8> {
    let mut inverse = [0u8; 256];
    for b in 0..256u32 {
        inverse[if b < 249 { (b * b * b % 249) as usize } else { b as usize }] = b as u8;
    }
    bytes.iter().map(|b| inverse[*b as usize]).collect()
}

fn profile(code: u8) -> String {
    match code {
        0 => "Gamma Still - Standard/Neutral",
        1 => "Gamma Still - Portrait",
        3 => "Gamma Still - Night View/Portrait",
        4 => "Gamma Still - B&W/Sepia",
        5 => "Gamma Still - Clear",
        6 => "Gamma Still - Deep",
        7 => "Gamma Still - Light",
        8 => "Gamma Still - Vivid",
        9 => "Gamma Still - Real",
        10 => "Gamma Movie",
        22 => "Gamma ITU709",
        24 => "Gamma Cine1",
        25 => "Gamma Cine2",
        26 => "Gamma Cine3",
        27 => "Gamma Cine4",
        28 => "Gamma S-Log2",
        29 => "Gamma ITU709 (800%)",
        31 => "Gamma S-Log3",
        33 => "Gamma HLG2",
        34 => "Gamma HLG3",
        36 => "Off",
        37 => "FL",
        38 => "VV2",
        39 => "IN",
        40 => "SH",
        48 => "FL2",
        49 => "FL3",
        _ => return format!("Unknown ({code})"),
    }
    .into()
}

fn from_ifd(ifd: &Ifd, order: ByteOrder, model: &str) -> SonyMetadata {
    let mut sony = SonyMetadata {
        lens_id: ifd.u32(0xb027).filter(|v| !matches!(*v, 65535 | u32::MAX)),
        creative_style: text(ifd, 0xb020).map(|s| match s.as_str() {
            "AdobeRGB" => "Adobe RGB".into(),
            "BW" => "B&W".into(),
            "Nightview" => "Night View/Portrait".into(),
            "Autumnleaves" => "Autumn Leaves".into(),
            _ => s,
        }),
        sequence_number: ifd.u16(0xb04a).filter(|v| *v != u16::MAX),
        ..Default::default()
    };
    if let Some(b) = ifd.bytes(0x202f).filter(|v| v.len() == 6)
        && matches!(b[5], 4 | 16)
        && b[4] <= b[5]
        && let Some(group_id) = order.read_u32(b, 0)
    {
        sony.pixel_shift = Some(SonyPixelShift { group_id, shot_number: b[4], shot_count: b[5] });
    }
    let modern_focus = !model.starts_with("DSC-")
        || ["DSC-RX10M4", "DSC-RX100M6", "DSC-RX100M7", "DSC-RX100M5A", "DSC-HX95", "DSC-HX99", "DSC-RX0M2", "DSC-RX1RM3"].contains(&model);
    if modern_focus {
        sony.focus_mode = ifd.u16(0x201b).map(|v| match v {
            0 => "Manual".into(),
            2 => "AF-S".into(),
            3 => "AF-C".into(),
            4 => "AF-A".into(),
            6 => "DMF".into(),
            7 => "AF-D".into(),
            _ => format!("Unknown ({v})"),
        });
        // The SLT/ILCA area-mode layouts differ; do not interpret as E-mount.
        if model.starts_with("ILCE-")
            || model.starts_with("NEX-")
            || model.starts_with("ILME-")
            || model.starts_with("ZV-")
            || model.starts_with("DSC-")
        {
            sony.focus_area = ifd.u16(0x201c).map(|v| match v {
                0 => "Wide".into(),
                1 => "Center".into(),
                3 => "Flexible Spot".into(),
                4 => "Flexible Spot (LA-EA4)".into(),
                9 => "Center (LA-EA4)".into(),
                11 => "Zone".into(),
                12 => "Expanded Flexible Spot".into(),
                13 => "Custom AF Area".into(),
                _ => format!("Unknown ({v})"),
            });
            if sony.focus_area.as_deref() == Some("Flexible Spot")
                && let Some(v) = ifd.u64s(0x201d).filter(|v| v.len() == 2 && v[0] <= 640 && v[1] <= 480)
            {
                sony.flexible_spot = Some([v[0] as u16, v[1] as u16]);
            }
        }
    } else if ["DSC-RX100", "DSC-RX1", "DSC-RX1R"].contains(&model)
        && let Some(v) = ifd.u16(0xb04e)
    {
        sony.focus_mode = match v {
            0 => Some("Manual".into()),
            2 => Some("AF-S".into()),
            3 => Some("AF-C".into()),
            5 => Some("Semi-manual".into()),
            6 => Some("DMF".into()),
            _ => None,
        };
    }
    let modern = ["ILCE-1", "ILCE-7M4", "ILCE-7RM5", "ILCE-7SM3", "ILME-FX3"].contains(&model);
    let middle = [
        "ILCE-6100",
        "ILCE-6100A",
        "ILCE-6300",
        "ILCE-6400",
        "ILCE-6400A",
        "ILCE-6500",
        "ILCE-6600",
        "ILCE-7C",
        "ILCE-7M3",
        "ILCE-7RM2",
        "ILCE-7RM3",
        "ILCE-7RM3A",
        "ILCE-7RM4",
        "ILCE-7RM4A",
        "ILCE-7SM2",
        "ILCE-9",
        "ILCE-9M2",
        "ILCA-99M2",
        "ZV-E10",
    ]
    .contains(&model);
    let recent = ["ILCE-6700", "ILCE-7CM2", "ILCE-7CR", "ILME-FX2", "ZV-E1", "ZV-E10M2"].contains(&model)
        || (["ILCE-1M2", "ILCE-7M5", "ILCE-7RM6"].contains(&model) && ifd.bytes(0x9050).is_some_and(|b| b.starts_with(&[0; 5])));
    let legacy = [
        "ILCE-7",
        "ILCE-7R",
        "ILCE-7S",
        "ILCE-7M2",
        "ILCE-5000",
        "ILCE-5100",
        "ILCE-6000",
        "NEX-3N",
        "NEX-5N",
        "NEX-5R",
        "NEX-5T",
        "NEX-6",
        "NEX-7",
        "SLT-A37",
        "SLT-A57",
        "SLT-A65",
        "SLT-A65V",
        "SLT-A77",
        "SLT-A77V",
        "SLT-A99",
        "SLT-A99V",
    ]
    .contains(&model);
    if let Some(b) = ifd.bytes(0x9050).filter(|_| modern || middle || recent || legacy) {
        let b = decipher(b);
        if (middle || legacy) && b.get(0x105) == Some(&2) {
            sony.lens_id = ByteOrder::Little.read_u16(&b, 0x107).map(u32::from).or(sony.lens_id);
        }
        let count_at = if recent {
            10
        } else if legacy {
            50
        } else {
            58
        };
        if !["ZV-E1", "ZV-E10M2"].contains(&model) {
            sony.shutter_count = ByteOrder::Little.read_u32(&b, count_at).map(|v| v & 0x00ff_ffff);
        }
        let serial_layout = if recent && model != "ZV-E10M2" {
            Some((56, 6))
        } else if model == "ILCE-1" {
            Some((138, 6))
        } else if modern || middle {
            Some((136, 6))
        } else if legacy && model.starts_with("SLT-") {
            Some((240, 5))
        } else if legacy && !["NEX-5N", "NEX-7"].contains(&model) {
            Some((124, 4))
        } else {
            None
        };
        if let Some((at, len)) = serial_layout {
            sony.internal_serial_number = b
                .get(at..at + len)
                .filter(|b| b.iter().any(|v| *v != 0) && !b.iter().all(|v| *v == 255))
                .map(|b| b.iter().map(|v| format!("{v:02x}")).collect());
        }
    }
    if modern || recent || ["ILCE-1M2", "ILCE-7M5", "ILCE-7RM6", "ILCE-9M3", "ILME-FX30"].contains(&model) {
        if let Some(b) = ifd.bytes(0x9416) {
            let b = decipher(b);
            // 13.59: +4 after version, -2 after release mode, +1 after creative style.
            let delta = if ["ILCE-7M5", "ILCE-7RM6"].contains(&model) { 3 } else { usize::from(model == "ILME-FX2") };
            sony.picture_profile = b.get(0x70 + delta).copied().map(profile);
            if b.get(0x4a + delta) == Some(&2) {
                sony.lens_id = ByteOrder::Little.read_u16(&b, 0x4b + delta).map(u32::from).or(sony.lens_id);
            }
        }
    } else if let Some(b) = ifd.bytes(0x2010) {
        let layout = if middle && !["ILCE-6300", "ILCE-6500", "ILCE-7RM2", "ILCE-7SM2", "ILCA-99M2"].contains(&model) {
            Some(0x237)
        } else if [
            "ILCE-6300",
            "ILCE-6500",
            "ILCE-7RM2",
            "ILCE-7SM2",
            "ILCA-99M2",
            "ILCE-7",
            "ILCE-7R",
            "ILCE-7S",
            "ILCE-7M2",
            "ILCE-5000",
            "ILCE-5100",
            "ILCE-6000",
            "ILCE-QX1",
            "ILCA-68",
            "ILCA-77M2",
            "DSC-RX10",
            "DSC-RX100M3",
            "DSC-RX0",
            "DSC-RX1RM2",
            "DSC-RX10M2",
            "DSC-RX10M3",
            "DSC-RX100M4",
            "DSC-RX100M5",
            "DSC-HX80",
            "DSC-HX90V",
            "DSC-WX500",
        ]
        .contains(&model)
        {
            Some(0x246)
        } else if ["DSC-RX1", "DSC-RX1R", "DSC-RX100", "SLT-A99", "SLT-A99V", "SLT-A58", "NEX-3N", "NEX-5R", "NEX-5T", "NEX-6"].contains(&model) {
            Some(0x1196)
        } else {
            None
        };
        if let Some(profile_at) = layout {
            let b = decipher(b);
            sony.picture_profile = b.get(profile_at).copied().map(profile);
        }
    }
    sony
}

#[cfg(test)]
mod tests {
    use super::*;
    use lightcraft_tiff::{IfdBuilder, TiffWriter, Value};

    fn encrypt(bytes: &[u8]) -> Vec<u8> {
        bytes.iter().map(|&b| if b < 249 { (u32::from(b).pow(3) % 249) as u8 } else { b }).collect()
    }

    fn file(order: ByteOrder, model: &str, fields: Vec<(u16, Value)>) -> Vec<u8> {
        let mut fields = fields;
        fields.sort_by_key(|(tag, _)| *tag);
        let mut note = b"SONY DSC \0\0\0".to_vec();
        order.put_u16(&mut note, fields.len() as u16);
        note.resize(12 + 2 + fields.len() * 12 + 4, 0);
        for (i, (tag, value)) in fields.iter().enumerate() {
            let at = 14 + i * 12;
            let mut entry = Vec::new();
            order.put_u16(&mut entry, *tag);
            order.put_u16(&mut entry, value.field_type() as u16);
            order.put_u32(&mut entry, value.count() as u32);
            let payload = lightcraft_tiff::writer::encode(order, value);
            if payload.len() > 4 {
                order.put_u32(&mut entry, note.len() as u32);
                note.extend_from_slice(&payload);
            } else {
                entry.extend_from_slice(&payload);
                entry.resize(12, 0);
            }
            note[at..at + 12].copy_from_slice(&entry);
        }
        let mut exif = IfdBuilder::new();
        exif.set(tags::MAKER_NOTE, Value::Undefined(note));
        exif.set(tags::LENS_MODEL, Value::Ascii("Sony FE 24-70mm F2.8 GM II".into()));
        let mut root = IfdBuilder::new();
        root.set(tags::MAKE, Value::Ascii("SONY".into()));
        root.set(tags::MODEL, Value::Ascii(model.into()));
        root.set_child(tags::EXIF_IFD, exif);
        let mut bytes = TiffWriter::new(order, false).write(&[root]).unwrap();
        let offset = Tiff::parse(&bytes).unwrap().exif().unwrap().get(tags::MAKER_NOTE).unwrap().offset as usize;
        let count = order.read_u16(&bytes, offset + 12).unwrap();
        for i in 0..usize::from(count) {
            let at = offset + 14 + i * 12;
            let ty = order.read_u16(&bytes, at + 2).unwrap();
            let n = order.read_u32(&bytes, at + 4).unwrap() as usize;
            let size = match ty {
                3 => 2,
                4 => 4,
                _ => 1,
            };
            if n * size > 4 {
                let absolute = order.read_u32(&bytes, at + 8).unwrap() + offset as u32;
                let mut v = Vec::new();
                order.put_u32(&mut v, absolute);
                bytes[at + 8..at + 12].copy_from_slice(&v);
            }
        }
        bytes
    }

    #[test]
    fn sony_metadata_real_offsets_cipher_and_pixel_shift_both_orders() {
        for order in [ByteOrder::Little, ByteOrder::Big] {
            let mut count = vec![0; 144];
            count[58..62].copy_from_slice(&0xab00_1234u32.to_le_bytes());
            count[138..144].copy_from_slice(&[0, 1, 2, 3, 4, 5]);
            let mut profile = vec![0; 0x954];
            profile[0x70] = 31;
            profile[0x4f..0x51].copy_from_slice(&(-123i16).to_le_bytes());
            profile[0x88f..0x891].copy_from_slice(&16i16.to_le_bytes());
            profile[0x914..0x916].copy_from_slice(&32i16.to_le_bytes());
            let mut shift = Vec::new();
            order.put_u32(&mut shift, 0xdeadbeef);
            shift.extend([3, 16]);
            let bytes = file(
                order,
                "ILCE-1",
                vec![
                    (0x9050, Value::Undefined(encrypt(&count))),
                    (0x9416, Value::Undefined(encrypt(&profile))),
                    (0xb020, Value::Ascii("Standard".into())),
                    (0xb027, Value::Long(vec![77])),
                    (0x201b, Value::Byte(vec![3])),
                    (0x201c, Value::Byte(vec![3])),
                    (0x201d, Value::Short(vec![320, 240])),
                    (0xb04a, Value::Short(vec![7])),
                    (0x202f, Value::Undefined(shift)),
                ],
            );
            let m = crate::extract(&bytes);
            assert_eq!(m.lens_model.as_deref(), Some("Sony FE 24-70mm F2.8 GM II"));
            let sony = m.sony.unwrap();
            assert_eq!(sony.shutter_count, Some(0x1234));
            assert_eq!(sony.lens_id, Some(77), "numeric IDs are preserved independently of the standard EXIF lens model");
            assert_eq!(sony.internal_serial_number.as_deref(), Some("000102030405"));
            assert_eq!(sony.creative_style.as_deref(), Some("Standard"));
            assert_eq!(sony.picture_profile.as_deref(), Some("Gamma S-Log3"));
            assert_eq!(sony.focus_mode.as_deref(), Some("AF-C"));
            assert_eq!(sony.flexible_spot, Some([320, 240]));
            assert_eq!(sony.sequence_number, Some(7));
            assert_eq!(sony.pixel_shift, Some(SonyPixelShift { group_id: 0xdeadbeef, shot_number: 3, shot_count: 16 }));
            assert!(sony.lens_correction.is_none(), "maker-note correction settings are not raw SubIFD curves");
        }
    }

    #[test]
    fn sony_metadata_unknown_bodies_and_regular_bursts_are_not_pixel_shift() {
        let bytes = file(
            ByteOrder::Little,
            "ILCE-UNKNOWN",
            vec![
                (0x9050, Value::Undefined(vec![1; 160])),
                (0x9416, Value::Undefined(vec![1; 2500])),
                (0xb04a, Value::Short(vec![4])),
                (0x202f, Value::Undefined(vec![0; 6])),
            ],
        );
        let m = crate::extract(&bytes).sony.unwrap();
        assert_eq!(m.sequence_number, Some(4));
        assert_eq!(m.shutter_count, None);
        assert_eq!(m.picture_profile, None);
        assert_eq!(m.pixel_shift, None);
        for len in 0..60 {
            let bytes = file(ByteOrder::Little, "ILCE-1", vec![(0x9050, Value::Undefined(vec![0; len])), (0x202f, Value::Undefined(vec![0; len]))]);
            let m = crate::extract(&bytes);
            assert!(m.sony.as_ref().is_none_or(|m| m.shutter_count.is_none() && m.pixel_shift.is_none()));
        }
    }
}

#[cfg(test)]
mod lens_tests {
    use super::*;
    use lightcraft_tiff::{IfdBuilder, TiffWriter, Value};
    #[test]
    fn sony_subifd_lens_curves_validate_counts_signed_values_and_handoff() {
        let mut raw = IfdBuilder::new();
        raw.set(0x7037, Value::SShort(vec![3, -100, 20, 30]));
        raw.set(0x7035, Value::SShort(vec![6, -2, 3, 4, 5, -6, 7]));
        raw.set(0x7032, Value::SShort(vec![3, 100, 200, 300]));
        let mut maker = b"SONY DSC \0\0\0".to_vec();
        ByteOrder::Little.put_u16(&mut maker, 1);
        ByteOrder::Little.put_u16(&mut maker, 0x201b);
        ByteOrder::Little.put_u16(&mut maker, 1);
        ByteOrder::Little.put_u32(&mut maker, 1);
        ByteOrder::Little.put_u32(&mut maker, 3);
        ByteOrder::Little.put_u32(&mut maker, 0);
        let mut exif = IfdBuilder::new();
        exif.set(tags::MAKER_NOTE, Value::Undefined(maker));
        let mut root = IfdBuilder::new();
        root.set(tags::MAKE, Value::Ascii("SONY".into()));
        root.set(tags::MODEL, Value::Ascii("ILCE-1".into()));
        root.set_child(tags::EXIF_IFD, exif);
        root.add_sub_ifd(raw);
        let bytes = TiffWriter::default().write(&[root]).unwrap();
        let m = crate::read_exif(&bytes);
        assert_eq!(m.sony.as_ref().unwrap().focus_mode.as_deref(), Some("AF-C"));
        let lens = m.sony.unwrap().lens_correction.unwrap();
        assert_eq!(
            lens,
            SonyLensCorrection { distortion: vec![-100, 20, 30], ca_red: vec![-2, 3, 4], ca_blue: vec![5, -6, 7], vignetting: vec![100, 200, 300] }
        );
        let mut tiff = Tiff::parse(&bytes).unwrap();
        assert_eq!(crate::from_tiff(&tiff).sony.unwrap().lens_correction, Some(lens));
        for bad in [Value::SShort(vec![4, 1, 2, 3, 4]), Value::SShort(vec![3, 1]), Value::SShort(vec![0]), Value::SShort(vec![17; 18])] {
            let entry = tiff.ifds[0].sub_ifds[0].entries.iter_mut().find(|e| e.tag == 0x7037).unwrap();
            entry.value = bad;
            assert!(lens_correction(&tiff).is_none());
        }
    }
}
