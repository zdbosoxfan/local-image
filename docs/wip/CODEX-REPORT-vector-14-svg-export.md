# Package 14: SVG export

Status: owned implementation and focused format/render/dialog/RTX gates pass.
All changed-crate test targets were run once; one pre-existing catalog test fails.
Integrated
acceptance is not yet complete: package 00 owns real manifests, module/menu/modal
routing, preferences/catalogs, aggregate GPU registration and attribution generation;
package 13 is not merged in this branch. Nothing is committed.

## Implemented

- Stable handwritten SVG writer uses exchange::ExportScope/ExportedVector/VectorWarning.
  Document, selection (including objects outside the document canvas) and artboard
  scopes validate all captured IDs, preserve ancestor compositing envelopes, and retain
  document-space viewBox origins. Physical root dimensions use explicit document DPI.
- XML escaping, deterministic IDs independent of native IDs, sorted resource/warning
  maps, configurable bounded decimal precision/minify, no external resource URLs.
  Named single-paint paths and data-name metadata prevent synthetic group growth.
- Shared pc-pathops normalization handles independent PSD boolean components,
  even-odd fills, inversion and aligned stroke outlines. Cubic closing segments precede
  Z; unused open endpoint handles are ignored. Width-relative dashes/offsets become
  absolute SVG lengths; zero-width strokes emit no path.
- Appearance items emit in paint order. Gradient and clip defs are deterministic and
  deduplicated. Native pc-vector::Paint supplies effective geometry, reverse, midpoint,
  opacity and offset behavior. Linear/reflected/radial gradients remain editable;
  angle/diamond/dither paints use PNG with an aggregated warning. Clip intersections
  emit once on the owning group, including duplicate importer clip/mask metadata.
- Editable text uses pc-text clusters/styles; curves use its public positioned outlines
  and decorations. No second font shaper. Faux-bold, active warp and disabled text AA
  use the native raster fallback. Named editable fonts produce a portability warning.
- Raster fallback refreshes detached ordinary caches in a pixel-budgeted region,
  preserves original gradient frames, composes through pc-compose and embeds PNG.
  Raster/smart layers, effects, soft masks, Lab compositing, adjustments and
  backdrop-dependent blends receive aggregated editability warnings. Missing enhanced
  corners/path-text/live/pattern/brush/width providers fail explicitly. A detached
  provider fixture exercises the actual extension boundary.
- Engine file.svg.export checks document/revision/the entire target set before and
  after serialization. It is read-only, non-journaled and cancellable, and uses the
  existing process font engine. No artificial undo/edit step is added.
- UI owns a captured dialog State, selection/artboard/curves/minify/precision controls,
  Export/Cancel actions, and the existing picker/write service adapter. It prepares
  bytes before picking a path and guards freshness again before writing. Cancellation
  and stale requests perform no writes; defaults load old preferences.
- Exact coordinator fragments, signatures, i18n rows and per-adapted-file PORTS rows
  are supplied in vector-tasks/integration/14.md and vector-tasks/i18n/14.tsv.

## Verification

Focused checks: 18 export tests passed (including 256 proptest cases), 4 engine
behavior tests plus 1 strict RTX roundtrip passed, 4 actual-interaction dialog tests
passed, and 1 strict full-app RTX preview/release/roundtrip passed; zero ignored.
resvg coverage IoU minimum was 0.99613996 for thin text; the largest measured mean
DeltaE_ok was 0.145436. Public-import roundtrip IoU was 1.0 for both tested paints.
Second/third export bytes match for solid, linear/reflected/radial and reversed
midpoint/independent-alpha gradients, each with and without a hard clip.

Final full-suite results (each test target run once):

| Changed crate | Passed | Failed | Ignored |
| --- | ---: | ---: | ---: |
| photocraft-svg | 18 | 0 | 0 |
| photocraft-engine | 1555 | 0 | 19 |
| photocraft-ui-egui | 1106 | 1 | 14 |

Total: **2679 passed, 1 failed, 33 ignored**. Doc tests pass with zero tests in each
crate. Existing ignored tests are not credited as acceptance; all new SVG tests
execute with zero ignored. The one failure is
`i18n::tests::every_tl_literal_is_translated`: 27 existing vector strings lack Japanese
catalog entries, including “Vector persona” and “Place SVG…”. `git grep HEAD`
confirms these callers existed before this package; the owned SVG dialog adds no
`tl!` call. Shared catalogs are owned by 00 and were not modified. The UI library
run reports 1013/1/10; the remaining targets report GPU 60/0/4, interaction 14/0/0,
pen 8/0/0, snapping 10/0/0, and SVG app GPU 1/0/0. Cargo stopped after the catalog
failure, so only the unexecuted UI targets and doc tests were run afterward;
no full suite or existing library target was repeated. Missing external corpus
and release performance gates are listed separately below.
`cargo +1.98.1 fmt -p photocraft-svg -p photocraft-engine -p photocraft-ui-egui -- --check`
passes through temporary manifests. All-target clippy with `--no-deps -- -D warnings`
passes on the three changed crates, including owned GPU/test targets. The combined
source-layout check found an unmerged package-13 `clippy::question_mark` warning in
`import/text.rs:31`; no importer source or lint allowance was changed. The final
isolated lint probe compiles that actual public importer as a separate dependency.
13/00 must resolve the warning before merged shared-crate lint acceptance.
Logs are under target/svg-export-focused-format.log,
svg-export-focused-ui-engine.log, svg-export-focused-app-gpu.log,
svg-export-clippy-owned.log, svg-export-full-tests.log,
svg-export-full-ui-remaining.log and svg-export-doc-tests.log. The combined
importer lint failure is preserved in svg-export-clippy.log.
All builds use Rust 1.98.1, --offline, CARGO_BUILD_JOBS=3 and the machine-wide rustc
resource gate. There is only one cargo build in this job at a time. The real shared
manifests/lock/registries were not changed. fixtures/export/bootstrap.py reproduces
coordinator wiring only under target/svg-export-workspace. The temporary lock has
zero new registry name/version pairs relative to the real pinned lock.

Geometry IoU follows the quality plan's 8x supersampled coverage rule: intersection
integrates min(a,b), union integrates max(a,b), using opaque geometry in both renderers.
Colour comparison retains actual paint opacities at output resolution and measures
mean DeltaE_ok over white. The initial single-pixel binary-alpha threshold produced
unstable thin-text and 50%-opacity edge classifications; those diagnostic failures
are not counted as passes. The final independent oracle retains IoU>0.995 and
mean DeltaE_ok<0.5 thresholds.

Read-only importer probes use package 13's real public import(bytes, &ImportOptions)
API, not a substitute. Sibling HEAD was c4e4dd083165599550abb86c9155239821c86c22,
with uncommitted importer WIP; this is dependency evidence, not merged acceptance.
The final read-only source snapshot SHA-256 is
`e6eb527196c1ff2e0cf731ca357b6ec5d6ea24e4861f9df110154a7c1fd2e437`.
Hash input is import.rs followed by sorted import/*.rs, each relative UTF-8 path,
NUL, exact file bytes, NUL. Sources are copied unchanged to target and re-exported
through the temporary importer dependency, preserving its actual public API.
The optional importer wiring includes its already locked simplecss 0.2.2 dependency;
that is not an exporter runtime dependency. This freezes a consistent probe while
its sibling builder continues working.

## Reproduction

Run from this worktree, with `PATH="$HOME/.cargo/bin:$PATH"` and
`CARGO_BUILD_JOBS=3`. The optional snapshot is dependency evidence only:

```bash
SVG_EXPORT_IMPORT_SOURCE="$PWD/../vec-13-svg-import/crates/pc-svg/src/import.rs" python3 crates/pc-svg/fixtures/export/bootstrap.py
cargo +1.98.1 fmt --manifest-path target/svg-export-workspace/Cargo.toml -p photocraft-svg -p photocraft-engine -p photocraft-ui-egui -- --check
CARGO_TARGET_DIR="$PWD/target/clippy" cargo +1.98.1 clippy --offline --manifest-path target/svg-export-workspace/Cargo.toml -p photocraft-svg -p photocraft-engine -p photocraft-ui-egui --features photocraft-svg/svg-import-integrated,photocraft-engine/svg-import-integrated,photocraft-ui-egui/svg-import-integrated --all-targets --no-deps -- -D warnings
CARGO_TARGET_DIR="$PWD/target" cargo +1.98.1 test --offline --manifest-path target/svg-export-workspace/Cargo.toml -p photocraft-svg -p photocraft-engine -p photocraft-ui-egui --features photocraft-svg/svg-import-integrated,photocraft-engine/svg-import-integrated,photocraft-ui-egui/svg-import-integrated -- --test-threads=1
```

Because the existing catalog failure stopped Cargo before the remaining UI targets,
the following ran those unexecuted targets and doc tests once:

```bash
CARGO_TARGET_DIR="$PWD/target" cargo +1.98.1 test --offline --manifest-path target/svg-export-workspace/Cargo.toml -p photocraft-ui-egui --features photocraft-engine/svg-import-integrated,photocraft-ui-egui/svg-import-integrated --test gpu --test it --test pen --test snapping --test svg_export_gpu --no-fail-fast -- --test-threads=1
CARGO_TARGET_DIR="$PWD/target" cargo +1.98.1 test --offline --manifest-path target/svg-export-workspace/Cargo.toml -p photocraft-svg -p photocraft-engine -p photocraft-ui-egui --features photocraft-svg/svg-import-integrated,photocraft-engine/svg-import-integrated,photocraft-ui-egui/svg-import-integrated --doc -- --test-threads=1
```

The final ownership audit found only 18 package-owned changed/new files plus the
pre-existing untracked CODEX-TASK.md. Real Cargo.toml and Cargo.lock match HEAD,
`git diff --check` passes, and the temporary lock adds zero registry versions.
No build remains running in this job. Nothing is committed.

## GPU and remaining integration

Strict RTX 5090/Vulkan/discrete/Rgba32Float tests are supplied in the engine export
module and tests/export/app_gpu.rs. The app test runs actual PhotocraftApp offscreen,
real Move press/motion/held-preview frame pixels/release, guarded export, public import,
and native canvas readback. Premultiplied maximum channel difference must be <=1/255.
Both strict gates executed successfully on NVIDIA GeForce RTX 5090, Vulkan,
DiscreteGpu, PCI 0000:01:00.0, NVIDIA driver 615.71.09, required Rgba32Float.
The app gate compares every document pixel in the actual held-pointer frame and
checks one release history entry; export introduces no additional history/revision.
There is no no-adapter skip and no desktop window is opened. No speed claim is made;
release pointer-frame performance acceptance is still coordinator-owned.

Do not mark this package integrated/done until 00 applies its handoff and runs the
actual File menu/modal route, integrated provider/roundtrip/GPU gates and attribution
regeneration on wip/vector. Enhanced provider packages 08/12/15/live remain named
dependencies. The pinned resvg graph cannot decode PNG without three additional
unapproved crate/version pairs; PNG decoding is independently tested through the
existing image crate, and that is not represented as a passed resvg PNG gate. 00 must
supply its approved raster reference-harness solution. Proprietary/external corpus,
upstream SVG goldens and performance fixtures were not present/run and do not count
as successful gates; generated GPL fixtures and their precise parameters are listed
in crates/pc-svg/fixtures/export/SOURCES.md.

## Sources and attribution

Read the task, vector architecture, persona including Owner decisions, vector quality
plan and vector-tasks README in full. Followed pinned VectorCraft svg/export.rs
(d522c1d7be4035bd4f4a84cd6ebfca44f5155092), native PhotoCraft gradient projection
(d438f842256435dba7aa41ca306656a216b98fbf), and the existing pc-text, pc-vector,
pc-pathops, pc-compose, engine Services and offscreen Move harness APIs. Existing
copyright/license notices are retained; 00 receives one PORTS row per adapted file.

Official SVG 1.1 references consulted: [paths/closepath](https://www.w3.org/TR/SVG11/paths.html#PathDataClosePathCommand),
[painting/dashes](https://www.w3.org/TR/SVG11/painting.html#StrokeDasharrayProperty),
[gradients](https://www.w3.org/TR/SVG11/pservers.html),
[text/textPath](https://www.w3.org/TR/SVG11/text.html#TextPathElement).
The document adapter, bounds/transaction guards, projection bridge and tests are
original Local Image code. No brand assets, new fonts, new registry packages,
production C/C++ or unsafe code were introduced.
