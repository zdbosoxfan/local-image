//! EXIF directories shared with the LightCraft TIFF metadata reader and writer.

use super::tiff_ifd::File;
use lightcraft_tiff::{IfdBuilder, ParseOptions, TiffWriter, Value};
use tiff::encoder::{DirectoryEncoder, TiffEncoder, TiffKind};
use tiff::tags::{Tag, Type};

const DESCRIPTIVE: [u16; 7] = [270, 271, 272, 305, 306, 315, 33432];
const POINTERS: [u16; 3] = [34665, 34853, 40965];

pub(crate) fn read(bytes: &[u8]) -> Option<IfdBuilder> {
    let bytes = bytes.strip_prefix(b"Exif\0\0").unwrap_or(bytes);
    let file = File::parse(bytes)?;
    if file.big {
        return None;
    }
    let root = file.ifd(file.first_ifd)?;
    let mut result = IfdBuilder::new();
    let mut remaining = 16usize << 20;
    for entry in &root.entries {
        if DESCRIPTIVE.contains(&entry.tag) && entry.ty == 2 {
            let value = file.data(entry)?;
            remaining = remaining.checked_sub(value.len())?;
            result.set(entry.tag, Value::AsciiBytes(value.to_vec()));
        }
    }
    for tag in [34665, 34853] {
        let Some(offset) = file.tag_uint(&root, tag).filter(|o| *o != root.at && *o != 0) else { continue };
        let options = ParseOptions { max_entries: 1024, max_ifds: 1, follow_children: false, value_budget: Some(remaining as u64), ..Default::default() };
        let Ok((directory, _)) = lightcraft_tiff::parse_ifd_at(bytes, offset, file.order(), 0, false, &options) else { continue };
        let mut child = IfdBuilder::new();
        for entry in directory.entries {
            remaining = remaining.checked_sub(entry.value.count().checked_mul(entry.value.field_type().size())?)?;
            if !POINTERS.contains(&entry.tag) {
                child.set(entry.tag, entry.value);
            }
        }
        if !child.entries.is_empty() {
            result.set_child(tag, child);
        }
    }
    Some(result)
}

pub(crate) fn block(dirs: &IfdBuilder) -> Option<Vec<u8>> {
    if dirs.children.is_empty() {
        return None;
    }
    TiffWriter::default().write(std::slice::from_ref(dirs)).ok()
}

pub(crate) fn write_directory<W: std::io::Write + std::io::Seek, K: TiffKind>(
    enc: &mut TiffEncoder<W, K>,
    fields: &IfdBuilder,
) -> tiff::TiffResult<K::OffsetType> {
    let mut directory = enc.extra_directory()?;
    write_fields(&mut directory, fields, |_| true)?;
    Ok(directory.finish_with_offsets()?.offset)
}

pub(crate) fn write_fields<W: std::io::Write + std::io::Seek, K: TiffKind>(
    directory: &mut DirectoryEncoder<'_, W, K>,
    fields: &IfdBuilder,
    keep: impl Fn(u16) -> bool,
) -> tiff::TiffResult<()> {
    let order = if cfg!(target_endian = "little") { lightcraft_tiff::ByteOrder::Little } else { lightcraft_tiff::ByteOrder::Big };
    let mut entries = tiff::Directory::empty();
    for (&tag, value) in &fields.entries {
        if !keep(tag) {
            continue;
        }
        let Some(kind) = Type::from_u16(value.field_type() as u16) else { continue };
        let bytes = lightcraft_tiff::writer::encode(order, value);
        entries.extend([(Tag::from_u16_exhaustive(tag), directory.write_entry_bytes(kind, &bytes)?)]);
    }
    directory.extend_from(&entries);
    Ok(())
}
