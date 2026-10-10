# Package 30 — optional V6c frame composer

## Status

Owned implementation and integration handoff are supplied. **Optional V6c is not yet an
active, integrated app feature**: package 00 must apply the native option field and app/type
routing described in [integration/30.md](vector-tasks/integration/30.md). The package must
not be marked integrated-complete before those changes and their acceptance gates land.
The final owner speed override prohibits GPU acceptance, broad suites, release builds and
benchmarks in this lane, so none are claimed. No dependency is imposed on required V6 path
type; `on_path.rs` and existing point/path routing were not changed.

Baseline: `614ee823`. Initially only the supplied untracked CODEX-TASK.md was present;
there were no frame_compose source/test/report files to recover. Existing vector, text,
font, renderer, document and engine foundations from previous builders were retained.
No commits were made. Changes are in the working tree.

## Implementation by acceptance point

- **Wrap/reference paragraphs:** adapted the pinned DesignCraft Knuth–Plass total-fit and
  balanced/greedy algorithms onto advances from native pc-text clusters. Work/state caps
  bound the total-fit search and use the port's greedy fallback. No alternate shaper,
  renderer, story/document tree or external dependency was introduced.
- **Justification:** existing paragraph alignment values are retained; elastic word spaces
  use the pinned 80%–133% range, with natural final-line handling. Letter/glyph expansion
  is disabled in the production adapter to keep native glyph/font/style behavior intact.
- **Optical margins/grid:** pinned punctuation protrusion fractions, alignment-aware hanging
  and forward snapping to a finite bounded text-space baseline grid. Existing indents,
  before/after paragraph spacing and native first-baseline ascender metric are retained.
- **Multilingual/native runs:** chosen lines are reshaped by the same TextEngine, preserving
  run styles, ligatures, font fallback, decorations and native caret clusters. Zero-advance
  marks cannot be split by emergency breaking. The detached source TextLayer is unchanged.
  Bidi/RTL, vertical, tabs, discretionary/dictionary hyphenation, unsupported controls and
  warped text explicitly select native layout. Dictionary hyphenation data is not ported.
  These are reported fallback routes, not claims of enhanced composition for every script.
- **Overflow/default identity:** Composition exposes the first unplaced/overfull byte.
  Finite/bounded sizes, transforms, grid, text/runs and clusters are checked. Defaults are
  off; native default/point/complex-text fixtures compare glyphs, face data, styles, ranges,
  decorations and lines. No shared serialization was changed: old-document loading remains
  unchanged; the proposed additive native metadata uses serde defaults in the 00 fragment.
- **Cache preparation:** existing rasterizer only, with a pre-allocation ink/logical-area
  budget of 16,777,216 pixels, document clipping and aggregate edited-cache budget. UI
  preparation regenerates PSD TySh from original text/runs and the resulting layout bounds.
- **Actual UI input/undo:** owned controls and text-space width handle provide detached,
  non-journaled previews and one Session::edit on apply/release/Enter. Document ID/revision
  and target IDs are captured. Entire sets are checked for duplicates, locks and valid Box
  type before cache work. Staleness, Escape, focus loss, pointer loss and disabled routing
  cancel previews. Preview failures drop transient state. Paragraph alignment/width edits
  use existing persisted TextLayer fields. Nondefault composer options require a native
  Metadata implementation; NativeOnly refuses their commit without publishing anything.
- **App/GPU:** supplied a real offscreen full-app source gate in
  `crates/pc-text/tests/frame_compose/app_gpu.rs`, requiring RTX 5090/Vulkan/discrete,
  Rgba32Float accumulation and required float-canvas support. It uses actual app option
  clicks and held width/release pointer events, verifies detached model/history, layout
  updates, undo/cancel and actual document-texture premultiplied max diff <= 1/255. This
  source is intentionally unregistered until its named native State/Metadata/router hooks
  exist; it has not been compiled or run and is not counted as a passed or ignored gate.
- **Integration/i18n/attribution:** exact public call signatures, native field/Metadata
  fragments, type-refresh/caret/outline/export hooks, preview routing, GPU registration and
  one PORTS row are in integration/30.md. New leaf-module registration is append-only per
  the final task override. UI uses existing i18n::t; 14 catalogue rows are in i18n/30.tsv.
  Manifests, lockfile, core router, native shared data and aggregate test catalogues were
  left to their owners. No file adapter was created.

## Verification

All cargo builds used `--offline`, Rust 1.98.1, CARGO_BUILD_JOBS=3 and the inherited
RUSTC_WRAPPER=/home/zdavidson/.local/share/local-image-dev/bin/li-rustc-gate. Only one cargo
build ran at a time. Formatting used cargo fmt for the two changed crates and rustfmt for
unregistered GPU acceptance source.

- `cargo +1.98.1 check --offline --workspace --all-targets`: passed once (2m37s).
- `CARGO_TARGET_DIR=target/clippy cargo +1.98.1 clippy --offline -p photocraft-text
  -p photocraft-ui-egui --all-targets -- -D warnings`: passed (1m34s). Final follow-up after
  the combining-mark and optical-measure corrections is recorded below.
- `cargo test --offline -p photocraft-text --test frame_compose frame_compose_`: initial
  run 5 passed / 1 failed / 0 ignored / 0 filtered. The failed test incorrectly prohibited
  Parley's legitimate separate combining-mark caret cluster. Corrected the independent
  oracle to require that it remain on the base's line, and guarded the emergency adapter
  against splitting zero-advance clusters. Only that test was rerun: 1 passed / 0 failed /
  0 ignored / 5 filtered. All 6 text integration tests have passed.
- `cargo test --offline -p photocraft-text --lib frame_compose_`: 1 passed / 0 failed /
  0 ignored / 83 filtered. Breaker tier-order test only.
- `cargo test --offline -p photocraft-ui-egui --lib frame_compose_`: 5 passed / 0 failed /
  0 ignored / 1040 filtered. Actual width drag, numeric typing/Enter, paragraph checkbox,
  composer-preview/cancel/rejected persistence, whole-set preflight and undo. Build 7m53s,
  test execution 0.05s; these are validation timings, not interactive performance claims.
- A review correction made the overflow indicator respect permitted optical-margin
  protrusion. Added an independent narrow-frame punctuation assertion and ran only its
  containing test again: 1 passed / 0 failed / 0 ignored / 5 filtered. Korean word-breaking
  flags also follow each native language run instead of being applied paragraph-wide.
- Final strict clippy follow-up on both changed crates/all targets: passed (19.51s).
- Distinct new CPU/UI tests passing: 12 (6 text integration + 1 breaker unit + 5 UI).
  Two targeted reruns above; no repeated whole-crate or workspace suite.
- `git diff --check`: passed; no unrelated source edits.

Commands above abbreviate the repeated PATH/CARGO_BUILD_JOBS assignments. Build/test logs
are retained under `target/refvec/frame30-*.log`. No full-crate/workspace test suites, GPU
runs, release builds, performance measurements or benchmarks were executed, following the
final owner speed override. There are no speed claims or GPU hardware-success claims.

## Sources and remaining gates

[DesignCraft breaker at the exact pin](https://github.com/storytold/designcraft/blob/66c7ce7e96ec286a091878cc2541ee8369aa79d6/crates/compose/src/breaker.rs)
provided total-fit fitness/demerits, balanced breaking, elastic capacities, optical fractions
and kinsoku rules. Copyright (c) 2026 ArtCraft Team and the DesignCraft contributors;
MIT OR Apache-2.0 notices are retained next to the adapted file. One per-file PORTS row
is supplied for 00's attribution regeneration. Original source/licence hashes, native
library versions, bundled-font parameters and fixture licences are recorded in
[fixture SOURCES.md](../../crates/pc-text/tests/frame_compose/SOURCES.md).

The native adapter, guards, transaction boundary and UI are original Local Image code,
following this branch's pc-text/layout.rs, render.rs and existing path-text transaction /
GPU observer patterns. No upstream full application or renderer was transplanted.

The build explicitly warned that craft-fonts are absent and Chinese/Japanese may render
as empty boxes. Unicode fixtures prove cluster/byte-range preservation and native fallback;
they do **not** count as successful multilingual font-quality gates. No proprietary/reference
paragraph corpus or external multilingual golden corpus was provided. Those gates remain
missing separately from the synthetic tests.

Required before activation: 00 native persistence plus save/reload/default-absence tests;
actual app/context/canvas preview routing; all type refresh/caret/outline/export consumers
respecting enabled options; i18n/attribution regeneration; registration/compilation/execution
of the supplied real-GPU app gate and coordinator broad acceptance. The shared-file ownership
contract prevents this lane from independently modifying those files. NativeOnly deliberately
makes that dependency visible instead of pretending a transient cache is persisted behavior.
