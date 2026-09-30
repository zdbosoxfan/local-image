# Local Image 0.7.0 validation

Validated on September 29, 2026. This Windows preview adds the changes from the simulated customer reviews and an image-led LoRA browser. The installer includes the native host, Python backend and interface resources; AI runtimes and model weights remain separate.

## Interface and workflow changes

The LoRA library shows large examples, short names and a small **i** button for usage, triggers, recommended settings, compatibility and publisher/license details. Ten bundled examples come from recorded tests of the exact retained adapters. Live publisher examples are labeled separately. Search and Refresh retrieve current Hugging Face results; recommendations remain available offline. Missing examples have an explicit fallback rather than an invented preview.

Editing now uses consistent menu, keyboard and visible history actions. Numeric transform/shadow inputs accept exact values, and pending brush selections or unfinished pen paths are reviewed before export. The image library distinguishes focusing an image from selecting several for deletion. Compact, Comfortable and Large interface sizes persist; Large uses 200% text with scrolling and reachable controls at the tested narrow sizes.

Draft & Refine keeps independent model, reference, resolution, steps, alpha and LoRA settings for each stage. Saved recipes identify missing dependencies. A visible warning explains that refinement can change lettering or composition; full-resolution comparison supports synchronized zoom and pan. Saved product treatments prepare a bounded queue of independent images, with fixed reviewed settings, per-image comparison and export status, cancellation and resume. Credits can be copied or saved beside exports.

First startup leads with four tasks and expandable GPU planning guidance. Read-only model inventory is cached briefly and can be refreshed explicitly. Engine progress reports the current operation and sampler steps when available, with elapsed-time fallback rather than an invented overall percentage.

## Regression and actual execution

| Check | Result | Scope |
| --- | --- | --- |
| Python regression | 360 passed, one expected skip; 361 discovered across 30 modules | Editing precision, projects, generation graphs, previews, setup, credits, queue integrity and lifecycle |
| Interface checks | 16 passed | 13 browser suites against the packaged backend plus three source-script contract suites; provider and inference responses are fixtures where stated |
| Native host self-test | Passed | 15 origin/download, seven project, seven close, six setup and three batch boundary checks |
| Live LoRA metadata and previews | Passed for Qwen, Z-Image Turbo, Klein 4B and Klein 9B | Twenty current search results per family and one actual publisher preview per family from the rebuilt package; no weights downloaded |
| Live preview correction | Ten preview tests and 15 source-network examples passed | Anonymous trusted Hub image redirects to its exact CDN, bounded PNG decoding/cache, and rejection of untrusted or credentialed targets |
| Actual GPU draft/refinement | Two requests passed | Klein 4B draft followed by Qwen INT8 reference refinement with the card illustration adapter and native RGBA output |

The actual GPU requests used 512 × 512 output: Klein 4B at four steps took 6.219 seconds, and Qwen refinement at 25 steps took 8.922 seconds on this PC. The Qwen result had 144,991 fully transparent pixels, 49,894 fully opaque pixels and 67,259 partial-alpha pixels. The cup and handle remained recognizable while its surface changed to the requested illustration style. These are individual requests, not latency averages, exact-geometry guarantees or benchmarks of every model/resolution. Earlier model-quality findings remain in [the 0.5 validation](LOCAL-IMAGE-VALIDATION.md).

The metadata measurement reduced five uncached ComfyUI inventory requests to one warm-up request plus cached reads; twelve concurrent callers shared one request. Recorded warm-read median was 27.648 ms versus 79.946 ms for uncached reads. This measures local metadata access, not GPU inference speed or a Core Web Vitals trace.

## Full real model and export matrix

The release run completed **88 cases and 440 verified exports** on an NVIDIA RTX 5090 with approximately 32 GB VRAM. There are 352 raster images and 88 editable projects. Every result was saved as PNG, JPEG, WebP, TIFF and `.lremove`; raster dimensions, ICC profiles and hashes were checked, and each project passed ZIP integrity, embedded-asset hashes, original-image decoding and generation-metadata checks.

| Generator | Cases |
| --- | ---: |
| Qwen Image 2.1, INT8 and BF16 | 40 |
| Z-Image Turbo | 8 |
| FLUX.2 Klein 4B | 8 |
| FLUX.2 Klein 9B | 16 |
| ERNIE-Image Base | 1 |

These 73 generation cases cover every valid subset of the ten installed curated LoRAs within its model family, supported text/image input modes, both Qwen precision choices, and opaque/transparent output where supported. The additional 15 cases cover maximum reference counts, background removal, generated backgrounds, transformations and shadows, removal engines, and an exact 3840 × 2160 SeedVR2 export. Seeds, strengths, adapter order and sampling settings are recorded; the run does not exhaust continuous parameter ranges or future community downloads.

All 88 PNGs were visually reviewed. Technical success is distinct from aesthetic quality. The combined Z-Image realism/children's-drawings preset lost subject fidelity; Qwen's hard rectangular removal test showed tonal seams; the large-object Telea stress test showed a gradient artifact. These outputs are retained and labeled in the accompanying quality notes. Individual adapters produced recognizable styles, and ERNIE reproduced both requested poster strings in this test. No model is claimed to guarantee exact identity, geometry or lettering.

JPEG cannot retain alpha. The cutout export guard correctly rejected transparent JPEG attempts; the completed cases explicitly used a white background for JPEG and restored transparency for PNG, WebP, TIFF and the project. The rejected attempts remain in the report. Tests used synthetic reference images and the local ComfyUI service. No model downloads or user-original images were needed for this run.

Evidence: `qa-artifacts/v07/release-matrix/final-results.json`, `export-manifest.csv`, `matrix.json`, `results.json`, `supplemental-results.json` and `QUALITY-NOTES.txt`. The labeled Desktop gallery contains the actual outputs and settings.

## Packaging, storage and exported files

The packaged smoke uses a copied application tree with a real Windows ACL denying writes and deletes, two simulated fresh Unicode-named AppData profiles and an actual WebView2 host. It checks first launch, retained settings, native page loading, wrong-profile denial, matching-profile shutdown, unchanged installation bytes and restored ACLs. CPU editing opens a generated compressed 16-bit TIFF, exercises both Quick Heal methods, saves an editable project and verifies original pixels. Existing ComfyUI discovery is read-only.

Packaged batch API checks verify credentialed folder export, unique filenames preserving existing files, correct subject/background pixels and credit sidecars. A controlled interrupted manifest resumes without duplicate outputs; cancellation retains completed previews; cache deletion preserves documents, originals and external exports. Original-format TIFF export retains exact 16-bit samples. A controlled active batch rejects shutdown, and matching packaged bytecode confirms the same guard and idle watcher are installed.

The actual native interface also prepared two synthetic cutouts, opened the Windows folder picker and exported them to the chosen test folder. Both rows reported their exported filenames and **Export complete**. The files retain exact RGBA pixels, attribution sidecars and unique names without changing originals or earlier outputs. A long permission wait exposed an expired request while the folder dialog was still open; interactive requests now wait for OK or Cancel. Tests advance the real request/reply code beyond ten minutes and check successful completion, cancellation and cleanup. The ready handshake and background-command deadlines remain bounded.

Three actual per-user QA install/uninstall cycles checked both custom folders, models only and portable runtime only. All 1,090 files in the earlier candidate matched its frozen payload. Folder choices, retained storage markers, registration and shortcut preservation passed. The wizard and registration source did not change in the subsequent preview correction; the final installer receives separate payload/checksum validation and production PLANONLY checks.

The package audit compares 45 application Python modules with current source, 21 interface/workflow resources with source bytes, and all ten examples with their recorded inference files and provenance. It scans for model weights, application profiles, credentials and test caches. Bundled example images are deliberate product resources; personal photos, weights, launcher credentials, browser profiles and QA run directories are excluded.

## Repeated simulated review and evidence limits

The reviews are explicitly simulated customer personas, not recruited participants or customer testimonials. The third round found no unresolved material usability findings in its tested scope. Corrections from the second round include model-switch search races, unrelated style-search recommendations, mutable reviewed batch settings, pending selections on inactive images, comparison labels and a delayed-startup navigation race. Automated reproductions and independent inspection support those closures.

Local evidence is under ignored `qa-artifacts/v07/`: `python-regression.json`, `packaged-ui-release/`, `live-lora.json`, `gpu-acceptance/`, `release-matrix/`, `deployment-release/`, `packaged-batch-api/`, `packaged-batch-guard.json`, installer reports and the final acceptance audit. Reproducible checks are in `tests/`. The repository contains the implementation and tests; private QA profiles and run artifacts are not committed.

Testing used one physical Windows PC. Fresh user profiles and a protected installation directory were simulated; an elevated all-users installation cycle and a second physical PC were not tested. Actual folder-picker testing used the normal Windows shell with an explicitly isolated app profile. Multi-gigabyte portable/model downloads were not repeated for this update. The preview is unsigned. Live search and publisher examples depend on external availability, and displayed appearance does not establish compatibility or license rights.
