# Develop C completion report

Tasks C1, C2, C3 and A1, continued on `wip/cx-develop-c` from the existing Develop layer implementation. The initial worktree had only the owner's task-file edit; it is preserved. The required spec/review were read fully from the parent checkout because those two documents are absent here. No commits, dependency changes, GUI windows, or changes to the owner's source photos.

## Delivered

- **C1:** Theme-derived curve colours adapt to dark/light palettes with 4.5:1 minimum stroke contrast. The plot has a separate fill/border, a faint dashed identity reference, a 5 px halo beneath the 2.75 px stroke, 6 px outlined resting handles (8 px when hovered/dragged), and hollow endpoint centres. Available histogram data is drawn behind it. Parametric display samples the actual monotone pipeline LUT, including the master curve, and has three draggable split dividers with grouped undo. Layer headers read `Curve - Face`. These widgets retain the requested panel width even when preceding RAW controls grow egui's available rectangle. A preferred 240 pt plot reduces margins/size to fit a 250 px panel instead of overflowing it.
- **C2:** Distinct `WhiteBalance`, `SkinTone` and `Defringe` vector glyphs, three original Glossy Duotone SVG variants in both styles, shared registry rows, and matching picker cursor badges. Skin picking uses an icon button. Existing artwork/paint is reused with original disc, face and fringe badges; provenance is recorded in `assets/icons/SOURCES.md` and both attribution JSON files. Point Color retains its own generic selector.
- **C3:** Sample swatch, J/C/h readout, Clear, and live Hue / Saturation (UCS chroma) / Lightness strips with marked windows, including hue wraparound. The missing-reference state has a visible hint and disabled controls. UI swatches reduce out-of-display-gamut chroma at constant J/h. `Overlay::SkinToneRange` supports Background and a layer id, stable cache/worker encoding, and excludes the selected tool's own correction while viewing its range. CPU selection uses scene-linear UCS22 weights; GPU computes the same coverage in a native kernel. Selected pixels stay coloured and other pixels become grey. The layer picker now receives clicks from the existing Masking panel; the range checkbox takes precedence over the red mask overlay and a stale layer selection cannot retain its diagnostic. Other mask-tool decoupling remains A4's scope.
- **A1:** Full and Empty creation commands and real UI buttons with `New Filled Layer` / `New Empty Layer` labels. Auto-naming by kind also covers deferred Describe creation. CPU Full-only masks use `Alpha::Scalar`; GPU packs spatial masks densely and keeps Full coverage in scalar parameters, allocating no full-resolution alpha or component plane for a Full-only layer, including its mask diagnostic overlay. Full combined with subtract/intersect components uses the spatial path and a native Full kernel. Opacity/amount/inversion work on both paths. Global-only Vignette, Grain, Treatment/B&W mix are excluded from layer UI/used/reset-section sets; whole-layer Reset also clears hidden legacy tool data, as required by review G8. Their existing engine rendering support is retained for old settings/golden compatibility.

## Full compatibility

Persisted Full components are `{ "kind": "brush", "strokes": [], "full": true }`. New readers recognise the marker as Full; pre-Full serde readers ignore the unknown field and load a safe empty brush. They retain the whole document and its edits, although the Filled layer has no coverage on a downgrade. An older reader that rewrites the component loses the marker; upgrading then loads that component as an empty brush. The compatibility test parses a complete document with a pre-Full nested shape schema and tests this rewrite/load case. Documents without Full retain their existing serialization and golden render hashes.

## Validation

Rust/Cargo 1.98.1, offline, the supplied `claude-env.sh`, `CARGO_BUILD_JOBS=4`, one build lane and one retained `target/`. No broad workspace run.

- Strict Clippy (`--all-targets -- -D warnings`) passed on all six changed crates.
- The complete six-crate run finished. Its initial results (before the affected-case follow-up) are:

| Crate | Passed | Failed | Ignored |
|---|---:|---:|---:|
| lightcraft-develop | 104 | 0 | 0 |
| lightcraft-engine | 880 | 0 | 6 |
| lightcraft-gpu | 94 | 0 | 11 |
| lightcraft-pipeline | 412 | 1 | 3 |
| lightcraft-ui-egui | 318 | 3 | 5 |
| photocraft-ui-egui | 1605 | 15 | 65 |

The new Skin Tone wire kind replaces the old test's unused kind 6; the unknown-kind assertion now uses 7, retaining 255 and the new kind's independent round-trip coverage. The existing skin-picker control id is preserved as an alias. The existing layer Reset interaction now scrolls to its button after the added Filled/Empty actions move it below the short viewport. Final follow-up: **pipeline 414 passed / 0 failed / 3 ignored; GPU 94 passed / 0 failed / 11 ignored** (complete suites after the scalar diagnostic fix). **UI affected cases 6 passed / 0 failed / 1 benchmark ignored**, including both repaired existing interactions and all four task acceptance tests on the 45.44 MP RAW. The benchmark was then explicitly run and passed (**1 passed / 0 failed**) on the final code. Strict Clippy was refreshed for pipeline/GPU/UI and passed. The final stale-layer diagnostic guard also passed its complete Skin Tone walkthrough (**1 passed / 0 failed**) and fresh UI all-target Clippy. Ignored numerical/model/milestone tests keep their existing annotations; the new interactive benchmark is explicitly executed, rather than counted as GPU evidence from an ignored run.

The three task-related initial failures are now reduced to the **16 out-of-scope failures retained below**: one documented Library fullscreen-navigation failure and 15 editor failures. The complete six-crate run was performed once; only changed/failing relevant cases and the pipeline/GPU suites were repeated after the concrete fixes. No whole-workspace rerun.
- New focused CPU tests prove Full exposure +1 equals global +1, 50% opacity equals a 1.5x scene-linear gain, Empty is inert, scalar coverage switches to spatial coverage after subtracting a brush, and Skin Tone range respects the scene reference/target/worker encoding.
- The new hardware GPU test `develop_c_full_scalar_opacity_mixed_masks_and_skin_overlay_match` passed on **NVIDIA GeForce RTX 5090 / Vulkan**. It covers Full at 50%, primary clarity/scalar mixing, scalar/spatial/scalar ordering, chromaticity packing, tone-equalizer offsets, Full mask overlay, Full minus Radial, and global/layer Skin Tone overlays. Existing parity tolerance is retained (mean < 0.5, maximum <= 3 display levels); native renders assert no host-stage fallback.
- Four UI acceptance tests pass: E1 raster/point/split drags on Background and Face; D2 icon/pick/range drags/overlay/Clear on both targets; Filled/Empty real creation plus exposure/opacity drags; distinct registered assets/cursor mapping. They use the Library/Develop `headless.rs` harness with actual clicks and drags. The final walkthroughs use the 8256 x 5504 (45.44 MP) Bayer DNG described below. The existing icon silhouette test is included in the full UI suites.
- `cargo xtask attributions`: regenerated successfully, 758 crate entries. `cargo xtask assets`: **2048 assets attributed**, licence files present.
- Formatting and scoped diff checks pass. The owner's pre-existing task-file trailing blank line is untouched.

Logs include `target/develop-c-clippy.log`, `develop-c-clippy-final.log`, `develop-c-full-tests.log`, `develop-c-pipeline-gpu-final.log`, `develop-c-ui-final.log`, `develop-c-ui-clippy-final.log`, `develop-c-skin-final.log`, `develop-c-interactive-final.log`, `develop-c-ui-raw-walkthrough.log`, `develop-c-gpu-filtered.log`, `develop-c-attributions.log` and `develop-c-assets.log`; earlier filtered/debugging logs are retained separately. No failing test was weakened or newly ignored to obtain a pass. TEST-CHANGED: the unknown wire-kind fixture advances from 6 to 7 because 6 is now implemented; the layer Reset test scrolls to its actual button. Their assertions remain intact.

## Interactive GPU measurement

The ignored `develop_c_layer_drag_interactive_gpu_budget` was explicitly run, using real Headless app input, engine jobs, GPU stage caches and displayed textures. Every live drag asserts the 1280 px draft limit and awaits the latest texture on release; GPU availability and retained GPU stages are required. The 45.44 MP Bayer DNG is a reproducible **synthetic RAW**, generated with this repository's DNG writer and existing RAW-test conventions; this is not a photographic sensor benchmark. Its generator and DNG live under `target/develop-c-ux/reference/`. A separate copied photographic ARW (3744 x 5616, 21 MP) also passed the same interactive path; its original was read only.

| 45.44 MP Bayer RAW case | UI p95 | Change to displayed texture p95 | Retained GPU stage cache |
|---|---:|---:|---:|
| 1 Brush layer, exposure + clarity | 1.59 ms | 15.63 ms | 243,719,320 B |
| 4 Brush layers, exposure + clarity | 1.05 ms | 25.55 ms | 413,623,384 B |
| 8 Brush layers, exposure + clarity | 1.10 ms | 32.36 ms | 640,162,136 B |
| 8 Full-only exposure layers | 1.19 ms | 8.09 ms | 621,691,640 B |

Six measured live moves per case, with previews awaited after each; these are step latencies, not a continuous-drag FPS claim. Stage-cache bytes include retained prior work and are **not peak VRAM**. The no-alpha-plane claim comes from the scalar representation/packed allocation path and exercised parity/layout assertions, not that cache-size comparison. This task does not claim full-resolution eight-layer/NR export or the full A–H integrated layer-bar walkthroughs (owned by the other streams).

Reproduce after regenerating the fixture:

```sh
source ~/.local/share/local-image-dev/claude-env.sh
LC_REQUIRE_GPU=1 LC_DEVELOP_C_RAW="$PWD/target/develop-c-ux/reference/45mp.dng" cargo test --offline -p lightcraft-ui-egui develop_c_layer_drag_interactive_gpu_budget -- --ignored --nocapture
LC_DEVELOP_C_REFERENCE="$PWD/target/develop-c-ux/reference/45mp.dng" cargo test --offline -p lightcraft-ui-egui tests_develop_c
```

## Visual review

I inspected the individual narrow/light/dark captures and all three contact sheets. The plots, endpoint outlines, dashed reference on edited curves, histogram, layer header, pick/readout/Clear, wrapped hue window, and three selection strips are visible without horizontal overflow. 250 and 400 px widths are covered in both themes; narrow plots are also rastered at 1.5 pixels/point. Saved under `target/develop-c-ux/`:

```text
curve-dark-250-background-edited.png
curve-dark-250-background-identity.png
curve-dark-250-background-parametric.png
curve-dark-250-face-edited.png
curve-dark-250-face-identity.png
curve-dark-250-face-parametric.png
curve-dark-250-hidpi.png
curve-dark-400-background-edited.png
curve-dark-400-background-identity.png
curve-dark-400-background-parametric.png
curve-dark-400-face-edited.png
curve-dark-400-face-identity.png
curve-dark-400-face-parametric.png
curve-dark-inspection.png
curve-light-250-background-edited.png
curve-light-250-background-identity.png
curve-light-250-background-parametric.png
curve-light-250-face-edited.png
curve-light-250-face-identity.png
curve-light-250-face-parametric.png
curve-light-250-hidpi.png
curve-light-400-background-edited.png
curve-light-400-background-identity.png
curve-light-400-background-parametric.png
curve-light-400-face-edited.png
curve-light-400-face-identity.png
curve-light-400-face-parametric.png
curve-light-inspection.png
skin-dark-250-background.png
skin-dark-250-face.png
skin-dark-400-background.png
skin-dark-400-face.png
skin-inspection.png
skin-light-250-background.png
skin-light-250-face.png
skin-light-400-background.png
skin-light-400-face.png
```

## References and scope

- Accepted `SPEC-develop-layers-c1.md` (including Owner decisions) and `REVIEW-develop-layers-fable.md` §2/§5/§6, read from the parent repository.
- Existing `lc-pipeline/src/finish.rs` curve LUT and `primary.rs`/`ucs.rs` selection/conversion algorithms; existing CPU/GPU Point Color and mask overlay paths; existing RAW DNG round-trip fixture conventions in `lc-raw/tests/it/dng.rs`.
- [Capture One: Creating a mask from a color range](https://support.captureone.com/hc/en-us/articles/360002633098-Creating-a-mask-from-a-color-range) and [Achieving Perfect Skin Tones](https://www.captureone.com/blog/achieving-perfect-skin-tones-using-capture-one): picker/range/uniformity workflow and selected-colour versus grey diagnostic. The task's direct Skin Tone support URL was inaccessible; the above primary pages were accessible.
- Existing Glossy Duotone `pipette.svg` and `assets/icons/SOURCES.md`; egui 0.36 painter/interaction APIs already used here. No new source asset or dependency.

A2–A6, B1–B3 and Auto Sync were not implemented. In particular `lc-engine/src/lib.rs` and `cmd/develop.rs` are untouched, as are pc-engine, other pc-ui-egui code, li-seg and li-ai. Only shared icon registry rows changed in pc-ui-egui. The full suites are not all green: Library's fullscreen Right-arrow navigation still reproduces the pre-existing jump to photo 21 instead of 14, recorded in `docs/wip/UPSTREAM-FOLLOWUPS.md:31`. This is outside C1–C3/A1. The editor failures below are in unchanged code outside the permitted two icon-registry files (including its separate vector-icon inventory); all shared picker SVG parsing/silhouette tests pass. No forbidden editor files were changed and no failures were ignored/allowlisted. They need their existing editor/navigation owners or the coordinator.

Editor failures retained in `target/develop-c-full-tests.log`:

- `lib`: `brush_panel::tests::options_bar_edits_are_one_set_brush_per_gesture`
- `lib`: `brush_panel::tests::options_bar_reaches_brush_settings_and_the_preset_library`
- `lib`: `canvas::rotate_view::tests::rotate_view_extra_window_controls_and_text_focus_are_local`
- `lib`: `constructive::tests::constructive_ui_contour_numeric_type_enter_and_escape`
- `lib`: `context_bar::tests::path_and_selection_ai_and_generated_layer_buttons_work`
- `lib`: `control::tests::ui_set_rejects_unknown_fields_before_changing_anything`
- `lib`: `fill_stroke_proxy::tests::toolbar46_frozen_pixel_menu_keys_and_all_chords_parse`
- `lib`: `fill_stroke_proxy::tests::toolbar46_registered_icon_alpha_and_styles_at_16_20_40`
- `lib`: `gallery_ui::tests::remembered_gallery_hex_glow_color_survives_success_restart_and_paint`
- `lib`: `generate_ui::draft_refine::tests::draft_refine_shared_aspect_presets_lock_and_model_grid`
- `lib`: `new_doc_ui::remembered_tests::remembered_new_recent_uses_real_create_close_and_recall_buttons`
- `lib`: `tool_tips::tests::every_tool_has_a_complete_entry`
- `tests/gpu/main.rs`: `pdf_export_app_gpu::pdf_export_app_real_gpu_preview_release_and_menu_dialog`
- `tests/gpu/main.rs`: `psd_advanced_fallback::vector_fallback_app_real_gpu_native_reopen_preview_release_warning_click`
- `tests/gpu/main.rs`: `psd_advanced_fallback::vector_fallback_native_width_brush_blend_repeat_envelope_full_app_gpu_shared_projection`
