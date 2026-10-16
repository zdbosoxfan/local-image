# J17 / D24 — shared UI tokens, icons and fonts

Completed from branch base `88a073c5`, preserving the existing monochrome artwork, module layouts,
font-size fixes and public UI APIs. Changes are uncommitted. The pre-existing modified task document
was not edited. LRC-UX was already merged (`7c65a884`); its host logic remains intact apart from the
palette delegation and a focused integration test.

## Delivered

- **Tokens:** `li-ui-kit::theme` owns `ThemeKind` and the canonical `Copy` token union. Common/editor
  roles live on `Tokens`; Library-only roles live in `LibraryRoles`, with layout in `Metrics`.
  Both inherited style applicators and the host contrast/palette mapping live in the kit. The editor
  theme module re-exports them; Library's old flattened `Tokens` is a lossless compatibility view,
  retaining struct-literal callers without another set of palette constants. `library_palette` is
  now one delegation. Library metric constants also re-export the kit.
- **Icons:** one compiled owner for generic, tool, vector and Library artwork/identities and family
  metadata. Registries intern identical SVG contents; white SVG preparation is lazy. One existing
  egui_extras/resvg rasterizer and one per-context texture backend serve image widgets, Library
  painters, cursors and optional colour tools. The backend implements egui's `TextureLoader` API.
  Physical size, stroke profile, tint, disabled state and texture options form its cache key.
  Normal residency is limited to 1024 entries / 8 MiB, with least-recently-used eviction; current-frame
  textures stay alive even when that frame temporarily exceeds a limit. Missing generic names share
  the same square fallback. Codegen copies of promoted SVG slices still coalesce by content.
- **Fonts:** one profile-aware stack builder and font-size plugin, borrowing only the existing
  `pc-text::fonts` Latin font statics. Library no longer embeds Inter/JetBrains Mono separately.
  Both inherited fallback orders, named weights, symbol coverage, original scale multipliers,
  deferred first-frame installation and reinstallation behavior remain intact. Engine preference
  enum adaptation and locale/CJK source selection remain thin frontend policy adapters.
- **Audit/documentation:** `xtask icons` now audits the canonical tables, rather than the re-export
  facades; the 168 existing tool assets still pass hash, licence, line-style and registry checks.
  `docs/DESIGN-SYSTEM.md` records backend ownership and the correct icon-registration locations.

## Quality and small-sample measurements

| Gate / measurement | Before | After |
| --- | --- | --- |
| Theme tokens | 36 editor fields × 5 themes; 43 Library fields × 6 profiles | All 438 field values identical |
| Full egui style fingerprints | 5 editor + 5 hosted Library + standalone Library | All 11 identical |
| Font stacks / face bytes / tweaks / Latin and symbol shaping | Both inherited profiles | Identical; shared static-byte pointer identity verified |
| Interface/tool/vector/colour/disabled SVG rasters | Frozen original algorithms at 16/20/25/32/40/50/80 physical px | 6,874 comparisons; maximum RGBA difference **0** |
| Shared 20 px Crop in Library painter + editor image widget | 2 raster uploads; 3,200 RGBA texture bytes | 1 upload; 1,600 bytes |
| Small egui icon-path sample, 13 CPU frames per implementation | Cold 448 µs; warm median 2 µs | Cold 245 µs; warm median 2 µs |
| Duplicate Latin embedding declarations | 3 duplicate declarations totalling 1,101,608 source bytes | 0; font data borrows the existing canonical statics |

The timings are one small observation through the original/current egui image and painter paths,
not a benchmark or a general application/GPU speed claim. Upload counts are egui texture-delta
requests; linked binary size and physical GPU memory were not measured. Font byte savings describe
removed embedding declarations, not an assumption about native linker deduplication.

The actual host `logic` + `ui` path was also exercised with a generated test photo, CPU rendering,
and real Library → Develop → Compositing switches in Pro Medium and Studio Light. Switch + three
frames took respectively **20.083 / 16.231 / 39.024 ms** and **12.360 / 14.455 / 31.445 ms**.
These are after-change smoke samples, including normal host work, without an overall before/after
speedup claim. No tuning depends on this machine.

Cache regressions additionally prove that 1025 current-frame textures remain valid before trimming
back to 1024, and 33 large-DPI 256 px textures trim back to 32 / at most 8 MiB next frame. Ordinary
icons retain exact silhouettes, optical padding, 1.75 px interface strokes and sourced 2/1.5 px tool
strokes. Public icon IDs, cursor artwork/hotspot behavior, disabled tint/opacity, monochrome default,
optional family tints and saved theme names/defaults remain unchanged. No user-facing strings or
catalog/settings/document schema changes were introduced.

## Validation

Followed the task's **ABSOLUTE MINIMUM TESTING** override: no full crate/workspace suites, GPU
acceptance runs or benchmarks. One cargo process at a time; offline Rust 1.98.1; launcher-provided
`CARGO_BUILD_JOBS=4` unchanged; the same retained `target/` directory for all commands.

**15 focused tests passed:**

- `li-ui-kit`: 9 (8 with `j17_`, plus the subsequently added named-registry priority regression).
- `lightcraft-ui-egui`: 1; actual icon/tool clicks and disabled behavior in 30 theme/DPI/colour-mode
  combinations using its headless harness, with rendered pixels checked.
- `photocraft-ui-egui`: 3; accessible clicks/disabled controls in all five themes and both modes,
  cursor source/geometry, and the updated AI Stop/status-icon integration regression.
- `local-image`: 1; real host module switching and token restoration.
- `xtask`: 1; all moved registries remain covered by the existing asset-manifest audit.

Commands used (test runs filtered by the listed new names; UI/host runs set `LOCAL_IMAGE_NO_PREFS=1`):

```sh
PATH=~/.cargo/bin:$PATH cargo +1.98.1 fmt -p li-ui-kit -p lightcraft-ui-egui -p photocraft-ui-egui -p local-image -p xtask
PATH=~/.cargo/bin:$PATH cargo +1.98.1 fmt --check -p li-ui-kit -p lightcraft-ui-egui -p photocraft-ui-egui -p local-image -p xtask
PATH=~/.cargo/bin:$PATH cargo +1.98.1 check --offline -p li-ui-kit -p lightcraft-ui-egui -p photocraft-ui-egui -p local-image -p xtask
PATH=~/.cargo/bin:$PATH cargo +1.98.1 clippy --offline -p li-ui-kit -p lightcraft-ui-egui -p photocraft-ui-egui -p local-image -p xtask --all-targets -- -D warnings
PATH=~/.cargo/bin:$PATH cargo +1.98.1 clippy --offline -p photocraft-ui-egui --all-targets -- -D warnings
PATH=~/.cargo/bin:$PATH cargo +1.98.1 test --offline -p li-ui-kit j17_ -- --nocapture
PATH=~/.cargo/bin:$PATH cargo +1.98.1 test --offline -p li-ui-kit j17_registry_names_keep_previous_lookup_priority_and_white_geometry -- --nocapture
PATH=~/.cargo/bin:$PATH LOCAL_IMAGE_NO_PREFS=1 cargo +1.98.1 test --offline -p lightcraft-ui-egui j17_ -- --nocapture
PATH=~/.cargo/bin:$PATH LOCAL_IMAGE_NO_PREFS=1 cargo +1.98.1 test --offline -p photocraft-ui-egui j17_ -- --nocapture
PATH=~/.cargo/bin:$PATH LOCAL_IMAGE_NO_PREFS=1 cargo +1.98.1 test --offline -p photocraft-ui-egui --lib j17_stop_cancels_ai_jobs_and_shared_icons_render -- --nocapture
PATH=~/.cargo/bin:$PATH LOCAL_IMAGE_NO_PREFS=1 cargo +1.98.1 test --offline -p local-image j17_ -- --nocapture
PATH=~/.cargo/bin:$PATH cargo +1.98.1 test --offline -p xtask j17_shared_icon_registries_remain_audited_through_compatibility_facades -- --nocapture
```

Final check, strict clippy and formatting pass. The final test-only AI URI adaptation received an
additional focused editor test and strict editor clippy pass. Logs and measurements are under
`target/j17-*.log`. The source-alias regression's initial failure is retained in
`target/j17-tests-kit-source-alias-failure.log`; the content fallback fixes it and the parity/lookup
tests pass. Frozen pre-change source/capture inputs remain under `target/j17-baseline/`; shipped
fixture provenance is documented in `crates/li-ui-kit/tests/golden/README.md`.

GPU tests added/run: **0**, as required by the minimum-testing override. The existing build-script
warning about absent optional craft-fonts remains; this checkout does not establish release CJK
coverage. Existing CJK ordering/loading adapters are retained. No GUI was opened, running app killed,
ComfyUI job submitted or owner image used.

## Compatibility and unavoidable outside edits

The duplicate production token constructors, SVG preparation/caches and Latin stack construction
were removed from the old UI modules; their APIs remain facades as requested. Library's flattened
field view and frontend domain mappings remain intentionally until J18 moves widget callers. Widget
consolidation belongs to J18, not this task; no J17 work remains.

Outside the owned implementation paths:

- Root/UI `Cargo.toml` files and `Cargo.lock`: required path dependency wiring. Lock inspection shows
  **only `li-ui-kit` added**, no new external package, version change or C/`-sys` dependency.
- Editor `mono_tool_icons.rs`, `tool_icon_data.rs`, `color_icon_data.rs`, and
  `vector_packages/fill_stroke_proxy/icons.rs`: re-export adapters required to stop compiling shared
  source/asset registries independently in both UI crates.
- `xtask/src/icons.rs`: required canonical registry paths and one focused audit regression.
- `crates/pc-ui-egui/src/ai_ui.rs`: **test-only** adaptation of an old literal SVG URI assertion.
- `docs/DESIGN-SYSTEM.md`: backend ownership/registration documentation. The host file also contains
  the focused module-switch test adjacent to its existing test fixtures.

TEST-CHANGED: `generic_cursor_artwork_and_tool_artwork_use_separate_sources` became
`j17_cursor_hotspot_artwork_remains_separate_from_tool_artwork`: old internal URI literals became
canonical source-byte/profile checks, retaining the distinct cursor/tool assertion.

TEST-CHANGED: `icons_are_recoloured` now checks the same white/no-currentColor invariant through
shared prepared bytes, replacing access to the removed private eager map.

TEST-CHANGED: `stop_cancels_ai_jobs_and_the_icons_render` became
`j17_stop_cancels_ai_jobs_and_shared_icons_render`: preserved all Stop/cancellation assertions;
icon readiness now uses the public image/texture-loader API rather than the retired literal URI.

## Sources followed

- Duplicate-systems audit §1, D24 §3 and J17 §4; current branch source was the parity reference.
- [Design system](../DESIGN-SYSTEM.md), [upstream PhotoCraft UI design](../upstream/photocraft/ui-design.md),
  and the merged [LRC-UX report](CODEX-REPORT-lrc-ux.md). No new Adobe/Affinity interaction or layout
  convention was introduced; the established module designs are preserved.
- The installed official egui **0.36.2** crate source/docs: `src/load.rs` (`TextureLoader`, `SizeHint`),
  `src/load/texture_loader.rs` (physical-size texture caching/lifecycle), and `src/widgets/image.rs`
  (`ImageSource::load`, `Image::load_for_size`, `paint_at`). Used the documented loader API, without
  reaching into egui internals or adding a rasterizer dependency.
- Existing `pc-text::fonts`, `assets/icons/SOURCES.md` and `source-manifest.json`: canonical font
  byte ownership, exact sourced artwork and licences. The bounded shared icon cache is the local
  integration policy; measurements and current-frame lifetime tests justify it.
