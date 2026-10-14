# TIFF/DNG adaptive parallel batches — 2026-10-14

Implemented in `crates/lc-raw/src/tiffraw.rs`, continuing the existing Sony/DJI scoped-budget implementation. Changes are uncommitted. The pre-existing edit to `docs/wip/CODEX-TASK.md` was preserved.

The decoder now subtracts the final output allocation from the caller's `memory_budget()` and plans contiguous parallel batches using the remaining allowance. Small images with enough allowance use one fully parallel batch, including Deflate images. Large images use successive parallel batches; only an allowance that fits one chunk forces sequential decoding. Each batch is consumed and freed before the next is dispatched. The allowance is captured on the calling thread before Rayon workers run, so its thread-local scope, restoration and unwind behavior remain intact. The app's existing `HeavyJobs` headroom wiring in `lc-engine::files` and merge paths needs no changes.

Per-chunk estimates include decoded samples, inflated Vec growth/reallocation, float predictor row buffers and their copies, packed-word row scratch, lossless JPEG frame capacity/Huffman storage, and lossy JPEG padded coefficient/row/metadata buffers. Declared JPEG byte counts are clipped to actual available input for planning, matching existing truncated-file decoding. Oversized chunks fail before output allocation instead of silently exceeding the allowance. Output allocation remains fallible. PR #13's independent 2 GiB output and 1 GiB per-chunk working limits are retained as defensive ceilings; neither selects a batch size. These ceilings tighten exceptionally large high-allowance outputs/monolithic strips compared with the Sony-only branch.

Interleaved output rows now use checked slice copies; planar samples retain the existing strided copy. This avoids repeated bounds checks per sample and preserves clipped edge tiles, padded short frames, channel order and sample bits. The decoder's existing partial-success/first-error policy remains intact. No codec, predictor, colour calculation or sample precision was changed.

**Verification.** Only `lightcraft-raw` was changed. The final source passed:

- `PATH=~/.cargo/bin:$PATH cargo +1.98.1 fmt -p lightcraft-raw -- --check`
- `PATH=~/.cargo/bin:$PATH cargo +1.98.1 check --offline -p lightcraft-raw`
- `PATH=~/.cargo/bin:$PATH cargo +1.98.1 clippy --offline -p lightcraft-raw --all-targets -- -D warnings`
- `PATH=~/.cargo/bin:$PATH cargo +1.98.1 test --offline -p lightcraft-raw --lib batch_tests`: **8 passed, 0 failed, 227 filtered out**.

The three PR tests were ported: 100 MP RGB float-output/batch planning without a huge test allocation; excessive/overflowing allocation rejection; and multi-batch integer/float, predictor, chunky/planar and edge-pixel preservation. Five additional tests cover variable strip packing and full-parallel planning, minimum/zero/overflow-safe allowances, float/packed-row scratch, nested scopes and partial failures, larger lossless JPEG frame capacity, and lossy JPEG budget boundaries/truncated byte counts. The existing Sony/DJI streaming/budget/restoration/unwinding test and all other existing tests remain present and unchanged. They were not run, following the task's overriding minimum-testing rule. No full crate/workspace/GPU acceptance suite was run. No GPU tests were added because this change is CPU TIFF decoding and allocation only.

**Measurements.** Before is this branch's original scoped streaming implementation, saved as `target/tiff-batches/baseline-tiffraw.rs` before edits, plus its statically linked `before` executable. After is the final source and `after` executable. Both use the same Cargo development profile (`opt-level=1`, dependencies `opt-level=2`), Rust 1.98.1 and existing target directory. These are local development-build measurements, not release-build or GUI latency claims.

The harness calls the same public `decode_with_memory_budget` entry point as the app. It times the entire RAW decode (container parsing, native decoding, calibration/metadata), after reading the file, and fingerprints every decoded U16 sample or every F32 bit pattern after timing. GNU time records peak process RSS, including file input and the fingerprint pass. Before and after alternate for three independent processes per input/allowance; every value below is the median of those three. The OS page cache was already warm. The real compute profile reported 16 logical / 8 physical cores, 60.2 GiB RAM, 70% automatic memory share and **7 automatic compute workers**. The harness sets `RAYON_NUM_THREADS` from that detected profile for both executables; no fixed thread count is used in app code. Cargo builds remained sequential with the launcher's `CARGO_BUILD_JOBS=4` unchanged.

All four owner panoramas in the owner's local test-shot folder (not in the repository) were measured. They are uncompressed 16-bit RGB, one-row strips, rather than floating-point tiled HDRs. Their files are approximately 343–403 MB. This explains why batching alone gave little improvement; the interleaved row-copy change accounts for most of their final speed gain. The float result combines parallel Deflate decoding with row copying.

The following constrained caller allowances deliberately exercise multiple batches: **512 MiB** for each panorama and **1536 MiB** for the 100 MP float sample. These are measurement scenarios, not decoder constants.

| Input | Before decode (ms) | After decode (ms) | Time reduction | Before peak RSS (MiB) | After peak RSS (MiB) |
|---|---:|---:|---:|---:|---:|
| Owner panorama 1 | 274.9 | 111.3 | 59.5% | 659.8 | 844.8 |
| Owner panorama 2 | 308.4 | 130.7 | 57.6% | 765.5 | 897.6 |
| Owner panorama 3 | 315.7 | 130.8 | 58.6% | 770.2 | 900.2 |
| Owner panorama 4 | 313.8 | 125.6 | 60.0% | 774.3 | 902.1 |
| Synthetic 100 MP Pixel Shift **encoding** | 2531.6 | 809.1 | 68.0% | 1202.4 | 1288.8 |

With the app's actual live `HeavyJobs::headroom()` after reading each input, allowances varied with other jobs from roughly 35.5–38.8 GiB and all chunks fit one parallel batch:

| Input | Before decode (ms) | After decode (ms) | Time reduction | Before peak RSS (MiB) | After peak RSS (MiB) |
|---|---:|---:|---:|---:|---:|
| Owner panorama 1 | 273.0 | 118.6 | 56.6% | 987.0 | 987.0 |
| Owner panorama 2 | 310.5 | 129.4 | 58.3% | 1145.6 | 1145.7 |
| Owner panorama 3 | 319.3 | 138.4 | 56.6% | 1153.2 | 1153.2 |
| Owner panorama 4 | 322.2 | 143.5 | 55.5% | 1158.6 | 1158.9 |
| Synthetic 100 MP Pixel Shift **encoding** | 2582.8 | 770.3 | 70.2% | 1202.3 | 2424.9 |

The scoped allowance bounds planned live pixel/codec storage, not total process RSS. Compressed/file input, TIFF/chunk/result metadata, allocator bookkeeping and freed-page retention are outside it. Parallel batches intentionally use more temporary memory than the former sequential path. At the constrained allowance, the float sample's final median peak was about 1.26 GiB versus 2.37 GiB with ample headroom, while retaining most of the speed improvement. The implementation has no fixed 128 MiB batch constant or host-specific concurrency tuning.

**Quality evidence.** Every before/after full-sample fingerprint matched for every run under both allowances. Fingerprints are kept only in the local target directory. The small predictor/planar/edge tests also compare complete decoded arrays directly against expected pixels. The float generator includes finite negative and fractional samples. Evidence shows no change in sample content; no rendered-look or GPU quality claim is made.

**Remaining input limitation.** An original owner Pixel Shift DNG was not found in the permitted test-shot, reference, working-data, Pictures or Downloads locations. A path was requested during the work and none was supplied. Nothing under `~/Desktop` was inspected. The substitute is explicitly synthetic: 10,000 × 10,000 RGB float32, 256-pixel Deflate tiles, predictor 3, 300 million samples, written by the existing `lightcraft_raw::write_dng` with the same encoding used by `lc-merge::output::write_linear_dng(DngSamples::Float)`. It is a 30,189,540-byte generated file, not an owner's Pixel Shift capture or a measured fusion result. The exact requested original-Pixel-Shift measurement remains outstanding until a permitted file is available; its encoding and large-output path have been exercised here.

**References followed.** The integration starts from PR #13 (`origin/codex/high-resolution-photo-input`, commit `3645327071935579752751fd5f1f03a03a97b9b3`) and the current Sony/DJI implementation, rather than a new decoder design. The scoped memory policy comes from `lc-sysmem/src/permits.rs` and `profile.rs`, and the actual app call from `lc-engine/src/files.rs`. Indexed parallel collection follows the [official Rayon documentation](https://docs.rs/rayon/latest/rayon/iter/trait.IndexedParallelIterator.html) and its [parallel iterator documentation](https://docs.rs/rayon/latest/rayon/iter/trait.ParallelIterator.html). Predictor/storage behavior remains the established implementation in `unpack.rs`, `dngwrite.rs` and `lc-merge/src/output.rs`, whose format references are TIFF 6.0, Adobe TIFF Technical Note 3 and the [Adobe DNG specification/SDK](https://www.adobe.com/support/downloads/dng/dng_sdk.html). No format algorithm was independently reinvented. Working-storage estimates were derived by inspecting the actual allocation sites in local `ljpeg.rs`, locked miniz_oxide 0.8.9 and zune-jpeg sources; summing those conservative costs into batches is the scheduling calculation added here. A read-only subagent reviewed codec allocations and the final copying/budget behavior; it made no edits or cargo calls.

No dependency or Cargo.lock changes, new C/-sys code, unsafe code, settings/catalog/document schema changes, UI strings or UI changes were introduced. Match Look files were not touched. No GUI was opened, app was killed, data was uploaded or commit was created. All generated data, measurement executables, raw logs and intermediate results remain under `target/tiff-batches/`.

Reproduction artifacts: `measure.rs`, `profile.rs`, `generate.rs`, `run_measurements.py`, `measurements.json`, `sources.json`, `profile.log`, `final-*.log`, `check.log`, `clippy.log` and `tests-final.log` in that target directory. Running `python3 target/tiff-batches/run_measurements.py` reuses the preserved baseline executable and rebuilds the after harness against the final built library. Earlier measurements before the row-copy optimization are retained in `pre-row-copy/`. No owner photo contents are embedded in this report.
