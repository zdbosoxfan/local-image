# Codex task: Vector integration (package 00, final pass) on wip/vector

You are package 00. Every vector package 01–47 (incl. 47 shared tools, report docs/wip/CODEX-REPORT-shared-tools.md) is now merged into this branch (`wip/vector`). Each package left
its shared wiring to you in `docs/wip/vector-tasks/integration/NN.md` (plus `NN-*.tsv/SOURCES.md` and
`docs/wip/vector-tasks/i18n/NN.tsv`). The coordinator already applied only the minimum needed to compile:
crate manifests/lib.rs for pc-svg, pc-pdf, pc-affinity, pc-vecfx; the 08 appearance registration; the 15 symbols
shared-file patch; the 25 centreline routing; `strokes` field initialisers. Check those against the handoffs.

## Goal
A working Vector persona inside the real app: every package's tools, panels, menus, commands, preferences and file
formats reachable and behaving as its handoff says, with the full suites green on the real RTX 5090.

## Do, in order
1. Read every `integration/NN.md` (00–47) and build a checklist table (package, item, status). Apply every
   remaining fragment: engine command aggregate (`crates/pc-engine/src/vector_packages.rs`), UI `vector_packages`
   aggregate and `UiState`/`PhotocraftApp` fields, menus/menu catalog, router/canvas hooks, persona toolbar
   (package 46) with per-persona shortcuts (owner decision), preferences (serde-defaulted), native format
   versioning/migrations, PSD/SVG/PDF routes, test target registrations. Where two handoffs touch the same spot,
   merge both intents. Package 47's shared-tool unification wins over earlier per-persona duplicates.
2. Known defects to fix (with regression tests):
   - Native save/reload loses live repeat (19) and envelope (20) data: shared serde tag collision; both reports
     give the exact repair.
   - HiDPI snapping failure in the UI suite (pre-existing; reproduced without vector hooks by 16 and 22).
   - Engine tests crashing in parallel but passing serially (24 report).
   - Pre-existing pc-trace T1 fill failures (`engine::binary_outputs_are_finite_closed_and_simple`,
     `benchmark::fast_six_image_benchmark`) and 27's seams / excessive node counts.
   - Every "fixed but not rerun under the test-once rule" item in reports 11, 34, 35, 36, 37, 39, 45 (and any later package) — rerun and fix.
   - Symbol instances in the scaled view (09) are resampled from cache by the coordinator as a stopgap; replace
     with a crisp vector projection if the 09/15 handoffs allow.
   - Live Paint (42): 3 failing tests (2 fixed but not rerun) and unfinished narrow-region hit testing — complete per its report.
   - `svg-import-integrated` feature: enable for acceptance now that 13 is merged (14 handoff).
3. Goldens that need this machine: generate Inkscape references required by 31 (and 12/09/41 where listed) with
   `/usr/bin/inkscape` (installed), following those handoffs exactly; commit the goldens with SOURCES notes.
   Two Photoshop corpus gates (23) and external scan/logo corpora (25/26) have no files: record as skipped.
4. i18n: translations are paused. Add every new user-visible string to the pending/KEEP_AS_IS lists so
   `every_tl_literal_is_translated` passes; do not write translations.
5. Full validation ONCE at the end (this is the big run the packages skipped): `cargo check --workspace
   --all-targets`, strict clippy all targets, `cargo +1.98.1 fmt --all`, full workspace tests, and every deferred
   GPU gate with `LC_REQUIRE_GPU=1` on the RTX 5090. Fix failures; rerun only what failed.
6. Pure-Rust rule: no C/-sys crates; check Cargo.lock additions. Regenerate attributions
   (`cargo +1.98.1 xtask attributions`) and PORTS rows if needed.

## Rules
Builds through `RUSTC_WRAPPER=/home/zdavidson/.local/share/local-image-dev/bin/li-rustc-gate`, Rust 1.98.1,
`--offline`. Don't open GUI windows or kill the running Local Image app. Follow official docs and the cited
upstream sources; don't guess. Leave changes uncommitted; the coordinator merges. Report:
`docs/wip/CODEX-REPORT-vector-integration.md` with the checklist table, defects fixed, validation counts, skipped gates.

DeepSeek is reviewing all new vector code in parallel; its findings file
(`/home/zdavidson/.local/share/local-image-dev/ds-review-vector/findings.jsonl`) will be sent to you as a follow-up
message when complete. Verify each finding (DeepSeek is often wrong), fix the real ones with tests, and add a verdict
table to your report.
