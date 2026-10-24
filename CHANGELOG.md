# Changelog

All notable changes to Local Image are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and version numbers follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html) with alpha and beta pre-release tags.

## [Unreleased]

### Fixed

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
- Develop tone and colour. Saved edits that use these tools render differently:
  - Whites and Blacks work again: they reshape the global tone map instead of acting only above
    sensor white and in the deepest noise. Auto Tone's Whites and Blacks now take effect.
  - Clarity is equally strong at every tone instead of rising and falling every 2 EV (and weakening
    contrast at middle grey).
  - HSL Luminance can no longer turn saturated colours black: Blue Luminance -100 darkens a deep
    blue sky by about 2.5 EV. The eight HSL and B&W mix bands no longer overshoot between bands,
    and an untouched band changes nothing.
  - Dehaze no longer takes shadows to black or leaves black fringes beside bright windows, and
    previews show the same edge as exports.
  - Luminance and Color noise reduction are independent: Color NR no longer smooths luminance
    texture, and Luminance NR reaches green-magenta luminance noise.
  - Range masks select the same pixels for every local tool, also after global Shadows, colour
    edits or B&W, and the mask overlay shows what is applied.
  - Highlights and Shadows no longer change with unrelated content such as a dark corner, a crop
    or a single black pixel.
  - Camera Calibration applies before HSL, Color Grading, Skin Tone and B&W, as in Lightroom; in
    B&W its primaries now shape the mix and Shadows Tint no longer tints the shadows.
  - On Apple Silicon, exact-black pixels under Point Color or a masked hue or saturation change
    no longer risk undefined colour.

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
