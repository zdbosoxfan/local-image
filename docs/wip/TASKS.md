# Work in progress (2026-10-09)

Owner feedback from testing V2, and the develop-engine upgrade. Each item is a branch
`worktree-agent-<id>` (worktree `.claude/worktrees/agent-<id>`), based on `fbc02a9`, with a WIP
commit that may not compile or pass yet. Plan: finish each → merge all → test the whole
workspace once → `cargo xtask package linux` → reinstall (`local-image --install`).

## UI bug fixes (first)

| # | Item | Branch / WIP commit |
|---|---|---|
| 1 | Develop: AI Remove asks before removing (Remove / Cancel); ghost stroke that blocked healing; heal brush as good as Compositing's (content-aware, stored as a patch) | **Merged** (ca2129a) |
| 2 | Develop: Save Over Original (confirm, backup, raw → JPEG beside + stack); Lightroom-style Export dialog | `acef5313be33a1eb5` 554aa9a |
| 3 | Compositing: Pen bar buttons work (Make Selection / Content-Aware Fill / AI Fill / Mask); live shape options; context menus follow the selected item | `a710220f846256f37` 0ceb78e |
| 4 | Library ↔ Compositing round trip without saving (Library shows the composite, quit warning, new Develop edits → new layer); Save → `<stem>-Edit.psd` stacked | `a78bac644dd1fe189` cce83cf |
| 5 | Models: delete buttons, grouped/explained CPU models, upscaler picker + "Choose another model…", LoRAs out of the model list + Browse LoRAs, eject/stop icons by the GPU stats | `ae3b30b09d49486c4` 84bab51 |
| 6 | Tear-off panel tabs with magnetic dock-back | `abdc6560b5c1fce24` c10a915 |
| 7 | Compositor stays on the GPU, less stutter | `ac614bc4e19a64ded` 261bd7a |
| 8 | Attributions page (low priority) | `a39281904c0608e21` 3ae996a |

## Develop engine upgrade (after the bug fixes)

Quality bar: Lightroom / Capture One. All Rust; faithful ports verified against upstream C
(reference vectors). Old edits need not render the same: one engine, no legacy path; re-record
golden hashes on intentional changes. Notes in `docs/wip/engine-*.md` on each branch; reference
data in `~/.local/share/local-image-dev/engine-sources/`.

| # | Item | Branch / WIP commit |
|---|---|---|
| 9 | Colour & tone: camera matrices (rawler data), three looks (Adobe-like / darktable sigmoid / Camera JPEG match + maker curves), WB per white, hue-safe curves, gamut compression, dither | `a8d90e3dc5155d0a9` 873247b |
| 10 | Detail: sharpening (CPU done), darktable denoise-profiled + haze removal ports, GPU | `afd6402aed9ef067b` 4a3a112 |
| 11 | Primary sliders: Highlights/Shadows (Capture One bar, A/B page for the owner), Clarity modes, Texture, Structure, color balance rgb, color equalizer, Skin Tone | `a5fc7556e46ab4857` 29f74bc |
| 12 | Raw quality: VNG4, AMaZE, segmentation highlights, reference checks of existing ports | `a39423f3409b6058a` 3dee439 |

## Follow-ups found along the way

- Occasional SIGSEGV at process exit in UI tests, inside the NVIDIA Vulkan driver during a background `lightcraft_gpu::render` (`wgpu create_buffer`) — likely a GPU render still running when the test process tears down. Check the app's own shutdown path for the same race.
- 2026-10-09 03:02: the kernel logged NVIDIA Xid 13 "Illegal Instruction Encoding" (SM warp exception) from the `tests_ai::heal_…` test process — a GPU kernel fault in the lc-gpu render path during the Develop UI tests; probably the same issue as the exit SIGSEGV. Reproduce with `cargo test -p lightcraft-ui-egui tests_ai` and check `journalctl -k | grep Xid`.
- Content-aware Heal on 24 MP photos with large strokes: unbenchmarked (runs in the background with progress).
- Remove overlay visibility (Auto / Always / Never) isn't bound to H yet (H opens the Remove panel).

## Integration (last)

13. Remove AI Denoise (dropped by the owner).
14. Merge everything; whole-workspace fmt, clippy and tests (incl. GPU equivalence).
15. Rebuild and reinstall V2.
