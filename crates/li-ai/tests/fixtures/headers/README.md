# Weights-file header fixtures

Trimmed headers of real model files, used by `tests/coverage/identify_coverage.rs`. They hold
tensor names, dtypes, shapes and the `__metadata__` map only, never weights. To keep them small,
each group of tensors that differ only by numeric indexes keeps its lowest and highest index (so
block counts and first-block shapes survive, with their quantisation sidecars), and metadata
strings longer than 512 characters are cut. `_quantization_metadata` keeps the entries of the kept
tensors, and the mixed-precision text encoders (FP4/FP8 mixes) keep every tensor, because their
precision is read from how the formats are distributed.

File names are `<source>__<path in the source, / as ~>.json`:

- `<owner>_<repo>__…`: the Hugging Face repository `<owner>/<repo>`, headers read with HTTP range
  requests from its `main` branch on 2026-10-24.
- `local__…` and `owner__…`: model files installed on the maintainer's machine (public models;
  headers only).

GGUF fixtures carry a `__gguf__` key; the tests turn them back into real GGUF header bytes.
`.safetensors` was dropped from the fixture names; the tests add it back to get the file name a
ComfyUI loader would list.
