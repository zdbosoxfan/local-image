# Vector 16 — layout and constraints

Implementation continues the existing registered foundations; no prior builder was replaced.
Changes are uncommitted and confined to this package's owned paths. No dependencies,
manifest/lock, shared registry/router, native types, language catalogs or renderer core
files were changed in the working tree.

## Implemented

- Constraint resize arithmetic with independent margin/center oracles, a pin-right
  fixed-size table, 256 deterministic property cases, both-pins/fixed-size precedence,
  nested parent behavior and rotated child affine resizing. Both pins plus fixed size
  preserve the size and split the parent size delta equally between the two margins.
  Rotated/sheared/flipped parent bases are rejected explicitly.
- Captured document/revision/target validation, inherited locks and parent/child overlap
  checks. Detached previews do not publish or journal. Commits validate/prepare all
  targets and execute one engine edit. Finite coordinates, knot/target/descendant,
  individual and aggregate cache budgets prevent unbounded transforms before allocation.
- Key-object edge/center alignment, automatic spacing preserving the outer span and
  precise fractional gaps with a stationary interior key. Existing auto-align/blend
  commands retain their implementations.
- Transform panel with nine anchors and X/Y/W/H plus relative rotation/shear fields in
  document design units; typing/drags preview, Enter/Apply commits, Escape/cancel discards.
  A custom point-transform origin is supported. Numeric artboard dimensions use child
  constraints rather than scaling fixed-size children as an ordinary affine transform.
- Vector Artboard tool creates, moves and resizes boards, including Shift/Alt modifiers
  and pinned children. Existing layer.artboard.set adds opt-in/default constraint handling
  while legacy unconstrained children retain their size/moveContents behavior.
  Fractional resize requests first resolve to the stored integer artboard frame, keeping
  pinned margins stable over repeated edits. Negative pasteboard positions remain valid.
- Point Transform chooses a custom origin on a non-node click and transforms about that
  origin when dragging a real node, including nodes inside a selected group; node dots
  are drawn at actual knot positions. Shift quantizes rotation and Alt preserves scale.
- Vector Crop stores a non-destructive clip, supports aspect/center handle gestures,
  content movement within a fixed crop, clear, cancel and undo. Detached display
  projection uses the existing CPU/GPU group vector-mask compositor and package 09
  scaled-view masks, retaining original content and original masks.
- Exact 72 pt/in and 25.4 mm/in conversions with document DPI. Print presets use mm,
  Web/Mobile/Film & Video/Art & Illustration use pt, Photo/old document metadata uses px.
  The existing Photo dialog's physical size field behavior remains unchanged.
- Original synthetic fixtures, old native fixture load/clip roundtrip, standalone real
  egui interaction tests and full-app strict RTX pointer/render tests. No desktop windows.

## Integration and limits

[Exact fragments](vector-tasks/integration/16.md), [English i18n rows](vector-tasks/i18n/16.tsv),
and [source/fixture record](vector-tasks/integration/16-SOURCES.md) are supplied for 00.
The production app hooks and app acceptance source are owned files. Shared registrations
are applied only in the disposable copy target/layout16-registration/worktree; it uses
this package's actual production source and normal PhotocraftApp canvas/GPU path.

00 must serially land those shared fragments and locale rows. 01 allocates dedicated
Tool variants and any explicit per-view ruler override preferences; until then the tested panel uses the existing PathSelection slot only
while an owned layout mode is selected. Move and pixel Crop retain their existing routes.
File adapters must consume clip geometry through vector_design::exchange rather than
adding UI/engine dependencies; display projection is ready for promotion into that shared
boundary by 00. Export-adapter rollout is not counted as verified here.

Live groups require their named geometry provider, anisotropic live corners require
conversion to curves, artboards are axis aligned, and R/S panel fields express changes
because baked paths have no persisted object transform decomposition. These are explicit
errors/documented limits, not successful skips.
Ordinary groups have no persisted container frame. A constrained resize that would need
empty padding is rejected atomically; use an artboard to retain that frame. Shared frame
type additions remain with 00. Constraint setters reject arrays, unknown keys and
non-boolean values without changing the document or journal.

## Verification

Commands use Rust 1.98.1, --offline, CARGO_BUILD_JOBS=3, one cargo build at a time.
Focused engine: final 26 passed, 0 failed/ignored, including strict RTX CPU/GPU parity,
cache/pasteboard limits, fractional artboard margins, frameless-group rejection and strict
constraint input regressions. The once-only full library run contained 23 owned tests;
the three final regressions were checked in the focused run rather than another full suite.
Standalone UI: final 8 passed, 0 failed/ignored, including the added numeric artboard
constraint regression. Full-app owned acceptance: final 4 passed, 0 failed, 1 release
benchmark ignored in debug.
Full app: preset-card clicks and old settings load; crop live/release/cancel/undo at
(zoom, PPP)=(1,1),(2,2),(4,1),(8,1); constrained board resize and point transform;
ordinary Move keeps the path and crop translation together.

GPU identity: NVIDIA GeForce RTX 5090, Vulkan, DiscreteGpu, driver NVIDIA 615.71.09,
PCI 0000:01:00.0; required accumulator Rgba32Float. Owned tests assert those properties
and cannot pass by adapter skip. Engine premultiplied max errors: 0 for crop and
2.9802322e-8 for constrained resize, below 1/255. Full-app screenshot live/release
samples allow at most one premultiplied byte and verify the crop reveals the background.

Final full-crate runs (each once, --no-fail-fast -- --test-threads=3):

| Crate/target | Passed | Failed | Ignored |
|---|---:|---:|---:|
| Engine lib | 982 | 0 | 13 |
| Engine coverage | 581 | 1 | 0 |
| Engine it | 47 | 0 | 5 |
| Engine vector_v1 | 12 | 0 | 1 |
| UI lib | 1024 | 1 | 12 |
| UI appearance | 11 | 0 | 0 |
| UI app | 60 | 0 | 4 |
| UI coverage | 14 | 0 | 0 |
| UI layout16 | 7 | 0 | 0 |
| UI layout16_app | 4 | 0 | 1 |
| UI path_text | 5 | 0 | 1 |
| UI pen | 8 | 0 | 0 |
| UI snapping | 9 | 1 | 0 |

Doc-tests: 0 cases in both crates. Engine initial total 1622 passed/1 failed/19 ignored.
The sole engine regression expected legacy BadParams for invalid artboard rectangles;
restored that variant and reran the entire affected ds_artboard_cmds_coverage module:
20 passed, 0 failed. No second full crate run was used.

UI total: 1142 passed/2 failed/18 ignored. The i18n scanner reports missing Japanese
strings (59 in its first reported locale) in existing vector modules; catalog ownership
is 00 and the known missing rows
are not suppressed. The other failure is snapping's HiDPI Move live/release gate;
the same 255/255 preview/release error reproduces with ALL package 16 app callbacks
disabled (window/context/display/synchronize/event/overlay). No foreign snapping or
renderer source was changed to mask it. This is a shared 07/09 integration issue.
The whole UI crate is therefore **not green**; package acceptance must not conceal those
failures or claim the shared integration lane has landed.

Ignored cases remain separate: engine timing benchmarks, two V1 node adapter gates,
slow registry fuzz/manual PSD/Camera Raw fixtures and a V1 interaction benchmark;
UI legacy performance/provider gates, plus this package's release-only measurement.
The layout16 release benchmark is run explicitly below and cannot pass in debug.

Final all-target clippy for both changed crates passed with -D warnings, including the
production registered hooks, ruler call, acceptance sources and the last
numeric/fractional constraint fixes. Formatting and whitespace checks also passed.

Release real-app measurement: final 1 passed, 0 failed/ignored. RTX 5090/Vulkan/discrete,
NVIDIA 615.71.09, GPU canvas, no fallback/recovery. CPU: AMD Ryzen 7 9800X3D, 8 cores/16
threads. Scene: 256×192 pixels, white background/red shape/clipped half-alpha blue
raster; 1500×1000 logical-pixel offscreen app, PPP=1, zoom=1, 30 crop pointer frames.
Each timed sample includes UI processing, GPU rendering and readback; release is timed
separately. There is no claim that every sample is warmed or that these results apply
to large documents or other tools.

| Release run | Median frame | p95 | p99 | Release |
|---|---:|---:|---:|---:|
| Final source | 3.804 ms | 5.411 ms | 9.296 ms | 4.728 ms |
| Earlier finalization, before last constraint fixes | 3.866 ms | 17.949 ms | 22.961 ms | 4.244 ms |

The median averages sorted samples 14/15, p95 uses sample 28 and p99 sample 29,
zero-based. The two runs illustrate variation on the owner's shared machine. The
earlier run changed no crop benchmark logic; the final repeat verifies the final
registered source. No all-frames-under-16-ms or general app latency claim is made.

Build caches reuse locked third-party artifacts only; every workspace Rust source in
the disposable copy was touched to force current local crates to rebuild, including
release. No borrowed local binary/metadata was treated as acceptance evidence.

Reproduction commands, run from the working-tree root after applying the exact shared
fragments to the disposable tree:

```sh
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_BUILD_JOBS=3
export CARGO_TARGET_DIR="$PWD/target"
cargo +1.98.1 test --offline --manifest-path target/layout16-registration/worktree/Cargo.toml -p photocraft-engine --no-fail-fast -- --test-threads=3
cargo +1.98.1 test --offline --manifest-path target/layout16-registration/worktree/Cargo.toml -p photocraft-ui-egui --no-fail-fast -- --test-threads=3
cargo +1.98.1 test --offline --manifest-path target/layout16-registration/worktree/Cargo.toml -p photocraft-engine --test coverage ds_artboard_cmds_coverage
cargo +1.98.1 test --offline --manifest-path target/layout16-registration/worktree/Cargo.toml -p photocraft-engine layout16 --lib -- --nocapture
cargo +1.98.1 test --offline --manifest-path target/layout16-registration/worktree/Cargo.toml -p photocraft-ui-egui --test layout16 --test layout16_app layout16_ -- --nocapture --test-threads=3
CARGO_TARGET_DIR="$PWD/target/clippy" cargo +1.98.1 clippy --offline --manifest-path target/layout16-registration/worktree/Cargo.toml -p photocraft-engine -p photocraft-ui-egui --all-targets -- -D warnings
cargo +1.98.1 test --offline --release --manifest-path target/layout16-registration/worktree/Cargo.toml -p photocraft-ui-egui --test layout16_app layout16_app_release_pointer_frames -- --ignored --nocapture --test-threads=1
```

Logs: [engine full](../../target/layout-16-engine-full.log),
[engine legacy regression](../../target/layout-16-engine-artboard-regression.log),
[final engine focus](../../target/layout-16-engine-final.log),
[UI full](../../target/layout-16-ui-full.log),
[snapping with all 16 hooks disabled](../../target/layout-16-ui-snapping-no-hooks.log),
[final UI/app focus](../../target/layout-16-ui-final.log),
[final clippy](../../target/layout-16-final-clippy.log),
[release measurement](../../target/layout-16-release-perf.log).
The isolation run used `--test snapping gpu::snapping_full_app_gpu_guide_preview_release_no_jump_and_override -- --nocapture --test-threads=1`;
its temporary callback removals were restored before final owned acceptance and lint.

## Sources and absent evidence

The Affinity official behavior pages and pinned local APIs are linked in the source record.
All new algorithms/fixtures are original GPL-3.0-or-later code; no new adapted files or
PORTS rows. Existing copyright/license notices remain intact. No proprietary Affinity
corpus, downloaded artwork, fonts or brand assets were used. Missing external corpus
and absent embedded CJK fonts are recorded separately and are not counted as successful
acceptance. Real app performance is measured on a specified synthetic scene and is not
claimed for large documents or all tools.
