# J24 blake3 content identity (D12) — Claude builder report

Status: done. One commit on `wip/cl-j24-blake3-identity`.

## What merged

Two ways of identifying a file's bytes became one:

| Before | After |
|---|---|
| Library: SipHash-1-3 128-bit (`lc-preview::hash_bytes`, 32 hex) for duplicate detection, relink / Find Missing, Import → Copy verification | **BLAKE3-256** (`lc-preview::ContentId`, 64 hex) for all of these |
| Editor: BLAKE3 for linked sources / pcraft dedupe | unchanged — and the Library's identity string for a file is now exactly the editor's `blake3::hash(bytes).to_hex()` for the same bytes (pinned by a test) |

New in `crates/lc-preview/src/hash.rs`:
- `ContentId` (`Blake3([u8;32])` | `Legacy(Hash128)`): `of(bytes)`, `parse` (64 hex → BLAKE3,
  32 hex → legacy), `of_like` / `hasher` (check bytes in a stored identity's own scheme), `Display`.
- `ContentHasher`: streaming BLAKE3, optionally plus the legacy SipHash in the same pass.
- Same `blake3` crate/features the editor already uses (1.8.7, `pure`, no default features —
  runtime AVX2/SSE4.1 detection still works through `cpufeatures`). No new crate in the lock file.

Kept on purpose (not identity): `Hash128` / `Hasher128` / `hash_bytes` as SipHash **cache keys**
(thumbnail disk cache, smart-preview file names, AI patch keys, headshot keys, library location
key). Several are persisted as file names; changing them would orphan smart previews and AI
patches. `hash_bytes` is also the legacy identity reader. sha256 (downloads) and CRC-32 (journal)
untouched, per the audit.

## Persisted keys: how old catalogs migrate (never lose links, never duplicate)

`Photo::content_hash` grammar (documented in `media.rs`):
- `<blake3>` — every photo probed from now on;
- `<legacy>` — catalogs written before J24 (opened as they are, nothing rewritten on open);
- `<blake3>~<legacy>` — a legacy photo a later scan read again and **proved unchanged** (the
  legacy hash of the fresh bytes equals the stored one);
- any of these plus `:suffix` (`:dupN` from Duplicate), or a host's own string (tests).

Rules:
- `media::content_key` (faces/people, smart sort, segment, AI patches, smart previews, cull) returns
  the part after `~` — exactly the key it returned before re-keying. Pinned by a test that
  checks `content_key`, `enhance::source_hash` and `smart::file_name` are identical before and
  after the re-key.
- `media::rekeyed(stored, probe)` (used by `content_op`: Reload, Relink, Find Missing relink,
  convert): same string / same BLAKE3 → unchanged; legacy confirmed → `<b3>~<legacy>` (suffix kept);
  legacy the probe could not check → unchanged (no guess); otherwise the file changed → fresh BLAKE3.
  Side fix: a Duplicate (`:dupN`) reloaded unchanged used to lose its suffix (and collide with its
  original); it now keeps it.
- Duplicate detection (Review Import scan and Import): catalog rows are indexed by their identity
  (`media::content_identity`: BLAKE3 for re-keyed rows, legacy for old ones); an incoming file is
  matched by BLAKE3, then by its legacy hash.
- The legacy hash is only computed when it can matter: `media::LegacyIds` = file sizes of photos
  still identified by a legacy hash (any size for unknown-size rows). Probes compute it in the same
  pass only for those sizes (`with_legacy_ids` thread-local around the probe call, so the
  `FileProbe` signature and every host stay unchanged). When no legacy rows are left, probes hash
  BLAKE3 only. Re-reads of library photos (`probe_paths`) compute it for every file while any
  legacy row exists. A cached probe without it (e.g. a merge's seeded probe) gets it from the file
  only if a legacy row of that size exists.
- Find Missing: matches a lost photo by its stored identity in its own scheme (legacy computed only
  if some lost photo is still legacy).
- Copy verification (`import_move::copy_verified`) checks the copy in the scheme of the expected
  identity, so a legacy hash from a host still works.

Migration is lazy ("on next scan"): a photo whose file is never re-read keeps its legacy key and
keeps working (it is still found as a duplicate through the legacy hash). Import scans skip files
already in the library by path, so they don't re-key those.

## Quality / speed (measured)

Release build, this machine (x86-64, AVX2), 256 MiB buffer, best of 5, one thread:

| | time | throughput |
|---|---|---|
| legacy SipHash-1-3 128 | 32.6 ms | 8.24 GB/s |
| BLAKE3 one-shot | 64.4 ms | 4.17 GB/s |
| BLAKE3 in 1 MiB chunks (copy verify) | 64.9 ms | 4.14 GB/s |
| both in one pass (only while legacy rows of that size exist) | 100.9 ms | 2.66 GB/s |

- Quality: BLAKE3-256 is collision-resistant; the fixed-key SipHash-128 is not (G11). Catalog
  and editor now agree on a file's identity.
- Speed: hashing is ~2× SipHash per byte, i.e. ~12 ms vs ~6 ms for a 50 MB raw — small next to
  reading the file (a card at ~90 MB/s takes ~550 ms for it), and probes run up to 8 files in
  parallel. The dual hash costs ~1.6× BLAKE3 alone and only applies to files a legacy photo could
  match. No change to thumbnails/render keys (they stay SipHash), so no cache invalidation.

## Tests

New (all pass, run filtered by name):
- `lightcraft-preview` `tests::j24_content_identity_is_blake3_and_reads_legacy` — BLAKE3 reference
  vectors, equality with the editor's `blake3::hash().to_hex()`, golden legacy value, parse/streaming/
  dual-pass parity.
- `lightcraft-engine` `import::j24_identity_tests::j24_rekey_rules_keep_links_and_suffixes` — every
  `rekeyed` / `link_key` / `content_identity` rule, incl. `:dup` suffixes and strings with `~` that
  are not re-keyed hashes.
- `lightcraft-engine` `import::j24_identity_tests::j24_legacy_catalog_migrates_on_scan_without_losing_links_or_duplicating`
  — old-catalog photo: duplicate caught by scan and import via legacy hash; Reload re-keys to
  `<b3>~<legacy>` with identical link keys; second Reload is a no-op; then nothing legacy is left;
  duplicate caught via BLAKE3; Find Missing finds a renamed file by legacy and by BLAKE3 identity;
  a real change gets a plain new identity.

Checks: `cargo check -p lightcraft-engine --all-targets`, `cargo check -p local-image` (all
dependents), `cargo fmt` on both packages, strict clippy (`--all-targets -D warnings`) on
`lightcraft-preview` and `lightcraft-engine` — all clean. The three TEST-CHANGED tests below were
run and pass.

## TEST-CHANGED

- `crates/lc-engine/src/tests_import.rs` `recursive_import_with_duplicates`: identity length
  `h.len() == 32` → `64` (probes now store BLAKE3).
- `crates/lc-engine/src/tests_import_move.rs` `copy_is_verified_against_the_probe_hash` and the
  copy-timing measurement test: expected hash built with `ContentId::of` instead of `hash_bytes`
  (what the probe now gives); assertions unchanged.
- `crates/lc-engine/src/tests_xmp.rs` `bugfix_develop_dng_refreshes_content_identity_and_virtual_copies`:
  expected identity of the converted DNG is `ContentId::of(bytes)` instead of `hash_bytes(bytes)`.

## Outside edits (minimal, all in `lightcraft-engine` / lock file)

- `crates/lc-engine/src/files.rs`: `probe_bytes` takes its identities from `media::probe_ids`
  (BLAKE3 + optional legacy); the body moved unchanged into `describe_bytes`; `legacy_hash: None`
  in two struct literals.
- `crates/lc-engine/src/import_move.rs`: `copy_new` / `copy_verified` / `hash_file` take a
  `ContentId` and verify in its scheme (was `Hash128`). Unavoidable: otherwise Import → Copy would
  fall back to re-reading the source for every file.
- `crates/lc-engine/src/cmd/convert.rs`: `content_op` uses `media::rekeyed` (the migration point);
  Convert to DNG stores a BLAKE3 identity.
- `crates/lc-engine/src/cmd/missing.rs`: Find Missing matches by identity in either scheme.
- `Cargo.lock`: `blake3` added to `lightcraft-preview`'s dependency list (crate already locked).

## Left / notes

- `crates/lc-engine/src/ephemeral.rs` still names ephemeral (not-in-library) photos
  `ephemeral:<SipHash>`: a cache key for photos that are never catalogued, outside my files; can
  move to `ContentId::of` any time.
- A re-key-only Reload reports the photo as "reloaded" (and drops its in-memory caches once),
  because `photo.reload` treats any `SetContent` as a reload. Render keys don't change, so disk
  thumbnails are reused. Could be filtered in `cmd/convert.rs::reload` if the UI shows a count.
- No proactive background re-key of untouched photos (the task asked for lazy migration). Legacy
  rows keep working; the dual hash only runs for incoming files whose size matches one.
- Not done: wiring pc-engine linked-source lookups to the catalog by identity — the strings now
  match, so that is possible later without any further migration.
