# Research: open AI denoise and photo restoration (2026-10-10, Opus research agent; owner chose picks 1–3)

Key sources (verify, cite in reports):
- darktable 5.6 "neural restore": https://www.darktable.org/2026/06/meet-darktable-5.6-ai-tools/ ,
  docs https://docs.darktable.org/usermanual/development/lo/module-reference/utility-modules/shared/neural-restore/ ,
  models + selection policy https://github.com/darktable-org/darktable-ai (GPL-3.0; rejects non-commercial weights and
  undisclosed data). Forum issues (magenta cast from wrong WB metadata; CPU sometimes faster than GPU):
  https://discuss.pixls.us/t/introducing-neural-restore-module-raw-denoise-denoise-and-upscale/57349
- RawNIND UtNet2 `rawdenoise-nind` (Bayer 4-ch → demosaic+denoise 2×, and linear Rec.2020 variant; GPL-3.0 code+weights;
  data CC BY 4.0/CC0; static 512 px tiles, opset 20, gain matching): darktable-ai/models/rawdenoise-nind,
  https://github.com/trougnouf/rawnind_jddc , https://arxiv.org/abs/2501.08924 . We shipped it before in li-seg
  ("AI Denoise · RawNIND UtNet2") and removed it in commit a6f962c6 — reuse that code from git history.
- NIND UNet `denoise-nind` (RGB, GPL-3.0): https://github.com/trougnouf/nind-denoise
- RealPLKSR 2×/4× (MIT; ~7M params; ONNX at https://huggingface.co/darktable-org/upscale-realplksr-onnx ; uses GridSample,
  supported by tract 0.23.8). BSRGAN (Apache-2.0; also cleans noise/JPEG/mild blur): darktable-ai/models/upscale-bsrgan.
- FBCNN JPEG artefacts (Apache-2.0): https://github.com/jiaxi-jiang/FBCNN . SCUNet (Apache-2.0). NAFNet SIDD (MIT):
  https://github.com/megvii-research/NAFNet . DDColor (Apache-2.0 since 2024-01-13 — use the relicensed HF weights
  https://huggingface.co/piddnad/DDColor-models , NOT older copies/spandrel's bundled one marked non-commercial).
  Microsoft "Bringing Old Photos Back to Life" scratch detector (MIT; do NOT use its dlib face stage).
- ComfyUI core nodes "Load Upscale Model" / "Upscale Image (using Model)" load any spandrel-recognised model, incl. 1×
  restoration models: https://github.com/comfyanonymous/ComfyUI/blob/master/comfy_extras/nodes_upscale_model.py ;
  spandrel main package = permissive arches only: https://github.com/chaiNNer-org/spandrel . NEVER install/use
  `spandrel_extra_arches`.
- AVOID (non-commercial or no licence): SUPIR, HYPIR, StableSR, CodeFormer, PromptIR, LED, AutoDIR, Zero-DCE, dlib 68-pt,
  GFPGAN/RestoreFormer (FFHQ data), anything needing spandrel_extra_arches.
- No independent comparison vs Topaz/Lightroom/DxO exists; measure our own results.
