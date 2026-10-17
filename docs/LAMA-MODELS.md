# On-device AI Remove

The existing `li-ai::catalog::Catalog` exposes on-device descriptors by borrowing
`li-seg::MODELS`. The manager and LaMa service resolve the default through that
catalog; the runner accepts that descriptor. This adds no copied registry or
model-specific model folder. The existing `download_seg_files` transport remains
the only installer for this model.

Preferences → AI → On-device Tools → AI Remove downloads big-LaMa (208 MB) and
verifies its pinned size and SHA-256. Manage offers the existing remove/download
controls. No ComfyUI, Python, external inference runtime, GPU or internet connection
is needed after installation. The model lives in the existing AI model folder's
`segmentation/big-lama-fp32.onnx`; no new settings or catalog format are introduced.

Compositing and Develop offer **Automatic**, **On-device · LaMa**, FLUX.2 Klein,
and both Qwen precisions. Automatic uses LaMa when the background poller reports
ComfyUI disconnected, and Klein when connected. Explicit engine selections persist;
existing saved `klein`/`qwen-*` choices continue to load. Missing LaMa does not submit
or discard a painted mask: the user can open On-device Tools and download it.
Painting, adding/subtracting and confirmation keep the existing workflow. Develop
keeps its patch store, opacity, regeneration, deletion, copy/sync and undo; the
editor keeps its separate repair layer and one-step undo. LaMa is deterministic,
so regenerate with the same mask/model has the same pixels (a new seed has no effect).

## Model and contract

The original [LaMa](https://github.com/advimman/lama) and
[Carve FP32 export](https://huggingface.co/Carve/LaMa-ONNX/blob/c3c0c9e468934d62e79c329e35d82dd09ff8c444/README.md)
are Apache-2.0. The exporter recommends FP32; it replaces Fourier operations with
ordinary ONNX mathematics. Local Image loads the unmodified graph using the existing
tract 0.23.8 runtime. There are no added Rust dependencies or Cargo.lock changes.
License and attribution: `licenses/lama-*` and the app's Attributions list.

- Revision: `c3c0c9e468934d62e79c329e35d82dd09ff8c444`.
- File: `lama_fp32.onnx`, 208,044,816 bytes.
- SHA-256: `1faef5301d78db7dda502fe59966957ec4b79dd64e16f03ed96913c7a4eb68d6`.
- Inputs: float32 `image [1,3,512,512]` RGB / 255 and `mask [1,1,512,512]`
  (1 removes, 0 keeps).
- Output: float32 RGB `[1,3,512,512]`, **0–255**, rounded/clamped to display RGB.

UI readiness reads file metadata only. Downloads use the existing verified atomic
publisher; inference workers check the hash and cache the optimized plan by file
path/size/modification stamp. Removing a model releases the cached plan; a running
job keeps its own reference. Inference checks cancellation between tract nodes,
before/after each context crop and while compositing; no partial patch is published.
The callback API accepts the shared `JobControl` check/progress methods directly
and can use a borrowed `pc-raster::Interrupt` check/progress view without adding
a control type or dependency. A shared inference lane bounds concurrency and memory. Thread count follows the
existing shared Rayon executor; no PC-specific worker count is hard-coded.

## Large photographs

Separate distant holes at empty bands at least 64 pixels wide; at most 32 context
crops, retaining all remaining holes in a larger crop. Around each region use a
square context side `max(512, 2 × max(mask width, mask height) + 128)`, clamped to the
photo. Fit that crop inside 512×512 **without changing aspect ratio** or enlarging
small images; use Lanczos RGB and conservative mask footprint pooling, then
symmetrically reflect-pad. Undo cropping/padding after inference and resize the
repair to source coordinates. Only the original coverage is published. Soft
coverage is applied once by the existing patch/layer compositor.

A connected large object is resized as a whole, so it cannot acquire incompatible
structures across independent tiles. This bounds inference memory on any hardware,
but very large removals lose fine detail. Nearby edits should use small masks with
adequate surrounding texture. The crop policy is an original adaptation for the
fixed export, selected by a measured bake-off.
LaMa can blur/reinvent hidden lines or texture; the available Qwen engine remains
useful when semantic reconstruction is needed.

## Reproduce

Use the worktree's retained target directory and the task's toolchain/build budget.
CC0 source URLs, checksums, original downloads, masks, before/after sheets and
reference scripts are under `target/lama/`. Python/ONNX Runtime reference packages
are isolated in `target/refvec/lama-env`; they are never application dependencies.

```sh
PATH="$HOME/.cargo/bin:$PATH" cargo +1.98.1 run --offline -p li-seg --example lama_eval -- \
  target/lama/lama_fp32.onnx target/lama/eval \
  target/lama/photos/interior.jpg target/lama/photos/exterior.jpg target/lama/photos/portrait.jpg
LOCAL_IMAGE_SEG_MODELS="$PWD/target/lama/models" PATH="$HOME/.cargo/bin:$PATH" \
  cargo +1.98.1 run --offline -p photocraft-engine --example lama_real -- \
  target/lama/photos/interior.jpg target/lama/actual-command
LOCAL_IMAGE_SEG_MODELS="$PWD/target/lama/models" PATH="$HOME/.cargo/bin:$PATH" \
  cargo +1.98.1 test --offline -p photocraft-engine lama_real_rtx5090 -- --ignored --nocapture
PATH="$HOME/.cargo/bin:$PATH" cargo +1.98.1 test --offline -p photocraft-ui-egui \
  lama_model_manager_rtx5090_screens -- --ignored
```

The familiar Add/Subtract → Remove interaction follows the owner's decision and
[Adobe's Lightroom Classic Remove help](https://helpx.adobe.com/ie/lightroom-classic/desktop/process-and-develop-photos/remove-tool.html).
All controls use the existing dropdown, buttons, model rows, tokens and Preferences
shell; no new widget style or colored tool icon is introduced.
