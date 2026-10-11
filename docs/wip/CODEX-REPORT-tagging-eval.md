# Smart Sort tagging evaluation — 2026-10-10

## Outcome

**Ship only CLIP (`clip-b32-laion`).** On the frozen 330-photo open sample it ranks
285/330 correctly (86.4%), versus evaluated SigLIP's 283/330 (85.8%). That small
difference is not evidence of universal superiority. CLIP is also faster, smaller
and materially more useful with the product's existing sensitivity thresholds.

**Current shipping status:** SigLIP was evaluated and is **not shipped**. Its
experimental implementation, tokenizer, model choice, fixtures and notices have
been removed at the owner's request. CLIP is the sole tagging model. The report
and both result JSON files are retained as the historical evaluation record.

The implementation, validation and timings below describe the evaluation branch
before removal, rather than features currently available in the app. The current
CLIP-only evaluator can reproduce the retained model's run; archived SigLIP results
require the historical evaluation revision `604657f0dc736af0a37443d50c1b936ec2d40a39` to reproduce.

Continued the previous builder's engine/UI rather than replacing them. No commit made.
No new dependencies, unsafe code, C/C++ code or linked -sys dependencies were introduced.
Cargo.lock is unchanged. Models are in the authorised isolated test cache, photos and
logs under target/. No desktop GUI or ComfyUI jobs were opened and no owner's library
or model folders were modified.

## 1. Real faces

Only the nine files pinned in smart-sort.md §4.1/§4.2 were downloaded to
`~/.cache/local-image-test-models/segmentation/`. Every byte size and SHA-256 matched:
four CLIP files, three SigLIP files and YuNet + SFace. Details:
`target/tagging-eval/model-verification.log`.

Both required ignored real-weight tests passed:

- li-seg `faces::tests::real_weights_migrant_mother_twice_and_no_tetons_faces`: **1 passed**, 0.75 s.
- engine `smart_sort::people::tests::real_weight_people_end_to_end`: **1 passed**, 1.56 s.

YuNet detects the repeated faces at score ≥0.9; SFace same-person cosine is ≥0.6;
Tetons has no faces. The engine test exercises detection, cached resume, clustering,
name assignment and find-person commands with the actual weights. These models run
on the CPU by design. Logs: `faces-real.log`, `people-real.log` under target/tagging-eval.
No face defect required a product-code fix.

## 2. Historical SigLIP evaluation prototype (removed)

Added the exact pinned FP32 encoders and 768-dimensional family specification.
The first encoder outputs are hidden states, so the loader explicitly selects
`pooler_output` for **both** encoders before optimization. The initial real-weight
probe caught this; the corrected probe passes (**1 passed**, 1.90 s).

The Rust Unigram tokenizer applies canonicalization, the pinned SentencePiece Unicode
map, maximum-score Viterbi segmentation, fused unknown id 2 and 64-token EOS id 1
padding/truncation. Image preprocessing uses Pillow-compatible separable bicubic
antialiasing with 22-bit coefficients and byte rounding between passes: stretch RGB
to 224 square, ignore alpha, NCHW, mean/std 0.5. Encoders' pooled vectors are L2-normalized.
Scoring is independent sigmoid, preserving Any/All tag aggregation and exemplar
blending. Ranking uses the underlying cosine to avoid ties when f32 probabilities
saturate at one; CLIP retains its softmax/background behavior.

Primary references followed by the historical prototype (its shipping PORTS rows are now removed):

- [Transformers SiglipTokenizer v4.45.2](https://github.com/huggingface/transformers/blob/v4.45.2/src/transformers/models/siglip/tokenization_siglip.py).
- [Transformers SiglipImageProcessor v4.45.2](https://github.com/huggingface/transformers/blob/v4.45.2/src/transformers/models/siglip/image_processing_siglip.py).
- [transformers.js Unigram v2.17.2](https://github.com/huggingface/transformers.js/blob/2.17.2/src/tokenizers.js).
- [spm_precompiled charsmap](https://github.com/huggingface/spm_precompiled/blob/c00612e48dda9b8b31a5d2d09530f5caa666d5c0/src/lib.rs).
- [Pillow Resample.c](https://github.com/python-pillow/Pillow/blob/bb1d8e8ab8d29048624d96e3ee53cecf7c13d13d/src/libImaging/Resample.c).

Golden fixtures were generated offline with installed transformers 5.18.0, tokenizers
0.23.2 and Pillow 12.3.0; no reference packages were downloaded. They covered English,
Unicode compatibility/composition, unknown fusion, long-prompt truncation, up/downscale
and one-axis image resizing (pixel agreement 1e-6). These fixtures, their provenance
file, the prototype's licence notice and its shipping attributions are now removed.

The evaluated Xenova `siglip-base-patch16-224` bundle was pinned to revision
`4649052661e53c7000355844105f8a1792088239` on Hugging Face:

| File | Bytes | SHA-256 |
| --- | ---: | --- |
| `onnx/vision_model.onnx` | 371819850 | `f89d41bac7f4d4b87e010a467d93f98689d708916ed22f5a07f96fdfa26f475f` |
| `onnx/text_model.onnx` | 441332132 | `3aa7fdbd20eaa8740cce17bf82913de641fcb632a768fed59f661cdcd0c32553` |
| `tokenizer.json` | 2398744 | `4a17c975210be5ab4c36b47d8dae4eefb866dbfb1e676e394aad85dc30a3ae08` |

These are archival identifiers, not product download options.

**Scalar provenance limitation:** the split encoders omit scale/bias and the authorised
pins omit the full checkpoint. Learned constants `117.3308` and `-12.9324` therefore
follow the [upstream conversion harness's published values](https://github.com/john-rocky/CoreML-Models/blob/1d103ecd243831f41dd099da78a5ddef3e568443/conversion_scripts/convert_siglip.py),
which are rounded to four decimal places. I did not download an unpinned full model
to re-extract full-precision initializers. Ranking/default selection is independent
of that rounding; probabilities use those explicitly documented constants.

## 3. Evaluation

The [manifest](../../crates/lc-engine/tests/references/tagging-eval/manifest.json)
contains **330** agent-reviewed Wikimedia photographs. Sources, creator/terms links,
acquisition URLs, exact sizes and hashes are recorded per photo; all 330 local files
passed fetcher hash/size verification. Licences are CC0, public domain, CC BY and CC
BY-SA. Images are uncommitted under target/. [SOURCES](../../crates/lc-engine/tests/references/tagging-eval/SOURCES.md)
and `scripts/fetch_tagging_eval.sh` make the sample reproducible.

Labels were reviewed before either model ran, with artwork, slides, obvious duplicate
shots and incorrect seed labels removed/corrected. Every photo has one primary label.
Conference, Wedding and Sports use the **unmodified built-in prompts**. General is an
evaluation-only custom preset (People/Landscape/Food/Architecture/Animals/Events),
because the product's fourth built-in is an empty Custom preset. No exemplars or
prompt tuning were used. This is an exploratory, uneven Commons sample, not an
independently human-annotated benchmark; overlapping categories and related scenes
limit generalization. Wedding Getting ready has only one example.

### Ranking accuracy

| Preset | n | CLIP top-1 | SigLIP top-1 | CLIP top-3 | SigLIP top-3 |
| --- | ---: | ---: | ---: | ---: | ---: |
| Conference | 53 | 43/53 (81.1%) | 39/53 (73.6%) | 96.2% | 96.2% |
| Wedding | 32 | 25/32 (78.1%) | 27/32 (84.4%) | 96.9% | 96.9% |
| Sports | 60 | 46/60 (76.7%) | 45/60 (75.0%) | 95.0% | 96.7% |
| General | 185 | 171/185 (92.4%) | 172/185 (93.0%) | 100.0% | 100.0% |
| **All** | **330** | **86.4%** | **85.8%** | **98.2%** | **98.5%** |

Built-in event presets alone: CLIP 114/145 top-1 (78.6%), SigLIP 111/145 (76.6%).
General is reported separately so its larger count does not hide this comparison.
Top-k ranks foreground categories before the Unsorted decision.

### Assigned-category precision/recall

These use the actual classifier's **Balanced, single-folder** decisions, including
abstentions, with no threshold tuning. P/R are percentages; zero is used for an
undefined precision denominator. JSON results retain TP/FP/FN and every prediction.

| Preset / category | Gold n | CLIP P | CLIP R | SigLIP P | SigLIP R |
| --- | ---: | ---: | ---: | ---: | ---: |
| Conference / Audience reactions | 5 | 50.0 | 60.0 | 100.0 | 40.0 |
| Conference / Candids & networking | 3 | 66.7 | 66.7 | 0.0 | 0.0 |
| Conference / Group photos | 12 | 100.0 | 66.7 | 100.0 | 8.3 |
| Conference / Speakers | 16 | 83.3 | 93.8 | 100.0 | 37.5 |
| Conference / Sponsors & exhibitors | 8 | 85.7 | 75.0 | 0.0 | 0.0 |
| Conference / Venue & details | 9 | 100.0 | 55.6 | 0.0 | 0.0 |
| Wedding / Ceremony | 5 | 62.5 | 100.0 | 0.0 | 0.0 |
| Wedding / Couple portraits | 5 | 100.0 | 100.0 | 0.0 | 0.0 |
| Wedding / Details | 4 | 100.0 | 100.0 | 0.0 | 0.0 |
| Wedding / Family & group formals | 7 | 100.0 | 71.4 | 100.0 | 42.9 |
| Wedding / Getting ready | 1 | 50.0 | 100.0 | 0.0 | 0.0 |
| Wedding / Reception & dancing | 5 | 100.0 | 40.0 | 0.0 | 0.0 |
| Wedding / Speeches & toasts | 5 | 100.0 | 60.0 | 100.0 | 40.0 |
| Sports / Action | 5 | 50.0 | 60.0 | 0.0 | 0.0 |
| Sports / Celebrations | 10 | 77.8 | 70.0 | 0.0 | 0.0 |
| Sports / Fans & crowd | 5 | 100.0 | 100.0 | 100.0 | 40.0 |
| Sports / Team & portraits | 23 | 93.3 | 60.9 | 100.0 | 4.3 |
| Sports / Venue | 17 | 84.2 | 94.1 | 87.5 | 41.2 |
| General / Animals | 33 | 93.8 | 90.9 | 0.0 | 0.0 |
| General / Architecture | 23 | 100.0 | 78.3 | 0.0 | 0.0 |
| General / Events | 20 | 81.0 | 85.0 | 100.0 | 5.0 |
| General / Food | 49 | 98.0 | 100.0 | 0.0 | 0.0 |
| General / Landscape | 17 | 100.0 | 70.6 | 0.0 | 0.0 |
| General / People | 43 | 97.0 | 74.4 | 0.0 | 0.0 |

Micro assignment P/R: CLIP **90.2% / 80.9%** (267 correct, 29 incorrect, 34 Unsorted);
SigLIP **96.2% / 7.6%** (25 correct, 1 incorrect, 304 Unsorted). Independent SigLIP
pair probabilities are not calibrated confidence over these preset categories;
reusing the CLIP sensitivity cutoffs leads to substantial abstention. Family-specific
threshold calibration would be separate product work with an independent labelled
validation set. No threshold was secretly tuned on this evaluation sample.

### CPU app-path timing, size and RAM

AMD Ryzen 7 9800X3D, 16 logical CPUs, normal eight-worker analysis pool; tract 0.23.8.
Separate fresh processes, `LIGHTCRAFT_GPU_BACKEND=off`; actual adapter output is None.
All model inference is CPU. These are **CPU measurements**, not GPU performance claims.
The shared machine may add noise. Built with the repository's dev opt-level 1,
dependencies opt-level 2; not an isolated release benchmark.

`examples/tagging_eval.rs` installs the app's filesystem decoder and calls the real
`smart_sort::analyze` decode/develop/render/inference/cache path. Photo import metadata
setup precedes the timer. CLIP uses the app's existing **three crops**, SigLIP one
224-square input. Per-photo speed below is batch throughput, not single-photo latency.
Warm interaction is actual Classifier reclassification with cached image/text vectors,
not a GUI-frame or desktop-window measurement; no desktop windows were opened.

| Measurement | CLIP | SigLIP |
| --- | ---: | ---: |
| Download bytes | 607,194,067 (607.2 MB) | 815,550,726 (815.6 MB) |
| Peak process RSS | 1,658,760 KiB (1.58 GiB) | 1,970,040 KiB (1.88 GiB) |
| Model load | 705.2 ms | 981.1 ms |
| Analysis, 330 photos | 16.12 s | 17.25 s |
| Analysis per photo | 48.84 ms | 52.28 ms |
| First classification, all presets | 1569.5 ms | 2455.2 ms |
| Warm reclassification, 330 photos | 2.63 ms | 3.76 ms |
| Cache-only resume | 0.242 ms | 0.249 ms |

Both resumed with **zero images re-analysed**. Analysis is about 7% slower with SigLIP
here, rather than the spec's early single-crop estimate of 3×. Peak RSS includes
model loading, decoder/render buffers and embedding cache. CPU logs and `/usr/bin/time -v`
outputs are under target/tagging-eval; preserved machine metadata, metrics and predictions:
[CLIP results](../../crates/lc-engine/tests/references/tagging-eval/results-clip.json),
[SigLIP results](../../crates/lc-engine/tests/references/tagging-eval/results-siglip.json).

## 4. Historical prototype persistence and caches (removed)

During evaluation CLIP was the default in the engine enum, default ID and li-seg
official tagging bundle. Legacy preference JSON selected Fast through serde defaults.
Both models were selectable per library; the Library worker, status, model size, licence link and download
controls use the selected ID. Compositing Local AI lists both bundles and independent
controls. A switch clears the loaded tagger/review rows and requires that model's
embeddings. Existing on-disk per-model filenames/header/dimension validation is retained;
a new regression test verifies Fast → Accurate → Fast analyses **2 → 2 → 0** photos
with 512/768-dimensional caches kept separate, including disk reopen/persistence.

## 5. Historical evaluation validation

The changed-crate suites were each executed once, serially with
`CARGO_BUILD_JOBS=3`, `cargo +1.98.1 --offline`, and test threads set to one.
The remaining-crate command used `--no-fail-fast --show-output` so all targets
completed and hardware output was recorded.

| Changed crate | Runnable unit tests verified | Integration tests passed | Ignored in ordinary suites | Doc tests |
| --- | ---: | ---: | ---: | ---: |
| li-seg | 48 | 0 | 5 | 0 |
| lightcraft-engine | 423 | 264 (261 coverage, 1 backend, 2 panic-hunt) | 5 | 0 |
| lightcraft-ui-egui | 230 | 0 | 3 | 0 |
| photocraft-ui-egui | 989 | 71 (57 GPU, 14 UI/system) | 9 | 0 |

**2,025 runnable tests verified**, plus the **three** explicit real-weight checks
(faces, engine people and SigLIP) listed above. The full runs caught two stale
assertions I had missed: li-seg still expected six model specifications, and
Compositing's settings test expected CLIP's old label/five download buttons. Both
were corrected and **each passed a focused rerun**; the other tests were not
repeated. Thus the full-run logs retain those initial assertion failures, while
`model-table-final.log` and `settings-rows-final.log` record the successful fixes.
Li-seg doc tests were checked separately because Cargo stopped on that first
assertion; there are none. No failure remains unresolved.

Earlier focused development checks included 10 Library Smart Sort headless tests,
2 Compositing model-option tests, real-weight probes, Unigram/pixel/sigmoid goldens,
scoring saturation and model/persistence/cache checks. New UI tests **click** both
model choices and download controls, and cancel the SigLIP bundle in Compositing.

**GPU tests added: 0** (the new model inference is CPU). Existing GPU verification
passed on the actual hardware: the engine backend test reports
`NVIDIA GeForce RTX 5090 (Vulkan)` and asserts GPU-stage bytes/render logging;
Compositing's 57 runnable GPU integration tests passed, with the hardware icon
checks also identifying the RTX 5090 at both scales. Driver: 615.71.09.
`LC_REQUIRE_GPU=1 LIGHTCRAFT_GPU_BACKEND=vulkan` was set for the full runs.
No llvmpipe or skipped-adapter result is used as evidence.

Formatting and `git diff --check` are clean; Cargo.lock has no diff. Attributions
were regenerated offline (including notice-reference validation). All four
changed crates pass Clippy `--all-targets -- -D warnings` with
`CARGO_TARGET_DIR=target/clippy`; the final post-fixture-correction check is logged
in `target/tagging-eval/clippy-final.log`. The existing build warning about missing
craft-fonts remains; no font download was authorised or attempted.

Logs: `target/tagging-eval/final-tests.log`, `final-remaining-tests.log`, the two
focused correction logs above, `seg-doc-final.log`, and the lint/model/performance
logs in the same directory.

## Remaining scope and limits

The historical evaluation task was completed. SigLIP is not shipped and the current
product offers only CLIP. The optional OpenAI CLIP B/16 comparison
was not done: §4.2 supplies abbreviated hashes and its licence/model-card caveat;
only the fully pinned, approved CLIP/SigLIP/YuNet/SFace artifacts were fetched.
No rejected model or quantized ONNX was used. Those historical limitations do not change the measured decision to retain CLIP.
No further implementation work on the rejected candidate is planned. No changes are committed.
