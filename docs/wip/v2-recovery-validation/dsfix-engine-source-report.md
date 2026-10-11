# DeepSeek engine finding fixes

Continued the existing `bc8dec3a` vector implementation. Rechecked all 26 assigned findings in `split-engine.jsonl`: 23 fixed, one already prevented by existing validation (V027), one coordinated UI protocol change deferred under the file allowlist (V132), and one unresolved GPU verdict under the no-GPU rule (V158). Changes are uncommitted and limited to the named files, their own regression tests, and this report. No aggregate registrations, UI wiring, menus, manifests, GUI windows, running-app changes, whole-suite tests, or GPU runs.

| id | Verdict after check | Fix | Test |
| --- | --- | --- | --- |
| V059 | REAL | Use maximum right/bottom edge across every input when deriving default spacing. | `dsfix_v059_spacing_uses_farthest_edge_on_both_axes` |
| V102 | REAL | Cache exact Expand availability per document snapshot, capture/revision/selection, and options; publication still prepares afresh. One thread-local entry holds a weak source reference and the reason, never prepared raster caches. | `dsfix_v102_expand_availability_reuses_preparation_and_invalidates` |
| V027 | NOT REAL in this branch | Existing `validate_rect` already calls `finite` on every value and both far edges, enforcing ±1,000,000 before rounding/casting. No production change; retained a focused regression proving the existing guard. | `dsfix_v027_array_artboard_coordinates_already_bounded` |
| V044 | REAL | Reject an explicitly supplied non-string boolean operation; only absence defaults to union. | `dsfix_v044_boolean_op_rejects_present_nonstrings` |
| V029 | REAL | Serialize default persona and saved set/tool fields so generic preference get/set can address them. | `dsfix_v029_default_persona_preferences_are_addressable` |
| V030 | REAL | Reset the active persona's saved set. | `dsfix_v030_v031_toolbar_edit_and_reset_follow_vector_persona` |
| V031 | REAL | Copy/edit the current persona's custom set and use that persona's canonical tool registry. | Same V030/V031 regression checks custom persona, ordering, hiding, and Pixel set preservation. |
| V026 | REAL | Treat Symbol content as a whole-object transform for linked raster masks. | `dsfix_v026_symbol_linked_mask_moves_whole_with_selection` |
| V111 | REAL | Reject clip release when vector mask density/feather cannot be represented by the released path. | `dsfix_v111_release_rejects_density_and_feather_without_mutating_source` |
| V060 | REAL | Assert knot counts in every PSD round-trip subpath before comparing zipped knots. | Strengthened/renamed `dsfix_v060_clip35_svg_pdf_psd_exports_keep_native_vector_masks_and_format_roundtrip` |
| V122 | REAL | Copy fill rule/inversion onto every scissors piece. | `dsfix_v122_v123_split_and_erase_preserve_fill_semantics` |
| V123 | REAL | Copy fill rule/inversion onto every single-contour eraser output. | Same V122/V123 regression verifies both eraser pieces. |
| V093 | REAL | Cache other committed envelope projections by source snapshot, revision, and edited group set; drag spec changes reproject only edited groups. Cache is bounded to one background per thread. | `dsfix_v093_envelope_background_reused_across_drag_and_invalidated` |
| V089 | REAL | Assert CPU/GPU pixel buffer lengths before zip. | Compiled only; GPU execution prohibited. |
| V137 | REAL | Index source knots once by anchor bits, normalizing signed zero; retain deterministic coincident/reversed endpoint priority. | `dsfix_v137_indexed_provenance_keeps_coincident_and_signed_zero_rules`, including 16,384 unique knots. |
| V132 | REAL — coordinator change required | Existing UI explicitly deserializes `bytes` into `Vec<u8>`; changing the command alone breaks export. Exact coordinated patch below. | Caller and command inspected; no incompatible partial protocol change. |
| V187 | REAL | Check the knot budget before beginning a new subpath; continuing an existing endpoint remains allowed. | `dsfix_v187_begin_new_subpath_checks_knot_budget_but_allows_endpoint` |
| V188 | REAL | Increment generation when release removes a zero-length line endpoint. | `dsfix_v188_zero_length_line_release_changes_generation` |
| V149 | REAL | Reject equal Sculpt endpoints during capture validation for open and closed contours. | `dsfix_v149_sculpt_equal_endpoints_fail_at_capture_for_open_and_closed_paths` |
| V170 | REAL | Check raw FX byte length before appearance validation or JSON serialization. | `dsfix_v170_raw_effects_budget_precedes_serialization` checks error ordering and the accepted budget boundary. |
| V172 | REAL | Placement checks the insertion anchor's effective locks without recursing into untouched descendants. | `dsfix_v172_symbol_placement_ignores_locked_sibling_descendants` |
| V178 | REAL | Construct Group metadata explicitly instead of cloning children to overwrite them. This shared source is compiled by `photocraft-doc`. | Workspace all-target check and strict clippy include `photocraft-doc`. |
| V199 | REAL | Eliminate both full-document clones; compose projected layers in a minimal render document retaining required settings, profiles, light, patterns and channel resources. | `dsfix_v199_minimal_symbol_render_matches_full_document_compositor` |
| V165 | REAL | Ignore the RTX 5090/Vulkan hardware test by default with an explicit reason. | Compiled only; no GPU run. |
| V204 | REAL | Cache curve control-hull bounds and reject queries outside tolerance-inflated bounds before nearest-point calls. | `dsfix_v204_curve_nearest_skips_distant_bounds_but_keeps_tolerance_edge` counts adapter calls across 100 curves. |
| V158 | UNSURE — prohibited/unregistered GPU gate | No speculative pointer edit. The named test requires the RTX app harness and is not registered in this branch's UI crate. Its required run conflicts with the explicit no-GPU rule. | Coordinator must register/run `pdf_import_full_app_rtx_pointer_preview_release_select_edit_undo_cancel` once on the RTX machine. |

## Exact coordinator changes

**V132:** Change `crates/pc-engine/src/vector_packages/pdf_export.rs::run` to return `{"bytes": photocraft_paint::tile::b64_encode(&output.bytes), "encoding":"base64", "warnings":output.warnings, "mime":"application/pdf"}`. In `crates/pc-ui-egui/src/vector_packages/pdf_export.rs::State::export`, read the bytes string and decode it with `base64::engine::general_purpose::STANDARD.decode(...)` (import `base64::Engine`), returning an engine error for a missing/invalid string. Update the engine export command regression and UI export tests to assert the encoding and decoded `%PDF-` header. The engine already depends on `photocraft-paint`, and the UI already depends on `base64 = "0.22"`; no manifest change is needed. This must land together because the current UI's `serde_json::from_value(output["bytes"].clone())` requires a numeric array. The UI file is outside this job's named-file allowlist, so neither half was applied here.

**V158:** After the integration job's UI registration, run the named RTX test once as part of its allowed final GPU gate. Preserve the existing pointer coordinates unless that run demonstrates a miss; if it does, derive the press position from the placed group's actual visible bounds and use that point for the drag. This job cannot settle its runtime verdict under the no-GPU rule.

## Validation

Passed:

- `cargo check --workspace --all-targets --offline` (one initial regression-test import error corrected, then passed).
- `cargo clippy -p photocraft-engine -p photocraft-doc --all-targets --offline -- -D warnings`.
- Changed-file `rustfmt --check` and `git diff --check`.

Initial focused run: `cargo test -p photocraft-engine --lib dsfix_ --offline` selected exactly 19 added/strengthened CPU regressions; 18 passed. The V027 test used an invalid positive far-edge boundary. Reading the pre-existing rectangle validator settled V027 as NOT REAL; its redundant added guard was removed and the accepted boundary fixture corrected. The corrected V027 regression passed on its isolated rerun (1 passed, 1,280 filtered out). Final strict clippy passed again after that correction. All 19 focused CPU regressions passed across these runs; no already-passing regressions were rerun.

Every build uses Rust 1.98.1, `RUSTC_WRAPPER=/home/zdavidson/.local/share/local-image-dev/bin/li-rustc-gate`, `CARGO_BUILD_JOBS=3`, and `--offline`. Workspace check used Fedora's 1.98.1. Clippy initially encountered incompatible Fedora/rustup artifacts; the successful clippy invocation used `rustup run 1.98.1 cargo`, `RUSTC=/home/zdavidson/.rustup/toolchains/1.98.1-x86_64-unknown-linux-gnu/bin/rustc`, and `CARGO_TARGET_DIR=target/clippy`. The focused tests use the matching rustup compiler as well. The initial plain-system `cargo +1.98.1` attempt was rejected before compiling; system cargo is not a rustup proxy.
