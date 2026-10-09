# darktable-ai and RawNIND: AI Denoise model and notices

Local Image is GPL-3.0-or-later (see `LICENSE`). Its **AI Denoise** (Develop › Detail) runs the
RawNIND UtNet2 denoiser, downloaded on request from the **darktable-ai** project's model release,
and tiles the image the way darktable-ai's reference script does. Nothing from either project is
compiled into Local Image; the model is a separate download (not shipped), and the tiling was
re-implemented in Rust. The project-wide list of ports is [`docs/PORTS.md`](../docs/PORTS.md).

## Code ported

**darktable-ai** — <https://github.com/darktable-org/darktable-ai>
Copyright (C) the darktable developers. GPL-3.0 (`license = "GPL-3.0"` in its `pyproject.toml`;
the repository's `LICENSE` is the GNU GPL version 3).

| Our file | Upstream file | Upstream commit | Licence |
|---|---|---|---|
| `crates/li-seg/src/denoise.rs` | [`models/rawdenoise-nind/demo.py`](https://github.com/darktable-org/darktable-ai/blob/6bcd41c6f296ca692e6f845b25cf7cdb8148305c/models/rawdenoise-nind/demo.py) | `6bcd41c6f296ca692e6f845b25cf7cdb8148305c` (tag `release-5.6.0`) | GPL-3.0 |

What was ported: the inference pipeline of the linear variant — the input contract (linear
Rec.2020, camera white balance, NCHW `[1, 3, H, W]`, sides divisible by 16), mirror-padded
overlapping tiles, and the scalar gain that matches the output's mean to the input's (sign
preserved). What differs: tiles are 256 px (the static 512 px graph is re-declared at 256), they
are blended with feathered weights instead of keeping each tile's core, and the gain is matched
once over the whole image.

## Model (downloaded, not shipped)

**RawNIND UtNet2, linear Rec.2020 variant** (`model_linear.onnx` in darktable-ai's
`rawdenoise-nind.dtmodel`, release `release-5.6.0`)
— <https://github.com/darktable-org/darktable-ai/releases/download/release-5.6.0/rawdenoise-nind.dtmodel>

* Package: 57,700,134 bytes, SHA-256 `d71b5f1e727c85a359e6f74dca9e2016c9d8fc3e2f7ac3e9b347d80ceca969af`.
* Model inside it (`rawdenoise-nind/model_linear.onnx`): 31,053,823 bytes, SHA-256
  `df957efadcc152c007d5d3b0917bdff9e41c0d4a0efe56584ef30b36393cd181`. Both are checked when the
  model is installed (`li_seg::MODELS`, `li_seg::denoise::install_package`).
* Model and training code: Benoit Brummer and Christophe De Vleeschouwer, *Learning Joint
  Denoising, Demosaicing, and Compression from the Raw Natural Image Noise Dataset* (2025),
  <https://arxiv.org/abs/2501.08924>; code <https://github.com/trougnouf/rawnind_jddc>, GPL-3.0.
  Checkpoint `DenoiserTrainingProfiledRGBToProfiledRGB_3ch_2024-10-09-prgb_ms-ssim_mgout_notrans_valeither_-1`,
  exported to ONNX by darktable-ai.
* Training data: RawNIND (UCLouvain Dataverse; images on Wikimedia Commons), CC BY 4.0 / CC0 per
  image.

The GPL-3.0 model is compatible with Local Image's GPL-3.0-or-later.
