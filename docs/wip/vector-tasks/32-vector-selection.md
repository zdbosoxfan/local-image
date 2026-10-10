# Codex task: vector selection (VG selection, V3)

Report file: `docs/wip/CODEX-REPORT-vector-32-vector-selection.md`. Integration branch: `wip/vector`.

Read `docs/specs/vector-architecture.md`, `docs/specs/vector-persona-spec.md` including Owner decisions, `docs/specs/vector-quality-plan.md`, `docs/wip/vector-tasks/README.md`, and `docs/wip/REPORT-vector-gaps.md` (the gap audit that created this package). Continue the existing branch work; inspect owned files and current tests before changing anything. Carry out this package completely, autonomously, within the shared integration contract.

## Scope

Implement the vector selection family that Illustrator and Affinity users expect. **Group Selection** tool: first click selects the object inside its group, each further click on it adds the next parent group; Alt with the Node tool temporarily switches to it. **Vector Magic Wand** (Y in the Illustrator keymap): click selects objects whose fill colour, stroke colour, stroke weight, opacity or blend mode match within per-attribute tolerances (colour tolerance in Oklab ΔE); Shift adds, Alt subtracts. **Object Lasso** (Q): with object tools it selects objects the loop touches; with the Node tool it selects the nodes it encloses across all selected curves; Shift/Alt add/subtract. **Select › Same**: Fill Color, Stroke Color, Fill & Stroke, Stroke Weight, Opacity, Blend Mode, Appearance, Appearance Attribute, Graphic Style (once 38 lands), Shape type (live shape kind), Width/Height/Rotation with <, ≤, =, ≥, > and Name. **Select › Object**: All on Same Layers, Open Paths, Stray Points, Clipping Masks, Text Objects, Groups, Symbol Instances, Unfilled, Stroked/Unstroked, with a Select Hidden toggle. Vector-aware Select Inverse, Next Object Above/Below, All on Active Artboard and Reselect. All queries respect locked/hidden layers, the Edit All Layers scope and the isolation scope from 37. Selection changes go through the existing session selection; they are never document edits.

## Files owned

pc-engine/src/vector_packages/selection.rs and selection/ tests; pc-ui-egui/src/vector_packages/selection.rs and selection/ tests (Group Selection, vector Magic Wand and object Lasso tool states and overlays). Existing pc-engine/src/select_extra_cmds.rs, pc-ui-egui/src/lasso_ui.rs, the pixel Magic Wand and 03's direct_select.rs are read-only; request bridges through 00.

Also owns `docs/wip/vector-tasks/integration/32.md`, `docs/wip/vector-tasks/i18n/32.tsv`, and its report/fixture source records. Core registries, manifests/lock, aggregate test registration and i18n catalogs belong to 00 unless explicitly owned above. No competing edits: provide exact fragments/call signatures and coordinate through the integration lane. Read-only shared types are not an invitation to expand them independently.

## Interfaces used

Layer tree/Group content and LayerId order; Layer.vector appearance through 08's public reader (legacy fill/stroke before 08 lands); V1 contains/nearest/bounds for hit and loop tests; existing session selection set and Edit All Layers state; 07 hit query read-only; 37 isolation-scope query; 38 graphic-style link query. Commands register in a new compiled `vector_packages::selection` slot requested from 00 (journal=true for edits, journal=false for queries/previews); tool IDs, labels, icons and shortcuts are supplied to 01/46 as fragments.

Every gesture captures document ID/revision and target IDs. Preview is detached, non-journaled and cancellable. Commit checks staleness, validates the entire target set, prepares bounds-limited caches, then executes one engine command/Session::edit. Old docs/settings load; numeric inputs are finite and bounded. File adapters use `vector_design::exchange` rather than depending on UI/engine.

## Acceptance

Engine oracles: Select Same Fill Color over 6 shapes (3 red, one red inside a nested group, one red on a locked layer) selects exactly the 3 eligible ones, and the nested one only with Edit All Layers semantics as documented; Stroke Weight ≥ comparison; Appearance compares whole stacks (order matters); Open Paths and Stray Points detection on seeded fixtures; Magic Wand tolerance boundaries at ΔE just below/above the threshold. Kittest real clicks: Group Selection clicking a grouped circle twice moves the Layers panel highlight from the circle to its group; object Lasso around 3 of 5 shapes; Node-tool Lasso selects 2 of 4 nodes and Delete (03) leaves the 2 others; Magic Wand click then Shift-click adds. Selection commands leave document revision, journal and undo stack untouched. Real-GPU app: Select Same on 500 shapes draws selection overlays within a warm 16 ms frame (release, measured) and canvas pixels are unchanged.

Tests must exercise behavior and independent oracles, not mirror implementation. UI tests really click/drag/type, and full-app GPU checks exercise live preview and release. Use an offscreen app/harness; do not open desktop windows. GPU gates assert RTX 5090/Vulkan/discrete adapter and required float format: llvmpipe or a skipped adapter does not pass. Pixel parity uses premultiplied max diff<=1/255. Performance claims require release real-app pointer-frame measurements and hardware identity. Pure data/format packages supply interaction/render tests via 00 once integrated rather than pretending the isolated parser is the app.

Run focused changed-code tests while developing, then full tests of only changed crates once and clippy all-targets -Dwarnings; record counts/ignored/missing corpus separately. Supply real-GPU tests even where final renderer is a named dependency. Complete the integrated acceptance before calling the package done.

## Dependencies

Engine queries and tools can start now on the existing model. The Appearance criterion waits 08's reader, Graphic Style waits 38, isolation scope waits 37. Tool registration (groupSelection, vectorMagicWand, vectorLasso) and menu wiring through 01/46/00.

## Sources and attribution

VC [tools/select.rs](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/tools/src/select.rs), [tools/xform/wand.rs](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/tools/src/xform/wand.rs), [engine cmd/select.rs](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/engine/src/cmd/select.rs), [cmd/wand.rs](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/engine/src/cmd/wand.rs); Illustrator [Select objects by characteristics](https://helpx.adobe.com/illustrator/desktop/manage-objects/select-objects/select-objects-by-characteristics.html), [Select object groups](https://helpx.adobe.com/illustrator/desktop/manage-objects/select-objects/select-object-groups-and-nested-object-groups.html), [Magic Wand](https://helpx.adobe.com/illustrator/desktop/manage-objects/select-objects/select-objects-magic-wand-tool.html); Affinity [Selecting objects by attribute](https://affinity.help/designer2/en-US.lproj/pages/ObjectControl/selectByAttribute.html).

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

