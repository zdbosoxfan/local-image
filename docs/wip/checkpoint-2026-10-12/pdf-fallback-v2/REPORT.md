# FS009 bounded PDF image fallback — corrected source-only handoff v2

Frozen in `/home/zdavidson/.codex/worktrees/pdf-raster-fallback/Local Image - Testing`, base `a8a6ddcfccea8744c4ff76df0503a048f683d23c`. This supersedes v1 for implementation. V1 remains immutable evidence. No source was changed in the integration candidate; no Cargo/build/tests/probes/renderers/network/downloads/commits/agents were run. Pinned Rust 1.98.1 rustfmt (owned files, skip_children), whitespace check and reverse-apply check passed. Every runtime/UX assertion below is **authored, not executed**.

## Review corrections

### PF-UI1: actual Place/drop routes and intent

The actual shared methods are `pc-ui-egui/src/file_open.rs::place_bytes` and `place_next_dropped`; the View menu calls the former. The picker command itself and lib.rs drop dispatch need no edits. Both byte drops and the absolute native-path branch now offer the PDF chooser before generic decoding. The native-path probe retains bounded reads/header recognition; non-PDF routing remains unchanged.

Placement captures its document/revision/selection/anchor and original bytes before metadata work. The source name/path is retained as dialog provenance, never assigned as a native document Save path. Shared Place intent disables Open-as-new-document and rejects stale destination before decoding/publication. Pending choices hold the drop queue and do not start Free Transform on a previously active layer. A successful route hands off to Free Transform only if document, revision and active-layer identity still match the just-published result; no changed document/layer is transformed.

The existing engine PDF special case already ignored `linked`. This correction does not preserve that silent loss or invent a live-PDF-link implementation: Place Linked shows an **unchecked** explicit “Place embedded artwork instead (no live link)” confirmation, preserving its original intent/path until consent or Cancel. Native and image placement are blocked until consent; unchecking cancels prepared work. Source bytes/document/selection remain unchanged on refusal/cancellation. Non-PDF linked placement still follows its original engine path.

### PF-UI2: cancellation wins over ready output

Worker responses are collected into a detached `ready_result`; collection cannot publish layers/documents/warnings. Canvas input no longer publishes a ready worker response. The PDF window defers actions and publication until its Cancel, option changes and close-button state are known for the frame. Escape/focus-loss cancellation is evaluated before ready-result acceptance; popup, text-edit and top-modal keyboard ownership are respected. Another modal's Escape is not consumed or used to cancel PDF work. Ready output waits while another popup/modal/text input owns interaction.

No-document Open uses this same window-owned path, so it does not depend on a canvas callback. Captured Place staleness is checked before publishing even warnings/fallback choices. Source replacement or changed/closed/switched/revised/selection-changed destinations cannot publish an old result. Existing direct preview release remains an explicit placement gesture, not an implicit worker publication.

### PF-R1: uncertain cleanup cannot release admission

A module-level admission state records an exact uncertain owned unit. Normal guard drop releases a healthy active slot. An uncertain stop poisons the session; guard drop cannot erase that record, and every later renderer request is denied with an explicit session-disabled error. No automatic “unit absent, so it must have stopped” recovery is attempted.

For ordinary cancellation, teardown now requires the trusted systemd-run launcher's **confirmed-start acknowledgement** before a collected/absent unit can be treated as terminal. The helper no longer suppresses that acknowledgement with --quiet. This closes the start/stop race while retaining existing systemd/prlimit/bubblewrap limits and independent RuntimeMaxSec. If cleanup remains uncertain, the owned unit identity is retained and image import stays disabled for the session. A pure admission regression covers normal release, poisoned identity retention and denied re-admission.

### Independent geometry oracle

The native runtime test now authors a PDF with nonzero MediaBox and CropBox origins, four known RGB quadrants and an off-centre black marker. It checks RGB/opaque alpha at independently calculated interior/marker/control positions for all 0/90/180/270 rotations × Crop/Media × applied/ignored rotation. Expected pixels derive from authored PDF coordinates, not a second Poppler rendering or only output dimensions. The Type4 constant-red analytic control remains.

## New authored UI regressions

- Actual File→Place Embedded/Linked picker callbacks: no pre-confirm mutation; unchecked link confirmation; real Cancel leaves original data/selection/source untouched; explicit reselect/confirmation and image Place publishes once with Undo/Redo and no Save-path assignment.
- Actual canvas-drop routing: absolute path with 1024 leading bytes uses the bounded native path instead of egui's reader; nonabsolute byte drop reaches the same chooser; following queued file waits; Cancel preserves the pending queue and destination.
- Deterministic queued-ready result plus **real next-frame Escape, Cancel pointer click, Close-window pointer click or focus loss**, separately for Open with no document, direct Place and Preview. Tests assert document/layer/history/revision/selection/path/journal/warnings remain unchanged. Positive no-cancel frames accept exactly once and preserve Undo/Redo.
- Ready output followed by source replacement, destination closure/switch/revision/selection change cannot mutate the resulting current session.
- Another actual top modal consumes Escape while the ready PDF response waits without publication/cancellation.

There are now fourteen new tests total relative to the original base, including the one explicitly ignored **required desktop runtime gate** from v1. Its geometry assertions were strengthened; no numerical thresholds or prior failure assertions were weakened. No successful test result is claimed.

## Artifacts / integration modes

| Artifact | Purpose | SHA-256 |
|---|---|---|
| `implementation.patch` (103,837 bytes) | Full cumulative nine-file source patch against base; review/reconstruction | `46ce29ff41105a2e1150cc6cfff433561c19a442f0a928eb98d8156668146c4c` |
| `core.patch` | Seven non-shared source files | `e7b3d135bc0f36eb526d91a89772b737f9d6258b4044035ca324cb217e1bcd7e` |
| `shared-lib-services.patch` | Only reserved lib.rs Services field/module hunks | `9ba98fe6c29a9969e0864b0e06c0108914bb09e58a0f9dcdf514eb859ee4b96d` |
| `place-route-file-open.patch` | Only two granted Place/drop methods | `7494c46f6b32fbbd6cf211032a971bb4eb39ac55b8ba480957d71368a08604f8` |
| `correction-from-v1.patch` | Delta atop exact frozen v1 plus original file_open base | `c141a97cd3686b4ea313a6b86d458f07256d9304de7d56fa8df8df02a94d7fad` |
| `strings-append-only.patch` | Cumulative pending-string additions; root merges | `3130aa87b21e73cfbfceba4145e6a25ba2df469d10cb4d31ba4f628ce6434069` |

Use the verified artifact manifest for every application. Do not combine the full patch with its split equivalents or layer the full v1 patch over v2. `base-sha256.json`, `sha256.json`, source snapshots and `files.txt` define exact ownership. If v1 is already applied, use its correction delta; otherwise use the core + two shared hunks, with current-file inspection.

Preserve Editor's separately granted lib.rs early-Escape/rotation logic hunk. No lib.rs logic, View rotation mapping, canvas code, View picker arm, engine document schema/compositor/exporter, Library/shared-shell files or dependencies changed. The only extra file beyond v1 is file_open.rs in the two explicitly granted methods. Pending strings were **not edited**; merge additions from `strings.json` / `strings-since-v1.json` while retaining Editor/shared-shell keys. The Windows cache-handle fixture fix remains its own unrelated patch, excluded here.

## Required coordinator validation and remaining scope

Start with exact-source strict lint in the controlled lane, then focused `pdf_raster`, `fallback_boundary_tests`, `pdf_import::tests` and existing `file_open::tests` plus affected native PDF tests. Run the explicit native gate in the intended Linux user context:

```text
cargo test --offline --frozen -p local-image --bin local-image --features local-image/heif services::pdf_raster::tests::pdf_raster_linux_runtime_geometry_calculator_cancel_and_output_limit -- --ignored --exact
```

The native provider still requires system Poppler/bubblewrap/prlimit/ldd and a user systemd session. The packaged app/CLI currently does not supply that runtime closure; release declaration/readiness and actual isolated-install acceptance remain required. No unrestricted retry or package installation is added.

Typed eligibility remains limited to explicit JPEG2000/JBIG2 image-decoder and calculator-function refusals. Named common native features stay native; K=true/password/resource/invalid/external-stream refusals are not weakened. General unsupported-group/colour/wrapper routes are not claimed complete. JPEG2000/JBIG2 fidelity, full hostile-input and descendant stress, runtime error/cleanup policy and actual native Open/Place/cancel/save-reopen/export/install workflows still need execution evidence. This is not full FS009 or release closure.

Frozen for independent UI/runtime re-review. Coding permit released; no further source edits without coordinator scheduling.
