# Changelog

All notable changes to Local Image are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and version numbers follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html) with alpha and beta pre-release tags.

## [Unreleased]

### Added

- Edit > Convert to Profile can dither 8-bit documents, so a change of profile that lands between
  whole 8-bit steps no longer bands smooth gradients. It is off by default and the same image
  always dithers the same way.
- Layer > Smart Objects > Reset Transform puts a smart object back at 100 % of its contents' own
  size with rotation, skew, Distort and Perspective cleared, centred where it sits, in one undo
  step. Its Warp and smart filters stay, and it re-renders from its contents without losing detail.
- The Match Color dialog has a Source Layer menu: match to one layer of the Source document instead
  of its merged image. Layers with the same name stay separate choices, and choosing another
  Source document goes back to Merged.
- Levels: hold Alt while dragging the black or white Input slider to see what it clips. The canvas
  turns white with the clipped shadows in black or colour (black slider), or black with the clipped
  highlights in white or colour (white slider); release Alt or the mouse to see the preview again.
  The document is never changed. Grayscale shows black and white; CMYK and Lab documents, channel,
  mask and Quick Mask targets, and Smart Filter Levels keep the normal preview.
- Hue/Saturation: drag between a colour range's handles to move them together. The fully-on span
  moves all four, each fade moves its own two; a side stops at the other and the range never grows
  past a full turn. Each drag is one undo step.
- Black & White has an Auto button beside Default, in the Properties panel and in Image >
  Adjustments. It sets the colour weights for the image (what lies under the adjustment layer, or
  the layer's own pixels) so colours the default mix turns into the same grey are told apart, with
  the mildest change that does it. It is one undo step; automation can ask for the weights with
  `image.adjustments.blackWhite.auto`, which changes nothing.

### Changed

- Filter > Other > Frequency Separation now puts its Low and High Frequency layers in a
  "Frequency Separation" group above the source, in one undo step, and rebuilds the source
  exactly at 16 and 32 bits (within one level at 8 bits). It asks for a radius of 0.1 to 1000 px
  (default 4) and is refused on an empty layer, a non-pixel layer, a targeted alpha channel, Quick
  Mask or shown layer mask, and in CMYK and Lab, where the split does not add back up.

### Security

- HEIF decoding bounds metadata parsing, extent-record allocations, item reconstruction,
  transforms and output samples. Alpha-grid tiles share the primary reconstruction budget.
- RAW, HEIF and metadata readers bound more hostile-input work: DNG opcode lists, embedded-preview
  header scans, HEIC parameter sets and decode peak memory, oversized Sony lossless tiles, and
  XML references, IPTC keywords and Luminar look settings read in linear time.

### Added

- Develop: holding Alt (Option on macOS) while dragging an Edit slider shows Lightroom's
  temporary previews: highlight clipping for Exposure, Highlights and Whites, shadow clipping for
  Shadows and Blacks, greyscale for Sharpening Amount and the detail band for Radius and Detail,
  as Masking already showed its mask. Settings and the histogram are unchanged.
- Develop: with Crop, Masking, Remove, Red Eye or a picker on the photo, holding Space lends the
  pointer to the view, as Lightroom's temporary Hand: a click zooms to the click-zoom ratio at the
  point clicked (or back to Fit), a drag pans the zoomed photo, and the tool gets neither. A quick
  tap still toggles the zoom, on its release.
- Develop: with Show Pins on, Object and Describe masks show a pin on the photo: an Object at its
  first click, a Describe mask in the middle of what it selected. Clicking the pin selects the
  mask; dragging it changes nothing and adds no History step.

### Changed

- Develop: Auto straighten levels a sea or landscape horizon among waves and clouds, and levels
  a building shot upward by where its verticals converge instead of by their lean. It turns a
  photo by at most 5° (was 10°).
- Develop: Auto Settings applies to every selected photo, each measured on its own source, as
  one undo step. The photos are decoded in the background, with progress and Cancel in the task
  panel; a photo that can't be read is skipped and counted, and a result that arrives after an
  edit or undo is not applied.

### Fixed

- Develop: holding Space no longer toggles the zoom over and over as the key repeats.
- Image decoding refuses hostile files before they can allocate far past their size: PNG colour
  profiles and dropped text, EXR previews and header sizes, TIFF strip scratch, chunk tables,
  repeated structural tags, predictor rows and JPEG chunks, GIF, BMP, TGA, ICO and QOI header sizes,
  and icon colour profiles. JPEG Extended XMP fragments no longer make opening a file slow.
- TIFF opens YCbCr images as RGB, blank Group 4 fax pages wider than 4096 pixels, and an alpha
  channel that follows another extra channel.
- A raw with damaged sensor data opens in the editor as its camera JPEG, with a warning, instead of failing.
- A raw's embedded preview is read in Adobe RGB when the raw's EXIF says the camera was set to it.
- Incomplete ISO and EXIF timestamps no longer become capture times, so the next valid date is used. A JPEG ICC profile whose chunks are missing, repeated or cut off is dropped from metadata.
- RAW/TIFF imports bound hostile tile, opcode, and maker-note work before it can multiply memory or CPU use. Truncated Deflate tiles are rejected, and non-finite DNG WhiteLevel uses the bit-depth default.
- Answering OK to the "Rasterize?" prompt that a brush or pencil opens on a type, shape, Smart
  Object or fill layer only rasterizes the layer, as in Photoshop. The click that opened the prompt
  no longer paints; the next click does.
- The Magnetic Lasso snaps to the edges of the visible image in an RGB document, whichever layer is
  active; hidden layers add none. Other colour modes still follow the active layer's own pixels.
- Magic Wand and Magic Eraser Anti-alias keep the matched region fully selected and soften only
  the pixels just outside it, by how close their colour is to the tolerance, as in Photoshop. A
  shape next to a clearly different colour or transparency gets a hard edge.
- Select > Subject looks at the selected layer's pixels; it uses the whole image only when no pixel
  layer is selected or Sample All Layers is asked for.
- Select and Mask's Shift Edge moves only soft borders, as in Photoshop: a hard selection edge
  stays where it is, and a feathered edge is pulled in or pushed out along a fixed coverage curve
  (at -100% a half-selected pixel drops to 2%, at +100% it rises to 98%).
- Saving a PSD chooses PSB by the size of the file actually written, not by the uncompressed pixels:
  a document that packs to well under 2 GB stays a PSD. A file over 2 GB is still saved as PSB
  automatically; if it can't be, the save warns.
- Panorama merges refuse more than 256 frames before decoding them at full size.
- A Develop layer in Compositing renders as the Library does: with the photo's AI Denoise result, and without AI Remove patches made from another photo.
- Cropping or rotating a photo in Develop no longer leaves its Compositing document at the old canvas size. A document that is still just the photo takes the new size; one with retouching keeps the photo as it was and says so, so its layers stay lined up.
- Cancelling the unsaved-changes prompt after quitting from Library or Develop goes back to that module instead of leaving Compositing on screen.
- A Camera Raw Filter session keeps its edits when another module is picked or the window is closed: the module switch asks to finish with OK or Cancel first, and cancelling the quit prompt returns to the session.
- A saved `-Edit.psd` selected in the Library opens in Compositing with its layers, also when coming from Develop or after a restart, instead of as a second, flattened document.
- Closing the window from Compositing when the Library couldn't save its catalog now shows the Library's Try Saving Again / Quit Anyway prompt instead of doing nothing.
- Undoing an update from Develop in Compositing lasts: switching modules no longer puts it back. A Library edit made outside a Develop session entered from a retouched document (Sync, Paste, a preset) no longer adds a Develop layer over the retouching; a notice says the Library's edit is newer, and opening Develop from that document brings it in.
- A file opened from the file manager or the command line is shown in Compositing instead of loading out of sight behind the Library, and a folder opened the same way is scanned for the Library's import instead of failing to open.
- Develop's AI Remove keeps what was painted when the removal can't start, Remove is disabled while the chosen engine reports a problem, and a Camera Raw Filter session no longer offers the AI mode it can't run.
- Crash recovery keeps autosaving Compositing documents while Library or Develop is shown, instead of pausing until Compositing is back.
- Export's After Export "Open in Other Application" with no application given opens the exported files in the system's default application, as its hint says, instead of opening every file in Compositing.
- A raw the Library has, opened in Compositing with File › Open, Open Recent, a drop, the filmstrip or the command line, opens as a Develop layer with its Develop edits instead of at camera defaults, and its saves join the Library.
- Documents recovered after a crash are announced as a notice, so the count shows in the Library and Develop as well as in the Compositing status bar.
- Ctrl/Cmd+Alt+1/2/3 no longer switches modules past an open Library dialog, Smart Sort or People, or past a running filter's progress window, which left the job without its Cancel.
- Switching from Library to Compositing keeps the document being worked on in front instead of opening the last photo clicked in Library over it. The Library photo still comes across into an empty Compositing, and Develop still carries its photo.
- Coming back from Compositing keeps the Library selection when it holds the edited photo, which becomes the active one, instead of collapsing it to that photo.
- Opening Info or Keywords from Develop's tool strip keeps Develop in the title bar and as the module to start in, instead of switching both to Library.
- Library Settings › Startup › Open in Photo Grid or Loupe now starts in the Library even when Develop or Compositing was last shown. The remembering choice is now called "Last workspace and view".
- Photo › Edit In… (Cmd+E) with no other application named opens the photo in Compositing as a Develop layer, linked to Develop, instead of writing a stacked 16-bit TIFF copy every time. Naming an application in Settings › External Editor still sends it a TIFF copy.
- A Library export, proof sheet or Develop AI result that finishes while Compositing is shown completes there (Show in Folder, Add to Library and the other After Export steps included) and its message shows as a Compositing notice, instead of waiting until the Library is opened again.
- Develop and Compositing use one AI Remove engine choice: picking an engine in either one sets it in both, and both offer the same engines.
- Preferences opened in Compositing no longer opens the Library's catalog first, so it doesn't wait on a slow or unreachable Library folder. The catalog opens when one of the Library's own pages is shown.
- Cmd+Alt+Shift+K opens Keyboard Shortcuts in Library and Develop as well, the same chord as in Compositing.
- Edit › Preferences › Raw Defaults… opens Library & Develop › Import, where the raw defaults Local Image uses are set, instead of an empty page. The empty Camera Raw Defaults row is gone from the Compositing list.
- Edit › Search in Compositing also finds Library and Develop, and running one switches to that module.
- A Library photo opened in Compositing carries its Library title, caption, copyright, creator, keywords and location in the document's metadata, the saved `-Edit` file's Library entry gets the same title, caption, copyright, creator and location where the file has none of its own, and Export As starts with Metadata: All for it, so a first export no longer leaves them out.
- AI Denoise that can't use the GPU because AI work holds its memory now says so and points to waiting for AI jobs or to Eject in Compositing's status bar, instead of asking to retry after other GPU work finishes.
- Compositing's File › Exit is now Quit Local Image, as in Library and Develop. Library and Develop messages and shortcut lists name Compositing and "Library & Develop" instead of "the editor" and "Library", and the Global shortcut F reads "Screen mode / Full Screen Preview".
- Blend modes in Develop's and the Library's Layers list read as in Compositing's Layers panel; German showed "Überlagerung" for Overlay instead of "Überlagern".
- Remove Location Info refuses an Original export whose XMP packet is too large or too deeply nested to read, instead of copying it unchecked.
- Remove Location Info refuses an Original export in any format the location check can't read, including HEIC, HEIF, HIF and JPEG XL, instead of copying it unchecked. The error names the format.
- Remove Location Info refuses an Original export holding metadata the check can't read, instead of copying it unchecked: malformed XMP in any format, compressed or text-chunk XMP and raw metadata profiles in PNG, and a second Exif block or XMP packet in JPEG, PNG or WebP. The error names the block.
- Remove Location Info now checks JPEG extended XMP, IPTC Location Created and Location Shown, country codes, GPS destination and area fields, and GPS values short of a full position.
- Remove Location Info offers DNG in its refusals only for raw photos.
- Remove Location Info on an Original export is best-effort, not a guarantee. Corrupt Exif in a file other than PSD or PSB reads as no Exif, so a location inside it isn't seen and the file is copied. The same goes for JPEG data past the first scan, IPTC past the first JPEG APP13 block that holds it, PSD resources other than Exif and XMP, XMP or IPTC outside a TIFF's first directory, CR3 previews and PNG text chunks under other keywords. Export as JPEG or TIFF when the location must go.
- Perspective Crop with no size set makes the canvas the rectangle the frame shows in perspective, so a façade or page shot at an angle keeps its proportions. It used the average length of the frame's opposite edges, which squashed it. A frame whose sides barely converge takes its longest edges.
- HEIC, HEIF and AVIF photos import with the capture time, camera, lens, location and keywords in their Exif and XMP items. They used to import with none of it, dated by the file's modification time. Remove Location Info still refuses to copy these originals.

## [0.8.0-alpha] - 2026-10-29

The first published alpha of the rebuilt Local Image. It was developed as 2.0.0-alpha.1, which was
never released, so this release includes everything listed under 2.0.0-alpha.1 below as well as
the changes in this section. The version follows on from the 0.7 previews so update checks see it
as newer.

### Added

- Custom ONNX models with external weights are copied as one self-contained file, and
  half-precision subject, sky and depth models now load. Weights must sit in the model's folder
  or the same Hugging Face repository's blobs folder.

- Affinity import keeps decorated text, table cells and grid lines, master-page content,
  embedded native documents and supported layer styles editable. CMYK pixel layers retain
  source-profile colour conversion, with a coated CMYK fallback and explicit approximation warnings.

- Compositing: Displace Smart Filters embed maps chosen from files or open documents, retain
  them through map replacement, undo and PSD/native saves, and no longer depend on the map source.
  Large embedded maps are omitted from inspect output and count toward the undo memory budget.
  Maps over 16,777,216 pixels are refused with a message.

- Compositing: Gradient Fill layers, the Gradient tool and Vector appearance paints offer
  Classic or Perceptual (OKLab) interpolation. Classic remains the default; native Perceptual stops
  stay editable in PSD, and supported SVG/PDF gradients retain vector output.

- Compositing: Export PDF carries File Info author, description, keywords, copyright status,
  notice and rights URL without camera, GPS or Develop metadata.

- Compositing: moving selected pixels uses parallel sparse tiles, shares fully selected tiles,
  and avoids an initial zero-offset preview composite on large documents.

- Compositing: PSD import and export preserve embedded Displace maps with editable scale, fit
  and edge settings.

- Compositing: PSD smart filters retain editable blur, sharpen, noise, pixelate, stylize,
  distort and Invert settings. Unmodelled settings and Average remain verbatim.

- Compositing: Save for Web's Image Size sets the exported Width and Height in pixels, kept in
  proportion while Constrain proportions is on, or a Percent, and offers the Resample list Export
  As uses (Bicubic by default; the choice is remembered). A layered composite can go out at, say,
  2048 px without resizing the document. Each document opens the dialog at its own size.
- Guided Upright guides can be selected, deleted individually, and adjusted by dragging endpoints.
- Middle-button drags pan zoomed Develop photos without leaving the active tool.

- AI: models without a Local Image profile run through the official workflow templates of the
  connected ComfyUI (FLUX.3 Klein, MOSS-Image, Krea 2 and later releases), matched to the
  installed file at any precision, with Create, Edit, Fill, Refine and Upscale derived from the
  template. Model setup checks each model before anything runs (ComfyUI version, missing nodes or
  options, missing companion files, GPU support for the precision, memory, licence) and explains
  each problem with a fix; verification tests a model with small renders on request.
- `local-image-cli ai doctor` lists every model on the connected ComfyUI, how it runs and what
  blocks it; `--verify` tests models one at a time; Ctrl-C cancels its render and exits 130;
  `--json` prints the report.
- AI: Window › Set Up Models… shows every installed model, what it is, what it needs and why,
  with one-click fixes (verified download of a missing file, use another installed file, move a
  misplaced file, choose a template, ignore) and test renders with progress and Cancel. Generate's
  picker marks template models "Not tested", "Test again" or "Verified", lists models that can't run
  yet with the reason and a Set up… button, and a banner announces new models.
- AI: an opt-in Enhance prompt switch for models whose official template has a local prompt
  enhancer (off by default, remembered per model), with its own optional verification render when
  installed.
- AI: Import Workflow… accepts a ComfyUI workflow without `li:` node titles; its prompt, negative,
  seed and size are found by their roles (basic controls).

### Changed

- Recovery uses private storage; existing folders lose group and other access.
- SVG: quoted font families round-trip through editable text export and import.
- Automation: saves preserve the existing file's group on Unix.
- Editor keeps one tab per file. Tabs whose file is overwritten are detached with a notice and
  must be saved under a new name.
- PSD and PSB export save sparse layers without pixels when their dense pixel buffer would
  exceed both the canvas size and half the memory budget.

- Drag the parametric Tone Curve line to adjust its tonal region with one undo step.
  Drag histogram zones to adjust Blacks, Shadows, Exposure, Highlights and Whites;
  double-click a zone to restore the photo's own default.

- Library and Develop: Cmd+L / Ctrl+L switches filtering off and restores the same filter.
  Find Similar enables filtering, and filters put aside follow keyword and folder changes.
- Pro and Studio Light secondary text is more readable on neutral panels and filled controls;
  Studio Light status colors are darker while image preview badges retain their existing amber.
- Affinity raster import leaves out pixel layers over 100 megapixels with a warning and checks
  shared allocation limits before decoding. Engine Open/Place captures a machine-sized
  memory allowance and rejects invalid detached native effects before publication.

- RAW development demosaics the white-balanced sensor data, as darktable and RawTherapee do. This
  removes false colour and zipper along slanted and fine high-contrast edges (window mullions,
  railings, roof lines, brick), most visibly under tungsten light. 1:1 views and full-size exports
  of every raw render slightly differently; previews and thumbnails are re-rendered.
- RAW highlights are found on the sensor data, before demosaic and before a DNG's built-in lens
  corrections. Blown windows no longer get cyan, blue or magenta outlines, and bright but unclipped
  corners of DJI and Hasselblad drone DNGs keep their colour. With Reconstruct (the default) a blown
  sun or sky continues the colour and brightness of its surroundings instead of becoming a grey
  disc; with Inpaint Opposed and Segmentation areas clipped in every channel render white instead
  of peach, lilac or magenta.
- RAW development removes isolated hot and dead photosites (long exposures, ageing sensors) before
  anything else reads the sensor, so they no longer show as coloured dots. Sharp point lights and
  stars, whose light reaches the neighbouring photosites, stay.
- DJI and Hasselblad drone DNG previews and thumbnails are developed from a binned sensor with the
  built-in lens corrections applied at that size, about three times faster on a 20 MP Mavic 3 file.
  The built-in distortion correction resamples with a de-ringed cubic kernel instead of bilinear,
  keeping more detail in full-size exports.
- Develop: on a monitor wider than sRGB (Display P3 on a Mac, or a wide-gamut monitor profile),
  the loupe, Before/After, compare and second-window views render in the narrowest of sRGB,
  Display P3, Adobe RGB, Rec. 2020 and ProPhoto that holds the monitor's colours, instead of sRGB.
  Saturated foliage, dusk skies and coloured light the monitor can show are no longer clipped to
  sRGB, and a Display P3 monitor shows exactly what a Display P3 8-bit export contains. The
  histogram and clipping warning then describe that space (Display P3 on a Mac), not sRGB; soft
  proof to sRGB to check sRGB clipping. sRGB monitors are unchanged.
- Export: output sharpening (Screen, Matte, Glossy) sharpens brightness only, so edges between a
  colour and a neutral surface no longer get coloured fringes, such as a cyan halo on a white wall
  beside brick.
- Export: with a resize kernel chosen, reductions average light rather than encoded values at
  every bit depth, so fine bright detail such as lit window grids keeps its brightness, and no
  dark halo forms beside bright windows. Saved export presets with a resize kernel produce
  slightly different files than before.
- Develop: at 100% and closer the loupe renders the frame, after crop and rotation, from the
  original at its native pixels on the GPU, so 1:1 shows the demosaicing, sharpening and noise
  reduction a full-size export has, one screen pixel per photo pixel. Without a GPU, while soft
  proofing, with a LUT profile, or for an original that is offline or can't be read, the loupe
  keeps its preview render. Known limitation: Fit and Fill views still render from a preview
  source at most 2560 pixels long, so a frame cropped to a small part of a high-resolution photo
  looks soft at Fit until it is zoomed to 100%.
- Develop: Paste, Sync, Auto Sync and presets that carry an automatic Upright mode analyse each
  photo's own lines in the background, with progress and Cancel, instead of giving every photo the
  first photo's correction or freezing the app while they solve.
- Develop: Reset, a section's reset and double-clicking a slider return to the photo's own
  defaults, such as a raw's camera defaults and As Shot white balance.
- AI: Set Up Models uses the Model Browser frame and neutral navigation with a fixed footer and
  shared accessible status chips. Distinct-file counts, model names and singular forms agree
  across the wizard, picker and preferences; test dates use the Library's format in the interface language.
- AI: previously verified template models ask for one re-test after the recipe-hash change.
- AI: model files are identified by their weights before their names; a name that points at a
  model generation no profile describes is no longer filed under an older family.
- AI: ComfyUI errors name the failing node's files and say what to do (update ComfyUI, the text
  encoder or VAE that doesn't fit, a file ComfyUI hasn't listed yet).
- AI: the Model Browser checks a template against the connected ComfyUI before downloading its
  files, badges templates that need a newer ComfyUI, and sets up an installed template's model to
  run through that template (full controls) instead of adding a basic-controls custom workflow.

### Fixed

- Library: opening a library folder no longer truncates or replaces files that planted catalog or settings links point to; writes follow such a link only to a valid catalog or settings file.

- Linux: explain how to upgrade from unpacked releases and report missing or incomplete uninstalls;
  match Open With file types to readable formats and register native `.pcraft` documents.

- Develop: dragging a curve region notch preserves neighboring splits and the Curve Regions
  undo label.

- Model Browser: worker-start failures report an error instead of leaving model checks, installs,
  catalogue searches or profile updates loading indefinitely.
- Compositing: Save for Web shows preview settings and codec errors instead of a blank preview,
  and recovers when the settings are corrected.

- Developer tools: upstream checks resolve linked project names and report checked and unchecked
  ports separately, failing when an upstream source or history could not be checked.
- Library: an edit is no longer shown as saved when the library folder could not be flushed to
  disk; it stays queued and is retried.

- Compositing: Add Noise and Wave retain the full unsigned 32-bit seed range.

- Compositing: 32-bit Levels, Curves, Exposure, Legacy Brightness/Contrast, Channel Mixer
  and Photo Filter support signed values and highlights with depth-aware formulas. Edited
  RGB Curves extend beyond their endpoints in RGB documents. Black & White caps its grayscale
  result at 16 before tinting. Integer Legacy Brightness/Contrast and tint follow measured
  Photoshop values, and float PSD merged references retain native precision.

- Develop and export preserve saturated colours already inside the selected output space,
  including deep blue. Colours outside that space transition smoothly, avoiding visible jumps
  from small edits. Rendered images and converted film negatives keep their saturated colours
  at neutral tone settings; RAW images retain smooth transitions in very saturated light.
- RAW capture sharpening leaves flat areas beyond the contrast mask's blur unchanged, keeping
  CPU and GPU previews consistent at high iteration counts.
- Develop: the scene gamut knee preserves its colour response across exposure changes, including
  very bright HDR values.

- Editor: moving a selection outline snaps to objects inside it, including when using the
  Lasso tool, and presets dropped onto the canvas follow its aligned view.
- PSD export keeps pattern fills inside their vector masks when reopened.
- PSD export uses the embedded CMYK profile when generating layer pixels, avoiding a colour shift
  when reopening fills without cached pixels.

- Develop: background Upright analyses discard results after the photo's source content is replaced.
- Develop: Refine Edges distinguishes different subject masks and image detail when reusing a
  matting result, preventing stale coverage with repeated or symmetric patterns.
- Develop: changing the monitor, Library or preview cache cancels full-size loupe renders that
  are still waiting for memory admission.
- Develop: background AI masks discard results after their source, decoding, shape or coordinate
  mapping changes, while keeping crop, tone and depth-range edits. Masks created by Paste, Sync or
  presets decode with the incoming RAW settings.
- Export: opaque 8-bit TIFF, PNG and WebP files are written without an alpha channel, which
  Photoshop showed as transparency and print preflight checks could flag. Images with real
  transparency keep it.
- Export: AI-enlarged 8-bit exports in Adobe RGB, ProPhoto, Display P3 or Rec. 2020 are dithered
  when converted from the model's sRGB output, so enlarged skies and smooth walls no longer band.
- AI: a changed model stays marked for testing across subsequent scans and temporary missing-node
  blockers until it is explicitly tested again.
- AI: a verification result cannot overwrite a newer model scan or changed setup choices.
- AI: verification retries test previously failed tools again; optional style files no longer block
  plain Create, and published capabilities honor their static checks. Background model refreshes
  recheck files and recipes after updates.
- AI: a Hugging Face model card no longer installs every file of its repository into one folder:
  files go to the folder their repository path names, diffusers copies and shards are left out,
  and a repository with several precisions installs the one chosen in the install dialog.
- Library: a catalog snapshot with an out-of-range change counter is refused as damaged instead of
  wrapping; captured times that are not ASCII no longer crash the By Date panel; impossible times
  such as 10:75 are refused instead of rolling over.
- Compositing: a PSD PackBits row whose declared length overflows is refused instead of decoding
  nothing.
- Colour fields refuse values such as `#+f+f+f` instead of reading them as hex.
- RAW import: damaged PEF, NEF, ORF, CR2, DNG and TIFF files are refused instead of crashing,
  allocating gigabytes or decoding padding as pixels. This covers short compressed streams, predictor
  overflow, wrapping maker-note crops, NaN or infinite float samples and black levels, two samples per
  pixel, missing strips and runaway TIFF entry counts.
- Library and Develop: crashes and hangs from damaged or extreme input are fixed. An export enlarged
  past 512 megapixels is capped, one that needs more memory than the computer has is refused with
  a message, and very long strips no longer panic the size calculation. A panic in an export's
  progress callback no longer leaves its worker threads hanging. Crops of no size, huge spot sizes,
  brush strokes with huge or non-finite segments, out-of-range defringe amounts, close tone-curve
  points and extreme `mask.move` deltas render or edit normally. Preset ZIPs are size-limited,
  preview caches refuse symlinked folders, imports skip symlink aliases, and GPX files and capture
  dates that cannot exist are refused.
- Editor: when the graphics device the window draws with is lost (a driver reset, or after sleep),
  every unsaved document is saved for crash recovery before the app closes, retrying until the
  saves succeed, instead of crashing or freezing. Saving over a file marked read-only is refused
  with a message instead of replacing it. A `.pcraft` file with numbers past the 32-bit range, and
  commands with such numbers, are refused instead of storing infinity. Clone, healing and history
  brush strokes too large to render are refused, PSD layers under unclosed groups past the depth
  limit are kept, and TIFF type layers with mixed byte order or unsafe transforms open safely.
- SVG import: a file whose elements nest tens of thousands deep is refused with a message instead
  of crashing the app.
- Commands and automation: a mistyped photo `id` or `ids` is refused instead of acting on the whole
  selection, and negative or fractional indices are refused instead of targeting the first version,
  sample or correction. `crop.set` refuses a crop smaller than the crop tool can make (it exported a
  1×1 image). Save a Copy over the document's own file needs `overwrite`, a save quality outside
  1 to 100 is refused instead of clamped, and out-of-range view zoom or centre values are refused.
- Graphics: GL adapters weaker than WebGL2's defaults no longer fail device creation, and small mip
  levels are redrawn whole, fixing canvas corruption on some AMD Vulkan drivers.
- DNG: JPEG XL compressed DNGs (DNG 1.7) keep their exposure, colour profile, tone curve and
  built-in lens corrections, and linear JPEG XL DNGs open. DNGs whose active area starts on an odd
  row or column demosaic with the right colours, and DNGs written by Local Image from such cameras
  open correctly in other apps (DNGs converted from those bodies by earlier builds change colour).
  Negative DeltaPerRow and DeltaPerColumn corrections are applied instead of dropped, and choosing
  another camera profile no longer adds the embedded profile's exposure offset to the new one's.
- RAW: a file with a damaged or truncated tile, strip or uncompressed band is refused with an error
  instead of developing with black blocks.
- Develop tone and colour. Saved edits that use these tools render differently:
  - Whites and Blacks work again: they reshape the global tone map instead of acting only above
    sensor white and in the deepest noise. Auto Tone's Whites and Blacks now take effect.
  - Clarity is equally strong at every tone instead of rising and falling every 2 EV (and weakening
    contrast at middle grey).
  - HSL Luminance can no longer turn saturated colours black: Blue Luminance -100 darkens a deep
    blue sky by about 2.5 EV. The eight HSL and B&W mix bands no longer overshoot between bands,
    and an untouched band changes nothing.
  - HSL Saturation and Luminance keep the hue; only the Hue slider moves it. They used to turn deep
    blues by up to 17 degrees at Saturation +100 and 6 degrees at Luminance +100, and yellows by up
    to 6 degrees.
  - Dehaze no longer takes shadows to black or leaves black fringes beside bright windows, and
    previews show the same edge as exports.
  - Luminance and Color noise reduction are independent: Color NR no longer smooths luminance
    texture, and Luminance NR reaches green-magenta luminance noise.
  - Range masks select the same pixels for every local tool, also after global Shadows, colour
    edits or B&W, and the mask overlay shows what is applied.
  - Color Range masks and the colour picker compare colours under the photo's Whites and Blacks
    as well as its Contrast. A saved Color Range mask on a photo with Whites or Blacks set
    (including by Auto Tone) may select somewhat different pixels; re-sample it if it does.
  - Highlights and Shadows no longer change with unrelated content such as a dark corner, a crop
    or a single black pixel.
  - Camera Calibration applies before HSL, Color Grading, Skin Tone and B&W, as in Lightroom; in
    B&W its primaries now shape the mix and Shadows Tint no longer tints the shadows.
  - On Apple Silicon, exact-black pixels under Point Color or a masked hue or saturation change
    no longer risk undefined colour.
- Develop resampling: reduced previews and exports no longer outline bright windows, lamps and
  skylines in black (the linear-light reduction rang below zero beside high-contrast edges). Crops,
  straighten, Upright and lens corrections resample with a clamped Catmull-Rom instead of one
  bilinear tap, so straightened and corrected exports keep their fine detail evenly across the
  frame; an unrotated crop snaps to whole pixels, so a full-size cropped export copies pixels; and
  warps widen the kernel where they squeeze the image instead of aliasing.
- Upright uses the photo's focal length (EXIF 35 mm equivalent, else the lens database's crop
  factor): wide and telephoto shots keep their proportions, and Auto corrects steeper tilts. Explicit
  Vertical and Full correct steep street-level and drone tilts instead of only levelling. Constrain
  Crop also moves the crop, keeping more of the frame. Panoramas record their own 35 mm-equivalent
  focal length, so Upright treats a stitched panorama as the wide view it is.
- Lens corrections: Remove CA and the manual CA sliders correct the sensor's colour planes on raw
  files, and noise reduction treats vignetting-corrected corners by their raw noise.
- Develop white balance: Tint follows Lightroom's and the DNG SDK's sign (positive pushes the photo
  towards magenta). The Daylight, Cloudy, Shade and Fluorescent presets and tints read from Lightroom
  XMP sidecars and presets render as in Lightroom instead of with a green cast, the Tint slider moves
  the photo the way its green-to-magenta track shows, and as-shot tints read like Lightroom's
  (daylight frames about +10). Custom tints saved by earlier builds now render with the opposite tint,
  and the As Shot tint shown for raws imported before this change keeps the old sign until the photo
  is imported again.
- Develop white balance selector and Auto WB on raws: the clicked grey (or Auto's average) is solved
  through the same camera model and camera look the render applies, so it renders neutral instead of
  keeping a 2-8% cast in mixed light. The selector samples the spot that was clicked after lens
  corrections, Upright and perspective, and reads a sharper preview. A layer's pick neutralises the
  spot as that layer sees it (the photo finished with its tone, colour and the layers below), instead
  of leaving warm light 3-13% off neutral. Picked values keep two decimals, so strongly coloured
  light is not left off neutral by rounding. Clicking on black shows a message instead of resetting
  to As Shot.
- Develop colour on raws: colours inside the export's gamut keep their saturation, and colours
  outside it keep their hue. The output knee used to start at 80% of the gamut distance, so
  saturated yellows, oranges and reds came out washed out (ColorChecker yellow dE00 2.9 in sRGB) and
  differently in sRGB, Display P3, Adobe RGB and ProPhoto exports; it now starts at 98% and only
  makes room for the working gamut that does not fit the output space. Colours outside the export's
  gamut move towards grey at constant OkLab lightness and hue instead of being compressed channel by
  channel, which bent saturated blues towards purple and cyan and oranges towards red. Scene colours
  past Rec. 2020 (LED coves, neon) roll off towards its edge the same way instead of being clipped
  onto it.
- LUT profiles (`.cube`, `.3dl`, `.look`) get sRGB / Rec. 709 display values in every export colour
  space, so a look that is right in the preview exports the same to Display P3, Adobe RGB and
  ProPhoto, a soft proof shows it, and adding an adjustment layer no longer changes it. On raws the
  LUT's colours are no longer compressed a second time by the output gamut knee. Grain now follows
  the LUT in every path. A LUT profile limits wide-gamut exports to sRGB colours, as the LUT itself
  does; at Amount 0 it is off and changes nothing.
- Camera profiles: the tone curve of a DCP picked under Custom Camera Profiles (Adobe's Camera
  Matching profiles, a DCamProf `-t acr` variant) is used by the Camera look instead of being
  dropped in favour of the JPEG-fitted or maker curve. Soft Film and Sigmoid keep their own curves.
- Develop white balance modes: Lightroom XMP sidecars and presets with Auto or a named white balance
  keep Lightroom's Temperature and Tint, so Auto no longer renders As Shot. Pasting, syncing, Auto
  Sync and presets measure Auto on each photo and give a named mode each photo's own values instead
  of copying the first photo's numbers. Photos that are not decoded yet are measured by a background
  task (shown in the task panel, cancellable, one undo step) instead of freezing the app. Choosing a white balance mode that matches one of the
  camera's own presets renders the camera's values the sliders show, instead of jumping on the first
  nudge.
- Camera profiles with a look table (Adobe Standard and Camera Matching DCPs, embedded DNG profiles)
  apply it after the Exposure slider, as the DNG specification and Lightroom do, so exposure changes
  keep the profile's colour response.

## [2.0.0-alpha.1] - 2026-10-20

This is the first Local Image 2 alpha. It combines the layered editor, the photo catalog and RAW
pipeline, and local AI in one native application. Supported platforms: Linux x86_64 and Apple
Silicon macOS 13 or later. Windows is experimental.

Important: Local Image 2 starts with a new Library. Catalogs, Library settings and caches from
earlier Local Image and LightCraft versions are not opened or migrated. An older catalog is
refused and left unchanged. Create a new Library and import your photos again.

The macOS app, disk image and installer of this alpha are not notarized, so macOS blocks them
the first time. On macOS 15 or later, open the app (or installer) once, then choose System
Settings > Privacy & Security > Open Anyway. On macOS 13 and 14, Control-click it and choose
Open. Check the download against its `.sha256` file first.

### Added

- Develop: a new colour and tone engine for raw processing, and raw quality improvements across
  supported cameras.
- Library: Save Over Original; a Lightroom-style Export dialog; a live round trip between Library
  and Compositing without an intermediate save; tear-off dock panels that can be docked back.
- Compositing: Blend If, noisy effects, CMYK and Lab tones, and wide blurs run on the GPU;
  contextual menus; pen and shape editing; Content-aware Heal, including healing over removed
  areas.
- AI: one official on-device model per function, including sky, plus custom ONNX models; model
  deletion; grouped CPU models; an upscaler picker; LoRAs; eject and stop controls.
- Smart Sort: File > Smart Sort & Export… runs the guided Library workflow. It tags photographs,
  groups faces, stacks bursts and duplicates, and exports after review. AI steps need their
  models installed.
- Help > Check for Updates… checks GitHub releases and offers a link to the release page. Nothing
  is downloaded automatically. A check at startup is an opt-in General preference, off by default.
- Settings > Attributions lists the attribution for each sampled project.
- Linux x86_64 AppImage and tarball, and Apple Silicon macOS 13+ app, DMG and installer artifacts.
  The release workflow signs and notarizes the macOS packages when Developer ID credentials are
  configured.
- Library and Develop translations.
- Crash reports: a panic writes a local `crash-*.txt` to the log folder. Nothing is sent.
  Release builds ship their debug symbols as separate downloads for symbolicating them.
- Recently Deleted can empty itself: a Library preference removes photos deleted a chosen
  number of days ago when the Library opens, as one undoable step. It is off by default, and
  files on disk are never touched.
- Automation: the MCP server marks every tool with accurate annotations. Irreversible commands
  need `--allow-destructive`, and bridge mode starts cloud generation only with
  `--allow-cloud-ai`.
- TIFF reads and writes CIE L\*a\*b\* and writes EXIF and GPS as the file's own directories.
  Composited CMYK documents export as CMYK TIFF and JPEG with their profile.
- Preferences > Performance > GPU Memory sets how much video memory the compositor may use.

### Changed

- Speed: lens corrections and capture sharpening run on the GPU; the compositor has a GPU path
  and stutters less while editing; raw processing is faster.
- Long exports no longer time out.
- AI Remove asks for confirmation before it removes anything. Custom-model errors are translated.
- SAM 3 stays out of the default release packages. AI models are separate downloads.
- The catalog has a format version. This build reads only its own format and refuses other
  versions without changing them.
- One executable: `local-image-cli` is a link to the app, which runs the command line when
  started under that name. The app is 177 MB instead of 445 MB.
- Paid cloud runs confirm the provider, model and image count first, and an optional session
  cap refuses runs past it. Rate limits, gateway errors and dropped connections are retried
  with backoff; a request that may already have been billed is never repeated.
- The cloud API keys in `preferences.json` are readable only by their owner.
- Find Similar results with thousands of hits filter a large Library about 50 times faster.
- Reading a TIFF-based raw's metadata no longer copies the whole file, twice per imported raw.
- Opening a raw measures sensor clipping on all cores: about 8 times faster for a 24 MP sensor.
- PDF import uses hayro 0.8. Form XObjects, patterns and Type 3 glyphs with their own Resources
  no longer borrow names from the page's resources, as the PDF specification requires.
- Keyboard Shortcuts refuses keys the system reserves and offers Accept and Go to Conflict or
  Undo Changes for a key another command holds.
- Layers panel: blend mode, Opacity, Fill and locks change every selected layer.
- Adjustments, filters, brushes and transforms follow Photoshop more closely: Levels, Curves,
  Brightness/Contrast, Hue/Saturation, Color Balance, Gradient Map, Threshold and Posterize
  (including at 16 bits), Blur More, Find Edges, Polar Coordinates, the round brush tip, Dual
  Brush, Color Range presets and bicubic Transform and Warp resampling.
- Save for Web has no matte by default, and JPEG keeps 4:4:4 chroma above quality 50.

### Fixed

- A GPU crash on exit is fixed: render workers finish before teardown, and the GPU is quiesced
  at exit.
- The native app waits for named fonts before its first paint.
- Shared checkbox accessibility.
- Missing-model errors point to Edit > Preferences > Local AI.
- AI settings download ownership is corrected, and the native GPU requirements are clarified.
- 41 review findings across file safety, rendering and app integration are fixed.
- The editor's file dialogs (Open, Save As, Export, Place, imports and folder pickers) no longer
  freeze the window on Linux or crash the app on macOS, where they now open as a sheet on the
  window. So do the Library's (Add Photos, Import from Folder, Find Missing and Locate, export
  and import destinations, backups, presets, track logs, Denoise to a new file and every folder
  picker), Swatches import and export, and the AI and Generate pickers (ONNX models, ComfyUI
  workflows and folders, reference images, Remove Backgrounds batches).
- Save asks before overwriting a file another app changed. Save As gives a chosen name the
  extension of its file type. Autosave recovers committed bundles whose sidecar was never
  written.
- PSD: Backgrounds, 16- and 32-bit layer blocks, merged transparency, layer flags, smart-object
  filter masks and pattern fills survive a round trip. Layered files with damaged merged data
  open.
- Untrusted files: PSD, EXR, PAM, Targa, PNG and brush imports check sizes and counts before
  allocating, and large edits (Fill, Fade, Copy, Clear, Free Transform, Image Size, Liquify,
  Warp, Offset) report allocation failures instead of aborting.
- CSV import keeps UTF-8 text intact, and CSV export guards against formula injection.
- A failed GPU start no longer forces CPU rendering on every later launch, and closing a
  document frees what the GPU compositor held for it.
- The window reopens where it was, a dialog no longer stops the title bar dragging, and the
  bundled macOS app keeps its own Dock icon.
- Clicking a Layers-panel row while editing type commits the edit.
- Match Color's live preview reads the chosen Source document, so it shows what OK applies.
- Render Video and Video Frames to Layers ask for a folder instead of failing without one.
- Wayland paste accepts JPEG, BMP and WebP clipboard images, not only PNG.
- Export As no longer freezes on large layered documents, and its size estimates use the
  configured exporter.
- Paragraph text breaks a word wider than the box at the box edge.
- The unsaved-changes prompt gives its default answer keyboard focus.
