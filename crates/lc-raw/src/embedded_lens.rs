//! Camera-embedded lens correction metadata, independent of the pixel decoder.
//!
//! Tag layouts follow darktable `src/common/exif.cc::_check_lens_correction_data`
//! and the official ExifTool FujiFilm, Sony, Olympus and PanasonicRaw tag tables.
//! Values remain in the camera's units; the rendering model converts them once.

use lightcraft_tiff::{Ifd, ParseOptions, Tiff, Value, makernote, tags};
use serde::{Deserialize, Serialize};

use crate::RawFormat;

/// Validated correction data supplied by the camera. Missing or unrecognised
/// layouts are ignored; they never prevent decoding the photograph.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum VendorLens {
    Sony { distortion: Vec<f64>, ca_red: Vec<f64>, ca_blue: Vec<f64>, vignetting: Vec<f64> },
    Fuji { knots: Vec<f64>, distortion: Vec<f64>, ca_red: Vec<f64>, ca_blue: Vec<f64>, vignetting: Vec<f64>, crop_factor: f64 },
    Olympus { distortion: Option<[f64; 4]>, ca: Option<[f64; 6]> },
    Panasonic { coefficients: [f64; 3], scale: f64 },
}

pub(crate) fn extract(bytes: &[u8], format: RawFormat) -> Option<VendorLens> {
    if format == RawFormat::Raf {
        return extract_raf(bytes);
    }
    let opts = ParseOptions { max_ifds: 64, max_depth: 8, ..Default::default() };
    let tiff = Tiff::parse_with(bytes, &opts).ok()?;
    // Sony's SubImage1 data may also appear in IFD0 after a DNG round trip.
    for ifd in tiff.all_ifds() {
        if let Some(lens) = sony(ifd) {
            return Some(lens);
        }
    }
    let ifd0 = tiff.ifds.first()?;
    if let Some(lens) = panasonic(ifd0, format) {
        return Some(lens);
    }
    let make = tiff.find(tags::MAKE).and_then(|e| e.value.as_str()).unwrap_or_default();
    let e = tiff.exif()?.get(tags::MAKER_NOTE)?;
    let mn = makernote::parse_makernote(bytes, e.offset, e.count() as u64, tiff.order, &make)?;
    match mn.kind {
        makernote::MakerNoteKind::Fujifilm => fuji(&mn.ifd),
        makernote::MakerNoteKind::OlympusNew | makernote::MakerNoteKind::OlympusOld => olympus(&crate::vendor::orf::sub_ifd(bytes, &mn, 0x2040)?),
        _ => None,
    }
}

fn extract_raf(bytes: &[u8]) -> Option<VendorLens> {
    let preview = raf_exif(bytes);
    // In RAF the raw-data TIFF's proprietary RawIFD carries these tags directly.
    // Some metadata round trips instead retain them in the JPEG maker note.
    let mut lens = raf_slice(bytes, 100).and_then(|raw| crate::vendor::raf::raw_ifd(raw).ok()).and_then(|ifd| fuji(&ifd)).or_else(|| {
        let tiff = Tiff::parse(preview?).ok()?;
        let e = tiff.exif()?.get(tags::MAKER_NOTE)?;
        let mn = makernote::parse_makernote(preview?, e.offset, e.count() as u64, tiff.order, "FUJIFILM")?;
        fuji(&mn.ifd)
    })?;
    if let Some(preview) = preview
        && let Ok(tiff) = Tiff::parse(preview)
        && let Some(e) = tiff.exif().and_then(|ifd| ifd.get(tags::MAKER_NOTE))
        && let Some(mn) = makernote::parse_makernote(preview, e.offset, e.count() as u64, tiff.order, "FUJIFILM")
        && let VendorLens::Fuji { crop_factor, .. } = &mut lens
    {
        *crop_factor = if matches!(mn.ifd.u16(0x104d), Some(2 | 4)) { 1.25 } else { 1.0 };
    }
    Some(lens)
}

fn raf_slice(bytes: &[u8], field: usize) -> Option<&[u8]> {
    let order = lightcraft_tiff::ByteOrder::Big;
    let start = order.read_u32(bytes, field)? as usize;
    let length = order.read_u32(bytes, field + 4)? as usize;
    if start == 0 || length == 0 {
        return None;
    }
    bytes.get(start..start.checked_add(length)?)
}

fn raf_exif(bytes: &[u8]) -> Option<&[u8]> {
    let jpeg = raf_slice(bytes, 84)?;
    if !jpeg.starts_with(&[0xff, 0xd8]) {
        return None;
    }
    let mut at = 2usize;
    while jpeg.get(at) == Some(&0xff) {
        let marker = *jpeg.get(at.checked_add(1)?)?;
        if marker == 0xff {
            at += 1;
            continue;
        }
        if marker == 0xda || marker == 0xd9 {
            break;
        }
        if marker == 0xd8 || marker == 0x01 || (0xd0..=0xd7).contains(&marker) {
            at += 2;
            continue;
        }
        let len = lightcraft_tiff::ByteOrder::Big.read_u16(jpeg, at + 2)? as usize;
        if len < 2 {
            return None;
        }
        let end = at.checked_add(2)?.checked_add(len)?;
        let payload = jpeg.get(at + 4..end)?;
        if marker == 0xe1 && payload.starts_with(b"Exif\0\0") {
            return payload.get(6..);
        }
        at = end;
    }
    None
}

fn numbers(ifd: &Ifd, tag: u16) -> Option<Vec<f64>> {
    let value = ifd.value(tag)?;
    if value.count() > 64 || matches!(value, Value::Undefined(_) | Value::Byte(_)) {
        return None;
    }
    (0..value.count()).map(|i| value.get_f64(i).filter(|v| v.is_finite())).collect()
}

fn sony(ifd: &Ifd) -> Option<VendorLens> {
    let d = numbers(ifd, 0x7037)?;
    let c = numbers(ifd, 0x7035)?;
    let v = numbers(ifd, 0x7032)?;
    let count = *d.first()?;
    if !(2.0..=16.0).contains(&count) || count.fract() != 0.0 {
        return None;
    }
    let n = count as usize;
    if c.first() != Some(&(2.0 * count)) || v.first() != Some(&count) || d.len() < n + 1 || c.len() < 2 * n + 1 || v.len() < n + 1 {
        return None;
    }
    Some(VendorLens::Sony {
        distortion: d[1..=n].to_vec(),
        ca_red: c[1..=n].to_vec(),
        ca_blue: c[n + 1..=2 * n].to_vec(),
        vignetting: v[1..=n].to_vec(),
    })
}

fn fuji(ifd: &Ifd) -> Option<VendorLens> {
    let d = numbers(ifd, 0xf00b)?;
    let c = numbers(ifd, 0xf00f)?;
    let v = numbers(ifd, 0xf010)?;
    let (n, modern) = match (d.len(), c.len(), v.len()) {
        (19, 29, 19) => (9, true),
        (23, 31, 23) => (11, false),
        _ => return None,
    };
    let knots = d[1..=n].to_vec();
    if knots[0] < 0.0 || !knots.windows(2).all(|pair| pair[0] < pair[1]) {
        return None;
    }
    let mut ca_red = Vec::with_capacity(n);
    let mut ca_blue = Vec::with_capacity(n);
    for i in 0..n {
        let ca_knot = if modern {
            c[i + 1]
        } else if i == 0 {
            0.0
        } else {
            c[i]
        };
        if knots[i] != ca_knot || knots[i] != v[i + 1] {
            return None;
        }
        ca_red.push(if modern {
            c[i + 10]
        } else if i == 0 {
            0.0
        } else {
            c[i + 10]
        });
        ca_blue.push(if modern {
            c[i + 19]
        } else if i == 0 {
            0.0
        } else {
            c[i + 20]
        });
    }
    Some(VendorLens::Fuji {
        knots,
        distortion: d[n + 1..].to_vec(),
        ca_red,
        ca_blue,
        vignetting: v[n + 1..].to_vec(),
        crop_factor: if matches!(ifd.u16(0x104d), Some(2 | 4)) { 1.25 } else { 1.0 },
    })
}

fn olympus(ifd: &Ifd) -> Option<VendorLens> {
    let distortion = numbers(ifd, 0x150a).and_then(|v| <[f64; 4]>::try_from(v).ok()).filter(|v| v[..3].iter().any(|x| *x != 0.0));
    let ca = numbers(ifd, 0x150c).and_then(|v| <[f64; 6]>::try_from(v).ok()).filter(|v| v.iter().any(|x| *x != 0.0));
    (distortion.is_some() || ca.is_some()).then_some(VendorLens::Olympus { distortion, ca })
}

fn panasonic(ifd: &Ifd, format: RawFormat) -> Option<VendorLens> {
    let value = if format == RawFormat::Rw2 { ifd.value(0x0119) } else { ifd.value(0xf119) }?;
    let bytes = match value {
        Value::Undefined(v) | Value::Byte(v) => v,
        _ => return None,
    };
    if bytes.len() != 32 {
        return None;
    }
    let mut v = [0i16; 16];
    for (out, pair) in v.iter_mut().zip(bytes.as_chunks::<2>().0) {
        *out = i16::from_le_bytes(*pair);
    }
    if v[7] & 15 != 1 {
        return None;
    }
    let scale = 1.0 / (1.0 + f64::from(v[5]) / 32768.0);
    if !scale.is_finite() || scale <= 0.0 {
        return None;
    }
    Some(VendorLens::Panasonic { coefficients: [v[8], v[4], v[11]].map(|v| f64::from(v) / 32768.0), scale })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lightcraft_tiff::{ByteOrder, IfdBuilder, ImageData, TiffWriter};

    fn write(ifd: IfdBuilder) -> Vec<u8> {
        TiffWriter::default().write(&[ifd]).unwrap()
    }

    fn sony_ifd(n: i16) -> IfdBuilder {
        let mut ifd = IfdBuilder::new();
        ifd.set(0x7037, Value::SShort(vec![n, -10, 20]));
        ifd.set(0x7035, Value::SShort(vec![2 * n, 1, 2, -3, -4]));
        ifd.set(0x7032, Value::SShort(vec![n, 100, 200]));
        ifd
    }

    #[test]
    fn embedded_lens_sony_subifd_counts_and_signed_values() {
        let mut root = IfdBuilder::new();
        root.add_sub_ifd(sony_ifd(2));
        let lens = extract(&write(root), RawFormat::Arw).unwrap();
        assert_eq!(
            lens,
            VendorLens::Sony { distortion: vec![-10.0, 20.0], ca_red: vec![1.0, 2.0], ca_blue: vec![-3.0, -4.0], vignetting: vec![100.0, 200.0] }
        );
        for count in [-1, 0, 1, 3, 17] {
            assert_eq!(extract(&write(sony_ifd(count)), RawFormat::Arw), None);
        }
        let mut bad = sony_ifd(2);
        bad.set(0x7035, Value::SShort(vec![3, 1, 2, 3, 4]));
        assert_eq!(extract(&write(bad), RawFormat::Arw), None);
    }

    // A classic little-endian note IFD; value offsets are relative to note start.
    fn note_ifd(entries: &[(u16, Value)], offset: usize) -> Vec<u8> {
        let order = ByteOrder::Little;
        let mut directory = Vec::new();
        let mut data = Vec::new();
        order.put_u16(&mut directory, entries.len() as u16);
        let data_offset = offset + 2 + entries.len() * 12 + 4;
        for (tag, value) in entries {
            order.put_u16(&mut directory, *tag);
            order.put_u16(&mut directory, value.field_type() as u16);
            order.put_u32(&mut directory, value.count() as u32);
            let encoded = lightcraft_tiff::writer::encode(order, value);
            if encoded.len() <= 4 {
                directory.extend_from_slice(&encoded);
                directory.resize(directory.len() + 4 - encoded.len(), 0);
            } else {
                order.put_u32(&mut directory, (data_offset + data.len()) as u32);
                data.extend_from_slice(&encoded);
            }
        }
        order.put_u32(&mut directory, 0);
        directory.extend(data);
        directory
    }

    fn with_note(note: Vec<u8>) -> Vec<u8> {
        let mut exif = IfdBuilder::new();
        exif.set(tags::MAKER_NOTE, Value::Undefined(note));
        let mut root = IfdBuilder::new();
        root.set_child(tags::EXIF_IFD, exif);
        write(root)
    }

    fn fuji_tiff(n: usize, mismatch: bool) -> Vec<u8> {
        let knots: Vec<_> = (0..n).map(|i| i as f64 / (n - 1) as f64).collect();
        let mut d = vec![0.0];
        d.extend(&knots);
        d.extend(vec![0.25; n]);
        let v = d.clone();
        let mut c = vec![0.0];
        c.extend(if n == 9 { &knots[..] } else { &knots[1..] });
        c.extend(vec![0.1; if n == 9 { 19 } else { 20 }]);
        if mismatch {
            c[2] += 0.01;
        }
        let mut note = b"FUJIFILM\x0c\0\0\0".to_vec();
        note.extend(note_ifd(
            &[(0xf00b, Value::Double(d)), (0xf00f, Value::Double(c)), (0xf010, Value::Double(v)), (0x104d, Value::Short(vec![2]))],
            12,
        ));
        with_note(note)
    }

    fn raf(tiff: &[u8]) -> Vec<u8> {
        let mut jpeg = vec![0xff, 0xd8, 0xff, 0xe1];
        jpeg.extend_from_slice(&((tiff.len() + 8) as u16).to_be_bytes());
        jpeg.extend_from_slice(b"Exif\0\0");
        jpeg.extend_from_slice(tiff);
        jpeg.extend_from_slice(&[0xff, 0xd9]);
        let mut bytes = b"FUJIFILMCCD-RAW".to_vec();
        bytes.resize(160, 0);
        bytes[84..88].copy_from_slice(&160u32.to_be_bytes());
        bytes[88..92].copy_from_slice(&(jpeg.len() as u32).to_be_bytes());
        bytes.extend(jpeg);
        bytes
    }

    #[test]
    fn embedded_lens_fuji_both_knot_layouts_and_raf_exif() {
        for n in [9, 11] {
            let bytes = raf(&fuji_tiff(n, false));
            let VendorLens::Fuji { knots, distortion, ca_red, ca_blue, vignetting, crop_factor } = extract(&bytes, RawFormat::Raf).unwrap() else {
                panic!("wrong vendor");
            };
            assert_eq!(knots.len(), n);
            assert_eq!(distortion, vec![0.25; n]);
            assert_eq!(vignetting, vec![0.25; n]);
            assert_eq!((ca_red.len(), ca_blue.len()), (n, n));
            assert_eq!(ca_red[0], if n == 11 { 0.0 } else { 0.1 });
            assert_eq!(crop_factor, 1.25);
            assert_eq!(extract(&raf(&fuji_tiff(n, true)), RawFormat::Raf), None);
            assert_eq!(extract(&bytes[..bytes.len() - 20], RawFormat::Raf), None);
        }
    }

    #[test]
    fn embedded_lens_fuji_raw_tiff_with_preview_crop_mode() {
        let preview_tiff = fuji_tiff(9, false);
        let tiff = Tiff::parse(&preview_tiff).unwrap();
        let e = tiff.exif().unwrap().get(tags::MAKER_NOTE).unwrap();
        let mn = makernote::parse_makernote(&preview_tiff, e.offset, e.count() as u64, tiff.order, "FUJIFILM").unwrap();
        let mut raw_ifd = IfdBuilder::new();
        for tag in [0xf00b, 0xf00f, 0xf010] {
            raw_ifd.set(tag, mn.ifd.value(tag).unwrap().clone());
        }
        let mut root = IfdBuilder::new();
        root.set_child(0xf000, raw_ifd);
        let raw = write(root);
        // The preview supplies crop mode, but deliberately has no lens tables.
        let mut note = b"FUJIFILM\x0c\0\0\0".to_vec();
        note.extend(note_ifd(&[(0x104d, Value::Short(vec![4]))], 12));
        let mut bytes = raf(&with_note(note));
        let raw_start = bytes.len() as u32;
        bytes[100..104].copy_from_slice(&raw_start.to_be_bytes());
        bytes[104..108].copy_from_slice(&(raw.len() as u32).to_be_bytes());
        bytes.extend(raw);
        let VendorLens::Fuji { crop_factor, distortion, .. } = extract(&bytes, RawFormat::Raf).unwrap() else {
            panic!("wrong vendor");
        };
        assert_eq!(crop_factor, 1.25);
        assert_eq!(distortion, vec![0.25; 9]);
        assert_eq!(extract(&bytes[..bytes.len() - 1], RawFormat::Raf), None);
    }

    #[test]
    fn embedded_lens_olympus_nested_ifd_and_neutral_values() {
        for (d, expected) in [(vec![0.1, -0.2, 0.3, 1.0], true), (vec![0.0, 0.0, 0.0, 1.0], false), (vec![f64::NAN, 0.0, 0.0, 1.0], false)] {
            let mut note = b"OLYMPUS\0II\x03\0".to_vec();
            note.extend(note_ifd(&[(0x2040, Value::Long(vec![30]))], 12));
            note.extend(note_ifd(&[(0x150a, Value::Double(d)), (0x150c, Value::Double(vec![0.0; 6]))], 30));
            let lens = extract(&with_note(note), RawFormat::Orf);
            assert_eq!(lens.is_some(), expected);
            if let Some(VendorLens::Olympus { distortion, ca }) = lens {
                assert_eq!(distortion, Some([0.1, -0.2, 0.3, 1.0]));
                assert_eq!(ca, None);
            }
        }
    }

    #[test]
    fn embedded_lens_panasonic_blob_enabled_layout_and_corruption() {
        let mut v = [0i16; 16];
        v[4] = -4096;
        v[5] = 8192;
        v[7] = 1;
        v[8] = 2048;
        v[11] = 1024;
        // Real enabled Panasonic files have differing word12 values.
        v[12] = 2571;
        let expected = Some(VendorLens::Panasonic { coefficients: [0.0625, -0.125, 0.03125], scale: 0.8 });
        for tag in [0x0119, 0xf119] {
            let mut ifd = IfdBuilder::new();
            ifd.set(tag, Value::Undefined(v.iter().flat_map(|v| v.to_le_bytes()).collect()));
            assert_eq!(extract(&write(ifd), if tag == 0x0119 { RawFormat::Rw2 } else { RawFormat::Dng }), expected);
        }
        for (index, value) in [(7, 0), (5, i16::MIN)] {
            let mut bad = v;
            bad[index] = value;
            let mut ifd = IfdBuilder::new();
            ifd.set(0x0119, Value::Undefined(bad.iter().flat_map(|v| v.to_le_bytes()).collect()));
            assert_eq!(extract(&write(ifd), RawFormat::Rw2), None);
        }
        assert_eq!(extract(&[0xff; 64], RawFormat::Rw2), None);
    }

    #[test]
    fn embedded_lens_decode_and_header_info_preserve_correction_data() {
        let mut ifd = sony_ifd(2);
        ifd.set(tags::DNG_VERSION, Value::Byte(vec![1, 4, 0, 0]));
        ifd.set(tags::PHOTOMETRIC, Value::Short(vec![34892]));
        ifd.set(tags::IMAGE_WIDTH, Value::Long(vec![2]));
        ifd.set(tags::IMAGE_LENGTH, Value::Long(vec![2]));
        ifd.set(tags::BITS_PER_SAMPLE, Value::Short(vec![16]));
        ifd.set(tags::SAMPLES_PER_PIXEL, Value::Short(vec![3]));
        ifd.set(tags::COMPRESSION, Value::Short(vec![1]));
        ifd.image = Some(ImageData::Strips { strips: vec![vec![0; 24]], rows_per_strip: 2 });
        let bytes = write(ifd);
        let image = crate::decode(&bytes).unwrap();
        assert!(image.vendor_lens.is_some());
        assert_eq!(crate::probe_info(&bytes).unwrap().vendor_lens, image.vendor_lens);
        assert_eq!(image.info().vendor_lens, image.clone().into_info().vendor_lens);
    }
}
