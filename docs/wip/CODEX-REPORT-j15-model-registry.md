# J15 — shared model registry and on-device download backend

> **Note (2026-10-14):** the preservation of legacy settings and legacy model directories described
> under "Existing root integration" below was removed by owner decision (v2 has no old-version
> compatibility). `AiSettings` now reads only the current `ai` section of `preferences.json`.

## Completed

Continued the branch's existing LaMa/catalog and preferences-root integration. D15/J15's
remaining on-device bundle orchestration now lives in `li-ai::inventory`, using the established
`li-ai::download` transport. `pc-ui-egui::ai_ui::download_seg_files` keeps its public signature
as a one-line adapter. Its duplicate production bundle loops and package installation logic
were deleted. A download captures the effective model root before starting its worker, so a
settings change cannot redirect that pending job to a different folder.

`li-ai::catalog::on_device_files` exposes cached common `FileSpec` plans for every local model.
Family presets already re-export the same `family::FileSpec`; no second download descriptor
is introduced. The plans distinguish downloaded denoise archives from their independently
pinned installed ONNX members. LaMa, single-file masks/depth, CLIP companions, Faces, shared
BSRGAN archives, and both denoisers are included.

`li-seg::MODELS`, `ModelSpec`, `Companion`, `Group::official`, and runtime APIs remain compatible.
Their static table is the sole source of local pins, licenses and inference settings; the
catalog borrows it and derives common download plans once. Moving these runtime types into
`li-ai` would create a dependency cycle because `li-ai` already depends on `li-seg`. The
existing common `FileSpec` projection is the compatibility adapter, not another maintained
model table. Generative `ModelInfo` APIs remain scoped to ComfyUI models rather than fabricating
ComfyUI families for local runners.

Existing root integration is retained: `AiSettings` loads/saves the `ai` section of the merged
preferences, preserves unknown fields and legacy settings/model directories, caches the root,
and publishes it to `lightcraft_sysmem::app_dirs`; editor segmentation and Develop already use
those adapters. No migration, settings schema or default directory changed in this slice.

## Retained and improved behavior

- The existing downloader enforces allowed hosts on every redirect, mandatory size/SHA-256,
  cancellation-aware transport, bounded streaming, file synchronization and unique partials.
- Ordinary bundles preserve main-first order, cumulative progress, first-failure termination,
  verified-member reuse, and repair of invalid canonical members.
- Repairs fetch a verified replacement before touching an invalid installed copy, then use
  same-volume rename. Failed fetch, checksum verification, cancellation or publication leaves
  the old bytes intact. A concurrently completed matching member is retained.
- Per-install staging directories isolate downloaded archives and the existing ZIP installer's
  temporary names. RAII removes staging on success, failure, cancellation and unwind. A bad
  inner denoise pin never changes installed destinations.
- Already verified denoise bundles skip archive download/extraction altogether. Archive progress
  still uses compressed download bytes; installed-member progress is never substituted for it.
- The current local model selection, normalization, runner shapes, precision, licenses,
  inference and output pixels are unchanged. No custom controls or visual design changes.

## Small-sample quality and work measurements

The focused registry test checks **14/14 models and 19/19 member references** against the
pre-existing runner descriptors: filename, URL, exact size, SHA-256 and installed path all
match. It also fixes independent golden expectations for CLIP's four-member order/sizes,
Faces' two members, LaMa's pin, RawNIND's archive versus extracted members, and shared BSRGAN.
CLIP's cumulative offsets remain **0, 351,613,724, 605,807,120, 606,669,448**; total download
bytes remain **607,194,067**.

A small synthetic bundle retry checks **one retained verified member**, **two fetched repair/
missing members**, and progress **1 → 3 → 6 bytes**. Injected failure preserves **3 bytes** of
an invalid pre-existing member; the replaced UI removed that member before beginning the
failed fetch. The real shared HTTP/downloader path publishes a valid first member, rejects an
equal-length bad-checksum companion, and retains the old companion with **zero staging leftovers**.

The tiny denoise test executes the replaced UI package sequence twice on a separate fixture,
then compares it with initial install plus retry through the shared backend. Both installed
members have identical bytes. Measured archive size: **229 bytes**; fetches **2 → 1** and transferred bytes **458 → 229**
(**50% less total transfer** over initial install plus one retry; **100% less on the verified retry**).
These are measured transport-call/byte and filesystem outcomes on tiny test resources, not
GPU/inference timing claims or a benchmark sweep. No image algorithm changes require an image
quality bake-off. No interactive frame-speed improvement is claimed; the actual function-row
Cancel click and public download adapter are exercised together by the new egui_kittest test.

## Validation

Used `PATH=~/.cargo/bin:$PATH`, `cargo +1.98.1`, `--offline`, the same `target/` directory and
launcher-provided `CARGO_BUILD_JOBS=4`. Only one cargo process at a time. Followed the task's
**ABSOLUTE MINIMUM TESTING** override: no full crate/workspace suites, GPU acceptance,
benchmarks or long quality sweeps.

- `cargo +1.98.1 fmt -p li-ai -p photocraft-ui-egui`: PASS.
- `cargo +1.98.1 check --offline -p li-ai -p photocraft-ui-egui`: PASS, including final source refresh.
- `cargo +1.98.1 test --offline -p li-ai j15_ -- --nocapture`: **10 passed**, none failed;
  only authored J15 tests ran. The strengthened package-reference measurement test was then
  run alone: **1 passed**. There are **10 unique backend tests** in total.
- `cargo +1.98.1 test --offline -p photocraft-ui-egui j15_ -- --nocapture`: **1 passed**, none failed;
  the actual Cancel click reaches the shared backend with zero filesystem changes.
- `cargo +1.98.1 clippy --offline -p li-ai -p photocraft-ui-egui --all-targets -- -D warnings`: PASS.
- Whitespace check on owned files: PASS. The launcher-provided `CODEX-TASK.md` modification
  was present on entry and was not edited; its pre-existing trailing blank-line warning remains.
- `Cargo.lock` and all dependency manifests: unchanged. No new dependencies, C/C++/`-sys`
  crates, unsafe code, GUI windows, owner-image access, ComfyUI jobs, commits or uploads.
- GPU tests added: **0**. This is catalog/download/storage work; no inference/GPU change or
  GPU performance claim. Broad integrated validation is left to the coordinator as instructed.

Logs: `target/j15-check.log`, `target/j15-check-final.log`, `target/j15-tests-li-ai.log`,
`target/j15-package-measurement.log`, `target/j15-tests-ui.log`, `target/j15-clippy.log`.
The inherited `lightcraft-engine` build script emits a missing optional `craft-fonts` notice;
this registry slice does not change font embedding.

## Test disposition and user-visible changes

`TEST-CHANGED: ai_ui::tests::bundle_download_retry_keeps_verified_files_and_repairs_invalid_ones`
was moved and strengthened as
`inventory::j15_tests::j15_bundle_retry_preserves_verified_members_and_repairs_after_fetch`.
It uses real SHA-256 pins instead of empty test hashes, retains the original skip/repair/
cumulative-progress/cancel assertions, and adds failed-repair preservation. The staging
fetch path is now absent even while the invalid destination remains until successful repair.

Existing UI order/progress and Faces tests now call the catalog's common file-plan visitor;
their counts/order/failure assertions are retained. New independent golden trace assertions
also cover the backend. No existing test was weakened or full-suite success claimed.

User-visible improvements: failed repairs preserve old files, cancellation cleans downloaded
packages, verified denoisers do not redownload on retry, and downloads keep the directory
selected at their start. No settings/document compatibility changes. New backend validation
errors are listed in `i18n-pending-j15-model-registry.md`; translations remain deferred.

## Scope and references

Source edits stay in the assigned `catalog.rs`, `inventory.rs`, and `ai_ui.rs`. The named report
is required by the task. The only additional file is `docs/wip/i18n-pending-j15-model-registry.md`,
required by the owner's pending-strings rule. Existing task-file changes were left alone.

Followed the repository's established behavior, rather than inventing new model/ZIP formats:

- Audit §1, D15 (§3.4), J15 (§4), and the completed LaMa/catalog/preferences integration.
- `crates/li-seg/src/lib.rs`: the canonical pinned model table, `ModelSpec::files`, readiness
  hash cache, installed paths, custom-model and shared-file contracts.
- `crates/li-seg/src/denoise.rs`: existing ZIP parser, archive/member size/hash enforcement,
  installer and stored-ZIP test-fixture format.
- `crates/li-ai/src/download.rs`: verified/cancellable streaming and no-clobber hard-link
  publication; kept the existing implementation intact.
- `crates/li-ai/src/settings.rs` and `crates/lc-sysmem/src/app_dirs.rs`: effective root and
  compatibility/migration behavior; existing editor and Develop adapters retained.
- Official Rust [`std::fs::rename`](https://doc.rust-lang.org/std/fs/fn.rename.html) and
  [`std::fs::hard_link`](https://doc.rust-lang.org/std/fs/fn.hard_link.html): same-volume
  replacement and no-replacement publication. No new application conventions or
  Lightroom/Photoshop interaction rules were introduced.

SAM 3 defeaturing and retirement/adapter work for `lc-fetch` and the SAM-specific downloader
belong to **J8**, which owns those files. They were not modified or enabled here. Mirrors/resume
remain that job's follow-up, not silently advertised by this registry slice. All J15-owned
active-model registry/download work is complete; no remaining implementation dependency on LaMa.

Changes are uncommitted.
