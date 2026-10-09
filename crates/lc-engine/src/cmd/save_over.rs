//! Save Over Original: the one explicit way to replace a photo's original with its edited render
//! (e.g. after removing some junk from a JPEG). Everything else LightCraft writes is kept off the
//! originals by [`crate::originals::OriginalGuard`]; this command bypasses it for exactly one
//! file, the photo's own, and only with `confirm: true`.
//!
//! What it does, in order:
//! 1. render the photo at full size through the export path, in the original's format (JPEG at
//!    quality 95 with the original's chroma subsampling, PNG / TIFF / WebP at the
//!    original's bit depth), in the original's colour space, with the photo's metadata;
//! 2. copy the original into the library's `originals-backup/` folder (written, synced and read
//!    back before anything is replaced);
//! 3. replace the original atomically (a temp file in the same folder renamed over it);
//! 4. reload the photo (new size, content hash, caches and thumbnail: `photo.reload`) and reset its
//!    develop settings to defaults (the edits are in the pixels now; keeping them would apply them
//!    twice), clearing its AI patches, spots and masks with them, its history and its
//!    versions (their settings describe the old pixels too).
//!
//! The file change can't be undone (only the backup brings the old file back), so the photo's
//! undo steps are dropped: undoing an earlier edit would re-apply it on top of the saved pixels.
//!
//! Raw files (DNG included) and formats LightCraft can't write (HEIC, PSD, …) are never replaced:
//! for them the command writes `<name>.jpg` beside the original (a free name, never over another
//! file), and by default adds it to the library stacked on top of the original — the same
//! sequence as Edit in External Editor. `beside: true` asks for that copy for any photo (in the
//! original's own format when it can be written).

use std::path::{Path, PathBuf};

use lightcraft_catalog::{MediaKind, Op, PhotoId, Source};
use lightcraft_codecs::{ChromaSubsampling, Format};
use serde_json::{Value, json};

use super::{CommandSpec, always, bad, cmd};
use crate::export::{Conflict, Destination, ExportFormat, ExportOptions, SourceEncoding};
use crate::{Result, Session};

const C: &str = "photo.saveOverOriginal";

/// The folder (inside the library) that keeps a copy of every original replaced.
pub const BACKUP_FOLDER: &str = "originals-backup";

/// What Save Over Original does with one photo.
struct Plan {
    id: PhotoId,
    /// The original's path.
    path: String,
    file_name: String,
    /// The original's format, when it can be re-encoded in place; `None` (raw, DNG, HEIC…): a
    /// JPEG is written beside it instead.
    format: Option<ExportFormat>,
    /// Virtual copies sharing the file (their edits then apply on top of the saved pixels).
    copies: usize,
    edited: bool,
}

/// The format a sniffed original is re-encoded in (`None`: raws and formats we can't write).
fn writable(format: Option<Format>) -> Option<ExportFormat> {
    match format? {
        Format::Jpeg => Some(ExportFormat::Jpeg),
        Format::Png => Some(ExportFormat::Png),
        Format::Tiff => Some(ExportFormat::Tiff),
        Format::WebP => Some(ExportFormat::Webp),
        _ => None,
    }
}

fn format_label(f: ExportFormat) -> &'static str {
    match f {
        ExportFormat::Jpeg => "JPEG",
        ExportFormat::Png => "PNG",
        ExportFormat::Tiff => "TIFF",
        ExportFormat::Webp => "WebP",
        ExportFormat::Avif => "AVIF",
        ExportFormat::Dng => "DNG",
        ExportFormat::Original => "Original",
    }
}

/// The original's bytes (through the session's reader when it has one).
fn read_original(s: &Session, path: &str) -> std::result::Result<Vec<u8>, String> {
    match s.media.file_bytes.as_ref() {
        Some(r) => r(path),
        None => std::fs::read(path).map_err(|e| format!("{path}: {e}")),
    }
}

/// The first bytes of the original (enough to tell its format).
fn read_head(s: &Session, path: &str) -> std::result::Result<Vec<u8>, String> {
    if s.media.file_bytes.is_some() {
        return read_original(s, path);
    }
    use std::io::Read;
    let mut f = std::fs::File::open(path).map_err(|e| format!("{path}: {e}"))?;
    let mut head = Vec::new();
    f.by_ref().take(64 * 1024).read_to_end(&mut head).map_err(|e| format!("{path}: {e}"))?;
    Ok(head)
}

fn plan(s: &Session, p: &Value) -> Result<Plan> {
    let id = match p.get("id") {
        Some(id) => PhotoId(id.as_u64().ok_or_else(|| bad(C, "`id` must be a photo ID"))?),
        None => s.active().ok_or_else(|| bad(C, "no photo selected"))?,
    };
    let ph = s.catalog.photo(id).ok_or_else(|| bad(C, "no such photo"))?;
    let Source::File { path } = &ph.source else {
        return Err(bad(C, "a generated demo photo has no original file to save over"));
    };
    if ph.copy_of.is_some() {
        return Err(bad(C, "this is a virtual copy: its original belongs to the master photo (export the copy instead)"));
    }
    if ph.kind == MediaKind::Video {
        return Err(bad(C, "videos can't be saved over"));
    }
    let head = read_head(s, path).map_err(|e| bad(C, e))?;
    let raw = ph.kind == MediaKind::Raw || ph.format.eq_ignore_ascii_case("DNG");
    let format = if raw { None } else { writable(lightcraft_codecs::sniff(&head)) };
    let copies = s.catalog.photos().filter(|c| c.copy_of == Some(id)).count();
    Ok(Plan { id, path: path.clone(), file_name: ph.file_name.clone(), format, copies, edited: ph.is_edited() })
}

/// Where originals are backed up: the library's `originals-backup/` (or, for a library that
/// isn't on disk, the system temp folder; `true` = temporary).
fn backup_root(s: &Session) -> (PathBuf, bool) {
    match s.library.as_ref().filter(|l| l.on_disk) {
        Some(l) => (l.dir.join(BACKUP_FOLDER), false),
        None => (std::env::temp_dir().join("local-image-originals-backup"), true),
    }
}

fn plan_json(s: &Session, pl: &Plan) -> Value {
    let (root, temporary) = backup_root(s);
    let mut warnings = Vec::new();
    if pl.copies > 0 {
        warnings.push(format!(
            "{} virtual cop{} of this photo use{} the same file: their edits will apply on top of the saved pixels.",
            pl.copies,
            if pl.copies == 1 { "y" } else { "ies" },
            if pl.copies == 1 { "s" } else { "" }
        ));
    }
    if pl.format == Some(ExportFormat::Webp) {
        warnings.push("WebP is saved lossless (larger than a lossy original).".into());
    }
    if temporary {
        warnings.push("This library isn't on disk: the backup goes to the system's temporary folder.".into());
    }
    let beside_name = Path::new(&pl.file_name).file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    json!({
        "id": pl.id.0,
        "path": pl.path,
        "fileName": pl.file_name,
        "mode": if pl.format.is_some() { "overwrite" } else { "beside" },
        "format": pl.format.map(format_label),
        "besideName": format!("{beside_name}.{}", pl.format.unwrap_or(ExportFormat::Jpeg).extension()),
        "backupDir": root.to_string_lossy(),
        "backupTemporary": temporary,
        "copies": pl.copies,
        "edited": pl.edited,
        "warnings": warnings,
    })
}

/// JPEG chroma subsampling from the frame header (`None` when it isn't a plain YCbCr JPEG).
pub fn jpeg_chroma(bytes: &[u8]) -> Option<ChromaSubsampling> {
    if bytes.get(..2)? != [0xFF, 0xD8] {
        return None;
    }
    let mut i = 2;
    while i + 4 <= bytes.len() {
        if bytes[i] != 0xFF {
            return None;
        }
        let marker = bytes[i + 1];
        if marker == 0xFF {
            i += 1; // fill byte
            continue;
        }
        let len = u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]) as usize;
        if len < 2 {
            return None;
        }
        // SOF0..SOF15 except DHT (C4), JPG (C8) and DAC (CC)
        if (0xC0..=0xCF).contains(&marker) && !matches!(marker, 0xC4 | 0xC8 | 0xCC) {
            let sof = bytes.get(i + 4..i + 2 + len)?;
            let comps = *sof.get(5)? as usize;
            if comps != 3 {
                return None;
            }
            let factors = |c: usize| sof.get(6 + c * 3 + 1).map(|f| (f >> 4, f & 0x0F));
            let (y, cb, cr) = (factors(0)?, factors(1)?, factors(2)?);
            if cb != cr || cb != (1, 1) {
                return (y == cb).then_some(ChromaSubsampling::S444);
            }
            return Some(match y {
                (1, 1) => ChromaSubsampling::S444,
                (2, 1) => ChromaSubsampling::S422,
                (2, 2) => ChromaSubsampling::S420,
                _ => return None,
            });
        }
        if marker == 0xDA {
            return None; // scan before any frame header
        }
        i += 2 + len;
    }
    None
}

/// The export options that re-encode `bytes` (the original, `format`) like it was: full size,
/// its colour space and bit depth, JPEG quality 95 with its chroma subsampling, all metadata.
fn options_like(bytes: &[u8], format: ExportFormat) -> Result<ExportOptions> {
    let decoded = lightcraft_codecs::decode(bytes, lightcraft_codecs::DecodeOptions::fit(64, 64)).map_err(|e| bad(C, e.to_string()))?;
    let profile = match decoded.icc.clone() {
        Some(profile) => {
            if !matches!(lightcraft_codecs::icc::parse(&profile).map(|p| p.kind), Some(lightcraft_codecs::icc::IccKind::MatrixTrc { .. })) {
                return Err(bad(C, "the original colour profile cannot be preserved by this encoder; export a copy instead"));
            }
            profile
        }
        None => {
            let named = decoded.space.named.ok_or_else(|| bad(C, "the original colour space is unknown; export a copy instead"))?;
            let curves = decoded.space.trc.as_ref().ok_or_else(|| bad(C, "the original encoding curves are unknown"))?;
            lightcraft_codecs::icc::write_matrix_trc(&named.rgb_space(), &curves[0])
        }
    };
    let depth = Some(match (decoded.bit_depth, decoded.float) {
        (_, true) | (32, _) => 32,
        (d, _) if d > 8 && format == ExportFormat::Avif => 10,
        (d, _) if d > 8 => 16,
        _ => 8,
    });
    let depth = depth.filter(|d| ExportOptions::bit_depths(format).iter().any(|(v, _)| v == d));
    Ok(ExportOptions {
        format,
        quality: 95,
        resize: None,
        source_encoding: Some(Box::new(SourceEncoding { space: decoded.space, profile })),
        source_metadata: Some(Box::new(lightcraft_meta::extract(bytes))),
        bit_depth: depth,
        jpeg_chroma: if format == ExportFormat::Jpeg { jpeg_chroma(bytes) } else { None },
        metadata: crate::export::MetadataPolicy::All,
        naming: "{name}".into(),
        conflict: Conflict::Unique,
        ..Default::default()
    })
}

/// `2026-09-30T12:00:00` → `2026-09-30T12-00-00` (a folder name on every system).
fn stamp(clock: &str) -> String {
    clock.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '-' }).collect()
}

/// Undo / redo steps that change photo `id` (they would bring back settings made for the old pixels).
fn touches(op: &Op, id: PhotoId) -> bool {
    let mut hit = false;
    op.photo_ids(&mut |p| hit |= p == id);
    hit
}

fn overwrite(s: &mut Session, pl: &Plan, format: ExportFormat) -> Result<Value> {
    let _ = s.end_interaction();
    let original = read_original(s, &pl.path).map_err(|e| bad(C, e))?;
    let opts = options_like(&original, format)?;
    s.media.forget(pl.id);
    // 1. render (nothing is touched if this fails)
    let rendered = crate::export::prepare_export(s, pl.id, &opts, 1).and_then(|e| e.run()).map_err(|e| bad(C, e))?;
    if read_original(s, &pl.path).map_err(|e| bad(C, e))? != original {
        return Err(bad(C, "the original changed while rendering; reload the photo and try again"));
    }
    // 2. back up the original: written, synced and read back before the original is replaced
    let (root, temporary) = backup_root(s);
    let folder = root.join(format!("{}-{}", stamp(&(s.clock)()), pl.id.0));
    std::fs::create_dir_all(&folder).map_err(|e| bad(C, format!("{}: {e}; the original is unchanged", folder.display())))?;
    let name = Path::new(&pl.file_name).to_path_buf();
    let (stem, ext) = (
        name.file_stem().map(|x| x.to_string_lossy().to_string()).unwrap_or_else(|| "original".into()),
        name.extension().map(|x| format!(".{}", x.to_string_lossy())).unwrap_or_default(),
    );
    let mut names = (1u32..).map(|n| if n == 1 { folder.join(format!("{stem}{ext}")) } else { folder.join(format!("{stem}-{n}{ext}")) });
    let backup = lightcraft_catalog::safe_file::write_new_unique(&mut names, &original)
        .map_err(|e| bad(C, format!("couldn't back up the original ({e}); it is unchanged")))?;
    drop(original);
    // 3. replace the original (through a link: the file it points to) atomically
    let target = std::fs::canonicalize(&pl.path).unwrap_or_else(|_| PathBuf::from(&pl.path));
    lightcraft_catalog::safe_file::write_atomic(&target, &rendered.bytes)
        .map_err(|e| bad(C, format!("{}: {e}; the original is unchanged (backup: {})", pl.path, backup.display())))?;
    // 4. the library catches up: new content, settings back to defaults (the edits are baked in)
    super::convert::reload(s, &json!({"ids": [pl.id.0]}))?;
    let info = s.source_info(pl.id);
    let fresh = lightcraft_develop::DevelopSettings {
        wb: lightcraft_develop::WhiteBalance { mode: lightcraft_develop::WbMode::AsShot, temp: info.as_shot_temp, tint: info.as_shot_tint },
        ..Default::default()
    };
    let label = "Save Over Original";
    let fresh = std::sync::Arc::new(fresh);
    let step = lightcraft_catalog::HistoryStep { label: label.into(), settings: fresh.clone() };
    let ops = vec![
        Op::SetDevelop { id: pl.id, settings: fresh, label: label.into(), edited: Some((s.clock)()) },
        Op::SetHistory { id: pl.id, history: vec![step] },
        Op::SetVersions { id: pl.id, versions: Vec::new() },
    ];
    s.commit(label, Op::Batch { ops })?;
    s.media.forget(pl.id);
    // the file change can't be undone: drop the steps that would re-apply old settings
    s.undo.retain(|e| !touches(&e.op, pl.id));
    s.redo.retain(|e| !touches(&e.op, pl.id));
    // A manually written sidecar must not bring the baked edits back on Read Metadata.
    if !s.xmp.auto_write && crate::sidecar::find_sidecar(&pl.path, s.sidecar_naming(pl.id)).is_some() {
        s.save_sidecar(pl.id)
            .map_err(|e| bad(C, format!("the photo was saved (backup: {}), but its reset XMP could not be written: {e}", backup.display())))?;
    }
    Ok(json!({
        "mode": "overwrite",
        "id": pl.id.0,
        "path": pl.path,
        "format": format_label(format),
        "backup": backup.to_string_lossy(),
        "backupTemporary": temporary,
        "width": rendered.width,
        "height": rendered.height,
        "bytes": rendered.bytes.len(),
    }))
}

/// Write the render beside the original (`<name>.<ext>`, a free name), optionally added to the
/// library stacked on top of the original.
fn beside(s: &mut Session, pl: &Plan, p: &Value) -> Result<Value> {
    let _ = s.end_interaction();
    let opts = match pl.format {
        Some(f) => options_like(&read_original(s, &pl.path).map_err(|e| bad(C, e))?, f)?,
        // a raw (or a format we can't write): a high-quality sRGB JPEG
        None => ExportOptions { format: ExportFormat::Jpeg, quality: 95, naming: "{name}".into(), conflict: Conflict::Unique, ..Default::default() },
    };
    let dir = Path::new(&pl.path).parent().map(|d| d.to_string_lossy().to_string()).unwrap_or_default();
    let mut written = Vec::new();
    let mut write = |path: &str, bytes: &[u8]| -> std::result::Result<(), String> {
        // durable: it becomes a library photo
        lightcraft_catalog::safe_file::write_new(Path::new(path), bytes).map_err(|e| format!("{path}: {e}"))?;
        written.push(path.to_string());
        Ok(())
    };
    let to = Destination { dir, exact: None };
    // the export guard stays on: never over the original or any other catalogued file
    crate::export::export_batch(s, &[pl.id], &opts, &to, &mut write, &|path| Path::new(path).exists()).map_err(|e| bad(C, e))?;
    let out = written.into_iter().next().ok_or_else(|| bad(C, "nothing was written"))?;
    let flag = |k: &str| p.get(k).and_then(Value::as_bool).unwrap_or(true);
    let mut new = None;
    if flag("import") {
        let r = s.execute("library.import", &json!({"paths": [out]}))?;
        new = r["imported"].get(0).and_then(Value::as_u64);
        if let Some(n) = new
            && flag("stack")
        {
            let _ = s.execute("stack.group", &json!({"ids": [n, pl.id.0], "top": n, "collapsed": false}));
        }
    }
    Ok(json!({"mode": "beside", "id": new, "original": pl.id.0, "path": out, "format": format_label(opts.format)}))
}

fn save_over(s: &mut Session, p: &Value) -> Result<Value> {
    let pl = plan(s, p)?;
    if p.get("confirm").and_then(Value::as_bool) != Some(true) {
        return Err(bad(
            C,
            "replacing an original needs `confirm: true` (see photo.saveOverOriginalPlan for what will happen); normal exports never write over an original",
        ));
    }
    let forced_beside = p.get("beside").and_then(Value::as_bool).unwrap_or(false);
    match pl.format {
        Some(f) if !forced_beside => overwrite(s, &pl, f),
        _ => beside(s, &pl, p),
    }
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            query "photo.saveOverOriginalPlan",
            "Save Over Original: Plan",
            [],
            None,
            "{id?} — what photo.saveOverOriginal would do with the photo (default: the active one), without doing it → {id, path, fileName, mode: overwrite | beside (raws, DNG and formats that can't be written), format, besideName, backupDir, backupTemporary, copies, edited, warnings}",
            always,
            |s, p| {
                let pl = plan(s, p)?;
                Ok(plan_json(s, &pl))
            }
        ),
        cmd!(
            "photo.saveOverOriginal",
            "Save Over Original",
            [],
            None,
            "{id?, confirm: true, beside?: false, import?: true, stack?: true} — replace the photo's original (JPEG, PNG, TIFF, WebP) with its full-size render in the same format, colour space and bit depth, after backing the original up into the library's originals-backup folder; then reload it and reset its edits (they are in the pixels now). Not undoable (the backup is the way back). Raws/DNG/HEIC/AVIF, or beside: true: write `<name>.jpg` (or the original's format) beside it instead and add it stacked on the original → {mode: overwrite, path, backup, format, width, height} | {mode: beside, path, id, original}",
            always,
            save_over
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chroma_of_our_own_jpegs() {
        let img = lightcraft_raster::Rgba8::from_fn(32, 16, |x, y| [(x * 7) as u8, (y * 9) as u8, 90, 255]);
        let e = lightcraft_codecs::EncodeImage::rgba8(&img);
        for sub in [ChromaSubsampling::S444, ChromaSubsampling::S422, ChromaSubsampling::S420] {
            let b = lightcraft_codecs::encode_jpeg(&e, 90, sub, &Default::default()).unwrap();
            assert_eq!(jpeg_chroma(&b), Some(sub));
        }
        assert_eq!(jpeg_chroma(b"not a jpeg"), None);
    }

    #[test]
    fn stamps_are_folder_names() {
        assert_eq!(stamp("2026-09-30T12:00:00"), "2026-09-30T12-00-00");
    }
}
