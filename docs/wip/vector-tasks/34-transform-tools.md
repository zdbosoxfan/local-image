# Codex task: transform tools (VG transform, V8)

Report file: `docs/wip/CODEX-REPORT-vector-34-transform-tools.md`. Integration branch: `wip/vector`.

Read `docs/specs/vector-architecture.md`, `docs/specs/vector-persona-spec.md` including Owner decisions, `docs/specs/vector-quality-plan.md`, `docs/wip/vector-tasks/README.md`, and `docs/wip/REPORT-vector-gaps.md` (the gap audit that created this package). Continue the existing branch work; inspect owned files and current tests before changing anything. Carry out this package completely, autonomously, within the shared integration contract.

## Scope

Implement Illustrator's reference-point transform tools for vector layers. **Rotate** (R), **Reflect** (O), **Scale** (S) and **Shear**: click sets a visible reference point (snappable), drag transforms about it, Shift constrains (45° / proportional / axis), Alt-drag leaves a copy, Alt-click or Enter opens the numeric dialog with Preview and Copy; options Scale Strokes & Effects, Scale Corners, Transform Objects/Patterns. **Reshape**: select nodes and drag; neighbouring nodes follow with distance falloff (VectorCraft semantics). **Free Transform** (E): Free Transform, Perspective Distort and Free Distort modes on vector paths, mapping control geometry through the homography/bilinear map with adaptive subdivision and a refit error bound; text/symbols prompt to convert, or route to an envelope (20) when the user picks the non-destructive option. **Transform Again** (Ctrl+D in the Illustrator keymap) repeats the last vector transform including its copy flag; **Transform Each** (Alt+Shift+Ctrl+D) scales/moves/rotates/reflects each object about its own reference point, with seeded Random; **Move dialog** (Enter with the Move tool, Affinity "Move data entry") moves by position/distance/angle with Copy. Bake rules: similarity transforms keep live shapes live; others convert to curves; stroke width, width profiles and corner radii scale per option.

## Files owned

pc-engine/src/vector_packages/transform_tools.rs and transform_tools/ tests; pc-ui-egui/src/vector_packages/transform_tools.rs and transform_tools/ tests (tools, reference-point overlay, Rotate/Reflect/Scale/Shear/Transform Each/Move dialogs); pc-vector/src/projective.rs (projective and bilinear path mapping with adaptive subdivision) and its tests. Existing transform_tool.rs, move_ui/move_input and `edit.transform*` commands are read-only; 16 owns the Transform panel and Point Transform.

Also owns `docs/wip/vector-tasks/integration/34.md`, `docs/wip/vector-tasks/i18n/34.tsv`, and its report/fixture source records. Core registries, manifests/lock, aggregate test registration and i18n catalogs belong to 00 unless explicitly owned above. No competing edits: provide exact fragments/call signatures and coordinate through the integration lane. Read-only shared types are not an invitation to expand them independently.

## Interfaces used

kurbo Affine; Layer.vector corners/live_shape/appearance (profile scaling contract with 11/17); V1 bounds/fit_path; 16 transform-origin and anchor helpers read-only; 07 snapping for the reference point; vector_api/Session::edit; existing transform-again state through a 00 bridge so Pixel Ctrl+D behaviour is untouched. Commands register in a new compiled `vector_packages::transform_tools` slot requested from 00 (journal=true for edits, journal=false for queries/previews); tool IDs, labels, icons and shortcuts are supplied to 01/46 as fragments.

Every gesture captures document ID/revision and target IDs. Preview is detached, non-journaled and cancellable. Commit checks staleness, validates the entire target set, prepares bounds-limited caches, then executes one engine command/Session::edit. Old docs/settings load; numeric inputs are finite and bounded. File adapters use `vector_design::exchange` rather than depending on UI/engine.

## Acceptance

Engine oracles: rotate 90° about a clicked node maps known coordinates exactly (1e-9); reflecting twice across a 30° axis is identity; scaling with Scale Strokes off keeps stroke width; shear preserves area; Free Distort of a square to a trapezoid maps the corners exactly, keeps straight edges straight and keeps curved edges within 0.25 px of the analytic map; Perspective Distort of a circle stays within 0.25 px of the homography oracle; Transform Each Random with a fixed seed is deterministic; Transform Again with Copy three times gives 4 objects at exact offsets. Kittest real input: R click origin then drag; Alt-click opens the dialog, type 45, Copy → 2 layers; Ctrl+D three times; E tool corner drag in Free Distort; Escape restores. Real-GPU app preview planes and release pixels, one journal entry per gesture; rotating a 500-shape group keeps warm release frames under 16 ms (measured).

Tests must exercise behavior and independent oracles, not mirror implementation. UI tests really click/drag/type, and full-app GPU checks exercise live preview and release. Use an offscreen app/harness; do not open desktop windows. GPU gates assert RTX 5090/Vulkan/discrete adapter and required float format: llvmpipe or a skipped adapter does not pass. Pixel parity uses premultiplied max diff<=1/255. Performance claims require release real-app pointer-frame measurements and hardware identity. Pure data/format packages supply interaction/render tests via 00 once integrated rather than pretending the isolated parser is the app.

Run focused changed-code tests while developing, then full tests of only changed crates once and clippy all-targets -Dwarnings; record counts/ignored/missing corpus separately. Supply real-GPU tests even where final renderer is a named dependency. Complete the integrated acceptance before calling the package done.

## Dependencies

Live-shape conversion via 02; width-profile/brush scaling via 11/17; reference-point snapping 07; Transform panel coordination with 16. Tool IDs (rotate, reflect, scale, shear, reshape, freeTransform) and the Ctrl+D persona keymap through 01/46/00.

## Sources and attribution

VC [tools/xform/transform.rs](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/tools/src/xform/transform.rs), [tools/xform/free.rs](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/tools/src/xform/free.rs), [tools/extra.rs](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/tools/src/extra.rs) (Reshape), [engine cmd/object.rs](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/engine/src/cmd/object.rs) (transform, transformAgain), [cmd/xform.rs](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/engine/src/cmd/xform.rs), [cmd/menucmds.rs](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/engine/src/cmd/menucmds.rs) (transformEach); Illustrator [Rotate](https://helpx.adobe.com/illustrator/desktop/manage-objects/arrange-objects/rotate-objects.html), [Reflect](https://helpx.adobe.com/illustrator/desktop/manage-objects/arrange-objects/reflect-or-flip-objects.html), [Scale](https://helpx.adobe.com/illustrator/desktop/manage-objects/reshape-transform-objects/scale-objects.html), [Shear](https://helpx.adobe.com/illustrator/desktop/manage-objects/reshape-transform-objects/shear-objects.html), [Distort](https://helpx.adobe.com/illustrator/desktop/manage-objects/reshape-transform-objects/distort-objects.html), [Transform objects](https://helpx.adobe.com/illustrator/desktop/manage-objects/reshape-transform-objects/transform-objects.html); Affinity [Transforming objects](https://affinity.help/designer2/en-US.lproj/pages/ObjectControl/transform.html).

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

