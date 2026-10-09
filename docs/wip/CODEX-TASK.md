# Codex task: faster raw decoding (AMaZE, segmentation highlights, VNG4/dual) — bit-identical

Worktree of "Local Image" (raw decode: crates/lc-raw; engine loader: crates/lc-engine files.rs). Read
`docs/wip/CODEX-REPORT-raw.md`: the new faithful ports are correct (reference vectors from darktable/RawTherapee in
`crates/lc-raw/tests/fixtures/`) but slow at 24 MP on a Ryzen 7 9800X3D (8 cores/16 threads):
AMaZE 1288 ms (scalar across tiles), dual AMaZE 1860 ms, full VNG4 353 ms, RCD + VNG-linear + medians 634 ms,
segmentation-based highlights 1067 ms (full CFA). For comparison RCD is 57 ms.

## Work

1. Parallelise and optimise these without changing a single output value: AMaZE tiles in parallel with rayon (already a
   dependency) — each tile has its own scratch buffers and writes a disjoint region (upstream overlaps tiles: keep its
   exact tile/overlap layout and only parallelise what is independent); VNG4 / dual low branch / median passes by rows or
   tiles; segmentation: the expensive per-segment / per-pixel stages (distance transform rows/columns, candidate
   evaluation, smoothing) where results are order-independent. Avoid per-pixel allocation, bounds-check-heavy inner loops
   (use slices/chunks), and redundant full-image passes.
2. **Bit-identical:** every existing fixture/reference-vector test and lc-engine decode golden must pass unchanged; add
   tests that compare the parallel result against a single-threaded run (e.g. a rayon pool of 1 thread) bit-for-bit on
   several sizes/CFA phases, including tile-seam and border cases.
3. Report before/after 24 MP timings with the existing `crates/lc-raw/examples/quality.rs --timings` (release, 8 rayon
   workers) for every method touched.

Tests: `cargo +1.98.1 test --offline -p lightcraft-raw -p lightcraft-engine`.

Report file: `docs/wip/CODEX-REPORT-raw-speed.md`.

## Rules (all Codex jobs)

- "Local Image" is GPL-3.0-or-later and **all Rust**: workspace denies `unsafe`; no new dependencies, no C/C++ or
  `-sys` crates in the build (C only in out-of-build reference harnesses under `target/refvec/`).
- Offline builds (`--offline`). The PC has 60 GB RAM shared with other jobs: ONE cargo build at a time in your job,
  `CARGO_BUILD_JOBS=3`. Use `PATH=~/.cargo/bin:$PATH` and `cargo +1.98.1`. Format: `cargo +1.98.1 fmt -p <crate>`;
  lint: `CARGO_TARGET_DIR=target/clippy cargo +1.98.1 clippy --offline -p <crate> --all-targets -- -D warnings`.
- Run the tests of every crate you change (`cargo +1.98.1 test --offline -p <crate>`); all must pass. Your sandbox has
  **no GPU adapter**: GPU tests print "skipped: no GPU adapter" and pass without running — still write GPU code/tests
  and list them in your report; the coordinator runs them on the real RTX 5090.
- UI is egui 0.36; Compositing UI tests use egui_kittest, Library/Develop uses its own `headless.rs` harness. Add tests
  that actually click/drag the UI you change.
- A Local Image app may be running on the desktop: don't kill it or open GUI windows. Don't touch the owner's files
  outside this worktree (read-only access to reference data is fine).
- Old settings/documents must still load (serde defaults). The owner does not need old edits to render identically.
- **Do not commit.** Leave changes in the working tree. When done, write the report file named in your task: what's
  done per point, tests with counts, GPU tests added, anything left and why.

