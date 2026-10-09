# Codex task: Compositing contextual UI, live shapes, Pen bar (item 3)

Worktree of "Local Image" (Compositing UI: crates/pc-ui-egui; engine: crates/pc-engine; document: crates/pc-doc;
vector: crates/pc-vector). A previous builder made progress and was stopped mid-work: its changes are in the last
WIP commit ("Compositing context bar, Pen bar actions, live shape options") — continue from it (`git show --stat HEAD~1`
and the diff against `fbc02a9` show what's done; e.g. `crates/pc-ui-egui/src/context_bar_tests.rs`).

The owner (a Photoshop user) reported:
1. "I added a shape and had no way to change parameters of the shape once it was placed. Photoshop 101."
2. "Make sure all the tools maintain their contextual menus after we select the items that came from those tools.
   Immensely important."
3. "I made a shape with the Pen tool and the contextual menu appeared but none of its buttons do anything. Make
   Selection, Content-Aware Fill, AI Fill and Mask should be the options."

## Findings (verify; line numbers are hints)

- **Pen bar bug (confirmed in egui 0.36.2):** `context_bar.rs` (~173) returns early when
  `ctx.input(|i| i.pointer.any_down())`; egui clears `potential_click_id` when that widget disappears for a frame, so a
  press on a bar button hides the bar and `clicked()` never fires. Keep the bar drawn during a press that began on it.
  This also broke the Selection bar.
- Path bar = exactly **Make Selection** (`path.toSelection`) | **Content-Aware Fill** (path → selection then
  `edit.contentAwareFill`; `can_caf` needs a pixel layer — when the active layer is a Develop smart object or other
  non-pixel layer, fill onto a new layer sampling the composite) | **AI Fill** (`ai.generativeFill`; empty prompt =
  fill from surroundings like Photoshop's Generative Fill, or show a prompt field) | **Mask** (prefer a vector mask
  `layer.vectorMask.currentPath {"name":"work"}`, fall back to `layer.layerMask.revealSelection`). Content-Aware Fill also
  as a visible button on the Selection bar.
- Shapes are already live (`shape.create` / `shape.edit` in pc-engine vector_cmds.rs; `LiveShape` in pc-doc;
  `vector_ui::shape_properties`). But the shape options bar only edits tool defaults; Fill is an unclickable painted
  rect; no Stroke swatch; the Properties dock group gets collapsed when Generate opens and nothing reveals it; the Studio
  floating card ignores shape layers. Make the options bar (shape tool or Move/Path Selection) show and edit THE
  SELECTED shape layer (Fill + Stroke swatches with colour pickers, stroke width, solid/dashed if feasible, W/H, X/Y,
  corner radius incl. per-corner if `LiveShape` can be extended compatibly, sides) via `shape.edit {…, coalesce}`;
  reveal the Properties group when a shape/text layer is created or selected; Studio card shows shape properties;
  double-clicking a shape thumbnail opens the fill colour picker; Pen-made shapes get at least Fill/Stroke/width.
- **Contextual UI by selected item, independent of the tool:** resolve `Context { Selection, Path, Shape, Text, Smart,
  Develop, Generated, Pixel }` from the active layer + content kind (+ selection / work path; Selection and Path take
  precedence), with per-context button rows (Shape: Fill/Stroke/Radius/Edit Path/Rasterize; Text: quick font/size/colour
  + Edit Text; Develop: Open in Develop; Smart: Edit Contents; Generated: Regenerate/Variations — add an optional
  `generation: Option<serde_json::Value>` on `Layer` written by ai_cmds.rs so AI layers keep prompt/seed; Pixel: Select
  Subject / Remove Background…). The options bar uses the same resolver with Move/Path Selection active. No
  flicker/hide during clicks.

Tests (egui_kittest): every Path-bar and Selection-bar button actually clicked with its effect asserted; selecting a
shape layer with Move shows options bound to it and editing Fill changes the layer; text context; generated-layer
Regenerate offered.

Report file: `docs/wip/CODEX-REPORT-compositing.md`.

## Rules (all Codex jobs)

- "Local Image" is GPL-3.0-or-later and **all Rust**: workspace denies `unsafe`; no new dependencies, no C/C++ or
  `-sys` crates in the build (C only in out-of-build reference harnesses under `target/refvec/`).
- Offline builds (`--offline`). The PC has 60 GB RAM shared with other jobs: ONE cargo build at a time in your job,
  `CARGO_BUILD_JOBS=3`. Use `PATH=~/.cargo/bin:$PATH` and `cargo +1.98.1`. Format: `cargo +1.98.1 fmt -p <crate>`;
  lint: `CARGO_TARGET_DIR=target/clippy cargo +1.98.1 clippy --offline -p <crate> --all-targets -- -D warnings`.
- Run the tests of every crate you change (`cargo +1.98.1 test --offline -p <crate>`); all must pass. Your sandbox has
  **no GPU adapter**: GPU tests print "skipped: no GPU adapter" and pass without running — still write GPU code/tests
  and list them in your report; the coordinator runs them on the real RTX 5090.
- UI is egui 0.36; Compositing UI tests use egui_kittest, Library/Develop uses its own `headless.rs` harness. Add tests
  that actually click/drag the UI you change.
- A Local Image app may be running on the desktop: don't kill it or open GUI windows. Don't touch the owner's files
  outside this worktree (read-only access to reference data is fine).
- Old settings/documents must still load (serde defaults). The owner does not need old edits to render identically.
- **Do not commit.** Leave changes in the working tree. When done, write the report file named in your task: what's
  done per point, tests with counts, GPU tests added, anything left and why.

