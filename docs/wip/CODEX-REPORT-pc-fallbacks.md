# GPU compositor fallback removal

Continued the existing GPU compositor, paging, effect-map programs and owned plan cache on this
branch. No commits, dependencies, unsafe code, serialization changes, GUI windows, or edits
to the owner's files outside this worktree. The supplied `CODEX-TASK.md` is unchanged.

## Completed work

1. **Blend If.** Added a GPU pass matching the CPU's finished-composite operation. It restores
   excluded channels first, tests masked layer content without effects or clipped layers, tests
   adjustment layers by their finished result, skips absent content/backdrop, and mixes back
   towards the original backdrop in premultiplied color. Gray/R/G/B ranges, both sides, split
   ramps and inclusive unsplit endpoints are supported. Grayscale/duotone honor Gray and their
   first channel; CMYK/Lab ignore Blend If just as the CPU does. Groups, clipping, masks and
   effects use the same wrapper. Converted inputs retain full float precision on plans using
   Blend If or native tone adjustments: half-float rounding crossed hard Blend If endpoints,
   and RGB8 conversion of CMYK colors amplified native Levels errors. Following the RTX 5090
   results, these plans also upload RGBA8 as CPU-decoded float32. Integer bit arithmetic
   preserves the CPU's channel/luma/white-subtraction rounding at inclusive endpoints;
   the earlier residual normalization was removed. See the 5090 follow-up below.
2. **Noisy shadows/glows.** WGSL applies the CPU's fixed seed (`0x9e3779b9`), wrapping per-pixel
   coordinate hash and monochrome coverage perturbation after the contour and before the inner
   shape gate. Drop/Inner Shadow and Outer/Inner Glow, softer/precise techniques and center/edge
   sources are covered. Noise remains anchored to document coordinates after layer moves.

   A necessary reference detail: the CPU's f64 running-sum blur leaves tiny positive residues
   outside its mathematical support. Its noise operation tests `v > 0`, turning those residues
   into visible speckles; f32 GPU convolution cannot reproduce their signs. A cached binary CPU
   eligibility map preserves this behavior, cropped/uploaded for each GPU window. Coverage,
   the hash, noise, effects and final compositing still run on the GPU. The CPU builders expose
   their existing pre-noise coverage without changing CPU output. Eligibility changes invalidate
   noisy programs across their full window, including residue changes beyond local blur damage.
   These gates share the effect-cache budget and are cleared on closure/limit/error resets.
   Cold renders and shape/settings edits of noisy layers therefore still have CPU preprocessing;
   unchanged noisy layers reuse it. This is a limitation of preserving the exact CPU reference,
   not a synchronous CPU composite fallback.
3. **Native CMYK/Lab Levels and Curves.** Uses the CPU's depth-specific tone LUTs, CMYK ink
   brightness (`1 - ink`), Lab normalization, ignored Lab master, CMYK black channel, identity
   handling, and difference of before/after round trips. Lab conversions execute in WGSL.
   CMYK uses the actual CMS accelerated device-link tables (including white fixes and accurate
   output shapers), tetrahedral 3D interpolation and linear-over-tetrahedral 4D interpolation.
   Tables are exported read-only through `pc-cms`/`pc-color`; the CPU conversions are unchanged.
   Both built-in coated CMYK and an embedded uncoated profile are tested, including switching
   profiles on the same document.
4. **Wide blur LUTs and aprons.** Effect LUTs now span 4096-wide rows; WGSL indexes all taps
   instead of truncating/refusing the table. Convolution loops visit only in-bounds input taps.
   Effect windows clip to their actual region and split a page into smaller work windows when
   its apron plus the original page would exceed the real device limit. Source pages remain
   unchanged, effect window keys stay stable across damage updates, and submission throttling
   also runs within subdivided pages. A 4096-limit regression exercises an apron that previously
   refused a 2048 page; another test uses a blur exceeding 4096 taps. If the conservative apron
   cannot fit even one output pixel and the full effect region is larger than the device limit,
   the document still falls back: fully paging intermediate effect stages is not implemented.
   Distance fields now retain the full consumer reach inside a large region; the CPU's
   512-pixel exterior margin is not a cap on those interior distances. A wide precise-glow
   regression covers pixels farther than 512 pixels from their source shape.

The existing plan structure key already records exact Blend If ranges, all effects (including
noise amount), adjustments/channel space, document mode/depth and profile identity. Added
invalidation coverage for these inputs, and included child Blend If/channel restrictions and
mode/depth/profile context in group effect-shape keys. Parallel shape workers explicitly enter
and restore the document's CMYK/Lab thread context. Noise has a fixed algorithm seed rather
than a new serialized parameter.

## GPU coverage

New parity tests in `crates/pc-gpu/tests/parity.rs`:

- `blend_if_ranges_depth_modes_and_boundaries`
- `blend_if_groups_clipping_adjustments_and_damage`
- `noisy_shadows_and_glows_depth_modes_and_pages`
- `noisy_maps_hash_damage_move_and_setting_changes`
- `native_cmyk_lab_tones_depth_blends_masks_clipping`
- `former_fallbacks_combined_damage_and_pages`
- `blur_lut_spans_multiple_rows`
- `oversized_effect_apron_splits_page_work`
- `noisy_group_shape_tracks_blend_if_edits_and_document_mode`
- `noisy_blur_damage_keeps_reference_residue_speckles`
- `wide_glow_fields_remain_exact_inside_the_reference_region`

These use the existing 1/255 maximum premultiplied error bound; the coordinate-hash check also
requires alpha error below 0.001. Matrices cover 8/16/32-bit documents, multiple blend modes and
opacities, masks, channel restrictions, clipped adjustments, isolated/pass-through groups,
negative origins, slider/amount/channel edits, moves, undo, partial damage and 256-pixel paging.
Supported-adapter pipeline construction errors now fail parity tests instead of silently skipping.
The synthetic noisy-group Blend If/mode edits purge the CPU effect cache before comparison:
that pre-existing cache omits those inputs and would otherwise return stale reference maps.
CPU cache behavior was left unchanged.

Two UI canvas tests cover all three features together on a tiled canvas with partial damage,
and native CMYK adjustments with an embedded profile. They require the GPU refresh kind and
no fallback reason, with the canvas's existing 3/255 display bound. UI behavior was not changed.

## Validation

All builds used Rust 1.98.1, offline dependencies, `CARGO_BUILD_JOBS=3`, and one Cargo
build at a time. Tests used `RUST_TEST_THREADS=1`.

The final full test run passed:

```sh
PATH="$HOME/.cargo/bin:$PATH" CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=1 \
  cargo +1.98.1 test --offline -p photocraft-gpu -p photocraft-compose \
  -p photocraft-ui-egui -p photocraft-engine -p photocraft-cms -p photocraft-color \
  -- --skip ai_commands_against_the_mock_server \
  --skip path_and_selection_ai_and_generated_layer_buttons_work
```

Counts include unit, integration and doc tests; ignored benchmarks are excluded:

| Crate | Passed | Ignored | Filtered |
| --- | ---: | ---: | ---: |
| photocraft-cms | 44 | 1 | 0 |
| photocraft-color | 22 | 0 | 0 |
| photocraft-compose | 134 | 0 | 0 |
| photocraft-engine | 846 | 15 | 1 |
| photocraft-gpu | 66 | 0 | 0 |
| photocraft-ui-egui | 967 | 6 | 1 |
| **Total** | **2079** | **22** | **2** |

GPU coverage is 22 unit tests, 3 device-loss tests, 40 parity tests and 1 vector-mask test.
The tests executed on Vulkan **llvmpipe (LLVM 22.1.8, 256 bits)**, Mesa 26.2.3; they did
not skip for lack of an adapter. The new canvas tests and all four active tests in
`gpu_canvas_perf.rs` passed. GPU parity kept the existing 1/255 bound. Adapter details
are also printed by the tests for hardware verification.

Two existing mock-server tests cannot bind a local socket in this sandbox. The unfiltered
engine run failed at `ai_cmds::tests::ai_commands_against_the_mock_server`; the subsequent
full UI run failed at `context_bar::tests::path_and_selection_ai_and_generated_layer_buttons_work`
(`context_bar_tests.rs:325`). Both fail at `MockComfy::start()` with OS error 1,
`PermissionDenied`, “Operation not permitted.” They were left unchanged and excluded from
the final run. This is a second sandbox socket restriction beyond the engine test anticipated
by the task; the other 899 UI unit tests passed in that attempted run.

Clippy passed for every changed crate, including all targets, with warnings denied:

```sh
PATH="$HOME/.cargo/bin:$PATH" CARGO_BUILD_JOBS=3 CARGO_TARGET_DIR=target/clippy \
  cargo +1.98.1 clippy --offline -p photocraft-gpu -p photocraft-compose \
  -p photocraft-ui-egui -p photocraft-color -p photocraft-cms --all-targets -- -D warnings
```

`cargo +1.98.1 fmt` and its `-- --check` passed for the same five crates.
`git diff --check` passed; dependency manifests and `Cargo.lock` are unchanged.
Full test and lint logs are `/tmp/pc-fallbacks-passing-tests.log` and
`/tmp/pc-fallbacks-clippy.log`.

## Benchmark and RTX 5090 commands

The existing ignored `bench_gpu_canvas_24mp` now includes a 6000×4000 document with Blend If on
the masked texture, a noisy glow on the type layer, and a Lab lightness curve. It times the
same full document through the CPU reference and GPU canvas (including GPU completion), prints
median/min/max for three samples, and asserts GPU refresh with no fallback. The original
cold/warm refresh, brush, stroke and thumbnail measurements remain.

The ignored benchmark was executed successfully from the full suite's built test binary
(`target/debug/deps/gpu_canvas_perf-121a0ef3c7899b1f bench_gpu_canvas_24mp --ignored --nocapture`,
with `RUST_TEST_THREADS=1`). It passed with `gpu-full` and no fallback. These are local
llvmpipe software-Vulkan measurements in the optimized test profile (workspace opt-level 1,
external dependencies opt-level 2), **not release or RTX measurements**:

| 6000×4000, Blend If + noisy glow + Lab curve | Median ms | Min ms | Max ms | Samples |
| --- | ---: | ---: | ---: | ---: |
| CPU reference full render | 1235.93 | 1189.26 | 1247.82 | 3 |
| GPU canvas full refresh, including completion | 1157.46 | 889.71 | 1328.68 | 3 |

The first GPU sample includes cold feature caches and noise eligibility preparation; subsequent
samples reuse them. The complete benchmark passed in 17.44 seconds. Its log is
`/tmp/pc-fallbacks-bench.log`.

No RTX 5090 timings are claimed from this sandbox. Run on the coordinator's hardware:

```sh
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=1
cargo +1.98.1 test --offline -p photocraft-gpu --test parity -- --nocapture
cargo +1.98.1 test --offline -p photocraft-ui-egui --test gpu_canvas_perf -- --nocapture
cargo +1.98.1 test --offline --release -p photocraft-ui-egui --test gpu_canvas_perf \
  bench_gpu_canvas_24mp -- --ignored --nocapture
```

## Remaining limitations

Multichannel ink documents remain on the CPU, as allowed. Patterns exceeding the real texture
limit, adapters without float effect-map targets, GPU errors/loss and aprons too large for one
output pixel retain their existing fallback handling. A CMYK transform without exportable
accelerated grids is declined rather than approximated. No new dependencies were needed.

## 5090 follow-up

The coordinator reported a deterministic failure on RTX 5090 / NVIDIA 615.71.09 / Vulkan:
the U8 Gray inclusive endpoint at code 200 disappeared, with 204/255 premultiplied error,
in three consecutive runs. The preceding llvmpipe run had passed. The previous shader's
UNORM normalization/residual correction and floating multiply/luma expressions could turn
a rounding difference into a hard inclusion decision.

The follow-up removes `F_EXACT8` and the residual-correction trick. Plans containing Blend If
or native tones now upload every color source, including RGBA8, as CPU-decoded RGBA32F.
Ordinary plans retain direct RGBA8 uploads. Existing residency-format invalidation and working
set accounting handle the change. The CPU compositor and sample decoding are unchanged.

Blend If now computes the CPU's f32 channel multiplication by 255, Rec. 601 luma products
and additions, and mirrored white subtraction using **u32 integer bit arithmetic** with
round-to-nearest-even, guard/round/sticky bits and subnormal handling. The 48-bit significand
product uses two u32 words; no 64-bit GPU capability is required. Inclusive comparisons use
ordered nonnegative f32 bit patterns. This keeps division, FMA, contraction, UNORM conversion
and driver float-operation accuracy out of the endpoint decision at every sample depth.

The white subtraction is also reproduced exactly: the CPU can round a float just above a
white slider onto its inclusive mirrored threshold. A direct `v <= white` rewrite would
change that behavior. Likewise, globally rounding Gray or composited colors to 8/16-bit
sample codes would change the CPU's fractional comparisons, so this fix preserves those
fractions instead. No comparison epsilon or parity-bound relaxation was added. Split ramps
classify their endpoints with the same bit comparisons and use float division only in their
continuous interior.

New coverage:

- Unit test `threshold_sources_upload_cpu_decoded_sample_bits` checks actual upload bytes
  at 8/16/32 bits, and verifies that ordinary RGBA8 sources retain their direct path.
- `blend_if_integer_arithmetic_matches_cpu` executes the production WGSL helpers and checks
  exact result bits against Rust f32 for 143,639 cases: every U8/U16 grayscale code, 65,536
  colored byte samples, neighboring F32 slider inputs, arbitrary exponents and subnormals.
- The coordinator's `blend_if_ranges_depth_modes_and_boundaries` test now also checks
  immediately adjacent F32 values, adjacent U16 sample codes, both This/Underlying sides,
  all four channels, hard black/white limits, ordinary split ramps and one-step split ramps.
  It retains the original failing U8 case and the existing 1/255 bound.

The tradeoff is 16 bytes per cached RGBA8 source pixel on these plans instead of 4, CPU
decoding on upload, and additional integer shader work. Cached uploads, paging, eviction
and the GPU compositor remain in use. Native tones share the precise upload path so they
also stop relying on the removed normalization flag.

Follow-up validation passed on the same Mesa llvmpipe Vulkan adapter:

- Full `photocraft-gpu` run: **68 passed**, comprising 23 unit, 3 device-loss, 41 parity
  and 1 vector-mask test. The production WGSL arithmetic matched all reference bits.
- Final `blend_if_` focused rerun: **4 passed**, including the expanded coordinator test,
  arithmetic check, group/clipping test and noisy-group Blend If invalidation test.
- `gpu_canvas_perf`: **4 passed, 3 ignored**, including tiled/damaged former-fallback
  documents and embedded CMYK profiles.
- GPU Clippy, all targets, `-D warnings`, with `CARGO_TARGET_DIR=target/clippy`: passed.
- GPU formatting check and `git diff --check`: passed. No CPU source, dependencies,
  serialization or commits changed in this follow-up.

The current 24 MP ignored benchmark also passed (1 passed, 6 filtered) with no fallback,
from `target/debug/deps/gpu_canvas_perf-b9a3f67901aef87e`. In the optimized test profile
on **software llvmpipe**, the featured document measured:

| Full render/refresh, three samples | Median ms | Min ms | Max ms |
| --- | ---: | ---: | ---: |
| CPU reference | 944.37 | 920.04 | 957.37 |
| GPU canvas including completion | 677.88 | 677.32 | 4699.57 |

The GPU maximum is the first feature refresh, including cold precise source uploads and
feature caches. It exposes a substantial cold cost on this software adapter; the later
samples reuse the caches. These timings are not an RTX estimate or a controlled comparison
against the earlier report's run. The entire benchmark passed in 16.75 seconds.

Logs: `/tmp/pc-fallbacks-5090-gpu.log`, `/tmp/pc-fallbacks-5090-focused.log`,
`/tmp/pc-fallbacks-5090-canvas.log`, `/tmp/pc-fallbacks-5090-clippy.log`, and
`/tmp/pc-fallbacks-5090-bench.log`.

The 5090 rerun remains the coordinator's responsibility; this sandbox exposes llvmpipe,
not NVIDIA hardware. Suggested rerun commands (offline, single Cargo build, three build jobs):

```sh
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=1
cargo +1.98.1 test --offline -p photocraft-gpu --test parity \
  blend_if_ranges_depth_modes_and_boundaries -- --nocapture
cargo +1.98.1 test --offline -p photocraft-gpu --test parity \
  blend_if_integer_arithmetic_matches_cpu -- --nocapture
cargo +1.98.1 test --offline -p photocraft-gpu
cargo +1.98.1 test --offline -p photocraft-ui-egui --test gpu_canvas_perf -- --nocapture
cargo +1.98.1 test --offline --release -p photocraft-ui-egui --test gpu_canvas_perf \
  bench_gpu_canvas_24mp -- --ignored --nocapture
```
