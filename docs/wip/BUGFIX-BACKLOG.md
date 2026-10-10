# Bug-fix backlog (owner, 2026-10-09: "complete coding all of this, then a big bug-fixing run at the end")

Until every planned feature is merged, merges are gated on: compiles, clippy clean, the job's own new tests pass.
Full workspace + GPU (RTX 5090) suites, benchmarks and these items are handled in the final bug-fixing run.

## Known issues
- NVIDIA exit crash: reproduced SIGABRT after 209 Library tests; warm-up/queue barriers repaired, but the driver compiler remains active during Vulkan unload; use `LIGHTCRAFT_GPU=0` (full-suite validation recorded in CODEX-REPORT-backlog.md).
- ComfyUI after quit: fixed on normal quit (17ce7e28, Opus scan); still left running if the app crashes or is killed
  (needs a watchdog); idle model unloading not done.
- PSD v7 nondefault slice alignments: writer follows Adobe SDK terminology and retains legacy read aliases; Photoshop CS6 fixture confirms defaults/color, but a Photoshop-saved nondefault-alignment fixture remains unavailable.
- Translations: complete in all 12 languages for both apps (2026-10-09, DeepSeek). Native-speaker spot checks would
  still help; Camera Raw contextual labels elsewhere (AI panel `tr_ctx` + shared widgets) may double-translate too.
- Vectorizer competitor gates (vtracer/potrace/Inkscape references) — results pending.
- NVIDIA banded-render compiler hang: not reproduced in five RTX 5090 runs, including three with disk caching requested off; compute-off fallback `LIGHTCRAFT_GPU=0` avoids this path; intermittent driver cause remains unknown.
