//! Parity and hostile-input tests for the shared borrowed pixel-codec directory reader.
use lightcraft_tiff::directory::{Entry, File};
use lightcraft_tiff::{ByteOrder, IfdBuilder, Tiff, TiffWriter, Value, tags};

const ORDERS: [ByteOrder; 2] = [ByteOrder::Little, ByteOrder::Big];

#[test]
fn shared_directory_values_match_owned_reader_in_all_layouts() {
    for order in ORDERS {
        for big in [false, true] {
            let ifd = IfdBuilder::new()
                .with(tags::IMAGE_WIDTH, Value::Long(vec![640]))
                .with(tags::BITS_PER_SAMPLE, Value::Short(vec![16, 16, 16]))
                .with(tags::MAKE, Value::Ascii("Test camera".into()))
                .with(1000, Value::Rational(vec![(3, 2)]))
                .with(1001, Value::Float(vec![0.25]))
                .with(1002, Value::Double(vec![0.125]))
                .with(1003, Value::Undefined(vec![1, 2, 3, 4, 5, 6, 7, 8, 9]))
                .with(1004, Value::Rational(vec![(1, 0)]));
            let bytes = TiffWriter::new(order, big).write(&[ifd]).unwrap();
            let owned = Tiff::parse(&bytes).unwrap();
            let file = File::parse(&bytes).unwrap();
            assert_eq!(file.order(), order);
            assert_eq!(file.big, big);
            let view = file.ifd(file.first_ifd).unwrap();
            assert_eq!(view.next, 0);
            assert_eq!(file.entry_count(view.at), Some(owned.ifds[0].entries.len() as u64));
            assert_eq!(file.tag_uint(&view, tags::IMAGE_WIDTH), owned.ifds[0].u64(tags::IMAGE_WIDTH));
            assert_eq!(file.tag_uints(&view, tags::BITS_PER_SAMPLE, 2), Some(vec![16, 16]));
            for tag in [1000, 1001, 1002, 1004] {
                assert_eq!(file.tag_f64(&view, tag), owned.ifds[0].f64(tag));
            }
            for tag in [tags::MAKE, 1003] {
                let entry = view.get(tag).unwrap();
                let payload = file.tag_bytes(&view, tag).unwrap();
                assert_eq!(entry.value_at, owned.ifds[0].get(tag).unwrap().offset);
                assert_eq!(payload.as_ptr(), bytes[entry.value_at as usize..].as_ptr());
            }
            assert_eq!(file.uint(view.get(tags::BITS_PER_SAMPLE).unwrap(), 3), None);
        }
    }
}

#[test]
fn shared_directory_rejects_incomplete_tables_and_offsets_in_header() {
    for order in ORDERS {
        for big in [false, true] {
            let bytes = TiffWriter::new(order, big).write(&[IfdBuilder::new().with(tags::IMAGE_WIDTH, Value::Short(vec![640]))]).unwrap();
            let file = File::parse(&bytes).unwrap();
            assert!(file.ifd(file.header_len() - 1).is_none());
            for len in file.header_len() as usize..bytes.len() {
                let short = File::parse(&bytes[..len]).unwrap();
                assert!(short.ifd(short.first_ifd).is_none(), "order {order:?} big {big} len {len}");
            }
            assert!(file.ifd(file.first_ifd).is_some());
            assert_eq!(file.u64_at(u64::MAX), None);
        }
    }
}

#[test]
fn shared_directory_keeps_first_duplicate_and_skips_unknown_field_types() {
    // Unsorted entries and duplicate tags are legal to inspect; keep original file order.
    let mut bytes = b"II\x2a\0\x08\0\0\0".to_vec();
    bytes.extend(3u16.to_le_bytes());
    for (tag, ty, value) in [(300u16, 3u16, 7u32), (256, 99, 22), (300, 3, 9)] {
        bytes.extend(tag.to_le_bytes());
        bytes.extend(ty.to_le_bytes());
        bytes.extend(1u32.to_le_bytes());
        bytes.extend(value.to_le_bytes());
    }
    bytes.extend(0u32.to_le_bytes());
    let file = File::parse(&bytes).unwrap();
    let view = file.ifd(file.first_ifd).unwrap();
    assert_eq!(view.entries.len(), 2);
    assert_eq!(file.tag_uint(&view, 300), Some(7));
    assert!(view.get(256).is_none());
}

#[test]
fn shared_directory_rejects_huge_counts_without_allocating_payloads() {
    let mut bytes = b"II\x2b\0\x08\0\0\0".to_vec();
    bytes.extend(16u64.to_le_bytes());
    bytes.extend(u64::MAX.to_le_bytes());
    let file = File::parse(&bytes).unwrap();
    assert_eq!(file.entry_count(file.first_ifd), Some(u64::MAX));
    assert!(file.ifd(file.first_ifd).is_none());
    let entry = Entry { tag: 1, ty: 16, count: u64::MAX, value_at: 0 };
    assert!(file.data(&entry).is_none());
    assert_eq!(file.uint(&entry, u64::MAX - 1), None);
}

#[test]
fn shared_directory_caps_subifd_offsets_before_materializing_them() {
    let bytes = TiffWriter::default().write(&[IfdBuilder::new().with(tags::SUB_IFDS, Value::Long(vec![8; 300]))]).unwrap();
    let file = File::parse(&bytes).unwrap();
    let view = file.ifd(file.first_ifd).unwrap();
    assert_eq!(file.sub_ifds(&view), vec![8; 256]);
}

#[test]
fn shared_directory_image_headers_reject_raw_magic_and_invalid_bigtiff_offsets() {
    for magic in [0x4f52u16, 0x5352, 0x0055] {
        let mut bytes = b"II\0\0\x08\0\0\0".to_vec();
        bytes[2..4].copy_from_slice(&magic.to_le_bytes());
        assert!(Tiff::sniff(&bytes).is_some());
        assert!(File::parse(&bytes).is_none());
    }
    for (size, reserved) in [(4u16, 0u16), (8, 1)] {
        let mut bytes = b"II\x2b\0".to_vec();
        bytes.extend(size.to_le_bytes());
        bytes.extend(reserved.to_le_bytes());
        bytes.extend(16u64.to_le_bytes());
        assert!(File::parse(&bytes).is_none());
    }
}

#[test]
fn shared_directory_exif_summary_orientation_and_adobe_rgb_hint() {
    use lightcraft_tiff::exif::{minimal_exif, summarize};
    for orientation in 1..=8 {
        assert_eq!(summarize(&minimal_exif(orientation)).orientation, Some(orientation));
    }
    for order in ORDERS {
        for big in [false, true] {
            let mut exif = IfdBuilder::new().with(0xa001, Value::Short(vec![0xffff]));
            exif.set_child(0xa005, IfdBuilder::new().with(1, Value::Ascii("R03".into())));
            let mut ifd0 = IfdBuilder::new().with(274, Value::Short(vec![6]));
            ifd0.set_child(0x8769, exif);
            let bytes = TiffWriter::new(order, big).write(&[ifd0]).unwrap();
            let summary = summarize(&bytes);
            assert_eq!(summary.orientation, Some(6));
            assert!(summary.adobe_rgb_hint);
            assert_eq!(summary.thumbnail, None);
            let mut prefixed = b"Exif\0\0".to_vec();
            prefixed.extend(bytes);
            assert_eq!(summarize(&prefixed), summary);
        }
    }
    assert_eq!(summarize(&minimal_exif(9)).orientation, None);
    assert_eq!(summarize(&[]), Default::default());
    assert_eq!(summarize(b"II*\0\xff\xff\xff\xff"), Default::default());
}

#[test]
fn shared_directory_exif_summary_borrows_only_bounded_jpeg_thumbnail_extent() {
    use lightcraft_tiff::exif::summarize;
    let thumbnail = [0xff, 0xd8, 7, 8, 0xff, 0xd9];
    let ifd0 = IfdBuilder::new().with(274, Value::Short(vec![1]));
    let ifd1 = IfdBuilder::new().with(0x0201, Value::Long(vec![1])).with(0x0202, Value::Long(vec![thumbnail.len() as u32]));
    let mut bytes = TiffWriter::default().write(&[ifd0, ifd1]).unwrap();
    let file = File::parse(&bytes).unwrap();
    let ifd0 = file.ifd(file.first_ifd).unwrap();
    let ifd1 = file.ifd(ifd0.next).unwrap();
    let offset_at = ifd1.get(0x0201).unwrap().value_at as usize;
    let length_at = ifd1.get(0x0202).unwrap().value_at as usize;
    let offset = bytes.len();
    bytes[offset_at..offset_at + 4].copy_from_slice(&(offset as u32).to_le_bytes());
    bytes.extend(thumbnail);
    assert_eq!(summarize(&bytes).thumbnail, Some((offset, thumbnail.len())));
    bytes[length_at..length_at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(summarize(&bytes).thumbnail, None);
}

#[test]
fn shared_directory_camera_raw_identifies_dng_and_cfa_children() {
    for order in ORDERS {
        let dng = IfdBuilder::new().with(tags::DNG_VERSION, Value::Byte(vec![1, 7, 0, 0]));
        let bytes = TiffWriter::new(order, false).write(&[dng]).unwrap();
        assert!(File::parse(&bytes).unwrap().is_camera_raw());
        for photo in [32803, 34892] {
            let mut preview = IfdBuilder::new().with(tags::PHOTOMETRIC, Value::Short(vec![2]));
            preview.add_sub_ifd(IfdBuilder::new().with(tags::PHOTOMETRIC, Value::Long(vec![photo])));
            let bytes = TiffWriter::new(order, false).write(&[preview]).unwrap();
            assert!(File::parse(&bytes).unwrap().is_camera_raw());
        }
        let flat = IfdBuilder::new().with(tags::PHOTOMETRIC, Value::Short(vec![2]));
        let bytes = TiffWriter::new(order, false).write(&[flat]).unwrap();
        assert!(!File::parse(&bytes).unwrap().is_camera_raw());
    }
}

#[test]
fn shared_directory_exif_dimensions_preserve_makernotes_offsets_and_padding() {
    use lightcraft_tiff::exif::update_dimensions;
    for order in ORDERS {
        for big in [false, true] {
            for width in [123u32, 100_000] {
                let opaque = b"Opaque MakerNote with vendor-specific offsets\x20\0\0\0";
                let mut exif = IfdBuilder::new()
                    .with(40962, Value::Short(vec![20]))
                    .with(40963, Value::Long(vec![20, 21, 22]))
                    .with(tags::MAKER_NOTE, Value::Undefined(opaque.to_vec()));
                // GPS/Interop dimensions must stay untouched: only IFD0 and its Exif IFDs
                // describe the replacement image, and MakerNotes retain all their offsets.
                exif.set_child(0xa005, IfdBuilder::new().with(40962, Value::Long(vec![77])));
                let mut root = IfdBuilder::new().with(tags::IMAGE_WIDTH, Value::Short(vec![20])).with(257, Value::Short(vec![30]));
                root.set_child(0x8769, exif);
                let mut bytes = TiffWriter::new(order, big).write(&[root]).unwrap();
                let file = File::parse(&bytes).unwrap();
                let root = file.ifd(file.first_ifd).unwrap();
                let exif_at = file.tag_uint(&root, 0x8769).unwrap();
                let exif = file.ifd(exif_at).unwrap();
                let maker_at = exif.get(tags::MAKER_NOTE).unwrap().value_at as usize;
                let old_height_at = exif.get(40963).unwrap().value_at as usize;
                let (head, stride, slot_offset) = if big { (8usize, 20usize, 12usize) } else { (2, 12, 8) };
                let mut allowed = vec![false; bytes.len()];
                // Seed distinctive unused inline bytes before the update, then compare every
                // byte except the exact type/count/new-value patch ranges.
                let mut padding = Vec::new();
                for (dir, desired_width_tag, desired_height_tag) in [(&root, 256, 257), (&exif, 40962, 40963)] {
                    for (index, entry) in dir.entries.iter().enumerate() {
                        let dimension = if entry.tag == desired_width_tag {
                            width
                        } else if entry.tag == desired_height_tag {
                            456
                        } else {
                            continue;
                        };
                        let at = dir.at as usize + head + index * stride;
                        let short = entry.ty == 3 && entry.count == 1 && dimension <= u16::MAX as u32;
                        let size = if short { 2 } else { 4 };
                        allowed[at + 2..at + slot_offset].fill(true);
                        allowed[at + slot_offset..at + slot_offset + size].fill(true);
                        // Only original inline entries have padding worth testing.
                        if entry.count == 1 {
                            for byte in at + slot_offset + size..at + stride {
                                padding.push(byte);
                            }
                        }
                    }
                }
                for byte in padding {
                    bytes[byte] = 0xa5;
                }
                let before = bytes.clone();
                update_dimensions(&mut bytes, width, 456).unwrap();
                for (index, (&old, &new)) in before.iter().zip(&bytes).enumerate() {
                    if !allowed[index] {
                        assert_eq!(old, new, "unexpected mutation at {index} order {order:?} big {big}");
                    }
                }
                let file = File::parse(&bytes).unwrap();
                let root = file.ifd(file.first_ifd).unwrap();
                let exif = file.ifd(exif_at).unwrap();
                assert_eq!(file.tag_uint(&root, 256), Some(u64::from(width)));
                assert_eq!(file.tag_uint(&root, 257), Some(456));
                assert_eq!(root.get(256).unwrap().ty, if width <= u16::MAX as u32 { 3 } else { 4 });
                assert_eq!(file.tag_uint(&exif, 40962), Some(u64::from(width)));
                assert_eq!(file.tag_uint(&exif, 40963), Some(456));
                assert_eq!((exif.get(40963).unwrap().ty, exif.get(40963).unwrap().count), (4, 1));
                assert_eq!(file.tag_bytes(&exif, tags::MAKER_NOTE), Some(opaque.as_slice()));
                assert_eq!(exif.get(tags::MAKER_NOTE).unwrap().value_at as usize, maker_at);
                assert_eq!(&bytes[old_height_at..old_height_at + 12], &before[old_height_at..old_height_at + 12]);
                let interop = file.ifd(file.tag_uint(&exif, 0xa005).unwrap()).unwrap();
                assert_eq!(file.tag_uint(&interop, 40962), Some(77));
            }
        }
    }
}

#[test]
fn shared_directory_exif_dimensions_validate_every_child_before_mutating() {
    use lightcraft_tiff::exif::update_dimensions;
    let root = IfdBuilder::new().with(256, Value::Short(vec![20])).with(257, Value::Short(vec![30])).with(0x8769, Value::Long(vec![u32::MAX]));
    let mut bytes = TiffWriter::default().write(&[root]).unwrap();
    let before = bytes.clone();
    assert!(update_dimensions(&mut bytes, 100_000, 456).is_err());
    assert_eq!(bytes, before);
    let root = IfdBuilder::new().with(256, Value::Short(vec![20])).with(0x8769, Value::Short(vec![12]));
    let mut bytes = TiffWriter::default().write(&[root]).unwrap();
    let before = bytes.clone();
    assert!(update_dimensions(&mut bytes, 100_000, 456).is_err());
    assert_eq!(bytes, before);
}

#[test]
fn shared_directory_exif_thumbnail_unlinks_only_ifd0_next_pointer() {
    use lightcraft_tiff::exif::{drop_thumbnail, update_dimensions};
    for order in ORDERS {
        for big in [false, true] {
            let bytes = TiffWriter::new(order, big)
                .write(&[IfdBuilder::new().with(256, Value::Short(vec![20])), IfdBuilder::new().with(256, Value::Short(vec![5]))])
                .unwrap();
            for prefix in [false, true] {
                let mut bytes = if prefix { [b"Exif\0\0".as_slice(), &bytes].concat() } else { bytes.clone() };
                update_dimensions(&mut bytes, 321, 456).unwrap();
                let body_at = if prefix { 6 } else { 0 };
                let file = File::parse(&bytes[body_at..]).unwrap();
                let entries = file.entries_at(file.first_ifd).unwrap();
                let at = body_at + entries.next_at as usize;
                let end = at + if big { 8 } else { 4 };
                let before = bytes.clone();
                assert!(drop_thumbnail(&mut bytes));
                assert_eq!(&bytes[..at], &before[..at]);
                assert_eq!(&bytes[end..], &before[end..]);
                assert!(bytes[at..end].iter().all(|&byte| byte == 0));
            }
            let mut truncated = bytes[..bytes.len() - 1].to_vec();
            let file = File::parse(&truncated).unwrap();
            let first = file.first_ifd as usize;
            truncated.truncate(first + if big { 8 } else { 2 });
            let before = truncated.clone();
            assert!(!drop_thumbnail(&mut truncated));
            assert_eq!(truncated, before);
        }
    }
}

#[test]
fn shared_directory_exif_dimension_patch_limits_distinct_directories_atomically() {
    use lightcraft_tiff::exif::update_dimensions;
    let mut root = IfdBuilder::new().with(256, Value::Short(vec![20]));
    for tag in 34665..=34668 {
        root.set_child(tag, IfdBuilder::new().with(40962, Value::Short(vec![20])));
    }
    let mut bytes = TiffWriter::default().write(&[root]).unwrap();
    let file = File::parse(&bytes).unwrap();
    let root = file.ifd(file.first_ifd).unwrap();
    let tag_positions = root
        .entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| entry.tag >= 34665)
        .map(|(index, _)| root.at as usize + 2 + index * 12)
        .collect::<Vec<_>>();
    for position in tag_positions {
        bytes[position..position + 2].copy_from_slice(&34665u16.to_le_bytes());
    }
    let before = bytes.clone();
    assert!(matches!(update_dimensions(&mut bytes, 123, 456), Err(lightcraft_tiff::TiffError::Limit("Exif directory count"))));
    assert_eq!(bytes, before);
}

#[test]
fn shared_directory_camera_raw_unifies_vendor_sony_bigtiff_and_next_ifds() {
    for order in ORDERS {
        for big in [false, true] {
            let sony = IfdBuilder::new()
                .with(tags::MAKE, Value::Ascii("SONY".into()))
                .with(tags::COMPRESSION, Value::Short(vec![7]))
                .with(tags::PHOTOMETRIC, Value::Short(vec![6]))
                .with(tags::SAMPLES_PER_PIXEL, Value::Short(vec![3]))
                .with(tags::CFA_PATTERN_EP, Value::Undefined(vec![0, 1, 1, 2]));
            let bytes = TiffWriter::new(order, big).write(&[sony]).unwrap();
            assert!(File::parse(&bytes).unwrap().is_camera_raw());
            let preview = IfdBuilder::new().with(tags::PHOTOMETRIC, Value::Short(vec![2]));
            let raw = IfdBuilder::new().with(tags::COMPRESSION, Value::Short(vec![34713]));
            let bytes = TiffWriter::new(order, big).write(&[preview, raw]).unwrap();
            assert!(File::parse(&bytes).unwrap().is_camera_raw());
            let flat = IfdBuilder::new()
                .with(tags::COMPRESSION, Value::Short(vec![7]))
                .with(tags::PHOTOMETRIC, Value::Short(vec![6]))
                .with(tags::SAMPLES_PER_PIXEL, Value::Short(vec![3]));
            let bytes = TiffWriter::new(order, big).write(&[flat]).unwrap();
            assert!(!File::parse(&bytes).unwrap().is_camera_raw());
        }
    }
}
