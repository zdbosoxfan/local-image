# Hand-verified API graphs

Graphs that rendered correctly on an RTX 5090 with ComfyUI 0.42.5 on 2026-10-24, built by hand
from the official templates' recipes with the maintainer's installed files. The template runtime
must compile the same graphs (node ids, seeds, prompts and file-name prefixes aside):

- `flux3_distilled.json`: FLUX.3 Klein distilled BF16, text to image (16 steps, cfg 1, euler).
- `flux3_base.json`: FLUX.3 Klein base BF16, text to image (50 steps, cfg 3.5).
- `flux3_edit.json`: FLUX.3 Klein distilled, one reference through `ReferenceMultiLatent` on an
  empty latent (the create-with-references shape).
- `moss.json`: MOSS-Image SFT BF16 with `flux2-vae`. This graph had `SkipLayerGuidanceDiTSimple`
  on; the official SFT template has it switched off, so the compiled graph has none.
- `krea.json`: Krea-2 Turbo BF16 (8 steps, cfg 1, zeroed negative).
