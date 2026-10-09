//! Bounded, seek-based extraction for classic TIFF/DNG/Sony previews. Unsupported layouts
//! return None so callers can use the complete buffer extractor without changing the image.
use crate::Orientation;
use lightcraft_tiff::{ByteOrder, tags as t};
use std::collections::HashSet;
use std::io::{Read, Seek, SeekFrom};

/// An embedded JPEG and the enclosing raw's display orientation. The JPEG's own EXIF
/// orientation takes precedence when it is greater than one, as in the buffer path.
pub struct EmbeddedPreview {
    pub jpeg: Vec<u8>,
    pub orientation: Orientation,
}
// None means the bounded prefix cannot establish the first SOF; never guess a candidate's type.
fn dct_prefix(bytes: &[u8]) -> Option<bool> {
    if !bytes.starts_with(&[0xff, 0xd8]) {
        return Some(false);
    }
    let mut i = 2;
    while i + 4 <= bytes.len() {
        if bytes[i] != 0xff {
            return Some(false);
        }
        let marker = bytes[i + 1];
        if marker == 0xff {
            i += 1;
            continue;
        }
        match marker {
            0xc0..=0xc2 => return Some(true),
            0xc3 | 0xc5..=0xc7 | 0xcb | 0xcd..=0xcf | 0xda | 0xd9 => return Some(false),
            _ => {}
        }
        i += 2 + u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]) as usize;
    }
    None
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Directory {
    Main,
    Child,
    Exif,
    MakerNote,
    MakerPreview,
}
struct Reader<'a, R> {
    stream: &'a mut R,
    len: u64,
    metadata_left: usize,
    visited: HashSet<u64>,
    exif_count: usize,
    best: Option<Vec<u8>>,
    orientation: Option<Orientation>,
    xmp_orientation: Option<Orientation>,
}
impl<R: Read + Seek> Reader<'_, R> {
    fn read(&mut self, at: u64, n: usize) -> Option<Vec<u8>> {
        if at.checked_add(n as u64)? > self.len || n > self.metadata_left {
            return None;
        }
        self.metadata_left -= n;
        self.stream.seek(SeekFrom::Start(at)).ok()?;
        let mut bytes = vec![0; n];
        self.stream.read_exact(&mut bytes).ok()?;
        Some(bytes)
    }
    fn jpeg(&mut self, at: u64, n: u64) -> Option<()> {
        if n < 4 {
            return Some(());
        }
        if at.checked_add(n)? > self.len {
            return None;
        }
        let header = self.read(at, 4)?;
        if !header.starts_with(&[0xff, 0xd8]) {
            return Some(());
        }
        // Never allocate according to an unchecked raw-file tag. Large or unusual previews
        // are handled by the existing buffer extractor instead.
        if n > 64 << 20 {
            return None;
        }
        // Lossless JPEG strips are sensor samples, not display previews. Inspect a bounded
        // prefix before reading their payload; unusually large APP headers use the buffer path.
        let prefix = self.read(at, n.min(4096) as usize)?;
        match dct_prefix(&prefix) {
            Some(false) => return Some(()),
            None => return None,
            Some(true) => {}
        }
        self.stream.seek(SeekFrom::Start(at)).ok()?;
        let mut jpeg = vec![0; n as usize];
        self.stream.read_exact(&mut jpeg).ok()?;
        if super::preview::is_dct_jpeg(&jpeg) {
            // The buffer extractor's tie order also includes maker-note previews after all
            // normal IFDs. Fall back for distinct equal-size candidates rather than guess.
            if self.best.as_ref().is_some_and(|b| b.len() == jpeg.len() && b != &jpeg) {
                return None;
            }
            if self.best.as_ref().is_none_or(|b| b.len() < jpeg.len()) {
                self.best = Some(jpeg);
            }
        }
        Some(())
    }
    fn numbers(&mut self, e: &[u8], order: ByteOrder, base: u64) -> Option<Vec<u64>> {
        let ty = order.read_u16(e, 2)?;
        let count = order.read_u32(e, 4)? as usize;
        let size = match ty {
            3 => 2,
            4 | 13 => 4,
            16 | 18 => 8,
            _ => return None,
        };
        if count > 64 {
            return None;
        }
        let total = count.checked_mul(size)?;
        let bytes = if total <= 4 { e[8..8 + total].to_vec() } else { self.read(base.checked_add(order.read_u32(e, 8)? as u64)?, total)? };
        (0..count)
            .map(|i| match size {
                2 => order.read_u16(&bytes, i * size).map(u64::from),
                4 => order.read_u32(&bytes, i * size).map(u64::from),
                _ => order.read_u64(&bytes, i * size),
            })
            .collect()
    }
    fn ifd(&mut self, at: u64, base: u64, order: ByteOrder, root: bool, kind: Directory) -> Option<()> {
        if at == 0 {
            return Some(());
        }
        if self.visited.len() >= 64 || !self.visited.insert(at) {
            return None;
        }
        if kind == Directory::Exif {
            self.exif_count += 1;
            if self.exif_count > 1 {
                return None;
            }
        }
        let maker = matches!(kind, Directory::MakerNote | Directory::MakerPreview);
        let head = self.read(at, 2)?;
        let n = order.read_u16(&head, 0)? as usize;
        if n == 0 || n > 8192 {
            return None;
        }
        let table = self.read(at.checked_add(2)?, n.checked_mul(12)?.checked_add(4)?)?;
        let mut off = None;
        let mut len = None;
        let mut strip = None;
        let mut strip_len = None;
        let mut compression = 0;
        let mut tiled = false;
        let mut children = Vec::new();
        let mut notes = Vec::new();
        let mut tags = HashSet::new();
        for e in table[..n * 12].chunks_exact(12) {
            let tag = order.read_u16(e, 0)?;
            if !tags.insert(tag) {
                return None;
            }
            let ty = order.read_u16(e, 2)?;
            let count = order.read_u32(e, 4)? as u64;
            if matches!(
                tag,
                t::JPEG_INTERCHANGE_FORMAT
                    | t::JPEG_INTERCHANGE_FORMAT_LENGTH
                    | t::COMPRESSION
                    | t::ORIENTATION
                    | t::SUB_IFDS
                    | t::EXIF_IFD
                    | t::GPS_IFD
                    | t::INTEROP_IFD
            ) {
                let values = self.numbers(e, order, base)?;
                let value = values.first().copied();
                match tag {
                    t::JPEG_INTERCHANGE_FORMAT => off = value,
                    t::JPEG_INTERCHANGE_FORMAT_LENGTH => len = value,
                    t::COMPRESSION => compression = value.unwrap_or(0),
                    t::ORIENTATION if root => self.orientation = value.filter(|v| (1..=8).contains(v)).map(|v| Orientation::from_exif(v as u16)),
                    t::SUB_IFDS | t::EXIF_IFD | t::GPS_IFD | t::INTEROP_IFD => {
                        if !maker {
                            let child_kind = if tag == t::EXIF_IFD { Directory::Exif } else { Directory::Child };
                            if tag != t::SUB_IFDS && values.len() != 1 {
                                return None;
                            }
                            children.extend(values.into_iter().map(|off| (off, child_kind)));
                        }
                    }

                    _ => {}
                }
            }
            if matches!(tag, t::TILE_OFFSETS | t::TILE_BYTE_COUNTS) && count == 1 {
                tiled = true;
            }
            if kind == Directory::MakerNote && tag == 0x0011 {
                let value = if matches!(ty, 1 | 7) {
                    if count <= 4 { e[8] as u64 } else { self.read(base.checked_add(order.read_u32(e, 8)? as u64)?, 1)?[0] as u64 }
                } else {
                    self.numbers(e, order, base)?.first().copied()?
                };
                if value != 0 {
                    children.push((value, Directory::MakerPreview));
                }
            }
            if matches!(tag, t::STRIP_OFFSETS | t::STRIP_BYTE_COUNTS) && count == 1 {
                let value = self.numbers(e, order, base)?.first().copied();
                if tag == t::STRIP_OFFSETS {
                    strip = value;
                } else {
                    strip_len = value;
                }
            }
            if ty == 7 && count > 1024 {
                let at = base.checked_add(order.read_u32(e, 8)? as u64)?;
                self.jpeg(at, count)?;
            }
            if tag == t::MAKER_NOTE {
                if kind != Directory::Exif || ty != 7 || count < 2 {
                    return None;
                }
                notes.push((base.checked_add(order.read_u32(e, 8)? as u64)?, count));
            }
            if root && tag == t::XMP {
                if !matches!(ty, 1 | 7) || count > 1 << 20 {
                    return None;
                }
                let bytes = if count <= 4 {
                    e[8..8 + count as usize].to_vec()
                } else {
                    self.read(base.checked_add(order.read_u32(e, 8)? as u64)?, count as usize)?
                };
                self.xmp_orientation = lightcraft_meta::parse_xmp(&String::from_utf8_lossy(&bytes)).ok().and_then(|x| x.metadata.orientation);
            }
        }
        if let (Some(off), Some(len)) = (off, len) {
            self.jpeg(base.checked_add(off)?, len)?;
        }
        if matches!(compression, 6 | 7 | 34892) && tiled {
            return None;
        }
        if matches!(compression, 6 | 7 | 34892)
            && let (Some(off), Some(len)) = (strip, strip_len)
        {
            self.jpeg(base.checked_add(off)?, len)?;
        }
        // Sony notes use TIFF-relative offsets; plain TIFF maker notes do too. Other vendor
        // dialects deliberately fall back rather than guessing their offset bases.
        for (note, count) in notes {
            let header = self.read(note, count.min(16) as usize)?;
            let start = if header.starts_with(b"SONY") || header.starts_with(b"VHAB     \0") {
                note.checked_add(12)?
            } else if order.read_u16(&header, 0).is_some_and(|n| n > 0 && n <= 8192) {
                note
            } else {
                return None;
            };
            self.ifd(start, 0, order, false, Directory::MakerNote)?;
        }
        for (off, child_kind) in children {
            self.ifd(base.checked_add(off)?, base, order, false, child_kind)?;
        }
        let next = order.read_u32(&table, n * 12)? as u64;
        // The buffer parser follows next pointers only in the main IFD chain. A child
        // next pointer is not another preview candidate (nor a second maker-note level).
        if next != 0 && kind == Directory::Main {
            self.ifd(base.checked_add(next)?, base, order, false, Directory::Main)?;
        }
        Some(())
    }
}
/// Extract without reading sensor payloads. Uses at most 1 MiB of metadata and two JPEG buffers
/// (up to 64 MiB each), independent of raw-file size. Returns None for unsupported layouts,
/// malformed/cyclic directories, or no DCT preview; fall back to [`crate::embedded_preview`].
pub fn embedded_preview_reader<R: Read + Seek>(stream: &mut R) -> Option<EmbeddedPreview> {
    let len = stream.seek(SeekFrom::End(0)).ok()?;
    let mut r =
        Reader { stream, len, metadata_left: 1 << 20, visited: HashSet::new(), exif_count: 0, best: None, orientation: None, xmp_orientation: None };
    let header = r.read(0, 8)?;
    let order = match &header[..2] {
        b"II" => ByteOrder::Little,
        b"MM" => ByteOrder::Big,
        _ => return None,
    };
    if order.read_u16(&header, 2)? != 42 {
        return None;
    }
    r.ifd(order.read_u32(&header, 4)? as u64, 0, order, true, Directory::Main)?;
    let mut jpeg = r.best?;
    jpeg.truncate(super::preview::trim_eoi(&jpeg).len());
    Some(EmbeddedPreview { jpeg, orientation: r.orientation.or(r.xmp_orientation).unwrap_or_default() })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod library_scale_tests {
    use super::*;
    use lightcraft_tiff::{IfdBuilder, ImageData, TiffWriter, Value};
    use std::io::Cursor;
    fn jpeg(n: usize) -> Vec<u8> {
        let mut j = vec![0xff, 0xd8, 0xff, 0xc0, 0, 11, 8, 0, 1, 0, 1, 1, 1, 0x11, 0];
        j.extend(std::iter::repeat_n(0x55, n));
        j.extend([0xff, 0xd9]);
        j
    }
    #[test]
    fn stream_and_buffer_select_the_same_largest_preview_and_orientation() {
        for order in [ByteOrder::Little, ByteOrder::Big] {
            let mut root = IfdBuilder::new();
            root.set(t::ORIENTATION, Value::Short(vec![6]));
            for n in [30, 100, 60] {
                let mut sub = IfdBuilder::new();
                sub.set(t::COMPRESSION, Value::Short(vec![6]));
                sub.set(t::IMAGE_WIDTH, Value::Long(vec![1]));
                sub.set(t::IMAGE_LENGTH, Value::Long(vec![1]));
                sub.set_image(ImageData::Strips { rows_per_strip: 1, strips: vec![jpeg(n)] });
                root.add_sub_ifd(sub);
            }
            let bytes = TiffWriter { order, bigtiff: false }.write(&[root]).unwrap();
            let actual = embedded_preview_reader(&mut Cursor::new(&bytes)).unwrap();
            assert_eq!(actual.jpeg, crate::embedded_preview(&bytes).unwrap());
            assert_eq!(actual.orientation, Orientation::Rotate90);
            for end in 0..bytes.len() {
                let _ = embedded_preview_reader(&mut Cursor::new(&bytes[..end]));
            }
        }
    }
    #[test]
    fn child_next_pointers_do_not_add_candidates_ignored_by_the_buffer_parser() {
        let small = jpeg(30);
        let large = jpeg(100);
        let mut sub = IfdBuilder::new();
        sub.set(t::COMPRESSION, Value::Short(vec![6]));
        sub.set(t::IMAGE_WIDTH, Value::Long(vec![1]));
        sub.set(t::IMAGE_LENGTH, Value::Long(vec![1]));
        sub.set_image(ImageData::Strips { rows_per_strip: 1, strips: vec![small.clone()] });
        let mut root = IfdBuilder::new();
        root.add_sub_ifd(sub);
        let mut bytes = TiffWriter::default().write(&[root]).unwrap();
        let child = lightcraft_tiff::Tiff::parse(&bytes).unwrap().ifds[0].sub_ifds[0].offset as usize;
        let n = u16::from_le_bytes(bytes[child..child + 2].try_into().unwrap()) as usize;
        let at = bytes.len() as u32;
        bytes[child + 2 + n * 12..child + 6 + n * 12].copy_from_slice(&at.to_le_bytes());
        bytes.extend(2u16.to_le_bytes());
        for (tag, val) in [(t::JPEG_INTERCHANGE_FORMAT, at + 30), (t::JPEG_INTERCHANGE_FORMAT_LENGTH, large.len() as u32)] {
            bytes.extend(tag.to_le_bytes());
            bytes.extend(4u16.to_le_bytes());
            bytes.extend(1u32.to_le_bytes());
            bytes.extend(val.to_le_bytes());
        }
        bytes.extend(0u32.to_le_bytes());
        bytes.extend(large);
        assert_eq!(crate::embedded_preview(&bytes).unwrap(), small);
        assert_eq!(embedded_preview_reader(&mut Cursor::new(bytes)).unwrap().jpeg, small);
    }
    #[test]
    fn distinct_equal_size_previews_use_the_buffer_tie_order() {
        let mut root = IfdBuilder::new();
        for value in [0x33, 0x44] {
            let mut j = jpeg(100);
            j[20] = value;
            let mut sub = IfdBuilder::new();
            sub.set(t::COMPRESSION, Value::Short(vec![6]));
            sub.set(t::IMAGE_WIDTH, Value::Long(vec![1]));
            sub.set(t::IMAGE_LENGTH, Value::Long(vec![1]));
            sub.set_image(ImageData::Strips { rows_per_strip: 1, strips: vec![j] });
            root.add_sub_ifd(sub);
        }
        let bytes = TiffWriter::default().write(&[root]).unwrap();
        assert!(crate::embedded_preview(&bytes).is_some());
        assert!(embedded_preview_reader(&mut Cursor::new(bytes)).is_none());
    }
    #[test]
    fn cyclic_and_oversized_directories_fall_back_without_allocating_payloads() {
        let mut bytes = b"II\x2a\0\x08\0\0\0".to_vec();
        bytes.extend(1u16.to_le_bytes());
        bytes.extend(t::ORIENTATION.to_le_bytes());
        bytes.extend(3u16.to_le_bytes());
        bytes.extend(1u32.to_le_bytes());
        bytes.extend(1u32.to_le_bytes());
        bytes.extend(8u32.to_le_bytes());
        assert!(embedded_preview_reader(&mut Cursor::new(&bytes)).is_none());
        bytes[8..10].copy_from_slice(&u16::MAX.to_le_bytes());
        assert!(embedded_preview_reader(&mut Cursor::new(&bytes)).is_none());
        assert!(embedded_preview_reader(&mut Cursor::new(b"not TIFF")).is_none());
    }
    #[test]
    fn lossless_sensor_jpeg_payload_is_skipped_before_loading_the_display_preview() {
        let mut lossless = jpeg(16 << 20);
        lossless[3] = 0xc3;
        let display = jpeg(100);
        let mut root = IfdBuilder::new();
        root.set(t::COMPRESSION, Value::Short(vec![7]));
        root.set(t::IMAGE_WIDTH, Value::Long(vec![8]));
        root.set(t::IMAGE_LENGTH, Value::Long(vec![8]));
        root.set_image(ImageData::Strips { rows_per_strip: 8, strips: vec![lossless] });
        let mut sub = IfdBuilder::new();
        sub.set(t::COMPRESSION, Value::Short(vec![6]));
        sub.set(t::IMAGE_WIDTH, Value::Long(vec![1]));
        sub.set(t::IMAGE_LENGTH, Value::Long(vec![1]));
        sub.set_image(ImageData::Strips { rows_per_strip: 1, strips: vec![display.clone()] });
        root.add_sub_ifd(sub);
        let bytes = TiffWriter::default().write(&[root]).unwrap();
        assert_eq!(crate::embedded_preview(&bytes).unwrap(), display);
        let mut reader = CountReads { file: Cursor::new(bytes), read: 0 };
        assert_eq!(embedded_preview_reader(&mut reader).unwrap().jpeg, display);
        assert!(reader.read < 8192, "lossless sensor payload was read: {}", reader.read);
    }
    struct CountReads {
        file: Cursor<Vec<u8>>,
        read: usize,
    }
    impl Read for CountReads {
        fn read(&mut self, b: &mut [u8]) -> std::io::Result<usize> {
            let n = self.file.read(b)?;
            self.read += n;
            Ok(n)
        }
    }
    impl Seek for CountReads {
        fn seek(&mut self, p: SeekFrom) -> std::io::Result<u64> {
            self.file.seek(p)
        }
    }
    #[test]
    fn sensor_payload_is_not_read_even_when_the_jpeg_is_at_the_end() {
        let j = jpeg(100);
        let at = 16 << 20;
        let mut bytes = b"II\x2a\0\x08\0\0\0".to_vec();
        bytes.extend(2u16.to_le_bytes());
        for (tag, val) in [(t::JPEG_INTERCHANGE_FORMAT, at as u32), (t::JPEG_INTERCHANGE_FORMAT_LENGTH, j.len() as u32)] {
            bytes.extend(tag.to_le_bytes());
            bytes.extend(4u16.to_le_bytes());
            bytes.extend(1u32.to_le_bytes());
            bytes.extend(val.to_le_bytes());
        }
        bytes.extend(0u32.to_le_bytes());
        bytes.resize(at, 0);
        bytes.extend(&j);
        let mut reader = CountReads { file: Cursor::new(bytes), read: 0 };
        assert_eq!(embedded_preview_reader(&mut reader).unwrap().jpeg, j);
        assert!(reader.read < 1024, "sensor payload was read: {}", reader.read);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod library_scale_sony_tests {
    use super::*;
    use lightcraft_tiff::{IfdBuilder, ImageData, TiffWriter, Value};
    use std::io::Cursor;
    #[test]
    fn sony_preview_ifd_pointers_use_the_enclosing_tiff_offset_base() {
        for order in [ByteOrder::Little, ByteOrder::Big] {
            let jpeg = vec![0xff, 0xd8, 0xff, 0xc0, 0, 11, 8, 0, 1, 0, 1, 1, 1, 0x11, 0, 0xff, 0xd9];
            let mut note = b"SONY DSC\0\0\0\0".to_vec();
            order.put_u16(&mut note, 1);
            order.put_u16(&mut note, 0x0011);
            order.put_u16(&mut note, 4);
            order.put_u32(&mut note, 1);
            order.put_u32(&mut note, 0);
            order.put_u32(&mut note, 0);
            let mut exif = IfdBuilder::new();
            exif.set(t::MAKER_NOTE, Value::Undefined(note));
            let mut root = IfdBuilder::new();
            root.set(t::MAKE, Value::Ascii("SONY".into()));
            root.set(t::ORIENTATION, Value::Short(vec![6]));
            root.set_child(t::EXIF_IFD, exif);
            let mut bytes = TiffWriter { order, bigtiff: false }.write(&[root]).unwrap();
            let note_at = lightcraft_tiff::Tiff::parse(&bytes).unwrap().exif().unwrap().get(t::MAKER_NOTE).unwrap().offset as usize;
            let preview_at = bytes.len() as u32;
            let mut pointer = Vec::new();
            order.put_u32(&mut pointer, preview_at);
            bytes[note_at + 12 + 2 + 8..note_at + 12 + 2 + 12].copy_from_slice(&pointer);
            order.put_u16(&mut bytes, 2);
            for (tag, value) in [(t::JPEG_INTERCHANGE_FORMAT, preview_at + 30), (t::JPEG_INTERCHANGE_FORMAT_LENGTH, jpeg.len() as u32)] {
                order.put_u16(&mut bytes, tag);
                order.put_u16(&mut bytes, 4);
                order.put_u32(&mut bytes, 1);
                order.put_u32(&mut bytes, value);
            }
            order.put_u32(&mut bytes, 0);
            bytes.extend_from_slice(&jpeg);
            let actual = embedded_preview_reader(&mut Cursor::new(&bytes)).unwrap();
            assert_eq!(actual.jpeg, crate::embedded_preview(&bytes).unwrap());
            assert_eq!(actual.orientation, Orientation::Rotate90);
        }
    }
    #[test]
    fn sony_maker_note_and_xmp_orientation_match_the_buffer_path() {
        let j = vec![0xff, 0xd8, 0xff, 0xc0, 0, 11, 8, 0, 1, 0, 1, 1, 1, 0x11, 0, 0xff, 0xd9];
        let mut note = b"SONY DSC\0\0\0\0".to_vec();
        assert_eq!(note.len(), 12);
        note.extend(1u16.to_le_bytes());
        note.extend(0x0011u16.to_le_bytes());
        note.extend(7u16.to_le_bytes());
        note.extend(4u32.to_le_bytes());
        note.extend(0u32.to_le_bytes());
        note.extend(0u32.to_le_bytes());
        let mut exif = IfdBuilder::new();
        exif.set(t::MAKER_NOTE, Value::Undefined(note));
        let mut root = IfdBuilder::new();
        root.set(t::COMPRESSION, Value::Short(vec![6]));
        root.set(t::IMAGE_WIDTH, Value::Long(vec![1]));
        root.set(t::IMAGE_LENGTH, Value::Long(vec![1]));
        root.set_image(ImageData::Strips { rows_per_strip: 1, strips: vec![j] });
        root.set_child(t::EXIF_IFD, exif);
        let xmp = lightcraft_meta::write_xmp(&lightcraft_meta::Metadata { orientation: Some(Orientation::Rotate270), ..Default::default() }, None);
        root.set(t::XMP, Value::Byte(xmp.into_bytes()));
        let bytes = TiffWriter::default().write(&[root]).unwrap();
        let actual = embedded_preview_reader(&mut Cursor::new(&bytes)).unwrap();
        assert_eq!(actual.jpeg, crate::embedded_preview(&bytes).unwrap());
        assert_eq!(Some(actual.orientation), lightcraft_meta::extract(&bytes).orientation);
    }
}
