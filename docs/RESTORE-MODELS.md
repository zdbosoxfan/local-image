# On-device Restore models

Colourise and Repair old photo run in Rust through tract, independently of ComfyUI.
AI Remove still uses the configured local ComfyUI for the confirmed fill.
The dialog contains exactly Fix JPEG artefacts, Clean up scan, Colourise and Repair old photo.
Denoise remains in its dedicated tool.

The two approved exports are produced locally. No export of the author's exact relicensed
tiny checkpoint with a verified provenance was found. Qualcomm's deployment licence and
unproven third-party conversions are not used. Local Image accepts only these exact ONNX
bytes; it never runs Python to prepare or execute them in the app.

| Export | Bytes | SHA-256 |
|---|---:|---|
| `ddcolor-tiny.onnx` | 220552826 | `56e33562efde1d866a7a804b11f07c024037904dbaba85611c86a1c6fd5b5cc5` |
| `microsoft-scratches.onnx` | 150600682 | `14036ea1fa86b748973b992ab734d86c7acd3b524cab8d3a198dd63d9fa06f9b` |

Prepared artifacts for this job are in `target/restore-export/`. Choose Colourise or Repair
old photo in Restore, click **Import Restore model…**, and select the corresponding file.
The worker checks size and SHA, copies to `<AI model directory>/restoration/` and verifies
the copy before atomic publication. The selected source stays intact. Preview is enabled
when the model is present; inference rechecks its SHA. Models are not silently installed
into the owner's model directory by development tests.

## Provenance and repeatable export

The approved DDColor weights are [the author's Apache-2.0 Hugging Face checkpoint](https://huggingface.co/piddnad/DDColor-models/blob/e9e7b527709c8aeb2f5e1bf701e72fd468a13baa/ddcolor_paper_tiny.pth),
220393145 bytes, SHA `81dd643904f4664c3718513e3320ae3db0c567f0d3e18398606659adee7bfc17`.
Use no earlier non-commercial copy, even with an identical basename.
The architecture/export reference is [official DDColor](https://github.com/piddnad/DDColor/tree/2adb63f2656ac41cbdf7b894cddd94121a3faf13), Apache-2.0.

Microsoft's [official v1.0 release](https://github.com/microsoft/Bringing-Old-Photos-Back-to-Life/releases/tag/v1.0)
contains `global_checkpoints.zip` (2036400762 bytes). Only
`checkpoints/detection/FT_Epoch_latest.pt` is extracted: 451663663 bytes,
SHA `b2d7ab04e9b3885c6b1991bb7a0b823129dd6e3ac078a9fd059ebd2a7ba59a95`.
Microsoft explicitly [licenses code and pretrained models under MIT](https://github.com/microsoft/Bringing-Old-Photos-Back-to-Life/blob/33875eccf4ebcd3665cf38cc56f3a0ce563d3a9c/README.md#license).
Neither dlib nor a face-stage checkpoint is obtained or executed.

Reference environment: Python 3.12.15; torch 2.14.1+cu130; torchvision 0.29.1+cu130;
ONNX 1.16.1; NumPy 2.5.2; timm 1.0.30; protobuf 7.36.2. Export is on CPU with three
threads. ONNX Runtime 1.19.2, OpenCV headless 5.0.0.93 and Pillow 12.3.0 are reference
validation tools only, never build dependencies. The owner's existing torch environment
was used read-only through a `.pth` file in the throwaway venv; newly installed packages
were written only to that venv. Use an equivalent reference environment for reproduction.

From the worktree root (all generated files and the venv stay under `target/`):

```bash
mkdir -p target/restore-export
# Replay this job's environment on this machine, borrowing installed reference
# packages read-only. On another machine supply the versions listed above.
/home/zdavidson/.local/share/comfyui/venv/bin/python -m venv --clear target/restore-export/venv
cat > target/restore-export/venv/lib/python3.12/site-packages/reference-only.pth <<'EOF'
/home/zdavidson/.local/share/comfyui/venv/lib64/python3.12/site-packages
/home/zdavidson/.local/share/comfyui/venv/lib/python3.12/site-packages
EOF
target/restore-export/venv/bin/python -m pip install --no-deps \
  onnx==1.16.1 timm==1.0.30 onnxruntime==1.19.2 \
  coloredlogs==15.0.1 humanfriendly==10.0

git clone https://github.com/piddnad/DDColor.git target/restore-export/DDColor
git -C target/restore-export/DDColor checkout 2adb63f2656ac41cbdf7b894cddd94121a3faf13
git clone https://github.com/microsoft/Bringing-Old-Photos-Back-to-Life.git target/restore-export/old-photos
git -C target/restore-export/old-photos checkout 33875eccf4ebcd3665cf38cc56f3a0ce563d3a9c
git clone https://github.com/vacancy/Synchronized-BatchNorm-PyTorch.git target/restore-export/sync-bn
git -C target/restore-export/sync-bn checkout 7553990fb9a917cddd9342e89b6dc12a70573f5b
cp -r target/restore-export/sync-bn/sync_batchnorm target/restore-export/old-photos/Global/detection_models/

curl --fail --location 'https://huggingface.co/piddnad/DDColor-models/resolve/e9e7b527709c8aeb2f5e1bf701e72fd468a13baa/ddcolor_paper_tiny.pth' \
  --output target/restore-export/ddcolor_paper_tiny.pth
# Range fetch extracts just the detector; skips all other archive entries.
target/restore-export/venv/bin/python scripts/fetch_restore_scratch.py

target/restore-export/venv/bin/python scripts/export_restore_models.py ddcolor \
  --source target/restore-export/DDColor \
  --checkpoint target/restore-export/ddcolor_paper_tiny.pth \
  --output target/restore-export/ddcolor-tiny.onnx
target/restore-export/venv/bin/python scripts/export_restore_models.py scratches \
  --source target/restore-export/old-photos \
  --checkpoint target/restore-export/FT_Epoch_latest.pt \
  --output target/restore-export/microsoft-scratches.onnx
```

The checkpoint hashes are enforced in the export script. DDColor's two unused classifier
head keys are discarded; every inference parameter then loads strictly. Eval-mode spectral
normalisation is frozen with torch's `remove_spectral_norm`, and equality with the original
forward pass is checked before export. This eliminates constant vector-dot MatMul unsupported
by tract, without changing inference. Both graphs use FP32, opset 17, embedded weights,
and no dynamic quantization. DDColor is static 512×512; scratch spatial axes are dynamic.
The installed Rust reader is tract 0.23.8. The public reproduction script produced byte-for-byte
identical exports, including both final hashes.

## Pixel contracts and review

[DDColor's pipeline](https://github.com/piddnad/DDColor/blob/2adb63f2656ac41cbdf7b894cddd94121a3faf13/ddcolor/pipeline.py)
resizes RGB, takes Lab L, forms neutral RGB and predicts the two chroma channels. The
Rust port uses D65 sRGB/Lab math, bilinear RGB downsampling and nearest chroma upsampling,
then restores original-size luminance and every alpha byte. Float OpenCV reference
vectors cover the conversion; its forward LUT approximation accounts for small input
conversion differences (under 0.004 normalized RGB in the sampled vectors).
The matrices, D65 white point and gamma branches follow
[OpenCV's colour conversion reference](https://docs.opencv.org/4.x/de/d25/imgproc_color_conversions.html#color_convert_rgb_lab).

[Microsoft's detector](https://github.com/microsoft/Bringing-Old-Photos-Back-to-Life/blob/33875eccf4ebcd3665cf38cc56f3a0ce563d3a9c/Global/detection.py)
uses grayscale [-1,1], shortest side 256, bilinear interpolation, sigmoid and threshold 0.4.
Rust resizes directly from the original to that rounded multiple-of-16 detector size rather
than first resizing the source document to multiples of 16. This preserves the document
geometry. Nearest resampling maps detections back to exact original dimensions, and fully
transparent pixels are excluded. Aspect ratios above 8:1 return a clear error before
allocating an excessive model shape. The app does not substitute a heuristic for detection.

The preview shows scratches in red. **Review scratches in AI Remove** creates a transient
mask, including when "Remove immediately on release" is on. Paint or subtract as usual,
then explicitly click Remove or press Enter. The existing fill produces its normal new
layer and undo step. Cancel leaves the document untouched. Optional **Colourise after
repair** runs a fresh Colourise preview only after a successful fill, for the same active
document; the resulting preview still needs Apply. Failed/cancelled fills cannot trigger it.

## Development verification

NASA's astronaut test image is public domain, as documented by
[scikit-image](https://scikit-image.org/docs/stable/api/skimage.data.html#skimage.data.astronaut).
Copy its `astronaut.png` into `target/restore-export/` for these opt-in tests.
No owner photos are used. Synthetic white scratches are added explicitly; visual results
and timing evidence are written to that same worktree directory.

```bash
PATH="$HOME/.cargo/bin:$PATH" CARGO_BUILD_JOBS=3 \
LOCAL_IMAGE_RESTORE_EXPORTS="$PWD/target/restore-export" \
cargo +1.98.1 test --offline -p li-seg restoration_real_cpu_colourise_and_scratch_mask -- --ignored --nocapture

# First ensure /queue is empty on the selected local server. This test clicks the
# real confirmation and submits one existing AI Remove fill; no GUI window opens.
PATH="$HOME/.cargo/bin:$PATH" CARGO_BUILD_JOBS=3 \
LOCAL_IMAGE_RESTORE_EXPORTS="$PWD/target/restore-export" \
LOCAL_IMAGE_RESTORATION_COMFY=127.0.0.1:8195 \
cargo +1.98.1 test --offline -p photocraft-ui-egui restoration_ui_local_real_5090_colourise_and_repair -- --ignored --nocapture
```

The GPU test requires an actual RTX 5090 adapter and server device. It fails on a CPU
adapter and checks actual Comfy model/node readiness before confirming the fill. Neural
Colourise/detection inference runs on the CPU; the UI uses the GPU, and the existing
confirmed AI Remove uses the GPU. Timing claims distinguish those stages.
