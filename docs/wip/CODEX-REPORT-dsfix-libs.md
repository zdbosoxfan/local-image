# DeepSeek vector library findings report

Continued from the existing vector implementation at bc8dec3a. Reviewed all 37 assigned findings against the branch. Changes are limited to finding-listed library files and their own regression tests, plus this requested report. No Cargo manifests, crate registrations, engine aggregate, menus, app/UI wiring or running app were changed. Changes are uncommitted.

All 36 REAL findings have fixes. V490 is FALSE: its named test passed in one actual execution and its tolerance is unchanged. Strict library Clippy has passed for all ten touched crates, and the workspace all-target check passed. All 35 distinct focused tests passed (33 new regressions and two finding-named existing tests).

| ID | Verdict after check | Fix | Test |
| --- | --- | --- | --- |
| V049 | REAL — fixed | Use a fixed five-channel stack buffer in the pixel loop. | dsfix_stack_channel_decode_preserves_rgba_grey_and_cmyk_pixels — passed |
| V217 | REAL — fixed | Compute shared content/appearance/warped-text bounds plus effect margin; intersect with the export scope and embed/place only that rectangle. | dsfix_raster_layer_embeds_only_its_content_bounds; dsfix_raster_text_bounds_include_warped_ink — passed |
| V251 | REAL — fixed | Index XML IDs/labels once, preserving first-ID lookup behavior; reuse the index for layers and warnings. | dsfix_names_use_labels_and_stroked_paths_are_charged_once — passed |
| V497 | REAL — fixed | Pass remaining sample capacity into each edge and recursive sampler; consume it before allocating the next edge. Apply the same bound to contour safety sampling. | dsfix_edge_sampling_consumes_shared_budget_before_allocation — passed |
| V002 | REAL — fixed | Write 13u16 only; verify the document marker remains intact and assert the version-specific error. | tests::preview::rejects_assets_versions_offsets_lengths_and_bombs — passed |
| V021 | REAL — fixed | Charge TextRun storage and owned characters; cap runs at 4096 and coalesce excess ranges into the last run with a warning, preserving all characters. | dsfix_character_attribute_run_budget_preserves_all_characters — passed |
| V052 | REAL — fixed | Truncate duplicate-name prefixes at Unicode scalar boundaries to leave space for the suffix within 1024 UTF-16 units. | dsfix_duplicate_maximum_utf16_name_stays_valid — passed |
| V224 | REAL — fixed | Compute each advanced-layer cache once in emit and pass it to content_blocks and record pixels; also reuse that cache for symbol pixels. | dsfix_shape_cache_block_and_record_share_masked_pixels — passed |
| V222 | REAL — fixed | Raise bounded recovery to 8192 chords; infallible boolean/boolean_n preserve the first finite input on sweep failure instead of deleting it. Invalid input still yields empty. | dsfix_fallback_accepts_more_than_1024_chords_and_keeps_a_finite_cap — passed |
| V205 | REAL — fixed | Accept and use the caller-provided fill rule; update all existing call sites. | dsfix_marquee_in_evenodd_hole_misses — passed |
| V210 | REAL — fixed | Use the clamped slope to scale the quarter-arc handle factor, retaining the original factor at slope zero. | dsfix_arc_slope_changes_handles_but_preserves_endpoints — passed |
| V239 | REAL — fixed | Measure spurious inset contours against closed source subpaths only. | dsfix_inset_ignores_open_internal_line — passed |
| V274 | REAL — fixed | On planar failure, build the filled regions with try_regions instead of immediately returning an empty arrangement. Both failures still return empty through this infallible wrapper; callers needing errors retain try_shape_builder. | Code inspection; workspace compilation. No new sweep-failure fixture. |
| V216 | REAL — fixed | Use the selected artboard rectangle regardless of its children. | dsfix_selected_empty_artboard_exports_its_background — passed |
| V233 | REAL — fixed | Normalize dashes/offset in f64 and reject nonfinite f32 results. Preserve the zero-width hairline policy. | dsfix_denormal_width_cannot_produce_infinite_dash_values — passed |
| V275 | REAL — fixed | Keep bounded per-filter decoding charges, but skip the duplicate final decoded-stream charge. Unfiltered streams are still charged. | dsfix_flate_decoded_bytes_are_charged_once_and_bombs_stay_bounded — passed |
| V276 | REAL — fixed | Validate dictionary dimensions without precharging DCT images; charge the larger dictionary/JPEG pixel extent exactly once. | dsfix_jpeg_pixel_allocation_charged_once_with_header_validation — passed |
| V262 | REAL — fixed | Move the warped-decoration error ahead of the outlines branch. Direct helper calls fail before emitting curves; public export already raster-falls back. | dsfix_warped_decorations_fail_before_curve_output — passed |
| V252 | REAL — fixed | Decode local stroke geometry directly, charging base commands once while retaining the separate expanded-stroke budget. | dsfix_names_use_labels_and_stroked_paths_are_charged_once — passed |
| V285 | REAL — fixed | Track the last non-soft-hyphen glyph while iterating, including spaces/forced breaks, and reuse it for discretionary hyphen metrics. | dsfix_consecutive_soft_hyphens_reuse_the_previous_glyph_width — passed |
| V279 | REAL — fixed | Measure each glyph/decoration once using its actual display transform, warp and faux styling; reject items outside each tile before drawing. | dsfix_overlapping_glyphs_composite_in_layout_order_and_cull_distant_items — passed |
| V280 | REAL — fixed | Composite in glyph layout order, followed by decorations; union coverage only across consecutive visible same-color items. | dsfix_overlapping_glyphs_composite_in_layout_order_and_cull_distant_items — passed |
| V296 | REAL — fixed | Build the continuous scalar field and resampled working buffer lazily for non-Pixel fitting or topology recovery. | dsfix_pixel_palette_trace_does_not_read_the_unused_continuous_field — passed |
| V426 | REAL — fixed | Use rotation zero when the normalized source contour is empty; retain existing bounded collapse/resampling behavior. | dsfix_empty_closed_contour_correspondence_does_not_panic — passed |
| V420 | REAL — fixed | Precompute bounded per-segment arc-length tables once; at uses lookup/interpolation and analytic cubic tangents with endpoint fallback, without path allocation. Straight segments remain exact. | dsfix_spine_inverse_length_matches_curved_metric_and_tangent (analytic length, quarter positions, tangents and endpoints) — passed |
| V451 | REAL — fixed | Include u=1 for the last segment of open art tracks, inside the existing vertex budget. | dsfix_bent_open_art_includes_its_last_corner — passed |
| V473 | REAL — fixed | Check failed coordinate mapping after fitting even when fit_to_cubic returns None; distinguish coordinate failure from exhausted work/depth/node budgets. | dsfix_coordinate_failure_is_distinct_from_work_budget — passed |
| V478 | REAL — fixed | Use inclusive coordinate-range overlap for subpath and brush bounds, including degenerate boxes. | dsfix_degenerate_subpath_bounds_are_culled_by_distance — passed |
| V448 | REAL — fixed | Resolve endpoint-to-curve hits to an endpoint index when coincident and mark both endpoints used. | dsfix_curve_bridge_marks_coincident_endpoint_used — passed |
| V456 | REAL — fixed | Reject faces whose bounds miss the pointer-segment box before computing intersections; retain zero-width/height gesture boxes. | dsfix_gesture_crosses_faces_even_with_horizontal_zero_height_bounds — passed |
| V468 | REAL — fixed | Build edge boundary tokens from only the requested source and reject missing source keys. | dsfix_coincident_edges_retain_the_requested_source — passed |
| V493 | REAL — fixed | Return no pins for count zero before seeding the center. | dsfix_zero_auto_pins_respects_requested_count — passed |
| V486 | REAL — fixed | Find the first and last open flattened subpaths for arrowheads. | dsfix_arrowheads_find_open_subpath_between_closed_subpaths — passed |
| V500 | REAL — fixed | Hide the radius handle on ellipse callouts. | dsfix_ellipse_callout_has_no_inert_radius_handle — passed |
| V502 | REAL — fixed | Use move_anchors_unchecked inside the already checked bend transaction. | dsfix_straight_segment_bend_keeps_translation_and_atomic_errors — passed |
| V484 | REAL — fixed | Sort midpoint values together with their owning color stops, defaulting omitted midpoint entries to 0.5. | dsfix_unsorted_gradient_midpoints_follow_their_stops — passed |
| V490 | FALSE — named test passes | No tolerance or implementation change. | tests::analytic_curve_bounds_and_sparse_shape_cache: passed once (1 test). |

## Validation and handoff notes

All Cargo commands use Rust 1.98.1, `--offline`, `CARGO_BUILD_JOBS=3`, and `RUSTC_WRAPPER=/home/zdavidson/.local/share/local-image-dev/bin/li-rustc-gate`. PATH and RUSTC are pinned to the rustup toolchain: Fedora also installs 1.98.1 but its metadata is incompatible with the rustup Clippy driver. Clippy uses `target/clippy` to keep the compiler artifacts separate.

- Workspace `cargo check --workspace --all-targets`: passed using `CARGO_TARGET_DIR=target/clippy`. The initial attempt found that the engine includes the blend source directly without a kurbo dependency; the spine fix now uses existing geometry APIs and needs no manifest edit.
- Strict Clippy (`--lib ... -- -D warnings`): passed for photocraft-affinity, photocraft-doc, photocraft-io, photocraft-pathops, photocraft-pdf, photocraft-svg, photocraft-text, pc-trace, photocraft-vecfx and photocraft-vector.
- An additional all-target strict-Clippy attempt stops on existing `clippy::duplicate_mod` in pc-trace: `src/cleanup/../../tests/cleanup/../it/common/mod.rs` is included by multiple pre-existing test modules. That unrelated wiring is outside the assigned files. No lint was suppressed to hide it.
- Only `dsfix` regressions, the corrected Affinity version-rejection test, and the named V490 test are run. No whole-suite or GPU tests, GUI windows, or app termination.
- Focused tests: 35 distinct tests passed. This comprises 33 new regressions, the corrected Affinity version-rejection test, and V490's named test. Failed SVG fixture attempts were corrected and only the failed fixture was rerun; already passing tests were not repeated. V490 ran once; earlier compilation failures never executed it.
- Late PDF/SVG changes passed strict Clippy with `--all-targets -- -D warnings`. Detached text-view and design-shape targets also passed strict Clippy after replacing the new test's constant-size chunks_exact call with as_chunks.
- `rustfmt --check --edition 2024` on every modified/new Rust file and `git diff --check`: passed.
- Scope audit: 33 finding-listed files changed, plus their own tests (`pc-affinity/tests/scene.rs`, `pc-affinity/tests/text.rs`, `pc-vector/tests/dsfix_design_shapes.rs`) and this report. The new design-shape test harness includes the detached source without changing production crate registration. The pre-existing task document remains untracked.

Focused target coverage: pc-trace lib (1), pc-doc lib (1), pc-io lib (1), pc-pathops lib (4), pc-pdf lib (6), pc-svg lib (2), pc-text lib (1) and test view (1), pc-vecfx lib (10, including live_corners), pc-vector lib (3 new plus V490) and test dsfix_design_shapes (1), and pc-affinity (2 new plus the named existing version test).

Commands used the common environment described above. Library tests selected `--lib dsfix`; Affinity selected its new synthetic tests and the version-rejection test separately. Detached targets selected `-p photocraft-text --test view dsfix` and `-p photocraft-vector --test dsfix_design_shapes dsfix`. The named UNSURE test used `-p photocraft-vector --lib tests::analytic_curve_bounds_and_sparse_shape_cache -- --exact`. No unfiltered test target ran.

Successful validation logs are in `/tmp/dsfix-libs-check-clean.log`, `/tmp/dsfix-libs-clippy-lib.log`, `/tmp/dsfix-libs-late-clippy.log`, `/tmp/dsfix-libs-detached-clippy-final.log`; focused results are in the matching `/tmp/dsfix-libs-*test*.log` and `/tmp/dsfix-libs-remaining-final.log` files.

V262 is real inside the text helper: the previous guard was unreachable. The public exporter already embeds all warped text as PNG before calling that helper. The regression verifies both the helper error before any curve output and the existing successful public PNG fallback, with text_as_curves both enabled and disabled.

V420 uses a 1024-interval numeric table per curved segment. Total lengths retain V1's native metric; at interpolates the inverse metric and evaluates the original cubic/tangent. This trades the former repeated allocations/integration for bounded lookup. Straight segments and endpoints are exact.

Outside-scope consistency follow-up for V484: `crates/pc-compose/src/gradient_fill.rs`, `Ramp::new`, also sorts color stops without their midpoint values. Apply the same stable stop+midpoint pairing used in `Paint::from_fill` here: pair each original stop i with `midpoints.get(i).copied().unwrap_or(0.5)`, sort the pairs by position, and populate the sorted ramp colors and interval midpoints together. That file was not in this job's findings, so it is intentionally left for the coordinator.
