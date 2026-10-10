# Upscale integration merge — 2026-10-10

Merged `claude/sleepy-franklin-egimjb` at `b8b64444` into `wip/codex-ai-enhance`.
Initial merge: `57dce004` (parents `f7acb3de`, `b8b64444`).
The working branch advanced during validation, so follow-up merge `0efb0dc3` also
includes `7096a5c9` (Fix4 and the completed Colourise/old-photo lane). The owner authorised
commits in this worktree for this integration task.

## Conflict resolutions

| File | Resolution |
|---|---|
| `assets/attributions.json` | Regenerated with `cargo +1.98.1 xtask attributions`, retaining both model registries and the incoming asset/icon notices. Final generation: 35 AI model entries, 738 Rust crate entries. |
| `lc-ui-egui/src/i18n.rs` | Retained separate pending-English lists for Upscale export and AI Denoise. |
| `li-ai/families/index.json` | Registered RealPLKSR, BSRGAN and Restoration together. |
| `li-ai/src/catalog.rs` | Kept the vetted downloader host policy and accepted both core Upscale PTH weights and embedded pinned restoration files. |
| `li-ai/src/family.rs` | Kept all embedded families; validation accepts core-upscaler PTH files alongside the incoming restoration policy. |
| `li-seg/src/lib.rs` | Retained the denoise/enhance modules, Raw/RGB groups and all 12 model specs. Dedicated tasks are rejected by mask inference. Combined exact-size/checksum requirements, preserving denoise's worker-thread hashing. |
| `li-seg/src/tests.rs` | Asserted eight grouped specs plus four Upscale candidates; still checks one official model for every group (two for Faces). |
| `pc-ui-egui/src/ai_ui.rs` | Retained all incoming denoise Settings controls and additive Upscale rows. Increased the unscrolled test fixture height and reconciled download/change counts. |
| `pc-ui-egui/src/generate_ui.rs` | Kept shared Upscale routing and the selected-layer refine guard as independent branches. Retained both headless regression tests, variety controls and the other incoming refine changes. |
| `pc-ui-egui/src/i18n/mod.rs` | Combined the pending-English lists for Upscale, denoise, restoration and BYO, deduplicating shared Change/Use default entries. |
| `docs/wip/RESEARCH-ai-restoration.md` | Retained the owner-authorised BYO extension and its licence/source review. |

The automatically merged `li-ai/src/mock.rs` needed reconciliation too: its duplicate
`UpscaleModelLoader` registration emptied the file list on the second removal. Kept one
registration, accepted PTH model inventories, and distinguished native 2×/4× enlargement
from original-resolution restoration in the shared core-node fixture.

Audit of the initial merge against its working-branch parent: all 229 incoming changed files outside the
Upscale overlap are retained byte-for-byte. DeepSeek fixes, denoise pipeline/GPU work,
restoration/BYO, Z-Image variety/refine changes and the icon mix remain present.

## Validation

Rust 1.98.1, offline, `CARGO_BUILD_JOBS=3`; one Cargo job at a time. The full
suites of the 21 changed packages ran once, including integration and doctest targets,
using `cargo +1.98.1 test --offline --no-fail-fast` with the package list below.
Initial full-run duration: 963.50 seconds; compilation: 6m 51s.

The initial full run reported one assertion failure and two native process crashes:

- Smart Sort started face analysis before tagging-cache validation and again after
  setup. Removed the early start, retained cancellation on a later spawn failure,
  and added a success-path test. Both setup-error and successful completion checks pass.
  Fix4 independently carried the same production fix and a UI-button recovery check;
  the follow-up merge retains those checks too.
- The incoming Variety regression was nested inside another test. Moved it to module
  scope; its settings, preset and request checks now execute and pass. The incoming
  Fix4 move agrees with this repair.
- LightCraft GPU's integration target passed 44 active tests, then crashed in NVIDIA's
  `libnvidia-gpucomp.so.615.71.09` compiler thread at process exit. Rechecked the affected
  target serially with the NVIDIA ICD: all 44 pass, two existing cases stay ignored,
  process exit is 0, 27.03 seconds. Adapter logging confirms RTX 5090 Vulkan.
- PhotoCraft UI's unit target passed 954 tests before a crash in Vulkan texture setup
  during the Draft GPU matrix. The isolated matrix passes all themes/resolutions on
  the RTX 5090 in 7.16 seconds. Its GPU screenshot fixture and the colour-icon GPU
  fixture now use the existing AI test mutex, alongside the Variety GPU fixture.
  Completed all 72 previously unfinished cases and the related repairs with targeted
  filters: 77 checks pass (75 PhotoCraft UI, two LightCraft UI), process exit is 0.
  The serial targeted checks do not prove a driver defect is permanently fixed;
  the two native crashes did not recur in those checks. No software GPU was substituted.

The initial merged snapshot has **5,936 distinct ordinary checks validated** after
these targeted repairs, with 66 ordinary-run ignored cases. This count combines full-run
results, the successful pre-crash UI cases, unfinished UI checks and new regressions;
rechecks are not counted twice. The later restoration/Fix4 merge gets additional
focused checks, recorded below, rather than another full 21-package run.

| Package | Validated checks for initial merged snapshot | Ordinary ignored |
|---|---:|---:|
| li-ai | 455 | 3 |
| li-attributions | 5 | 0 |
| li-seg | 64 | 6 |
| lightcraft-catalog | 188 | 1 |
| lightcraft-color | 31 | 0 |
| lightcraft-develop | 97 | 0 |
| lightcraft-engine | 837 | 6 |
| lightcraft-gpu | 94 | 6 |
| lightcraft-meta | 84 | 0 |
| lightcraft-pipeline | 408 | 3 |
| lightcraft-raster | 13 | 0 |
| lightcraft-raw | 173 | 4 |
| lightcraft-ui-egui | 254 | 4 |
| local-image | 87 | 2 |
| pc-trace | 19 | 3 |
| photocraft-engine | 1479 | 16 |
| photocraft-gpu | 92 | 0 |
| photocraft-io | 427 | 2 |
| photocraft-testkit | 8 | 0 |
| photocraft-ui-egui | 1103 | 10 |
| xtask | 18 | 0 |

Strict clippy passes for all 21 packages and all targets, on the follow-up merge:
`CARGO_TARGET_DIR=target/clippy cargo +1.98.1 clippy --offline --all-targets`
with the same package list and `-- -D warnings`. Duration: 29.07 seconds. The optional
craft-fonts build-script notice remains; Rust/Clippy checks pass.

The follow-up merge required only concatenating its new pending-English strings with
Upscale's list. It preserves DDColor, scratch detection, AI Remove review/confirmation,
optional colourise follow-up, and all Fix4 changes. Regeneration found the new scratch
notice lacked an attribution reference; added it to the existing Microsoft entry and
regenerated successfully. No model pins or chosen Upscale defaults changed.

The follow-up restoration/Fix4 subset validates **88 checks** (including overlapping
rechecks): li-ai 10, li-seg 9, LightCraft UI 22, PhotoCraft UI 40, xtask 7. Four existing
real-model/hardware tests remain ignored in this subset. Its first run exposed the
incoming Smart Sort button-recovery assertion checking rows immediately after workers
finished. Both Smart Sort regressions now wait for the review rows, retaining their
row-count and worker assertions; the final two-case rerun passes in 5.12 seconds.
The hardware slider check reports RTX 5090 Vulkan throughout, with 24 actual drag
samples and full-resolution releases; the mock restoration tests do not claim real
model inference. No full suite was repeated.

Cargo also found both restoration packages' examples wrote the same executable name.
Renamed only the li-seg example to `restoration_onnx_probe`, retaining its code and
updating the reproduction command in the Restore report. The final targeted build
compiles the new example and reports no filename collision. The Comfy probe keeps
its existing name. Final focused lint for the last test/probe edits passes for lightcraft-ui-egui and li-seg, all targets with `-D warnings` (43.69 seconds). Formatting and `git diff --check` pass.

Logs, all in this worktree:

- `target/enhance-merge-full.log`: the one full run, including original failures.
- `target/enhance-merge-smart-sort-fix.log`: initial worker fix and new-test timing repair.
- `target/enhance-merge-targeted.log`: all 77 targeted repairs/unfinished UI checks pass.
- `target/enhance-merge-gpu-crash-recheck.log`: standalone RTX Draft matrix.
- `target/enhance-merge-lc-gpu-recheck.log`: RTX GPU parity and clean process exit.
- `target/enhance-merge-core-2045252.log`, `target/enhance-merge-core-2132208.log`: native crash stacks.
- `target/enhance-merge-clippy.log`: all-package, all-target strict lint pass.
- `target/enhance-merge-attributions-latest.log`: final successful regeneration.
- `target/enhance-merge-latest-checks.log`: newly merged restoration/Fix4 checks, including the superseded row-timing assertion.
- `target/enhance-merge-smart-sort-final.log`: repaired row-readiness predicates pass.
- `target/enhance-merge-final-focused-clippy.log`: final test/probe lint check.

No owner application or ComfyUI process was opened or stopped; no owner photos were used.
