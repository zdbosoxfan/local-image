# Changelog

All notable changes to Local Image are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and version numbers follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html) with alpha and beta pre-release tags.

## [Unreleased]

### Added

- Affinity import keeps decorated text, table cells and grid lines, master-page content,
  embedded native documents and supported layer styles editable. CMYK pixel layers retain
  source-profile colour conversion, with a coated CMYK fallback and explicit approximation warnings.

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

- Develop: dragging a curve region notch preserves neighboring splits and the Curve Regions
  undo label.

