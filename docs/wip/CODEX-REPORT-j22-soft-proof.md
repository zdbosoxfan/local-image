# J22 / D6 — Develop soft proofing through the shared CMS

Completed on 2026-10-14. Changes are uncommitted. Continued the existing branch at
`3f949a02`, using its shared J2 colour math, CMS/display service, and shared UI widgets;
no existing implementation was reset. The task-file modification present at entry
was preserved.

## What merged

- Replaced Develop's matrix-only proof and private desaturation-based gamut warning
  with `photocraft_cms::Transform::proof` and `GamutCheck`. Compositing and Develop now
  use the same pure-Rust colour-management backend. The removed production proof
  implementation survives only as a small test reference for the comparison below.
- Retained all five named proof spaces. Added local bidirectional ICC loading, including
  RGB, gray and CMYK printer profiles; all four CMS intents; black point compensation;
  and the shared paper/ink simulation. The profile field and Load ICC Profile button
  accept a local path. `view.softProof` accepts `profilePath`, `intent`, `bpc`, and
  `simulatePaper` in addition to its previous fields. A named-space selection clears
  the custom profile; `profilePath: null` clears it too.
- Profiles retain their original bytes and parsed model. Serialization carries the
  bytes, rather than an external path, so restoring settings does not depend on that
  file still existing. Old camelCase named-space settings load through serde defaults.
  Bad profiles/paths/patch values do not change the active proof. File and serialized
  profile inputs are bounded to 16 MiB. Stage builders validate both profile directions
  for all intents without building expensive preview tables on the UI thread.
- Proofing runs on the completed float edit, including grain and profile LUT, before
  final quantization. Precise CMS evaluation is used; no 8-bit intermediate or fast
  device-link approximation is inserted into the proof calculation.
- UI proof requests carry the active shared monitor profile. The CMS encodes directly
  for that monitor, avoiding an sRGB clipping intermediate and a second display
  conversion. Monitor identity participates in render/cache keys; existing monitor
  change invalidation remains in use. This monitor target is transient preview state,
  not an export colour-space preference.
- Both warnings use the completed original colour. Destination is red, monitor is
  blue, and simultaneous loss is pink. Monitor checking uses the active monitor ICC.
  Link failures return a normal RenderJob error instead of substituting another
  printer, intent or monitor. A destination-only monitor can display a proof but
  cannot provide a round-trip monitor gamut warning; enabling that unavailable
  operation reports its linking error.
- Added fallible `render_checked` / `render_cached_checked` entry points. Existing
  `render` / `render_cached` signatures remain thin adapters; an invalid proof yields
  an empty result through those infallible APIs. Actual application jobs preflight
  the link and return its error. `Proof::params` is now fallible and returns a shared
  `Arc<ProofParams>`; `try_params` is its explicit construction entry point.
- Eight recent parameter sets retain shared transforms/gamut checks. Cold construction
  is serialized to avoid duplicate table builds by simultaneous preview workers.
  Image-independent keys cover ICC content, destination space, monitor, intent, BPC,
  simulation and warnings. Proof jobs bypass both ordinary thumbnail and view caches.
  Existing CPU proof fallback is retained; there is no GPU proof implementation added.
- Rebuilt soft-proof rows with the existing `develop_body`, `panel_note`, `choice`,
  `field_with_action`, `checkbox`, `text_button` and divider widgets. The new controls
  add no bespoke colours, sizes, spacing or icons. Proof-copy naming uses the selected
  ICC description. New strings are listed in `i18n-pending-j22-soft-proof.md`.

## Measured quality and speed

All data was synthetic/demo data and the existing shipped synthetic CMYK ICC.
No owner photographs or Desktop files were used. Generated artifacts are under `target/`.

| Small-sample measurement | Before / reference | After |
|---|---|---|
| Agreement with the shared CMS on a fixed 64-patch Rec.2020 chart, default sRGB proof | Legacy matrix/desaturation: mean DeltaE76 **10.663726**, maximum **47.456703** from the CMS reference | Precise new adapter: **0** reference error |
| Imported ICC proof parity, 8 profile types, two destinations, four intents, three BPC/paper combinations, seven patches | Direct shared CMS using the same parsed ICC bytes | **1,344** evaluations; maximum device-channel error **0** |
| Complete grain-bearing float edit through CMYK, U8/U16/F32 output | Direct shared CMS applied to the completed float edit, then device-range clipping/encoding | **288 pixels × 3 depths**; maximum linear-float error **1.4901161e-8**; U16 within 3e-5 and U8 within 2/255 of reference |
| Scalar evaluation of the 64-patch chart, one small timed sample | Legacy matrix: **15.189 microseconds** | Precise CMS: **12.320 microseconds** |
| Actual Session/RenderJob path, 192×128 fit, shared StageCache | Ordinary preview: **406.688 ms**, including startup work | CMYK proof cold: **317.307 ms**; cached proof: **2.779 ms**; exposure command plus draft proof render: **1.241 ms** |

The quality numbers measure agreement with the selected shared CMS, not physical
printer accuracy. The old method's hue-preserving luminance desaturation differs
materially from CMS device clipping; that is an intentional user-visible policy change.
Printer-specific quality remains dependent on the supplied ICC and the shared CMS.

The scalar timings are a tiny diagnostic sample, not an interactive benchmark or a
basis for a general speed claim. The real job sample ran on the actual
**NVIDIA GeForce RTX 5090 (Vulkan)** application path; proof jobs used the established
CPU route. Cold timings include startup/concurrent UI profile construction and vary:
an earlier development run recorded cold proof 10.678 ms and cached proof 1.047 ms.
These measurements justify retaining cached parameter construction; they do not claim
an overall GPU speed improvement or a measured old-versus-new interactive speedup.
No production setting is tuned to this machine. Existing bounded workers, Rayon and
stage-cache reuse continue to adapt work to available hardware.

## Verification

Followed the task's later **ABSOLUTE MINIMUM TESTING** override. One cargo process ran
at a time, offline, with `cargo +1.98.1`, the existing target directory and the inherited
`CARGO_BUILD_JOBS=4`. No full crate/workspace suite, GPU acceptance run, benchmark
program or long quality sweep ran.

- `cargo +1.98.1 fmt -p lightcraft-pipeline -p lightcraft-engine -p lightcraft-ui-egui -p lightcraft-gpu`: completed.
- `cargo +1.98.1 check --offline` for those four packages: passed.
- `cargo +1.98.1 clippy --offline` for those four packages, `--all-targets -- -D warnings`: passed, with no lint suppressions added.
- `cargo +1.98.1 test --offline -p lightcraft-pipeline j22_proof -- --nocapture`: **6 passed**.
- `cargo +1.98.1 test --offline -p lightcraft-engine j22_proof -- --nocapture`: **2 passed**.
- `cargo +1.98.1 test --offline -p lightcraft-ui-egui j22_proof -- --nocapture`: **3 passed**.
- Total: **11 new tests passed**. Existing unrelated tests were filtered out.
- Tests cover parsed-ICC/CMS parity, legacy JSON defaults, byte serialization, cache/key
  changes, both warnings and active-monitor changes, unavailable monitor directions,
  explicit checked-render errors, all three output depths, post-grain ordering, atomic
  command patches, thumbnail-cache isolation, direct monitor pixels avoiding double
  conversion, and actual UI dropdown/checkbox/text-field/load/copy clicks.
- The UI test exercises **dark/light × 250/400 pt × 1.0/1.5 scale** and asserts dock
  bounds. All **eight PNGs were visually inspected**: no clipped controls or overflowing
  rows; long selected profile names elide, notes wrap, and the load action stacks.
  Screenshots: `target/j22/proof-*.png`.
- **GPU tests added: 0.** Existing GPU tests received ownership/clone fixes only and were
  compiled by strict clippy, not executed. Proof computation remains CPU-only by design.
- `git diff --check` on changed source, lockfile and new documentation: passed. The
  pre-existing task-file trailing-blank-line warning was not edited away.

Final logs: `target/j22-final-check.log`, `target/j22-clippy-final.log`,
`target/j22-pipeline-final.log`, `target/j22-engine-final.log`, `target/j22-ui-final.log`.
Earlier failure/debug logs were retained under `target/j22-*.log` (and the first parity
failure under `target/j22-pipeline-attempt-1.log`). Initial failures identified ownership
moves, use of an unparsed reference model, missing final device clipping in the test
reference, and the infallible parameter assertion; these were corrected without
relaxing parity tolerances or the crate's panic/lint rules.

The inherited craft-fonts build warning remains; it is unrelated to this task's
English controls and does not prevent check/clippy or the new tests from passing.

## TEST-CHANGED and compatibility

- **TEST-CHANGED:** `lc-pipeline/src/tests.rs::soft_proof_maps_into_the_proof_gamut_and_flags_what_does_not_fit`
  now expects CMS proofing to differ from an ordinary gamut-desaturated sRGB preview.
  The previous identity assertion encoded the removed matrix policy. New exact CMS
  parity and complete-render tests provide the replacement quality gate. Existing
  explicit Proof literals gained defaults for the new fields.
- **TEST-CHANGED:** `lc-ui-egui/src/tests_masking.rs::soft_proofing_flags_out_of_gamut_colours_and_makes_proof_copies`
  expects the command result's added profile/intent/BPC/simulation metadata.
- **TEST-CHANGED (ownership only):** the requests in `lc-pipeline/src/visualize.rs`,
  `lc-engine/src/export.rs`, `lc-engine/src/tests_gpu.rs`, and the three GPU test files
  listed below now clone when reused. Assertions and quality thresholds are unchanged.

`Proof` and `RenderRequest` are Clone rather than Copy because they now own shared ICC
state. All repository callers needing ownership adaptation were moved. `ProofParams`
is a CMS adapter rather than a public matrix bundle, and `Proof::params` returns its
linking error. Existing crate/module paths, render entry points, named-space JSON,
proof toggle, old warning fields and proof-copy action remain available.

## Unavoidable edits outside the three owned files

| File | Reason |
|---|---|
| `crates/lc-pipeline/Cargo.toml`, `Cargo.lock` | Reuse the already locked workspace `photocraft-cms`; one dependency edge, **zero new packages/versions or C/-sys dependencies**. |
| `crates/lc-pipeline/src/finish.rs` | Remove the duplicate proof algorithm and evaluate the shared CMS after the completed float edit. |
| `crates/lc-pipeline/src/lib.rs` | Re-export profile/intent adapters, own ICC state, expose checked renders and adapt borrowed requests. |
| `crates/lc-pipeline/src/tests.rs` | Render-depth/post-grain parity gate and obsolete assertion update. |
| `crates/lc-pipeline/src/visualize.rs` | One reused test request needs a clone. |
| `crates/lc-engine/src/lib.rs` | Export the view-state patch helper. |
| `crates/lc-engine/src/media.rs` | Proof cache keys, thumbnail/view cache isolation, linking errors and monitor-encoding metadata. |
| `crates/lc-engine/src/export.rs`, `crates/lc-engine/src/tests_gpu.rs` | Mechanical request ownership fixes in existing tests. |
| `crates/lc-ui-egui/src/menus.rs` | Route all soft-proof patches through atomic engine validation and expose their state. |
| `crates/lc-ui-egui/src/panels/detail.rs` | Carry the shared monitor into proof jobs and show the ICC label. |
| `crates/lc-ui-egui/src/render.rs` | Supply the monitor, skip a second monitor conversion and test that boundary. |
| `crates/lc-ui-egui/src/tests_masking.rs` | Existing command-result metadata expectation. |
| `crates/lc-gpu/src/remaining_tests.rs`, `crates/lc-gpu/tests/it/equivalence.rs`, `crates/lc-gpu/tests/it/toolset.rs` | Mechanical clone fixes so existing GPU tests still compile with owned proof state; no GPU implementation changes. |
| This report and `docs/wip/i18n-pending-j22-soft-proof.md` | Required task report and deferred translation list. |

No Match Look files were changed. No GUI window was opened or running application/service
was stopped. No files outside the worktree were modified.

## Sources and remaining work

Followed the existing `pc-cms` transform, gamut, built-in/profile and precise-float APIs;
`pc-engine/src/color_cmds.rs` and `display_color.rs`; the audit's D6/J22 scope;
`docs/DESIGN-SYSTEM.md`; and the shared Library headless/widget conventions.

Official behavior references:

- [Adobe Lightroom Classic Develop module options](https://helpx.adobe.com/ie/lightroom-classic/desktop/process-and-develop-photos/develop-module-options.html):
  monitor/destination warnings, pink overlap and proof copies.
- [Adobe Photoshop proofing colors](https://helpx.adobe.com/in/photoshop/using/proofing-colors.html):
  custom device profile, rendering intent, BPC and paper/black simulation.

**Nothing remains for J22 implementation.** Deferred by the owner's explicit testing
and translation policy: merged full-workspace/GPU acceptance verification and translation
of the pending strings. Display/proof transformations use the already shared pure-Rust
CMS's supported ICC features; no alternative colour-management implementation was added.
