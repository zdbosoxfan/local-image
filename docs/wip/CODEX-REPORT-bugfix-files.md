Continued the existing branch at `c8ca0b63`, including hotfix `e698208d` and the known-failure fixes. Rechecked the assigned findings against that code, rather than the review snapshot. Changes remain uncommitted. The pre-existing change to `CODEX-TASK.md` was preserved.

| Item | Verdict after current-code recheck | Fix | Regression / existing coverage |
|---|---|---|---|
| O-011; subreview 1 | Real: a channel-free record still reached whole-rectangle conversion. | Empty records produce an empty surface. A shared 512 MiB conversion budget includes tile padding, masks, merged pixels and generated shape caches; extraction reserves fallibly. | `channel_free_huge_layer_is_empty`, `layers_share_one_conversion_budget`; existing PSD import/pixel tests. |
| O-012; subreview 2 | Real: short records could move the cursor backwards. | Require an eight-byte record header, input-derived count, at most 4096 annotations, and text ending inside its record. | `short_unknown_entry_cannot_rewind_cursor`, including the exact review payload; existing annotation round trips. |
| O-017; subreview 4 | Real: the reference graph counted marker art once while usvg instantiated it per vertex. | Charge marker art recursively against vertex expansion before usvg; include inherited/CSS marker declarations, with no multiplication for unused markers. | `marker_work_is_charged_per_vertex_before_usvg`, `unused_marker_definitions_do_not_multiply_path_work`, `nested_css_marker_chains_are_charged_before_usvg`, `markers_on_use_charge_the_referenced_path_vertices`; SVG import tests. |
| O-017; subreview 5 | Real: both the local matcher and usvg's simplecss matcher could backtrack. | Memoize local `(selector part, node)` matching. Admit at most eight selector compounds, 256 rules, 1024 bytes per selector and a bounded descendant/class matching-work estimate using the actual input depth before either matcher. | `exponential_css_is_rejected_before_either_style_matcher`, `short_documents_admit_bounded_longer_descendant_selectors`, `large_class_compounds_are_charged_before_style_matching`, `inherited_markers_inside_their_own_art_do_not_recurse`; existing SVG CSS/text/import tests. |
| O-018; subreview 6 | Real: unresolved objects cost zero, and the decoder's xref/repair interpretation could select bodies preflight skipped. | Unresolved graph references and duplicate identities are errors. Build the decoder's xref from exactly the lexically admitted object bodies, including unpacked object-stream members, before hayro constructs its page tree. Recheck decoded dictionaries against admitted values. Decode PDF information and scenes on cancellable workers. | `pdf_unresolved_content_references_are_not_zero_cost`, `pdf_duplicate_object_identity_is_rejected`, `pdf_xref_hidden_content_cannot_bypass_preflight`, `pdf_worker_cancel_and_stale_destination_do_not_publish`; complete PDF import tests. |
| O-019; subreview 3 | Real: each pattern could expand sparse data into a huge blank plane, and multiple sections had separate allowances. | Shared 128 MiB decoded-plane budgets and 4096-pattern limits; destination/input plausibility check before allocating; shared conversion budgets include native tile padding. ABR sections share their pattern budget. | `tiny_plane_cannot_expand_to_huge_blank_pattern`, `pattern_sections_share_a_decoded_plane_budget`, `pattern_count_is_bounded_for_standalone_and_global_blocks`; pattern/ABR round trips. |
| O-019; subreview 7 | Real: lossy DNG JPEG dimensions were not constrained to their tile. | Read JPEG headers with width/height limits before decoding. Bound the sum of retained parallel chunk outputs plus the final RAW buffer to 1 GiB. | `jpeg_dimensions_cannot_exceed_dng_tile`; existing RAW/DNG tests. |
| O-019; subreview 8 | Real: the thumbnail PSD reader decoded up to 56 planes while using at most five. | Consume unused channels without expanding/copying them. Budget decoded planes plus output and reject implausible RLE expansion before plane allocation. | `tiny_rle_payload_cannot_claim_large_zero_padded_planes`, `unused_raw_channels_are_consumed_without_decoding`; existing PSD thumbnail tests. |
| O-041 | Real: a panic released the worker slot but killed the worker and never completed the owner's pending request. | Guard pool jobs and engine render jobs, send explicit failure completion, clear matching pending UI state, and continue servicing the queue. | `inline_panic_reports_completion_and_releases_slot`, `native_worker_survives_panic_and_runs_next_job`; existing pool/render tests. |
| O-042; subreview 12 | Real: direct Place, dialog staging and clipboard-file decoding bypassed the job guard; Library/filmstrip callbacks could die. | Prepare Place in the established job system and apply one undoable edit. Move copied-file Paste and SVG/Affinity/PDF scene decoding to workers, pass cancellation flags, discard cancelled/stale results, and guard synchronous imports, clipboard callbacks, Library layers, filesystem callbacks and filmstrip workers. Dialog workers have a 30-second timeout and four-worker admission cap. | `svg_place_is_a_job_and_cancel_never_adds_layers`, `synchronous_place_is_one_undo_step`, `prepared_svg_publication_records_one_command_without_redecoding`, `copied_file_paste_decodes_on_a_cancellable_job`, worker panic/cancellation tests, SVG stale-destination test, PDF/Affinity cancellation tests; existing UI placement/clipboard/Library tests. |
| O-043; subreview 9 | Real: PSD `Idnt` and loaded native linked paths could trigger ambient reads. | PSD IDs are identifiers restricted to ASCII letters/digits, `_`, `-`, `$`. Loaded native links use cached art until explicit Place Linked/Relink/Convert to Linked grants process-local access. Every disk read validates regular files, rejects device/UNC paths and symlink targets in pseudo-filesystems, caps input at 512 MiB, and uses nonblocking Unix opens to avoid FIFO races. | `psd_identifiers_never_become_paths`, `loaded_link_needs_explicit_relink_before_reading`, device/UNC/FIFO/oversize read tests; existing linked/embedded smart-object round trips. |
| Subreview 10 | Real: repeated presets copied the same sampled tip without a shared conversion allowance. | A 128 MiB running tip/texture conversion budget is charged before copies; excess tips use a round fallback with a warning. Descriptor sections share the existing 20,000-brush cap. | `tip_duplicates_consume_one_running_budget`, `descriptor_sections_share_the_brush_count_limit`; existing ABR mapping/preset painting tests. |
| Subreview 11 | Real: large finite percentages narrowed to infinity and dab rectangle arithmetic overflowed. | Clamp before narrowing, normalize angles, bound scatter, use saturating bounds and omit nonfinite/negative/oversized dabs. | `finite_huge_percentages_stay_finite`, `non_finite_dabs_have_no_pixel_reach`; existing dynamics/render/continuous stroke tests. |
| Lower: flattened PSD allocations | Real: separate decode/extraction calls could each allocate gigabytes. | Shared import allowance, 512 MiB per-decode backstop, combined extraction allowance, and no second RGBA decode after merged decode refusal. Layout arithmetic and encoding keep their existing allowance. | `oversized_flattened_psd_is_refused_before_decode`, `import_decode_budget_is_checked_before_every_compression_method`, shared-budget test; existing merged/pixel tests. |
| Lower: TIFF chunk inference | Real: each missing byte count scanned all offsets. | Sort/deduplicate once only when inference is needed; use binary search for the next larger offset. | `many_missing_byte_counts_complete_within_the_work_budget` (30,000 chunks), `inferred_lengths_handle_unsorted_and_duplicate_offsets`; TIFF tests. |
| Lower: workspace TIFF JPEG feature | Real: feature unification enables TIFF's JPEG decoder despite pc-codecs opting out. | Independently inspect each chunk's SOF dimensions before invoking TIFF; support abbreviated JPEG whose tables are in JPEGTables. | `jpeg_dimensions_must_fit_the_declared_tiff_strip_before_decode`; complete TIFF codec tests. |
| Lower: deep EXR overhead | Real: pixel count/offset/chunk tables were omitted from the allocation allowance. | Include 32 bytes per pixel in header admission and combined sample/table accounting; guard the public deep decoder. | `deep_exr_per_pixel_tables_share_the_sample_budget`; existing deep EXR corpus/structural tests. |
| Lower: PNM temporary allocation | Real: ASCII and binary integer paths store samples in a u32 temporary. | Account for six bytes per sample before temporary/final conversion, including PAM. | `ascii_u32_temporary_is_in_the_allocation_budget`; existing PNM format/limit/round-trip tests. |
| Lower: preset name deduplication | Real: suffix search repeatedly scanned all prior names. | Case-insensitive hash membership and monotonic suffix cursors. | `duplicate_name_suffixes_advance_without_rescanning_the_group` (20,000 names); existing naming/import tests. |
| Lower: TIFF next-IFD overflow | Real in debug: attacker-controlled entry count overflowed next-position multiplication/addition. | Saturating offset arithmetic makes the unavailable next pointer fail the existing bounds check. | `huge_bigtiff_entry_count_does_not_overflow_next_offset`; TIFF parser tests. |
| Lower: NEF predictor overflow | Real in debug: corrupted differences overflowed i32 accumulators. | Checked predictor addition returns a corruption error. | `damaged_nef_predictor_is_an_error_at_integer_overflow`; existing Nikon lossless/lossy predictor tests. |
| Lower: RAW maker-note preview offset | Real in debug: maker-note base plus LONG8 preview offset could overflow. | Saturating addition yields an out-of-file offset, refused by the TIFF parser. | `nikon_preview_offset_cannot_wrap_its_maker_note_base`; existing preview tests. |
| Lower: native tile-coordinate overflow | Real: extreme manifest tile coordinates could overflow before allocating a tile. | Check tile multiplication and upper edge before fetching bytes or constructing a tile. | `extreme_tile_coordinates_are_rejected_before_fetch`; native format tests. |
| Lower: plugin memory per worker | Real: one 512 MiB wasm allowance could be multiplied by every Rayon thread. | Run at most two instances/bands concurrently regardless of the host pool size; preserve existing fuel/memory/deadline limits. | `large_rayon_pool_does_not_multiply_wasm_instances` uses a 16-thread pool; existing plugin execution/resource-limit tests. |
| Lower: SVG sniff before raster detection | Real: a raster containing `<svg` in metadata could be misrouted. | Require an XML-looking bounded leading header before looking for the SVG root; retain bounded SVGZ handling. | `raster_metadata_cannot_select_svg_decoder`; existing format/vector-routing tests. |

The engine/UI/app changes are the necessary import boundaries and worker publication paths. The only dependency additions are existing workspace `libc` for Unix nonblocking reads and `photocraft-affinity` for direct worker decoding; no dependency versions changed. New messages are recorded in `pc-ui-egui/src/i18n/pending_vector.rs` and the Library i18n pending list.

Explicit relinking follows Photoshop's documented **Relink to File** operation; a file embedded in a document does not grant ambient disk access. See [Adobe's linked Smart Object guidance](https://helpx.adobe.com/photoshop/desktop/create-manage-layers/smart-objects/update-linked-smart-objects.html). The PDF chooser retains page/box/DPI selection and destination-DPI placement; see [Adobe's PDF import guidance](https://helpx.adobe.com/photoshop/desktop/create-open-import-images/import-files/open-pdf-files-in-photoshop.html).

Default imports now have conservative fixed memory limits. Legitimate very large images/files or unusually complex CSS/marker art may be refused, and PSD's existing warning/omitted-pixels fallback remains in use. Duplicate PDF object identities (including ambiguous incremental updates) are refused. Native third-party decoder calls cannot be forcibly interrupted in the middle of a call: cancellation prevents publication, admission checks bound known amplification, and dialog timeouts release the UI. Web builds retain the existing inline-job behavior.

No existing test was removed or weakened. No GUI window was opened, and neither Local Image nor ComfyUI was stopped.

TEST-CHANGED: `crates/lc-preview/tests/pool_coverage.rs::a_panicking_job_frees_its_slot` formerly required an inline job panic to escape. O-041/O-042 require containing that panic. It now requires a nonpanicking run, failure completion with the correct slot/key and a message, and no successful completion; its original subsequent-job/slot-reuse assertions are retained.

Validation used native Fedora Rust 1.98.1 and the shared compiler gate, offline, without a `CARGO_BUILD_JOBS` cap. Native `/usr/bin/cargo` does not implement rustup's `+toolchain` syntax; `rustc --version` verifies the required compiler. Formatting is not a build and has no offline flag.

Exact validation commands:

```bash
rustc --version
PATH=/home/zdavidson/.local/share/local-image-dev/bin:$PATH RUSTC_WRAPPER=li-rustc-gate cargo check --workspace --offline > /tmp/li-files-workspace-check-final.log 2>&1
PATH=/home/zdavidson/.local/share/local-image-dev/bin:$PATH RUSTC_WRAPPER=li-rustc-gate cargo test --offline --no-fail-fast -p photocraft-io -p photocraft-psd -p photocraft-svg -p photocraft-pdf -p lightcraft-raw -p lightcraft-codecs -p lightcraft-preview -p lightcraft-engine -p lightcraft-ui-egui -p lightcraft-tiff -p photocraft-format -p photocraft-engine -p photocraft-ui-egui -p photocraft-paint -p photocraft-codecs -p photocraft-plugins -p local-image > /tmp/li-files-tests-final.log 2>&1
PATH=/home/zdavidson/.local/share/local-image-dev/bin:$PATH RUSTC_WRAPPER=li-rustc-gate cargo test --offline -p lightcraft-preview --test pool_coverage a_panicking_job_frees_its_slot > /tmp/li-files-pool-corrected-test.log 2>&1
PATH=/home/zdavidson/.local/share/local-image-dev/bin:$PATH RUSTC_WRAPPER=li-rustc-gate cargo test --offline -p lightcraft-ui-egui --lib tests_people::ds_finding_large_people_query_keeps_ui_cancellable -- --test-threads=1 > /tmp/li-files-people-timing-rerun.log 2>&1
PATH=/home/zdavidson/.local/share/local-image-dev/bin:$PATH RUSTC_WRAPPER=li-rustc-gate cargo test --offline -p photocraft-psd --lib > /tmp/li-files-psd-final-tests.log 2>&1
PATH=/home/zdavidson/.local/share/local-image-dev/bin:$PATH RUSTC_WRAPPER=li-rustc-gate cargo test --offline -p photocraft-svg > /tmp/li-files-svg-final-tests.log 2>&1
PATH=/home/zdavidson/.local/share/local-image-dev/bin:$PATH RUSTC_WRAPPER=li-rustc-gate cargo test --offline -p photocraft-ui-egui --lib -- pdf_worker_cancel_and_stale_destination_do_not_publish worker_decodes_without_editing_and_rejects_stale_destination draft_refine_mock_comfy_uses_source_and_library_results_can_be_used_or_opened copied_file_paste_decodes_on_a_cancellable_job import_worker::tests:: jobs_ui:: --test-threads=1 > /tmp/li-files-ui-corrected-tests.log 2>&1
PATH=/home/zdavidson/.local/share/local-image-dev/bin:$PATH RUSTC_WRAPPER=li-rustc-gate cargo test --offline -p photocraft-ui-egui --test gpu svg_import_app_acceptance::svg_import_app_gpu_live_preview_release_editability_warning_and_undo -- --test-threads=1 > /tmp/li-files-svg-ui-gpu-rerun.log 2>&1
PATH=/home/zdavidson/.local/share/local-image-dev/bin:$PATH RUSTC_WRAPPER=li-rustc-gate cargo test --offline -p photocraft-engine --lib -- file_worker_regressions prepared_publication_regressions vector_packages::svg_import::tests:: > /tmp/li-files-engine-svg-final-tests.log 2>&1
PATH=/home/zdavidson/.local/share/local-image-dev/bin:$PATH RUSTC_WRAPPER=li-rustc-gate LOCAL_IMAGE_DATA_DIR=/tmp/li-files-ai-test-profile LOCAL_IMAGE_MODELS_DIR=/tmp/li-files-ai-test-profile/models cargo test --offline -p photocraft-ui-egui --lib generate_ui::draft_refine::tests::draft_refine_mock_comfy_uses_source_and_library_results_can_be_used_or_opened -- --test-threads=1 > /tmp/li-files-ai-profile-rerun.log 2>&1
PATH=/home/zdavidson/.local/share/local-image-dev/bin:$PATH RUSTC_WRAPPER=li-rustc-gate cargo clippy --offline --all-targets -p photocraft-io -p photocraft-psd -p photocraft-svg -p photocraft-pdf -p lightcraft-raw -p lightcraft-codecs -p lightcraft-preview -p lightcraft-engine -p lightcraft-ui-egui -p lightcraft-tiff -p photocraft-format -p photocraft-engine -p photocraft-ui-egui -p photocraft-paint -p photocraft-codecs -p photocraft-plugins -p local-image -- -D warnings > /tmp/li-files-clippy-pass.log 2>&1
PATH=/home/zdavidson/.local/share/local-image-dev/bin:$PATH RUSTC_WRAPPER=li-rustc-gate cargo clippy --offline --all-targets -p lightcraft-preview -- -D warnings > /tmp/li-files-pool-clippy.log 2>&1
PATH=/home/zdavidson/.local/share/local-image-dev/bin:$PATH RUSTC_WRAPPER=li-rustc-gate cargo fmt --all --check > /tmp/li-files-fmt-check-final.log 2>&1
git diff --check > /tmp/li-files-diff-check-final.log 2>&1
```

Compiler output:

```text
rustc 1.98.1 (48a229cea 2026-09-01) (Fedora 1.98.1-1.fc44)
```

Workspace check, strict Clippy over all targets in all 17 touched crates, strict Clippy after the pool test correction, formatting and whitespace checks all exited 0. Exact completion output:

```text
workspace check: Finished `dev` profile [optimized + debuginfo] target(s) in 11m 30s
all touched crates Clippy: Finished `dev` profile [optimized + debuginfo] target(s) in 11m 09s
corrected pool test Clippy: Finished `dev` profile [optimized + debuginfo] target(s) in 31.43s
fmt --check: no output (exit 0)
git diff --check: no output (exit 0)
```

The broad test run executed the new regressions and the existing unit/integration/doc tests: 6765 passed, eight failed across six targets, and 118 existing ignored tests remained ignored. Failures and subsequent changes were handled as follows:

| Broad-run failure | Resolution and final evidence |
|---|---|
| Pool panic expectation | The single declared `TEST-CHANGED` above; targeted rerun passed. |
| People UI two-second timing assertion | Unchanged isolated rerun passed in 0.33 s with the original two-second threshold. This supports parallel-suite contention as the cause. |
| PSD large-PSB layout test | Kept its original expectation. Apply the new 512 MiB bound to import decoding, while retaining existing layout/encoding allowances. All 160 PSD unit tests passed. |
| SVG corpus ratchet | Refined admission to use observed depth and allow bounded longer selectors; inherited markers inside their own art are suppressed consistently with usvg. The unchanged 400-fixture ratchet and the full SVG suite passed. |
| Two new stale-destination worker tests | Corrected the new tests' command identifier from `layer.new` to the registered `layer.new.layer`. Both passed. No existing expectation changed. |
| SVG app GPU command-history assertion | Prepared scene publication now executes the usual command bookkeeping without decoding again. The unchanged GPU acceptance test and a new engine regression passed. |
| Mock-Comfy refinement test | The unchanged test failed again using the ordinary profile, then passed with isolated data/model directories. This supports profile dependence; the precise profile setting responsible was not isolated. No AI implementation or test changed. |

All eight broad-run failures now have passing targeted reruns, and all new regressions named in the item table appear in passing output. Reruns were limited to failures and code changed while resolving them. The broad run's historical exit code remains 101; it is not represented as a clean rerun. The corrected UI subset's historical exit 101 is also retained because its mock-Comfy test failed before the successful isolated-profile rerun. Final PSD, SVG, engine SVG/Place, pool, People, GPU acceptance and isolated mock-Comfy runs each exited 0. No failure remains without passing final evidence.

Exact broad-run per-target output (individual tests and failure details are in the raw transcript):

```text
Finished `test` profile [optimized + debuginfo] target(s) in 11m 49s
     Running unittests src/lib.rs (target/debug/deps/lightcraft_codecs-073c7676ccf6dc07)
test result: ok. 22 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.15s
     Running tests/coverage/main.rs (target/debug/deps/coverage-1befd3b5bfb62cfa)
test result: ok. 58 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
     Running tests/it/main.rs (target/debug/deps/it-d818215d014da249)
test result: ok. 29 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.43s
     Running unittests src/lib.rs (target/debug/deps/lightcraft_engine-25f0a96ea3a2dfe6)
test result: ok. 454 passed; 0 failed; 4 ignored; 0 measured; 0 filtered out; finished in 140.52s
     Running tests/coverage/main.rs (target/debug/deps/coverage-9df006139ebbcb6a)
test result: ok. 381 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.79s
     Running tests/develop_backend.rs (target/debug/deps/develop_backend-81c1c13fec422a8c)
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.42s
     Running tests/panic_hunt.rs (target/debug/deps/panic_hunt-f31647c1f91d84a0)
test result: ok. 2 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 1.07s
     Running unittests src/lib.rs (target/debug/deps/lightcraft_preview-77955d48f366a80a)
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.30s
     Running tests/pool_coverage.rs (target/debug/deps/pool_coverage-a2a94efd576ccd19)
test result: FAILED. 20 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.11s
     Running unittests src/lib.rs (target/debug/deps/lightcraft_raw-46cfb654e08e14c3)
test result: ok. 103 passed; 0 failed; 3 ignored; 0 measured; 0 filtered out; finished in 1.30s
     Running tests/coverage/main.rs (target/debug/deps/coverage-7366ea385b48c746)
test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
     Running tests/it/main.rs (target/debug/deps/it-66747731127dffd6)
test result: ok. 58 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 4.40s
     Running unittests src/lib.rs (target/debug/deps/lightcraft_tiff-d679e91a63a878d3)
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running tests/coverage/main.rs (target/debug/deps/coverage-83e3c332fd0cf905)
test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running tests/roundtrip.rs (target/debug/deps/roundtrip-deed13a647ad8c5a)
test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s
     Running unittests src/lib.rs (target/debug/deps/lightcraft_ui_egui-ac1661cc191becfc)
test result: FAILED. 257 passed; 1 failed; 4 ignored; 0 measured; 0 filtered out; finished in 391.57s
     Running unittests src/main.rs (target/debug/deps/local_image-2b9cac31ead8dc66)
test result: ok. 90 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 10.24s
     Running tests/mac_menu_appkit.rs (target/debug/deps/mac_menu_appkit-fc99f3d38d2dc227)
     Running unittests src/lib.rs (target/debug/deps/photocraft_codecs-d88a3eafedebd5de)
test result: ok. 36 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.69s
     Running tests/coverage/main.rs (target/debug/deps/coverage-d586d4c3705833ce)
test result: ok. 19 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running tests/it/main.rs (target/debug/deps/it-be0e487556ccff0b)
test result: ok. 508 passed; 0 failed; 3 ignored; 0 measured; 0 filtered out; finished in 6.78s
     Running unittests src/lib.rs (target/debug/deps/photocraft_engine-dd79307758a099d6)
test result: ok. 1291 passed; 0 failed; 32 ignored; 0 measured; 0 filtered out; finished in 76.02s
     Running tests/coverage/main.rs (target/debug/deps/coverage-fd962cfcd0fe1a65)
test result: ok. 582 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.13s
     Running tests/it/main.rs (target/debug/deps/it-9318c628c796f5c2)
test result: ok. 47 passed; 0 failed; 5 ignored; 0 measured; 0 filtered out; finished in 3.97s
     Running tests/vector_v1.rs (target/debug/deps/vector_v1-fc4ccdabb2d32875)
test result: ok. 12 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.11s
     Running unittests src/lib.rs (target/debug/deps/photocraft_format-b711048c05f3cdcb)
test result: ok. 33 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.69s
     Running tests/coverage/main.rs (target/debug/deps/coverage-abefef0f93c9f487)
test result: ok. 39 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
     Running tests/it/main.rs (target/debug/deps/it-bbac87a740253717)
test result: ok. 68 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.90s
     Running unittests src/lib.rs (target/debug/deps/photocraft_io-bf233ff377a29443)
test result: ok. 117 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 8.65s
     Running tests/coverage/main.rs (target/debug/deps/coverage-61b7bf4be61169a3)
test result: ok. 120 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.19s
     Running tests/it/main.rs (target/debug/deps/it-53d755ec7ef2c95b)
test result: ok. 211 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 14.04s
     Running tests/overwrite_original.rs (target/debug/deps/overwrite_original-b91f245dd67933b1)
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s
     Running unittests src/lib.rs (target/debug/deps/photocraft_paint-4ea5a70728998d3f)
test result: ok. 75 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.44s
     Running tests/coverage/main.rs (target/debug/deps/coverage-2d5fb32971572578)
test result: ok. 123 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.36s
     Running unittests src/lib.rs (target/debug/deps/photocraft_pdf-a72d8b57c18e27a0)
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
     Running tests/export/main.rs (target/debug/deps/export-173824573eec611b)
test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.35s
     Running tests/import/main.rs (target/debug/deps/import-b3d35c1e1071bb94)
test result: ok. 21 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s
     Running unittests src/lib.rs (target/debug/deps/photocraft_plugins-8b527d6dcae9a3f5)
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.07s
     Running tests/abi.rs (target/debug/deps/abi-539e7e4d5ef22262)
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.35s
     Running tests/coverage/main.rs (target/debug/deps/coverage-494fe4ea0f51d158)
test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
     Running unittests src/lib.rs (target/debug/deps/photocraft_psd-0c7b12e598d8dfc4)
test result: FAILED. 158 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.06s
     Running tests/coverage/main.rs (target/debug/deps/coverage-c3a46527d847b9fe)
test result: ok. 97 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s
     Running tests/it/main.rs (target/debug/deps/it-0eeef9f84a965308)
test result: ok. 136 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.70s
     Running unittests src/lib.rs (target/debug/deps/photocraft_svg-435327544cad4b78)
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running tests/export/main.rs (target/debug/deps/export-cb3cdb977b1fe6ba)
test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 10.02s
     Running tests/import/main.rs (target/debug/deps/import-fb6b4f62b7bd222e)
test result: FAILED. 11 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 10.44s
     Running unittests src/lib.rs (target/debug/deps/photocraft_ui_egui-751d8a2f94677d8e)
test result: FAILED. 1289 passed; 3 failed; 40 ignored; 0 measured; 0 filtered out; finished in 80.81s
     Running tests/appearance/main.rs (target/debug/deps/appearance-9ea2fce991f0d8b8)
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.29s
     Running tests/gpu/main.rs (target/debug/deps/gpu-3c2edc339de3595c)
test result: FAILED. 85 passed; 1 failed; 16 ignored; 0 measured; 0 filtered out; finished in 75.64s
     Running tests/it/main.rs (target/debug/deps/it-4bb36ce8324beb62)
test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 21.38s
     Running tests/layout16.rs (target/debug/deps/layout16-802ada6158095480)
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.21s
     Running tests/path_text/main.rs (target/debug/deps/path_text-c786c1aa5f4d9c9b)
test result: ok. 5 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.20s
     Running tests/pen/main.rs (target/debug/deps/pen-9d0c8d9569098aa7)
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.76s
     Running tests/snapping/main.rs (target/debug/deps/snapping-aaea51b4dba7c5fd)
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.39s
   Doc-tests lightcraft_codecs
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
   Doc-tests lightcraft_engine
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
   Doc-tests lightcraft_preview
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
   Doc-tests lightcraft_raw
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
   Doc-tests lightcraft_tiff
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
   Doc-tests lightcraft_ui_egui
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
   Doc-tests photocraft_codecs
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
   Doc-tests photocraft_engine
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
   Doc-tests photocraft_format
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
   Doc-tests photocraft_io
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
   Doc-tests photocraft_paint
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
   Doc-tests photocraft_pdf
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
   Doc-tests photocraft_plugins
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
   Doc-tests photocraft_psd
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
   Doc-tests photocraft_svg
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
   Doc-tests photocraft_ui_egui
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
error: 6 targets failed:
    `-p lightcraft-preview --test pool_coverage`
    `-p lightcraft-ui-egui --lib`
    `-p photocraft-psd --lib`
    `-p photocraft-svg --test import`
    `-p photocraft-ui-egui --lib`
    `-p photocraft-ui-egui --test gpu`
```

Exact selected output from `corrected-pool-test.log` (exit 0):

```text
    Finished `test` profile [optimized + debuginfo] target(s) in 26.72s
     Running tests/pool_coverage.rs (target/debug/deps/pool_coverage-df70ef2e57ef5299)
running 1 test
test a_panicking_job_frees_its_slot ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 20 filtered out; finished in 0.00s
```
Exact selected output from `people-timing-rerun.log` (exit 0):

```text
    Finished `test` profile [optimized + debuginfo] target(s) in 3m 36s
     Running unittests src/lib.rs (target/debug/deps/lightcraft_ui_egui-18f70eb28d7b50bd)
running 1 test
test tests_people::ds_finding_large_people_query_keeps_ui_cancellable ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 261 filtered out; finished in 0.33s
```
Exact selected output from `psd-final-tests.log` (exit 0):

```text
    Finished `test` profile [optimized + debuginfo] target(s) in 5.80s
     Running unittests src/lib.rs (target/debug/deps/photocraft_psd-2c03c136b4d0e044)
running 160 tests
test compression::tests::decoded_len_allows_large_psb_planes ... ok
test compression::tests::import_decode_budget_is_checked_before_every_compression_method ... ok
test result: ok. 160 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.28s
```
Exact selected output from `svg-final-tests.log` (exit 0):

```text
    Finished `test` profile [optimized + debuginfo] target(s) in 45.80s
     Running unittests src/lib.rs (target/debug/deps/photocraft_svg-573aa4e91db60929)
running 12 tests
test import::preflight::benign_complexity_regressions::short_documents_admit_bounded_longer_descendant_selectors ... ok
test import::preflight::benign_complexity_regressions::inherited_markers_inside_their_own_art_do_not_recurse ... ok
test import::preflight::class_matching_regression::large_class_compounds_are_charged_before_style_matching ... ok
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running tests/export/main.rs (target/debug/deps/export-42654fe8c5541e61)
running 16 tests
test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.36s
     Running tests/import/main.rs (target/debug/deps/import-d2b3e9ecdd5c1d8d)
running 12 tests
test svg_import_resvg_subset_ratchet ... ok
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.17s
   Doc-tests photocraft_svg
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```
Exact selected output from `ui-corrected-tests.log` (exit 101):

```text
    Finished `test` profile [optimized + debuginfo] target(s) in 4m 50s
     Running unittests src/lib.rs (target/debug/deps/photocraft_ui_egui-ce53894bee242351)
running 13 tests
test generate_ui::draft_refine::tests::draft_refine_mock_comfy_uses_source_and_library_results_can_be_used_or_opened ... FAILED
test pdf_import::tests::pdf_worker_cancel_and_stale_destination_do_not_publish ... ok
test vector_packages::svg_import::async_placement_regressions::worker_decodes_without_editing_and_rejects_stale_destination ... ok
test result: FAILED. 12 passed; 1 failed; 0 ignored; 0 measured; 1319 filtered out; finished in 18.03s
```
Exact selected output from `svg-ui-gpu-rerun.log` (exit 0):

```text
    Finished `test` profile [optimized + debuginfo] target(s) in 6m 19s
     Running tests/gpu/main.rs (target/debug/deps/gpu-f692e30a81b27e57)
running 1 test
test svg_import_app_acceptance::svg_import_app_gpu_live_preview_release_editability_warning_and_undo ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 101 filtered out; finished in 3.08s
```
Exact selected output from `engine-svg-final-tests.log` (exit 0):

```text
    Finished `test` profile [optimized + debuginfo] target(s) in 10m 36s
     Running unittests src/lib.rs (target/debug/deps/photocraft_engine-91dccaaaf7a30834)
running 8 tests
test vector_packages::svg_import::prepared_publication_regressions::prepared_svg_publication_records_one_command_without_redecoding ... ok
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 1315 filtered out; finished in 2.25s
```
Exact selected output from `ai-profile-rerun.log` (exit 0):

```text
    Finished `test` profile [optimized + debuginfo] target(s) in 5m 15s
     Running unittests src/lib.rs (target/debug/deps/photocraft_ui_egui-ce53894bee242351)
running 1 test
test generate_ui::draft_refine::tests::draft_refine_mock_comfy_uses_source_and_library_results_can_be_used_or_opened ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 1331 filtered out; finished in 1.99s
```

Full raw output, including compiler warnings and individual tests, is retained locally in [target/bugfix-files-validation](../../target/bugfix-files-validation/). These generated validation artifacts are ignored and are not source changes. The report embeds exact result output above for the merge queue. The existing build-script warning about absent optional `craft-fonts` also occurs during validation; strict Rust/Clippy checks passed. Preliminary compile/lint failures retained in that directory were corrected (test byte literals, a direct Affinity dependency, translation-list ordering and two collapsible conditions).

A comparison of existing `#[test]`/`#[tokio::test]` names in all changed Rust files against HEAD found no removal or rename. Test-generated changes to the tracked PDF reference image were restored, and the temporary PDF review images were removed. The user's task-file edit and all requested source changes remain uncommitted.

