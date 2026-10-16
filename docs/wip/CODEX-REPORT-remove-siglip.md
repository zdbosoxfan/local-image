# CLIP-only tagging — 2026-10-10

Continued the existing evaluation builder's implementation on this branch. CLIP
(`clip-b32-laion`) is the only shipped tagging model. Changes are uncommitted.

## Completed task points

- Removed the retired inference implementation, its SentencePiece Unigram tokenizer,
  all three golden fixtures, fixture provenance and licence notice. Repository search
  confirmed the tokenizer had no other consumers.
- Removed the retired model specification, model-family enum, engine tagger/scoring
  branches, model-choice enum, preference field and `smartSort.setModel` command.
  Restored the dedicated `Task::ImageText` boundary. CLIP retains its existing BPE,
  three crops, 512-dimensional embeddings, softmax/background scoring, thresholds,
  cached text vectors and per-model embedding file/header validation.
- Removed the Library model selector and its worker preference plumbing. Library
  downloads/status and Compositing's single tagging row use CLIP. Both UI harnesses
  exercise download controls with actual clicks; Compositing's cancellation test
  now targets the CLIP bundle.
- Old `taggingModel` fields are ignored by serde regardless of their value. The
  library regression opens saved preferences with removed/current/future IDs and
  null, checks preserved presets and face settings, checks CLIP status, and verifies
  saving drops the obsolete field. Preferences without that field also still load.
  A second regression proves retired 768-dimensional cache entries cannot satisfy
  CLIP analysis, and CLIP's 512-dimensional disk cache resumes without inference.
- Removed the retired model and five algorithm-port attributions and their PORTS
  rows. Reviewed `assets/attributions-curated.json`: it had no corresponding rows.
  Removed the three selector-only strings from all 12 Library JSON catalogs. The
  Compositing TSV catalogs had no matching strings; no TSV changes were necessary.
- Updated the specification and earlier phase report. The archived tagging
  evaluation report explicitly states the other candidate was evaluated and is
  not shipped, and retains its pinned artifact metadata. Both measurement/result
  JSON files are unchanged. The recorded 330-photo open comparison is CLIP 86.4%
  versus the retired candidate's 85.8% top-1; no new accuracy or speed claim is made.
  The evaluation example and reproduction instructions now support only CLIP.

## Established sources followed

- Existing [CLIP implementation](../../crates/li-seg/src/clip.rs),
  [Smart Sort specification](../specs/smart-sort.md),
  [classification path](../../crates/lc-engine/src/smart_sort/classify.rs) and
  [embedding store](../../crates/lc-engine/src/smart_sort/store.rs).
- The installed serde_derive 1.0.229 source, `src/de/struct_.rs:276–291`, uses
  `MapAccess::next_value::<IgnoredAny>` for unknown fields when `deny_unknown_fields`
  is absent. The repository's existing serde defaults and library loader already
  follow that convention; no migration handler or dependency is needed.
- Existing Library `headless.rs`, Compositing `egui_kittest` tests, and
  [attributions generator/check](../../xtask/src/attributions.rs).

## Validation

All required checks passed. Commands used Rust 1.98.1, `--offline`,
`CARGO_BUILD_JOBS=3` and one Cargo process at a time. The four changed Rust crates'
full suites were each run once, serially, after development checks and Clippy.

| Changed crate | Unit tests passed | Integration tests passed | Ignored | Doc tests |
| --- | ---: | ---: | ---: | ---: |
| li-seg | 44 | 0 | 4 | 0 |
| lightcraft-engine | 423 | 384 (381 coverage, 1 backend, 2 panic-hunt) | 6 | 0 |
| lightcraft-ui-egui | 230 | 0 | 3 | 0 |
| photocraft-ui-egui | 991 | 73 (59 GPU, 14 UI/system) | 9 | 0 |

**2,145 runnable changed-crate tests passed, zero failures.** The **7** attribution
checks passed, including generated-JSON freshness, port completeness, licence-file
references and CLIP's single model attribution. The **4** embedded-attribution data
consumer tests also passed: **2,156 final checks/tests passed in total**.

I18n checks are included in the full UI suites: **17 Library** and **39 Compositing**
tests passed, including catalog consistency, placeholder agreement, source-string
coverage, language switching and old settings.

Earlier focused checks passed **47** tests: model table **1**, engine Smart Sort
**33**, Library Smart Sort headless UI **10**, Compositing tagging/cancellation **2**
and settings rows **1**. The machine restart interrupted Compositing compilation
before its focused tests ran; that build was resumed, and successful checks were
not repeated because of the restart.

The offline compiler check passed for all four changed crates. Clippy passed for
all four with `--all-targets -- -D warnings` and `CARGO_TARGET_DIR=target/clippy`.
Formatting checks and `git diff --check` passed. Cargo.lock and both archived result
JSON files have no diff. Final tracked-reference audit finds the retired model
name only in the archived evaluation report and its result JSON.

**GPU tests added: 0**; tagging inference is CPU-based. The existing engine backend
test asserts GPU stage use and reports **NVIDIA GeForce RTX 5090 (Vulkan)**.
Compositing's **59 runnable GPU integration tests** passed; hardware icon tests at
scales 1 and 2, draft/refinement snapshots and group-drag checks also identify the
RTX 5090/Vulkan discrete adapter, driver **615.71.09**. Hardware runs used
`LC_REQUIRE_GPU=1`, `LIGHTCRAFT_GPU_BACKEND=vulkan`, `WGPU_BACKEND=vulkan` and
`WGPU_ADAPTER_NAME=NVIDIA GeForce RTX 5090`. No no-adapter skips or software-adapter
results are used as GPU evidence. No desktop GUI or ComfyUI jobs were opened.

The existing craft-fonts build warning remains. The host's external `li-test-runner`
also printed a nonfatal RSS-accounting arithmetic diagnostic after the Library
suite, while every test and Cargo returned zero; read-only inspection found its
`/proc` sampler can produce an empty value as a process exits. The host wrapper
was left untouched as required by the task.

Logs and the parsed count summary are under `target/remove-model/`: `check.log`,
focused logs (Compositing's completed ones end in `-resumed.log`), `clippy.log`,
`attributions.log`, `attributions-data.log`, four `full-*.log` files and
`summary.json`. The serial final command runner is `run_final.py` in that directory.

## Remaining work

None for the requested task. Changes remain uncommitted. No new dependencies,
unsafe code, C/C++ code or -sys crates were introduced. Existing downloaded model
files outside this worktree are untouched.
