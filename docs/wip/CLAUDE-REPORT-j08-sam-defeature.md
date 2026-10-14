# J8 SAM defeature + `lc-fetch` retire — Claude builder report

Branch `wip/cl-j08-sam-defeature`. Audit items D15 / D16 (small slices). Written as the work goes.

## Findings before changes

- **SAM 3 was already out of the default build.** `apps/local-image/Cargo.toml` has no
  `default` feature list; `sam` is opt-in only. `cargo tree -p local-image` (default features)
  contains no `candle-core`, `lightcraft-segment` or `lightcraft-fetch`; `local-image-cli` neither.
  Normal-dependency package count: default 901, `--features sam` 931 (+30: candle, safetensors,
  gemm, the hand-written HTTP client's rustls/webpki stack…). So the "drop `sam` from default"
  slice needed no manifest change, only accurate comments (several still said "the desktop app
  enables it").
- `lc-fetch` (553-line hand-written HTTP/1.1 client + 400-line downloader) is used only by
  `lc-segment::fetch` → `lc-engine/src/segment/download.rs` (SAM 3 weights), behind `sam`.
- `lc-segment/src/tokenizer.rs` (177 lines) and `li-seg/src/bpe.rs` are the same CLIP BPE port;
  the only differences: SAM pads with EOS and returns a "real token" mask, li-seg pads with 0 and
  stops early on long input (same result, proven by li-seg's own prefix test).

## What merged

1. **SAM 3 off in every default build (D16).** No manifest change was needed (see above); the
   `sam` feature comments in `apps/local-image/Cargo.toml` and `crates/lc-engine/Cargo.toml` and
   the `lc-engine::segment` module doc now say it is off by owner decision. `lightcraft-segment`
   still builds behind `--features sam` (checked, clippy-clean).
2. **One CLIP BPE tokenizer (D16).** `crates/lc-segment/src/tokenizer.rs` is now a 30-line
   wrapper over `li_seg::bpe::Bpe` (same `Tokenizer::{load, new, encode}` API and `CONTEXT`),
   adding only SAM 3's end-token padding and the real-token mask. 177 → 96 lines incl. tests.
   Token output is identical: li-seg's own `ds_finding_bpe_preserves_prefix_…` test already
   proves its early stop equals the old truncate-after-encode; the new golden test checks padding,
   mask, id 0 inside a prompt, unknown pieces, truncation and empty prompts.
3. **One downloader (D15).** `crates/lc-fetch/src/http.rs` (553-line hand-written HTTP/1.1 +
   rustls client) deleted. `lc-fetch` keeps its public API (`FileSpec`, `Options`, `Progress`,
   `DownloadError`, `mirrors`, `download`) as a thin adapter: mirror order, tries per mirror,
   multi-file progress and the `AtomicBool` cancel flag (bridged to `li_ai::JobControl`) — every
   byte goes through `li_ai::download::download_file_with`. `lc-engine/src/segment/download.rs`
   (the background job wrapper) is unchanged apart from its doc: it already called
   `lc-fetch::download`, whose signature stayed. `lc-fetch` lost its own rustls,
   rustls-rustcrypto, webpki-roots and sha2 dependencies (now only li-ai's).

## Behaviour changes (SAM 3 download only; feature is off)

Best of each kept where the shared client has it; the rest documented:

| | old `lc-fetch` | now (li-ai) |
|---|---|---|
| hash | optional (vocab.json / merges.txt unpinned) | **mandatory**: unpinned files are never downloaded; clear error "put it in the model folder yourself" |
| hosts | any http(s) mirror | li-ai allow-list (Hugging Face, GitHub, …) checked on every redirect |
| token | https only, first host only | same (https check here, first-host check in li-ai) |
| damaged file | replaced on rename | deleted before re-download (never kept); the old client's `<name>.part` is removed |
| disk space | not checked | li-ai checks size + 256 MB free first |
| resume | yes (Range) | **no**: an interrupted / cancelled download starts over |
| stall | 30 s without bytes → next try | li-ai budget (30 s to first response; body 120 s + size / 256 KiB/s); cancel is immediate |

The SAM 3 tokenizer files have no published hash we can pin (facebook/sam3 is gated; checked via
the Hugging Face hub tool: access denied), so in-app SAM 3 download now needs the user to place
vocab.json / merges.txt first. `DEFAULT_MIRRORS` was already empty, so the in-app download never
worked without user configuration anyway.

## Numbers

- Default build (`local-image`, `local-image-cli`): no candle, no lightcraft-segment, no
  lightcraft-fetch — before and after (901 normal-dep packages, unchanged).
- `--features sam`: 931 → 929 packages; `lc-fetch` + tokenizer source 1,462 → 693 lines (tests
  included); repository total 1,071 lines deleted, 322 added (before the report).
- Compile: `cargo check -p lightcraft-fetch -p lightcraft-segment` 13 s warm. No speed numbers for
  downloads (no network in tests; the li-ai client is the one already used by every other model).

## Checks run

- `cargo check --offline -p local-image -p local-image-cli` (default): OK; `-p local-image --features sam`: OK.
- `cargo fmt` on lightcraft-fetch, lightcraft-segment, lightcraft-engine: clean.
- `cargo clippy --offline --all-targets -- -D warnings` on lightcraft-fetch, lightcraft-segment,
  lightcraft-engine (default) and lightcraft-engine `--features sam`: clean.
- Tests (filtered, only new/changed): `lightcraft-fetch j08_` 9/9 pass; `lightcraft-segment
  tokenizer` 2/2 pass; `lightcraft-engine --features sam a_failing_download_ends_with_an_error_and_never_blocks` pass.
- `cargo xtask attributions` regenerated `assets/attributions.json` (its up-to-date test not run).

## TEST-CHANGED

- `crates/lc-fetch/src/tests.rs`: rewritten against the adapter (tests renamed `j08_*`; the local
  server is allowed with `li_ai::download::allow_test_host`).
  - hash mismatch: message assertion `"SHA-256"` → `"checksum"` (li-ai's wording); the "nothing
    kept" assertion is now "the folder is empty".
  - too much data: `"more than"` → `"larger than expected"`.
  - cancel: "keeps the part for later" → "leaves nothing" (no resume); the next run still succeeds.
  - token: the redirect-host half is li-ai's (`open` sends the token only to the first host); the
    https / empty / unnamed-variable assertions are kept.
  - chunked + unpinned: chunked body now on a pinned file; an unpinned file → `Unpinned`, no request
    made; a user-placed unpinned file within its cap is accepted, over its cap is not.
  - Removed with the deleted code: `resumes_partial_files_and_restarts_bad_ones`,
    `a_416_or_a_bad_complete_part_starts_over`, `content_ranges` (Range/resume no longer exist),
    `a_stalled_server_times_out_and_the_next_mirror_finishes` (the 30 s stall timeout is gone).
  - New: `j08_damaged_files_and_old_part_files_are_replaced`, `j08_a_host_off_the_allow_list_is_refused`.
- `crates/lc-segment/src/tokenizer.rs`: `splits_like_clip` removed with the private
  `pre_tokenize` (the identical test and assertion exists in `li-seg/src/bpe.rs`);
  `merges_by_rank` kept unchanged; new `j08_shared_bpe_matches_sam3_padding_and_mask`.
- `crates/lc-engine/src/tests_segment.rs::a_failing_download_ends_with_an_error_and_never_blocks`
  (sam only): the error was "not found" from a local 404 mirror; now the first run must fail on
  the unpinned tokenizer files ("vocab.json" … "model folder"), and with them placed the local
  plain-http mirror is refused ("unexpected host"); never-blocks / nothing-left-behind assertions kept.

## Outside edits

- `crates/lc-segment/Cargo.toml` (+ `li-seg` dep), `crates/lc-segment/src/fetch/mod.rs` (doc only).
- `crates/lc-engine/Cargo.toml` (comment), `crates/lc-engine/src/segment/mod.rs` (doc line),
  `crates/lc-engine/src/tests_segment.rs` (test above).
- `docs/PORTS.md` (dropped the deleted `lc-fetch/src/http.rs` row), `assets/attributions.json`
  (regenerated by `cargo xtask attributions`), `Cargo.lock` (dependency edges only; no new packages).

## Left / why

- Resume + stall timeout + mirror lists as features of `li-ai::download` itself (the audit's "if SAM
  3 ever returns" item): `li-ai/src/download.rs` is not this job's file; only matters if SAM 3 is
  switched back on.
- Pin vocab.json / merges.txt (size + SHA-256) if SAM 3 ever returns, so the in-app download can
  fetch them again (needs access to the gated repo or LightCraft's own CDN).
- `docs/ai-masks.md` may still describe resume / any-mirror downloads (docs not owned; SAM is off).
- The rest of D15 (one `FileSpec`, one model root, folding `li-seg::MODELS` into the catalog) and
  `li-matte` (D16 edge refine) are other jobs (J15, J14).
- `lc-segment` is Apache-2.0 and now depends on li-seg (GPL workspace crate); the app is GPL
  anyway and the bpe code is itself the Apache-2.0 port, so no licence conflict.
