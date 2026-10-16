//! Bounded TIFF directory views without copying field payloads.
//!
//! This is the shared structural reader for pixel codecs. Unlike [`crate::Tiff`], it retains
//! entries in file order, leaves field data in the original buffer, and requires complete
//! directory tables including their next-IFD pointers. A malformed value is rejected when it
//! is accessed, so unrelated damaged metadata does not prevent image decoding.
//!
//! Callers traversing chains must apply a visited set and a total entry budget in addition to
//! the per-directory limit enforced here. [`File::entry_count`] permits charging that budget
//! before allocating entries. The codec's traversal limits remain its own policy.
//!
//! Layout follows TIFF 6.0 §2 and the BigTIFF format description, as does the decoded reader.

use crate::{ByteOrder, FieldType};

/// Classic TIFF's entry limit, also applied to BigTIFF directory views.
const MAX_ENTRIES: u64 = u16::MAX as u64;
/// Maximum child offsets materialised by [`File::sub_ifds`].
const MAX_SUBIFDS: u64 = 256;

fn type_size(ty: u16) -> Option<u64> {
    FieldType::from_u16(ty).map(|ty| ty.size() as u64)
}

/// One directory entry, located but not read.
#[derive(Clone, Copy, Debug)]
pub struct Entry {
    pub tag: u16,
    pub ty: u16,
    pub count: u64,
    /// Position of the value: inside the entry when it fits, else the stored offset.
    pub value_at: u64,
}

/// One directory: its entries in file order and the next-IFD offset (0 at the end of a chain).
#[derive(Clone, Debug)]
pub struct Ifd {
    pub at: u64,
    pub entries: Vec<Entry>,
    pub next: u64,
    /// Position of the next-IFD pointer field.
    pub next_at: u64,
}

impl Ifd {
    /// The first entry with this tag.
    pub fn get(&self, tag: u16) -> Option<&Entry> {
        self.entries.iter().find(|e| e.tag == tag)
    }
}

/// A zero-allocation directory iterator, including unknown field types.
///
/// Unknown entries retain the position of their value/offset field in `value_at`; their
/// payload cannot be accessed through [`File::data`]. This lets tag-specific policies honor
/// the first duplicate even if its type is invalid.
pub struct Entries<'a> {
    file: File<'a>,
    first: u64,
    index: u64,
    count: u64,
    /// Directory offset.
    pub at: u64,
    /// Next directory offset, or zero at the end of the chain.
    pub next: u64,
    /// Position of the next-directory pointer.
    pub next_at: u64,
}

impl Iterator for Entries<'_> {
    type Item = Entry;

    fn next(&mut self) -> Option<Entry> {
        if self.index >= self.count {
            return None;
        }
        let stride = if self.file.big { 20 } else { 12 };
        let at = self.first.checked_add(self.index.checked_mul(stride)?)?;
        self.index += 1;
        self.file.entry_at(at)
    }
}

/// A TIFF or BigTIFF file's header and bytes.
#[derive(Clone, Copy)]
pub struct File<'a> {
    /// The original TIFF buffer; field values are borrowed from it.
    pub b: &'a [u8],
    /// Little-endian byte order.
    pub le: bool,
    /// BigTIFF uses 8-byte offsets and 20-byte directory entries.
    pub big: bool,
    /// Offset stored in the header (not validated until the directory is read).
    pub first_ifd: u64,
}

impl<'a> File<'a> {
    /// The header, or `None` when this is not a (Big)TIFF.
    pub fn parse(b: &'a [u8]) -> Option<Self> {
        let (order, big, first_ifd) = crate::reader::strict_header(b).ok()?;
        Some(Self { b, le: order == ByteOrder::Little, big, first_ifd })
    }

    /// Byte order shared with the decoded-value reader and writer.
    pub fn order(&self) -> ByteOrder {
        if self.le { ByteOrder::Little } else { ByteOrder::Big }
    }

    /// Length of the header; no directory can start inside it.
    pub fn header_len(&self) -> u64 {
        if self.big { 16 } else { 8 }
    }

    fn slice(&self, at: u64, len: u64) -> Option<&'a [u8]> {
        let at = usize::try_from(at).ok()?;
        let len = usize::try_from(len).ok()?;
        self.b.get(at..at.checked_add(len)?)
    }

    pub fn u16_at(&self, at: u64) -> Option<u16> {
        self.order().read_u16(self.b, usize::try_from(at).ok()?)
    }

    pub fn u32_at(&self, at: u64) -> Option<u32> {
        self.order().read_u32(self.b, usize::try_from(at).ok()?)
    }

    pub fn u64_at(&self, at: u64) -> Option<u64> {
        self.order().read_u64(self.b, usize::try_from(at).ok()?)
    }

    /// The declared entry count of the directory at `at`, before its table is parsed.
    pub fn entry_count(&self, at: u64) -> Option<u64> {
        if at < self.header_len() {
            return None;
        }
        if self.big { self.u64_at(at) } else { self.u16_at(at).map(u64::from) }
    }

    /// Entries of a complete directory, without allocating entries or value payloads.
    /// Unknown field types are retained; [`Self::ifd`] skips them for decoded use.
    pub fn entries_at(&self, at: u64) -> Option<Entries<'a>> {
        if at < self.header_len() {
            return None;
        }
        let count = self.entry_count(at)?;
        if count > MAX_ENTRIES {
            return None;
        }
        let (head, stride) = if self.big { (8, 20) } else { (2, 12) };
        let first = at.checked_add(head)?;
        let next_at = first.checked_add(count.checked_mul(stride)?)?;
        let next = if self.big { self.u64_at(next_at)? } else { u64::from(self.u32_at(next_at)?) };
        Some(Entries { file: *self, first, index: 0, count, at, next, next_at })
    }

    fn entry_at(&self, at: u64) -> Option<Entry> {
        let tag = self.u16_at(at)?;
        let ty = self.u16_at(at.checked_add(2)?)?;
        let count = if self.big { self.u64_at(at.checked_add(4)?)? } else { u64::from(self.u32_at(at.checked_add(4)?)?) };
        let field = at.checked_add(if self.big { 12 } else { 8 })?;
        let inline = if self.big { 8 } else { 4 };
        let external = type_size(ty).is_some_and(|size| !count.checked_mul(size).is_some_and(|len| len <= inline));
        let value_at = if !external {
            field
        } else if self.big {
            self.u64_at(field)?
        } else {
            u64::from(self.u32_at(field)?)
        };
        Some(Entry { tag, ty, count, value_at })
    }

    /// The directory at `at`: its whole table, up to and including the next-IFD pointer, must
    /// be inside the file and after the header. Unknown field types are skipped.
    pub fn ifd(&self, at: u64) -> Option<Ifd> {
        let raw = self.entries_at(at)?;
        let (next, next_at) = (raw.next, raw.next_at);
        let mut entries = Vec::with_capacity(usize::try_from(raw.count).ok()?);
        entries.extend(raw.filter(|entry| type_size(entry.ty).is_some()));
        Some(Ifd { at, entries, next, next_at })
    }

    /// The value bytes of an entry, when they are all inside the file.
    pub fn data(&self, e: &Entry) -> Option<&'a [u8]> {
        self.slice(e.value_at, e.count.checked_mul(type_size(e.ty)?)?)
    }

    /// Value `i` of an unsigned integer entry (BYTE, SHORT, LONG, LONG8, IFD, IFD8).
    pub fn uint(&self, e: &Entry, i: u64) -> Option<u64> {
        if i >= e.count {
            return None;
        }
        let size = type_size(e.ty)?;
        let at = e.value_at.checked_add(i.checked_mul(size)?)?;
        match e.ty {
            1 => self.slice(at, 1)?.first().map(|&v| u64::from(v)),
            3 => self.u16_at(at).map(u64::from),
            4 | 13 => self.u32_at(at).map(u64::from),
            16 | 18 => self.u64_at(at),
            _ => None,
        }
    }

    /// The first value of an unsigned integer tag.
    pub fn tag_uint(&self, ifd: &Ifd, tag: u16) -> Option<u64> {
        self.uint(ifd.get(tag)?, 0)
    }

    /// Up to `max` values of an unsigned integer tag (`None` if any of them is unreadable).
    pub fn tag_uints(&self, ifd: &Ifd, tag: u16, max: u64) -> Option<Vec<u64>> {
        let e = ifd.get(tag)?;
        (0..e.count.min(max)).map(|i| self.uint(e, i)).collect()
    }

    /// The first value of a numeric tag as a float: RATIONAL, FLOAT, DOUBLE or an integer.
    pub fn tag_f64(&self, ifd: &Ifd, tag: u16) -> Option<f64> {
        let e = ifd.get(tag)?;
        if e.count == 0 {
            return None;
        }
        match e.ty {
            5 => {
                let (n, d) = (self.u32_at(e.value_at)?, self.u32_at(e.value_at.checked_add(4)?)?);
                (d != 0).then(|| f64::from(n) / f64::from(d))
            }
            11 => Some(f64::from(f32::from_bits(self.u32_at(e.value_at)?))),
            12 => Some(f64::from_bits(self.u64_at(e.value_at)?)),
            _ => self.uint(e, 0).map(|v| v as f64),
        }
    }

    /// The bytes of a one-byte-per-value tag (BYTE, ASCII, SBYTE or UNDEFINED).
    pub fn tag_bytes(&self, ifd: &Ifd, tag: u16) -> Option<&'a [u8]> {
        let e = ifd.get(tag)?;
        matches!(e.ty, 1 | 2 | 6 | 7).then(|| self.data(e)).flatten()
    }

    /// Recognize camera raws before a flat-image codec decodes a preview. This combines
    /// DNG/CFA/LinearRaw, vendor compression, Sony linear YCbCr and CR2 signatures.
    /// Visits at most sixteen directories (main chain and eight SubIFDs per directory),
    /// with a file-size entry budget and cycle detection; payloads stay borrowed.
    pub fn is_camera_raw(&self) -> bool {
        if self.b.get(8..10) == Some(b"CR") {
            return true;
        }
        let sony = self
            .entries_at(self.first_ifd)
            .and_then(|mut entries| entries.find(|entry| entry.tag == crate::tags::MAKE))
            .and_then(|entry| self.data(&entry))
            .and_then(|bytes| bytes.get(..4))
            .is_some_and(|bytes| bytes.eq_ignore_ascii_case(b"SONY"));
        let mut pending = vec![self.first_ifd];
        let mut seen = Vec::new();
        let mut budget = self.b.len() as u64 / 12 + 64;
        while let Some(at) = pending.pop() {
            if at == 0 || seen.contains(&at) {
                continue;
            }
            if seen.len() >= 16 {
                break;
            }
            seen.push(at);
            let Some(count) = self.entry_count(at) else { continue };
            let Some(remaining) = budget.checked_sub(count) else { break };
            budget = remaining;
            let Some(entries) = self.entries_at(at) else { continue };
            let next = entries.next;
            let (mut compression, mut photometric, mut samples, mut cfa) = (None, None, None, false);
            for entry in entries {
                match entry.tag {
                    crate::tags::DNG_VERSION => return true,
                    crate::tags::COMPRESSION if compression.is_none() => compression = self.uint(&entry, 0),
                    crate::tags::PHOTOMETRIC if photometric.is_none() => photometric = self.uint(&entry, 0),
                    crate::tags::SAMPLES_PER_PIXEL if samples.is_none() => samples = self.uint(&entry, 0),
                    crate::tags::CFA_PATTERN_EP => cfa = true,
                    crate::tags::SUB_IFDS => pending.extend((0..entry.count.min(8)).filter_map(|i| self.uint(&entry, i))),
                    _ => {}
                }
            }
            if matches!(photometric, Some(32803 | 34892))
                || matches!(compression, Some(32767 | 32769 | 32770 | 34316 | 34713 | 65000 | 65535))
                || (sony && cfa && compression == Some(7) && photometric == Some(6) && samples == Some(3))
            {
                return true;
            }
            if next != 0 {
                pending.push(next);
            }
        }
        false
    }

    /// SubIFD offsets of a directory (tag 330 as LONG, IFD, LONG8 or IFD8).
    pub fn sub_ifds(&self, ifd: &Ifd) -> Vec<u64> {
        let Some(e) = ifd.get(crate::tags::SUB_IFDS) else { return Vec::new() };
        if !matches!(e.ty, 4 | 13 | 16 | 18) {
            return Vec::new();
        }
        (0..e.count.min(MAX_SUBIFDS)).map_while(|i| self.uint(e, i)).filter(|&o| o != 0).collect()
    }
}
