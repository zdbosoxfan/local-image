# Codex task: Object › Path and Expand commands (VG commands, V5)

Report file: `docs/wip/CODEX-REPORT-vector-36-object-path-commands.md`. Integration branch: `wip/vector`.

Read `docs/specs/vector-architecture.md`, `docs/specs/vector-persona-spec.md` including Owner decisions, `docs/specs/vector-quality-plan.md`, `docs/wip/vector-tasks/README.md`, and `docs/wip/REPORT-vector-gaps.md` (the gap audit that created this package). Continue the existing branch work; inspect owned files and current tests before changing anything. Carry out this package completely, autonomously, within the shared integration contract.

## Scope

Implement the Object › Path menu and the unified Expand family. **Join** (Ctrl+J in the Illustrator keymap): coincident endpoints merge, otherwise a straight segment joins the nearest open ends; a single open path closes. **Average** (Alt+Ctrl+J): horizontal, vertical or both. **Simplify** dialog with detached live preview (curve precision, corner angle threshold, original/new point counts) over V1 simplify. **Add Anchor Points** (midpoints), **Remove Anchor Points** (selected, keeping shape), **Clean Up** (stray points, unpainted objects, empty text), **Divide Objects Below**, **Split Into Grid** (rows/columns/gutters, optional guides), and menu routing of Outline Stroke and Offset Path to V1/10. **Object › Expand** dialog (Object/Fill/Stroke) and **Expand Appearance** in one transaction: multi-fill/stroke stacks become a group of plain paths in paint order; live shapes, corners, widths, brushes, path text, symbols, blends, repeats, envelopes, live paint and path effects expand through their owners' public expand APIs. Unavailable expanders show a disabled item with a reason, never a fake success. **Paste in Front / Paste in Back** (Ctrl+F / Ctrl+B) for vector layers.

## Files owned

pc-engine/src/vector_packages/path_commands.rs and path_commands/ tests; pc-engine/src/vector_packages/expand.rs and expand/ tests; pc-ui-egui/src/vector_packages/path_commands.rs and path_commands/ tests (Simplify, Split Into Grid, Expand dialogs). Existing clipboard commands are read-only; menu fragments go to 00.

Also owns `docs/wip/vector-tasks/integration/36.md`, `docs/wip/vector-tasks/i18n/36.tsv`, and its report/fixture source records. Core registries, manifests/lock, aggregate test registration and i18n catalogs belong to 00 unless explicitly owned above. No competing edits: provide exact fragments/call signatures and coordinate through the integration lane. Read-only shared types are not an invitation to expand them independently.

## Interfaces used

V1 join/simplify/offset_path/outline_stroke/pathfinder/regions/bounds; 03 node operations read-only; public expand/projection functions of 02, 06, 08, 11, 12, 15, 17, 18, 19, 20, 42 and 45; vector_api/Session::edit. Commands register in a new compiled `vector_packages::path_commands` slot requested from 00 (journal=true for edits, journal=false for queries/previews); tool IDs, labels, icons and shortcuts are supplied to 01/46 as fragments.

Every gesture captures document ID/revision and target IDs. Preview is detached, non-journaled and cancellable. Commit checks staleness, validates the entire target set, prepares bounds-limited caches, then executes one engine command/Session::edit. Old docs/settings load; numeric inputs are finite and bounded. File adapters use `vector_design::exchange` rather than depending on UI/engine.

## Acceptance

Engine oracles: Join two open paths with coincident ends gives one path with n1+n2−1 nodes, non-coincident ends add one straight segment; Average Both collapses selected anchors to their centroid; Simplify of a 100-node circle reaches ≤8 nodes within the tolerance (Hausdorff); Clean Up removes exactly the seeded strays/unpainted/empty text; Split Into Grid 3×2 with gutter 10 gives exact rectangles; Divide Objects Below conserves area. Expand Appearance of 2 fills + 2 strokes yields 4 paths whose composite equals the source ≤1/255 on CPU and RTX; expanding a live shape, blend, repeat and symbol gives plain layers with identical pixels. Kittest: menu clicks, Simplify slider drag with preview and Cancel, Ctrl+J, Ctrl+F/Ctrl+B. One undo per command. Real-GPU app pixel parity before/after expand.

Tests must exercise behavior and independent oracles, not mirror implementation. UI tests really click/drag/type, and full-app GPU checks exercise live preview and release. Use an offscreen app/harness; do not open desktop windows. GPU gates assert RTX 5090/Vulkan/discrete adapter and required float format: llvmpipe or a skipped adapter does not pass. Pixel parity uses premultiplied max diff<=1/255. Performance claims require release real-app pointer-frame measurements and hardware identity. Pure data/format packages supply interaction/render tests via 00 once integrated rather than pretending the isolated parser is the app.

Run focused changed-code tests while developing, then full tests of only changed crates once and clippy all-targets -Dwarnings; record counts/ignored/missing corpus separately. Supply real-GPU tests even where final renderer is a named dependency. Complete the integrated acceptance before calling the package done.

## Dependencies

V1 now; expand dispatch integrates each owner's API as it lands (08 first). 45 optional. Menu wiring and Vector keymap entries through 00/46.

## Sources and attribution

VC [engine cmd/path.rs](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/engine/src/cmd/path.rs) (join, average), [cmd/pathops.rs](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/engine/src/cmd/pathops.rs) (object.path.*), [cmd/expand.rs](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/engine/src/cmd/expand.rs), tests_expand.rs/tests_objexpand.rs; Illustrator [Average](https://helpx.adobe.com/illustrator/desktop/draw-shapes-and-paths/modify-paths/average-the-position-of-anchor-points.html), [Simplify](https://helpx.adobe.com/illustrator/desktop/draw-shapes-and-paths/modify-paths/manually-simplify-paths.html), [Stray points](https://helpx.adobe.com/illustrator/desktop/draw-shapes-and-paths/modify-paths/find-and-delete-stray-anchor-points.html), [Divide / Split into grid](https://helpx.adobe.com/illustrator/desktop/manage-objects/edit-objects/divide-or-split-objects.html), [Expand objects](https://helpx.adobe.com/illustrator/desktop/manage-objects/arrange-objects/expand-objects.html), [shortcuts](https://helpx.adobe.com/illustrator/using/default-keyboard-shortcuts.html); Affinity [Expand stroke](https://affinity.help/designer2/en-US.lproj/pages/CurvesShapes/expandStroke.html).

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

