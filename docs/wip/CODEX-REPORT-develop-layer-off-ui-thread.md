# Develop layers off the UI thread

Continues the branch's existing `6f580fca` decode-cache and `plain_layer_surface` work. No resets, new dependencies, unsafe code, GUI windows, or commits.

## Implementation

1. `develop.openPhoto` now supports `Session::start`: the worker performs bounded file reads, raw decode, full-size develop, and image-to-document conversion. Apply wraps the developed surface in the linked Develop smart object and opens the document. The editor immediately displays its existing opening tab/card, progress and Cancel button. An already-open photo is focused synchronously; the host separately schedules settings synchronization. Original raw paths are never installed as the compositing document's Save path. Duplicate pending opens reuse their job; a changed opening request supersedes the old one.
2. `develop.syncPhoto` and `layer.develop.set` use background jobs. Pending requests leave pixels, settings and history unchanged. New requests cancel intersecting photo/layer requests; returning to current settings also cancels an obsolete pending render. Full-size work is serialized, and canceled waiters exit before rendering. Apply validates document identity, geometry, format and the original target layer, preserving unrelated edits and active-document focus. Each successful update is one undo step per document. If compositing starts while a plain photo is rendering, completion adds a clean Develop layer above that work. Missing sources fail without changing the old settings or pixels. Closed or changed targets are discarded. Existing synchronous engine execution remains available for CLI/scripts/tests; the desktop uses background jobs.
3. The GPU finish kernel supports U16 RGB output in all existing output spaces, including ProPhoto. It uses the exact transfer function rather than the preview LUT and rounds to 16 bits without 8-bit dithering. The accompanying preview retains the completion sentinel, fault handling and CPU fallback. The output packs RGB into two words per pixel alongside one preview word, keeping the largest output buffer at the existing RGB-float size. Deep NR uses the CPU variance transform and the existing exact-rounding WGSL helpers at discontinuous edge-weight boundaries, with the wavelet filtering still on the device. Deep and preview NR have distinct cache keys. Spatial scratch is released earlier; a deep render that exhausts shared VRAM gets one bounded retry that allows the existing CPU haze preparation before falling back fully to CPU. `media::develop` accepts U16 requests, and `develop_with_settings` routes through it, so existing U16 exports also gain this path. Linear float output, soft proofing and other established unsupported cases still fall back to CPU.
4. `RawOptions::of(DevelopSettings)` supplies the photo's Raw Processing options to `load_bytes_with`. The single-entry decode cache key now includes those options. Host settings already serialize the complete `DevelopSettings`, so no new persisted fields or schema migration are needed.

## Established references followed

- Existing `pc-engine/src/jobs.rs` and `pc-ui-egui/src/jobs_ui.rs`: worker/apply split, cancellation, progress, opening tabs and normal open/color handling.
- Existing `pc-engine/src/smart_cmds.rs`: source-image cache, `plain_layer_surface`, placement and smart-filter processing; these are reused rather than reimplemented.
- Existing `lc-engine/src/files.rs`: `RawOptions::of`, `RawOptions::key`, and `load_bytes_with`, matching the Library decoder.
- Existing `lc-pipeline/src/finish.rs` (`finish_deep`, exact transfer curve and U16 quantization) and `output.rs`: the numerical CPU oracle.
- [GPU pipeline documentation](../upstream/lightcraft/gpu-pipeline.md): completion sentinel, CPU fallback, bounds and real-adapter validation. New deep comparisons enforce mean error below 0.5 U16 LSB and maximum error at most 3 U16 LSB.
- Existing `lc-pipeline/src/detail/nr.rs` and `lc-gpu/src/wgsl/toneeq_round.wgsl`: the darktable-derived variance transform and already-tested scalar rounding helpers. Reusing these fixed rare full-size NR deviations instead of loosening tolerance.
- [wgpu buffer documentation](https://docs.rs/wgpu/30.0.1/wgpu/struct.Buffer.html), read from the installed offline registry source (`wgpu-30.0.1/src/api/buffer.rs`): storage buffers and synchronized readback. Existing buffer/readback helpers handle transfers.

The request superseding keys and layer-level apply validation are local integration decisions: the existing generic job infrastructure locks whole documents, whereas Develop layers must retain their old pixels while compositing can continue.

## Validation

All commands used Rust 1.98.1, offline builds and `CARGO_BUILD_JOBS=3`, with one cargo build at a time. Named changed crates were formatted. Clippy passed with `CARGO_TARGET_DIR=target/clippy`, all six packages, `--all-targets -- -D warnings`.

The final full suites ran once, only for the six changed crates, with `LC_REQUIRE_GPU=1` and `--nocapture --test-threads=3`:

| Crate | Passed | Ignored |
|---|---:|---:|
| lightcraft-engine | 680 | 4 |
| lightcraft-gpu | 87 | 5 |
| local-image | 81 | 2 |
| photocraft-engine | 1,444 | 15 |
| photocraft-io | 422 | 2 |
| photocraft-ui-egui | 1,032 | 7 |
| Total | 3,746 | 35 |

No failures or missing-GPU skips. The Linux AppKit integration target exited successfully without opening a window. Doc tests also passed (no runnable examples in these crates). Build output warned that optional craft-fonts are absent; this did not fail validation.

Added or extended checks:

- Eight engine DNG job tests: pixels/metadata/Develop link match inline import; duplicate/already-open requests; superseding/reverting settings; `layer.develop.set` superseding sync; compositing during rendering; changed/closed targets; document reorder/focus and undo; unavailable source.
- One kittest UI test: frames run with a gated worker, opening view appears, a real Cancel click discards it, another request completes, and duplicate requests reuse the opening job (including automation wait bookkeeping). Caller-thread work counter stays unchanged.
- One host shortcut round-trip test: Library → Compositing → Develop → Compositing, old pixels while pending, one undo per completion, existing retouch layers preserved under Develop 2.
- One raw-options/cache regression, plus the existing decode-reuse regression.
- GPU `deep_output_matches_cpu`: all five output spaces × plain/typical edits, starting from cached U8 previews. RTX 5090 Vulkan; maximum 1–3 U16 LSB, mean 0.0068–0.0225 LSB. ProPhoto maximum 1 LSB for both cases.
- Engine `deep_raw_output_uses_the_gpu_with_raw_processing_options`: generated 640 × 480 DNG with RCD/Clip options, actual `media::develop` GPU routing; maximum 1 / mean 0.0234 U16 LSB.
- Deep-output lost-work fault check: cleared/incomplete output is rejected, GPU reset restores U16 rendering.
- Opt-in full-size RAW CPU/GPU comparison and opt-in real `Host::switch` timing test. These use the final test binaries directly through the existing memory-watchdog runner, avoiding another build or full-suite rerun.

Full logs are in `target/develop-validation/` (ignored build artifacts). The full suite command was:

```sh
PATH=$HOME/.cargo/bin:$PATH CARGO_BUILD_JOBS=3 LC_REQUIRE_GPU=1 cargo +1.98.1 test --offline \
  -p photocraft-engine -p photocraft-io -p photocraft-ui-egui \
  -p lightcraft-gpu -p lightcraft-engine -p local-image -- --nocapture --test-threads=3
```

## Performance

The final full-size corpus check passed on `_DSC4606.ARW` (3744 × 5616), RTX 5090 Vulkan: plain maximum 1 / mean 0.0073 U16 LSB; typical maximum 1 / mean 0.0184 U16 LSB. The test asserts actual GPU routing through `media::develop`. Typical used only the CPU NR variance transform in this run. Earlier runs under greater shared VRAM pressure exercised the bounded retry and CPU haze preparation, still with maximum 1 LSB.

The actual host path was measured without a window, using the final development-profile app test binary (not a substitute renderer):

| Host::switch case | UI dispatch | Complete |
|---|---:|---:|
| Library → Compositing, plain | 0.672 ms | 2.524 s |
| Develop → Compositing, plain | 0.335 ms | 0.781 s |
| Library → Compositing, typical | 0.156 ms | 1.557 s |
| Develop → Compositing, typical | 0.231 ms | 1.613 s |

These are single measurements on the shared machine. Plain ran first, including cold decode/device startup; typical reused the single-entry source-decode cache. The caller-thread work counter stayed unchanged, and each re-develop produced exactly one undo step. The benchmark imported into its own temporary catalog, disabled XMP writes and reset develop settings before applying the test controls. The original RAW was read-only. Both opt-in checks passed (one additional test each).

The existing memory-watchdog runner emitted its known empty-RSS arithmetic warning after the short-lived host test had already passed; it returned exit status 0. No owner runner files were changed.

Release `bench_develop_open` was rerun with plain and `--typical` settings, one repetition each, in separate processes. A matched CPU control used the same final binary with `LIGHTCRAFT_GPU=0 --inline`:

| Case | Task's before: UI blocked | Current CPU inline control | After: UI dispatch | After: complete |
|---|---:|---:|---:|---:|
| Open, plain | 1.8–1.9 s | 1.755 s | 0.173 ms | 3.618 s |
| Sync, plain | 0.8–0.9 s | 0.882 s | 0.079 ms | 0.521 s |
| Open, typical | 9.2–10.2 s | 11.362 s | 0.192 ms | 5.640 s |
| Sync, typical | ≈9 s | 9.556 s | 0.062 ms | 1.214 s |

The important interactive result is that dispatch returns immediately in every case. Typical re-develop completion was about 7.9× faster than the matched CPU control; cold typical opening was about 2× faster. A cold plain GPU opening was slower overall than the CPU control: it includes decode and first device/shader setup, but this work now happens on the worker. Sync reuses decode and the initialized device. Cold startup and shared-machine contention are included in the totals; these single measurements are not steady-state averages. The source-file cache and developed-surface cache from the previous builder remain in place.

A separate `LIGHTCRAFT_PROFILE=1 --typical` diagnostic confirmed the GPU stage trace through finish/readback. Its instrumented timings are excluded from the comparison table.

Reproduction (add `--typical` for the edited case; add `LIGHTCRAFT_GPU=0` before cargo and `--inline` after the arguments for the CPU control):

```sh
PATH=$HOME/.cargo/bin:$PATH CARGO_BUILD_JOBS=3 cargo +1.98.1 run --release --offline \
  -p photocraft-engine --example bench_develop_open -- <photo.ARW> --reps 1
```

## Completion and limits

All four requested points are implemented and verified. Changes are uncommitted. No new dependencies, unsafe code, GUI windows or changes to owner data were introduced. The optional corpus/host checks supplement the 3,746 passing default tests with two passing tests.

The GPU path remains the project's per-stage hybrid renderer: U16 NR deliberately uses CPU variance transforms for precision, and very large spatial buffers or shared VRAM pressure may use CPU preparation or the established complete CPU fallback. F32 linear output, soft proofs, LUT profiles and refined masks retain their existing CPU routes. The test enforces actual GPU routing for this corpus, rather than accepting a skipped adapter or a complete CPU fallback. Cancellation discards the old result immediately; work already inside a pipeline render exits at the next command-level cancellation check, while newer waiting requests remain cancellable before rendering.

