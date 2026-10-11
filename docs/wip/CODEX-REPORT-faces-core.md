# Smart Sort faces core — Phase 3a report

Completed on 2026-10-09. Changes remain uncommitted in the working tree.

## 1. Inference, geometry and clustering

- Added `crates/li-seg/src/faces.rs`, with private `faces/geometry.rs` and `faces/cluster.rs` submodules. The only modified tracked file is `crates/li-seg/src/lib.rs`, adding `pub mod faces;`. The supplied task and build spec were preserved; no existing faces implementation was present at the start of this checkout.
- Exported `FaceModelFile`, `YUNET` and `SFACE` with the exact pinned filenames, URLs, byte counts, hashes and licences from §4.1. No changes to `MODELS`, `Group`, attributions, Cargo manifests or `Cargo.lock`.
- `FaceModels::load(dir)` accepts the normal `<dir>/segmentation/` layout or a directory containing the two files directly. `load_files` also accepts explicit paths. The model manager/downloader remains responsible for file size/hash verification; these loaders also work with synthetic ONNX graphs.
- `FaceModels::faces(FaceImage)` returns `anyhow::Result<Vec<Face>>`, including original-image pixel rectangles, five landmarks, score and a unit `[f32; 128]` embedding. `detect` and `embed` are independently callable. The models are `Send + Sync`, with concurrent run tests, a per-instance detector-plan cache bounded to four padded shapes, and no global inference state, persistence, network access or face logging.
- Borrowed interleaved RGB8, RGB16 and normalised RGB f32 inputs are supported by `FaceImage`/`RgbPixels`. Finite float values are clamped to 0..1. Letterboxing samples the source directly, sets the long edge to 1024, pads only right/bottom to multiples of 32, and produces BGR NCHW 0..255. It avoids a full-resolution intermediate copy.
- YuNet loads with ignored declared output/intermediate shapes. Its twelve outputs are selected by name. Decoding implements strides 8/16/32, clamped square-root cls/object scores, exponential bbox sizes and five landmark offsets. Detection applies score ≥ 0.9, the 40-pixel short-side filter on detector coordinates, IoU 0.3 NMS and top-k 5000, then maps rectangles/landmarks back to the original frame. Returned rectangles are clipped to the image bounds.
- Alignment uses the closed-form 2D proper-rotation Umeyama least-squares solution and the exact ArcFace reference landmarks, followed by a bilinear inverse warp with black borders. The warp returns RGB f32 0..255 to preserve RGB16/float precision. SFace receives those RGB values without duplicating the normalisation already in its ONNX graph; the 1×128 output is L2-normalised. Invalid shapes, zero/nonfinite embeddings, degenerate landmarks and malformed image buffers return errors.
- `chinese_whispers` sorts nodes by `(photo_date, key, face_index)`, starts labels at sorted node indices, constructs cosine ≥ 0.50 edges, updates labels sequentially in place, sums neighbour weights, chooses the smallest label on exact ties, and stops at stability or 30 passes. Suggestions exclude singletons and sort by descending size, with smallest member key breaking size ties. Centroids and members are deterministic under input permutations.
- `assign_person` and `assign_cluster` apply cosine ≥ 0.45 and a runner-up margin ≥ 0.05. Rejected pairs veto the best assignment without forcing a second choice or erasing ambiguity. A rejection for any cluster member vetoes assigning the whole cluster to that person. Person ids, rather than display names, identify rejections.
- Source commits are documented in the module. These are independent Rust implementations of the equations, with no copied upstream source. `docs/PORTS.md` was left untouched to respect the new-files-only rule.

## 2. Tests and checks

All builds used `PATH="$HOME/.cargo/bin:$PATH"`, toolchain `+1.98.1`, offline Cargo, and `CARGO_BUILD_JOBS=3`; only one Cargo build ran at a time in this job.

| Check | Result |
|---|---|
| `cargo +1.98.1 test --offline -p li-seg` | **35 passed, 0 failed, 2 ignored**; 0 doc tests. Final run after lint fixes passed. |
| New faces tests in that suite | **21 passed, 2 ignored** (23 total). Existing 14 tests also passed. |
| `CARGO_TARGET_DIR=target/clippy cargo +1.98.1 clippy --offline -p li-seg --all-targets -- -D warnings` | Passed, no warnings. |
| `cargo +1.98.1 fmt -p li-seg -- --check` | Passed. |
| `git diff --check` | Passed. |
| Manifest/lockfile diff and tracked-file scope | No dependency/lockfile changes; only the permitted module declaration in `lib.rs`. |

Coverage includes all Phase 3 pure acceptance items, decode at every stride, score clamping/inclusive thresholds, malformed outputs, NMS overlap/disjoint/tie/top-k cases, letterbox coordinates/orientations/padding, input validation and all three pixel formats, proper least-squares alignment including noisy/reflected points, bilinear warp/borders, normalisation, clustering ordering/ties/thresholds/singletons, assignment margin/rejection, and model errors.

Tiny in-test ONNX graphs implement dynamically shaped YuNet-like twelve-output tensors and an SFace-like 1×128 output including internal normalisation. YuNet graph outputs are intentionally shuffled and declare stale 640×640 shapes. Full `faces()` tests cover square/portrait/landscape inputs, original coordinates, repeatability, plan reuse, concurrent calls, thresholds and embedding contents. No test downloads files or requires real weights by default.

The ignored `faces::tests::real_weights_migrant_mother_twice_and_no_tetons_faces` reads `LI_SEG_TEST_MODELS`. It asserts two main faces near the spec's centres, both with scores ≥ 0.9, same-woman embedding cosine ≥ 0.6, and no faces in Tetons. The repository's 3200×2000 Migrant Mother fixture already contains two side-by-side copies, so the test does not duplicate the entire screenshot again.

Coordinator command (with weights already present):

```sh
PATH="$HOME/.cargo/bin:$PATH" CARGO_BUILD_JOBS=3 LI_SEG_TEST_MODELS=/path/to/models \
  cargo +1.98.1 test --offline -p li-seg \
  real_weights_migrant_mother_twice_and_no_tetons_faces -- --ignored
```

GPU tests added: **none**. This task implements CPU tract inference and pure functions only.

## 3. Ignored 24 MP benchmark

Added and ran `faces::tests::bench_faces_24mp_mock_models` on a 6000×4000 RGB image with local mock models: **1 passed**, one returned face. Separate measurements from the run:

| Stage | Time |
|---|---:|
| Letterbox | 33.60 ms |
| Mock detector, including new aspect-ratio plan | 9.62 ms |
| Output decoding + NMS | 0.387 ms |
| Similarity fit + bilinear alignment | 0.721 ms |
| Warm complete `faces()` call | 39.45 ms |

These timings exercise preprocessing and integration; they do not predict real-weight inference latency.

```sh
PATH="$HOME/.cargo/bin:$PATH" CARGO_BUILD_JOBS=3 \
  cargo +1.98.1 test --offline -p li-seg bench_faces_24mp_mock_models -- --ignored --nocapture
```

## Remaining work

No Phase 3a implementation items remain. Real-weight validation is intentionally left to the coordinator as required by the task. Model-manager registration, engine orchestration/storage/people commands, UI opt-in, and attribution updates belong to Phase 3b and were not touched. The CPU API neither enables recognition nor writes face data; the engine must enforce the per-library opt-in and normalise pixel rectangles for persisted MWG regions.
