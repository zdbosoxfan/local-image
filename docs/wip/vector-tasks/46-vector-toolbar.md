# Codex task: Vector toolbar, keymaps, fill/stroke proxy and icons (VG toolbar, V2)

Report file: `docs/wip/CODEX-REPORT-vector-46-vector-toolbar.md`. Integration branch: `wip/vector`.

Read `docs/specs/vector-architecture.md`, `docs/specs/vector-persona-spec.md` including Owner decisions, `docs/specs/vector-quality-plan.md`, `docs/wip/vector-tasks/README.md`, and `docs/wip/REPORT-vector-gaps.md` (the gap audit that created this package). Continue the existing branch work; inspect owned files and current tests before changing anything. Carry out this package completely, autonomously, within the shared integration contract.

## Scope

Implement the Vector toolbar agreed in `docs/wip/REPORT-vector-gaps.md` §4. **Layout data**: flyout groups and order for the Vector All Tools, Essentials, Illustration, Layout & Type and vector Photographer sets; tools appear only once their package registers them (no placeholder tools). **Keymaps**: two Vector-persona presets, Illustrator (default) and Affinity, each conflict-free; Pixel persona keys stay byte-identical. **Persona-scoped menu accelerators** for the conflicts listed in the report (today Cmd+D Deselect, Cmd+J Layer via Copy, Cmd+Y Proof Colors, Cmd+U Hue/Saturation, Cmd+B Color Balance, Cmd+T Free Transform, Cmd+Shift+O Open Folder; Cmd+F, Cmd+2/3 and Cmd+7/8, absent from menu_catalog.rs today, are claimed only in the Vector persona after checking shortcut_dispatch): the Vector persona maps them to the Illustrator commands; Pixel keeps Photoshop meanings. **Fill/stroke proxy** under the tools: active fill/stroke (X), swap (Shift+X), default (D), none (/), colour/gradient/none buttons, and the draw-mode button hosted for 35. **Icons**: original Glossy Duotone colour and monochrome icons for every new vector tool ID on the 24 px grid per `docs/wip/CODEX-REPORT-icons-standard.md`, a recognition table and a regenerated contact sheet.

## Files owned

pc-engine/src/vector_toolbar_layout.rs (groups, presets, accelerator overrides) and its tests; pc-ui-egui/src/vector_packages/fill_stroke_proxy.rs and fill_stroke_proxy/ tests; xtask/src/tool_art_vector.rs (geometry for new icons); new SVGs under assets/icons/tools/ and assets/icons/color/ for new IDs only; docs/design/vector-tool-icons.png. The `tool_art::TOOLS`/main.rs hookup is a 00 fragment; toolsets.rs and the Tool registry stay with 01.

Also owns `docs/wip/vector-tasks/integration/46.md`, `docs/wip/vector-tasks/i18n/46.tsv`, and its report/fixture source records. Core registries, manifests/lock, aggregate test registration and i18n catalogs belong to 00 unless explicitly owned above. No competing edits: provide exact fragments/call signatures and coordinate through the integration lane. Read-only shared types are not an invitation to expand them independently.

## Interfaces used

01's Tool registry, Persona and ToolSet model; existing colour_icons/mono_tool_icons loaders; xtask tool-icons renderer; shortcut_dispatch and menu_catalog conflict checks (read-only; conflicts reported to 00); 08 paint commands for the proxy. Commands register in a new compiled `vector_packages::toolbar` slot requested from 00 (journal=true for edits, journal=false for queries/previews); tool IDs, labels, icons and shortcuts are supplied to 01/46 as fragments.

Every gesture captures document ID/revision and target IDs. Preview is detached, non-journaled and cancellable. Commit checks staleness, validates the entire target set, prepares bounds-limited caches, then executes one engine command/Session::edit. Old docs/settings load; numeric inputs are finite and bounded. File adapters use `vector_design::exchange` rather than depending on UI/engine.

## Acceptance

Every registered vector tool appears exactly once in Vector All Tools with both icon styles (enumerating test); no duplicate keys within a preset (as VC's catalog test); switching presets changes dispatch; Pixel shortcuts and menu accelerators match a frozen snapshot; Vector Ctrl+D runs Transform Again while Pixel Ctrl+D still deselects. Proxy: X toggles the active paint, Shift+X swaps on the selected shape with one undo, D resets, / sets none. Kittest: press-and-hold opens a flyout and picks Curvature; Shift+~ selects Curvature; proxy clicks. Icons: freshness test, alpha checks at 16/20/40 px, recognition rows equal the new IDs; the real-GPU icon upload test covers the new icons on the RTX 5090.

Tests must exercise behavior and independent oracles, not mirror implementation. UI tests really click/drag/type, and full-app GPU checks exercise live preview and release. Use an offscreen app/harness; do not open desktop windows. GPU gates assert RTX 5090/Vulkan/discrete adapter and required float format: llvmpipe or a skipped adapter does not pass. Pixel parity uses premultiplied max diff<=1/255. Performance claims require release real-app pointer-frame measurements and hardware identity. Pure data/format packages supply interaction/render tests via 00 once integrated rather than pretending the isolated parser is the app.

Run focused changed-code tests while developing, then full tests of only changed crates once and clippy all-targets -Dwarnings; record counts/ignored/missing corpus separately. Supply real-GPU tests even where final renderer is a named dependency. Complete the integrated acceptance before calling the package done.

## Dependencies

01 registry and persona; the merged icons-standard work; feature packages' tool IDs as they land. Menu wiring through 00.

## Sources and attribution

VC [tools/catalog.rs](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/tools/src/catalog.rs); Illustrator [tools reference](https://helpx.adobe.com/illustrator/using/tools-in-illustrator.html) and [shortcuts](https://helpx.adobe.com/illustrator/using/default-keyboard-shortcuts.html); Affinity [Tools panel](https://affinity.help/designer2/en-US.lproj/pages/Panels/toolsPanel.html) and [tool pages](https://affinity.help/designer2/en-US.lproj/pages/Tools/tools_move.html); `docs/wip/CODEX-REPORT-icons-standard.md`.

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

