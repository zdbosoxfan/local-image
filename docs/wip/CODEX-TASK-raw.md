# Codex task: raw-processing quality (item 12)

You are working in a git worktree of "Local Image", a GPL-3.0-or-later, **all-Rust** raw photo editor
(raw decode: crates/lc-raw; engine: crates/lc-engine; pipeline: crates/lc-pipeline; UI: crates/lc-ui-egui). Read
`docs/wip/engine-raw.md` first: it is the previous builder's research note for this exact task (findings, ordered next
steps, file locations). Also read `docs/PORTS.md`, `licenses/darktable-NOTICE.md` (section "Develop toolset upgrades"),
`docs/DEVELOP-DESIGN.md` §4.1.

Goal (owner): best raw quality; **faithful ports, no simplifications** — "are we degrading the algorithms with a
rewrite? if we are, prioritize quality".

## Work (in the note's order)

1. **VNG4**: port darktable `src/iop/demosaicing/vng.c` completely (and compare with RawTherapee's full VNG4); use it
   in a new dual demosaic option. Keep the existing "Dual (RCD + bilinear)" decode bit-identical for photos that
   already use it (hide it from the menu unless in use).
2. **AMaZE** (RawTherapee `rtengine/amaze_demosaic_RT.cc`, Emil Martinec, GPL-3+; darktable's copy too) and
   **dual-AMaZE**, Bayer only.
3. **Segmentation-based highlight reconstruction** (darktable `src/iop/hlreconstruct/segbased.c` + `segmentation.h`):
   needs a pre-demosaic hook in both the full-size and the binned-preview path; measure and document the preview
   vs full-size difference.
4. **Reference vectors for every port** (existing: RCD, dual mask, opposed, capture radius + RL deconvolution,
   negadoctor, colorcal adaptation, toneeq solve/correction; new: VNG4, AMaZE, segbased): gcc is available; extract the
   upstream functions into tiny standalone harnesses under `target/refvec/<name>/` (inside this worktree, ignored by
   git), run on deterministic synthetic inputs (small crops, 64–256 px), commit only compact fixtures under each crate's
   `tests/fixtures/` + a README with regeneration steps, assert the Rust port matches within a stated tight tolerance.
   Fix any real bug or quality loss you find; report every measured discrepancy.
5. Wire the new options (`RawOptions` keys, lc-develop `Demosaic::ALL` / `HighlightMode::ALL`, the Develop Raw panel
   labels) with minimal UI edits.
6. Quality metrics (zone plate / Siemens star / colour edges: PSNR, false colour) and 24 MP timings per method;
   update `docs/PORTS.md`, the darktable/RawTherapee notices and DEVELOP-DESIGN §4.1.

## Reference material (read-only, outside the worktree)

`/home/zdavidson/.local/share/local-image-dev/engine-sources/`: `dt/` darktable at 733bd69f32cac7ff5e41025115942772add1f088,
`refvec/upstream/` (darktable + RawTherapee 5f486d3678b34c74ba0c63571c17babe20935019 sources: AMaZE, dual, VNG4, RCD,
segbased, opposed, capture, negadoctor, toneequal, colorcal). Real Sony ILCE-1 test raws may be read from
`/home/zdavidson/Desktop/10-7-26 - SNHU Headshots/*.ARW` — client photos: read-only, never copy them into the repo,
never commit or upload them.

## Rules

- **Pure Rust**, workspace denies `unsafe`, no new C/C++ or `-sys` dependencies (C only in out-of-build refvec harnesses).
- Work offline (`--offline`). Format/lint: `~/.cargo/bin/cargo +1.98.1 fmt -p <crate>` and
  `CARGO_TARGET_DIR=target/clippy ~/.cargo/bin/cargo +1.98.1 clippy -p <crate> --all-targets -- -D warnings`.
- Old edits need not render identically (owner decision), but existing decodes for options photos already use should
  stay bit-identical unless you deliberately improve them — then re-record lc-engine `tests_toolset.rs` goldens and say why.
- Tests that must pass at the end: `cargo test --offline -p lightcraft-raw -p lightcraft-engine -p lightcraft-pipeline
  -p lightcraft-develop -p lightcraft-ui-egui`.
- Don't touch: lc-raw `color.rs` / camera matrices / DCP (another job), lc-pipeline tone/finish/colorops/local/masks,
  lc-gpu, crates/pc-*, crates/li-*.
- A Local Image app may be running on the desktop: don't kill it or open GUI windows.
- **Do not commit.** Leave all changes in the working tree; the coordinator reviews and commits.
- When done, write `docs/wip/CODEX-REPORT.md`: per algorithm the fidelity result vs upstream (numbers), quality
  metrics, 24 MP timings, changes, deviations, anything left.

## Note on the GPU

Your sandbox has no GPU adapter: GPU tests print "skipped: no GPU adapter" and pass without running. Still write the
GPU (WGSL) code and the GPU-vs-CPU test cases, make all CPU tests pass, and list in CODEX-REPORT.md exactly which GPU
tests you added — the coordinator runs them on the real RTX 5090 during review and will send failures back to you.
