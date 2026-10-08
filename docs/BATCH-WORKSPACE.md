# Batch background removal and reviewed export

Open a folder or several photos, switch to **Cutout**, then choose **Remove
backgrounds**. Select the images and choose a transparent, white or image
background. Existing cutouts are reused; photos without one need an installed
Qwen background removal preset. Preparation runs one image at a time and stores
reviewed copies separately from the editor documents and original files.

## Destination and filenames

Choose **Output folder** in the desktop app and **Choose folder…** to set the
batch destination before processing. You can change it after reviewing results.
Cancelling the folder picker retains your previous choice. If you leave it
unset, exporting opens the folder picker. **ZIP download** is also available in
the desktop app and is the destination in a browser.

The file naming scheme offers original name plus `-local-image`, original name,
sequential names (`Image-001.png`), or a custom pattern. `{name}` inserts the
source filename without its extension; `{index}` inserts the three-digit
position among the selected exports. `product-{index}-{name}` becomes
`product-001-Photo.png`. The example updates before export. All outputs are PNG;
folder separators and unknown placeholders are rejected. Duplicate names get
numbered suffixes, and existing files are preserved.

## Prepare and review

Choose **Remove backgrounds**, then review each result. Clicking a thumbnail
expands the same Fluent dialog into a large image viewer. **Fit**, **100%** and
**200%**, zoom buttons and wheel zoom control magnification. Drag or use the
arrow keys to pan; `F` fits, `1` shows actual size, and `+`/`-` zoom. **Original**
compares the source. Previous/next buttons move between prepared images. The
**X** or **Escape** returns to the batch and restores focus to the thumbnail.
Uncheck results that should not be exported.

If images contain a pending selection or unfinished pen path, return to apply
it, or explicitly check **Use images as they are; keep pending selections**.
Batch preparation never silently applies those selections. Background and
removal settings become fixed after preparation; **New batch** starts another
set. Output destination and filename scheme can still be changed before export.

The viewer replaces the contents of the batch dialog so there is one modal
focus context. An inline expansion competes with settings and history for image
space; a side panel is narrower; a large dialog gives the image most of the
window while retaining familiar Fluent controls. This follows
[Fluent dialog guidance](https://fluent2.microsoft.design/components/web/react/core/dialog/usage)
on focused tasks and avoiding nested dialogs, and the
[WAI-ARIA modal dialog pattern](https://www.w3.org/WAI/ARIA/apg/patterns/dialog-modal/)
on Escape and returning focus to the invoking control.

## Export, cancel and resume

**Export reviewed to folder** writes selected copies to the chosen directory.
For ZIP, choose **Export reviewed as ZIP**, then **Download ZIP**. Source credits
are included as companion text files when required. ZIP exports include a
report of per-image status, filenames, credits and precision.

If a document changes after review, its export reports **Edits changed**. Create
and review a new batch for those edits. The original images and saved projects
are preserved. **Cancel batch** stops after the current image and retains
completed work. **Resume** continues an interrupted batch without replacing
completed outputs. Problems with individual photos are reported in the list.

## Storage and cleanup

Expand **Previous batches** to reopen a prepared or interrupted batch. Each
batch has a 12 GiB cache limit. **Clear batch** asks before removing that batch's
previews, snapshots and cached export copies. It preserves original files,
editor documents, saved projects and files exported to a chosen folder.
Download ZIPs before clearing their cache.

Related: [Cutout](CUTOUT-WORKSPACE.md), [model setup](GEN-MODELS.md),
[stock credits](STOCK-LIBRARY.md), [installation and storage](INSTALLATION.md).
