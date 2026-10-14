# Claude builder report — compat-d-forwards (old-version compatibility removal, job D)

Source: `docs/wip/INVENTORY-old-version-compat.md` section D + the "Left" sections of the job A/B/C
reports. Owner rule (Oct 14 2026): no backward compatibility; callers use the shared `li-*` crates
directly, no forwards/aliases left behind. Merge-parity test oracles stay (owner may drop later).

Status: done. One helper (no cargo, disjoint files) did the algorithm/engine rows (raster, matte, merge,
colour/CMS, metadata/codec, job control, engine entry points); the builder did the rest and all checks.

## Removed / changed

### `lightcraft-color` crate (J02 whole-crate facade) — deleted
- `crates/lc-color` removed from the workspace, `[workspace.dependencies]` and Cargo.lock.
- 75 source files: `lightcraft_color::` → `li_color::` (mechanical, no other change).
- Cargo.toml: lc-codecs, lc-engine, lc-pipeline, lc-raster, lc-ui-egui already had `li-color` (line
  dropped); lc-develop, lc-gpu, lc-merge, lc-raw, lc-scenes now depend on `li-color`.
- li-color README and the `lc-gpu` WGSL helper comment no longer mention the facade.

### `lightcraft-fetch` crate (J08 adapter) — folded into its only caller
- `crates/lc-fetch/src/lib.rs` → `crates/lc-segment/src/fetch/mirror.rs` (private module; same code,
  crate-level attributes dropped because lc-segment already has them), tests →
  `fetch/mirror/tests.rs`. `lightcraft_segment::fetch` re-exports exactly the items its users need
  (`DownloadError, FileSpec, Options, Progress, download`) instead of `pub use lightcraft_fetch::*`.
- lc-segment now depends on `li-ai` (native only; it already did through lc-fetch) and has `sha2` as
  a dev-dependency (workspace crate, already in Cargo.lock). Crate removed from the workspace.
- docs/PORTS.md and assets/attributions.json point at the new file paths; comments in lc-engine
  `segment/download.rs`, lc-sysmem `jobs.rs` and apps/local-image/Cargo.toml updated.

### GPU facades (J12) — deleted
- `lc-gpu/src/backend.rs` (`pub use li_gpu::backend::*`) and `pc-gpu/src/health.rs`
  (`pub use li_gpu::health::*`) deleted. lc-gpu uses `li_gpu::backend::…` directly; pc-gpu imports
  `li_gpu::health::{DeviceHealth, Fault}` privately (no crate-root re-export any more); pc-ui-egui
  (gpu_canvas, gpu_status, control, lib) and the pc-gpu / pc-ui-egui GPU tests use `li_gpu::health::…`.
- The standalone "legacy init-marker API" in li-gpu `backend.rs` (`set_init_marker`,
  `with_init_marker`, `take_init_marker`, its `MARKER` static) is deleted: nothing in the app ever set
  a marker path (only tests did), so `with_init_marker` was a no-op around `create_headless`. Window
  startup crash detection is the app's `gpu_startup::Sentinel` over `li_gpu::startup` (unchanged).
  lc-ui-egui `tests_people.rs` no longer clears the never-set marker.

### File-writer facades (J11) — deleted
- `lc-catalog/src/safe_file.rs` (`pub use li_fs::{…}`): every caller (lc-catalog store.rs, ~30 lc-engine
  files incl. all `fail_writes_after` fault-injection tests, the lc-catalog coverage test) uses `li_fs::`
  directly; lc-engine now depends on `li-fs`. The file's tests moved unchanged to the test-only module
  `lc-catalog/src/tests_safe_file.rs` (they call `li_fs` directly; names unchanged).
- `pc-format/src/atomic.rs` (`pub use li_fs::{…}` + crate-root `atomic_write`): callers in pc-format,
  pc-engine, pc-automation, pc-ui-egui (generate_ui, snapshot example) and apps/local-image use `li_fs::`;
  pc-engine, pc-automation and pc-ui-egui now depend on `li-fs`. Its tests (`atomic/tests.rs`,
  `atomic/staged_tests.rs`) moved unchanged to the test-only module `pc-format/src/tests_atomic.rs`
  (+ `tests_atomic/`).
- li-ai `settings::write_atomic` (anyhow wrapper over `li_fs::atomic_write`) deleted; its 12 callers in
  li-ai call `li_fs::atomic_write` directly.

### UI-kit facades (J17/J18) — deleted where they were forwards
- Deleted pc-ui-egui `icon_data.rs`, `color_icon_data.rs`, `tool_icon_data.rs`, `mono_tool_icons.rs` and
  `vector_packages/fill_stroke_proxy/icons.rs` (the latter three were also compiled into lc-ui-egui via
  `#[path]`; lc-ui-egui's `vector_tool_icon_data`, `tool_icon_data`, `color_icon_data`, `mono_tool_icons`
  modules are gone too). Callers use `li_ui_kit::{icon_data, color_icon_data, tool_icon_data,
  vector_icon_data, mono_tool_icons, color_icons}` directly.
- pc-ui-egui `color_icons.rs` keeps only `image()` (the shell's theme-token tint, real code); its
  re-exports are gone. The mono/colour silhouette test that lived in `mono_tool_icons.rs` moved into
  `color_icons.rs` tests unchanged (runs in both shells as before).
- pc-ui-egui `icons.rs` / `notices.rs` / `widgets.rs` re-exports (`li_ui_kit::icons::{exists, image}`,
  `li_ui_kit::notices::{MAX_NOTICES, Notice}`, `li_commands::widgets::{menu_row_button, menu_row_label}`)
  are private imports now; callers use the shared paths. `generate_ui::IMAGE_EXTS` (unused alias of
  `photocraft_codecs::PHOTO_EXTENSIONS`) deleted.
- xtask `j17_shared_icon_registries_remain_audited_through_compatibility_facades` →
  `j17_icon_registries_live_only_in_li_ui_kit` (asserts the facade files are gone + the same asset audit).
- Comments only (not forwards): li-ui-kit `ToastHistory` (the Library HUD tuple is the current Library
  API), `histogram::Bins` (`u32` = the Library preview format, not an old one), `TOOL_LABELS`,
  pc-ui-egui `rgb_histogram.rs` (real adapter + J18 parity oracles), pc-ui-egui `theme.rs` (see Left).

### Shortcut forwards (J20) — deleted
- lc-ui-egui `keymaps.rs` deleted (callers: `li_keys::keymaps::library_default`, `CAPTURE_ONE`).
- pc-ui-egui `keymaps.rs`: `pub use {AFFINITY, is_global}` and the `library` re-export module gone
  (the editor's `default`/`effective`/`global_conflict` logic stays). `shortcuts.rs`
  (`parse`, `consume`, `key_matches`), `prefs_ui.rs` (`is_modifier_key`, `shortcut_text`) and
  `fill_stroke_proxy::shortcut` (alias of `parse`) no longer re-export; ~15 callers use `li_keys::input::…`
  / `li_keys::keymaps::…`.

### Command-model path re-exports (J21) — deleted
- li-commands `menu.rs` `pub use li_keys::Chord` ("public compatibility path") → private import.
- pc-ui-egui `native_menu.rs` no longer re-exports `li_commands::menu::{Backend, Chord, Event, Node, …}`;
  pc-ui-egui `menus.rs` no longer re-exports `MenuItem`; lc-ui-egui `menus.rs` `UiCommand` and
  `menubar.rs` `CommandMenuNode as MenuNode` alias gone (callers use `li_commands::menu::CommandMenuNode`).
- apps/local-image (`library_host.rs`, `mac_menu.rs`) use `li_commands::menu::…` / `li_keys::Chord`
  directly; the app now depends on `li-commands` and `li-keys` (workspace crates it already linked).
  **mac_menu.rs is macOS-only and could not be compiled here** (no Apple target installed): path-only
  change, every replaced name checked by grep.
- KEPT (inventory recommendation): the Session-specialised type aliases in lc-engine `cmd/mod.rs` and
  pc-engine `commands.rs`.

### App directories facade — deleted
- apps/local-image `app_dirs.rs` (`pub use lightcraft_sysmem::app_dirs::{…}`) deleted; main.rs,
  services.rs, app_log.rs, library_host.rs use `lightcraft_sysmem::app_dirs::…`.

### Job B leftovers (handlers were in job C's files) — done
- `li.aiModels` and `edit.preferences.integrations` deleted: ai_ui.rs id list + match arm, pc-engine
  `section!("edit.preferences.integrations")`, prefs_ui.rs redirect, the two `UI_COMMANDS` rows,
  `MENU_ALIASES` and the alias half of the menu shortcut lookup in `menu_items`. Local AI setup has one
  command, `li.localAi` (Edit › Preferences › Local AI…). The hidden, empty Integrations preferences
  section is unchanged.
- Remove Background engine alias `"quick"` → `"fast"` deleted (`deserialize_engine`, `cpu()`, the
  `settings()` fix-up, ai_ui.rs `AiOptions` serde attribute and the subject-quality check). `settings()`
  now shows any engine value this build doesn't offer (hand-edited preferences) as the default `fast`
  model — a validity rule, not an alias.

### Job C leftovers — done
- li-gpu `j12_legacy_init_marker_shares_lock_and_clears_after_unwind` and
  `init_marker_is_written_during_creation_and_removed_after` deleted with the dead marker API.
- apps/local-image `j12_shell_shared_sentinel_preserves_legacy_marker_format` deleted: every assertion
  (clean → busy → crashed with the written marker → finish clears) is covered by the existing
  `sentinel_detects_a_crash_and_clears_on_success` in the same file, which also retries the
  fork/exec flock race this copy did not.
- `lc-engine/tests/references/tagging-eval/SOURCES.md`: `LOCAL_IMAGE_GPU_BACKEND=off`.
- docs/INSTALLATION.md, docs/LINUX-INSTALLATION.md (both labelled legacy 0.7 pages): the port-51247
  sentences now say they apply to 0.7 only and that V2 opens no editor port (control server only with
  `--control <port>` / `LOCAL_IMAGE_CONTROL_PORT`).

### Raster / resample forwards (J01/J27/J28) — helper
- Re-exports of `li_raster` deleted from lc-raster `histogram.rs`, `blur.rs`, `resample.rs` (the GPU tap
  tables stay), lc-pipeline `blur.rs` (second-hop `GaussianPlan`), pc-algo `resample.rs`, `image.rs`
  (`Edge`, `premultiply`, `unpremultiply`), lc-engine `export.rs` (`ResampleChoice`), li-ai `lib.rs`
  (`Filter`, `Frame`, `GrayFrame`, `RgbFrame`, `RgbaFrame`). pc-algo `histogram.rs` (pure re-export)
  deleted; its tests moved unchanged to `pc-algo/src/tests_histogram.rs`. ~40 callers repointed.
- KEPT: li-raster resample numeric policy, the 8-bit adapters with live Develop callers (`run`,
  `run_patch`, `composite_rgb`, `denoise_frame`).

### Matte forwards (J14) — helper
- pc-algo `matting.rs`, `cf_matting.rs` re-exports and li-seg `pub use li_matte as matte` deleted;
  `selection/distance.rs` is now test-only (it keeps the `old_edt` parity oracle). Callers in pc-algo,
  pc-engine and pc-ui-egui use `li_matte::…`.

### Merge facades (J19) — helper
- pc-algo `features.rs` (source-compatible registration facade, no production callers) deleted; its
  tests and J19 parity module moved to lc-merge `editor/features.rs`. lc-merge `ransac` lost the unused
  `iters` parameter (all callers updated). pc-algo `hdr.rs`/`panorama.rs` re-exports and the
  `photo_util.rs` test alias removed; pc-engine `photo_cmds.rs`/`align_cmds.rs` use
  `lightcraft_merge::editor::…` (pc-engine now depends on lightcraft-merge).
- KEPT: lc-merge `editor/geom.rs` (real type), the BRIEF descriptor path (public `detect`/`match_pairs`,
  used only by tests/benches; removing a whole matcher is an owner call), lc-engine `merge.rs` demosaic
  fallback (handheld X-Trans HDR needs it), pc-algo panorama adapter types. Comments reworded.

### Colour / CMS forwards (J02/J22) — helper
- pc-cms `curve.rs`, `lutfile.rs`, `math.rs` (pure `li_color` re-exports) deleted; pc-cms internals
  and every caller use `li_color::{cms_math, curve, lutfile}`. pc-color `convert.rs` no longer
  re-exports `li_color::convert`; callers in pc-compose, pc-engine, pc-io, pc-raster, pc-vecfx,
  pc-vector, pc-ui-egui use `li_color::convert::…`; li-gpu's WGSL test uses `li_color` (dev-dep swapped
  from photocraft-color).
- KEPT: lc-codecs `icc.rs` (real code), lc-pipeline `render` (infallible wrapper, many callers).

### Metadata / codec facades (J03/J06/J23) — helper
- lc-codecs `exif.rs` and `jpeg_par.rs` deleted (callers use `lightcraft_tiff::exif` and
  `photocraft_codecs::jpeg_par`); pc-codecs `orientation.rs` / crate-root re-exports of
  `upright_xmp`, `exif_orientation`, `upright_exif` deleted (callers use `lightcraft_meta::upright_xmp`
  and `lightcraft_tiff::exif::{orientation, upright}`). pc-codecs fuzz target updated (standalone fuzz
  workspace, not compiled here).
- KEPT: pc-algo `exif.rs` (own CameraInfo type), lc-codecs `jpeg.rs`, pc-codecs raster-only `detect`.

### Job-control forwards (J09) — helper + builder
- pc-raster `interrupt.rs` (`pub use lightcraft_sysmem::jobs::{Cancelled, Interrupt}`) deleted, its two
  tests moved unchanged to lc-sysmem `jobs.rs`; ~66 callers use `lightcraft_sysmem::jobs::…`. li-ai and
  li-seg no longer re-export `Cancelled`. li-fs's public `Interrupt` re-export became a private import
  of `lightcraft_sysmem::jobs::Interrupt` (li-fs now depends on lightcraft-sysmem instead of the heavier
  photocraft-raster). lc-segment `fetch/mirror.rs` uses `lightcraft_sysmem::jobs::Cancelled`.
- KEPT: li-ai `JobControl` newtype and `Progress` (`JobStatus<Stage>`) — Stage-specialised types.

### Engine entry points (J04/J28) and leftovers — helper
- pc-engine `smart_cmds::source_image`, `developed_image` and `lens_cmds::camera_raw_surface`
  (built-in-profile wrappers; only tests called them) deleted; tests call the `*_in_profile` / `_in`
  functions with the same profile and fallback. lc-meta `write_file_info` (infallible wrapper) deleted;
  callers use `try_write_file_info`. pure `pub use li_commands::CommandInfo` removed from lc-engine and
  pc-engine; pc-engine `file_cmds` no longer re-exports `lightcraft_meta` functions.
- KEPT: pc-engine import-job `"document": index` (part of the automation/MCP result shape, asserted by
  jobs tests; comment reworded), lc-engine `heal` (maps progress/errors), li-ai catalog runner
  descriptors (inventory: leave).

## Checks run
- `cargo check --offline --workspace --all-targets`: clean (0 warnings). Also
  `cargo check -p lightcraft-engine --features sam --all-targets` (the only user of the folded fetch).
- `cargo fmt` on the 38 touched packages.
- `cargo clippy --offline --all-targets -- -D warnings` on the 38 touched packages: clean.
- `cargo xtask attributions` regenerated (paths + first-line descriptions);
  `attributions_json_is_up_to_date` passes.
- Filtered tests only (all pass): lc-segment `fetch` (10, incl. the moved mirror tests); lc-catalog
  `tests_safe_file` (8); pc-format `tests_atomic` (19); xtask `j17_icon_registries_live_only_in_li_ui_kit`;
  pc-ui-egui `color_icons::tests` (4, incl. the moved silhouette test),
  `background_ui_engine_values_are_not_aliased_and_defaults`, `menus_ux_preferences_hide_empty_integrations`,
  `menus_ux_order_geometry_and_verified_duplicates`; lc-sysmem `interrupt_tests` (2); pc-algo
  `tests_histogram` (7), `selection::distance` (7); lc-merge `editor::features` (8); pc-codecs
  `j03_shared_metadata_helpers…`; lc-meta `j03_file_info` (3); lc-gpu
  `develop_gaussian_host_plan_matches_cpu_policy`; li-seg `j14_shared_matte…`; lc-raster `j01_final_dense…`;
  pc-engine lib `smart_cmds::tests` + `file_cmds::tests` (49) and coverage `ds_smart_cmds/ds_lens_cmds/
  ds_file_cmds` (42).
- Not compiled: apps/local-image `mac_menu.rs` (macOS only) and the pc-codecs fuzz workspace.

## TEST-CHANGED
- li-gpu `backend.rs`: `j12_legacy_init_marker_shares_lock_and_clears_after_unwind`,
  `init_marker_is_written_during_creation_and_removed_after` deleted with the dead marker API.
- apps/local-image `gpu_startup.rs`: `j12_shell_shared_sentinel_preserves_legacy_marker_format` deleted
  (duplicate of `sentinel_detects_a_crash_and_clears_on_success`, see above).
- xtask `icons.rs`: `j17_shared_icon_registries_remain_audited_through_compatibility_facades` →
  `j17_icon_registries_live_only_in_li_ui_kit` (asserts the facade files no longer exist).
- pc-ui-egui `menus/conventions_tests.rs`: `menus_ux_order_geometry_and_verified_duplicates` asserts
  `edit.preferences.integrations` / `li.aiModels` are gone (not live, not in menus) and `li.localAi` is
  live (was: aliases live but hidden); `menus_ux_preferences_hide_empty_integrations_and_preserve_legacy_route`
  → `menus_ux_preferences_hide_empty_integrations` (the alias-route half removed).
- pc-ui-egui `background_ui_tests.rs`: `background_ui_quick_migration_and_defaults` →
  `background_ui_engine_values_are_not_aliased_and_defaults` (`"quick"` stays `"quick"`; defaults asserted
  from `{}`).
- Moved unchanged: lc-catalog `safe_file` tests → `tests_safe_file`; pc-format `atomic::{tests,
  staged_tests}` → `tests_atomic::…`; `mono_tool_icons::tests::every_tool_style_has_the_same_rendered_
  silhouette_at_actual_sizes` → `color_icons::tests::…`; pc-algo `histogram` tests → `tests_histogram`;
  pc-algo `features` tests + `j19_parity` → lc-merge `editor::features`; pc-raster `interrupt` tests →
  lc-sysmem `jobs::interrupt_tests`.
- Renamed: lc-gpu `develop_gaussian_host_plan_matches_cpu_compatibility_policy` → `…_cpu_policy`;
  pc-codecs `j03_metadata_adapters_share_the_library_backend_policy` →
  `j03_shared_metadata_helpers_read_and_reset_orientation` (the adapter-vs-backend asserts would be
  `x == x`; now asserts orientation round trips).
- Calls changed with the API: lc-merge `ransac` without `iters`; pc-engine tests call
  `source_image_in_profile` / `camera_raw_surface_in` (same profile/fallback); pc-engine file_cmds tests
  use `try_write_file_info(..).unwrap()` (stricter); lc-meta j03 file-info test inlines the old wrapper.
  No assertion was weakened.

## Outside edits
- Docs: docs/PORTS.md, assets/attributions.json (regenerated), assets/icons/SOURCES.md,
  docs/DESIGN-SYSTEM.md, docs/specs/smart-sort.md, docs/INSTALLATION.md, docs/LINUX-INSTALLATION.md,
  crates/li-color/README.md, lc-engine tagging-eval SOURCES.md.
- Cargo.toml dependency edges (workspace crates only, Cargo.lock only loses lightcraft-color and
  lightcraft-fetch): li-color → lc-develop/lc-gpu/lc-merge/lc-raw/lc-scenes/pc-raster/pc-vector,
  li-gpu dev; li-fs → lc-engine/pc-engine/pc-automation/pc-ui-egui; li-raster → lc-codecs/lc-gpu/
  lc-pipeline/pc-engine/pc-ui-egui/lc-ui-egui (+dev lc-raw/lc-scenes); li-matte → pc-engine/pc-ui-egui;
  lightcraft-merge → pc-engine; lightcraft-sysmem → pc-algo/lc-pipeline/pc-io/li-fs/lc-segment (+dev
  pc-format; removed from pc-raster); li-ai + sha2(dev) → lc-segment; li-commands + li-keys →
  local-image; lightcraft-tiff (dev) → local-image-cli and the pc-codecs fuzz crate.
- J29 files (i18n engines) were not touched.

## Left, and why
- **pc-ui-egui `theme.rs`** `pub use li_ui_kit::theme::*` (496 uses in 162 files) and **lc-ui-egui**
  `theme.rs` token re-exports / flattened `Tokens`, `widgets::{register, take_registry, Registry}`
  (87 uses), `icons::Icon` (23 uses): each shell's theme/widget module is the design-system entry point
  its panels import; renaming several hundred call sites is cosmetic and would collide with J29's
  edits across the same files. Comment on pc-ui-egui theme.rs reworded. Can be a follow-up if wanted.
- li-ui-kit `ToastHistory::observe`: the Library HUD tuple (`ui.toast`, 5 writers) is current Library
  code, not an old API; converting the writers to `post()` is a refactor, not compat removal.
- BRIEF descriptor matcher in lc-merge editor (tests/benches only) — owner call.
- Merge-parity oracles (li-keys `legacy_tests.rs`, li-color compatibility tests, J18/J19/J23 goldens…)
  kept per task.
- UX follow-up (not compat): three pc-engine error messages tell users to open
  "Help › AI Models & GPU…", which has never been a visible menu item (select_extra_cmds.rs:62, seg.rs:29,
  smartselect_cmds.rs:188). They should say "Edit › Preferences › Local AI…"; not changed here because
  they are translated strings (i18n is J29's).
- docs/wip/vector-integration-validation.json still lists old test paths (historical record).
