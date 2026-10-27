# Coming from Photoshop / Affinity

Choose **Compositing** in the title bar for the layered editor. Tools sit on the left, contextual
options above the canvas, and dockable panels on the right. **Window** restores/shows panels;
**Preferences → Keyboard Shortcuts** selects Photoshop or Affinity bindings and lets you
customize them. The active profile's keys appear in menus and the shortcut sheet. Linux uses
Ctrl where macOS usually uses Command. The Photo persona focuses on raster editing; the Vector
persona exposes vector tools and Object commands.

## Keep a layered master

Open a photo, PSD, SVG or PDF with **File → Open…** and inspect the result before resaving.
Local Image can read and write PSD, but Photoshop, Affinity and Local Image have different
feature models. Complex smart objects, proprietary effects and imported vector/text features
can be approximated or rasterized. Read any import warning and compare a rendered reference.
Keep the original PSD or Affinity file and work on a copy; do not use an interchange file as
your sole master. An exported PSD is useful for collaboration, not a guarantee of lossless
round trips. Native Affinity import preserves editable shapes, decorated text, table text and grid lines,
master-page content, embedded native documents, masks and supported layer styles. Mixed text
outlines become companion vector layers. CMYK pixels use the embedded profile or the built-in
coated CMYK profile; vector CMYK colours still warn. Bevels, nonuniform effect transforms,
gradient projections, unavailable fonts and unsupported records can differ: inspect the import
warnings and compare the original. HEIC originals and some text metrics remain unsupported.
Keep PSD or rendered TIFF as a fallback for documents outside this subset.

Save Local Image's editable master as **`.pcraft`**. Export a PNG, JPEG or TIFF for delivery.
Save As and Export serve different purposes: a JPEG cannot preserve your editable layers,
masks and adjustments. If colour matters, review the document profile, bit depth, export profile
and display configuration rather than judging an untagged image in another viewer.

## Familiar editing routes

| Photoshop / Affinity habit | Local Image route |
| --- | --- |
| Layers, groups, clipping and blend modes | Layers panel and Layer menu; select the layer before editing |
| Nondestructive tone/colour corrections | Adjustment layers and masks; Develop for raw parameters |
| Clone, heal, remove a distraction | Clone/Heal tools and AI Remove; inspect the target layer and sampling options |
| Marquee, lasso, magic wand | Selection tools and Select menu, then refine/mask as needed |
| Free transform / shape nodes | Transform tools for raster; Vector persona for paths, shapes and nodes |
| Brush settings and presets | Brush Settings and Brushes panels; options bar for the active tool |
| Embedded/linked raw work | A Develop layer linked to Library/Develop settings |
| Keyboard workflow | Choose a profile, review the shortcut sheet, customize conflicts |

Layers and adjustment/mask-based editing are familiar from
[Adobe's nondestructive editing guide](https://helpx.adobe.com/photoshop/using/nondestructive-editing.html).
Local Image follows that general workflow; it does not promise that every Photoshop Smart
Object or Smart Filter behaves identically. Affinity similarly uses a layer stack, whose order
determines visible content, documented in its [layer ordering help](https://affinity.help/photo2/English.lproj/pages/LayerOperations/order.html).
Use the layer thumbnails, visibility controls and undo history to check your target before a
large edit. Native support details live in [TOOLSET.md](TOOLSET.md).

## Raw development and local AI

Use **Library** for folders, ratings and catalog organization, **Develop** for raw adjustments,
and **Compositing** for pixel/layer work. A photograph opened from Develop as a Develop layer
keeps a live link to its original and settings. This replaces an intermediate rendered copy for
that in-app route. Retain the referenced original and a catalog backup alongside the `.pcraft`
master. For workflow details see [Coming from Lightroom Classic](COMING-FROM-LIGHTROOM-CLASSIC.md).

AI Remove is **brush → adjust → confirm**. Paint the distraction, refine the area/settings, and
review before applying. AI Cutout removes a background with an alpha matte; Generative Fill and
the Generate panel use installed ComfyUI workflows/models. Models are separate downloads with
separate licences. The app has no cloud account or per-image service fee. ComfyUI can be local
or a remote server you choose; the latter receives the image/mask only for the requested job.
Do not assume every AI tool is available before configuring its engine. SAM 3 stays off by default.

## Settle in

Try a small test document: add an adjustment, paint on a separate layer, make a mask, undo/redo,
save `.pcraft`, reopen, and export a colour-managed copy. Check your usual PSD exchange against
an exported reference before relying on it for a client job. Linux x86_64 and Apple Silicon
macOS 13+ are the supported release targets; Windows is experimental. See [RELEASE.md](RELEASE.md)
for artifacts and update privacy. **Help → Check for Updates…** offers a release-page link;
automatic startup checks are optional and off by default.
