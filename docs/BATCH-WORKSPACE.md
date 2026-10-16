# Batch treatments and reviewed export

Open a folder or several photos, then choose **File > Batch treatment & export…**.
This window keeps preparation, review and export together. It processes up to 100
selected images per queue, one at a time. The editor's documents and original
files stay separate from the queue's copies.

## Save a product treatment

Prepare one product in Cutout: refine its mask, choose a background, position the
subject and adjust its shadow. Open the batch window, choose an export format,
then **Save current treatment…**, enter a name and choose **Save treatment**.

A treatment stores the copied background, subject placement, rotation, feather,
shadow and export format. Placement and distances are normalized to the source
canvas so they can be reused across different photo dimensions. Each product
uses its own alpha mask and foreground. The treatment does not copy a product,
its selection or its repair layers onto other photos. Different subject shapes
can still need individual adjustment after a treatment is applied.

Choose **Current edits only** to export each image's own applied layers and
cutout composition instead. **Remove treatment** removes the saved preset after
confirmation; prepared queues and image documents retain their existing copies.

## Prepare and review

1. Select the photos to include and choose a treatment or **Current edits only**.
2. Choose the export format. PNG and lossless WebP can keep alpha; JPEG needs an
   opaque result. **Original precision & format** follows each photo's source
   format. TIFF can preserve a 16-bit source; PNG, WebP and JPEG exports are 8-bit.
3. If a treatment needs cutouts that are missing, optionally enable **Prepare
   missing cutouts with Qwen** and choose an installed INT8 or BF16 preset. This
   is an explicit AI operation, using the connected ComfyUI service. Existing
   cutouts are retained. Without it, missing cutouts are reported for attention.
4. Choose **Prepare selected**. Wait for the sequential preparation to finish,
   then inspect the preview and status of every photo.
5. Click a preview for a full-size inspection. Use **Fit**, **100%** or **200%**,
   scroll around the image and use **Original** to compare the source. **Back to
   images** returns to the queue. Uncheck results that should not be exported.

If selected photos contain an unapplied selection or unfinished pen path, the
window names them. Choose **Return to apply selection** to finish that work, or
explicitly check **Use applied pixels only; keep pending selections**. Preparing
and exporting never applies a pending brush selection silently.

Once prepared, the queue's treatment, format and cutout settings are fixed for
its previews and exports. They remain visible in the queue summary. Choose
**New queue…** to use different settings. Later edits to a saved treatment do
not alter a prepared queue.

## Export, cancel and resume

Choose **Export reviewed to folder…** in the desktop app. The native folder picker
chooses the destination; the app creates unique image filenames there. In the web
editor, choose **Export reviewed as ZIP**, then **Download ZIP** when ready.
Required source credits are included as companion text files. A ZIP also contains
an export report with per-image status, filenames, credits and precision.

The queue never overwrites original images or saved projects. Preparation and
export use snapshots rather than replacing the documents open in the editor.
If a source document changes after review, its export reports **Edits changed**
instead of exporting a result that no longer matches the reviewed document.
Create and review a new queue for those edits.

**Cancel batch** stops after the current image and keeps completed work.
**Resume** continues an interrupted queue without replacing completed outputs.
If the app exits unexpectedly, the queue returns as interrupted and waits for
explicit resume. A problem with one photo is shown as **Needs attention** or
**Cutout needed**, while other photos can complete. Inspect that photo, correct
its source or settings and create another queue as needed.

## Storage and cleanup

Expand **Previous queues & cache** to reopen a prepared or interrupted queue.
The displayed totals cover owned batch copies and saved treatments. Each queue
has a 12 GiB storage limit; large images and export copies can reach that limit
before 100 photos.

**Clear queue cache** asks before removing that queue's previews, snapshots and
cached export copies. It preserves original files, open editor documents, saved
projects and files already exported to a chosen folder. Save or download outputs
you want to keep before clearing their queue. Treatments have their own copied
backgrounds and can be retained independently of queue caches.

Related: [Cutout](CUTOUT-WORKSPACE.md), [generated image library](GENERATION-LIBRARY.md),
[stock credits](STOCK-LIBRARY.md), [installation and storage](INSTALLATION.md).
