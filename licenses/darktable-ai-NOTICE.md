# darktable-ai, RawNIND and NIND

Local Image is GPL-3.0-or-later. AI Denoise uses separately downloaded GPL-3.0 models;
no weights are bundled with the application.

The restored tiling/gain-match code in `crates/li-seg/src/denoise.rs` follows
[darktable-ai's RawNIND demo](https://github.com/darktable-org/darktable-ai/blob/6bcd41c6f296ca692e6f845b25cf7cdb8148305c/models/rawdenoise-nind/demo.py),
commit `6bcd41c6f296ca692e6f845b25cf7cdb8148305c`, tag `release-5.6.0`, checked against
`f1a46e2d3feebc917a94f14705d128316fcf8114` (2026-10-10).
Copyright the darktable developers, GNU GPL version 3. Local Image uses reflected context,
feathered overlapping interiors and one whole-image, sign-preserving scalar gain match.
WGSL graph execution is an independent implementation of ONNX operator semantics.

**RawNIND UtNet2** — Benoit Brummer and Christophe De Vleeschouwer,
[Learning Joint Denoising, Demosaicing, and Compression from the Raw Natural Image Noise Dataset](https://arxiv.org/abs/2501.08924),
2025. [Code and weights](https://github.com/trougnouf/rawnind_jddc): GPL-3.0.
Training data: RawNIND, CC BY 4.0 / CC0 per image (Wikimedia Commons and UCLouvain Dataverse).
Both the Bayer and linear Rec.2020 canonical checkpoints are included in the verified download.

**NIND UNet** — Benoit Brummer and Christophe De Vleeschouwer,
[Natural Image Noise Dataset](https://arxiv.org/abs/1906.00270), CVPR Workshops 2019.
[Code and weights](https://github.com/trougnouf/nind-denoise): GPL-3.0.
Training data: NIND, CC BY 4.0 / CC0 per image (Wikimedia Commons).

The GPL-3.0 models are compatible with Local Image's GPL-3.0-or-later.
Official packages: [darktable-ai release-5.6.0](https://github.com/darktable-org/darktable-ai/releases/tag/release-5.6.0).
Package and extracted-member byte counts and SHA-256 values are pinned in `li_seg::MODELS`.
The downloader checks the archive and the installer checks every extracted weight file.
