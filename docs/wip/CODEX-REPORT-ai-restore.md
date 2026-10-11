# Restoration lane — implementation and verification

Task: `CODEX-TASK.md` and the owner's BYO extension, 2026-10-10. Changes are uncommitted. The starting revision was `2b3989c0`; initial status contained only the untracked task/research briefs. No restoration-specific builder source was present. This work extends the existing family profiles, verified downloader, model browser, ComfyUI client, pixel conversion and Session undo transaction.

## Implemented

- Immutable embedded `families/restoration.json`: FBCNN colour, SCUNet-real **PSNR**, NAFNet-SIDD width 64 and **only the relicensed Apache-2.0 HF DDColor tiny weights**. All four have official pinned sources, exact byte counts, SHA-256 and licence links. Editable/downloaded restoration profiles cannot widen the policy. Models are tool-only, excluded from Generate.
- Offline Restoration browser cards and a strict PTH exception for these four exact pins. Plans resolve to canonical sources/licences and use the existing redirect-checked, streaming, atomic verified installer. Generic discovery still rejects arbitrary PTH. Both browser installation and preset downloading reject altered names, folders, URLs, lengths or hashes.
- Default execution verifies regular-file size/hash on every request, matches the configured local ComfyUI installation to the server's arguments, and uses its own folder resolver to reject conflicting same-name copies. Read-only interrogation of the configured interpreter requires a main-spandrel 1× RGB image descriptor. It never imports extra arches. User-installed extra arches for BYO do not disable otherwise valid main architectures. Successful architecture evidence is cached only while the actual package source fingerprint, model pin, path and server arguments remain unchanged; file hashing/path resolution are repeated.
- Four-node core workflow: `LoadImage → ImageUpscaleWithModel → SaveImage`, with `UpscaleModelLoader` feeding the image node. Node provenance must identify core modules. Existing upload, queue admission, progress, cancellation, history and output download are reused. Output dimensions are checked and source alpha restored. FBCNN uses automatic quality-factor prediction because the core node has no quality-factor input.
- Compositing → Filter → AI → Restore…: task selection, licences, async preview, draggable before/after split, elapsed time, cancellation and model-browser navigation. Actions remain visible above the preview. Apply inserts a root raster layer in **one history step**, hiding the original root layers in the same step to preserve partial transparency. Their pixels/hierarchy remain intact; undo restores previous visibility. Document revision/switch, task change and BYO selection changes invalidate previews. Closing cancels the owned job.
- **Bring your own model**: read-only `/object_info` and model-folder detection of LED, Zero-DCE, Zero-DCE++, StableSR, CodeFormer, PromptIR, GFPGAN v1.4 and RestoreFormer++. Compatible core, simple community image-node and loader/processor contracts are discovered from actual advertised types/defaults. Only weight-filename choices count. Unknown community nodes require a matching local file and cannot turn a hardcoded download catalogue into an installed-model card; required automatic download/install flags and remote URL defaults are refused. Standard and recognised named model directories are scanned without following symlinks. StableSR uses its installed community adapter and a user-selected installed diffusion checkpoint, following the upstream non-turbo example. User-installed extra arches are permitted here; Local Image installs no architecture packages or community nodes.
- Every detected BYO model displays its licence name/link and the exact owner-required commercial-use notice. Preview is gated by an atomically persisted acknowledgement specific to model, server and licence-policy revision. RestoreFormer++ also displays the FFHQ background-risk explanation. Known face-restore nodes require the existing detector/parser files in the running ComfyUI model root, preventing known first-run auxiliary downloads. Unsupported files remain visible with a compatibility reason. The legacy face loader cannot use RestoreFormer++; an installed compatible image node is needed.
- **No BYO downloads**: browser results/plans/installers, preset and lowest-level downloads reject recognised BYO model names/URLs. SUPIR and HYPIR are excluded from detection, graph submission and download paths even if installed or acknowledged. No BYO entries were added to bundled-component attributions. `docs/BYO-MODELS.md` contains the owner's licence summary, short quoted clauses, primary sources and output/software distinction, including the requested “not legal advice” statement.
- ComfyUI model-path setup includes `upscale_models`. Four default-model upstream licence texts and curated/generated attributions are registered. English UI strings are in the pending i18n list. No dependencies, unsafe code, model binaries or community nodes were added. No desktop window was opened or owner application stopped.

## Unfinished requirements

1. **Default DDColor inference / Colourise** cannot run with the installed main-spandrel registry. Installed spandrel 0.4.2 and the official upstream support table put DDColor in `spandrel_extra_arches`. Its relicensed HF weights are pinned and available in the permissive browser section, but the default execution path enforces the original main-only rule and refuses them. The owner's extra-arches exception explicitly applies to BYO models. No DDColor quality/speed result is claimed.
2. **Microsoft scratch detector → mask → AI Remove → optional Colourise** is not implemented. The upstream detector is a separate one-channel UNet, outside ComfyUI core/main spandrel. The old Azure URL fails, but a final source re-check found the **official GitHub v1.0 release is reachable**. Its archive directory was inspected using a bounded HTTP range read: the 2,036,400,762-byte ZIP contains `checkpoints/detection/FT_Epoch_latest.pt` (451,663,663 bytes). No model file from that ZIP was downloaded or installed. The checkpoint is PyTorch; no official ONNX export was found. The current app has no verified Rust-compatible detector implementation/artifact for it. The UI and this report were corrected to remove the mistaken checkpoint-unavailability explanation. The old-photo preset refuses submission; no heuristic substitute, unvetted third-party export, dlib stage or custom node was added. Completion needs a vetted conversion/loading and Rust inference path, followed by detector-mask/AI Remove integration and real validation. This is unfinished implementation, not a claim that Microsoft's weights are unavailable. Default DDColor remains independently blocked.

These blockers are recorded as unfinished requirements, not completed features. The task's licence and backend rules are preserved.

## Model pins

| Task | Official file | Bytes | SHA-256 |
|---|---|---:|---|
| JPEG cleanup | FBCNN v1.0 `fbcnn_color.pth` | 287755111 | `8b0e4ef23d59cf7ac934a342cb31a17619e4fa4a0b3374a9d78c5174312387e8` |
| Scan cleanup | cszn/KAIR v1.0 `scunet_color_real_psnr.pth` | 71982841 | `fa78899ba2caec9d235a900e91d96c689da71c42029230c2028b00f09f809c2e` |
| Denoise | author's HF Space revision `5964ed4955416df99210106b708e4a2df9e9eca0`, `NAFNet-SIDD-width64.pth` | 464154961 | `cd685efaae01f7c4e9951f2deab05780079c8eb1e49ed664b72f6db04dabb445` |
| Colourise | author's relicensed HF revision `e9e7b527709c8aeb2f5e1bf701e72fd468a13baa`, `ddcolor_paper_tiny.pth` | 220393145 | `81dd643904f4664c3718513e3320ae3db0c567f0d3e18398606659adee7bfc17` |

FBCNN/SCUNet pins were established by streaming the official release bodies into a hash accumulator without saving model files. Author-space/model LFS metadata supplied NAFNet/DDColor pins. Persisted test copies use the real `browser::plan`/`browser::install` app flow in `~/.cache/local-image-test-models/restoration/upscale_models/`, with size/hash verification.

## Verification

Final validation:

| Check | Result |
|---|---|
| Strict clippy, li-ai + photocraft-ui-egui, all targets, dedicated `target/clippy` | Passed, `-D warnings` |
| Full li-ai unit / coverage suites, once | 117 / 319 passed; one ignored in each target |
| Final affected restoration/BYO tests after catalogue guards | 10 passed, one opt-in GPU test ignored |
| Full UI unit suite, once | 994 passed, one i18n failure, six ignored; failure fixed and affected i18n test passed |
| Final restoration UI click/drag tests | Four passed, one opt-in GPU test ignored |
| Remaining UI GPU / integration targets, once | 59 / 14 passed; four GPU tests ignored |
| li-attributions full suite | Four passed |
| Doc tests, li-ai / UI / attributions | Zero tests, passed |
| Opt-in restoration real RTX 5090 test | One passed; three actual models + DDColor refusal |
| Opt-in UI real RTX 5090 preview/apply/render test | One passed |
| Formatting and `git diff --check` | Passed |

The UI suite's lone failure identified `Task`, `Before` and `After` missing from the owner-deferred English pending list. Those entries were added and the failed test rerun successfully. The full unit suite was not repeated. The GPU/integration targets, which cargo did not start after that failure, were subsequently run once. Final targeted tests cover the later conservative BYO catalogue guards and the actual updated dialog. Logs are `target/restoration-final-*.log` and `target/restoration-clippy.log`. No whole-workspace test suite was run.

One cargo build in this job, `CARGO_BUILD_JOBS=3`, offline toolchain 1.98.1 and the existing machine-wide rustc/test gates were respected. Coordinator pause left changes intact; an earlier odd-dimension hardware run was stopped during severe shared-memory/IO pressure after FBCNN/SCUNet passed. That interrupted run is not counted as passing. Both explicit hardware tests subsequently passed under normal resource availability.

The owner's shared ComfyUI on 8188 was unreachable, including the resume-time `/queue` check. A separate headless ComfyUI 0.38.0 on loopback **8195** uses the owner's interpreter/code read-only, custom nodes disabled, approved test-cache models, in-memory database and worktree-only input/output/temp/user directories. `/system_stats` identifies CUDA NVIDIA GeForce RTX 5090, PyTorch 2.14.1+cu130 and 32 GB VRAM. Small jobs run serially with queue admission. After validating its exact worktree/port arguments and empty queue, only the owned test server (PID 1575309, port 8195) was stopped. The desktop application and owner settings were untouched.

Added automated tests exercise all four default graph shapes/pins, profile tampering, architecture refusal, download plans, mock image/dimension/alpha/cancellation, all eight BYO detections (including V3 combo metadata and folder-only discoveries), per-model persisted acknowledgement, download exclusions and SUPIR/HYPIR rejection. egui_kittest actually clicks the menu/tasks/BYO acknowledgement/preview/apply/cancel/browser navigation and drags the split. Opt-in real hardware tests require the 5090 rather than CPU fallback, exercise original-alpha/odd-dimension restoration and click the real preview/apply path with wgpu renders/screenshots.

`examples/restoration_probe.rs` measures PSNR and cold/warm app-path timings on a deterministic 256×192 synthetic gradient/edge/texture fixture. Warm inputs change a pixel to force actual inference rather than ComfyUI's identical-input output cache. Results are synthetic regression probes, **not** SIDD validation or comparisons with commercial restorers. BYO community nodes/weights are validated with mocks only; none were installed or downloaded for testing.

## Real RTX 5090 measurements

All runs used the app's pin-verification, architecture-detection, upload/queue/progress/download path. Cold time includes initial external-interpreter architecture recognition; warm runs reuse valid recognition evidence while still hashing the file and resolving its backend path. Each warm input differs by one pixel, forcing a fresh ComfyUI image inference. Three warm samples per model; median shown.

| Model / synthetic task | Cold ms | Warm median ms | Input PSNR dB | Output PSNR dB | Gain dB |
|---|---:|---:|---:|---:|---:|
| FBCNN / JPEG quality 15 | 3403.45 | 420.18 | 35.7417 | 39.5807 | 3.8389 |
| SCUNet-real PSNR / deterministic RGB noise | 2830.36 | 325.43 | 24.9360 | 44.1978 | 19.2618 |
| NAFNet-SIDD / same noise | 4750.24 | 610.93 | 24.9360 | 30.6365 | 5.7005 |

The independent 65×49 hardware test passed all three models, preserving every alpha byte: FBCNN 2491 ms, SCUNet 2308 ms, NAFNet 3330 ms, including cold recognition in a new process. It also verified the real Apache DDColor file and observed main-spandrel refusal without submitting it. `target/restoration-quality.log`, `target/restoration-gpu.log` and `target/restoration-real/*` hold local evidence. The existing test-runner shell printed non-fatal RSS arithmetic warnings while short child processes exited; cargo/test exit status and model assertions succeeded. The owner's runner was not modified.

The headless **actual egui UI path** selected each task, clicked Preview, waited for completion, rendered ten wgpu frames, saved screenshots and clicked Apply/Undo. The adapter asserted NVIDIA GeForce RTX 5090 / Vulkan / DiscreteGpu (driver 615.71.09); no CPU adapter or skip counted.

| UI task (256×192) | Preview ready ms | Mean update+render frame ms | Apply + three UI updates ms |
|---|---:|---:|---:|
| JPEG cleanup | 2905.73 | 11.10 | 16.68 |
| Scan cleanup | 2458.84 | 8.37 | 7.77 |
| Denoise | 3353.67 | 6.98 | 7.52 |

These UI previews are cold per-model architecture checks in a new test process; timings are not large-image throughput promises. `target/restoration-ui-gpu.log` and `target/restoration-ui/{Jpeg,Scan,Denoise}.png` hold the results. The Scan screenshot was visually inspected: all task/action controls, licence link, split preview and elapsed time are visible, with no desktop window opened.

## Sources followed

- Existing repository conventions: `docs/specs/smart-sort.md` §4; `li-ai/{family,browser,download,comfy,setup}.rs`; existing family profiles; `pc-engine::ai_cmds::{flatten_rgba,layer_from_rgba}`; `Session::edit`; existing egui_kittest menu and AI tests; attribution generator and i18n pending lists. UI and pixel assembly reuse these implementations.
- [ComfyUI core upscale nodes](https://github.com/comfyanonymous/ComfyUI/blob/master/comfy_extras/nodes_upscale_model.py), also inspected in the owner's installed 0.38.0 tree and `/object_info`. Node names, inputs, main spandrel loader, image-descriptor requirement, original scale and quality-factor absence follow that implementation.
- [ComfyUI model-path setup](https://github.com/comfyanonymous/ComfyUI/blob/master/main.py), [folder_paths](https://github.com/comfyanonymous/ComfyUI/blob/master/folder_paths.py), [extra model paths](https://github.com/comfyanonymous/ComfyUI/blob/master/utils/extra_config.py): the read-only probe uses those installed core modules and the running server's arguments, without loading custom nodes.
- [spandrel support table](https://github.com/chaiNNer-org/spandrel), [main registry](https://github.com/chaiNNer-org/spandrel/blob/main/libs/spandrel/spandrel/__helpers/main_registry.py), [loader](https://github.com/chaiNNer-org/spandrel/blob/main/libs/spandrel/spandrel/__helpers/loader.py): actual architecture detection and refusal policy; DDColor's extra-only status. Upstream `tests/test_FBCNN.py`, `test_SCUNet.py` and `test_NAFNet.py` confirm core descriptor usage and official weight provenance.
- [Official FBCNN](https://github.com/jiaxi-jiang/FBCNN), [release v1.0](https://github.com/jiaxi-jiang/FBCNN/releases/tag/v1.0), [licence](https://github.com/jiaxi-jiang/FBCNN/blob/main/LICENSE).
- [Official SCUNet](https://github.com/cszn/SCUNet), [author's download script](https://github.com/cszn/SCUNet/blob/main/main_download_pretrained_models.py) (KAIR release source and PSNR/GAN distinction), [licence](https://github.com/cszn/SCUNet/blob/main/LICENSE).
- [Official NAFNet readme](https://github.com/megvii-research/NAFNet/blob/main/readme.md): SIDD variant, MIT and the explicitly linked [author's official HF Space](https://huggingface.co/spaces/chuxiaojie/NAFNet/tree/5964ed4955416df99210106b708e4a2df9e9eca0), whose LFS metadata gives the pinned width-64 checkpoint hash.
- [Official DDColor HF card](https://huggingface.co/piddnad/DDColor-models/tree/e9e7b527709c8aeb2f5e1bf701e72fd468a13baa): Apache-2.0 and the pinned model metadata. This establishes its licence only; architecture eligibility is checked separately.
- [Microsoft detector](https://github.com/microsoft/Bringing-Old-Photos-Back-to-Life/blob/master/Global/detection.py), [official download script](https://github.com/microsoft/Bringing-Old-Photos-Back-to-Life/blob/master/download-weights), [MIT licence](https://github.com/microsoft/Bringing-Old-Photos-Back-to-Life/blob/master/LICENSE): scratch-only stage/provenance. The stale Azure URL in the script fails; the [current official README](https://github.com/microsoft/Bringing-Old-Photos-Back-to-Life#installation) instead points to the reachable [GitHub release](https://github.com/microsoft/Bringing-Old-Photos-Back-to-Life/releases/tag/v1.0). Bounded archive metadata is recorded in `target/restoration-sources/microsoft-archive-inventory.json`. Face stages are excluded.

- BYO licence and community-node primary sources are listed and linked in [BYO-MODELS.md](../BYO-MODELS.md); the owner review supersedes the original research brief’s BYO exclusions. Workflow contracts were inspected from the official installed-node implementations/examples, not installed or ported.

The narrow PTH exception, immutable policy boundary and stale-preview UI are Local Image integration decisions; there is no upstream drop-in Rust restoration dialog. No external code was ported into the app, so no new code-port row is needed in `docs/PORTS.md`.
