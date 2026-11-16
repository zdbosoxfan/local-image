//! XMP sidecars: `<stem>.xmp` (default) or `<file>.xmp` next to the original.
//!
//! **Write** ([`Session::save_sidecar`], `photo.saveMetadataToFile`, or automatically after every
//! change when [`XmpPrefs::auto_write`] is on): rating, colour label, title, caption, copyright,
//! creator, keywords (names in `dc:subject`, paths in `lr:hierarchicalSubject`), capture time and GPS
//! in the standard namespaces, plus our complete develop settings (`lc:settings`, JSON), the
//! pick/reject flag (`lc:flag`) and location (`lc:location`).
//!
//! An existing sidecar is never replaced wholesale (issue #92): LightCraft's properties
//! ([`OWNED`], [`OWNED_IF_STATED`]) are merged into it ([`lightcraft_meta::merge_xmp`]) and
//! everything else — another editor's `crs:` develop settings, `xmpMM` history, unknown
//! namespaces — is kept byte for byte. A sidecar that can't be read as XMP is first copied to
//! `<name>.xmp.bak-<time>` (never overwritten), then replaced. Writes are atomic (temp file,
//! fsync, rename).
//!
//! **Shared names** (Stem naming): `IMG_0001.CR3` and `IMG_0001.JPG` would share `IMG_0001.xmp`.
//! When two catalogued files map to one stem sidecar, it belongs to one of them (a raw first, then
//! the first by file name — [`Session::sidecar_naming`]); the others use `<file>.xmp`
//! (`IMG_0001.JPG.xmp`) for writing and reading, so neither overwrites the other.
//!
//! **Read** (on import, and `photo.readMetadataFromFile`): the sidecar wins for metadata, except
//! the capture time: a time embedded in the file wins, and the sidecar's (`exif:DateTimeOriginal`,
//! else `photoshop:DateCreated`, else `xmp:CreateDate`) fills in only when the file has none; develop
//! settings are restored from `lc:settings` when present, else mapped from interoperable `crs:`
//! fields ([`crate::crs`]). For raw/DNG files without a sidecar the file's embedded XMP is used.

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use lightcraft_catalog::{ColorLabel, Flag, MediaKind, Op, Photo, PhotoId, Source};
use lightcraft_develop::DevelopSettings;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{EngineError, Result, Session};

/// Sidecar file naming.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SidecarNaming {
    /// `IMG_0001.xmp` (the common convention; shared by `IMG_0001.CR3` and `IMG_0001.JPG`).
    #[default]
    Stem,
    /// `IMG_0001.CR3.xmp` (unambiguous when raw+JPEG pairs live side by side).
    Full,
}

/// Library preferences for XMP sidecars.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct XmpPrefs {
    /// Write the sidecar after every change to a photo's metadata or develop settings.
    pub auto_write: bool,
    pub naming: SidecarNaming,
}

/// The sidecar path for an original, by naming convention.
pub fn sidecar_path(original: &str, naming: SidecarNaming) -> PathBuf {
    match naming {
        SidecarNaming::Stem => Path::new(original).with_extension("xmp"),
        SidecarNaming::Full => PathBuf::from(format!("{original}.xmp")),
    }
}

/// Where to look for a catalogued photo's sidecar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SidecarLookup {
    pub naming: SidecarNaming,
    /// Whether a shared stem sidecar (`IMG_0001.xmp`) may be read for this photo. False when
    /// another catalogued file with the same stem owns it (see [`Session::sidecar_naming`]).
    pub stem_ok: bool,
}

/// An existing sidecar for `original`: the preferred naming first, then the other one (also
/// accepting an upper-case `.XMP`).
pub fn find_sidecar(original: &str, naming: SidecarNaming) -> Option<PathBuf> {
    find_sidecar_in(original, SidecarLookup { naming, stem_ok: true })
}

/// [`find_sidecar`] for a catalogued photo: never a stem sidecar that a sibling owns.
pub fn find_sidecar_in(original: &str, lookup: SidecarLookup) -> Option<PathBuf> {
    let other = if lookup.naming == SidecarNaming::Stem { SidecarNaming::Full } else { SidecarNaming::Stem };
    [lookup.naming, other]
        .into_iter()
        .filter(|n| *n != SidecarNaming::Stem || lookup.stem_ok)
        .flat_map(|n| {
            let p = sidecar_path(original, n);
            let upper = p.with_extension("XMP");
            [p, upper]
        })
        .find(|p| p.is_file())
}

/// What a sidecar (or embedded XMP packet) says about a photo. `None` = not stated.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SidecarData {
    pub rating: Option<u8>,
    pub flag: Option<Flag>,
    pub label: Option<Option<ColorLabel>>,
    /// The label's text as written (`xmp:Label`), for [`SidecarData::resolve_label`].
    pub label_text: Option<String>,
    pub title: Option<String>,
    pub caption: Option<String>,
    pub copyright: Option<String>,
    /// `xmpRights:Marked`: `Some(true)` copyrighted, `Some(false)` public domain.
    pub copyright_marked: Option<bool>,
    pub usage_terms: Option<String>,
    pub copyright_url: Option<String>,
    pub creator: Option<String>,
    /// Structured source sequence, stated whenever the packet states dc:creator.
    pub creators: Option<Vec<String>>,
    pub location: Option<String>,
    pub city: Option<String>,
    pub state: Option<String>,
    pub country: Option<String>,
    pub alt_text: Option<String>,
    pub extended_description: Option<String>,
    pub keywords: Option<Vec<String>>,
    pub regions: Option<Vec<lightcraft_meta::Region>>,
    /// The size the regions were drawn on, when they don't say how the photo was turned
    /// ([`lightcraft_meta::XmpData::regions_drawn_on`]), and how the photo's stored pixels turn upright: the
    /// sidecar's `tiff:Orientation`, or the file's own Exif orientation where the caller knows it. With both,
    /// [`merge_into`] puts boxes drawn on the photo as stored where the upright photo has them.
    pub regions_drawn_on: Option<(f64, f64)>,
    pub orientation: Option<lightcraft_meta::Orientation>,
    /// Capture time (ISO 8601) from `exif:DateTimeOriginal`, `photoshop:DateCreated` or
    /// `xmp:CreateDate` (first found). Used only when the file itself has no capture time.
    pub captured: Option<String>,
    pub develop: Option<DevelopPatch>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum DevelopPatch {
    /// Our own complete settings (`lc:settings`).
    Full(Box<DevelopSettings>),
    /// Mapped `crs:` fields (partial JSON, merged like a preset).
    Partial(Value),
}

/// Parse a sidecar / XMP packet. `raw` selects absolute (raw) vs relative white balance for `crs:`.
pub fn parse_sidecar(xmp: &str, raw: bool) -> std::result::Result<SidecarData, String> {
    // Text with no markup at all (empty, or a stray text file) is not an XMP packet. Anything that
    // *is* markup stays lenient: a packet with no recognised properties is a valid, empty sidecar.
    if !xmp.contains('<') {
        return Err("not an XMP packet (no XML markup)".to_string());
    }
    let d = lightcraft_meta::parse_whole_xmp(xmp).map_err(|e| e.to_string())?;
    let m = &d.metadata;
    let creator_stated = d.values.contains_key("dc:creator") || d.properties.contains_key("dc:creator");
    let lc = |k: &str| d.properties.get(&format!("lc:{k}")).and_then(|v| v.first()).cloned();
    let mut out = SidecarData {
        title: m.title.clone(),
        caption: m.caption.clone(),
        copyright: m.copyright.clone(),
        copyright_marked: m.copyright_marked,
        usage_terms: m.usage_terms.clone(),
        copyright_url: m.copyright_url.clone(),
        creator: creator_stated.then(|| m.artist.clone().unwrap_or_default()),
        creators: creator_stated.then(|| m.creators.clone()),
        location: lc("location").or_else(|| m.sublocation.clone()),
        city: m.city.clone(),
        state: m.state.clone(),
        country: m.country.clone(),
        alt_text: m.alt_text.clone(),
        extended_description: m.extended_description.clone(),
        keywords: (!m.keywords.is_empty() || !m.hierarchical_keywords.is_empty())
            .then(|| lightcraft_catalog::keywords::from_file(&m.keywords, &m.hierarchical_keywords)),
        // A sidecar that has `mwg-rs:Regions` at all (even an empty list) was written by an app that
        // knows about regions, so it's authoritative: its list, empty or not, replaces the catalog's.
        // One without it (most writers, LightCraft's own included, which keeps another app's
        // `mwg-rs:Regions` byte for byte but never writes one) says nothing about regions, and the
        // photo's are kept.
        regions: d.values.contains_key("mwg-rs:Regions").then(|| m.regions.clone()),
        regions_drawn_on: d.regions_drawn_on(),
        // (only what the regions need: a packet that says nothing else still says nothing)
        orientation: d.regions_drawn_on().and(m.orientation),
        captured: m.capture_time.map(|d| d.to_iso()),
        ..Default::default()
    };
    if let Some(r) = m.rating {
        if r < 0 {
            out.flag = Some(Flag::Reject);
            out.rating = Some(0);
        } else {
            out.rating = Some(r.clamp(0, 5) as u8);
        }
    }
    if let Some(f) = lc("flag").and_then(|f| Flag::parse(&f)) {
        out.flag = Some(f);
    }
    if let Some(l) = &m.label {
        out.label = Some(ColorLabel::parse(l.trim()));
        out.label_text = Some(l.trim().to_string());
    }
    let full =
        d.lc_settings.as_deref().and_then(|j| serde_json::from_str::<Value>(j).ok()).and_then(|v| DevelopSettings::from_untrusted_json(&v).ok());
    out.develop = match full {
        Some(s) => Some(DevelopPatch::Full(Box::new(s))),
        None if crate::crs::has_adjustments(&d.properties) => {
            Some(DevelopPatch::Partial(crate::crs::to_partial_report(&d.properties, Some(&d.values), Some(raw), crate::crs_masks::DEFAULT_ASPECT).0))
        }
        None => None,
    };
    Ok(out)
}

/// Apply sidecar data to a photo record (the sidecar wins, except that a capture time embedded in
/// the file is kept). Returns true if develop changed.
pub fn merge_into(p: &mut Photo, sc: &SidecarData, now: &str) -> bool {
    if p.captured.is_none() {
        p.captured = sc.captured.clone();
    }
    if let Some(r) = sc.rating {
        p.rating = r;
    }
    if let Some(f) = sc.flag {
        p.flag = f;
    }
    if let Some(l) = sc.label {
        p.label = l;
    }
    let m = &mut p.meta;
    for (src, dst) in [
        (&sc.title, &mut m.title),
        (&sc.caption, &mut m.caption),
        (&sc.copyright, &mut m.copyright),
        (&sc.usage_terms, &mut m.usage_terms),
        (&sc.copyright_url, &mut m.copyright_url),
        (&sc.creator, &mut m.creator),
        (&sc.location, &mut m.location),
        (&sc.city, &mut m.city),
        (&sc.state, &mut m.state),
        (&sc.country, &mut m.country),
        (&sc.alt_text, &mut m.alt_text),
        (&sc.extended_description, &mut m.extended_description),
    ] {
        if let Some(v) = src {
            *dst = v.clone();
        }
    }
    if sc.creator.is_some() {
        m.creators = sc.creators.clone().unwrap_or_default();
    }
    if let Some(marked) = sc.copyright_marked {
        m.copyright_status = lightcraft_catalog::CopyrightStatus::from_marked(Some(marked));
    }
    if let Some(k) = &sc.keywords {
        m.keywords = k.clone();
    }
    if let Some(r) = &sc.regions {
        m.regions = r.clone();
        // (the photo's size is the upright one)
        if let (Some(drawn_on), Some(o)) = (sc.regions_drawn_on, sc.orientation) {
            let stored = if o.swaps_axes() { (p.height, p.width) } else { (p.width, p.height) };
            lightcraft_meta::upright_regions(&mut m.regions, drawn_on, o, stored);
        }
    }
    let develop = match &sc.develop {
        Some(DevelopPatch::Full(s)) => (**s).clone(),
        Some(DevelopPatch::Partial(v)) => {
            let mut v = v.clone();
            if p.width > 0 && p.height > 0 {
                crate::crs_masks::refit_radials(&mut v, crate::crs_masks::DEFAULT_ASPECT, p.width as f64 / p.height as f64);
            }
            lightcraft_develop::apply_partial(&p.develop, &v, 1.0)
        }
        None => return false,
    };
    if develop == *p.develop {
        return false;
    }
    p.develop = Arc::new(develop);
    p.edited = Some(now.to_string());
    true
}

impl SidecarData {
    /// Read the label through the catalog's label names: other editors write the label's name
    /// (a custom set's "To Do" as well as "Red"); unknown names clear the label.
    pub fn resolve_label(mut self, cat: &lightcraft_catalog::Catalog) -> Self {
        if let Some(t) = &self.label_text {
            self.label = Some(if t.is_empty() { None } else { cat.label_from_name(t) });
        }
        self
    }

    /// Take the orientation of the photo file at `original` (the photo is shown turned by it, and the packet may
    /// not repeat it) when the regions need it: they were drawn on a stated size ([`SidecarData::regions_drawn_on`]).
    /// Only then is the file read ([`file_orientation`]).
    pub fn with_file_orientation(self, original: &str) -> Self {
        if self.regions_drawn_on.is_none() {
            return self;
        }
        SidecarData { orientation: file_orientation(original).or(self.orientation), ..self }
    }
}

/// The orientation the photo file at `path` states, as [`lightcraft_meta::extract`] reads it from the whole file (the
/// import's): Exif's, else XMP's. A file shorter than [`ORIENTATION_SPAN`] is read whole. Most others state it in the
/// Exif of their first [`ORIENTATION_SPAN`]; when that part doesn't, the Exif may come later ([`late_orientation`]).
/// A WebP's or PNG's chunks are always walked, since the last Exif chunk is the one that counts ([`webp_orientation`],
/// [`png_orientation`]).
fn file_orientation(path: &str) -> Option<lightcraft_meta::Orientation> {
    let mut file = li_fs::read::open_regular(Path::new(path), li_fs::read::MAX_FILE_BYTES).ok()?;
    let mut head = Vec::new();
    (&mut file).take(ORIENTATION_SPAN).read_to_end(&mut head).ok()?;
    if (head.len() as u64) < ORIENTATION_SPAN {
        return lightcraft_meta::extract(&head).orientation;
    }
    if head.get(..4) == Some(&b"RIFF"[..]) && head.get(8..12) == Some(&b"WEBP"[..]) {
        return webp_orientation(&mut file);
    }
    if head.starts_with(b"\x89PNG\r\n\x1a\n") {
        return png_orientation(&mut file);
    }
    if let Some(o) = exif_orientation(&head) {
        return Some(o);
    }
    late_orientation(&mut file, &head).or_else(|| lightcraft_meta::extract(&head).orientation)
}

/// The orientation the Exif in `bytes` (a file, or the start of one) states, leaving out a separate XMP packet (the
/// one the Exif holds in tag 700 counts, as in [`lightcraft_meta::read_exif`]).
fn exif_orientation(bytes: &[u8]) -> Option<lightcraft_meta::Orientation> {
    lightcraft_meta::embedded(bytes).exif.as_deref().map(lightcraft_meta::read_exif).and_then(|m| m.orientation)
}

/// The orientation of a JPEG, TIFF or PSD whose `head` has no Exif stating it, found by the file's structure without
/// reading its image data (a JPEG's Exif can follow other long segments, many TIFF writers put the first IFD after the
/// strips, and a PSD's image resources can outgrow `head`).
fn late_orientation(file: &mut (impl Read + Seek), head: &[u8]) -> Option<lightcraft_meta::Orientation> {
    let len = file.seek(SeekFrom::End(0)).ok()?;
    file.rewind().ok()?;
    match head.get(..4)? {
        [0xff, 0xd8, ..] => jpeg_orientation(file),
        b"II*\0" | b"MM\0*" => tiff_orientation(file, 0, len),
        b"8BPS" => psd_orientation(file),
        _ => None,
    }
}

/// A JPEG's orientation as the import takes it: from its first Exif segment, else its first XMP segment
/// ([`jpeg_app1`]).
fn jpeg_orientation(file: &mut (impl Read + Seek)) -> Option<lightcraft_meta::Orientation> {
    let (mut exif_seen, mut xmp) = (false, None);
    let exif = jpeg_app1(file, |segment| {
        let embedded = lightcraft_meta::embedded(segment);
        if let Some(exif) = embedded.exif
            && !exif_seen
        {
            // the first Exif segment ends the walk if it states one; later ones don't count
            exif_seen = true;
            return lightcraft_meta::read_exif(&exif).orientation;
        }
        if xmp.is_none() {
            xmp = embedded.xmp.map(|x| lightcraft_meta::parse_whole_xmp(&x).ok().and_then(|x| x.metadata.orientation));
        }
        None
    });
    exif.or(xmp.flatten())
}

/// The first APP1 segment (in a JPEG of its own) that `pick` finds something in, found by walking the segment headers
/// from the start of `file` up to the image data: at most [`JPEG_HEADER_SPAN`] bytes, skipped payloads included, and
/// [`JPEG_MARKER_STEPS`] markers. Only APP1 payloads are read (under 64 KiB each).
fn jpeg_app1<T>(file: &mut (impl Read + Seek), mut pick: impl FnMut(&[u8]) -> Option<T>) -> Option<T> {
    let mut header = file.take(JPEG_HEADER_SPAN);
    let mut soi = [0; 2];
    header.read_exact(&mut soi).ok()?;
    if soi != [0xff, 0xd8] {
        return None;
    }
    let mut filled = false;
    for _ in 0..JPEG_MARKER_STEPS {
        if !filled {
            let mut prefix = [0];
            header.read_exact(&mut prefix).ok()?;
            if prefix != [0xff] {
                return None;
            }
        }
        let mut code = [0];
        header.read_exact(&mut code).ok()?;
        let [marker] = code;
        filled = marker == 0xff;
        match marker {
            0xff | 0xd8 | 0xd0..=0xd7 | 0x01 => continue,
            0xd9 | 0xda => return None,
            _ => {}
        }
        let mut length = [0; 2];
        header.read_exact(&mut length).ok()?;
        let payload_len = u16::from_be_bytes(length).checked_sub(2)?;
        if marker != 0xe1 {
            // seeking spends the same span budget as reading, before the seek is attempted
            let remaining = header.limit().checked_sub(u64::from(payload_len))?;
            header.set_limit(remaining);
            header.get_mut().seek(SeekFrom::Current(i64::from(payload_len))).ok()?;
            continue;
        }
        // the segment in a JPEG of its own, as the metadata reader takes it
        let [hi, lo] = length;
        let mut segment = vec![0xff, 0xd8, 0xff, 0xe1, hi, lo];
        segment.resize(6 + usize::from(payload_len), 0);
        header.read_exact(segment.get_mut(6..)?).ok()?;
        if let Some(found) = pick(&segment) {
            return Some(found);
        }
    }
    None
}

const JPEG_HEADER_SPAN: u64 = 16 * 1024 * 1024;
const JPEG_MARKER_STEPS: usize = 4096;

/// A WebP's orientation as the import takes it: from its last `EXIF` chunk, else its last `XMP ` chunk, in whichever
/// order they come. The chunk headers are walked from the start of `file` (at most [`WEBP_CHUNK_STEPS`], ending at a
/// chunk that runs past the end of the file); then only the first IFD of the EXIF chunk is read, whatever its size
/// ([`tiff_orientation`]), and the XMP chunk if it is at most [`ORIENTATION_SPAN`].
fn webp_orientation(file: &mut (impl Read + Seek)) -> Option<lightcraft_meta::Orientation> {
    let len = file.seek(SeekFrom::End(0)).ok()?;
    let (mut exif, mut xmp) = (None, None);
    let mut at = 12u64;
    for _ in 0..WEBP_CHUNK_STEPS {
        let mut header = [0; 8];
        if file.seek(SeekFrom::Start(at)).is_err() || file.read_exact(&mut header).is_err() {
            break;
        }
        let [a, b, c, d, s0, s1, s2, s3] = header;
        let size = u64::from(u32::from_le_bytes([s0, s1, s2, s3]));
        let Some(end) = at.checked_add(8 + size).filter(|&end| end <= len) else { break };
        match &[a, b, c, d] {
            b"EXIF" => exif = Some((at + 8, end)),
            b"XMP " => xmp = Some((at + 8, size)),
            _ => {}
        }
        // chunks are padded to an even size
        at = end + (size & 1);
    }
    if let Some(o) = exif.and_then(|(start, end)| exif_chunk_orientation(file, start, end)) {
        return Some(o);
    }
    let (start, size) = xmp?;
    xmp_orientation(file, start, size)
}

/// How many chunks [`webp_orientation`] passes over (an animated WebP has one per frame).
const WEBP_CHUNK_STEPS: usize = 4096;

/// The orientation the Exif chunk from `start` to `end` in `file` states: its TIFF stream comes after an `Exif\0\0` if
/// the chunk starts with one (as `strip_exif_header` takes it).
fn exif_chunk_orientation(file: &mut (impl Read + Seek), start: u64, end: u64) -> Option<lightcraft_meta::Orientation> {
    let mut prefix = [0; 5];
    file.seek(SeekFrom::Start(start)).ok()?;
    let prefixed = start + 6 <= end && file.read_exact(&mut prefix).is_ok() && &prefix == b"Exif\0";
    tiff_orientation(file, if prefixed { start + 6 } else { start }, end)
}

/// A PNG's orientation as the import takes it: from its last `eXIf` chunk, else its last uncompressed XMP `iTXt` chunk,
/// wherever they come (a writer may put them after the image data). The chunk headers are walked from the start of
/// `file` to `IEND` (at most [`png_chunk_steps`], ending at a chunk that runs past the end of the file); then only the
/// first IFD of the `eXIf` chunk is read, whatever its size ([`tiff_orientation`]), and the `iTXt` chunk if it is at
/// most [`ORIENTATION_SPAN`].
fn png_orientation(file: &mut (impl Read + Seek)) -> Option<lightcraft_meta::Orientation> {
    let len = file.seek(SeekFrom::End(0)).ok()?;
    let (mut exif, mut xmp) = (None, None);
    let mut at = 8u64;
    for _ in 0..png_chunk_steps(len) {
        let (mut header, mut key) = ([0; 8], [0; 19]);
        if at + 12 > len || file.seek(SeekFrom::Start(at)).is_err() || file.read_exact(&mut header).is_err() {
            break;
        }
        let [s0, s1, s2, s3, a, b, c, d] = header;
        let size = u64::from(u32::from_be_bytes([s0, s1, s2, s3]));
        let Some(end) = at.checked_add(8 + size).filter(|&end| end <= len) else { break };
        match &[a, b, c, d] {
            b"eXIf" => exif = Some((at + 8, end)),
            // the keyword, then compression flag 0
            b"iTXt" if size >= 20 && file.read_exact(&mut key).is_ok() && &key == b"XML:com.adobe.xmp\0\0" => xmp = Some((at + 8, size)),
            b"IEND" => break,
            _ => {}
        }
        // past the chunk's CRC
        at = end + 4;
    }
    if let Some(o) = exif.and_then(|(start, end)| exif_chunk_orientation(file, start, end)) {
        return Some(o);
    }
    let (start, size) = xmp?;
    let chunk = read_within_span(file, start, size)?;
    // keyword\0 compression-flag compression-method language\0 translated\0 text
    let text = chunk.get(20..)?.splitn(3, |&c| c == 0).nth(2)?;
    lightcraft_meta::parse_whole_xmp(&String::from_utf8_lossy(text)).ok()?.metadata.orientation
}

/// How many chunks [`png_orientation`] passes over in a file of `len` bytes: 4096 and one per KiB of the file, at most
/// 2^20. Image data is often split into many `IDAT` chunks (libpng writes 8 KiB ones by default), so the limit grows
/// with the file; each chunk costs two system calls, so a short file of empty chunks can't hold a worker for seconds.
fn png_chunk_steps(len: u64) -> u64 {
    (4096 + len / 1024).min(1 << 20)
}

/// A PSD's or PSB's orientation as the import takes it, from its image resources: the file's start up to their end is
/// read if it is at most [`PSD_RESOURCES_SPAN`]. They come before the layers and the image data, after the 26-byte
/// header and the colour-mode data; each of those two sections starts with its length.
fn psd_orientation(file: &mut (impl Read + Seek)) -> Option<lightcraft_meta::Orientation> {
    let mut length = [0; 4];
    file.seek(SeekFrom::Start(26)).ok()?;
    file.read_exact(&mut length).ok()?;
    let resources = 30 + u64::from(u32::from_be_bytes(length));
    file.seek(SeekFrom::Start(resources)).ok()?;
    file.read_exact(&mut length).ok()?;
    let end = resources + 4 + u64::from(u32::from_be_bytes(length));
    if end > PSD_RESOURCES_SPAN {
        return None;
    }
    file.rewind().ok()?;
    let mut bytes = Vec::new();
    (&mut *file).take(end).read_to_end(&mut bytes).ok()?;
    lightcraft_meta::extract(&bytes).orientation
}

/// How far into a PSD or PSB [`psd_orientation`] reads.
const PSD_RESOURCES_SPAN: u64 = 16 * 1024 * 1024;

/// The orientation the TIFF stream from `start` to `end` in `file` states in its first IFD (a TIFF file, or the Exif
/// block of a WebP's EXIF chunk; its offsets count from `start`, and nothing past `end` is read). Only the IFD's 12-byte
/// entries are read (at most 65535 of them, under 1 MiB): its orientation tag, else (as [`lightcraft_meta::read_exif`]
/// does) the XMP packet its tag 700 holds, read if it is at most [`ORIENTATION_SPAN`].
fn tiff_orientation(file: &mut (impl Read + Seek), start: u64, end: u64) -> Option<lightcraft_meta::Orientation> {
    // where the `len` bytes at `offset` in the stream are in the file, when all of them are in the stream
    let within = |offset: u32, len: u64| {
        let at = start.checked_add(u64::from(offset))?;
        (at.checked_add(len)? <= end).then_some(at)
    };
    file.seek(SeekFrom::Start(within(0, 8)?)).ok()?;
    let mut header = [0; 8];
    file.read_exact(&mut header).ok()?;
    let [b0, b1, b2, b3, i0, i1, i2, i3] = header;
    let le = match &[b0, b1, b2, b3] {
        b"II*\0" => true,
        b"MM\0*" => false,
        _ => return None,
    };
    let u16_of = |b: [u8; 2]| if le { u16::from_le_bytes(b) } else { u16::from_be_bytes(b) };
    let u32_of = |b: [u8; 4]| if le { u32::from_le_bytes(b) } else { u32::from_be_bytes(b) };
    let ifd = u32_of([i0, i1, i2, i3]);
    file.seek(SeekFrom::Start(within(ifd, 2)?)).ok()?;
    let mut count = [0; 2];
    file.read_exact(&mut count).ok()?;
    let count = u16_of(count);
    within(ifd, 2 + 12 * u64::from(count))?;
    let mut entries = vec![0; usize::from(count) * 12];
    file.read_exact(&mut entries).ok()?;
    let mut xmp = None;
    for &[t0, t1, y0, y1, c0, c1, c2, c3, v0, v1, v2, v3] in entries.as_chunks::<12>().0 {
        let (tag, kind, count) = (u16_of([t0, t1]), u16_of([y0, y1]), u32_of([c0, c1, c2, c3]));
        match (tag, kind) {
            // exactly one SHORT (with more, the field also holds a second value, or an offset to them all)
            (274, 3) if count == 1 => {
                let v = u16_of([v0, v1]);
                if (1..=8).contains(&v) {
                    return Some(lightcraft_meta::Orientation::from_exif(v));
                }
            }
            // BYTE or UNDEFINED, at an offset (a packet longer than the field's four bytes)
            (700, 1 | 7) if count > 4 => xmp = Some((u32_of([v0, v1, v2, v3]), u64::from(count))),
            _ => {}
        }
    }
    let (offset, len) = xmp?;
    xmp_orientation(file, within(offset, len)?, len)
}

/// The orientation the XMP packet of `len` bytes at `at` in `file` states, read if it is at most [`ORIENTATION_SPAN`].
fn xmp_orientation(file: &mut (impl Read + Seek), at: u64, len: u64) -> Option<lightcraft_meta::Orientation> {
    let packet = read_within_span(file, at, len)?;
    lightcraft_meta::parse_whole_xmp(&String::from_utf8_lossy(&packet)).ok()?.metadata.orientation
}

/// The `len` bytes at `at` in `file`, read if they are at most [`ORIENTATION_SPAN`].
fn read_within_span(file: &mut (impl Read + Seek), at: u64, len: u64) -> Option<Vec<u8>> {
    if len > ORIENTATION_SPAN {
        return None;
    }
    file.seek(SeekFrom::Start(at)).ok()?;
    let mut bytes = Vec::new();
    (&mut *file).take(len).read_to_end(&mut bytes).ok()?;
    Some(bytes)
}

/// How much of a photo file [`file_orientation`] reads first.
const ORIENTATION_SPAN: u64 = 1 << 20;

/// The sidecar packet for a photo; the colour label is written by its name in `cat` (custom
/// label names included, as other editors do).
pub fn sidecar_packet(p: &Photo, cat: &lightcraft_catalog::Catalog) -> String {
    let nz = |s: &str| (!s.trim().is_empty()).then(|| s.to_string());
    // names in `dc:subject`, paths in `lr:hierarchicalSubject`
    let keywords = lightcraft_catalog::keywords::to_file(&p.meta.keywords);
    let meta = lightcraft_meta::Metadata {
        dji: p.meta.dji.clone(),
        software: Some("Local Image".into()),
        title: nz(&p.meta.title),
        caption: nz(&p.meta.caption),
        alt_text: nz(&p.meta.alt_text),
        extended_description: nz(&p.meta.extended_description),
        sublocation: nz(&p.meta.location),
        city: nz(&p.meta.city),
        state: nz(&p.meta.state),
        country: nz(&p.meta.country),
        copyright: nz(&p.meta.copyright),
        copyright_marked: p.meta.copyright_status.marked(),
        usage_terms: nz(&p.meta.usage_terms),
        copyright_url: nz(&p.meta.copyright_url),
        artist: nz(&p.meta.creator),
        creators: p.meta.creators.clone(),
        keywords: keywords.flat,
        hierarchical_keywords: keywords.hierarchical,
        rating: Some(p.rating.min(5) as i8),
        label: p.label.map(|l| cat.label_name(l)),
        capture_time: p.captured.as_deref().and_then(lightcraft_meta::DateTime::parse_iso),
        gps: p.meta.gps.map(|(latitude, longitude)| lightcraft_meta::Gps { latitude, longitude, altitude: None }),
        ..Default::default()
    };
    let settings = serde_json::to_string(&*p.develop).unwrap_or_default();
    let flag = match p.flag {
        Flag::None => "none",
        Flag::Pick => "pick",
        Flag::Reject => "reject",
    };
    let mut lc = vec![("settings", settings.as_str()), ("flag", flag)];
    if !p.meta.location.trim().is_empty() {
        lc.push(("location", p.meta.location.as_str()));
    }
    lightcraft_meta::write_xmp_lc(&meta, &lc)
}

/// The properties LightCraft writes and owns in a sidecar: replaced on every save (removed when
/// LightCraft has no value, so clearing a field clears it in the file too). All of them are read
/// back on import and by Read Metadata from File.
pub const OWNED: &[&str] = &[
    "xmp:Rating",
    "xmp:Label",
    "dc:title",
    "dc:description",
    "dc:rights",
    "dc:creator",
    "dc:subject",
    "lr:hierarchicalSubject",
    "Iptc4xmpCore:Location",
    "Iptc4xmpCore:AltTextAccessibility",
    "Iptc4xmpCore:ExtDescrAccessibility",
    "photoshop:City",
    "photoshop:State",
    "photoshop:Country",
    "xmpRights:Marked",
    "xmpRights:UsageTerms",
    "xmpRights:WebStatement",
    "lc:*",
];

/// Properties replaced only when LightCraft has a value (capture time; GPS, which LightCraft
/// doesn't read from sidecars): otherwise the sidecar's own value stays. Anything else LightCraft
/// writes (`xmp:CreatorTool`) goes only into new sidecars.
pub const OWNED_IF_STATED: &[&str] = &["exif:DateTimeOriginal", "photoshop:DateCreated", "exif:GPSLatitude", "exif:GPSLongitude"];

/// Write `data` to `path` atomically: a temp file next to it, synced, then renamed over it.
fn write_atomic(path: &Path, data: &[u8]) -> std::io::Result<()> {
    li_fs::write_atomic(path, data)
}

/// Copy `path` to a new `<name>.bak-<stamp>` (`-1`, `-2`… if taken; never overwriting).
fn backup(path: &Path, stamp: &str) -> std::io::Result<PathBuf> {
    use std::io::Write;
    // only a plain file is copied aside: a link under the sidecar's name would have its target's
    // contents copied into a new, readable file
    if !std::fs::symlink_metadata(path)?.is_file() {
        return Err(std::io::Error::other(format!("{} is not a regular file", path.display())));
    }
    let bytes = read_regular(path, SIDECAR_READ_MAX)?;
    let stamp: String = stamp.chars().filter(char::is_ascii_alphanumeric).collect();
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    for k in 0..1000 {
        let b = path.with_file_name(if k == 0 { format!("{name}.bak-{stamp}") } else { format!("{name}.bak-{stamp}-{k}") });
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&b) {
            Ok(mut f) => {
                f.write_all(&bytes)?;
                f.sync_all()?;
                return Ok(b);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::other("no free backup name"))
}

/// What [`Session::save_sidecar`] did.
#[derive(Clone, Debug, PartialEq)]
pub struct SidecarSaved {
    pub path: PathBuf,
    /// An existing sidecar was merged into (its other contents kept).
    pub merged: bool,
    /// An existing sidecar that couldn't be read as XMP was copied here before being replaced.
    pub backup: Option<PathBuf>,
}

/// The new contents for the sidecar at `path`: `fresh` merged into the existing file, if any.
/// Returns (contents, merged, backup).
fn sidecar_contents(path: &Path, fresh: String, stamp: &str) -> std::io::Result<(String, bool, Option<PathBuf>)> {
    let bytes = match read_regular(path, SIDECAR_READ_MAX) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok((fresh, false, None)),
        // can't see what is there: never overwrite it blindly
        Err(e) => return Err(e),
    };
    if bytes.iter().all(u8::is_ascii_whitespace) {
        return Ok((fresh, false, None));
    }
    let rules = lightcraft_meta::MergeRules { owned: OWNED, owned_if_present: OWNED_IF_STATED };
    let why = match std::str::from_utf8(&bytes) {
        Ok(old) => match lightcraft_meta::merge_xmp(old, &fresh, rules) {
            Ok(merged) => return Ok((merged, true, None)),
            // a readable sidecar whose merge fails, or one over the parser's complexity limits:
            // fail the save and keep the file, rather than replace metadata that could be read.
            // (Text with no `rdf:RDF` element parses as empty metadata but holds nothing to keep:
            // it is replaced after a backup, as any sidecar that is not XMP.)
            Err(why)
                if why == lightcraft_meta::XmpError::TooComplex
                    || (lightcraft_meta::parse_xmp(old.strip_prefix('\u{feff}').unwrap_or(old)).is_ok()
                        && !matches!(&why, lightcraft_meta::XmpError::Xml(m) if m.contains("no rdf:RDF element"))) =>
            {
                return Err(std::io::Error::other(format!("the sidecar is readable but could not be merged ({why}); it was left unchanged")));
            }
            Err(why) => why.to_string(),
        },
        Err(why) => why.to_string(),
    };
    let b = backup(path, stamp)?;
    log::warn!("{}: not readable as XMP ({why}); kept a copy as {} before replacing it", path.display(), b.display());
    Ok((fresh, false, Some(b)))
}

/// Most bytes of a sidecar read into memory.
const SIDECAR_READ_MAX: u64 = 256 << 20;

/// The whole of the regular file at `path`, at most `max` bytes: a FIFO or device under a
/// sidecar's or original's name is refused, not read.
fn read_regular(path: &Path, max: u64) -> std::io::Result<Vec<u8>> {
    li_fs::read::file(path, max, &lightcraft_sysmem::jobs::Interrupt::NONE)
}

/// Key of the stem sidecar a file maps to (case-insensitive).
fn stem_key(original: &str) -> String {
    sidecar_path(original, SidecarNaming::Stem).to_string_lossy().to_lowercase()
}

fn file_path(p: &Photo) -> Option<&str> {
    match &p.source {
        Source::File { path } => Some(path),
        Source::Demo { .. } => None,
    }
}

/// The XMP packet that describes a photo on disk: its sidecar, else (raw/DNG) the embedded XMP.
pub fn read_packet(original: &str, kind: MediaKind, naming: SidecarNaming) -> Option<(String, PathBuf)> {
    read_packet_in(original, kind, SidecarLookup { naming, stem_ok: true })
}

/// [`read_packet`] for a catalogued photo: never a stem sidecar that a sibling owns.
pub fn read_packet_in(original: &str, kind: MediaKind, lookup: SidecarLookup) -> Option<(String, PathBuf)> {
    if let Some(sc) = find_sidecar_in(original, lookup)
        && let Some(s) = read_regular(&sc, SIDECAR_READ_MAX).ok().and_then(|b| String::from_utf8(b).ok())
    {
        return Some((s, sc));
    }
    if kind == MediaKind::Raw {
        let bytes = read_regular(Path::new(original), li_fs::read::MAX_FILE_BYTES).ok()?;
        let x = lightcraft_meta::embedded(&bytes).xmp?;
        return Some((x, PathBuf::from(original)));
    }
    None
}

/// Photo ids touched by an op (for auto-write).
pub(crate) fn op_photos(op: &Op, out: &mut Vec<PhotoId>) {
    match op {
        Op::SetRating { id, .. } | Op::SetFlag { id, .. } | Op::SetLabel { id, .. } | Op::SetDevelop { id, .. } | Op::SetMeta { id, .. } => {
            if !out.contains(id) {
                out.push(*id);
            }
        }
        Op::Batch { ops } => ops.iter().for_each(|o| op_photos(o, out)),
        _ => {}
    }
}

/// Which photo owns each stem sidecar that several catalogued files map to (see
/// [`Session::sidecar_naming`]).
pub(crate) struct StemOwners(HashMap<String, PhotoId>);

impl StemOwners {
    pub(crate) fn of(cat: &lightcraft_catalog::Catalog) -> StemOwners {
        // per stem: (raw first, then file name, then id) of the best candidate; and how many
        let mut best: HashMap<String, ((bool, String, u64), PhotoId, usize)> = HashMap::new();
        for p in cat.photos().filter(|p| p.copy_of.is_none()) {
            let Some(path) = file_path(p) else { continue };
            let rank = (p.kind != MediaKind::Raw, path.to_lowercase(), p.id.0);
            let e = best.entry(stem_key(path)).or_insert_with(|| (rank.clone(), p.id, 0));
            e.2 += 1;
            if rank < e.0 {
                (e.0, e.1) = (rank, p.id);
            }
        }
        StemOwners(best.into_iter().filter(|(_, v)| v.2 > 1).map(|(k, v)| (k, v.1)).collect())
    }

    /// `naming` for photo `p`, except that a stem sidecar another file owns becomes Full.
    fn naming(&self, p: &Photo, naming: SidecarNaming) -> SidecarNaming {
        match (naming, file_path(p)) {
            (SidecarNaming::Stem, Some(path)) if self.0.get(&stem_key(path)).is_some_and(|owner| *owner != p.id) => SidecarNaming::Full,
            _ => naming,
        }
    }

    /// [`StemOwners::naming`], plus whether the stem sidecar may be read for `p`.
    fn lookup(&self, p: &Photo, naming: SidecarNaming) -> SidecarLookup {
        let stem_ok = file_path(p).is_none_or(|path| self.0.get(&stem_key(path)).is_none_or(|owner| *owner == p.id));
        SidecarLookup { naming: self.naming(p, naming), stem_ok }
    }
}

impl Session {
    /// The sidecar naming for photo `id`: the library's, except under Stem naming when another
    /// catalogued file shares the stem (`IMG_0001.CR3` + `IMG_0001.JPG`) and owns the stem
    /// sidecar — a raw first, else the first by file name. The others use Full naming
    /// (`IMG_0001.JPG.xmp`), so their metadata never overwrites each other.
    pub fn sidecar_naming(&self, id: PhotoId) -> SidecarNaming {
        match self.catalog.photo(id) {
            Some(p) if self.xmp.naming == SidecarNaming::Stem => StemOwners::of(&self.catalog).naming(p, self.xmp.naming),
            _ => self.xmp.naming,
        }
    }

    /// [`Session::sidecar_naming`] and whether the stem sidecar may be read, for reading `id`'s sidecar.
    pub fn sidecar_lookup(&self, id: PhotoId) -> SidecarLookup {
        match self.catalog.photo(id) {
            Some(p) => StemOwners::of(&self.catalog).lookup(p, self.xmp.naming),
            None => SidecarLookup { naming: self.xmp.naming, stem_ok: true },
        }
    }

    /// Write the photo's XMP sidecar, merged into an existing one (see the module docs).
    pub fn save_sidecar(&self, id: PhotoId) -> Result<SidecarSaved> {
        self.save_sidecar_with(id, &StemOwners::of(&self.catalog))
    }

    pub(crate) fn save_sidecar_with(&self, id: PhotoId, owners: &StemOwners) -> Result<SidecarSaved> {
        let p = self.catalog.photo(id).ok_or(lightcraft_catalog::CatalogError::NoPhoto(id))?;
        if p.copy_of.is_some() {
            return Err(EngineError::Other(format!("{} is a virtual copy: its settings live only in the library", p.file_name)));
        }
        let orig = file_path(p).ok_or_else(|| EngineError::Other(format!("{} is not a file on disk", p.file_name)))?;
        let path = sidecar_path(orig, owners.naming(p, self.xmp.naming));
        let err = |e: std::io::Error| EngineError::Other(format!("{}: {e}", path.display()));
        let (data, merged, backup) = sidecar_contents(&path, sidecar_packet(p, &self.catalog), &(self.clock)()).map_err(err)?;
        write_atomic(&path, data.as_bytes()).map_err(err)?;
        Ok(SidecarSaved { path, merged, backup })
    }

    /// The op that applies a photo's sidecar (or embedded XMP) to the catalog, if there is one.
    pub fn read_sidecar_op(&self, id: PhotoId) -> Result<Option<(Op, PathBuf)>> {
        let p = self.catalog.photo(id).ok_or(lightcraft_catalog::CatalogError::NoPhoto(id))?;
        let Some(orig) = file_path(p) else { return Ok(None) };
        let Some((packet, from)) = read_packet_in(orig, p.kind, self.sidecar_lookup(id)) else { return Ok(None) };
        let sc = parse_sidecar(&packet, p.kind == MediaKind::Raw)
            .map_err(|e| EngineError::Other(format!("{}: {e}", from.display())))?
            .with_file_orientation(orig)
            .resolve_label(&self.catalog);
        let mut q = (**p).clone();
        let develop_changed = merge_into(&mut q, &sc, &(self.clock)());
        let mut ops = vec![
            Op::SetRating { id, rating: q.rating },
            Op::SetFlag { id, flag: q.flag },
            Op::SetLabel { id, label: q.label },
            Op::SetMeta { id, meta: Box::new(q.meta.clone()) },
        ];
        if q.captured != p.captured {
            ops.push(Op::SetCaptured { id, captured: q.captured.clone() });
        }
        if develop_changed {
            ops.extend(self.develop_op(id, (*q.develop).clone(), "Read Metadata from File"));
        }
        Ok(Some((Op::Batch { ops }, from)))
    }

    /// Auto-write: sidecars for photos changed by `ops` (errors are logged, not returned).
    pub(crate) fn auto_write_sidecars(&self, ops: &[Op]) {
        let mut ids = Vec::new();
        ops.iter().for_each(|o| op_photos(o, &mut ids));
        self.auto_write_sidecars_for(&ids);
    }

    /// New derivatives inherit user metadata inside AddPhoto, rather than later
    /// SetMeta operations. Use the same auto-write naming and error policy.
    pub(crate) fn auto_write_sidecars_for(&self, ids: &[PhotoId]) {
        if ids.is_empty() {
            return;
        }
        let owners = StemOwners::of(&self.catalog);
        for &id in ids {
            if !self.is_ephemeral(id)
                && self.catalog.photo(id).is_some_and(|p| file_path(p).is_some() && p.copy_of.is_none())
                && let Err(e) = self.save_sidecar_with(id, &owners)
            {
                log::warn!("auto-write XMP: {e}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lightcraft_catalog::Meta;

    fn photo() -> Photo {
        let mut p = Photo::new(PhotoId(7), Source::File { path: "/x/IMG_1.jpg".into() }, "IMG_1.jpg", "JPEG", 40, 30, "2026-01-01T00:00:00");
        p.rating = 4;
        p.flag = Flag::Pick;
        p.label = Some(ColorLabel::Purple);
        p.captured = Some("2025-06-07T08:09:10".into());
        p.meta = Meta {
            title: "Dune & sky".into(),
            caption: "Line <one>\nline two".into(),
            copyright: "© 2026 Me".into(),
            creator: "Ann Lee".into(),
            location: "Namib".into(),
            keywords: vec!["desert".into(), "red".into()],
            gps: Some((-24.75, 15.3)),
            ..Default::default()
        };
        let mut d = DevelopSettings::default();
        d.light.exposure = 0.75;
        d.effects.clarity = 22.0;
        d.crop.geometry.angle = 2.5;
        d.masks.push(lightcraft_develop::Mask { id: 1, ..Default::default() });
        p.develop = Arc::new(d);
        p
    }

    #[test]
    fn naming() {
        assert_eq!(sidecar_path("/a/b/IMG_1.CR3", SidecarNaming::Stem), PathBuf::from("/a/b/IMG_1.xmp"));
        assert_eq!(sidecar_path("/a/b/IMG_1.CR3", SidecarNaming::Full), PathBuf::from("/a/b/IMG_1.CR3.xmp"));
    }

    #[test]
    fn packet_roundtrip_restores_everything() {
        let p = photo();
        let x = sidecar_packet(&p, &lightcraft_catalog::Catalog::new());
        let sc = parse_sidecar(&x, false).unwrap();
        assert_eq!(sc.rating, Some(4));
        assert_eq!(sc.flag, Some(Flag::Pick));
        assert_eq!(sc.label, Some(Some(ColorLabel::Purple)));
        let mut q = Photo::new(PhotoId(7), p.source.clone(), "IMG_1.jpg", "JPEG", 40, 30, "2026-01-01T00:00:00");
        q.captured = p.captured.clone();
        q.meta.gps = p.meta.gps;
        assert!(merge_into(&mut q, &sc, "now"));
        assert_eq!(q.develop, p.develop);
        assert_eq!(q.meta, p.meta);
        assert_eq!((q.rating, q.flag, q.label), (p.rating, p.flag, p.label));
        assert_eq!(q.edited.as_deref(), Some("now"));
    }

    #[test]
    fn foreign_sidecar_rating_reject_label_and_crs() {
        let x = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
          <rdf:Description rdf:about="" xmlns:xmp="http://ns.adobe.com/xap/1.0/" xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/"
            xmp:Rating="-1" xmp:Label="Green" crs:Exposure2012="-0.40" crs:Vibrance="+12"/></rdf:RDF></x:xmpmeta>"#;
        let sc = parse_sidecar(x, true).unwrap();
        assert_eq!((sc.rating, sc.flag, sc.label), (Some(0), Some(Flag::Reject), Some(Some(ColorLabel::Green))));
        let mut p = photo();
        let before = p.develop.clone();
        assert!(merge_into(&mut p, &sc, "t"));
        assert_eq!(p.develop.light.exposure, -0.4);
        assert_eq!(p.develop.color.vibrance, 12.0);
        assert_eq!(p.develop.effects.clarity, before.effects.clarity, "crs partial keeps unrelated settings");
        assert_eq!(p.meta.title, "Dune & sky", "fields the sidecar doesn't state are kept");
    }

    /// A sidecar written by Lightroom Classic (names flat, paths in `lr:hierarchicalSubject`)
    /// gives the photo its keywords as paths; one without paths, the names as they are.
    #[test]
    fn sidecar_keywords_keep_their_hierarchy() {
        let m = lightcraft_meta::Metadata {
            keywords: vec!["Lisbon".into(), "PLACES".into(), "beach".into()],
            hierarchical_keywords: vec!["Places|Lisbon".into()],
            ..Default::default()
        };
        let sc = parse_sidecar(&lightcraft_meta::write_xmp(&m, None), false).unwrap();
        assert_eq!(sc.keywords, Some(vec!["Places|Lisbon".to_string(), "beach".to_string()]));
        let flat = lightcraft_meta::Metadata { keywords: vec!["a|b".into(), " sea ".into()], ..Default::default() };
        let sc = parse_sidecar(&lightcraft_meta::write_xmp(&flat, None), false).unwrap();
        assert_eq!(sc.keywords, Some(vec!["a|b".to_string(), "sea".to_string()]));
    }

    /// Saving a sidecar keeps keywords where other applications look for them: names in
    /// `dc:subject`, never paths, and the paths in `lr:hierarchicalSubject`. Read back, the same
    /// keywords; and the hierarchical list is ours, so clearing the keywords clears it in a sidecar
    /// that had it.
    #[test]
    fn sidecar_writes_keyword_names_flat_and_paths_hierarchical() {
        let cat = lightcraft_catalog::Catalog::new();
        let mut p = photo();
        p.meta.keywords = vec!["Places|Italy|Rome".into(), "sunset".into()];
        let written = sidecar_packet(&p, &cat);
        let lists = lightcraft_meta::parse_xmp(&written).unwrap().metadata;
        assert_eq!(lists.keywords, ["Places", "Italy", "Rome", "sunset"], "names, not paths, in dc:subject:\n{written}");
        assert_eq!(lists.hierarchical_keywords, ["Places|Italy|Rome", "sunset"], "{written}");
        assert_eq!(parse_sidecar(&written, false).unwrap().keywords, Some(p.meta.keywords.clone()), "read back as written");

        p.meta.keywords.clear();
        let rules = lightcraft_meta::MergeRules { owned: OWNED, owned_if_present: OWNED_IF_STATED };
        let cleared = lightcraft_meta::merge_xmp(&written, &sidecar_packet(&p, &cat), rules).unwrap();
        assert!(!cleared.contains("<dc:subject>") && !cleared.contains("<lr:hierarchicalSubject>"), "{cleared}");
    }

    /// A WebP keeps its EXIF and XMP chunks after the image data, a TIFF's first IFD can follow its strips and a
    /// JPEG's Exif other long segments, so the orientation can be past the first [`ORIENTATION_SPAN`] of the file.
    /// Every file gives what the import gives reading it whole: Exif over XMP wherever each is, a WebP's last EXIF
    /// chunk and a JPEG's first Exif segment; a TIFF orientation is exactly one SHORT, and a file that states none has
    /// none.
    #[test]
    fn the_file_orientation_is_found_after_the_image_data() {
        use lightcraft_meta::Orientation;
        let dir = std::env::temp_dir().join(format!("lc-file-orientation-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let exif = lightcraft_meta::write_exif(&lightcraft_meta::Metadata { orientation: Some(Orientation::Rotate90), ..Default::default() });
        let span = ORIENTATION_SPAN as usize;
        let (o6, o8) = (Some(Orientation::Rotate90), Some(Orientation::Rotate270));
        // a WebP: VP8X, then `chunks`
        let webp = |chunks: &[(&[u8], Vec<u8>)]| {
            let mut w = b"RIFF\0\0\0\0WEBP".to_vec();
            for (id, data) in [(&b"VP8X"[..], vec![0; 10])].iter().chain(chunks) {
                w.extend_from_slice(id);
                w.extend_from_slice(&(data.len() as u32).to_le_bytes());
                w.extend_from_slice(data);
                if data.len() % 2 == 1 {
                    w.push(0);
                }
            }
            w
        };
        let image = |len: usize| (&b"VP8L"[..], vec![0; len]);
        let exif_chunk = (&b"EXIF"[..], [&b"Exif\0\0"[..], &exif].concat());
        // an EXIF chunk over the span (orientation 6 too), one stating no orientation, and an XMP packet stating 8
        let huge_exif = (&b"EXIF"[..], [&b"Exif\0\0"[..], &exif, &vec![0; span]].concat());
        let plain = lightcraft_meta::write_exif(&lightcraft_meta::Metadata::default());
        let plain_chunk = (&b"EXIF"[..], [&b"Exif\0\0"[..], &plain].concat());
        let packet = lightcraft_meta::write_xmp(&lightcraft_meta::Metadata { orientation: o8, ..Default::default() }, None);
        let xmp_chunk = (&b"XMP "[..], packet.clone().into_bytes());
        // the XMP packet padded with whitespace to `len` bytes
        let padded_xmp = |len: usize| {
            let mut p = packet.clone().into_bytes();
            p.resize(len, b' ');
            (&b"XMP "[..], p)
        };
        // an EXIF chunk holding only a TIFF header, whose IFD offset (16) points past the chunk's end into the next
        // chunk's data, where an IFD stating orientation 6 sits
        let stray_exif = (&b"EXIF"[..], b"II*\0\x10\0\0\0".to_vec());
        let stray_ifd = (&b"JUNK"[..], vec![1, 0, 0x12, 1, 3, 0, 1, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0]);
        // a TIFF whose first IFD follows more than the first MiB of image data: an orientation tag of `values` values
        // whose field starts with `value` (with more than two, it is an offset), a tag pointing elsewhere, and an XMP
        // packet (tag 700) stored after the IFD
        let tiff = |le: bool, orientation: Option<(u32, u16)>, xmp: Option<&str>| {
            let (u16_of, u32_of): (fn(u16) -> [u8; 2], fn(u32) -> [u8; 4]) =
                if le { (u16::to_le_bytes, u32::to_le_bytes) } else { (u16::to_be_bytes, u32::to_be_bytes) };
            let strips = span + 64;
            let mut entries = Vec::new();
            if let Some((values, value)) = orientation {
                entries.push((274u16, 3u16, values, [u16_of(value), [0, 0]].concat()));
            }
            entries.push((305, 2, 64, u32_of(4).to_vec()));
            let packet_at = 8 + strips + 2 + 12 * (entries.len() + usize::from(xmp.is_some())) + 4;
            if let Some(x) = xmp {
                entries.push((700, 7, x.len() as u32, u32_of(packet_at as u32).to_vec()));
            }
            let mut t = if le { b"II*\0".to_vec() } else { b"MM\0*".to_vec() };
            t.extend_from_slice(&u32_of(8 + strips as u32));
            t.resize(8 + strips, 0);
            t.extend_from_slice(&u16_of(entries.len() as u16));
            for (tag, kind, count, value) in entries {
                t.extend_from_slice(&u16_of(tag));
                t.extend_from_slice(&u16_of(kind));
                t.extend_from_slice(&u32_of(count));
                t.extend_from_slice(&value);
            }
            t.extend_from_slice(&[0; 4]);
            t.extend_from_slice(xmp.unwrap_or_default().as_bytes());
            t
        };
        // a JPEG: the APP1 payloads `early`, more than the first MiB of other segments, `late`, the image data
        let jpeg = |early: &[Vec<u8>], late: &[Vec<u8>]| {
            let app1 = |j: &mut Vec<u8>, payload: &Vec<u8>| {
                j.extend_from_slice(&[0xff, 0xe1]);
                j.extend_from_slice(&(payload.len() as u16 + 2).to_be_bytes());
                j.extend_from_slice(payload);
            };
            let mut j = vec![0xff, 0xd8];
            early.iter().for_each(|p| app1(&mut j, p));
            for _ in 0..17 {
                j.extend_from_slice(&[0xff, 0xef, 0xff, 0xff]);
                j.resize(j.len() + 0xfffd, 0);
            }
            late.iter().for_each(|p| app1(&mut j, p));
            j.extend_from_slice(&[0xff, 0xda, 0, 2, 0xff, 0xd9]);
            j
        };
        let exif_app1 = [&b"Exif\0\0"[..], &exif].concat();
        let plain_app1 = [&b"Exif\0\0"[..], &plain].concat();
        let xmp_app1 = [&b"http://ns.adobe.com/xap/1.0/\0"[..], packet.as_bytes()].concat();
        for (name, bytes, want) in [
            ("small.webp", webp(&[image(64), exif_chunk.clone()]), o6),
            ("large.webp", webp(&[image(span + 64), exif_chunk.clone(), xmp_chunk.clone()]), o6),
            // EXIF wins in whichever order the chunks come, and an XMP in the first MiB doesn't stop the search
            ("reversed.webp", webp(&[image(span + 64), xmp_chunk.clone(), exif_chunk.clone()]), o6),
            ("xmp-first.webp", webp(&[xmp_chunk.clone(), image(span + 64), exif_chunk.clone()]), o6),
            ("none.webp", webp(&[image(span + 64), exif_chunk.clone()])[..span + 100].to_vec(), None),
            ("xmp.webp", webp(&[image(span + 64), xmp_chunk.clone()]), o8),
            // an EXIF chunk of any size is read by its IFD: one over the span, one (without `Exif\0\0`, big-endian)
            // whose IFD is more than the span into it
            ("huge.webp", webp(&[image(span + 64), huge_exif, xmp_chunk.clone()]), o6),
            ("late-ifd.webp", webp(&[image(span + 64), (&b"EXIF"[..], tiff(false, Some((1, 6)), None)), xmp_chunk.clone()]), o6),
            // nothing outside the EXIF chunk is read for its IFD
            ("outside.webp", webp(&[image(span + 64), stray_exif, stray_ifd, xmp_chunk.clone()]), o8),
            // the last EXIF chunk counts, here a late one stating no orientation after one in the first MiB
            ("last-exif.webp", webp(&[exif_chunk, image(span + 64), plain_chunk, xmp_chunk]), o8),
            // an XMP chunk of the span is read
            ("span-xmp.webp", webp(&[image(64), padded_xmp(span)]), o8),
            ("le.tif", tiff(true, Some((1, 8)), None), o8),
            ("be.tif", tiff(false, Some((1, 8)), None), o8),
            ("offset.tif", tiff(true, Some((3, 8)), None), None),
            // the tag wins over the IFD's XMP; XMP fills in for no tag or an invalid one
            ("both.tif", tiff(true, Some((1, 6)), Some(&packet)), o6),
            ("xmp.tif", tiff(false, None, Some(&packet)), o8),
            ("invalid.tif", tiff(true, Some((1, 0)), Some(&packet)), o8),
            ("none.jpg", jpeg(&[], &[]), None),
            ("xmp-first.jpg", jpeg(std::slice::from_ref(&xmp_app1), std::slice::from_ref(&exif_app1)), o6),
            ("xmp-only.jpg", jpeg(&[], std::slice::from_ref(&xmp_app1)), o8),
            // the first Exif segment counts, though it states no orientation
            ("two-exif.jpg", jpeg(&[plain_app1], &[exif_app1, xmp_app1]), o8),
        ] {
            let path = dir.join(name);
            std::fs::write(&path, bytes).unwrap();
            assert_eq!(file_orientation(&path.to_string_lossy()), want, "{name}");
        }
        // a larger XMP chunk isn't read, where the import (reading the whole file) takes its orientation
        let over = webp(&[image(64), padded_xmp(span + 1)]);
        assert_eq!(lightcraft_meta::extract(&over).orientation, o8);
        std::fs::write(dir.join("over-xmp.webp"), over).unwrap();
        assert_eq!(file_orientation(&dir.join("over-xmp.webp").to_string_lossy()), None);
        assert_eq!(file_orientation(&dir.join("missing.webp").to_string_lossy()), None);
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// The same for a PNG, whose `eXIf` and XMP `iTXt` chunks can follow the image data, and a PSD, whose
    /// image resources can outgrow the first [`ORIENTATION_SPAN`].
    #[test]
    fn a_png_or_psd_gives_the_orientation_the_import_gives() {
        let dir = std::env::temp_dir().join(format!("lc-png-psd-orientation-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let span = ORIENTATION_SPAN as usize;
        let (o6, o8) = (Some(lightcraft_meta::Orientation::Rotate90), Some(lightcraft_meta::Orientation::Rotate270));
        let exif = lightcraft_meta::write_exif(&lightcraft_meta::Metadata { orientation: o6, ..Default::default() });
        let plain = lightcraft_meta::write_exif(&lightcraft_meta::Metadata::default());
        let packet = lightcraft_meta::write_xmp(&lightcraft_meta::Metadata { orientation: o8, ..Default::default() }, None);
        // a PNG: its IHDR, then `chunks`, each with a CRC (nothing checks it)
        let png = |chunks: &[(&[u8], Vec<u8>)]| {
            let mut p = b"\x89PNG\r\n\x1a\n".to_vec();
            for (id, data) in [(&b"IHDR"[..], vec![0; 13])].iter().chain(chunks) {
                p.extend_from_slice(&(data.len() as u32).to_be_bytes());
                p.extend_from_slice(id);
                p.extend_from_slice(data);
                p.extend_from_slice(&[0; 4]);
            }
            p
        };
        let idat = |len: usize| (&b"IDAT"[..], vec![0; len]);
        let iend = (&b"IEND"[..], Vec::new());
        let exif_chunk = (&b"eXIf"[..], exif.clone());
        let plain_chunk = (&b"eXIf"[..], plain.clone());
        // an `iTXt` chunk: the keyword, compression flag `flag`, method, no language, no translated keyword, the text
        let itxt = |keyword: &[u8], flag: u8, text: &[u8]| (&b"iTXt"[..], [keyword, &[0, flag, 0, 0, 0], text].concat());
        let xmp_key = &b"XML:com.adobe.xmp"[..];
        let xmp_chunk = itxt(xmp_key, 0, packet.as_bytes());
        // the XMP packet padded with whitespace, in a chunk of `len` bytes
        let padded_xmp = |len: usize| {
            let mut p = packet.clone().into_bytes();
            p.resize(len - 22, b' ');
            itxt(xmp_key, 0, &p)
        };
        // a big-endian TIFF stream whose IFD, stating orientation 6, is more than the span into it
        let ifd = [0u8, 1, 1, 0x12, 0, 3, 0, 0, 0, 1, 0, 6, 0, 0, 0, 0, 0, 0];
        let late_ifd = [&b"MM\0*"[..], &(8 + span as u32).to_be_bytes(), &vec![0; span], &ifd].concat();
        // an `eXIf` holding only a TIFF header, whose IFD offset (20) points past the chunk's end into the next chunk's
        // data, where an IFD stating orientation 6 sits
        let stray_exif = (&b"eXIf"[..], b"II*\0\x14\0\0\0".to_vec());
        let stray_ifd = (&b"juNK"[..], vec![1, 0, 0x12, 1, 3, 0, 1, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0]);
        // a PSD: its header, `colour` bytes of colour-mode data, the image resources (id, data), `pixels` other bytes
        let psd = |colour: usize, resources: &[(u16, &[u8])], pixels: usize| {
            let mut section = Vec::new();
            for (id, data) in resources {
                section.extend_from_slice(b"8BIM");
                section.extend_from_slice(&id.to_be_bytes());
                section.extend_from_slice(&[0, 0]);
                section.extend_from_slice(&(data.len() as u32).to_be_bytes());
                section.extend_from_slice(data);
                if data.len() % 2 == 1 {
                    section.push(0);
                }
            }
            let mut p = b"8BPS\0\x01".to_vec();
            p.resize(26, 0);
            p.extend_from_slice(&(colour as u32).to_be_bytes());
            p.resize(p.len() + colour, 0);
            p.extend_from_slice(&(section.len() as u32).to_be_bytes());
            p.extend_from_slice(&section);
            p.resize(p.len() + pixels, 0);
            p
        };
        let long = vec![0; span + 64];
        let (exif_res, plain_res, xmp_res, long_res) = ((1058, &exif[..]), (1058, &plain[..]), (1060, packet.as_bytes()), (1039, &long[..]));
        let late_png = png(&[idat(span + 64), exif_chunk.clone()]);
        let late_psd = psd(0, &[long_res, exif_res], 0);
        for (name, bytes, want) in [
            ("early.png", png(&[exif_chunk.clone(), idat(span + 64), iend.clone()]), o6),
            ("late.png", png(&[idat(span + 64), exif_chunk.clone(), xmp_chunk.clone(), iend.clone()]), o6),
            // the `eXIf` wins in whichever order the chunks come, and an XMP in the first MiB doesn't stop the search
            ("reversed.png", png(&[idat(span + 64), xmp_chunk.clone(), exif_chunk.clone(), iend.clone()]), o6),
            ("xmp-first.png", png(&[xmp_chunk.clone(), idat(span + 64), exif_chunk.clone(), iend.clone()]), o6),
            ("xmp.png", png(&[idat(span + 64), xmp_chunk.clone(), iend.clone()]), o8),
            ("none.png", png(&[idat(span + 64), iend.clone()]), None),
            // the `eXIf` is cut short, or comes after the end of the image
            ("cut.png", late_png[..late_png.len() - 6].to_vec(), None),
            ("after-iend.png", png(&[idat(span + 64), iend.clone(), exif_chunk.clone()]), None),
            // an `eXIf` of any size is read by its IFD: with `Exif\0\0`, over the span, and with its IFD past the span
            ("prefixed.png", png(&[idat(span + 64), (&b"eXIf"[..], [&b"Exif\0\0"[..], &exif].concat())]), o6),
            ("huge.png", png(&[idat(64), (&b"eXIf"[..], [&exif[..], &vec![0; span]].concat()), xmp_chunk.clone()]), o6),
            ("late-ifd.png", png(&[idat(span + 64), (&b"eXIf"[..], late_ifd), xmp_chunk.clone()]), o6),
            // nothing outside the `eXIf` is read for its IFD
            ("outside.png", png(&[idat(span + 64), stray_exif, stray_ifd, xmp_chunk.clone()]), o8),
            // the last `eXIf` counts, here a late one stating no orientation after one in the first MiB
            ("last-exif.png", png(&[exif_chunk.clone(), idat(span + 64), plain_chunk, xmp_chunk.clone()]), o8),
            // a compressed packet or another keyword isn't the XMP; a chunk of the span is read
            ("compressed.png", png(&[idat(span + 64), xmp_chunk.clone(), itxt(xmp_key, 1, packet.as_bytes())]), o8),
            ("only-compressed.png", png(&[idat(span + 64), itxt(xmp_key, 1, packet.as_bytes())]), None),
            ("other-text.png", png(&[idat(span + 64), itxt(b"Comment", 0, packet.as_bytes())]), None),
            ("short-text.png", png(&[idat(span + 64), xmp_chunk.clone(), (&b"iTXt"[..], b"XML:com.adobe.xmp\0\0".to_vec())]), o8),
            ("span-xmp.png", png(&[idat(64), padded_xmp(span)]), o8),
            ("late.psd", late_psd.clone(), o6),
            ("early.psd", psd(0, &[exif_res], span + 64), o6),
            // Exif wins in whichever order the resources come; XMP fills in for none, or none stating an orientation
            ("both.psd", psd(0, &[xmp_res, long_res, exif_res], 0), o6),
            ("xmp.psd", psd(768, &[long_res, xmp_res], 0), o8),
            ("plain.psd", psd(0, &[long_res, plain_res, xmp_res], 0), o8),
            ("none.psd", psd(0, &[long_res], span), None),
            // as at import, nothing is taken from resources that are cut short or state their Exif twice
            ("cut.psd", late_psd[..late_psd.len() - 1].to_vec(), None),
            ("two-exif.psd", psd(0, &[long_res, exif_res, exif_res], 0), None),
        ] {
            assert_eq!(lightcraft_meta::extract(&bytes).orientation, want, "import: {name}");
            let path = dir.join(name);
            std::fs::write(&path, bytes).unwrap();
            assert_eq!(file_orientation(&path.to_string_lossy()), want, "{name}");
        }
        // a larger XMP chunk isn't read, where the import (reading the whole file) takes its orientation
        let over = png(&[idat(64), padded_xmp(span + 1)]);
        assert_eq!(lightcraft_meta::extract(&over).orientation, o8);
        std::fs::write(dir.join("over-xmp.png"), over).unwrap();
        assert_eq!(file_orientation(&dir.join("over-xmp.png").to_string_lossy()), None);
        std::fs::remove_dir_all(dir).unwrap();

        // the `eXIf` as the last chunk walked, and as the first one not: IHDR, `chunks` of empty `IDAT`, the `eXIf`
        let steps = png_chunk_steps;
        let split = |chunks: usize| png(&[vec![idat(0); chunks], vec![exif_chunk.clone()]].concat());
        let len = |chunks: usize| (8 + 25 + 12 * chunks + 12 + exif.len()) as u64;
        let walked = (0..).find(|&n| n as u64 + 2 == steps(len(n)) && n as u64 + 3 > steps(len(n + 1))).unwrap();
        let (last, past) = (split(walked), split(walked + 1));
        assert_eq!(last.len() as u64, len(walked));
        assert_eq!(png_orientation(&mut std::io::Cursor::new(&last)), o6);
        assert_eq!(lightcraft_meta::extract(&past).orientation, o6);
        assert_eq!(png_orientation(&mut std::io::Cursor::new(&past)), None);
        // image data split into chunks of a KiB is walked whatever its length, up to the limit
        let kib_chunks = png(&[vec![idat(1012); 6000], vec![exif_chunk.clone()]].concat());
        assert_eq!(png_orientation(&mut std::io::Cursor::new(&kib_chunks)), o6);
        assert_eq!(steps(u64::MAX), 1 << 20);

        // image resources that end exactly at the limit, and a byte past it
        let limit = PSD_RESOURCES_SPAN as usize;
        let padding = vec![0; limit - 58 - exif.len().next_multiple_of(2)];
        let within = psd(0, &[(1039, &padding[..]), exif_res], 0);
        assert_eq!(within.len(), limit);
        assert_eq!(psd_orientation(&mut std::io::Cursor::new(&within)), o6);
        let past = psd(1, &[(1039, &padding[..]), exif_res], 0);
        assert_eq!(lightcraft_meta::extract(&past).orientation, o6);
        assert_eq!(psd_orientation(&mut std::io::Cursor::new(&past)), None);
    }

    #[test]
    fn op_photo_collection() {
        let mut v = Vec::new();
        op_photos(
            &Op::Batch {
                ops: vec![
                    Op::SetRating { id: PhotoId(1), rating: 2 },
                    Op::SetFlag { id: PhotoId(1), flag: Flag::Pick },
                    Op::SetLabel { id: PhotoId(3), label: None },
                ],
            },
            &mut v,
        );
        assert_eq!(v, vec![PhotoId(1), PhotoId(3)]);
    }

    /// A sidecar over the parser's namespace-declaration limit cannot be read or merged: the
    /// save fails and the file stays, with no backup made.
    #[test]
    fn a_sidecar_over_the_complexity_limit_is_kept() {
        let dir = std::env::temp_dir().join(format!("lc-sidecar-complex-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let decls: String = (0..600).map(|i| format!(" xmlns:n{i}=\"urn:n:{i}\"")).collect();
        let old = format!(
            "<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\"{decls}><rdf:Description rdf:about=\"\"/></rdf:RDF></x:xmpmeta>"
        );
        assert_eq!(lightcraft_meta::parse_xmp(&old).unwrap_err(), lightcraft_meta::XmpError::TooComplex);
        let path = dir.join("a.xmp");
        std::fs::write(&path, &old).unwrap();
        let fresh = lightcraft_meta::write_xmp_lc(&lightcraft_meta::Metadata { rating: Some(3), ..Default::default() }, &[("settings", "{}")]);
        assert!(sidecar_contents(&path, fresh, "t").is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), old);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1, "no backup was made");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
