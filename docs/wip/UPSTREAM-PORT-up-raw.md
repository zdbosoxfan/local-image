# Upstream port: up-raw (LightCraft raw-decoder fixes), branch wip/up-raw

Fork point b8df5f4. All edits are in `crates/lc-raw/**` (package `lightcraft-raw`), plus one allowed line:
`crates/lc-engine/src/media.rs` `RENDER_CACHE_VERSION` 15 -> 16 (decoded output changes; bumped once for the whole batch).

All items below are PORTED unless noted. Test names are in `lightcraft-raw` unit tests (`--lib`); all pass.

## MUST
| # | upstream sha | status | files (crates/lc-raw/src/...) | tests |
|---|---|---|---|---|
| 1 | a443659 | PORTED | opcodes.rs | `opcodes::tests::bad_pixels_constant_is_capped`, `bad_pixels_list_is_capped_inside_a_rectangle` |
| 2 | 1ba0421 | PORTED | vendor/rw2.rs | `rw2::tests::format8_cell_is_coded_column_by_column` (known-answer); round-trip test encoder updated to the new order |
| 3 | ff8d027 | PORTED | vendor/nefc.rs (our predictor overflow guard kept) | `nefc::tests::lossy_type2_version_0x40_spans_a_quarter_of_the_range` |
| 4 | b5440f1 | PORTED | vendor/cr2.rs (`masked_columns`) | `cr2::tests::black_from_masked_columns_only` |
| 5 | 1399c44 + c7719dd | PORTED (needed 9894223's `cfa_from_exif` helper, see below) | vendor/pef.rs, vendor/mod.rs | `pef::tests::the_pattern_is_the_exif_cfapattern_from_the_array_origin_for_every_compression` |
| 6 | b59ffcc | PORTED | vendor/mod.rs (`white_from_data`) | `vendor::tests::white_plateau_with_strays_above` |

## SHOULD
| # | upstream sha | status | files | tests |
|---|---|---|---|---|
| 7 | 34ecea4 | PORTED | vendor/cr2.rs | `cr2::tests::color_data_stored_as_bytes`, `wb_from_undefined_bytes` |
| 8 | b4c4a43 + 010d9bb | PORTED | saturation.rs (new), lib.rs (`lift_clipped` on the DNG decode path) | 8 `saturation::tests::*` incl. `decoding_a_dng_lifts_the_plateau`, `early_clipping_renders_neutral_after_white_balance` (ran against our highlight::reconstruct) |
| 9 | 3100cc1 + 179910c | PORTED | vendor/arw.rs, vendor/sr2.rs (new), vendor/mod.rs (`mod sr2;`) | `arw::tests::sr2_black_level_*`, `white_balance_prefers_the_applied_levels`, `sr2::tests::*`. Upstream's keystream SHA-256 test used the `sha2` crate (not a dependency here); replaced with a small in-test SHA-256 checked against the standard "abc"/empty answers (`sr2::tests::sha256_known_answers`), same expected digest. |

## Other small fixes
| sha | status | files | tests / notes |
|---|---|---|---|
| 5bcedbe (D4) | PORTED (measured-body table only; our nef.rs has no CropArea support, so only the D4 entry applies) | vendor/nef.rs | `nef::tests::measured_d4_image_area_and_white_level` |
| 3d9f849 (NRW black) | PORTED, together with its dependency a12f169's NRW data block | vendor/nef.rs | `nef::tests::nrw_block_selects_32_bit_word_packing_and_supplies_the_black_level` |
| a12f169 (12-bit in 32-bit words) | PORTED (RAF flag 0xf006 + NRW) | tiffraw.rs, unpack.rs, vendor/nef.rs, vendor/raf.rs | `unpack::tests::words32_are_read_as_numbers_then_msb_first`, `raf::tests::flag_selects_word_packed_12_bit`. Upstream's `overflowing_row_stride_is_an_error` edit not applicable (no such test here). |
| f1fa336 (Sony 16:9 crop + refuse encrypted DSC-R1 SR2) | PORTED (chosen over ed49c73, which is the crop-only earlier variant of the same change) | vendor/arw.rs | `arw::tests::recorded_size_crops_in_camera_aspect_ratios`, `sr2_is_refused_not_decoded_as_noise`. Merged by hand with 5ed5925's `centred` flag exactly as upstream main does. |
| ed49c73 | SKIPPED (superseded by f1fa336 above) | | |
| cfca299 (A200/A230/A350 packed ARW refused) | PORTED | vendor/arw.rs | `arw::tests::strip_larger_or_smaller_than_one_byte_per_pixel_is_the_packed_variant` |
| 5ed5925 (A900 packed 12-bit) | PORTED | vendor/arw.rs | `arw::tests::packed_12_bit_strip_of_one_and_a_half_bytes_per_pixel_decodes`, `packed_12_bit_needs_the_exact_strip_size`, `a900_white_balance_block_is_read_only_when_it_matches` |
| 169a4ed (ORF CFA from Exif) | PORTED (uses the shared `cfa_from_exif` in vendor/mod.rs) | vendor/orf.rs, lib.rs (`orf::decode` now takes `Mode`; probe_info docs) | `orf::tests::exif_cfa_pattern_states_the_layout`, `packed_12_bit_probe_agrees_with_decode`, `data_fallback_*` |
| 37d3621 (two-field 12-bit ORF) | PORTED | vendor/orf.rs | `orf::tests::two_field_12_bit_layout`, `two_field_layout_needs_every_condition` |
| 44b1262 (12.8 bpp block ORF) | PORTED | vendor/orf.rs, lib.rs (doc line) | `orf::tests::block16_*` |
| 9894223 (PEF PackBits-tagged packed 12-bit) | PORTED, except its lib.rs "Samsung-branded body with a Pentax maker note is probed as PEF" part (SKIPPED: not part of the requested scope, and lib.rs has no probe-test scaffolding; can be added later) | vendor/pef.rs, vendor/mod.rs (`cfa_from_exif`, `EXIF_CFA_PATTERN`) | `pef::tests::packed_samples_under_the_packbits_tag_are_read_when_the_strip_has_exactly_the_packed_size`, `a_packbits_tagged_strip_of_any_other_size_stays_unsupported` |

Also ported with the ORF/PEF commits: 82566817's doc-only cleanup (removed references to analysis files not in the repo).

NOT touched (as instructed): CR3, compressed RAF, ARW6, JPEG-XL DNG, 024e983 (Nikon CropArea / preview colour space).

## Render cache
`crates/lc-engine/src/media.rs`: `RENDER_CACHE_VERSION` 15 -> 16 (single bump covers every decoder output change above).

## Final checks
- `cargo fmt -p lightcraft-raw`: applied (only owned files touched).
- `cargo clippy --offline -p lightcraft-raw --all-targets -- -D warnings`: clean.
- `cargo test --offline -p lightcraft-raw --lib`: 151 passed, 0 failed, 3 ignored (the ignores were already there).
- Also ran the crate's integration targets (`--tests`): coverage 15 passed, it 58 passed (1 pre-existing ignore). None edited.
- `cargo check --offline -p lightcraft-engine`: OK.

## TEST-CHANGED
None. (The RW2 test encoder in `rw2::tests` was changed to code cells column by column, as in the upstream fix; its round-trip assertion is unchanged.)

## Needs from other owners
- Nothing required. Optional: `docs/PORTS.md` rows at integration (shas listed above). Real-file corpus tests upstream added in `crates/raw/tests/corpus.rs` were not ported (they need client/corpus raws).
- Behaviour note for the engine/highlight owner: DNG decode now raises plateau-clipped samples to the stated white level (saturation.rs), which feeds highlight reconstruction; renders of DNGs that state a white level above their true clip point will change (iPhone-style DNGs).
