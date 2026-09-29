# Local Image 0.5.0 validation

Validation date: **29 September 2026**. This report separates isolated Python regressions, browser checks with controlled model responses, and actual image generation through local ComfyUI. Timings are individual runs on the validation PC, not cross-model quality scores or general performance guarantees.

Guides: [Image Gen](IMAGE-GENERATION.md), [Cutout](CUTOUT-WORKSPACE.md), [model setup](GEN-MODELS.md), [LoRA library](LORA-LIBRARY.md), and [development/test commands](DEVELOPMENT.md).

## Complete Python regression

Every `tests/**/test_*.py` module ran in its own Python process, including both nested texture suites. The complete run finishing at **19:55 UTC** completed all **25 modules** successfully: **290 tests discovered, 289 passed, one expected skip, zero failures or errors**. Total subprocess time was 33.782 seconds. Runtime: Python 3.12.14, Windows x64.

| Module | Tests | Skipped |
| --- | ---: | ---: |
| `test_app_paths.py` | 5 | 0 |
| `test_cutout.py` | 26 | 0 |
| `test_ernie_image.py` | 4 | 0 |
| `test_flux2_image.py` | 8 | 0 |
| `test_frontend_render.py` | 3 | 0 |
| `test_generation_library.py` | 10 | 0 |
| `test_generation_model_details.py` | 4 | 0 |
| `test_hardware_guide.py` | 4 | 0 |
| `test_hidream_image.py` | 5 | 0 |
| `test_image_generation.py` | 16 | 0 |
| `test_image_upscale.py` | 8 | 0 |
| `test_layer_projects.py` | 29 | 0 |
| `test_local_comfy_client.py` | 7 | 0 |
| `test_lora_library.py` | 22 | 0 |
| `test_lora_workflow.py` | 2 | 0 |
| `test_managed_ai.py` | 29 | 0 |
| `test_qwen_image.py` | 21 | 0 |
| `test_qwen_setup.py` | 19 | 0 |
| `test_seedvr2_image.py` | 5 | 0 |
| `test_setup_routes.py` | 8 | 0 |
| `test_stock_integration.py` | 7 | 0 |
| `test_stock_library.py` | 13 | 0 |
| `test_z_image.py` | 5 | 0 |
| `texture/test_backend_texture.py` | 22 | 0 |
| `texture/test_fast_inpaint.py` | 8 | 1 |

The skipped test requires a native texture helper and a protected local photo fixture. Synthetic texture and persisted-layer tests ran. A duplicate ZIP-entry warning is intentional input to the hostile-project-archive test, which passed.

This run includes the library-boundary checks for actual PNG dimensions, final exact-family LoRA catalog checks and pixel-art withdrawal, ERNIE native graph and request validation, ERNIE-only backend readiness, and corrected Qwen opaque compositing and empty-background rejection.

The preceding full run found a document-ordering defect when two writes shared a Windows clock tick: reopening a file could choose the older edited session. Session writes now retain strictly increasing modification timestamps, with a frozen-clock regression covering both creation order and a later edit to the older session. The final run above passed this regression in both inherited backend suites. The preceding failure is retained in `qa-artifacts/v05/python-regression-session-order-failure.log` and `.json`.

Evidence is saved in [the complete log](../qa-artifacts/v05/python-regression.log) and [the machine-readable report](../qa-artifacts/v05/python-regression.json). The report records each test file's SHA-256, exit status, count, skip count and duration. The [runner](../qa-artifacts/v05/run-python-regression.py) discovers nested modules and creates fresh subprocesses to avoid shared test-module state. These artifacts are ignored local QA files and are not distributed with the app.

Coverage includes:

- Source-pixel preservation, brush/pen alpha refinement, feathering, shadows, undo/redo, inverse-transformed selections, translation/scale/rotation, and premultiplied-alpha edges without dark halos.
- Native 16-bit compositing/export, source-alpha import, transparent PNG export, opaque-format restrictions, editable project round trips, hostile archive rejection, and legacy project compatibility.
- Generation model capabilities, strict dimensions and model-specific inputs, seed zero, current-document reference snapshots, transparent-output validation, unsaved-document state and portable generation metadata.
- Native Qwen, Z, FLUX and HiDream graph wiring, semantic references versus Z's starting-image latent, both ComfyUI legacy and V3 combo schemas, and ordered model-only LoRA application. HiDream's model-specific default canvas is 2048 × 2048 even when dimensions are omitted from an API request; explicit dimensions are retained.
- Exact-model LoRA rejection before a GPU job or document is created, bounded strengths and identifiers, library/download integrity, authorization, shared operation locks, failure cleanup and model setup.
- Offline model descriptions, recommended sampling, per-variant storage and memory guidance, native-selected download destinations, folder-change reconnection messaging, and the managed launch's model-folder configuration.
- Faster completion polling, bounded timeout/error handling, and no job resubmission after a transient history-fetch error.
- Stock image, background and reference handoff, credit retention in editable projects, stale background requests, bounded remote downloads, thumbnail decoding and color-profile normalization.

Model execution, remote downloads and online Hub responses are mocked in these unit tests. They do not establish GPU inference or arbitrary third-party LoRA compatibility.

## Real Image Gen inference

The explicit GPU runner was [smoke_generation_live.py](../tests/smoke_generation_live.py). It called the app API against a running local ComfyUI, exported each PNG, verified its dimensions and alpha, checked its unsaved state and seed, and exported a project whose generation metadata matched the document. No mocked model responses were used for the following results.

Hardware recorded by the app: **NVIDIA GeForce RTX 5090, 31.8 GiB reported GPU memory; 61.4 GiB reported system RAM**. ComfyUI may offload model components to system memory. All images below are 1024 × 1024.

| Operation | Preset | Seconds | Verified image |
| --- | --- | ---: | --- |
| Text-to-image, red chair in a room | Qwen INT8 | 6.36 | Opaque RGBA |
| Transparent origami fox | Qwen INT8 | 6.24 | RGBA, alpha 0–255 |
| Combine chair and fox references | Qwen INT8 | 13.03 | Opaque RGBA, two references |
| Text-to-image, red chair in a room | Z-Image Turbo BF16 | 10.41 | Opaque RGB |
| Starting-image variation | Z-Image Turbo BF16 | 4.66 | Opaque RGB, strength 0.55 |
| Text-to-image, red chair in a room | FLUX.2 Klein 4B BF16 | 6.33 | Opaque RGB |
| Reference-guided chair edit | FLUX.2 Klein 4B BF16 | 4.55 | Opaque RGB |
| Text-to-image, red chair in a room | FLUX.2 Dev FP8 | 36.31 | Opaque RGB |
| Reference-guided chair edit | FLUX.2 Dev FP8 | 46.58 | Opaque RGB |

The transparent fox contains **77.638% fully transparent pixels**; both fully transparent and opaque subject pixels were present. This is an output characteristic, not a segmentation-accuracy score. Every opaque case had alpha 255 throughout when converted to RGBA.

Qwen used 25 steps and guidance 1; Z used 8 steps and guidance 1; distilled Klein used 4 steps and guidance 1; Dev used 20 steps and guidance 4. Text-to-image jobs used seed 0, the transparent fox used seed 42, and image-guided jobs used seed 123. Negative prompts were empty and no optional LoRAs were selected in this baseline.

Evidence: [Qwen/Z results](../qa-artifacts/v05/generation-qwen-z/results.json), [FLUX results](../qa-artifacts/v05/generation-flux/results.json), and [recorded hardware](../qa-artifacts/v05/generation-flux/hardware.json). Both result files report `complete: true`; each folder contains the corresponding PNGs, projects, exact prompts, returned settings and model inventory. The Qwen/Z run preceded the optional-LoRA metadata field; project loading accepts that earlier metadata shape. The FLUX run records an empty `loras` list.

The automated checks establish successful inference, image format, alpha handling, dimensions and metadata export. They do not score visual prompt adherence or establish that either image-guided model preserves every reference detail. Live checks used two Qwen references and one reference per FLUX model; maximum-reference counts are covered by request/graph tests, not by a maximum-memory GPU benchmark. Dev results are historical: the regular catalog now offers Klein 9B instead, while preserving Dev project compatibility. HiDream's initial results below are evaluation evidence for a model subsequently rejected on visual quality.

The initially anonymous Klein 9B download returned HTTP 401 because the publisher requires access approval. After the user accepted the publisher terms, the official **FP8 diffusion model and matching Qwen 8B FP8 encoder were installed and checksum-verified**. The app does not accept terms or manage publisher credentials automatically.

Klein 9B then completed the same **1024 x 1024**, four-step, guidance-1 chair test: text-to-image **6.49 seconds** and one-reference red-to-blue upholstery edit **3.67 seconds**. Both returned opaque RGB and passed PNG and editable-project metadata checks. Visual review found the chair shape, wooden legs and room arrangement closely retained, with the requested blue upholstery; small texture and lighting changes remain. Evidence: [9B results](../qa-artifacts/v05/generation-klein9b/results.json), [diffusion verification](../qa-artifacts/v05/klein-9b-diffusion-verification.json), and [encoder verification](../qa-artifacts/v05/klein-9b-encoder-verification.json).

A further Klein 9B studio portrait used seed 71, four steps and **1536 x 864**, completing in **7.453 seconds**. A preservation/refinement prompt used that portrait as one reference at **2048 x 1152**, seed 72 and four steps, completing in **6.468 seconds**. The person, pose, mug and broad studio composition remain similar, but small facial, material and shape details change. The requested exact text **LOCAL CERAMICS** is imperfect in the draft and remains malformed after refinement. This is useful semantic refinement, not guaranteed text reproduction or pixel-preserving restoration. [Portrait requests and results](../qa-artifacts/v05/klein-quality/results.json)

The installed **HiDream O1 Full FP8** model then completed real API inference at its recommended **2048 × 2048**, **50 steps**, guidance **5**. Text-to-image produced a furniture-exhibition poster in **44.17 seconds** with seed 0; a one-reference edit changed the red chair toward blue velvet in **100.84 seconds** with seed 123. Both returned opaque RGB and passed PNG dimensions and editable-project generation metadata checks. Visual inspection found readable poster typography and closely retained room/layout; the edit also changed parts of the chair arms, so exact material boundaries were imperfect. Evidence: [HiDream results](../qa-artifacts/v05/generation-hidream/results.json), [poster](../qa-artifacts/v05/generation-hidream/hidream-o1-text.png), and [reference edit](../qa-artifacts/v05/generation-hidream/hidream-o1-image.png).

The poster correctly rendered **FORM & LIGHT** and **DESIGN IN EVERYDAY LIFE**. The edit retained that text and the room layout, but extended the requested deep blue onto the wooden chair arms. This successful semantic edit does not establish pixel preservation outside the requested upholstery.

The validation PC's 32 GB RTX 5090 completed both HiDream jobs. Driver GPU-usage readings observed during these runs were **21,879, 22,455 and 22,705 MiB**. These are sampled observations, not a measured peak, model-only allocation or minimum VRAM requirement. Different resolutions, references, loaded components and offloading can change memory use. See [HiDream setup and measured limits](HIDREAM-O1.md).

The installed HiDream checkpoint is **8,067,535,296 bytes**, verified against its pinned SHA-256. The two Dev-only diffusion and text-encoder files were removed, reclaiming **53,490,239,687 bytes**; the shared FLUX VAE was preserved. The [deletion report](../qa-artifacts/v05/removed-flux-dev.json) records both exact paths and sizes. Klein 9B was subsequently installed with user-approved publisher access, as recorded above. The Dev measurements above describe historical tests before that cleanup.

The final HiDream diagnostic changed only the sampling pair to **res_multistep / beta**, following the native implementer's suggestion. At 2048 × 2048, 50 steps, guidance 5 and seed 0 it completed in **48.828 seconds**. Text remained correct, but regular grid/block texture persisted on the wall and floor and detail remained soft. This failed visual acceptance. **HiDream was removed from the model picker, hardware guide and app download choices**; its historical project metadata and downloaded weights are retained. No unvalidated replacement family was added. See [the workflow audit](HIDREAM-O1.md), [diagnostic result](../qa-artifacts/v05/hidream-audit/results.json) and [image](../qa-artifacts/v05/hidream-audit/res-multistep-beta.png).

## ERNIE poster and exact-copy acceptance

ERNIE-Image Base BF16 completed a real **1024 x 1536** poster with **50 steps**, guidance **4**, seed **87** and the unmodified user prompt in **70.719 seconds**. It used the 32 GB RTX 5090, exported an opaque PNG and retained model/settings in an editable project. Independent visual inspection found all five requested strings correct, each once, with no extra text: **SHAPE & SOUND**, **Independent Design Festival**, **SATURDAY 12 OCTOBER**, **STUDIO 08 / NEW YORK**, and **CREATE. EXPLORE. CONNECT.** No obvious malformed glyph or baseline defect was observed.

The image did miss a layout requirement: it rendered a paper sheet with a gray surround and drop shadow despite an explicit edge-to-edge/no-mockup instruction. The large circle also leaned toward magenta/red rather than vermilion. ERNIE is accepted as an optional text-heavy poster model based on this one exact-copy result; universal perfect typography or layout is not claimed.

The same fixed prompt and seed with Qwen Image 2.1 INT8 at 25 steps and guidance 1 completed in **13.078 seconds**. Four requested strings were correct, but the headline read **SHAPE & SOUNFD**, and three unsolicited small pseudo-text blocks appeared at lower left. This particular comparison favors ERNIE for exact copy, without establishing a general model ranking. Evidence: [ERNIE result](../qa-artifacts/v05/poster-quality/ernie-image/results.json), [poster preview](../qa-artifacts/v05/poster-quality/ernie-image/poster.png), [ERNIE visual review](../qa-artifacts/v05/poster-quality/ernie-image/visual-review.json), [Qwen result](../qa-artifacts/v05/poster-quality/qwen/results.json), and [Qwen visual review](../qa-artifacts/v05/poster-quality/qwen/visual-review.json). Model pins and workflow are in [the ERNIE guide](ERNIE-IMAGE.md).

## Fast draft measurements and manual refinement

After shortening completion-history polling for fast jobs, Z-Image Turbo was exercised sequentially with one fixed teapot prompt, no LoRA and guidance 1. The first 1024-pixel, 8-step job warmed the model using seed 2026 and took 3.415 seconds. The following jobs used seed 2027. Their measured duration covers the app request through the completed response and excludes image export.

| Square canvas | Steps | Seconds |
| --- | ---: | ---: |
| 1024 × 1024 | 1 | 1.151 |
| 1024 × 1024 | 2 | 1.577 |
| 1024 × 1024 | 4 | 2.332 |
| 1024 × 1024 | 8 | 3.520 |
| 512 × 512 | 4 | 1.647 |
| 512 × 512 | 8 | 1.573 |

These are single warm observations, not averages. Loading, allocation and cache state can affect them; the two 512-pixel results do not establish that eight steps are generally faster than four. Visual review of the seed-2027 examples found the one-step result soft and grainy, while four steps produced a cleaner, sharper teapot and table with coherent lighting. The app retains the recommended eight-step Z default while exposing lower step counts for experimentation. These few examples do not establish general quality at reduced steps. Evidence and output PNGs are under [z-image-fast-poll](../qa-artifacts/v05/z-image-fast-poll/results.json).

The real UI also completed a manual **Klein draft → FLUX.2 Dev reference refinement**. It opened the existing Klein draft, selected Dev FP8, added the current image as a reference, and requested preservation of the chair and composition while improving fabric, materials and lighting. The Dev pass used 20 steps, guidance 4, seed 456 and 1024 × 1024 output; it returned a new unsaved document after 57.484 seconds with no recorded browser errors. Visual review found the chair, window, rug and composition closely retained; fabric detail sharpened while the red became more saturated and the light and shadows stronger. Evidence includes the [request/result JSON](../qa-artifacts/v05/refinement/results.json), [draft](../qa-artifacts/v05/refinement/klein-draft.png) and [refined image](../qa-artifacts/v05/refinement/dev-refined.png). This is historical Dev validation: Dev is retained for old-project compatibility and is no longer offered in the regular model browser.

This is a supported manual reference-editing workflow. It does not automatically chain models or perform a dedicated upscale/low-denoise pass, and the second model can change composition or details. The user steps are in [the Image Gen guide](IMAGE-GENERATION.md#draft--refine-and-optional-upscaling).

After removing Dev, a second real UI test used the installed **Klein 4B 1024-square draft → HiDream Full 2048-square reference edit**, with 50 steps, guidance 5 and seed 789. Inference completed in 100.403 seconds; the UI, PNG and project exports completed in 101.395 seconds with no browser errors. The input and output sizes, reference count and sampling provenance were verified. Visual review was mixed: the broad chair/window/rug arrangement remained, but HiDream added a table, camera, plant and small objects despite an explicit no-additions prompt, and changed the upholstery, central seam, framing and color. This validates the draft-to-reference pipeline, **not** a faithful quality-only enhancement. Evidence and qualitative review are in [refinement-hidream/results.json](../qa-artifacts/v05/refinement-hidream/results.json).

## SeedVR2 4K enhancement acceptance

The selected **SeedVR2 7B FP16 base** upscaler completed a real **2048 x 1152 to 3840 x 2160** portrait enlargement in **13.812 seconds**, using one Euler/simple step, guidance 1, seed 31 and Lab color correction. A separate **1024 x 1024 to 2048 x 2048** transparent origami-fox enlargement took **5.593 seconds**. Both left their source files unchanged. These runs used the validation PC's 32 GB RTX 5090; no peak-memory measurement was made.

Independent side-by-side 100% crops compared SeedVR2 against Lanczos for the face, hand, linen, lettering and mug. The portrait shows a material apparent-detail gain in eyelashes, fabric weave and ceramic edges, with stable overall composition and no gross anatomy change observed. It also synthesizes pores, skin creases, thread patterns and glaze mottling. Dark letters gain a thin outline/halo, malformed source text stays malformed, and tiny stray marks become more character-like. The result is accepted as **optional enhancement**, not recovered ground truth.

The fox's alpha matches the Lanczos-resampled source **byte for byte**. Black-composited comparisons retain the source's light fringe while some fine paper folds smooth and surface detail changes. This verifies matte preservation, not matte correction. The quality scope is one generated portrait and one generated transparent asset; it does not establish universal photographic fidelity. Timings and visual crop coordinates are in [portrait metadata](../qa-artifacts/v05/seedvr2-quality/portrait-4k.json), [fox metadata](../qa-artifacts/v05/seedvr2-quality/transparent-fox.json), and [independent visual findings](../qa-artifacts/v05/seedvr2-quality/visual-review.json). See [SeedVR2 details](SEEDVR2.md).

## Cutout and removal baseline

The earlier real-model acceptance remains documented in [Qwen and Cutout validation](QWEN-VALIDATION.md), including its source fixture provenance, exported alpha statistics, visual findings and per-operation evidence. Its inputs were AI-generated backpack/cup and long-haired-cat photographs, created with the built-in image generator; Qwen then performed the actual removal, extraction and background generation.

That run exercised Qwen INT8 cup removal, backpack extraction, empty studio-background generation, local shadow compositing, subject transform and cat extraction. BF16 was separately exercised on the cat. The final removal comparison verified **1,512,573 outside-mask pixels unchanged**, zero changed pixels and zero maximum channel difference. The app preserves that region through local masked compositing.

The extracted subject uses the original photograph's RGB with the generated alpha; backgrounds, transforms and shadows remain editable. The cat shows some blue-chair contamination at the fur edge and loss of fine whiskers. BF16 did not remove those defects in that example, so manual cutout refinement remains relevant. The empty-scene instructions worked for the tested prompt; guidance-1 negative prompts alone do not guarantee empty backgrounds.

The complete current Python run above rechecked the cutout, repair, project and precision behavior after adding Image Gen. The earlier 0.4.0 packaged Cutout results are historical evidence, not a claim that the 0.5.0 installer has already passed every packaging check.

## Browser interaction regressions

All seven browser suites passed against the 0.5.0 source server on port 51249: `test_ui_browser.cjs`, `test_ui_cutout.cjs`, `test_ui_setup.cjs`, `test_ui_navigation.cjs`, `test_ui_projects_layers.cjs`, `test_ui_generation.cjs`, and `test_ui_stock.cjs`.

The browser and Cutout suites exercise real local import, CPU repair, mask refinement, composition, projects and export. Controlled model responses are used for AI availability and selected request/response behavior. Setup and generation suites simulate native download actions and online LoRA responses; they do not install a model or submit inference while checking those controls. The Image Gen suite also uses the actual hardware API and reference import path.

Image Gen checks cover five-model capability changes, width/height and seed validation, references, per-file LoRA compatibility decisions, selected adapter payloads, FLUX and HiDream sampling defaults, generated-document handoff, missing project assets, preservation of an unfinished prompt, keyboard operation and three layouts: **1440 × 900, 1024 × 768 and 800 × 600**. The browser and setup suites each check four desktop layouts. Cutout checks include subject dragging/scaling/rotation, refinement after a transform, undo/redo, shadows, background imports and the conditional bottom filmstrip.

Screenshots are under `qa-artifacts/studio-browser/`, `studio-cutout/`, `studio-setup/` and `studio-generation/`. These interaction regressions are separate from the real inference evidence below and above.

## Real UI-to-LoRA generation

An additional explicit acceptance used the actual Image Gen UI, installed adapter registry, ComfyUI model and PNG/project exports. [smoke_generation_browser_live.cjs](../tests/smoke_generation_browser_live.cjs) selected **Z-Image Turbo BF16**, opened the LoRA library, selected **Children's drawings**, and generated through the normal button without intercepted model responses.

The adapter was `ostris/z_image_turbo_childrens_drawings`, revision `7fcd66a99149c58741990fca28562a4a581af7a9`, SHA-256 `25b9959eb2054ccb9dd1815e90add6da3818f550e8e60a081392095830ff0f7a`. It was used at strength **1**, with **8 steps**, guidance **1**, seed **20260929** and **1024 × 1024** output. The prompt requested a child's crayon drawing of a yellow submarine, pink fish and blue ocean on white paper.

The UI action, image generation, screenshot and exports completed in **10.649 seconds**, with **zero browser errors**. Visual inspection found the intended hand-drawn submarine and colored shapes. The returned document retained the exact selected LoRA ID and strength; the runner downloaded both its PNG and editable project. This verifies one installed style adapter on Z-Image Turbo. The unit graph checks cover LoRA routing for Qwen and both FLUX models, but this run does not establish visual compatibility for every adapter or model.

Evidence: [result JSON](../qa-artifacts/v05/live-studio/results.json), [studio screenshot](../qa-artifacts/v05/live-studio/z-image-lora-studio.png), [generated PNG](../qa-artifacts/v05/live-studio/z-image-lora.png), and [project](../qa-artifacts/v05/live-studio/z-image-lora.lremove). The JSON records `real_ui: true` and `real_gpu: true`.

## Ten retained style adapters and Qwen alpha correction

Eleven candidate adapters ran on the actual GPU. Ten loaded cleanly and remain offered: **three Qwen 2.1, two Z-Image Turbo, two Klein 4B and three Klein 9B**, totaling **1,297,544,776 bytes**. The pixel-art candidate emitted six unmapped global modulation tensor warnings; the author's alternate format had the same mappings missing. Its newly installed registry entries were removed from the three profiles and the repository marked unsupported. The original weights and outputs remain as audit evidence. See [the final manifest](../qa-artifacts/loras/curation/release-manifest.json) and [withdrawal record](../qa-artifacts/loras/curation/pixel-art-withdrawal.json).

| Model | Retained style | Actual request, seconds |
| --- | --- | ---: |
| Z-Image Turbo | Children's drawings | 9.078 |
| Z-Image Turbo | Photographic realism | 4.609 |
| Qwen INT8 | Natural exposure | 5.578 |
| Qwen INT8 | Anime character consistency | 9.235 |
| Qwen INT8 | Faceted card illustration | 10.110 |
| Klein 4B | Watercolor wash | 2.313 |
| Klein 4B | Claymation miniature | 2.734 |
| Klein 9B | Orange splatter illustration | 7.297 |
| Klein 9B | Teal dark illustration | 3.156 |
| Klein 9B | Blueprint wireframe | 3.047 |

These are one-example timings rather than averaged comparisons. Visual review found recognizable intended styles, coherent scenes and no gross corruption in the retained outputs. It also found duplicated objects in watercolor and clay, a generated signature in watercolor and invented fine labels in blueprint art. The realism example alone does not prove an improvement over its base model. The Qwen exposure edit kept the portrait and studio broadly coherent but retained malformed source text. The anime edit retained the fresh reference's hair, outfit, pose and framing while changing the mouth to the requested smile; the card example produced a coherent faceted shield and red rim light.

The first Qwen anime and card exports revealed purple hidden RGB because the app had replaced meaningful alpha with 255. Raw ComfyUI PNG inspection confirmed actual transparency: the original anime baseline, anime edit and card had approximately 55.95%, 54.49% and 30.35% alpha-zero/one pixels respectively. This was an app compositing defect, not evidence of a purple-background style. Opaque Qwen generation now normalizes only alpha 254–255 and composites over white. The dedicated background task rejects results with more than 1% of pixels below alpha 250; fully transparent results are rejected. Transparent generation keeps its alpha, and cutout endpoint cleanup remains separate.

All three Qwen styles were then rerun through the corrected app. The anime edit used a fresh, correctly composited baseline. Independent full-image review found no purple background; every final retained PNG is fully opaque and every retained adapter has a clean load log. The baseline's requested park is still absent, so white is explicitly a fallback matte, not a fulfilled scene instruction. Tests now cover these boundaries. Evidence: [raw alpha audit](../qa-artifacts/v05/lora-quality/raw-alpha-audit.json), [corrected Qwen runs](../qa-artifacts/v05/lora-quality/alpha-fixed/results.json), [final ten-style results](../qa-artifacts/v05/lora-quality/accepted-style-results.json), [final comparison](../qa-artifacts/v05/lora-quality/accepted-styles-comparison.png), and [visual findings](../qa-artifacts/v05/lora-quality/visual-review.json). Style triggers and publisher sources are in [the LoRA guide](LORA-LIBRARY.md).

## Stock-library validation

Openverse is the sole in-app stock connector in this build, restricted to Flickr-hosted results with supported Creative Commons or public-domain metadata. Pexels and Unsplash remain external website links. No Wikimedia Commons connector is offered.

The stock suites currently pass **20 tests**: 13 provider/download tests and seven editor/project integration tests. Provider coverage includes allowed remote hosts and redirects, byte limits, opaque expiring result IDs, thumbnail handling, image normalization and ICC-to-sRGB conversion, input validation and the write guard. No browser-supplied arbitrary download URL is accepted.

Integration tests cover RGBA pixels and unsaved document state; portable source, reference and background credits; generation-reference credit propagation; background undo/redo; source alpha; retention of the current transform, shadow and repair layers; stale-revision rejection after a slow download; and invalid project credit links. Opening stock as a new image does not overwrite a source file. Applying stock as a background compares the document revision under its session lock.

The real stock UI searched Openverse for `forest`, returned 12 results, and successfully fetched and decoded all 12 thumbnails. It imported the actual **Fall-Forest** image by **Chris Sorge**, licensed **CC BY-SA 2.0**, as an editable image, a cutout background and a generation reference. Saving and reopening the background project retained the credits. This run exercised the live provider, editor and project paths. Evidence: [live stock results](../qa-artifacts/studio-stock/live-stock-results.json), [thumbnail results](../qa-artifacts/studio-stock/thumbnail-results.json), screenshots at 1440, 1024 and 800 pixels, and PNG/project exports under `qa-artifacts/studio-stock/`.

All seven UI suites passed after adding stock and the model-browser changes. That earlier generation suite also checked Klein 9B and the then-visible HiDream controls using controlled inventory and generation responses; HiDream has since been retired. Actual Klein 9B inference is recorded separately above. Legacy Dev projects retain an explicit unavailable historical model selection rather than silently switching their settings to another model.

## Final packaged acceptance

The final Windows 0.5.0 package built successfully with Python 3.12.14, PyInstaller 6.22.3 and Inno Setup. Acceptance ran the packaged executable with an isolated profile rather than the source server. It includes the ERNIE option, ten retained style adapters and corrected Qwen alpha handling.

**All ten packaged checks passed:** backend pixel/16-bit TIFF/project preservation; native model-folder configuration and restoration; Cutout UI; Image Gen UI; Draft & Refine UI; stock UI; actual Openverse search/import/credits; actual Z-Image Turbo generation with the installed Children's drawings LoRA; actual SeedVR2 upscaling and library lifecycle; and actual ERNIE poster generation.

The real UI-to-Z-LoRA generation and PNG/project export took **9.519 seconds**, with zero browser errors. Packaged SeedVR2 enlarged the portrait from 2048 x 1152 to **3840 x 2160 in 16.453 seconds**, retained upscale provenance through a project round trip, and opened the cached result in an editor document. Deleting that selected library entry freed **11,026,745 bytes** while preserving the original source, opened document, saved projects and both pre-existing library entries.

Packaged ERNIE produced the same 1024 x 1536, 50-step, guidance-4 poster test in **74.093 seconds**. Visual inspection again found all five requested strings correct, each once, with no extra text. The unwanted paper-sheet mockup remained, so packaged acceptance confirms the working model path and this exact-copy example, not perfect layout compliance.

The native executable passed 12 trusted-origin checks, seven project-boundary checks, seven close-handshake checks and six setup-bridge checks. The WebView launch probe reported `Local Image`, bridge v2 and Microsoft WebView2 runtime 154.0.4258.37.

The executable's ten embedded icon frames, from 16 through 256 pixels at 32-bit depth, match the source icon. The packaged HTML, JavaScript, stylesheet and app-icon files match the final source bytes. [Asset verification](../qa-artifacts/v05/packaged-icon-check.json)

Evidence: [all ten packaged checks](../qa-artifacts/v05/packaged-checks/results.json), [native self-test](../qa-artifacts/v05/native-self-test.json), [WebView probe](../qa-artifacts/v05/packaged-native-probe.json), [packaged LoRA result](../qa-artifacts/v05/packaged-checks/live-lora/results.json), [packaged 4K/library result](../qa-artifacts/v05/packaged-checks/live-upscale-library/results.json), and [packaged poster result](../qa-artifacts/v05/packaged-checks/live-poster/results.json). Installer: `dist/local-image/installer/Local-Image-Setup-0.5.0.exe`, with an adjacent SHA-256 file. Earlier 0.4.0 packaged results remain in the historical Cutout report.

