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

/// The orientation the photo file at `path` states. Most files state it in their first [`ORIENTATION_SPAN`]; when
/// that part doesn't, the Exif may come later ([`late_orientation`]).
fn file_orientation(path: &str) -> Option<lightcraft_meta::Orientation> {
    let mut file = li_fs::read::open_regular(Path::new(path), li_fs::read::MAX_FILE_BYTES).ok()?;
    let mut head = Vec::new();
    (&mut file).take(ORIENTATION_SPAN).read_to_end(&mut head).ok()?;
    if let Some(o) = lightcraft_meta::extract(&head).orientation {
        return Some(o);
    }
    if (head.len() as u64) < ORIENTATION_SPAN {
        return None;
    }
    late_orientation(&mut file, &head)
}

/// The orientation of a file whose `head` doesn't state it, found by the file's structure without reading its image
/// data: a JPEG's Exif can follow other long segments, WebP keeps its EXIF chunk after the image data, and many TIFF
/// writers put the first IFD after the strips.
fn late_orientation(file: &mut (impl Read + Seek), head: &[u8]) -> Option<lightcraft_meta::Orientation> {
    file.rewind().ok()?;
    match head.get(..4)? {
        [0xff, 0xd8, ..] => jpeg_orientation(file),
        b"RIFF" if head.get(8..12) == Some(&b"WEBP"[..]) => webp_orientation(file),
        b"II*\0" => tiff_orientation(file, head, true),
        b"MM\0*" => tiff_orientation(file, head, false),
        _ => None,
    }
}

/// A JPEG's orientation from the first APP1 segment that states one (its Exif, or its XMP), found by walking the
/// segment headers from the start of `file` up to the image data: at most [`JPEG_HEADER_SPAN`] bytes, skipped
/// payloads included, and [`JPEG_MARKER_STEPS`] markers. Only APP1 payloads are read (under 64 KiB each).
fn jpeg_orientation(file: &mut (impl Read + Seek)) -> Option<lightcraft_meta::Orientation> {
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
        if let Some(o) = lightcraft_meta::extract(&segment).orientation {
            return Some(o);
        }
    }
    None
}

const JPEG_HEADER_SPAN: u64 = 16 * 1024 * 1024;
const JPEG_MARKER_STEPS: usize = 4096;

/// A WebP's orientation from its `EXIF` chunk, or else its `XMP ` chunk (which follows it), found by walking the chunk
/// headers from the start of `file` (at most [`WEBP_CHUNK_STEPS`]). Only those chunks are read, each if it is at most
/// [`ORIENTATION_SPAN`].
fn webp_orientation(file: &mut (impl Read + Seek)) -> Option<lightcraft_meta::Orientation> {
    file.seek(SeekFrom::Start(12)).ok()?;
    for _ in 0..WEBP_CHUNK_STEPS {
        let mut header = [0; 8];
        file.read_exact(&mut header).ok()?;
        let [a, b, c, d, s0, s1, s2, s3] = header;
        let size = u32::from_le_bytes([s0, s1, s2, s3]);
        // chunks are padded to an even size
        let mut skip = i64::from(size) + i64::from(size & 1);
        if matches!(&[a, b, c, d], b"EXIF" | b"XMP ") && u64::from(size) <= ORIENTATION_SPAN {
            let mut chunk = b"RIFF\0\0\0\0WEBP".to_vec();
            chunk.extend_from_slice(&header);
            (&mut *file).take(u64::from(size)).read_to_end(&mut chunk).ok()?;
            if let Some(o) = lightcraft_meta::extract(&chunk).orientation {
                return Some(o);
            }
            skip = i64::from(size & 1);
        }
        file.seek(SeekFrom::Current(skip)).ok()?;
    }
    None
}

/// How many chunks [`webp_orientation`] passes over (an animated WebP has one per frame).
const WEBP_CHUNK_STEPS: usize = 4096;

/// A TIFF's orientation from the first IFD that `head` points to, of which only the 12-byte entries are read (at most
/// 65535 of them, under 1 MiB). `le`: little-endian (`II`).
fn tiff_orientation(file: &mut (impl Read + Seek), head: &[u8], le: bool) -> Option<lightcraft_meta::Orientation> {
    let u16_of = |b: [u8; 2]| if le { u16::from_le_bytes(b) } else { u16::from_be_bytes(b) };
    let u32_of = |b: [u8; 4]| if le { u32::from_le_bytes(b) } else { u32::from_be_bytes(b) };
    let ifd = u32_of(<[u8; 4]>::try_from(head.get(4..8)?).ok()?);
    file.seek(SeekFrom::Start(u64::from(ifd))).ok()?;
    let mut count = [0; 2];
    file.read_exact(&mut count).ok()?;
    let mut entries = vec![0; usize::from(u16_of(count)) * 12];
    file.read_exact(&mut entries).ok()?;
    let orientation = entries.as_chunks::<12>().0.iter().find_map(|&[t0, t1, y0, y1, c0, c1, c2, c3, v0, v1, _, _]| {
        // tag 274, exactly one SHORT (with more, the field also holds a second value, or an offset to them all)
        (u16_of([t0, t1]) == 274 && u16_of([y0, y1]) == 3 && u32_of([c0, c1, c2, c3]) == 1).then(|| u16_of([v0, v1]))
    })?;
    (1..=8).contains(&orientation).then(|| lightcraft_meta::Orientation::from_exif(orientation))
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

    /// A WebP keeps its EXIF and XMP chunks after the image data, and a TIFF's first IFD can follow its strips, so
    /// the orientation can be past the first [`ORIENTATION_SPAN`] of the file; EXIF wins over XMP, a chunk over that
    /// span isn't read, a TIFF orientation is exactly one SHORT, and a file that states none has none.
    #[test]
    fn the_file_orientation_is_found_after_the_image_data() {
        use lightcraft_meta::Orientation;
        let dir = std::env::temp_dir().join(format!("lc-file-orientation-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let exif = lightcraft_meta::write_exif(&lightcraft_meta::Metadata { orientation: Some(Orientation::Rotate90), ..Default::default() });
        let span = ORIENTATION_SPAN as usize;
        // a WebP: `image` bytes of image data, then the `meta` chunks
        let webp = |image: usize, meta: &[(&[u8], Vec<u8>)]| {
            let mut w = b"RIFF\0\0\0\0WEBP".to_vec();
            for (id, data) in [(&b"VP8X"[..], vec![0; 10]), (b"VP8L", vec![0; image])].iter().chain(meta) {
                w.extend_from_slice(id);
                w.extend_from_slice(&(data.len() as u32).to_le_bytes());
                w.extend_from_slice(data);
                if data.len() % 2 == 1 {
                    w.push(0);
                }
            }
            w
        };
        let exif_chunk = (&b"EXIF"[..], [&b"Exif\0\0"[..], &exif].concat());
        // an EXIF chunk over the span that is read (orientation 6 too), and an XMP packet stating orientation 8
        let huge_exif = (&b"EXIF"[..], [&b"Exif\0\0"[..], &exif, &vec![0; span]].concat());
        let packet = lightcraft_meta::write_xmp(&lightcraft_meta::Metadata { orientation: Some(Orientation::Rotate270), ..Default::default() }, None);
        let xmp_chunk = (&b"XMP "[..], packet.into_bytes());
        // a TIFF whose first IFD (orientation 8, then a tag pointing elsewhere) follows `strips` bytes of image data
        // (`values` of the orientation tag: more than two are at an offset, here 8)
        let tiff = |le: bool, strips: usize, values: u32| {
            let (u16_of, u32_of): (fn(u16) -> [u8; 2], fn(u32) -> [u8; 4]) =
                if le { (u16::to_le_bytes, u32::to_le_bytes) } else { (u16::to_be_bytes, u32::to_be_bytes) };
            let mut t = if le { b"II*\0".to_vec() } else { b"MM\0*".to_vec() };
            t.extend_from_slice(&u32_of(8 + strips as u32));
            t.resize(8 + strips, 0);
            t.extend_from_slice(&u16_of(2));
            for (tag, kind, count, value) in [(274u16, 3u16, values, [u16_of(8), [0, 0]].concat()), (305, 2, 64, u32_of(4).to_vec())] {
                t.extend_from_slice(&u16_of(tag));
                t.extend_from_slice(&u16_of(kind));
                t.extend_from_slice(&u32_of(count));
                t.extend_from_slice(&value);
            }
            t.extend_from_slice(&[0; 4]);
            t
        };
        // a JPEG whose first MiB is other segments, then the start of its image data: no Exif
        let mut jpeg = vec![0xff, 0xd8];
        for _ in 0..17 {
            jpeg.extend_from_slice(&[0xff, 0xef, 0xff, 0xff]);
            jpeg.resize(jpeg.len() + 0xfffd, 0);
        }
        jpeg.extend_from_slice(&[0xff, 0xda, 0, 2, 0xff, 0xd9]);
        for (name, bytes, want) in [
            ("small.webp", webp(64, std::slice::from_ref(&exif_chunk)), Some(Orientation::Rotate90)),
            ("large.webp", webp(span + 64, &[exif_chunk.clone(), xmp_chunk.clone()]), Some(Orientation::Rotate90)),
            ("none.webp", webp(span + 64, &[exif_chunk])[..span + 100].to_vec(), None),
            ("xmp.webp", webp(span + 64, std::slice::from_ref(&xmp_chunk)), Some(Orientation::Rotate270)),
            ("huge.webp", webp(span + 64, &[huge_exif, xmp_chunk]), Some(Orientation::Rotate270)),
            ("le.tif", tiff(true, span + 64, 1), Some(Orientation::Rotate270)),
            ("be.tif", tiff(false, span + 64, 1), Some(Orientation::Rotate270)),
            ("offset.tif", tiff(true, span + 64, 3), None),
            ("none.jpg", jpeg, None),
        ] {
            let path = dir.join(name);
            std::fs::write(&path, bytes).unwrap();
            assert_eq!(file_orientation(&path.to_string_lossy()), want, "{name}");
        }
        assert_eq!(file_orientation(&dir.join("missing.webp").to_string_lossy()), None);
        std::fs::remove_dir_all(dir).unwrap();
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
