# Import safety, catalog backups and proof sheets

Import Photos offers **Build previews: Embedded / Standard / 1:1 / Smart**. Embedded builds the
grid thumbnails using the existing embedded-preview source. Standard and 1:1 build developed
loupe previews; Smart builds the offline-editable proxies. Building starts in the background
for the photos successfully imported; the existing preview progress/cancel controls apply.
The last import options are remembered.

Enable **Make a second copy to…** and choose an existing folder on your backup drive. The
original file and its XMP sidecars are copied independently (never hard-linked to the source),
synced and read back before that photo imports. Filename collisions get a unique suffix.
Copy as DNG still backs up the original raw. A failed second copy leaves that photo unimported
and its source untouched, including in Move mode. Import reports the failed photo. Duplicate
photos that the catalog already contains are skipped as usual.

Preferences > General > **Catalog backup on quit** offers Never, Every quit, Daily and Weekly,
a folder and the number to keep. Weekly and five backups are the default. An empty folder choice
uses `Backups/` inside the library; select another drive for protection against drive failure.
A chosen folder must already exist: an unavailable backup drive is reported instead of silently
creating a replacement on the current drive. A quit-time failure is also saved in library preferences
and shown in the backup controls on the next launch; a successful backup clears it. Daily/weekly intervals are measured from the last
published backup of this library. **Back up now** bypasses the interval.

After saving and finishing its closing snapshot, the application copies `catalog.snap` and
`catalog.log` through the shared durable writer into a hidden staging directory. Both files are
compared with their sources and replayed in memory. Only then is the complete directory atomically
published as `catalog-backup-<timestamp>-<unique id>/`. Older backups of this library are removed
only after successful publication. Interrupted hidden `.catalog-backup-*.tmp` directories are
not backups; they can be removed after the app has exited. Backups include catalog edits and
metadata; original photos, presets, preferences and smart previews need separate backups.

**Verify / Optimize library** finishes any background snapshot, verifies the saved snapshot/log
pair in memory without repairing or rewriting the original bytes, then compacts a valid catalog
into a fresh snapshot. Damage is reported and optimization is refused.

## Restore from backup

1. Quit Local Image so its library lock is released. Preserve the damaged library and originals.
2. Choose a published `catalog-backup-*` directory with both `catalog.snap` and `catalog.log`
   and `backup.json`. Ignore hidden staging directories.
3. Copy both catalog files together into a **new, empty restore library directory**. Do not mix
   the snapshot from one backup with the log from another. Keep the original backup untouched.
4. Open the restore directory with Preferences > General > Open Library. Existing absolute
   photo paths reconnect automatically. If the original photos moved, use Find Missing Photos
   to relink them. Copy original photos from their separate backup if necessary.
5. Check representative photos, ratings, keywords, albums and Develop edits; run Verify / Optimize
   library. Restore presets/preferences separately if needed, then choose a backup folder again.

File > **Export Proof Sheet…** exports the selection in grid order (or the active photo, matching Export) as
a PDF. Choose A4, Letter or A3, portrait/landscape, rows/columns, filename/title/caption metadata,
and optional text watermark. Photos retain their aspect ratio and use the full-quality,
color-managed Develop export renderer, with a 300 dpi cell target and lossless sRGB PNG embedding.
Captions use the existing text engine and vector outlines, with up to three lines and an ellipsis;
fonts and shaping follow the app's available fonts. The worker uses the shared memory admission
policy and renders one photo at a time. If retained PDF data would exceed current memory headroom,
export stops with a request to use a smaller selection, preserving print quality. Export is cancellable and never overwrites an existing
file. No partial PDF becomes visible after a failed write.

The workflow and terminology follow Adobe's official
[import options](https://helpx.adobe.com/lightroom-classic/desktop/import-photos/photo-video-import-options.html),
[catalog backups](https://helpx.adobe.com/lightroom-classic/help/back-catalog.html), and
[contact-sheet layouts and photo information](https://helpx.adobe.com/lightroom-classic/help/print-module-layouts-templates.html).
