# Restore completion report — 2026-10-10

Completed on branch `wip/codex-restore2`, continuing the prior builder's Restore dialog,
model profiles/browser, BYO controls, preview/apply transaction and AI Remove confirmation.
No commits. All app inference remains Rust; no Cargo dependency or lockfile changes,
new C/C++/`-sys` crates, `unsafe`, Python runtime integration or non-commercial architecture.

## Task results

1. **Colourise:** the author's exact approved Apache DDColor tiny checkpoint is exported
   to pinned FP32 ONNX and executes through existing tract 0.23.8 in `li-seg`.
   Original Lab luminance, image dimensions and every alpha byte are retained. A worker
   produces the existing before/after preview; Apply uses the existing new-layer/undo
   transaction. Source revisions, task changes and Cancel discard stale results.
2. **Repair old photo:** Microsoft's official MIT one-channel detector executes through
   tract. Its mask is previewed in red and handed to the existing transient AI Remove
   mask. Importing detection never submits a fill, including with immediate removal
   enabled. The mask can be painted/subtracted, cancelled or explicitly confirmed.
   Existing AI Remove produces its normal fill layer/undo step. **Colourise after repair**
   automatically runs a fresh local preview after a successful fill for the same active
   document; Apply remains manual. Failed/cancelled fills and a different active document
   do not trigger it.
3. **Curated tasks:** the dialog now contains exactly **Fix JPEG artefacts · Clean up
   scan · Colourise · Repair old photo**. The dedicated Denoise tool and serialized
   legacy enum are preserved; no settings/document schema changed. JPEG/scan retain
   the previous working core-Comfy paths. BYO remains available as before.
4. **Verification:** actual CPU inference, actual RTX 5090 Vulkan rendering and actual
   RTX 5090 Comfy fill were run on public-domain NASA imagery. Click/drag tests, numeric
   reference comparisons and visual inspection are recorded below.

Main code: `crates/li-seg/src/restoration.rs`, `crates/pc-ui-egui/src/restoration_ui.rs`,
`ai_remove_ui.rs`, `jobs_ui.rs` and their tests. Model import reuses the existing ONNX
picker. A cheap file-size check gates Preview; workers verify SHA even before cached
model reuse. Imports verify the source and copied bytes, publish atomically and preserve
the source/previous valid installation on error. Import cancellation is checked while
copying and before publication. Each model caches one runnable shape, releasing a prior
shape before loading another.

## Approved model provenance

| Artifact | Bytes | SHA-256 |
|---|---:|---|
| HF `ddcolor_paper_tiny.pth` | 220393145 | `81dd643904f4664c3718513e3320ae3db0c567f0d3e18398606659adee7bfc17` |
| `ddcolor-tiny.onnx` | 220552826 | `56e33562efde1d866a7a804b11f07c024037904dbaba85611c86a1c6fd5b5cc5` |
| Microsoft `FT_Epoch_latest.pt` | 451663663 | `b2d7ab04e9b3885c6b1991bb7a0b823129dd6e3ac078a9fd059ebd2a7ba59a95` |
| `microsoft-scratches.onnx` | 150600682 | `14036ea1fa86b748973b992ab734d86c7acd3b524cab8d3a198dd63d9fa06f9b` |

DDColor weights: [author's Apache HF revision
e9e7b527709c8aeb2f5e1bf701e72fd468a13baa](https://huggingface.co/piddnad/DDColor-models/blob/e9e7b527709c8aeb2f5e1bf701e72fd468a13baa/ddcolor_paper_tiny.pth).
The already approved cached checkpoint was read and hash-checked; it was not modified.
Architecture/export code: [official DDColor commit
2adb63f2656ac41cbdf7b894cddd94121a3faf13](https://github.com/piddnad/DDColor/tree/2adb63f2656ac41cbdf7b894cddd94121a3faf13).
No existing export of these exact relicensed weights with sufficient provenance was
found. Qualcomm deployment and unproven third-party conversions were not used.

Scratch code: [Microsoft commit
33875eccf4ebcd3665cf38cc56f3a0ce563d3a9c](https://github.com/microsoft/Bringing-Old-Photos-Back-to-Life/tree/33875eccf4ebcd3665cf38cc56f3a0ce563d3a9c).
The [official v1.0 ZIP](https://github.com/microsoft/Bringing-Old-Photos-Back-to-Life/releases/download/v1.0/global_checkpoints.zip)
is 2036400762 bytes. HTTP ranges/ZIP extraction obtained only
`checkpoints/detection/FT_Epoch_latest.pt`, with ZIP CRC validation and the checkpoint
SHA enforced by the exporter. Microsoft explicitly [licenses code and pretrained
models under MIT](https://github.com/microsoft/Bringing-Old-Photos-Back-to-Life/blob/33875eccf4ebcd3665cf38cc56f3a0ce563d3a9c/README.md#license).
No dlib, face-stage weights or Microsoft's other repair networks were used. The upstream
Python import needs the sync-BN module from vacancy/Synchronized-BatchNorm-PyTorch
commit `7553990fb9a917cddd9342e89b6dc12a70573f5b`; it is copied only into the throwaway
checkout, and the actual network uses `sync_bn=False`.

`scripts/export_restore_models.py` and `scripts/fetch_restore_scratch.py` are reference
tools outside the Rust build. The app never calls them. Exports are FP32, embedded weights,
opset 17, without dynamic quantization. DDColor is static 512×512; scratch spatial axes
are dynamic. The first DDColor graph contained constant vector-vector MatMul from
spectral normalization, which tract does not accept. Freezing eval-mode spectral weights
with torch's official `remove_spectral_norm` removes that operation; the exporter asserts
forward equality within 1e-5 before export. Only the two unused classifier-head keys are
discarded; all inference parameters load strictly. Both final graphs load/run in tract,
including DDColor attention/LayerNorm/Einsum and the scratch antialias/Resize operations.
No WGSL model port was necessary.

### Environment and exact export commands

The throwaway venv is `target/restore-export/venv`, Python **3.12.15**. It was created with
`/home/zdavidson/.local/share/comfyui/venv/bin/python -m venv --clear target/restore-export/venv`.
`reference-only.pth` in that venv points to the existing Comfy venv's `lib64` and `lib`
Python 3.12 site-packages for read-only reference use. Newly installed packages belong
to the throwaway venv. Environment: torch **2.14.1+cu130**, torchvision **0.29.1+cu130**,
ONNX **1.16.1**, NumPy **2.5.2**, timm **1.0.30**, protobuf **7.36.2**.
Reference validation also uses ONNX Runtime **1.19.2**, OpenCV headless **5.0.0.93** and
Pillow **12.3.0**. The exact pinned venv replay and upstream checkout commands are in
[`docs/RESTORE-MODELS.md`](../RESTORE-MODELS.md). The final pinned package check was:

```bash
target/restore-export/venv/bin/python -m pip install --no-deps \
  onnx==1.16.1 timm==1.0.30 onnxruntime==1.19.2 \
  coloredlogs==15.0.1 humanfriendly==10.0
```

Initial exports used `target/restore-export/export.py` and `export_scratch.py`; the
checked-in reproducible exporter was then run against the same checkpoints:

```bash
target/restore-export/venv/bin/python scripts/export_restore_models.py ddcolor \
  --source target/restore-export/DDColor \
  --checkpoint /home/zdavidson/.cache/local-image-test-models/restoration/upscale_models/ddcolor_paper_tiny.pth \
  --output target/restore-export/reproduced/ddcolor-tiny.onnx
target/restore-export/venv/bin/python scripts/export_restore_models.py scratches \
  --source target/restore-export/old-photos \
  --checkpoint target/restore-export/FT_Epoch_latest.pt \
  --output target/restore-export/reproduced/microsoft-scratches.onnx
sha256sum target/restore-export/reproduced/*.onnx \
  target/restore-export/ddcolor-tiny.onnx target/restore-export/microsoft-scratches.onnx
```

Both reproductions are byte-for-byte identical to the installed pins. Logs:
`target/restore-export/{ddcolor-export,scratch-export,reproduce,reproduce-scratch,venv-pins}.log`.
Torch legacy-export warnings concern fixed channel conditions/constant Slice folding;
both ONNX checker and actual tract inference pass. The optional upstream `onnxsim`
install had no wheel and wanted a CMake build, so it was abandoned; no such build or
runtime dependency was added.

## Pixel/reference validation and observed quality

Followed [DDColor's pipeline](https://github.com/piddnad/DDColor/blob/2adb63f2656ac41cbdf7b894cddd94121a3faf13/ddcolor/pipeline.py):
bilinear RGB resize, Lab L to neutral RGB, predict ab, nearest chroma resize and combine
with original L. D65 sRGB matrices/gamma use [OpenCV's documented conversion](https://docs.opencv.org/4.x/de/d25/imgproc_color_conversions.html#color_convert_rgb_lab).
Rust uses float math instead of OpenCV's forward lookup approximation. Independent
OpenCV vectors bound neutral input differences below 0.004 normalized RGB; inverse Lab
vectors agree below 1e-4. On the NASA fixture, final Rust/reference RGB differences were
mean **0.584/255**, maximum **20/255**. This is close visual agreement, not pixel identity.

Followed [Microsoft's detection pipeline](https://github.com/microsoft/Bringing-Old-Photos-Back-to-Life/blob/33875eccf4ebcd3665cf38cc56f3a0ce563d3a9c/Global/detection.py):
PIL grayscale coefficients, [-1,1], shortest side 256, dimensions rounded to 16, bilinear
sampling and probability threshold 0.4. One intentional geometry choice: resize directly
from the original rather than first rounding the document itself to multiples of 16.
Nearest mapping restores the exact original mask dimensions and excludes transparent
pixels. Aspect ratios above 8:1 are rejected before allocating excessive model shapes.

Raw independent PyTorch/tract FP32 tensor comparison:

| Network | Maximum absolute difference | Mean absolute difference |
|---|---:|---:|
| DDColor ab | 0.0003147125 | 0.00005211485 |
| Scratch logits | 0.0000762939 | 0.00001230715 |

Scratch threshold-mask disagreements: **0** on the deterministic reference tensor.
Evidence: `*-input.f32`, `*-reference.f32`, `*-tract.f32`, `numeric-comparison.log` and
`pixel-reference.log` under `target/restore-export/`. Rust probe:
`cargo +1.98.1 run --offline -p li-seg --example restoration_onnx_probe -- <onnx> <channels> <size> <input.f32> <output.f32>`.

The test image is [NASA's public-domain astronaut](https://scikit-image.org/docs/stable/api/skimage.data.html#skimage.data.astronaut),
read from the installed scikit-image fixture and copied to `target/`. No owner photos.
Inspected grayscale, colourised, damaged, detected mask, UI previews, reviewed mask,
repaired and optional-colourised PNGs. DDColor gives plausible skin/red suit colours.
The detector misses much of a synthetic four-pixel line: **81/660** true scratch pixels,
**14** false positives, **95** total detections. The real UI drag adds the missed line to
the review mask; the confirmed fill changes **660/660** scratch pixels, and inspection
shows the line removed in `repaired-colourised-preview.png`. This observed model weakness
is why editable review matters; no heuristic is presented as learned detection.

## Real hardware measurements

CPU: AMD Ryzen 7 **9800X3D**, 8 cores/16 threads. UI: discrete NVIDIA **RTX 5090**,
Vulkan driver **615.71.09**, PCI `0000:01:00.0`. The test rejects a CPU adapter and asserts
the actual 5090 name. Source fixture: **257×193**; scratch tensor **336×256**. Neural
Colourise/detection execute on the CPU; egui rendering and existing AI Remove use the GPU.

| Stage | Measured elapsed |
|---|---:|
| CPU Colourise, three calls including verification/preprocessing | 3866.86 / 3195.96 / 3239.89 ms |
| CPU scratch detection, three calls | 1890.83 / 1796.28 / 1930.08 ms |
| Real UI Colourise preview, latest run | 4069.00 ms |
| Median rendered UI frame while Colourise runs | 7.27 ms |
| Colourise Apply, three UI steps | 6.99 ms |
| Real UI scratch preview | 1980.96 ms |
| Median rendered UI frame while detector runs | 7.26 ms |
| Confirmed existing AI Remove, warm model cache | 2201.43 ms |
| Automatic optional Colourise preview after repair | 3284.46 ms |

An earlier cold run measured 7156 ms Colourise, 4169 ms detection and **39830 ms** fill
including model loading. Do not extrapolate these small-image timings to large documents;
the PC is shared and timings vary. Raw tract probes loaded DDColor/scratch in 655/206 ms
and ran their square tensors in 3377/1373 ms, respectively.

Shared Comfy at `127.0.0.1:8188` was unreachable. A separate headless instance on **8195**
used the existing installation/models read-only, with all writable base/input/output/temp/
user directories under `target/restore-comfy`, database in memory, custom/API nodes and
auto-launch disabled. Comfy **0.38.0**, torch **2.14.1+cu130**, actual CUDA RTX 5090, existing
Klein base 4B/Qwen 4B/Flux2 VAE and existing Remove LoRA; no new fill-model download.
Each run checked the queue and submitted one small 28-step fill. Server arguments/device
are saved in `comfy-system-stats.json`; inference log is `comfy.log`.

## Tests and final checks

All Cargo commands used `PATH="$HOME/.cargo/bin:$PATH"`, `CARGO_BUILD_JOBS=3`,
`+1.98.1`, `--offline` and one Cargo invocation at a time. Changed crates are `li-seg`,
`photocraft-ui-egui`, plus `li-attributions` for generated attribution assets.

New tests cover OpenCV Lab vectors, luminance/alpha, mask geometry/threshold/transparency,
pin tampering, atomic/cancelled import, mask transfer with immediate removal enabled,
review cancellation, one confirmed fill/undo and successful same-document follow-up.
Existing Restore tests now click all four curated captions, drag the split, apply/undo,
reject changed sources and retain BYO/browser behavior. Import rejection clicks the actual
button with the existing test-picker injection instead of opening a native GUI window.

Added opt-in real tests:

- `restoration_real_cpu_colourise_and_scratch_mask`: real pinned imports and three calls
  per model; actual chroma, exact dimensions/alpha, nonempty learned mask. **Passed**.
- `restoration_ui_local_real_5090_colourise_and_repair`: real eframe/wgpu renderer, actual
  menu/task/import/preview/apply clicks, alpha/undo assertions, review with no submitted
  job, actual canvas brush drag, explicit Remove, real Comfy fill, optional CPU Colourise
  preview and final undo. **Passed**, latest run 12.46 s. No fake neural result or llvmpipe.

Final default-suite coverage (one full invocation, then only the failed test and previously
unreached integration/doc targets):

| Target | Passed | Ignored | Result |
|---|---:|---:|---|
| li-attributions unit | 5 | 0 | pass |
| li-seg unit | 65 | 6 | pass |
| photocraft-ui-egui unit | 1026 | 7 | 1025 first pass; one existing test passes filtered retry |
| photocraft-ui-egui GPU integration | 62 | 4 | pass |
| photocraft-ui-egui CPU integration | 14 | 0 | pass |
| All three crates' doctests | 0 | 0 | pass |

**1172 default tests have passed**, plus both opt-in real tests. The full invocation
initially returned 101 because `ai_ui::tests::ds_finding_unload_completion_wakes_idle_ui`
read a process-global notification already consumed by another parallel full-app test.
Its repaint assertion passed; the notification `unwrap` failed. The unchanged test passes
alone (0.21 s). The existing neighbouring removal test already documents this global-note
race. This is reported explicitly; no repeated full suite or test weakening was used.

```bash
cargo +1.98.1 test --offline -p li-seg -p photocraft-ui-egui -p li-attributions
cargo +1.98.1 test --offline -p photocraft-ui-egui ds_finding_unload_completion_wakes_idle_ui -- --nocapture
cargo +1.98.1 test --offline -p photocraft-ui-egui --test gpu --test it
cargo +1.98.1 test --offline -p li-seg -p photocraft-ui-egui -p li-attributions --doc
CARGO_TARGET_DIR=target/clippy cargo +1.98.1 clippy --offline \
  -p li-seg -p photocraft-ui-egui -p li-attributions --all-targets -- -D warnings
```

Strict clippy **passes**. Formatting and `git diff --check` pass. Logs:
`full-tests.log`, `unload-retry.log`, `remaining-integration.log`, `remaining-doctests.log`,
`clippy-final.log`, `seg-real-and-targeted.log`, `ui-real-reviewed.log` under
`target/restore-export/`. The owner's test runner occasionally prints an RSS arithmetic
error after a child exits; Cargo/test results are successful for those runs. The existing
missing craft-fonts build-script warning remains. A pre-existing nested `#[test]` in
`generate_ui.rs` caused strict lint failure; moved it to module scope without changing
assertions or runtime behavior. That test now executes and passed in the full suite.

## Delivery and limits

Use Restore → Colourise / Repair old photo → **Import Restore model…** with
`target/restore-export/ddcolor-tiny.onnx` / `microsoft-scratches.onnx`. No binary models
are committed or silently installed in the owner's model directory. These exact exports
are available locally and reproducible; publication to a hosted distribution is outside
this task. Licence/notice and both attribution assets are updated; provenance is also
recorded in `docs/PORTS.md`.

No requested implementation remains blocked. Remaining practical limits: detector recall
needs user review, scratch aspect ratio is bounded at 8:1, and cancellation cannot preempt
a single tract evaluation (its result is discarded afterward). A hosted export is not
invented as a download source. The separate development Comfy server is stopped after
an idle-queue check; the owner's app/shared server are left alone. No desktop GUI opened,
no source-document edits persisted outside tests, and no commits made.
