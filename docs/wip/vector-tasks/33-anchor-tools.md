# Codex task: anchor tools and Curvature (VG node family, V3)

Report file: `docs/wip/CODEX-REPORT-vector-33-anchor-tools.md`. Integration branch: `wip/vector`.

Read `docs/specs/vector-architecture.md`, `docs/specs/vector-persona-spec.md` including Owner decisions, `docs/specs/vector-quality-plan.md`, `docs/wip/vector-tasks/README.md`, and `docs/wip/REPORT-vector-gaps.md` (the gap audit that created this package). Continue the existing branch work; inspect owned files and current tests before changing anything. Carry out this package completely, autonomously, within the shared integration contract.

## Scope

Implement the Pen flyout's anchor tools and the Curvature tool. **Add Anchor Point** (+): click a segment of any selected curve to insert a node without changing the shape; Alt toggles to delete. **Delete Anchor Point** (−): click a node to remove it while refitting the neighbouring segments to keep the shape within tolerance; Alt toggles to add. **Anchor Point / Convert** (Shift+C): click a smooth node to make it a corner, drag from a corner to pull out symmetric smooth handles, drag a single handle to break it, drag a segment to bend it. **Curvature** (Shift+~): each click adds a point and the path stays a smooth curve through every clicked point; Alt-click or double-click toggles a point between smooth and corner; drag existing points; Delete removes the selected point; Esc/Enter ends; a rubber-band preview shows the next segment; clicking the first point closes. Provide the shared hover helpers (`segment_under`, `node_under`) that let the Pen tool (04) auto add/delete on hover, documented for 00 to wire with Illustrator's "disable auto add/delete" preference. Corner metadata (`Layer.vector.corners`) is remapped in the same transaction; live shapes prompt Convert to curves as in 03.

## Files owned

pc-engine/src/vector_packages/anchor_tools.rs and anchor_tools/ tests; pc-ui-egui/src/vector_packages/anchor_tools.rs and anchor_tools/ tests; pc-vector/src/curvature_fit.rs (interpolating spline through clicked points) and its tests. 03's direct_select.rs, path_edit_cmds.rs and pc-vector/src/edit.rs and 04's pen modules are read-only: call their public operations or request additions through 00.

Also owns `docs/wip/vector-tasks/integration/33.md`, `docs/wip/vector-tasks/i18n/33.tsv`, and its report/fixture source records. Core registries, manifests/lock, aggregate test registration and i18n catalogs belong to 00 unless explicitly owned above. No competing edits: provide exact fragments/call signatures and coordinate through the integration lane. Read-only shared types are not an invitation to expand them independently.

## Interfaces used

Path/Knot and vector_design::KnotCorner; V1 split_at/nearest/fit_path/smooth; 03 node-selection state and edit commands read-only; 07 snapping query; vector_api snapshots and prepared commits. Commands register in a new compiled `vector_packages::anchor_tools` slot requested from 00 (journal=true for edits, journal=false for queries/previews); tool IDs, labels, icons and shortcuts are supplied to 01/46 as fragments.

Every gesture captures document ID/revision and target IDs. Preview is detached, non-journaled and cancellable. Commit checks staleness, validates the entire target set, prepares bounds-limited caches, then executes one engine command/Session::edit. Old docs/settings load; numeric inputs are finite and bounded. File adapters use `vector_design::exchange` rather than depending on UI/engine.

## Acceptance

Engine oracles: inserting a node at t on a cubic leaves the sampled curve within 1e-6 px; deleting one node of an 8-node circle approximation (r=100 px) keeps the Hausdorff distance ≤0.5 px; Convert drag yields collinear, equal-length handles; Curvature with 4 clicks passes exactly through all 4 points with G1 continuity at smooth points and a true corner where Alt-clicked; random insert/delete sequences keep finite valid paths and remapped corner references. Kittest real input: + tool click mid-segment adds exactly 1 node; − tool click removes 1; Shift+C drag on a corner makes it smooth; Curvature click-click-click-Esc creates a 3-node open path; Escape mid-gesture commits nothing; one undo per finished gesture. Real-GPU app: preview and release pixels for each tool on the RTX 5090, exactly one journal entry per gesture.

Tests must exercise behavior and independent oracles, not mirror implementation. UI tests really click/drag/type, and full-app GPU checks exercise live preview and release. Use an offscreen app/harness; do not open desktop windows. GPU gates assert RTX 5090/Vulkan/discrete adapter and required float format: llvmpipe or a skipped adapter does not pass. Pixel parity uses premultiplied max diff<=1/255. Performance claims require release real-app pointer-frame measurements and hardware identity. Pure data/format packages supply interaction/render tests via 00 once integrated rather than pretending the isolated parser is the app.

Run focused changed-code tests while developing, then full tests of only changed crates once and clippy all-targets -Dwarnings; record counts/ignored/missing corpus separately. Supply real-GPU tests even where final renderer is a named dependency. Complete the integrated acceptance before calling the package done.

## Dependencies

Needs 03's node display/selection model and V1 split/fit; can start the fitting module and tool state machines now. Tool IDs (addAnchor, deleteAnchor, anchorPoint, curvature) and icons via 01/46; Pen hover integration via 00 after 04 settles.

## Sources and attribution

VC [tools/draw2/anchor.rs](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/tools/src/draw2/anchor.rs), [tools/draw2/curvature.rs](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/tools/src/draw2/curvature.rs), [engine cmd/path.rs](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/engine/src/cmd/path.rs); Illustrator [Add or remove anchor points](https://helpx.adobe.com/illustrator/desktop/draw-shapes-and-paths/modify-paths/add-or-remove-anchor-points.html), [Convert anchor points](https://helpx.adobe.com/illustrator/desktop/draw-shapes-and-paths/modify-paths/convert-anchor-points-on-a-path.html), [Curvature tool](https://helpx.adobe.com/illustrator/desktop/draw-shapes-and-paths/draw-shapes/draw-shapes-with-the-curvature-tool.html), [tools reference](https://helpx.adobe.com/illustrator/using/tools-in-illustrator.html); Affinity [Node Tool](https://affinity.help/designer2/en-US.lproj/pages/Tools/tools_node.html).

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

