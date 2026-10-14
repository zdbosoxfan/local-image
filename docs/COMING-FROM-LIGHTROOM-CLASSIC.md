# Coming from Lightroom Classic

Start in **Library**, develop a selected photograph in **Develop**, then use **Compositing**
for layers and retouching. The module picker is in the title bar; the default module shortcuts
are Ctrl+Alt+1/2/3 on Linux and Option+Command+1/2/3 on macOS. Choose the Lightroom Classic
keyboard profile in **Preferences → Keyboard Shortcuts** if you want familiar bindings.
The shortcut sheet and menus show the active profile; customized keys take priority.

## Bring photographs, keep originals

Local Image has its own catalog. It does not open `.lrcat` files or translate Adobe's Develop
recipes. Start with a small copied/test folder, use **File → Import Photos…**, or browse a folder
in place from the Library folder sidebar. **Compositing → File → Open Folder…** opens a filmstrip. Keep your Lightroom catalog and originals backed up.
Local Image stores references and its own edits; exporting creates rendered files. Moving or
renaming referenced files outside the app can leave missing photographs.

Lightroom Classic separates the catalog's records/edits from the original photographs, as
[Adobe explains](https://helpx.adobe.com/uk/lightroom-classic/desktop/manage-catalogs-and-files/lightroom-catalog-basics.html).
That is the useful starting model here too. XMP can carry shared metadata such as ratings and
keywords, but a matching XMP filename does not make Adobe's raw adjustments equivalent to Local
Image's. To preserve an existing Lightroom look, export a rendered 16-bit TIFF from Lightroom,
keep the raw beside it, then edit the TIFF in Local Image. Import a small sample first and
check metadata and colour before migrating a whole archive.

## Find and develop

| Lightroom habit | Local Image route |
| --- | --- |
| Browse / Loupe / compare photographs | Library grid, single-photo view, Compare and filmstrip |
| Stars, picks/rejects, labels, keywords | Library rating/flag/label controls and metadata panel |
| Organize sets without moving originals | Collections and smart collections; some existing labels/automation IDs still say Albums |
| Raw tonal and colour edits | Develop's Light, Color, Detail, Optics, Effects and masking tools |
| Apply a look to a group | Copy/Paste Settings or Sync Settings; choose the groups to transfer |
| Deliver JPEGs/TIFFs with presets | Library **File → Export…** with size, metadata, sharpening and presets |
| Find faces or group a burst | People and **File → Smart Sort & Export…** |
| Send to a layered editor | Open the developed photograph in Compositing as a Develop layer |

Most Develop changes remain parameters until export. Supported raw decoding varies by camera
and encoding; a **preview-only** notice means you are editing the embedded preview rather than
full raw sensor data. Inspect that notice before committing to a raw workflow. See
[DEVELOP-TOOLS-SOURCES.md](DEVELOP-TOOLS-SOURCES.md) for pipeline provenance and limits.
Film-negative conversion is an additional tool, not required for ordinary camera raws.
Vignette, grain and B&W apply to the whole image.

Subject, Sky and Background masks can be refined before applying local edits. AI Remove keeps
the same sequence: **brush → adjust → confirm**. Availability depends on the selected engine
and installed model/server. SAM 3 is off in the default release packages. A ComfyUI server may
run locally or on a configured trusted LAN machine; source images are sent there only when you
run its AI operation. Read [MACOS.md](MACOS.md) for remote server setup.

## Layer work and delivery

A Develop layer in Compositing retains the original file reference and Develop settings.
Later Develop edits can refresh the linked layer without an intermediate TIFF. Add raster
layers, masks, type and adjustments above it, then save the layered document as `.pcraft`.
Keep the source files available. A catalog record is not a backup of those source photographs.
Use **Save and Return to Library** for the app's linked round trip; exporting is for a rendered
copy to deliver. This differs from Lightroom's external-editor rendered-file workflow, described
in [Adobe's external editing help](https://helpx.adobe.com/lightroom-classic/desktop/work-with-external-editors/open-lightroom-photos-external-applications.html).

Prefer export to a separate delivery folder. **Save Over Original** is an explicit overwrite
workflow; read its destination and confirmation carefully. Save the editable `.pcraft` document
before exporting a flattened image. For backups and troubleshooting see [BACKUP.md](BACKUP.md)
and [RELEASE.md](RELEASE.md). **Help → Check for Updates…** opens a release page only; startup
checking is an optional General preference and defaults off.
