# SigLIP golden references

Generated 2026-10-10 offline with installed Hugging Face transformers 5.18.0,
tokenizers 0.23.2 and Pillow 12.3.0. The tokenizer was the spec-pinned Xenova
`tokenizer.json` (SHA-256 `4a17c975210be5ab4c36b47d8dae4eefb866dbfb1e676e394aad85dc30a3ae08`, Apache-2.0).
`siglip-unigram.json` keeps the original IDs, normalization map and scores for
all pieces that can occur in the golden strings; other pieces are empty placeholders.

Token inputs follow SiglipTokenizer canonicalization (lowercase, remove ASCII
string.punctuation, collapse spaces), then the pinned Unigram tokenizer with
EOS id 1, truncation to 64, EOS padding to 64. Goldens include unknown fusion,
Unicode compatibility/composition and a truncated long prompt.

Pixel inputs are RGB 8×5, 301×199 and 224×300 (upscale, antialiased downscale, one-axis resize): `(31*x+17*y, 7*x+61*y, 43*x+13*y) mod 256`.
`SiglipImageProcessor()` resizes it to 224×224 with Pillow bicubic, rescales by
1/255, normalizes mean/std 0.5 and emits NCHW. Pixel goldens must agree to 1e-6.
No model weights or photos are included in these fixtures.
