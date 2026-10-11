# Package 37 — view modes and isolation

Date: 2026-10-10. Branch: `wip/vec-37-view-modes-isolation`; integration lane: `wip/vector`.

The owned implementation is compiled and supplied with interaction tests, explicit production
integration fragments, labels and source records. **Integrated acceptance remains pending**:
the task reserves application menu/router/canvas edits for 00/09. This report does not claim
that the running application already exposes these modes, that GPU acceptance passed, or
that the 5000-path performance target was measured.

## Continuation and scope

Inspected the existing branch, owned files, shared APIs and current tests before editing.
The starting HEAD was `571f0cd4`; the package-37 implementation files were absent, while
the merged vector foundations and existing package providers were present. Those providers
were reused rather than replaced. Read the architecture, persona specification including
Owner decisions, quality plan, package README and gap audit, and followed the final
2026-10-10 speed/registration overrides in `CODEX-TASK.md`.

Changes remain uncommitted. Shared edits are limited to the permitted append-only module
registrations in vector/lib.rs, engine/vector_packages.rs and ui-egui/lib.rs. No manifest,
lockfile, dependency, registry aggregate, persona map, canvas router, settings schema,
central translation catalog or attribution catalog was edited. No desktop window was opened;
no running owner application was stopped. Builds used the existing `li-rustc-gate` wrapper,
offline Rust 1.98.1 and `CARGO_BUILD_JOBS=3`, with one cargo build at a time.

## Implemented behavior

- **Outline:** device-space one-pixel strokes built with the existing vector exact-area
  renderer. Original fills/strokes/effects do not participate. Raster/smart bounds have
  a box and cross; text uses pc-text glyph/on-path outlines, including quadratic conversion;
  native corner and symbol geometry reuse their existing providers. Actual path-distance
  hit testing exposes underlying paths despite overlapping fills, and does not invent
  closing segments for open paths. Clip/mask paths are visible wireframes.
- **X-ray:** detached fill-only, effects-free appearance projection, with leaf fill opacity
  multiplied by the documented Local Image value **0.20**. Outline overlays sit over the
  complete composite. Existing native symbol expansion and text preparation are reused.
  Advanced live projection requires the shared 08/09 provider and fails explicitly when absent.
- **Pixel Preview:** exact inverse-transform nearest sampling of the existing 1x CPU
  compositor cache, bounded to the visible source/device viewport. Handoff specifies the
  production 09 1x texture bypass, nearest sampling and HiDPI behavior.
- **Per-layer/group outline:** runtime flags inherit through the source tree; Ctrl/Cmd-eye
  consumes the click before ordinary visibility handling. Overrides suppress their original
  paint, including X-ray fill. Document switches clear the UI flags/mode.
- **Isolation:** validated group/clip-group, compound and symbol targets; nested breadcrumbs;
  outside dimming at **0.25**, applied only to maximal outside roots; atomic selection scope
  and Select All query; double-click/Alt-thumbnail entry, outside double-click and Escape
  exit. Persona/document changes exit and cancel capture. Captures retain document,
  revision, scope generation and pointer target ID; stale/changed releases cannot enter.
- **Insertion:** prepared root/group objects commit in one `Session::edit`, with stale
  capture, whole-tree ID/path/cache/depth and anchor checks. Symbol insertion exposes the
  inverse instance transform and delegates to existing `symbols::update_master`, refreshing
  dependent instances with one undo. Compound insertion has an explicit package-35 provider
  hook and errors when missing; it never falls back to document-root creation.
- **Session state/commands:** no Document or serialized-settings additions. Enter/exit,
  scope and view command specs are non-journaled. Captured view requests validate before
  changing state. Authoritative per-session storage hooks and the requested view command
  slot are supplied as exact 00 fragments; absent storage hooks produce command errors.
- **Paint/cache boundary:** detached base, exact-device raster/geometric overlay fragments,
  scope-aware hit queries and a retained frame cache keyed by source Arc identity, revision,
  dependency generation, mode, per-layer flags, scope, affine and viewport. This also
  distinguishes detached previews that retain the document revision.

## Verification

Logs are local under `target/view37-*.log`; they are build artifacts, not committed fixtures.

| Changed crate | New CPU tests passed | Failed | GPU tests ignored | Evidence |
|---|---:|---:|---:|---|
| photocraft-vector | 5 distinct | 0 remaining | 0 | `view37-vector-tests.log`, `view37-vector-tile-retry.log` |
| photocraft-engine | 9 distinct | 0 | 0 | `view37-engine-tests-retry.log`, `view37-engine-depth-test.log` |
| photocraft-ui-egui | 13 distinct | 0 remaining | 3 | `view37-ui-tests.log`, `view37-ui-{doubleclick-retry,outside-final,selection-retry}.log` |

- Workspace `check --offline --workspace --all-targets` was run once as instructed. It
  found an egui-0.36 API rename (`egui_wants_keyboard_input`) in the new code. The call and
  unused import were corrected. Subsequent changed-crate all-targets clippy compiled the
  corrected code/tests. The one workspace-check attempt itself is **not a green gate**;
  it was not repeated under the owner's once-only workspace-check budget.
- Strict all-targets clippy was run for `photocraft-vector`, `photocraft-engine` and
  `photocraft-ui-egui`. Initial style findings were corrected. The final combined run
  **passed (exit 0)** after all code/test changes, with `--all-targets -- -D warnings`;
  evidence: `target/view37-clippy-final-all.log`. All three changed crates were formatted
  with the specified toolchain, and `git diff --check` passed. The existing lightcraft-engine build-script
  warning about missing embedded Chinese/Japanese fonts is separate from package warnings.
- New tests use `view37_` filters only; no whole-crate/workspace suite was run. The vector
  run initially had four passes and one tile assertion failure. Same-clip repeat remained
  byte-identical; independent tiles differed by one RGBA8 byte at half coverage. The tile
  oracle was corrected to the task's premultiplied **max difference <=1/255**, keeping the
  separate exact-repeat assertion. Only that failed test was retried and passed.
- The engine filter's first attempt did not execute tests: the final optional-master-cache
  fix needed an explicit collection type. Corrected it, then the same filter executed once:
  **8 passed, 0 failed, 0 ignored**, with 1099 unrelated unit tests and 647 unrelated
  integration tests filtered out. The optional-cache symbol fixture passed.
- One additional engine boundary test was added and executed once after the final review:
  **1 passed, 0 failed**, 1107 unrelated unit tests filtered out. It verifies that insertion
  beside an anchor already nested at MAX_GROUP_DEPTH rejects a new sibling group atomically.
  Together, **27 distinct new CPU tests passed** across the changed crates.
- UI's first filter executed **10 passed, 3 failed, 3 ignored, 1075 unrelated filtered out**.
  A single failed double-click test was traced: an unrelated thumbnail response cleared
  canvas capture on release, and kittest's 250ms default event step exceeded the double-click
  window. Capture now retains its response owner, target ID and document capture; only the
  owning widget consumes/releases it. Isolation/GPU harnesses use 60Hz event steps, with
  independent click pairs separated from egui's triple-click history. Removed trace code.
  Retried only the three failed tests (each final run **1 passed, 0 failed**); all **13 distinct
  UI CPU tests passed**, with the **3 GPU gates still ignored**. Passed UI tests were not
  repeated. The unrelated count grows in individual retries because the other package tests
  are filtered out too. These tests include actual keys, eye/menu/thumbnail clicks, canvas
  double-clicks, rectangle drag/release, outside exit, breadcrumb exit and Escape.

Final lint command (wrapper remained `/home/zdavidson/.local/share/local-image-dev/bin/li-rustc-gate`):

```sh
PATH="$HOME/.cargo/bin:$PATH" CARGO_BUILD_JOBS=3 CARGO_TARGET_DIR=target/clippy \
  cargo +1.98.1 clippy --offline -p photocraft-vector -p photocraft-engine \
  -p photocraft-ui-egui --all-targets -- -D warnings
```

The tests above are distinct passing behaviors, rather than a claim of a fresh combined
full-suite run. Focused failed-test retries and the one additional depth test are listed
explicitly; no successful filter or broad suite was repeatedly executed.
- Three ignored, compiled real-app offscreen GPU gates were added: deterministic Outline
  and Pixel Preview CPU parity; isolation with actual rectangle preview/cancel/release,
  Escape/persona/undo; and release 5000-path warm pointer/GPU/readback timing. They require
  RTX 5090, Vulkan, discrete adapter, Rgba32Float and float canvas support. They assert
  production routes and do not substitute direct module calls for application wiring.
- Per the final speed override: **no GPU acceptance runs, release builds, benchmarks,
  whole-crate suites or whole-workspace test suites**. No hardware identity, performance
  or external/proprietary corpus success is claimed. GPU tests being ignored is a pending
  gate, not a GPU pass. Local CPU interaction hosts test the real module responses and
  engine insertion boundary; they do not establish production-router acceptance.

## Integration still required (ownership/dependency boundary)

Apply [integration/37.md](vector-tasks/integration/37.md) in the serial 00 lane:

1. Runtime DocState stores/provider installation and command aggregation; the new
   `vector_packages::view_modes` command slot; persona/keymap accelerator gates and labels.
2. Production menu, Layers eye/thumbnail, breadcrumb and canvas hooks, authoritative
   selection gating, document/persona transition handling and cancellation of every tool
   gesture. Rectangle and other drawing routes must call the supplied insertion boundary.
3. 08/09 shared feature geometry, dependency keys, 1x Pixel Preview base sampling, exact
   device overlay upload/paint and view damage. NativeGeometry explicitly rejects live
   groups requiring that projection; advanced symbol fills also need the shared provider.
4. 32 compound/symbol/master namespace selection/index extension. Its current group-only
   scope adapter errors on unsupported namespaces instead of allowing outside selection.
5. 35 compound insertion provider; nested symbol-master insertion/anchors need the existing
   master namespace adapter (current symbol top-level insertion requires `above=None`).
6. After routing lands, run the supplied real-app/GPU and release performance gates and
   integration milestone suites. Merge the i18n and PORTS fragments and regenerate central
   attribution discovery. Package acceptance cannot be marked complete before these gates.

These remaining edits are outside the owned file list, rather than unfinished competing
changes to core files. The task expressly requires handoff fragments for that lane.

## Sources, licenses and reproducibility

See [SOURCES.md](../../crates/pc-ui-egui/src/vector_packages/view_modes/SOURCES.md) for
official Adobe/Affinity links, pinned VectorCraft source paths/commit, existing repository
providers, tool versions, generated fixture parameters and the two conservative PORTS rows.
VectorCraft pin was verified as `d522c1d7be4035bd4f4a84cd6ebfca44f5155092`; its existing
MIT/Apache copyright/license notices remain. The implementation is original Rust under
GPL-3.0-or-later; no vendor renderer, brand asset or proprietary artwork was copied.
Affinity specifies reduced opacity without a number; 0.20 X-ray and 0.25 isolation dimming
are explicitly documented local policy choices. Bundled Inter is used for the glyph test;
its existing font notices apply. All artwork fixtures are generated and parameterized.

Fixed label fragment: [i18n/37.tsv](vector-tasks/i18n/37.tsv).
