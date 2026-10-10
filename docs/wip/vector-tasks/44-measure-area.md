# Codex task: vector Measure and Area tools (VG measure, V8)

Report file: `docs/wip/CODEX-REPORT-vector-44-measure-area.md`. Integration branch: `wip/vector`.

Read `docs/specs/vector-architecture.md`, `docs/specs/vector-persona-spec.md` including Owner decisions, `docs/specs/vector-quality-plan.md`, `docs/wip/vector-tasks/README.md`, and `docs/wip/REPORT-vector-gaps.md` (the gap audit that created this package). Continue the existing branch work; inspect owned files and current tests before changing anything. Carry out this package completely, autonomously, within the shared integration contract.

## Scope

Implement measuring for design work. **Measure**: drag between two points (snapping to nodes, edges, guides and page through 07) to read distance, angle, ΔX and ΔY in the document's DesignUnits (pt/mm/in/px); Shift constrains to 45°; the reading stays until the next measurement, does not print and is not saved. **Area** (Affinity): hover or select to read area, area excluding holes, total perimeter and the hovered segment length (Full edges / Cusped segments); multiple selected objects sum. Optional **drawing scale** per document/artboard (e.g. 1:100) with Assign from the current measurement (additive optional data through 00). The Pixel persona's Ruler tool is unchanged.

## Files owned

pc-engine/src/vector_packages/measure.rs and measure/ tests; pc-ui-egui/src/vector_packages/measure.rs and measure/ tests. Drawing-scale data and overlay paint hook are requested from 00.

Also owns `docs/wip/vector-tasks/integration/44.md`, `docs/wip/vector-tasks/i18n/44.tsv`, and its report/fixture source records. Core registries, manifests/lock, aggregate test registration and i18n catalogs belong to 00 unless explicitly owned above. No competing edits: provide exact fragments/call signatures and coordinate through the integration lane. Read-only shared types are not an invitation to expand them independently.

## Interfaces used

V1 area/bounds and kurbo arc length; 07 snapping query; DesignUnits and 16's exact unit conversions; overlay painter hook via 00. Measurements are queries (journal=false). Commands register in a new compiled `vector_packages::measure` slot requested from 00 (journal=true for edits, journal=false for queries/previews); tool IDs, labels, icons and shortcuts are supplied to 01/46 as fragments.

Every gesture captures document ID/revision and target IDs. Preview is detached, non-journaled and cancellable. Commit checks staleness, validates the entire target set, prepares bounds-limited caches, then executes one engine command/Session::edit. Old docs/settings load; numeric inputs are finite and bounded. File adapters use `vector_design::exchange` rather than depending on UI/engine.

## Acceptance

A 100×50 px rectangle reports 5000 px² and the exact mm² at 300 DPI; a ring reports area excluding the hole; a circle's perimeter is 2πr within 1e-6 relative; node-to-node snapped distance is exact; drawing scale 1:100 multiplies lengths; document bytes and revision are unchanged. Kittest: snapped measure drag, hover area readout, unit switch. Real-GPU: overlay drawn without re-rendering document layers within a warm 16 ms frame (measured).

Tests must exercise behavior and independent oracles, not mirror implementation. UI tests really click/drag/type, and full-app GPU checks exercise live preview and release. Use an offscreen app/harness; do not open desktop windows. GPU gates assert RTX 5090/Vulkan/discrete adapter and required float format: llvmpipe or a skipped adapter does not pass. Pixel parity uses premultiplied max diff<=1/255. Performance claims require release real-app pointer-frame measurements and hardware identity. Pure data/format packages supply interaction/render tests via 00 once integrated rather than pretending the isolated parser is the app.

Run focused changed-code tests while developing, then full tests of only changed crates once and clippy all-targets -Dwarnings; record counts/ignored/missing corpus separately. Supply real-GPU tests even where final renderer is a named dependency. Complete the integrated acceptance before calling the package done.

## Dependencies

07 snapping and 16 units. Tool IDs (vectorMeasure, area) via 01/46.

## Sources and attribution

VC [tools/xform/measure.rs](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/tools/src/xform/measure.rs); Illustrator [Measure distance](https://helpx.adobe.com/illustrator/desktop/measure-and-align/plot-and-measure/measure-the-distance-between-two-points.html), [Measure area](https://helpx.adobe.com/illustrator/desktop/measure-and-align/plot-and-measure/measure-the-area-of-objects.html); Affinity [Measure Tool](https://affinity.help/designer2/en-US.lproj/pages/Tools/tools_measure.html), [Area Tool](https://affinity.help/designer2/en-US.lproj/pages/Tools/tools_area.html).

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

