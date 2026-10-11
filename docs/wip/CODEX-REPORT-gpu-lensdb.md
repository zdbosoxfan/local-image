# GPU lens-database corrections — implementation report

2026-10-09. Continued the existing branch, including its native GPU capture
implementation and stage cache. No commits, dependencies, unsafe code, build
configuration changes, or GUI activity.

## Completed work

1. **Native warp.** Extended `lc-gpu/src/wgsl/geom.wgsl::sample_warp` and
   `render.rs::warp_params` with lensfun poly3/poly5/ptlens distortion, linear
   and poly3 TCA, and PA vignetting. Distortion precedes TCA; manual distortion
   precedes database sampling, manual CA follows it, and gain uses the final
   green source position. The same source orientation, Mitchell prefilter,
   crop/straighten/flips/perspective and per-channel bilinear sampler are used.
   The map already contains `set_lensdb`'s reoriented optical centre and
   replacement of embedded lens data. Strengths retain the reference's range
   and PA exponent/denominator guard. `Frame::gpu_samplable()` now includes
   database profiles. Native geometry consumes the capture-sharpened device
   source directly.

   Source positions/gains are f32. Distortion coefficient × strength and linear
   TCA scales are resolved in f64 before packing. The existing f64 host coverage
   mask is unchanged, including outward-rounded interval blocks and exact
   per-pixel edge decisions. The shader uses these bits for blank classification.
   CPU lens-model evaluation formulas and coverage formulas are unchanged.

2. **Fast identities.** All-zero strengths select the existing copy/affine plan;
   there is no warp dispatch, coverage mask or CPU lens resample. Numerically
   identity distortion/TCA/PA coefficients are recognized too. If another tool
   requires a warp, inactive database models bypass normalization and gain.
   Geometry-cache keys already include profiles and strengths; profile changes
   rebuild sampled stages while retaining the original/capture source upload.
   Device tests assert source-buffer reuse, and zero strength returns that same
   buffer directly at native size.

3. **Tests and benchmarks.** Preserved the three existing lens tests and added
   two toolset tests with 100 additional GPU/CPU comparisons (96 border cases,
   three embedded-profile replacement strengths, one PA guard case). Border
   cases use strong barrel/pincushion poly3/poly5/ptlens, an off-centre axis,
   TCA at 200%, all eight orientations, high-contrast edges reaching every
   border, native size and prefiltered previews with manual corrections and
   crop/perspective. Every rendered comparison requires mean |delta| <0.5 LSB
   and maximum <=3 LSB. The existing capture + lens cache test now exercises
   capture followed by native GPU geometry.

   Added adapter-independent packed-model checks, zero/identity and PA guard
   checks, and coverage comparisons for every pixel in mixed lens frames under
   all eight orientations. Added a boundary test where two f64 positions round
   to the same f32 coordinate but require opposite coverage decisions, plus a
   source-upload reuse test.

   Added ignored `render::lensdb_tests::bench_lensdb_geometry_24mp`: 6000x4000,
   0/100/200%, actual previous CPU resample + upload versus new source upload +
   GPU warp and GPU warp with a reused upload. Device work is synchronized;
   minimum of three timed repeats after warm-up, no finish/readback in either
   geometry timing. `bench_toolset_24mp` retains its full-render lens row and
   adds a zero-strength row. The historical full-render result is 315 ms hybrid
   versus 796 ms CPU; native measurements are pending, not inferred.

4. **Documentation.** Updated `docs/GPU-VALIDATION.md`: lens geometry is no
   longer a CPU stage or an unimplemented candidate. Historical measurements
   remain clearly labelled; native operation, precision, coverage, fast paths,
   tests and before/after benchmark are documented.

## Sandbox verification

All Cargo commands used Rust 1.98.1, `--offline` for tests/builds, and
`CARGO_BUILD_JOBS=3`; only one Cargo build ran at a time.

| Check | Result |
| --- | --- |
| `cargo +1.98.1 test --offline -p lightcraft-pipeline` | 181 passed, 2 ignored; 0 failures; 0 doc tests |
| `cargo +1.98.1 test --offline -p lightcraft-gpu` | 61 passed, 4 ignored; 0 failures; 0 doc tests |
| GPU unit tests | 20 passed, 2 ignored |
| GPU `equivalence` / `fallback` / `memory` | 13 / 3 / 1 passed |
| GPU `toolset` | 24 passed, 2 ignored |
| `cargo +1.98.1 fmt -p lightcraft-gpu -p lightcraft-pipeline -- --check` | passed |
| Clippy, both changed crates, all targets, `-D warnings`, `CARGO_TARGET_DIR=target/clippy` | passed |
| `git diff --check` | passed |

The adapter-independent WGSL parse/validation test passed. Packed f32 models
versus f64 reference at tiny, odd and 24 MP sizes measured maximum coordinate
error **0.000667 px** and maximum relative PA gain error **0.00000761**.
These measurements use Rust f32 arithmetic; actual device sqrt/pow/division
and multiply-add contraction still require the GPU parity tests.

There is no GPU adapter in this sandbox. Device tests explicitly print
`skipped: no GPU adapter` and return successfully; their pass counts above do
not certify hardware parity. The rounding-boundary test's host assertions ran
before its device portion skipped. Logs are in `target/test-pipeline.log`,
`target/test-gpu.log`, `target/test-lensdb-details.log`, and
`target/clippy-lensdb.log` (untracked build output).

## Exact RTX 5090 tests

Run from this worktree, sequentially. Confirm the reported adapter is the NVIDIA
RTX 5090 and no device tests skip. The first command runs the complete GPU crate
regression suite; the next three isolate the lens and capture integration checks.

```sh
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_BUILD_JOBS=3
export LIGHTCRAFT_GPU_BACKEND=vulkan

cargo +1.98.1 test --offline -p lightcraft-gpu -- --nocapture
cargo +1.98.1 test --offline -p lightcraft-gpu --test toolset lens_profile -- --nocapture
cargo +1.98.1 test --offline -p lightcraft-gpu --lib render::lensdb_tests:: -- --nocapture
cargo +1.98.1 test --offline -p lightcraft-gpu --test toolset capture_sharpening_source_info_and_geometry_changes_invalidate_cache -- --exact --nocapture

cargo +1.98.1 test --release --offline -p lightcraft-gpu --lib render::lensdb_tests::bench_lensdb_geometry_24mp -- --ignored --exact --nocapture
cargo +1.98.1 test --release --offline -p lightcraft-gpu --test toolset bench_toolset_24mp -- --ignored --exact --nocapture
```

The `lens_profile` filter runs exactly:

- `lens_profiles_match`
- `lens_profiles_with_crop_rotation_and_perspective_match`
- `lens_profile_strength_changes_reach_cached_renders`
- `lens_profiles_at_borders_all_orientations_match`
- `lens_profiles_replace_embedded_and_zero_strength_matches_disabled`

The `render::lensdb_tests::` filter runs exactly these non-ignored unit tests:

- `packed_lens_models_track_f64_reference_in_f32`
- `zero_strength_and_identity_models_disable_database_branches`
- `lens_database_coverage_matches_every_f64_pixel`
- `lens_database_gpu_uses_f64_coverage_at_rounding_boundary`
- `lens_database_renders_reuse_uploaded_source`

The two ignored benchmarks above provide direct geometry before/after and full
render timings. Record their output in `docs/GPU-VALIDATION.md` after hardware
validation. Implementation is complete; hardware numerical comparisons and
timings are the only remaining validation, assigned to the coordinator by the
task's no-adapter rule. The existing source-buffer limit can still cause a host
resample regardless of lens settings; ordinary fitting sources use native GPU
geometry.
