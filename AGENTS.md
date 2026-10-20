# AGENTS.md: rules for AI agents and contributors

Local Image 2 is a native Rust/egui desktop app for photo cataloguing, RAW development, layered
editing, vector design and AI, for Linux x86_64 and Apple Silicon macOS 13+. It merges PhotoCraft
(`crates/pc-*`, packages `photocraft-*`) and LightCraft (`crates/lc-*`, packages `lightcraft-*`) with
shared crates (`crates/li-*`). Read this file before changing anything, then
[docs/DEVELOPMENT.md](docs/DEVELOPMENT.md).

These rules came out of a full production-readiness and de-slop pass (PRs #21 and #22, October
2026). They exist because AI agents produced most of this codebase and left the usual residue. Keep
the codebase from drifting back.

## 1. Before you start

- **Package names differ from directory names.** `crates/pc-engine` is `photocraft-engine`,
  `crates/lc-engine` is `lightcraft-engine`, and `apps/local-image` is `local-image`. Check
  `Cargo.toml` before running `cargo -p`.
- **Toolchain:** Rust 1.98.1. Build only what you touch (`cargo check -p <pkg> --all-targets --locked`).
  Run a whole-workspace build once at the end, not on every edit.
- **Work on a branch and open a PR.** Never commit agent scratch: plans, reports, logs, task briefs,
  proof screenshots or `docs/wip`-style folders. Keep that in your own scratch space.
- **Parallel agents get their own git worktree and disjoint file sets.** Two writers never edit the
  same files at once. When splitting work, name each agent's files explicitly.

## 2. Quality gates (all must pass before a PR merges)

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo deny --manifest-path ./Cargo.toml --locked check   # advisories, licences, bans, sources
cargo xtask deps-pure                                     # no C-building crates
cargo xtask assets                                        # every bundled asset is attributed
cargo xtask test-fast                                     # the Fast CI lane
```

- After any dependency change, run `cargo xtask attributions` and commit `assets/attributions.json`.
- New RustSec advisories appear almost daily. When `deny` fails without a code change, upgrade if a
  fix exists. If none exists, add a narrowly scoped `deny.toml` ignore whose reason states the
  advisory, why it is unreachable or unfixable, and what would clear it.
- Tests can be flaky under heavy parallel load. Rerun a failure alone, and check it against `main`,
  before blaming your change or "fixing" the test.

## 3. Production-readiness checklist for this app

Adapted from a widely shared list of what a "vibe-coded" app needs before real users depend on it.
SaaS concerns (SSO, SAML, SCIM, multi-tenant isolation, billing, subscriptions, dunning) do not apply
to a local desktop app. These do:

**Data safety**
- The Library catalog (`crates/lc-catalog`: JSON snapshot plus CRC-framed op log; see
  [docs/adr/0001-journal-catalog.md](docs/adr/0001-journal-catalog.md)) must keep opening existing
  libraries. New fields need serde defaults and must round-trip. Never bump `VERSION` without a
  migration and a backup-before-upgrade.
- Every catalog mutation is an undoable `Op`. Deletes are soft (Recently Deleted) and never touch
  originals. Destructive work needs undo or an explicit confirmation.
- Backups must have a tested restore. A backup you have never restored is a hope.
- Writes are atomic (temp file, fsync, rename via `li-fs`). Files holding secrets use
  `li_fs::atomic_write_private` (owner-only).
- Store times with their offset, and sort and display them deliberately (`lc-meta` `DateTime`).

**Untrusted input**
- Every parser (RAW, PSD, TIFF, SVG, PDF, Affinity, `.pcraft`, XMP/EXIF/IPTC, CSV) treats input as
  hostile. Enforce size, dimension and allocation limits before allocating. Use checked or saturating
  arithmetic where a value feeds a length or index; the release profile has no overflow checks.
- No panics on malformed files. Add a fuzz target (`crates/*/fuzz`) for new parsers.
- Text is UTF-8 end to end: emoji, accents, right-to-left text and apostrophes in every field and
  path. CSV import strips the BOM and handles quoted newlines. CSV export guards against formula
  injection.
- Archive and project import must not write outside its destination: no path traversal, no symlink
  creation, no permission bits applied.

**Secrets and privacy**
- No secrets in code, the repo, logs, crash reports, exports or MCP responses. Cloud API keys live
  only in the owner-only `preferences.json` and are sent only as headers to allow-listed hosts.
- Keep [PRIVACY.md](PRIVACY.md) true. Every new network call (provider, model download, update check)
  goes in its network inventory. No telemetry.
- Images sent to cloud providers are re-encoded from pixels, so EXIF and GPS never leave the machine.

**AI and automation**
- Paid cloud runs need confirmation (provider, model, count) and respect the optional session cap.
  Retry only idempotent calls or rate limits, with backoff. Never replay a POST that may have been
  billed.
- The MCP automation server is least-privilege. Irreversible commands stay behind
  `--allow-destructive`. Bridge mode never reads or sets credentials and never starts cloud
  generation without `--allow-cloud-ai`. Every tool carries accurate annotations. Don't loosen any of
  this; treat every field that reaches a model as a possible prompt injection.

**Supply chain**
- Before adding or swapping any dependency, confirm it exists and is the crate you meant: check the
  owner, age, downloads and repository on crates.io. Prefer established maintainers. A young
  single-owner fork replacing security-critical code (TLS, crypto, parsers) is a no.
- Pure Rust only (`xtask deps-pure`). No git dependencies (`deny.toml` sources). Pin GitHub Actions
  to commit SHAs. Let Dependabot propose upgrades; review them like any other change.

**Reliability and UX**
- Long work runs off the UI thread and can be cancelled. Errors tell the user what to do next,
  never "something went wrong".
- Logs go through `log` (no stray `println!`/`eprintln!` in library code) and never contain secrets.
  Panics write a local crash report (`apps/local-image/src/crash_guard.rs`).
- Empty, loading and error states exist on every screen. Keyboard navigation and screen-reader
  labels (AccessKit) work. User-facing strings go through `tl!` (`li-i18n`).

**Release**
- CI runs tests before packaging. Stable tag builds must be signed and notarized (macOS), never
  ad-hoc. An alpha or beta tag may ship ad-hoc only when dispatched with `unsigned_prerelease`,
  and its release notes must say so.
- Update [CHANGELOG.md](CHANGELOG.md). Record architecture decisions in [docs/adr/](docs/adr/).
  Report security issues per [SECURITY.md](SECURITY.md).

## 4. De-slop rules

Write code a senior engineer would sign off on. These patterns were removed at scale. Don't
reintroduce them.

**Comments**
- Comments state intent or an invariant the code can't. Delete comments that restate the next line.
- Never write about the process that produced the code: "package 12", job ids (J13), `TEST-CHANGED`,
  "Previously…", "Now we…", "Fixed:", "NEW:", "as requested", or pointers to agent reports, task
  briefs or chat history. Git history holds that.
- No decorative dividers (`// ===== Foo =====`). Use a plain `// Foo` heading only in long files where
  it helps navigation.

**Code**
- No dead code. Unused functions, never-installed hooks, zero-impl traits and blanket
  `#[allow(dead_code)]` get deleted. Verify with the compiler (demote to `pub(crate)` and let rustc
  confirm) before deleting.
- No duplication. Reuse the shared crate (`li-*`, `photocraft-export-common`, `li-raster`,
  `li-color`, `li-fs`, the `half` crate) instead of copying a helper. Check before writing a new one.
- No pass-through wrappers that only call the same function in another crate. Use `pub use`.
- No `#[path = "../other-crate/..."]` includes of production code across crates. Depend on the crate.
- UI crates don't do file IO directly. Put it in an engine or service crate and call it from the UI.
- Keep files focused. Split a file past about 1,500 lines into submodules by responsibility, moving
  code without rewriting it. Don't keep appending to a large file.
- No new dependencies to save a few lines.

**Tests**
- A test must assert behaviour. Delete tests that duplicate another test, assert only `is_ok()` on a
  call that can't fail, test derives, constants or getters, or have no assertion (deliberate no-panic
  tests on hostile input are fine).
- Name tests after the behaviour they check, not the job or agent that wrote them.
- GPU tests fail (or are explicitly ignored) when no GPU is present; they never silently return. Only
  GPU runners execute them.
- Golden tests must be portable. Never fingerprint exact `f32`/`f64` output bits: `powf`, `cbrt`,
  `sin` and similar differ in the last bit between macOS and Linux libm. Compare against a
  higher-precision reference within a stated tolerance, or compare two implementations on the same
  machine.
- Fix a test only when it is wrong, and say why in the commit message. Never weaken a check to make
  CI green.

**Tools**
- Review mode follows the ai-slop-cleaner method (yeachan-heo/oh-my-claudecode,
  `skills/ai-slop-cleaner`): protect behaviour with tests first, plan, classify each smell
  (duplication, dead code, needless abstraction, boundary violation, weak tests), then clean in
  single-focus passes (deletions, then duplicates, then naming and errors, then tests), re-running
  the quality gates after each pass. Prefer deletion. Keep diffs small and reversible.
- [dabit3/deslop](https://github.com/dabit3/deslop) is a quick regex check (dividers, explicit
  boolean comparisons, vague TODOs, debug logging). It scans only a branch's diff against `main`,
  which is the right default for reviewing a PR (`npx deslop -b main`). Its patterns target
  JavaScript, so treat it as a first pass, not proof of clean Rust.
- Comment-only cleanups must be verified as comment-only: strip comments from the old and new file
  and compare the remaining tokens.

## 5. Upstream

PhotoCraft (`storytold/photocraft`) and LightCraft are upstreams under MIT OR Apache-2.0. Port fixes
by hand: rate each commit MUST (crash, data loss, security), SHOULD (correctness or parity in a
shipped feature), NICE or SKIP (Windows-only, web/wasm, printing, shared-UI migration, branding).
Record every ported upstream SHA in [docs/PORTS.md](docs/PORTS.md) with its licence. New upstream
dependencies must pass the pure-Rust and `deny.toml` gates. Skip `craft-print`, which pulls C
crypto through a git dependency.
