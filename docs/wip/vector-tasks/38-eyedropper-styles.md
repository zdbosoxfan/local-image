# Codex task: appearance eyedropper, Style Picker and Graphic Styles (VG styles, V4)

Report file: `docs/wip/CODEX-REPORT-vector-38-eyedropper-styles.md`. Integration branch: `wip/vector`.

Read `docs/specs/vector-architecture.md`, `docs/specs/vector-persona-spec.md` including Owner decisions, `docs/specs/vector-quality-plan.md`, `docs/wip/vector-tasks/README.md`, and `docs/wip/REPORT-vector-gaps.md` (the gap audit that created this package). Continue the existing branch work; inspect owned files and current tests before changing anything. Carry out this package completely, autonomously, within the shared integration contract.

## Scope

Implement attribute sampling and reusable styles. **Vector Eyedropper** (I): click samples the clicked object's appearance (fill/stroke stack, stroke options, opacity/blend, and character/paragraph attributes for text) and applies it to the selection; Alt-click applies the loaded appearance to the clicked object; Shift samples colour only (including from images and gradient intermediates); Alt+Shift-click appends appearance; Eyedropper Options choose what is picked and applied. **Style Picker** (Affinity): load from an object, choose Stroke/Fill/Opacity/Effects/Character/Paragraph/Object attributes, apply by click or marquee, Unload. **Graphic Styles / Styles panel**: document styles (appearance stack, layer FX, opacity/blend), New from selection, Apply (replace; Alt adds), link objects to a style so Redefine updates every linked object, Break Link on local edit, rename/duplicate/delete, an original starter library, and a Select Same Graphic Style query for 32. Data are additive: `Document.graphic_styles` plus a per-layer style link, requested from 00 with serde defaults and skip-if-empty; coordinate the native format with 15's FORMAT_VERSION 2 change.

## Files owned

pc-engine/src/vector_packages/styles.rs and styles/ tests; pc-ui-egui/src/vector_packages/eyedropper.rs and eyedropper/ tests; pc-ui-egui/src/vector_packages/styles_panel.rs and styles_panel/ tests; pc-doc/src/vector_design/styles.rs (data module; lib registration and manifest/convert mirror through 00). The pixel Eyedropper, layer_style.rs and Photoshop layer-style copy/paste are read-only.

Also owns `docs/wip/vector-tasks/integration/38.md`, `docs/wip/vector-tasks/i18n/38.tsv`, and its report/fixture source records. Core registries, manifests/lock, aggregate test registration and i18n catalogs belong to 00 unless explicitly owned above. No competing edits: provide exact fragments/call signatures and coordinate through the integration lane. Read-only shared types are not an invitation to expand them independently.

## Interfaces used

Appearance (08), existing layer FX data, pc-text runs and the existing character/paragraph styles, Session::edit/vector_api, pc-format manifest mirror via 00. Commands register in a new compiled `vector_packages::styles` slot requested from 00 (journal=true for edits, journal=false for queries/previews); tool IDs, labels, icons and shortcuts are supplied to 01/46 as fragments.

Every gesture captures document ID/revision and target IDs. Preview is detached, non-journaled and cancellable. Commit checks staleness, validates the entire target set, prepares bounds-limited caches, then executes one engine command/Session::edit. Old docs/settings load; numeric inputs are finite and bounded. File adapters use `vector_design::exchange` rather than depending on UI/engine.

## Acceptance

Eyedropper copies a 2-fill/1-stroke dashed stack exactly (structural equality) and the target's pixels match the source's; Alt-click apply; Shift samples a raster pixel colour; Options exclusions respected. Style Picker applies only the chosen attributes. Redefining a graphic style updates 3 linked objects pixel-exact while an unlinked (locally edited) object stays unchanged; save/load/undo round-trip; old documents load with no styles. Kittest real clicks: I-tool click/Alt-click/Shift-click; Styles panel create, apply, redefine. Real-GPU parity of styled objects ≤1/255 and app click-to-apply release.

Tests must exercise behavior and independent oracles, not mirror implementation. UI tests really click/drag/type, and full-app GPU checks exercise live preview and release. Use an offscreen app/harness; do not open desktop windows. GPU gates assert RTX 5090/Vulkan/discrete adapter and required float format: llvmpipe or a skipped adapter does not pass. Pixel parity uses premultiplied max diff<=1/255. Performance claims require release real-app pointer-frame measurements and hardware identity. Pure data/format packages supply interaction/render tests via 00 once integrated rather than pretending the isolated parser is the app.

Run focused changed-code tests while developing, then full tests of only changed crates once and clippy all-targets -Dwarnings; record counts/ignored/missing corpus separately. Supply real-GPU tests even where final renderer is a named dependency. Complete the integrated acceptance before calling the package done.

## Dependencies

08 appearance; existing layer FX; text from 12/existing; 32 consumes the style query; 00 data and registration.

## Sources and attribution

VC [tools/xform/eyedropper.rs](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/tools/src/xform/eyedropper.rs), [engine cmd/style.rs](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/engine/src/cmd/style.rs), [cmd/stylelib.rs](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/engine/src/cmd/stylelib.rs), tests_eyedropper.rs/tests_stylelib.rs; Illustrator [Eyedropper](https://helpx.adobe.com/illustrator/desktop/manage-colors/select-and-adjust-colors/select-colors.html), [Graphic Styles panel](https://helpx.adobe.com/illustrator/desktop/special-effects-styles/apply-graphic-styles/graphic-styles-panel-overview.html), [Appearance attributes](https://helpx.adobe.com/illustrator/using/appearance-attributes.html), [shortcuts](https://helpx.adobe.com/illustrator/using/default-keyboard-shortcuts.html); Affinity [Style Picker](https://affinity.help/designer2/en-US.lproj/pages/Tools/tools_stylePicker.html), [Styles](https://affinity.help/designer2/en-US.lproj/pages/ObjectControl/styles.html).

Use pinned sources and official docs linked by the architecture. Retain copyright/license notices, one PORTS row per adapted file, and give 00 the attribution fragment to regenerate. No brand assets. Corpus/fixture sources, license, tool versions and parameters must be recorded; missing proprietary/external fixtures never count as successful gates.

## Rules (all Codex jobs)

- "Local Image" is GPL-3.0-or-later and **all Rust**: workspace denies `unsafe`; no new dependencies, no C/C++ or
  `-sys` crates in the build (C only in out-of-build reference harnesses under `target/refvec/`).
- Offline builds (`--offline`). The PC has 60 GB RAM shared with other jobs: ONE cargo build at a time in your job,
  `CARGO_BUILD_JOBS=3`. Use `PATH=~/.cargo/bin:$PATH` and `cargo +1.98.1`. Format: `cargo +1.98.1 fmt -p <crate>`;
  lint: `CARGO_TARGET_DIR=target/clippy cargo +1.98.1 clippy --offline -p <crate> --all-targets -- -D warnings`.
- **Testing budget (owner, 2026-10-09: "all day we are just waiting for tests")**: while developing use
  `cargo +1.98.1 check --offline -p <crate>` and run ONLY the tests you added or that cover the code you changed,
  filtered by name (`cargo +1.98.1 test --offline -p <crate> <filter>`). At the end, run clippy on the changed crates
  and the full test suite of ONLY the crates you changed, once. Do not run whole-workspace or multi-crate suites
  repeatedly; the coordinator runs full and GPU suites at milestones.
- Run the tests of every crate you change (`cargo +1.98.1 test --offline -p <crate>`); all must pass. You run **without
  a sandbox** on the owner's PC: the real RTX 5090 (Vulkan) and the local ComfyUI (127.0.0.1:8188, shared with the
  owner: check /queue, one small job at a time) are available. GPU tests and any performance claims must come from the
  real GPU — llvmpipe or "skipped: no GPU adapter" results don't count. Measure interactive speed in the real app path,
  not only image equivalence.
- UI is egui 0.36; Compositing UI tests use egui_kittest, Library/Develop uses its own `headless.rs` harness. Add tests
  that actually click/drag the UI you change.
- A Local Image app may be running on the desktop: don't kill it or open GUI windows. Don't touch the owner's files
  outside this worktree (read-only access to reference data is fine).
- Old settings/documents must still load (serde defaults). The owner does not need old edits to render identically.
- **Do not commit.** Leave changes in the working tree. When done, write the report file named in your task: what's
  done per point, tests with counts, GPU tests added, anything left and why.
- **Don't guess when you don't have to (owner, 2026-10-09).** Before designing anything, look for the established
  answer and follow it: official documentation (ComfyUI docs and Comfy-Org workflow templates, egui/wgpu/crate docs,
  file-format specs), the open-source projects already sampled for this project (`~/.local/share/local-image-dev/engine-sources/`,
  `docs/PORTS.md`, `docs/upstream/`, `crates/*/tests/references/`), and other reputable open-source code online. Cite
  what you followed in your report. Work things out yourself only when there's no obvious existing solution, and say so.

