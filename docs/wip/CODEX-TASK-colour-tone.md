# Codex task: develop engine — colour & tone (item 9)

You are working in a git worktree of "Local Image", a GPL-3.0-or-later, **all-Rust** raw photo editor
(Library/Develop: crates/lc-*; Compositing: crates/pc-*; app: apps/local-image). Read `docs/wip/engine-colour-tone.md`
first: it is the previous builder's progress note for this exact task (what's done, what's left, file locations).
Also read `docs/DEVELOP-DESIGN.md` §4.1, `docs/PORTS.md`, `licenses/darktable-NOTICE.md`, `docs/GPU-VALIDATION.md`.

Goal (owner): best-in-class colour and tone, with **Lightroom Classic and Capture One as the quality bar**.

## Owner decisions that override the note

1. **No Process Version / Legacy split.** Old edits do NOT need to render as before. Remove the `ProcessVersion`
   enum/field, `v2026()`, the Process UI, `develop.setProcess` and every `if s.v2026()` branch — keep ONE code path:
   the new behaviour. Old settings JSON must still *load* (serde defaults, unknown fields ignored, no crash).
   Re-record golden hashes (lc-pipeline `tests_toolset.rs`, `tests_layers.rs`; lc-engine `tests_toolset.rs`) when the
   behaviour change is intentional; keep them as guards against unintended changes.
2. **Three looks**, chosen per photo (`look`), default look for new photos in Settings: **Adobe-like** (our OWN fitted
   curve — never copy the DNG SDK / ACR3 table), **darktable sigmoid** (faithful port of `src/iop/sigmoid.c`), **Camera**
   (per-photo fit to the embedded JPEG for every raw format, with darktable `basecurve.c` maker/per-camera presets as
   fallback). Base-curve variants Standard / Extra Shadow / High Contrast / Linear; hue-preservation default ~75 %;
   smooth path-to-white; Whites/Blacks roll off, never hard-clip; no vendor trademarks in names.
3. **Camera matrices** from rawler (dnglab v0.8.0, `rawler/data/cameras/**/*.toml`, LGPL-2.1 → GPL-3 via §3),
   dual-illuminant A + D65, generated into a committed Rust table (no rawler dependency); gap-fill from RawTherapee
   `camconst.json` (GPL-3). **Not** RawSpeed cameras.xml (CC BY-SA). DCP loader; bundle only public-domain / CC0
   RawTherapee DCPs (skip "Maciej Dworak" and unclear ones); "Camera profiles folder" setting to READ user DCPs (never
   bundle Adobe's). WB re-derives the camera matrix per chosen white (DNG CCT interpolation).
4. Tone curves hue-preserving with a Luminance/RGB toggle; soft gamut compression; deterministic dithering on 8-bit.
5. **Faithful ports, no simplifications**; any deviation must be equal-or-better and documented. **Reference vectors**:
   gcc is available; extract upstream C/C++ functions into tiny standalone harnesses under `target/refvec/<name>/`
   (inside this worktree, ignored by git), run on deterministic inputs, commit only small fixtures under the crate's
   `tests/fixtures/` + a README with regeneration steps, and assert the Rust port matches within a stated tolerance.
6. **Every pixel stage needs its GPU twin** (crates/lc-gpu WGSL + host) and GPU-vs-CPU cases in
   `crates/lc-gpu/tests/equivalence.rs` / `toolset.rs` (mean |Δ| < 0.5 LSB, max ≤ 3 LSB). This PC has an RTX 5090
   (Vulkan): those tests really run. If a stage can't be ported now, make it a CPU stage *inside* the GPU render, not a
   whole-render fallback.
7. Record every port in `docs/PORTS.md` + `licenses/` notices (authors, upstream path, commit, licence, differences).

## Reference material (read-only, outside the worktree)

`/home/zdavidson/.local/share/local-image-dev/engine-sources/`: `dt/` darktable at 733bd69f32cac7ff5e41025115942772add1f088,
`dnglab/` (rawler camera TOMLs), `dcp/` (RawTherapee DCPs + dump), `pv/curves.py` (curve calibration), `src/`, `refvec/`.

## Rules

- **Pure Rust**, workspace denies `unsafe`, no new C/C++ or `-sys` dependencies (C only in out-of-build refvec harnesses).
- Work offline: `cargo build --offline` / `cargo test --offline` (all deps are cached; adding crates isn't allowed anyway).
- Format/lint with the rustup toolchain: `~/.cargo/bin/cargo +1.98.1 fmt -p <crate>` and
  `CARGO_TARGET_DIR=target/clippy ~/.cargo/bin/cargo +1.98.1 clippy -p <crate> --all-targets -- -D warnings`.
- Tests that must pass at the end: `cargo test --offline -p lightcraft-develop -p lightcraft-pipeline -p lightcraft-gpu
  -p lightcraft-engine -p lightcraft-raw -p lightcraft-ui-egui`. Engine tests needing the GPU take
  `crate::tests_gpu::gpu_state()`.
- Don't touch: lc-raw `demosaic/` and `highlight/` (another job), colorops.rs / Highlights-Shadows-Clarity-Texture /
  masks.rs (another job), crates/pc-*, crates/li-*.
- A Local Image app may be running on the desktop: don't kill it or open GUI windows.
- **Do not commit.** Leave all changes in the working tree; the coordinator reviews and commits.
- When done, write `docs/wip/CODEX-REPORT.md`: what's done per point above, tests run with counts, GPU equivalence
  numbers, golden hashes re-recorded (and why), deviations from upstream, anything left.

## Note on the GPU

Your sandbox has no GPU adapter: GPU tests print "skipped: no GPU adapter" and pass without running. Still write the
GPU (WGSL) code and the GPU-vs-CPU test cases, make all CPU tests pass, and list in CODEX-REPORT.md exactly which GPU
tests you added — the coordinator runs them on the real RTX 5090 during review and will send failures back to you.
