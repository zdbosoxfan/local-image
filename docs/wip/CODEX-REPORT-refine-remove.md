# Confirm AI Remove and restore Draft/Refinement — implementation report

## Part 1: AI Remove

- Painting accumulates an exact document-space removal mask, shown in red after release; additional strokes union into it and Alt/Option strokes subtract coverage. The live brush trail is also tinted red. Painting does not change pixels, selection, or undo history.
- The floating confirmation bar is anchored to the painted bounds, stays inside the canvas, and mirrors Remove, Keep painting, Clear, Cancel, and the engine picker in the options bar. Enter submits; Esc discards, including an unfinished gesture. Keep painting hides the floating bar until another stroke while the options controls remain available.
- Remove Selection from the options bar, contextual task bar, Edit menu, and legacy `ai.remove` menu/shortcut invocation now previews the selection and waits for confirmation.
- Clear/Cancel/Esc and changing tool/document discard the pending mask. The persisted “Remove immediately on release” opt-in defaults to false, including when loading old settings.
- Only confirmation (or the explicit immediate opt-in) submits the mask. The existing background `ai.remove` command adds the repair as one undo step. Explicit masks are size-validated and preserve subtracted holes.
- Four added kittest tests cover real strokes, two-stroke union, subtraction, selection preview, both bars, Keep painting, Clear/Cancel/Esc, document/tool switches, Enter/button confirmation, one submission, immediate mode, and undo. The in-process service mock runs without sockets; an additional test path uses the real ComfyUI mock when local sockets are allowed. One engine test validates the explicit mask and rejects invalid/empty masks.

## Part 2: Draft/Refinement

- Read all six specified frontend files from `main` with `git show`; the installed Legacy app was not run. The existing Generate implementation, job machinery, and library were extended.
- Open from the Generate/Refine panel, Image > AI > Draft / Refinement, or Window > Draft / Refinement. Native hosts use an immediate egui viewport; embedded hosts use a movable, resizable modal. Both large previews remain visible side by side; Draft/Refinement tabs switch the independent settings and result actions, following the closing tabbed-layout note. The previews have independent fit/actual-size/zoom/pan cameras and a draggable comparison slider. Wheel zoom preserves the pixel beneath the pointer; F, 1, +/−, and arrow keys operate the focused preview. The embedded modal keeps editor shortcuts from taking its preview keys.
- Each stage has model/variant, prompt, aspect, width/height, and always-visible Steps/Guidance; Refinement also has Denoise. Model changes reset Steps/Guidance/Denoise to the model/family defaults, and edits clamp to the real model ranges (Denoise 0–1). Fixed sampling values remain visible. Working sizes use the model's size grid.
- Refinement defaults to the current open-image composite, with explicit Selected layer and Draft result choices. Pixels are freshly captured when Refine is clicked; the source preview is cached only for unchanged document revisions. Chosen output dimensions are honored without replacing the default open-image pixels. The comparison keeps the input snapshot associated with the result.
- The original dock's Mode::Refine already assigned `request.source`. A real failure path was retained Create references: those controls disappear in Refine, but their images could still fail request validation. Refine/Upscale requests now drop those hidden Create references. A shared request builder and explicit composite/layer choice prevent the two UIs from diverging.
- Use places a generated result as one new layer/undo step; Open as new document uses the existing importer. Every generation still goes through the existing library save and result-history path; each stage also has a library history picker. Generation progress/cancellation uses the existing job system.
- Seven added tests cover opening via the panel, preview geometry at 1280×800 and 1920×1080 (each preview at least 500×300), tab switching, sampling drags/clamping, fresh composite/layer/draft requests, dropping hidden references, zoom/pan/comparison, old-setting defaults, and Use/Open/library history. The ComfyUI integration test checks an image upload/VAE source and saved results when sockets are available.

## Verification

All commands used Rust 1.98.1, offline Cargo, `CARGO_BUILD_JOBS=3`, and one Cargo build at a time. Test AI data was isolated with `LOCAL_IMAGE_DATA_DIR=/tmp/refine-remove-test-data`; no desktop GUI or owner files were changed. No dependency manifests or lockfiles changed, no unsafe code was added, and no commits were made.

| Crate / target | Passed | Existing ignored tests |
| --- | ---: | ---: |
| `photocraft-ui-egui` unit | 952 | 4 |
| `photocraft-ui-egui` integration (`it`) | 14 | 0 |
| `photocraft-ui-egui` GPU target | 54 | 3 |
| `photocraft-engine` unit | 819 | 11 |
| `photocraft-engine` command coverage | 560 | 0 |
| `photocraft-engine` integration | 44 | 4 |
| Both crates' doc tests | 0 | 0 |

The UI full-suite attempt had 950 passing unit tests, 4 ignored tests, and two failures from translation-completeness checks requiring translations of the new labels. The task explicitly says translations are done separately. Those checks now have a narrow, documented `PENDING_AI_TRANSLATIONS` list containing only this task's English additions; the existing English fallback is unchanged and all unrelated labels remain required. The filtered translation checks passed (22 tests, including both failures). The full unit suite was not repeated; its previously unreached GPU/integration/doc targets were run separately and passed. The engine full suite ran once and passed.

Clippy passed for both changed crates with `-D warnings`; formatting and `git diff --check` passed. Final commands:

- `cargo +1.98.1 test --offline -p photocraft-ui-egui -- --nocapture` (single full attempt)
- `cargo +1.98.1 test --offline -p photocraft-ui-egui i18n::tests:: -- --nocapture` (translation failure follow-up)
- `cargo +1.98.1 test --offline -p photocraft-ui-egui --test gpu --test it -- --nocapture`
- `cargo +1.98.1 test --offline -p photocraft-ui-egui --doc`
- `cargo +1.98.1 test --offline -p photocraft-engine -- --nocapture`
- `cargo +1.98.1 test --offline -p photocraft-ui-egui draft_refine_sampling` (model picker, catalogue defaults, editable/clamped sampling)
- `cargo +1.98.1 test --offline -p photocraft-ui-egui draft_refine_large_previews` (both resolutions, tabs, default values, visible actions)
- `CARGO_TARGET_DIR=target/clippy cargo +1.98.1 clippy --offline -p photocraft-ui-egui --all-targets -- -D warnings`
- `CARGO_TARGET_DIR=target/clippy cargo +1.98.1 clippy --offline -p photocraft-engine --all-targets -- -D warnings`
- `cargo +1.98.1 fmt -p photocraft-ui-egui -p photocraft-engine`; `git diff --check`

After the suite targets completed, the final default audit aligned Denoise with the family catalogue rather than the old dock's fixed default; only the affected sampling/model-picker and window/default tests were rerun. Clippy was rerun for the UI crate after that change. Development runs otherwise used only added/affected filters and crate checks, never a whole-workspace suite.

## GPU and remaining work

No GPU kernels or GPU-specific tests were added: this change uses egui texture overlays, existing compositing, and the existing AI service. The final UI GPU target actually ran using `llvmpipe (LLVM 22.1.8)` software Vulkan (`DeviceType::Cpu`), with 54 passed and 3 ignored benchmarks; it did not validate the RTX 5090. Native window behavior and real-model inference remain coordinator desktop/RTX 5090 checks because this task forbids opening desktop GUI windows and the sandbox has no hardware GPU.

The two added real ComfyUI mock checks reported `skipped: sandbox denies mock sockets`; the in-process request/mask and UI interaction tests executed. An existing engine mock HTTP check also reported a socket-denied skip. These early returns count as passing in Rust's totals above.

The existing external `li-test-runner` memory watchdog printed arithmetic warnings once as a short-lived target exited; the target and Cargo still returned success. Its files were left untouched.

Implementation is complete. Translations remain intentionally deferred to the separate translation job; remove the explicit pending-string entries as translations land.

## New English UI strings

New direct labels use `tl!`; menu catalogue labels use the existing translated menu renderer. Existing labels (Remove, Keep, Clear, Cancel, Engine, Model, Precision, Steps, Guidance, Width, Height, Fit, Close, etc.) are reused. New English sources:

- `+`
- `Choose or generate an image.`
- `Compare with input`
- `Comparison`
- `Create a draft, then choose Draft result as the refinement input.`
- `Denoise`
- `Describe the draft…`
- `Draft`
- `Draft / Refinement`
- `Draft / Refinement…`
- `Draft preview`
- `Draft result`
- `Generate draft`
- `Input`
- `Keep painting`
- `Open as new document`
- `Open image (composite)`
- `Paint to mark removal; Alt/Option-drag subtracts`
- `Refine image`
- `Refinement`
- `Refinement preview`
- `Refinement uses the current pixels of the open image.`
- `Remove immediately on release`
- `Running…`
- `Selected layer`
- `Wheel to zoom; drag to pan`
- `−`
