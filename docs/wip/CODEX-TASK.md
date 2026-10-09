# Codex task: develop engine — detail tools (item 10) + remove AI Denoise

You are working in a git worktree of "Local Image", a GPL-3.0-or-later, **all-Rust** raw photo editor
(Library/Develop: crates/lc-*; Compositing: crates/pc-*; app: apps/local-image). Read `docs/wip/engine-detail.md`
first: the previous builder's progress note for this task (CPU sharpening done in `crates/lc-pipeline/src/detail.rs`,
interim NR/dehaze to replace, tests written but unregistered, lc-gpu callers broken). The branch was just merged with
the finished colour & tone and raw-quality work (see `docs/wip/CODEX-REPORT-colour-tone.md` and `CODEX-REPORT-raw.md`
for what changed: one renderer, no Process Version). Also read `docs/PORTS.md`, `licenses/darktable-NOTICE.md`,
`docs/GPU-VALIDATION.md`.

Goal (owner): Lightroom Classic / Capture One quality for the Detail tools; faithful darktable ports where they're used.

## Work

1. **One renderer — remove Process Version.** The merge left this branch's `ProcessVersion` / `v2026()` code behind
   (the rest of the codebase no longer has it). Remove the enum, field, helpers and every `v2026()` branch: the NEW
   detail tools are the only behaviour. Old settings JSON must still load (serde defaults; an old `process` field is
   ignored). Re-record golden hashes (lc-pipeline `tests_toolset.rs`, `tests_layers.rs`; lc-engine `tests_toolset.rs`)
   where the output changes intentionally, and say why.
2. **Sharpening** (already on the CPU in `detail.rs`): pixel-scale USM on log luminance with overshoot clamping to the
   local min/max (C1-style halo suppression), Radius / Amount / Detail / Masking all working, Masking preview overlay.
   Finish it, register `tests_detail.rs`, and port it to the GPU.
3. **Noise reduction — faithful darktable port** of `src/iop/denoiseprofile.c` wavelet mode (its Y0U0V0 colour mode
   and newer variance-stabilising transform) + `src/common/eaw.c` edge-aware decompose/synthesize (+ `fast_mexp2f`).
   No camera noise-profile database: estimate the noise model from the image (or use darktable's generic profile) and
   document the slider mapping (Luminance Amount / Detail / Contrast, Colour Amount / Detail / Smoothness). Edge-aware
   chroma (no colour bleeding across edges); NR Contrast actually works.
4. **Dehaze — faithful port** of darktable `src/iop/hazeremoval.c` + `src/common/guided_filter.c` + `box_filters.cc`
   (dark channel, guided transmission refinement, airlight), scene-linear; no halos at strong edges.
5. **Remove the AI Denoise feature entirely** (owner dropped it): li-seg `denoise.rs` and its model entry in
   `li_seg::MODELS`, lc-engine `enhance/denoise.rs` + its commands/session/UI (Develop › Detail AI Denoise controls),
   `DevelopSettings.enhance.denoise` / `enhance.ai` rendering (keep old JSON loadable: unknown/old fields ignored),
   tests that use it (e.g. `tests_enhance::denoise_through_the_commands`, the AI Denoise part of
   `lc-engine/src/tests_gpu.rs`, `lc-gpu/tests/toolset.rs::ai_denoised_sources_render_on_the_gpu`), and
   `licenses/darktable-ai-NOTICE.md` + its `docs/PORTS.md` row. AI Remove stays.
6. **Reference vectors**: gcc is available; extract upstream C functions into tiny standalone harnesses under
   `target/refvec/<name>/` (inside this worktree, ignored by git), run on deterministic inputs, commit only small
   fixtures under the crate's `tests/fixtures/` + README with regeneration steps, assert matches within stated
   tolerances. Any deviation must be equal-or-better and documented.
7. **GPU twins** for every pixel stage (crates/lc-gpu WGSL + host) and GPU-vs-CPU cases in
   `crates/lc-gpu/tests/equivalence.rs` / `toolset.rs` (mean |Δ| < 0.5 LSB, max ≤ 3 LSB). If a stage can't be ported
   now, make it a CPU stage inside the GPU render (not a whole-render fallback). Add 24 MP timings to the ignored bench
   in `toolset.rs`.
8. Map Lightroom XMP (`crs:Sharpness`, `SharpenRadius`, `SharpenDetail`, `SharpenEdgeMasking`, `LuminanceSmoothing`,
   `LuminanceNoiseReductionDetail`, `LuminanceNoiseReductionContrast`, `ColorNoiseReduction`,
   `ColorNoiseReductionDetail`, `ColorNoiseReductionSmoothness`) in lc-engine `crs.rs`. Update `docs/PORTS.md` and the
   darktable notice.

## Reference material (read-only, outside the worktree)

`/home/zdavidson/.local/share/local-image-dev/engine-sources/`: `dt/` (darktable at
733bd69f32cac7ff5e41025115942772add1f088), `dt-src/` (denoiseprofile.c, eaw.c, hazeremoval.c, guided_filter.c,
box_filters.cc, math.h — fetched for this task), `refvec/`.

## Rules

- **Pure Rust**, workspace denies `unsafe`, no new dependencies (C only in out-of-build refvec harnesses).
- Offline builds (`--offline`); ONE cargo build at a time (the PC has 60 GB RAM); `PATH=~/.cargo/bin:$PATH`, use
  `cargo +1.98.1`. Format: `cargo +1.98.1 fmt -p <crate>`; lint: `CARGO_TARGET_DIR=target/clippy cargo +1.98.1 clippy
  -p <crate> --all-targets -- -D warnings`.
- Tests that must pass at the end: `cargo +1.98.1 test --offline -p lightcraft-develop -p lightcraft-pipeline
  -p lightcraft-gpu -p lightcraft-engine -p lightcraft-ui-egui -p li-seg`.
- Your sandbox has **no GPU adapter**: GPU tests print "skipped: no GPU adapter" and pass without running. Still write
  the WGSL and the GPU-vs-CPU cases; list them in the report — the coordinator runs them on the real RTX 5090.
- Don't touch: colorops.rs, masks.rs, Highlights/Shadows/Clarity/Texture code (another job), lc-raw demosaic/highlight,
  crates/pc-*. A Local Image app may be running: don't kill it or open GUI windows.
- **Do not commit.** Leave changes in the working tree. When done, write `docs/wip/CODEX-REPORT-detail.md`: what's done
  per point, tests with counts, fidelity numbers, golden hashes re-recorded (and why), GPU tests added, timings, gaps.
