# Opus scan: speed and bug pass (2026-10-09, branch `wip/opus-scan`)

The machine: RTX 5090, Ryzen 9800X3D, 60 GB RAM. It was shared with other build jobs (load average
10–46), so wall-clock numbers vary by about ±20 %. Every before/after pair below was measured the
same way: the same binary type, interleaved runs or back-to-back runs. No GUI windows were opened.
The owner's running app was not touched. Client raws were used only for timing (one file copied to
scratch; never viewed or committed).

## Measurements

| Path | Tool | Before | After | Notes |
|---|---|---|---|---|
| **Compositing: toggle layer visibility** (designer layout, 174 layers, 33 stroked shapes) | `layout_bench` (whole app, kittest + GPU canvas, GPU-synced) | 96 ms median | **7.1 ms** | eaa474ee |
| Compositing: opacity change | `layout_bench` | 96 ms | **7.8 ms** | eaa474ee |
| Compositing: Move release (pixel / text layer) | `layout_bench` | 99 / 111 ms | **8.7 / 8.9 ms** | eaa474ee |
| Compositing: Move drag frames, p90 (pixel / text / smart) | `layout_bench` | 100 / 99 / 182 ms | **7.8 / 7.9 / 8.1 ms** | eaa474ee |
| Compositing: Move group drag / release | `layout_bench` | 132 / 164 ms | **19 / 26 ms** | rest → codex task `group-move-shape-rerasterise` |
| Compositing: idle frame, hover, select layer, switch tool | `layout_bench` | 0.7 / 0.7 / 2.1 / 1.3 ms | same | already fine |
| **Develop → Compositing re-develop** (21 MP ARW, no edits) | `bench_develop_open` (new), A/B interleaved | 2.25–2.50 s | **0.80–0.93 s** | 6f580fca; UI thread |
| Reopen same photo in Compositing | `bench_develop_open` | 1.8 s | **0.7 s** | 6f580fca |
| Library → Compositing first open (21 MP ARW, no edits) | `bench_develop_open` | 1.9 s | 1.9 s | UI thread → codex task `develop-layer-off-ui-thread` |
| Library → Compositing with typical edits (NR, clarity, dehaze…) | `bench_develop_open --typical` | **9.2–10.2 s** | not fixed | CPU pipeline on the UI thread → codex task |
| Re-develop with typical edits | `bench_develop_open --typical` | **≈ 9.3 s** | not fixed | same |
| Develop-layer peak RSS (open + sync) | `/usr/bin/time` | 1.99 GB | 1.44 GB | 6f580fca |
| Library frames with unsaved Compositing docs (50 k photos, 3 dirty docs) | new test in `library_host.rs` (test profile) | 8.07 ms/frame | **2.39 ms/frame** | aa8a2411 |
| Library grid, unchanged frame (10 k / 85 k photos) | `grid_bench` | 0.20 / 0.21 ms | — | already fine (0 rebuilds) |
| Library thumbnail upload to texture (2560×1707 / 512×341) | temporary micro-bench | 3.5 / 0.06 ms | — | not a hot spot; no change |
| Library loupe render, 21 MP ARW, typical edits | `render_bench` | GPU 40 ms (CPU 1.0 s) | — | GPU path is used in the Library |
| Library export render, full size | `render_bench` | GPU 0.32 s (CPU 10.8 s) + JPEG 0.21 s | — | 16-bit export stays on the CPU (codex task, step 3) |
| Compositing 36 MP: open JPEG / open PSD | `interactive_bench` | 299 / 434 ms | — | background job |
| Compositing 36 MP: brush dab / 40-dab stroke / undo | `interactive_bench` | 0.7 / 23 / 2.0 ms | — | fine |
| Compositing 36 MP: Gaussian Blur r40 / Curves layer preview / Levels tweak | `interactive_bench` | 24 / 0.8 / 5.4 ms | — | fine |
| Compositing 36 MP: save .psd / export PNG / export JPEG | `interactive_bench` | 1.07 / 1.40 / 0.84 s | — | CPU flatten (458 ms) is part of each |
| Compositing 36 MP peak RSS | `interactive_bench` | 3.06 GB | — | |
| App start-up to "GPU startup finished" | owner's log (`local-image.log`) | 0.29 s | — | fine |

Hot spots found by sampling. There was no `perf` or sudo, so I sampled with gdb: I attached to my own
`layout_bench --spin` process and collected main-thread backtraces. The 96–164 ms frames all came
from one place: `pc-gpu/src/plan.rs::effects` → `pc-compose/src/shape_split.rs::split` → re-rasterising
every stroked shape over the whole canvas. The split cache held 32 entries with LRU eviction. A plan
rebuild looks up every stroked shape in turn, so a document with 33 or more of them missed on every
lookup. The combined-mask cache (64 entries) had the same pattern and got the same fix (d916f54a).

## Bugs

| # | Severity | Bug | Evidence | Status |
|---|---|---|---|---|
| 1 | High (memory: 29–46 GB RAM + ~20 GB VRAM after quit) | A ComfyUI started by Local Image kept running after the app quit. Its `Child` sat in a static that is never dropped, and nothing killed it on exit (backlog item). | Code: `ai_ui.rs` `Shared.child`; there was no `on_exit` on `PhotocraftApp` | **Fixed** 17ce7e28. `PhotocraftApp::on_exit` (which the Library host already calls) kills and reaps it. A user-started ComfyUI is never touched. Test `quitting_stops_the_engine_we_started`. |
| 2 | High (UX) | Stroked-shape split cache thrashed at 33+ stroked shapes: ~100 ms frames on every layer edit. | `layout_bench`, gdb samples | **Fixed** eaa474ee. Test fails at the old capacity (96 renders instead of 48). |
| 3 | Medium | The same thrash in the combined-mask cache at 65+ vector-masked or feathered layers. | Code (same pattern) | **Fixed** d916f54a, with a test. |
| 4 | Medium (UX) | A Develop re-develop decoded the raw again and round-tripped flatten/convert every time. | `bench_develop_open` stage timings | **Fixed** 6f580fca. Tests check pixels identical to a fresh decode and to the flatten path. |
| 5 | Low–Medium | `photo_at_path` cloned every catalog path on every Library frame per unsaved document. | New test | **Fixed** aa8a2411, with a test. |
| 6 | High (UX) | Develop layers are developed on the UI thread on the CPU pipeline (up to ~10 s freezes). | `bench_develop_open --typical` | **Handed off**: `codex-tasks/develop-layer-off-ui-thread.md` |
| 7 | Medium (correctness, unverified visually) | `develop_with_settings` ignores the photo's Raw Processing options (it uses `RawOptions::default()`). A Develop layer can differ from Develop for such photos. | Code reading | Handed off (same task, step 4) |
| 8 | Medium (UX) | Moving a group of stroked shapes re-rasterises them every frame (19–26 ms). | `layout_bench`, gdb | **Handed off**: `codex-tasks/group-move-shape-rerasterise.md` |
| 9 | Unknown | The command fuzz (`panic_hunt`) can't safely run on a desktop: some commands spawn programs, print, or write files. | Code reading | **Handed off**: `codex-tasks/panic-hunt-safe-run.md` |

Backlog housekeeping: `BUGFIX-BACKLOG.md` now marks the ComfyUI item fixed. It also notes that no
`#[ignore = "BUG…"]` coverage tests remain. The lc-pipeline and pc-algo items were fixed in 5085dcc9 and
cfb3873c.

## Commits (`wip/opus-scan`)

- 17ce7e28: quitting stops the ComfyUI Local Image started
- 6f580fca: Develop layers: re-develop reuses the decode and skips the flatten round trip (+ `bench_develop_open` example)
- aa8a2411: Library frames: `photo_at_path` borrows catalog paths
- eaa474ee: stroked-shape split cache no longer thrashes past 32 shapes
- d916f54a: combined-mask cache no longer thrashes past 64 masked layers
- (last): `bench_develop_open --typical`, Codex task files, this report, backlog update

## Codex task files

- `docs/wip/codex-tasks/develop-layer-off-ui-thread.md`: biggest remaining stall. Move it to a
  background job, add a 16-bit GPU path, and handle Raw Processing options.
- `docs/wip/codex-tasks/group-move-shape-rerasterise.md`
- `docs/wip/codex-tasks/panic-hunt-safe-run.md`

## Checks run

- Full test suites of the changed crates, each run once: photocraft-compose (185), photocraft-io (421),
  photocraft-engine (1436), photocraft-ui-egui (lib 962 + gpu 54 + it 14, on the 5090; no exit
  segfault this run), local-image (79). All pass.
- `cargo clippy --all-targets -D warnings` (`CARGO_TARGET_DIR=target/clippy`) on those five crates:
  clean. `cargo fmt --check`: clean.

## Left / not done

- A crash or kill of Local Image still leaves its ComfyUI running. A fix needs a process-group or
  parent-death signal, which is not possible without `unsafe`/libc; a watchdog in the ComfyUI launch
  is an option. Idle model unloading is not done.
- NVIDIA driver segfault at test-process exit: it did not reproduce in this pass's pc-ui-egui run.
  It was not investigated further.
- The smart-object source cache (`pc-engine/src/smart_cmds.rs`, 32 entries / 1 GiB) has the same LRU
  shape, but it is only looked up when a smart object is re-rendered, not on every plan rebuild. I
  left it alone.
- Not measured: import throughput and thumbnail generation for raws (the Library side is mostly the
  `fix-develop-speed` job's area), and module switching other than the Develop-layer cost (no host
  frame bench exists; the switch code itself does no other heavy work).
- The Compositing CPU flatten for save/export (458 ms at 36 MP, 3 layers) could use the GPU compositor.
  Saves and exports already run as jobs, so it was not pursued.
