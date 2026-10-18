# Changelog

All notable changes to Local Image are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and version numbers follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html) with alpha and beta pre-release tags.

## [Unreleased]

- Clicking a Layers-panel row while editing type commits the edit.
- Match Color's live preview reads the chosen Source document, so it shows what OK applies.
- Render Video and Video Frames to Layers ask for a folder instead of failing without one.
- Wayland paste accepts JPEG, BMP and WebP clipboard images, not only PNG.
- Keyboard Shortcuts refuses keys the system reserves and offers Accept and Go to Conflict or Undo Changes for a key another command holds.
- Export As size estimates for large documents use the configured exporter on the preview worker.
- Security hardening and cleanup — see PR
- Find Similar results with thousands of hits filter a large Library about 50 times faster.

## [2.0.0-alpha.1] - TBD

This is the first Local Image 2 alpha. It combines the layered editor, the photo catalog and RAW
pipeline, and local AI in one native application. Supported platforms: Linux x86_64 and Apple
Silicon macOS 13 or later. Windows is experimental.

Important: Local Image 2 starts with a new Library. Catalogs, Library settings and caches from
earlier Local Image and LightCraft versions are not opened or migrated. An older catalog is
refused and left unchanged. Create a new Library and import your photos again.

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
  Developer ID signing and notarization run when the repository secrets are configured.
- Library and Develop translations.

### Changed

- Speed: lens corrections and capture sharpening run on the GPU; the compositor has a GPU path
  and stutters less while editing; raw processing is faster.
- Long exports no longer time out.
- AI Remove asks for confirmation before it removes anything. Custom-model errors are translated.
- SAM 3 stays out of the default release packages. AI models are separate downloads.
- The catalog has a format version. This build reads only its own format and refuses other
  versions without changing them.

### Fixed

- A GPU crash on exit is fixed: render workers finish before teardown, and the GPU is quiesced
  at exit.
- The native app waits for named fonts before its first paint.
- Shared checkbox accessibility.
- Missing-model errors point to Edit > Preferences > Local AI.
- AI settings download ownership is corrected, and the native GPU requirements are clarified.
- 41 review findings across file safety, rendering and app integration are fixed.
