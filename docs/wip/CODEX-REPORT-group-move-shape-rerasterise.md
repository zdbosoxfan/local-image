# Group Move stroked-shape cache report

Task: `docs/wip/CODEX-TASK.md`. Continued the existing opus-scan implementation at
`fb32dc3e`, including its 1024-entry / 512 MiB split-cache capacity fix. No commits.

## Implementation

- `pc-compose/src/shape_split.rs` hashes path coordinates relative to an integer origin,
  path operations, fill, every stroke field, live-shape parameters, and the shape pixel cache's
  translation identity. It no longer builds Debug strings of paths or paints.
- Relative coordinates use fine hash buckets to tolerate the final-bit rounding caused by
  adding integer displacements to f64 Bézier handles. Every candidate must also pass an exact
  translated-coordinate and paint comparison; even edits smaller than a bucket invalidate it.
- `pc-raster::Surface::translated` copies encoded pixels without resampling. Whole-tile moves
  share tiles. Clones and subsequent integer translations retain a process-local pixel lineage;
  every mutation invalidates it. This metadata is not serialized and does not change pixel
  equality or the existing revision identity. Exact content bounds are cached per immutable
  surface state, carried through lossless translations, and cleared by the same mutation
  entry points. Non-tile translations trim edge-tile default padding, avoiding blank neighbors
  and unnecessary GPU uploads. The compositor's existing per-tile bounds cache reads these
  known bounds directly when available.
- The engine's existing Move preview translation uses that operation, so newly copied shape
  cache tiles no longer defeat the split cache on every pointer move.
- Cache misses render over `CompiledShape`'s actual fill/stroke bounds, including miter/cap
  reach. That renderer already restricts its bands to compiled bounds; no new guessed margin
  or vector rasterizer was introduced. Fitting remains the existing Photoshop pixel-coverage
  and edge-colour calculation. Normal bounded entries retain off-canvas pixels and clip only
  when placed. Inverted fills and geometry exceeding the cache memory budget use canvas-scoped
  entries, with reuse restricted to that canvas and position.
- GPU planning continues to use the same split function as CPU `split_parts`, so no parallel
  backend implementation or shader changes are needed. Its structure key also uses direct
  field hashing, with exact absolute coordinates (including edits below the split hash bucket).
- GPU plan rebuilds preserve the previous LUT Arc when every f32 bit of the new table matches.
  `frame_res` caches uploaded LUT textures by that identity. Group moves previously minted
  identities for unchanged tables, uploaded them again, and eventually cleared the LUT cache.
  Changed table values still receive fresh identities. This follow-up came from profiling the
  first release measurement after fixing shape splits.
- Levels/Curves programs also have a bounded 64-entry cache of exact parameter bits and depth,
  avoiding the unchanged CPU LUT calculation on a Move rebuild. CMYK programs still use the
  existing uncached path because their tables embed the active profile grids. PSD effect archive
  bytes are excluded from the plan key; rendered effect items and their reference point remain
  in the key. Those bytes are only used for PSD export.

## Correctness and tests

Full changed-crate suites (toolchain 1.98.1, offline, three build jobs):

| Crate | Passed | Ignored |
|---|---:|---:|
| photocraft-raster | 19 | 0 |
| photocraft-compose | 192 | 0 |
| photocraft-engine | 1437 | 15 |
| photocraft-ui-egui | 1032 | 7 |
| photocraft-gpu | 89 | 0 |

The initial UI suite includes 55 GPU tests passed and three pre-existing ignored tests. A
subsequent PSD-lineage regression passed separately (one additional test). The GPU crate's final
full suite includes 27 unit tests, 17 coverage tests, and 45 GPU image tests. Adapter logs name
the NVIDIA GeForce RTX 5090, driver 615.71.09, Vulkan; there were no unavailable-adapter skips.

After the profile-driven follow-ups, focused reruns passed: raster identity/bounds tests 4,
split tests 9 (including the new distant-cache allocation case), existing stroke tests 8,
engine group-preview test 1, GPU plan tests 14. The final UI GPU image rerun passed 56 tests,
with three pre-existing ignored benchmark tests, on the same RTX 5090 Vulkan adapter. The
new actual-pointer drag/readback/commit/undo test passed; no unavailable-adapter skips occurred.
Full suites of the original four changed crates were not repeated, and no whole-workspace
suite was run.

`cargo +1.98.1 fmt -p …` was applied to all five changed crates. Clippy passed for all five
with `CARGO_TARGET_DIR=target/clippy`, `--offline --all-targets -- -D warnings` (one command,
three jobs). `git diff --check` passed. Logs are in `target/*full*.log`,
`target/*final*.log`, and `target/group-move-clippy.log`.
The owner's test-runner wrapper printed timing-accounting arithmetic errors after some
successful runs; Cargo exited zero and all test result summaries passed. The external wrapper
was left untouched.

New coverage:

- Split-cache render counters for positive, negative, and tile-sized integer moves; fresh
  rendering comparisons; raw fill/stroke byte equality for an axis-aligned rectangle.
- Curves, fractional path positions, gradient fill/stroke, dashed strokes, all stroke
  alignments, and RGBA8/RGBA16/RGBA32F shape caches. Exact rendered RGBA comparisons ignore only
  straight RGB behind zero alpha: the existing area rasterizer can leave numerical residue
  there, which has no effect on composited pixels. The vector renderer itself is unchanged.
- Off-canvas edges moving into view, a changed canvas, inverted fills, oversized geometry,
  mutated cache pixels, a cache moved independently of its path, subpixel edits smaller than
  a hash bucket, and distant cached pixels that must not expand the fitter's allocation.
- Surface encoded-byte/default preservation, copy-on-write whole-tile sharing, translation
  chains, snapshots, and invalidation through mutable tiles, take/put tiles, and pruning.
  Known bounds are invalidated through those same entry points, and trimmed translations do
  not allocate blank edge tiles.
- A group Move engine preview preserves each shape cache's source identity.
- The full benchmark layout is saved/reopened as PSD and its moved stroked shapes retain their
  imported pixel identities.
- GPU plan rebuilds reuse unchanged LUTs, invalidate edited tone tables, and distinguish
  subpixel path edits and live-shape parameters. Cached Levels/Curves match uncached tables
  for RGB/Lab and all three depths; channel edits invalidate them. Changing archival PSD
  effect bytes does not rebuild a plan, while changing the rendered reference point does.
- A real app/GPU test presses and drags the group through six pointer frames, reads back the
  actual app canvas against fresh CPU rendering, releases for one undo step, and checks undo.
  The test rejects a software adapter and does not accept an unavailable-GPU skip.
  Added test: `layout_perf::stroked_group_drag_gpu_pixels_match_fresh_render_and_commit`.
  The separate `layout_perf::psd_group_preview_preserves_stroked_shape_pixel_identity` tests
  the import path and pixel lineage in the same integration binary.

## Interactive performance

Required command, run offline with toolchain 1.98.1 and three build jobs:

```sh
WGPU_BACKEND=vulkan VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/nvidia_icd.x86_64.json \
  cargo +1.98.1 run --offline --release -p photocraft-ui-egui --example layout_bench \
  -- --reps 5 --json target/layout-bench-group-move-complete.json
```

Real GPU canvas (`gpu true`, fallback `None`), 4000×3000, 174 layers / 60 text layers,
10.2 MB PSD saved/reopened with zero warnings. The example enables GPU synchronization, so
frame times include the actual composite, and drives the app's real Move pointer input.

| Move group row | Median ms | p90 ms | Max ms |
|---|---:|---:|---:|
| Press | 1.36 | 2.60 | 2.60 |
| Drag frames | 15.45 | 26.06 | 30.13 |
| Release (commit) | 35.46 | 40.40 | 40.40 |
| Of which layer.translate | 15.02 | 22.63 | 22.63 |

**The <8 ms drag target is not met.** The task's supplied median was 19.0 ms; the initial
translation-cache run was 16.97 ms, and a subsequent plan/LUT run was 18.12 ms. These complete
measurements are retained in `target/layout-bench-group-move*.log` and their JSON files; none
used a smaller canvas or software adapter. Release/commit speed did not improve against the
supplied baseline.

Sampling only my offscreen benchmark process confirmed split-cache hit paths and found GPU
texture uploads, command encoding/submission and fence waits, plus tone-program calculation
and PSD effect Debug formatting. The latter two prompted the bounded tone cache and planner-key
changes above. The remaining total-frame target would require further profiling/optimization
of preview copying, GPU input synchronization and plan/composite submission; the measured
frame cost is not all shape rasterization. Profiles are under `target/group-profile*.log`.

The final distant-cache allocation guard was added during the release build, after compose
had compiled. It affects only oversized fit regions; ordinary layout shapes and these timings
are unchanged. It is covered by the subsequent focused split tests.

## Sources followed

- Existing `crates/pc-raster/src/lib.rs`: copy-on-write tiles, encoded region copying, and
  process-local mutation revisions.
- Existing `crates/pc-engine/src/layer_multi_cmds.rs`: Move previews translate shown pixels
  rather than rerendering them; `vector_cmds::transform_shape` moves paths and live parameters.
- Existing `crates/pc-vector/src/lib.rs`: `CompiledShape::bounds` / `render` compute real
  rasterized fill/stroke reach and restrict tile bands to it.
- Existing `crates/pc-compose/src/stroke_tests.rs` and `shape_split::fit_stroke`: Photoshop
  stroke order, cached coverage, and thin-stroke edge colour.
- Existing `crates/pc-gpu/src/plan_cache.rs` and `pc-gpu/src/lib.rs::frame_res`: owned plan
  templates and Arc-keyed resident LUT textures. LUT sharing compares the existing program's
  table bytes rather than introducing another approximation or adjustment implementation.
- [Skia Graphite RasterPathUtils](https://chromium.googlesource.com/skia/+/refs/heads/main/src/gpu/graphite/RasterPathUtils.cpp),
  blob `cd7f2f8768b283475132d28fce5efadba017039e`: raster-mask cache keys omit integer translation,
  include fractional position and stroke parameters, and treat clipped bounds relative to the
  full shape. Followed that design principle; no Skia code was ported. Local exact-coordinate
  verification is stricter than Skia's fractional-position quantization.
- `docs/PORTS.md`, `docs/upstream/photocraft`, and the locally sampled VectorCraft effects
  descriptions were inspected. There is no ready-made implementation of this Surface lineage
  cache; its integration with the existing Rust surface and split cache is project-specific.

## Rules and remaining work

All changes are Rust plus this report. No dependencies, unsafe code, or GUI windows added.
Offline Cargo commands use toolchain 1.98.1 and `CARGO_BUILD_JOBS=3`, one build at a time.
Owner files and the running desktop app were not modified. ComfyUI is not needed for this task.
Remaining limitation: raw straight RGB behind zero alpha can differ from a fresh curve
rasterization, as described above; visible RGBA bytes and the rectangular raw-byte regression
match. The interactive drag target remains unmet and is reported explicitly above. All
validation commands completed successfully. Changes remain uncommitted in this worktree.
