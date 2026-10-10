# Vector work packages

Ready-to-run tasks on the vector foundations, integration branch `wip/vector`. Do not commit.

The architecture is [docs/specs/vector-architecture.md](../../specs/vector-architecture.md). Each task is an implementation job, not a request to re-plan the architecture. Finish its scope and acceptance; write its report. Preserve previous builders.

## Ownership and integration

Package 00 owns shared module/manifests/lock/UI-router/catalog wiring. Package 01 owns persona/toolset/prefs/toolbar changes. Every other package owns its listed algorithm/UI modules, test directory and a unique `integration/<NN>.md` plus `i18n/<NN>.tsv` and attribution fragment. No simultaneous edits to shared files: submit exact hook signatures/call sites to 00, which wires them while other work continues. Modules must be registered before final crate/app acceptance; isolated widgets alone do not complete a UI package.

All packages can start their isolated engine/evaluator/fixture work on these foundations. V1-dependent geometry integration awaits `wip/codex-vector-v1`; no duplicate implementation or placeholder boolean success is allowed. Rendering consumers can use injected legacy projection fixtures, but final enhanced behavior waits the named providers. Independent means disjoint ownership and stable interfaces, not pretending those final acceptance dependencies do not exist.

`structure.rs` is read-only to 15/16 during parallel work; send symbol/constraint helper additions to 00, which integrates both. 00 completes the known #1321 gradient V0 port before handing pc-vector renderer files to 08. Each package supplies GPU tests in its own uniquely named test module; 00 performs central test-binary registration. Package 08 owns plan/shape_split feature routing; 09 owns gpu_canvas and scaled-view modules; avoid touching each other’s core files.

Common new files live under `crates/` (prefix omitted in individual ownership lists for readability). Existing filenames in ownership lists must be located before editing; stale names do not authorize broad edits. Catalogue/core changes are exact integration fragments, not a reason to stop asking the owner.

## Schedule

| Task | Phase | File |
|---|---|---|
| 00 integration | shared/V0 | [00-integration.md](00-integration.md) |
| 01 persona | V2 | [01-persona.md](01-persona.md) |
| 02 live-shapes | V3 | [02-live-shapes.md](02-live-shapes.md) |
| 03 node-editing | V3 | [03-node-editing.md](03-node-editing.md) |
| 04 pen | V3 | [04-pen.md](04-pen.md) |
| 05 pencil | V3 | [05-pencil.md](05-pencil.md) |
| 06 corners | V3 | [06-corners.md](06-corners.md) |
| 07 snapping | V3/V8 | [07-snapping.md](07-snapping.md) |
| 08 appearance | V4 | [08-appearance.md](08-appearance.md) |
| 09 scaled-canvas | V5 display | [09-scaled-canvas.md](09-scaled-canvas.md) |
| 10 constructive-tools | V5 geometry | [10-constructive-tools.md](10-constructive-tools.md) |
| 11 width-profiles | V5 width | [11-width-profiles.md](11-width-profiles.md) |
| 12 path-text | V6 | [12-path-text.md](12-path-text.md) |
| 13 svg-import | V7 import | [13-svg-import.md](13-svg-import.md) |
| 14 svg-export | V7 export | [14-svg-export.md](14-svg-export.md) |
| 15 symbols | V8 symbols | [15-symbols.md](15-symbols.md) |
| 16 layout-constraints | V8 layout | [16-layout-constraints.md](16-layout-constraints.md) |
| 17 vector-brushes | V9 brushes | [17-vector-brushes.md](17-vector-brushes.md) |
| 18 live-blend | V9 blend | [18-live-blend.md](18-live-blend.md) |
| 19 live-repeat | V9 repeat | [19-live-repeat.md](19-live-repeat.md) |
| 20 live-envelope | V9 envelope | [20-live-envelope.md](20-live-envelope.md) |
| 21 pdf-export | V10 export | [21-pdf-export.md](21-pdf-export.md) |
| 22 pdf-import | V10 import | [22-pdf-import.md](22-pdf-import.md) |
| 23 psd-advanced-fallback | V4/V6/V8/V9 format bridge | [23-psd-advanced-fallback.md](23-psd-advanced-fallback.md) |
| 24 affinity-import | Affinity | [24-affinity-import.md](24-affinity-import.md) |
| 25 trace-centreline | Vectorizer T2 centreline | [25-trace-centreline.md](25-trace-centreline.md) |
| 26 trace-logo-cleanup | Vectorizer T2 cleanup | [26-trace-logo-cleanup.md](26-trace-logo-cleanup.md) |
| 27 trace-cutout | Vectorizer T2 cutout | [27-trace-cutout.md](27-trace-cutout.md) |
| 28 trace-dialog-output | Vectorizer T2 dialog/output | [28-trace-dialog-output.md](28-trace-dialog-output.md) |
| 29 library-vectorize-handoff | Vectorizer T2 Library | [29-library-vectorize-handoff.md](29-library-vectorize-handoff.md) |
| 30 frame-composer | optional V6c | [30-frame-composer.md](30-frame-composer.md) |
| 31 geometry-quality | V1-Q | [31-geometry-quality.md](31-geometry-quality.md) |

Package 30 is optional V6c; package 31 is the V1-quality follow-up. All required V2–V10, Vectorizer T2 and Affinity implementation scope is in 01–29. V1 remains a separate running job. No later feature implementation is part of the architecture job itself.

## DeepSeek fixture/test jobs

See [deepseek.tsv](deepseek.tsv). Each row is a small task described in [deepseek.md](deepseek.md). These are test/fixture-only lanes on existing public APIs or synthetic inputs, not delegated implementation of the complex feature. Queue each row only after its owning package stabilizes the relevant interface. Do not give two rows ownership of the same file.

## Command registration ownership

The following slots are compiled by the foundations. Existing V1/legacy command IDs must be reused, not registered twice. Query/preview specs use `journal=false`; edit specs use `journal=true`. Trace algorithm packages supply pure evaluators and result types; 28 remains the only owner of public T1 command extensions, so their registration slots can stay empty.

| Slot | Package | Command family / integration contract |
|---|---|---|
| `vector_packages::drawing` | 02 | `shape.design.*` |
| `vector_packages::node` | 03 | `path.node.* (existing path edit IDs are reused)` |
| `vector_packages::pen` | 04 | `path.penCommit (reuse shape.create/path.set where sufficient)` |
| `vector_packages::pencil` | 05 | `path.pencil.*` |
| `vector_packages::corners` | 06 | `path.corner.*` |
| `vector_packages::snapping` | 07 | `vector.snap.* (queries journal=false)` |
| `vector_packages::appearance` | 08 | `shape.appearance.*` |
| `vector_packages::constructive` | 10 | `path.builder.* / path.cut.* (V1 path.offset/pathfinder IDs are reused)` |
| `vector_packages::width` | 11 | `shape.width.*` |
| `vector_packages::path_text` | 12 | `type.path.*` |
| `vector_packages::symbols` | 15 | `symbol.*` |
| `vector_packages::constraints` | 16 | `vector.constraints.*` |
| `vector_packages::layout` | 16 | `vector.layout.* (extend existing align/artboard commands)` |
| `vector_packages::brushes` | 17 | `vector.brush.*` |
| `vector_packages::blend` | 18 | `vector.blend.*` |
| `vector_packages::repeat` | 19 | `vector.repeat.*` |
| `vector_packages::envelope` | 20 | `vector.envelope.*` |
| `vector_packages::svg_import` | 13 | `file.svg.import / file.svg.place` |
| `vector_packages::svg_export` | 14 | `file.svg.export` |
| `vector_packages::pdf_export` | 21 | `file.pdf.export` |
| `vector_packages::pdf_import` | 22 | `file.pdf.import / file.pdf.place` |
| `vector_packages::affinity` | 24 | `file.affinity.import / file.affinity.place` |
| `vector_packages::trace_centerline` | 25 | `layer.vectorize.* existing T1 namespace (coordinate command IDs with 28)` |
| `vector_packages::trace_cleanup` | 26 | `layer.vectorize.* existing T1 namespace (coordinate command IDs with 28)` |
| `vector_packages::trace_cutout` | 27 | `layer.vectorize.* existing T1 namespace (coordinate command IDs with 28)` |
| `vector_packages::trace_output` | 28 | `layer.vectorize.* existing T1 namespace` |
