# Codex task: view modes and isolation mode (VG view, V5)

Report file: `docs/wip/CODEX-REPORT-vector-37-view-modes-isolation.md`. Integration branch: `wip/vector`.

Read `docs/specs/vector-architecture.md`, `docs/specs/vector-persona-spec.md` including Owner decisions, `docs/specs/vector-quality-plan.md`, `docs/wip/vector-tasks/README.md`, and `docs/wip/REPORT-vector-gaps.md` (the gap audit that created this package). Continue the existing branch work; inspect owned files and current tests before changing anything. Carry out this package completely, autonomously, within the shared integration contract.

## Scope

Implement vector view modes and isolation mode. **Outline** (Ctrl+Y in the Illustrator keymap): every path draws as a 1-device-pixel hairline with no fill, stroke or effects, raster layers as boxes with a cross, text as outlines; hit-testing uses paths, not fills. **X-ray** (Affinity): outlines over faded fills. **Pixel Preview** (Alt+Ctrl+Y) toggles 1x pixel-grid rendering against the crisp 09 view. **Per-layer outline** (Ctrl-click a Layers eye), stored as UI state. **Isolation mode**: double-click a group, compound, clip group or symbol instance (or Alt-click its Layers thumbnail, Affinity) to isolate it; everything else is dimmed and unselectable; an isolation bar shows the breadcrumb; new objects are created inside the isolated container; Esc or double-click outside exits. Isolation exposes a scope query (used by 32 and Select All) and an insertion target (used by drawing tools and 35). Modes and isolation are view/session state, never document edits; persona switch or document switch exits isolation and cancels gestures.

## Files owned

pc-ui-egui/src/vector_packages/view_modes.rs and view_modes/ tests; pc-ui-egui/src/vector_packages/isolation.rs and isolation/ tests; pc-engine/src/vector_packages/isolation.rs (scope query, insertion target) and tests; pc-vector/src/outline_view.rs (hairline geometry from the projection) and tests. gpu_canvas/canvas routing belong to 09/00: supply overlay and paint-hook fragments.

Also owns `docs/wip/vector-tasks/integration/37.md`, `docs/wip/vector-tasks/i18n/37.tsv`, and its report/fixture source records. Core registries, manifests/lock, aggregate test registration and i18n catalogs belong to 00 unless explicitly owned above. No competing edits: provide exact fragments/call signatures and coordinate through the integration lane. Read-only shared types are not an invitation to expand them independently.

## Interfaces used

Shared feature projection (08/09) for display geometry; 09 ViewRequest and scaled-view tiles; existing view_cmds; session selection; Layers panel row hooks via 00; documented insertion-target API for 02/04/05/35. Commands register in a new compiled `vector_packages::view_modes` slot requested from 00 (journal=true for edits, journal=false for queries/previews); tool IDs, labels, icons and shortcuts are supplied to 01/46 as fragments.

Every gesture captures document ID/revision and target IDs. Preview is detached, non-journaled and cancellable. Commit checks staleness, validates the entire target set, prepares bounds-limited caches, then executes one engine command/Session::edit. Old docs/settings load; numeric inputs are finite and bounded. File adapters use `vector_design::exchange` rather than depending on UI/engine.

## Acceptance

Outline view of two overlapping filled shapes renders only hairlines (column-profile test), and a click inside the top shape's fill but on the bottom shape's path selects the bottom one; at 800% zoom the hairline stays 1 device px; X-ray fill alpha matches the documented value; Pixel Preview equals the 1x CPU cache. Isolation: double-click a group, draw a rectangle → new layer inside that group; other layers cannot be selected; Esc exits; persona switch exits. Kittest: Ctrl+Y and Alt+Ctrl+Y keys, Layers eye Ctrl-click, double-click and Esc. Real-GPU: outline render of a 5000-path document within a warm 16 ms release frame (measured), deterministic outline pixels, no document revision change.

Tests must exercise behavior and independent oracles, not mirror implementation. UI tests really click/drag/type, and full-app GPU checks exercise live preview and release. Use an offscreen app/harness; do not open desktop windows. GPU gates assert RTX 5090/Vulkan/discrete adapter and required float format: llvmpipe or a skipped adapter does not pass. Pixel parity uses premultiplied max diff<=1/255. Performance claims require release real-app pointer-frame measurements and hardware identity. Pure data/format packages supply interaction/render tests via 00 once integrated rather than pretending the isolated parser is the app.

Run focused changed-code tests while developing, then full tests of only changed crates once and clippy all-targets -Dwarnings; record counts/ignored/missing corpus separately. Supply real-GPU tests even where final renderer is a named dependency. Complete the integrated acceptance before calling the package done.

## Dependencies

09 scaled view for hairlines at zoom; 08 projection; 32 consumes the scope query; 00 routing. Ctrl+Y is Proof Colors in the Pixel persona: the Vector keymap in 46 owns the persona-scoped accelerator.

## Sources and attribution

Illustrator [Viewing artwork](https://helpx.adobe.com/illustrator/using/viewing-artwork.html), [Isolate objects](https://helpx.adobe.com/illustrator/desktop/manage-objects/select-objects/isolate-objects.html), [shortcuts](https://helpx.adobe.com/illustrator/using/default-keyboard-shortcuts.html); Affinity [View modes](https://affinity.help/designer2/en-US.lproj/pages/GetStarted/view.html), [Isolating objects](https://affinity.help/designer2/en-US.lproj/pages/ObjectControl/isolating.html); VC [ui-egui canvas.rs](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/ui-egui/src/canvas.rs) (view.outline), [engine cmd/object.rs](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/engine/src/cmd/object.rs) (object.isolate/exitIsolation).

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

