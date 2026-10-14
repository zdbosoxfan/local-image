# compat-a-catalog — Claude builder report

Job A of the old-version-compatibility removal (source: docs/wip/INVENTORY-old-version-compat.md,
section A, plus "Owner decisions (Oct 14 2026, final)"). Status: done, one commit on
`wip/cl-compat-a-catalog`. No helpers were used. The owner's real Library files were never opened.

## Removed / changed, by inventory row

| Inventory row | What was done |
|---|---|
| Catalog format v0 (log without snapshot) — care | `Journal::open` refuses a non-blank `catalog.log` without `catalog.snap` with the new `CatalogError::Older` ("made by an earlier version of Local Image … Create a new Library … left unchanged"), nothing modified. To make "log without snapshot" impossible for current builds, a new library's first (empty) snapshot is now written strictly: if it can't be written, `open` fails instead of only logging a warning. A blank/empty log without a snapshot still opens as a new library. |
| Catalog format v1 — yes | `version < VERSION` is refused untouched (`CatalogError::Older`). The upgrade-on-open branch and `LoadReport::upgraded_from` are gone. `VERSION` stays 2; `CatalogError::Newer` refusal, the `version` header and CRC/torn-tail repair kept. |
| Glued-fragment resync — care | Removed (`resync`, `LoadReport::torn_fragments`, the one reader in lc-engine `library_safety::verify_pair`). Proof it is unreachable for current builds: a failed append cuts the log back; if that fails, `tail_dirty` makes the next `append` truncate first and return the error if truncation fails, so nothing is ever appended after a fragment; a crash leaves the fragment as a torn final record (dropped on open). A glued line in a log is now handled as ordinary mid-log damage (log preserved whole, state recovered up to it). |
| Catalog format tests/fixtures — yes | `tests_format_version.rs`: v1/versionless upgrade tests replaced by `older_snapshot_is_refused_untouched` (every version below current) and `log_without_snapshot_is_refused_untouched`; newer/unknown-op/bad-CRC/new-library tests kept. `contracts_catalog.rs`: the two frozen migration tests became one `frozen_pre_v2_libraries_are_refused_untouched` (see "Deviations"). `ds_journal_coverage`: upgrade test → `journal_open_refuses_older_snapshot_without_modifying_it`. `tests_journal.rs` "old-style snapshot" reworded. |
| Format-policy doc comments — yes | lc-catalog lib.rs and journal.rs module docs now say "reads only this format; refuses older and newer"; the format table has only v2. |
| Local records without baseline / folders without time — care | Done the "safer" way the row suggests: records without a baseline are always kept (`looks_unedited` deleted); folders without a browse time are never forgotten and are no longer stamped (the "stamp now" path, `ForgetPlan::stamp` / `now`, and the `foldersStamped` field of `library.forgetLocal` output + its help text removed). |
| `Meta.creator` comment — KEEP | Comment reworded only; test renamed (`…_single_creator_defaults`), assertions unchanged. |
| `Filter.person` assertion — KEEP (mostly) | The removed `"person"` key in the defaults test input replaced by `{}`; same assertions. |
| J24 `ContentId::Legacy` — yes | `ContentId` is a BLAKE3-only struct (`of`, `parse` 64-hex, `Display`); `ContentHasher` BLAKE3 only; `of_like`, `hasher`, `is_legacy`, `From<Hash128>`, `with_legacy`, `legacy_only`, `finish_legacy` gone. `Hash128`/`Hasher128`/`hash_bytes` (SipHash cache keys) kept; the lc-preview golden now pins `hash_bytes("hello")` as a cache key and checks it is not accepted as an identity. |
| J24 re-key + link keys — care (owner: remove all) | `split_rekeyed`, `link_key`, `content_identity`, `is_legacy_identity`, `LegacyIds`, `LEGACY_IDS`, `with_legacy_ids`, `probe_ids`, `ProbeInfo::legacy_hash` removed. `content_key` = stored hash as-is. `rekeyed` → `refreshed_hash` (same bytes / same identity with `:dupN` keep the stored hash, else the probe's). No re-keying, no orphan checks — the owner starts a fresh Library. |
| J24 call sites — yes | `probe_all` lost its legacy arg; `ScanInput`/`ImportJob` `legacy` fields, `identities`, `with_legacy_hash` gone; duplicate lookup is a plain hash lookup; Find Missing hashes BLAKE3 only; `import_move::hash_file` uses `ContentHasher` directly; `files::probe_bytes` computes BLAKE3 directly. |
| J24 tests — yes | Replaced by BLAKE3-only `j24_refresh_rules_keep_suffixes` and `j24_blake3_identity_finds_duplicates_reload_and_missing` (duplicate in scan + import, stable Reload, Find Missing by content, real change → new identity). |
| J24 report (doc) | "Superseded in part" note added to docs/wip/CLAUDE-REPORT-j24-blake3-identity.md. |
| Preview-cache legacy temp names — yes | `<32 hex>.tmp<alnum>` arm of `is_cache_file` removed; the `.lightcraft-<pid>-<n>.<key>.jpg.pcsave.tmp` arm kept. |
| Smart-preview tone curve without chroma — care | `chroma` is now required when reading a `CameraTone`; a curve without it is refused (a smart preview holding one renders without the stored camera tone, exactly like an invalid curve today). Smart previews live in the Library folder, so a fresh Library has none of the old ones. |
| Heal/Remove spot renderer — KEEP | Behaviour kept; `apply_legacy_heal` → `apply_source_heal`, docs say it serves source-based spots incl. Camera Raw `crs:RetouchAreas`. |
| Camera profiles in old LightCraft dir — yes | `profile_read_dirs` (old `lightcraft`/`LightCraft` sibling folder) removed; profiles are read from `dir()` only. Env var renamed `LIGHTCRAFT_CAMERA_PROFILES` → `LOCAL_IMAGE_CAMERA_PROFILES`, no fallback (owner decision). Test replaced by `profiles_are_read_from_the_one_profiles_folder_only`. |
| Dead `default_dir` + `LIGHTCRAFT_LIBRARY` — yes | Deleted, with the now-unused `DEFAULT_NAME`. |
| Smart Sort people aliases — yes | `faces` / `rejected` aliases removed; the one test feeding them now uses `confirmedFaces` / `rejectedFaces`. |
| Smart Sort sensitivity aliases — yes | `Strict`/`Balanced`/`Loose` capitalised aliases removed (no data file uses them). |
| `last_export` alias — yes | Removed. |
| Develop `process` field test — yes (test only) | The `"process":"legacy"` input dropped; test renamed `unknown_process_fields_are_ignored_and_looks_roundtrip`; all assertions kept (the unknown-`process` case is still covered). |
| Develop global-only layer fields — care | **Kept** (see "Left"). Comment reworded. |
| XMP orientation fragments — care | Behaviour kept (the row suggests keep: also helps malformed third-party XMP); comments reworded. |
| Lens-correction "legacy catalog" test — care | Behaviour kept; comment and names reworded (photo without the embedded lens field from a host/probe that skips it). |
| Library UI old-language test (i18n.rs) | Not touched: i18n.rs belongs to J29 per owner decision. |
| Export defaults, Develop "written before", KEEP rows, command IDs, headless | Left alone (KEEP). |
| docs/whats-new.md | V2 note: fresh Library, older catalogs are refused and left unchanged. |

## Deviations from the inventory (and why)

- **Frozen v0/v1 catalog fixtures kept** (`crates/lc-catalog/tests/fixtures/contracts/catalog-v0.log`,
  `catalog-v1.log`, `catalog-v1.snap`) instead of deleted: they are now the inputs of
  `frozen_pre_v2_libraries_are_refused_untouched` (real old bytes must be refused untouched — the
  "add a test that v1/v0 are refused" the row asks for). Deleting them would also leave
  `crates/pc-format/tests/contracts_support/freeze_references.py` (job B's file) generating files nobody
  reads. SOURCES.md updated to say they are refusal inputs.
- `op_variants_are_versioned` kept: it still enforces "a new Op variant bumps `VERSION`".

## Left, and why

- **Develop layer Treatment / B&W mix / Vignette / Grain fields** (lc-develop settings.rs): still in
  `LAYER_KEYS` (writable through the layer API) and rendered by lc-gpu / lc-pipeline (lc-gpu is outside
  job A). The owner rule "vignette/grain/B&W whole-image only" is a UI rule already enforced by
  `used_keys`. Removing the data fields would be a data-model change, not compat removal.
- Other "legacy"/"old" wording in lc-engine smart-sort tests (`both_legacy_folder_layouts…`,
  `p2b_legacy_embeddings…`, `retired_tagging_preference…`) and lc-pipeline `obsolete_process_field_is_ignored`:
  not inventory rows; they test current optional/unknown-field handling.
- `CatalogError::Newer` message still says "LightCraft" (branding, not compat).
- **Owner follow-up:** with older formats refused, any future `VERSION` bump makes every existing Library
  unopenable (no migrations). The bump rule is documented that way in journal.rs; the owner may want a policy
  for that before v2 ships.

## Checks run

- `cargo check --offline --all-targets` for lightcraft-catalog, -preview, -engine, -pipeline, -develop, -meta,
  -ui-egui; `cargo check --offline -p local-image`: clean.
- `cargo fmt` on those 7 packages.
- `cargo clippy --offline -p <pkg> --all-targets -- -D warnings` for each of the 7: clean.
- Tests (only new/changed ones, filtered): lc-catalog lib modules tests_format_version, tests_torn_append,
  tests_journal, tests_local + 2 renamed tests (28 passed); contracts_catalog `frozen` (2), coverage
  `journal_open|forget_plan|journal_persists` (14); lc-preview 2; lc-pipeline 1; lc-develop 1; lc-engine 8
  (j24 ×2, camera profiles, people centroids, forget-local, copy verify, find missing, import duplicates).
  All passed.

## TEST-CHANGED

- lc-catalog `tests_format_version.rs`: `v1_library_loads_and_is_upgraded`, `versionless_log_only_library_is_upgraded`
  → `older_snapshot_is_refused_untouched`, `log_without_snapshot_is_refused_untouched`; `upgraded_from` assertion
  dropped from `new_library_is_written_in_this_format`.
- lc-catalog `tests/contracts_catalog.rs`: `frozen_v1_snapshot_and_log_migrate_using_existing_legacy_case`,
  `frozen_versionless_journal_migrates_without_losing_metadata` → `frozen_pre_v2_libraries_are_refused_untouched`.
- lc-catalog `tests/coverage/ds_journal_coverage.rs`: `journal_open_upgrades_older_snapshot_in_place` →
  `journal_open_refuses_older_snapshot_without_modifying_it`.
- lc-catalog `tests_torn_append.rs`: `legacy_fragment_glued_to_the_retry_is_skipped` → `fragment_glued_to_a_record_is_damage`
  (now expects damage handling); `torn_fragments` dropped from `partial_append_then_retry_loads_cleanly`.
- lc-catalog `tests_journal.rs`: `torn_last_record_is_tolerated` stores now include the snapshot (log-only stores are refused);
  `streamed_snapshot_keeps_the_on_disk_format` wording.
- lc-catalog `tests_local.rs`: `upgrade_without_times_forgets_nothing_then_counts_from_then` →
  `folders_without_times_and_records_without_baselines_are_kept`.
- lc-catalog `tests/coverage/ds_local_coverage.rs`: `stamp`/`now` assertions removed with the fields; ops test expects 2 ops.
- lc-catalog `tests.rs`: creator test renamed; Filter defaults test input `{"person":"Jane"}` → `{}`.
- lc-preview `tests.rs`: `j24_content_identity_is_blake3_and_reads_legacy` → `j24_content_identity_is_blake3`;
  `disk_cache_never_touches_foreign_files` (old `.tmp<alnum>` name is now foreign; own temp uses the shared-writer name).
- lc-pipeline `tone.rs`: `chroma_curve_is_identity_unless_set_and_survives_serde` (a tone without chroma is refused).
- lc-develop `lib.rs`: `old_process_fields_are_ignored_and_looks_roundtrip` → `unknown_process_fields_are_ignored_and_looks_roundtrip`.
- lc-engine `import.rs` j24 module: both tests replaced by BLAKE3-only tests (see table).
- lc-engine `camera_profiles.rs`: `j16_legacy_profiles_remain_readable…` → `profiles_are_read_from_the_one_profiles_folder_only`.
- lc-engine `tests_forget_local.rs`: `upgraded_catalogs_forget_nothing_at_first` → `records_without_times_or_baselines_are_never_forgotten`.
- lc-engine `smart_sort/people/tests.rs`: centroid test renamed, uses current field names.
- lc-ui-egui `tests_lenscorr.rs`: comment/variable names only.

## Outside edits

None. All edits are in job-A files (lc-catalog, lc-engine, lc-preview, lc-pipeline, lc-develop, lc-meta,
lc-ui-egui test, docs/whats-new.md, docs/wip/CLAUDE-REPORT-j24-blake3-identity.md, this report).
