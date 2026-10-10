# Package 29 sources and fixtures

- Base Local Image commit: `6be90631`. Existing implementations followed in place:
  `crates/lc-ui-egui/src/headless.rs` (HeadlessView/raw input), `widgets.rs`
  (automation registry), `panels/grid.rs` (photo menus), `panels/doc_layers.rs`
  (same-window open capability), `apps/local-image/src/library_host.rs`
  (module/open bridge), `crates/pc-ui-egui/src/file_open.rs`, `jobs_ui.rs` and
  `crates/pc-engine/src/jobs.rs` (bounded reader, decoder, job event/result routing).
  These are existing Local Image sources, GPL-3.0-or-later, not new external ports.
- Contract: `docs/specs/vector-architecture.md` sections 1/5,
  `vector-quality-plan.md` B.3, persona spec Owner decisions and the task's final
  2026-10-10 SPEED/REGISTRATION rules. No new settings/document schema fields.
- egui **0.36.2**, egui_kittest **0.36.2**, eframe/wgpu versions in Cargo.lock.
  Pinned local official source documentation used:
  `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/egui-0.36.2/src/util/id_type_map.rs`
  (`get_temp`, `get_temp_mut_or_default`: transient cloneable context state).
  API documentation: <https://docs.rs/egui/0.36.2/egui/struct.Context.html>,
  <https://docs.rs/egui/0.36.2/egui/struct.Response.html#method.context_menu>.
- Package 28 WIP read-only API inspected at
  `../vec-28-trace-dialog-output/crates/pc-ui-egui/src/vector_packages/trace_dialog.rs`
  and `../vec-28-trace-dialog-output/crates/pc-engine/src/vector_packages/trace_output.rs`:
  State::open/cancel/show/poll/request_preview/pending_preview; Capture and Preview.
  This provider is not present/registered in this branch. No code copied from it.
- Original Library fixture: three in-memory catalog photos, IDs 1/2/3, paths
  `/library-fixture/{id}.png`, 4x3 px, active 1, selection [1,3], context target 2,
  People filter IDs [17,21], everyone=true, find_name=Ada. No filesystem writes
  or external proprietary corpus; missing paths are never successful decode gates.
- Original decode fixture: PNG generated at test time by existing image **0.25.10**
  PngEncoder, RGBA8, 4x3 px, alternating opaque red [255,0,0,255] and blue
  [0,0,255,255] texels, no ICC profile. Independent expected decoded raster
  samples: (0,0)=red, (1,0)=blue. Invalid input is literal `not a PNG`.
  These synthetic inputs are original Local Image test code, GPL-3.0-or-later.
  No brand assets, external fixtures or corpus success claims.
- GPU source: existing Local Image offscreen eframe harness/adapter setup idiom;
  original 128x96 RGBA8 PNG with alternating 32-pixel opaque red/blue columns,
  generated with the same image PngEncoder. This larger fixture keeps native-texel
  zoom inside the dialog's 0.1..64 range and gives substantial colour regions.
  Actual 28 PixelArt preview at scale 1, Result mode, 1 texel per screen pixel,
  integer-aligned image origin; pointer pan (+24,+18) press/move/release then Cancel.
  Sample displayed bytes against opaque CPU preview bytes (premultiplied is equal
  to straight RGBA for alpha=255), max channel error <=1/255. RTX 5090, Vulkan,
  discrete, Rgba32Float accumulation, float16 canvas required. No execution or
  performance claim in 29 under the owner speed rule.
- No newly adapted upstream files; PORTS/attribution fragment is empty.
