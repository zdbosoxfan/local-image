# Vector V0 — PhotoCraft vector sync

Continued the previous builder's work on `worktree-agent-a402385380348640e`.
Merged `wip/vector` with `--no-commit`, resolved `shape_split.rs`, and made no commits.
The merge is left ready for the coordinator: there are no unresolved index entries.
The broad staged changes are the requested integration merge; inspect V0 separately with
`git diff wip/vector -- crates/pc-vector crates/pc-compose crates/pc-gpu crates/pc-engine docs/PORTS.md`.

## Task points

- **#865 — Direct Selection:** already included in the initial Local Image import
  `c18d18cf613422fe08d31001e1f803cf8ef7dd68`. `pc-vector/src/edit.rs`,
  `pc-engine/src/path_edit_cmds.rs`, `pc-ui-egui/src/direct_select.rs`, and its tests
  are byte-identical to the requested upstream revision. Retained the existing tool,
  commands, shape re-rendering, handle/anchor editing, previews and single-step undo.
- **#1321 — shape gradients:** ported the exact upstream vector implementation and tests,
  plus the compositor parity test. Shape fills and strokes now honour sorted colour and
  opacity stops, colour midpoints, centre offset, dither and alignment. Linear/reflected
  gradients use upstream's pixel endpoint snapping and pixel-corner sampling; all five
  styles follow the existing PhotoCraft compositor. `CompiledShape` receives the canvas
  frame separately from the bounded render region, so unaligned gradients remain anchored
  to the canvas. No SVG gradient semantics were substituted for Photoshop's model.
- **#747 — bounds overflow:** already included, with the three rasterizer regression tests
  and engine extreme-coordinate/feather test. The rasterizer matches the requested upstream
  file byte for byte. Retained saturated exclusive pixel bounds and feather padding.
- **Previous builder's #1495 — sample depth:** retained and reapplied to today's cache.
  Split fill/stroke surfaces use the document's U8/U16/F32 depth; depth is in the cache key
  and is checked on a hit. Fitted float stroke RGB retains HDR values. CPU and GPU callers
  pass the same document depth. Kept the builder's precision/cache tests and expanded
  depth coverage of GPU stroke-effects and clipped-shape tests.
- **Crash checkpoint #1807 (issue #992):** retained the existing feather/density-aware vector
  mask rasterization and its 24-case regression (three depths, eight mask configurations).
  The checkpoint's implementation/test diff matches the upstream change. It is included
  in engine validation and attribution rather than discarded during the merge.
- **Cache conflict resolution:** kept the Opus scan's 1024-entry / 512 MiB bounds, direct
  field hashing, exact translated-coordinate verification, off-canvas retention, sparse
  placement, translation lineage, huge-geometry clipping and group-move caching.
  The allocation threshold now accounts for the output sample size. Layer-aligned paint
  without dither still reuses integer translations at every depth. Canvas-anchored gradients
  include and verify the exact canvas; dither depends on world coordinates, so translated
  pixels cannot be reused for those paints. This cache adaptation is local work derived
  from upstream's frame/dither inputs, checked against independent fresh rendering.
- **Attribution:** added pinned per-file PhotoCraft rows in `docs/PORTS.md`, covering the
  audited ports, new gradient port and retained builder changes. Existing MIT/Apache licence
  notices are retained; the application remains GPL-3.0-or-later.

No document fields, settings fields, native/PSD format versions, dependencies, C/C++ code,
`-sys` packages or unsafe blocks were added by V0. `Cargo.toml` and `Cargo.lock` are identical
to `wip/vector`; differences against the old branch HEAD come from the required merge.
Existing serialized documents/settings keep their schema and defaults.

## Sources followed

The local read-only upstream Git objects and public GitHub API responses were inspected
before porting. The spec's claim that all three commits postdated the fork was corrected by
checking actual imported source, rather than re-porting existing work.

- [PhotoCraft #865](https://github.com/storytold/photocraft/commit/75ff41a26beb445413f6ee9248eb40b42152bd77), MIT OR Apache-2.0.
- [PhotoCraft #1321](https://github.com/storytold/photocraft/commit/d438f842256435dba7aa41ca306656a216b98fbf), MIT OR Apache-2.0.
- [PhotoCraft #747](https://github.com/storytold/photocraft/commit/8302f57190c83439adb961ae23b9b110e819dea3), MIT OR Apache-2.0.
- [PhotoCraft #1495](https://github.com/storytold/photocraft/commit/5355221f81e325a8682c7e3a7e03dd66cc00b5e6), MIT OR Apache-2.0.
- [PhotoCraft #1807](https://github.com/storytold/photocraft/commit/d4884d33fa83486be13172dc7c9a0182a7ac94c3), MIT OR Apache-2.0.
- The existing group-move implementation/report and `pc-ui-egui/examples/layout_bench.rs`
  provide the established cache design and real-app measurement path. No new geometry,
  SVG/PDF, Affinity, UI-persona or rendering architecture was designed in V0.
- [PhotoCraft corpus pins](https://github.com/storytold/photocraft/blob/25e349b048f37615d7986774ff7714ba0822646e/xtask/src/corpus_pins.rs)
  and the corresponding `psd-corpus.sha256` select the 170 real-file fixtures.
  Inputs are from [psd-tools](https://github.com/psd-tools/psd-tools/tree/96eb134c17b2c65edf4c4151c0f00b802ada86c2/tests/psd_files)
  and [ag-psd](https://github.com/Agamnentzar/ag-psd/tree/387049670cb89b88fb8fe1b7c01aeacf98dd2e3b/test), both MIT.
  Every downloaded fixture was SHA-256 verified; upstream licence files accompany them.
  Inputs and raw reference/build logs stay under gitignored `target/refvec/v0/`.

## Verification

Final validation passed. Each V0/WIP crate's full suite was run once, sequentially, after
development. All Cargo builds used `cargo +1.98.1`, `--offline`, and `CARGO_BUILD_JOBS=3`.
Formatting passed for the four crates. Clippy passed for all four with `--all-targets`,
`-D warnings` and `CARGO_TARGET_DIR=target/clippy`.

| Full crate suite | Unit tests passed | Integration/coverage passed | Total passed | Existing ignored |
|---|---:|---:|---:|---:|
| `photocraft-vector` | 39 | 42 | 81 | 1 |
| `photocraft-compose` | 146 | 52 | 198 | 0 |
| `photocraft-gpu` | 27 | 63 (including 46 real GPU tests) | 90 | 0 |
| `photocraft-engine` | 846 | 628 | 1474 | 16 |
| **Total** | **1058** | **785** | **1843** | **17** |

Zero failures; all doc-test targets also passed (zero doc tests). The ignored tests are
existing opt-in benchmarks, full-registry fuzz/child entry point and manual or user-fixture
checks, not adapter skips. No new test is ignored. The GPU log contains no unavailable
adapter, software adapter or skip result. Raw logs and the exact sequential validation
script are under `target/refvec/v0/`.

Focused development checks passed:

| Check | Result |
|---|---|
| Compose compile check | Passed |
| Split-cache regressions | 11 passed |
| Compose gradient cases, including coverage tests | 23 passed |
| Engine path editing, extreme bounds and retained mask rasterization | 7 passed |
| Direct Selection egui input tests | 7 passed |
| Offscreen stroked-group drag with GPU pixel readback, commit and undo | 1 passed on RTX 5090 Vulkan |
| Vector IO and real-file corpus | 5 passed; 116/116 plain shape layers above 0.97 IoU, 587 vector blocks byte-identical |

The GPU addition `shape_gradient_fields_strokes_clips_and_effects` covers all five gradient
styles, both alignments and all three sample depths, with nondefault gradient fields,
gradient strokes, clipped layers and a stroke effect. It compares both the regular and
paged compositor with the CPU before and after movement: 60 scenarios / 120 comparisons,
including a shape crossing the 256-pixel page seam. The two retained stroke-effects and
clipped-shape tests also cover all three depths. The real adapter is NVIDIA GeForce RTX
5090, Vulkan, discrete GPU, driver 615.71.09; no software-adapter or missing-adapter result
is accepted as validation.

## Interactive app measurement

Ran the existing `layout_bench` through the actual `PhotocraftApp`, egui pointer input and
app GPU canvas, offscreen with `PHOTOCRAFT_GPU_SYNC=1` so GPU completion is timed. The
4000×3000 RGB8 layout has 174 layers and 60 text layers, nested groups, stroked shapes,
effects, masks and smart objects. It was saved and reopened as a 10.2 MB PSD with zero import
warnings. The app reported `gpu true`, `fallback None`. Three repetitions were measured
with the workspace's optimized development profile (workspace opt-level 1, dependencies 2).

| Interaction | Median ms | p90 ms | Maximum ms |
|---|---:|---:|---:|
| Idle frame | 0.94 | 1.01 | 1.01 |
| Shape press | 1.52 | 1.80 | 1.80 |
| Shape drag frame | 6.32 | 6.76 | 8.21 |
| Shape release/commit | 9.19 | 9.27 | 9.27 |
| Group press | 1.40 | 1.43 | 1.43 |
| Group drag frame | 13.86 | 16.74 | 19.97 |
| Group release/commit | 35.68 | 44.78 | 44.78 |

These are current app-path diagnostic timings, not a before/after speedup claim or the
later V5 release-build performance gate. Group release exceeds 16 ms in this run. The
existing cache design was preserved; V0 does not add a group-commit performance rewrite.
The full timing JSON is `target/refvec/v0/app-timings.json` and the log is
`target/refvec/v0/app-bench-timings.log`. A separate offscreen screenshot pass saved eight
fit/100% drag/release views in `target/refvec/v0/shots/`; the group-drag view was visually
inspected. No desktop window was opened. The external test runner emitted post-run
accounting arithmetic diagnostics for the screenshot pass and full GPU suite. The
executables had completed successfully, with saved screenshots and 46 passing GPU tests;
Cargo returned success. The timing pass and GPU input regression completed successfully.
The external runner was not modified.

## Remaining work / handoff

V0's implementation and acceptance checks are complete. No required V0 work remains.
Whole-workspace milestone validation and later Vector phases remain with the coordinator,
as specified by the task's testing budget. The later release performance gate was not run
or claimed here. No commits were made, no owner documents/settings were touched, and no
desktop app was stopped. ComfyUI was not needed for this vector task.

The temporary `corpus` symlink was removed after validation; all 170 verified inputs remain
under `target/refvec/v0/corpus/`. `git diff --check wip/vector` passes, there are no unmerged
index entries, and V0 adds no manifest or lockfile changes relative to `wip/vector`.
