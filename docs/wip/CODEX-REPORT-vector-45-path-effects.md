# Package 45: live path effects

2026-10-10, branch `wip/vec-45-path-effects`, base `529619af`. Changes are uncommitted.
The initial worktree had the task brief but no package-45 implementation. Existing vector
builders and shared provider work in this branch were retained and reused.

Package-owned implementation and integration fragments are supplied. **Integrated acceptance
is pending**: the task reserves native data-field/command aggregation, application routing,
shared projection, and Expand integration to 00/08/09/36. This branch does not claim that the
new effects are already enabled in the production application. Exact patches and call
signatures are in [integration/45.md](vector-tasks/integration/45.md).

## Implemented scope

- Typed editable data in `pc-doc/src/vector_design/effects.rs`: layer or Appearance-item
  target, enabled state, all nine effects and parameters, serde defaults, presets and item
  index remapping. Additive `VectorData.effects` field and skip-if-empty fragment supplied
  to 00; native format version remains 2. No production surrogate persistence.
- Pure ordinary-path projection in `pc-vecfx/src/path_effects/`: Roughen (size, relative,
  detail, smooth/corner, seed), Zig Zag, Pucker & Bloat, Twist, Transform (scale, movement,
  rotation, reflection, copies), Round Corners, Offset Path, Free Distort, Convert to Shape.
  Ordered layer/item targeting, visibility and separate paint operations for translucent
  Transform copies preserve source geometry. Source paths, handles, fill rules, ordered
  component operations and explicit closedness survive mapping, apart from the intentional
  replacement outline in Convert to Shape.
- Round Corners reuses package 06's evaluator; Offset calls V1 `offset_path`; primitives
  and rasterization use the existing pc-vector implementation. Renderer adapter uses
  existing stroke/profile providers and pc-color blending. Dissolve stays in appearance
  coverage for both merged and detached rendering.
- Engine provider in `pc-engine/src/vector_packages/path_effects.rs`: captured document ID,
  revision and target IDs; complete-set validation; detached Preview; atomic preparation;
  one `Session::edit` commit; Undo/Redo; registered command specifications with edit
  journal=true and preview/info journal=false. Production dispatch uses the registered
  engine command. Access installation is explicit and fails when native integration is absent.
- Geometry cache excludes document revision and keeps stable transient child IDs on hits.
  Unrelated edits preserve geometry and package 09 child identities. Source/effect changes
  invalidate the cache. `refresh`, `project_document`, `projected_layer` and `scaled_paint`
  provide source-edit, compositor, export, Expand and view integration interfaces.
- Bounds and work guards: 64 targets/effects, 4096 paint paths, 65536 aggregate knots,
  finite bounded numeric inputs/coordinates, bounded nonlinear fitting, and a 512 MiB
  whole-request preflight for merged plus expanded child caches before pixel allocation.
- UI fragment in `pc-ui-egui/src/vector_packages/path_effects.rs`: grouped Effect menu,
  all parameter controls, item targeting, Preview/OK/Cancel/Escape, editable Appearance
  section with toggle/edit/remove, actual drag reorder and move buttons. Preview is transient
  and cancelled by stale document state or changes of tool/persona/module/modal owner.
  App hook calls and modal routing are supplied to the integration lane; 77 English
  translation entries are supplied in [i18n/45.tsv](vector-tasks/i18n/45.tsv).
- Expand consumes ordinary projected children and clears effects under the existing
  transaction. Exporters receive an exchange warning when editable effects are expanded.
  These routing fragments are supplied to 36 and 14/21/23 rather than editing their files.
- Only append-only module registration edits touch shared files. No dependency, manifest,
  lockfile, shared UI router, native data layout or shared catalog changes. No unsafe code,
  brand assets, desktop window, owner-file writes, application termination or commits.

## Validation

All cargo builds use `cargo +1.98.1`, `--offline`, `CARGO_BUILD_JOBS=3`, and the existing
`RUSTC_WRAPPER=/home/zdavidson/.local/share/local-image-dev/bin/li-rustc-gate`. One build
at a time; strict clippy uses `CARGO_TARGET_DIR=target/clippy`.

Final workspace `check --all-targets` passed (exit 0). Final strict clippy on all four
touched crates with `--all-targets -- -D warnings` passed (exit 0), including the corrected
UI harnesses and all deferred test sources. Format completed; `git diff --check` passed;
Cargo.lock and dependencies are unchanged.
An initial workspace check found two local compile errors (an untyped node counter and a
redundant import); both were corrected. The final verification covers the detached-Dissolve
correction and the submenu query correction.

The first filtered run passed the new document test and five engine tests. The three UI
tests failed before effect interaction because egui's submenu access labels include `⏵`;
the exact label query did not match. The CPU and deferred GPU harnesses now use the existing
repository's `get_by_label_contains` query for submenu groups. No UI retry is run under the
owner's explicit once-only test limit; corrected harness interaction still needs verification
at integration. Cargo stopped before the geometry test binary, so its first run is separate.

Final filtered execution counts: **16 passed, 3 failed before the UI query correction,
3 ignored integration gates**. Every new test executed at most once. The second cargo test
command ran only the ten previously unexecuted geometry tests.

| Crate | Passed | Failed | Ignored | Other tests filtered out |
|---|---:|---:|---:|---:|
| photocraft-doc | 1 | 0 | 0 | 53 |
| photocraft-vecfx | 10 | 0 | 0 | 11 |
| photocraft-engine | 5 | 0 | 2 | 1216 |
| photocraft-ui-egui | 0 | 3 | 1 | 1149 |

Commands/logs (inside this worktree):

```sh
PATH="$HOME/.cargo/bin:$PATH" CARGO_BUILD_JOBS=3 cargo +1.98.1 test --offline \
  -p photocraft-doc -p photocraft-vecfx -p photocraft-engine -p photocraft-ui-egui \
  --lib path_effects45
# target/path-effects45-tests.log: exit 101, UI query failures above
PATH="$HOME/.cargo/bin:$PATH" CARGO_BUILD_JOBS=3 cargo +1.98.1 test --offline \
  -p photocraft-vecfx --lib path_effects45
# target/path-effects45-geometry-tests.log: exit 0, ten passed
PATH="$HOME/.cargo/bin:$PATH" CARGO_BUILD_JOBS=3 cargo +1.98.1 check --offline --workspace --all-targets
# target/path-effects45-check.log: exit 0
PATH="$HOME/.cargo/bin:$PATH" CARGO_BUILD_JOBS=3 CARGO_TARGET_DIR=target/clippy \
  cargo +1.98.1 clippy --offline -p photocraft-doc -p photocraft-vecfx \
  -p photocraft-engine -p photocraft-ui-egui --all-targets -- -D warnings
# target/path-effects45-clippy.log: exit 0
```

New test coverage only:

| Crate | New active tests | Behavior/oracles |
|---|---:|---|
| photocraft-doc | 1 | Parameter roundtrip, missing target/enabled defaults, paint remapping |
| photocraft-vecfx | 10 | Exact identities; independent square area; V1 offset; primitive bounds/area; seeded Roughen/DPI; analytic line normals and Pucker; dense independent nonlinear samples/V1 nearest; order/item/copy paint semantics; budgets; tiny/large geometry; independent affine matrix |
| photocraft-engine | 5 | Source retention, detached Preview, atomic release, Undo/Redo, source re-projection; entire-set rejection/staleness/locks; cache hits/stable child IDs; 3-copy independent RGBA8 pixel oracle plus floating guard; command journal flags, bounds, Dissolve parity, persistence failure |
| photocraft-ui-egui | 3 | Real menu clicks/numeric typing/Preview/edit/hide; actual slider press/drag/release/reorder/delete/stale cancel; actual drag-and-drop row reorder |

Synthetic engine/UI fixtures explicitly inject test storage and dispatch. They do not
demonstrate native persistence or production journal routing. No old tests, full crate/workspace
test suites, GPU acceptance runs, release builds or benchmarks were run: the final owner
SPEED RULES override those earlier gates. No performance claim is made.

Three ignored integration gates are supplied and compile, but have **not passed**:

1. `path_effects45_path_effects_on_shapes_rtx5090_preview_release_parity`: all nine effects,
   real GPU compositor, Preview/release, 1/2/4/8x through injected shared package 09 provider.
2. `path_effects45_full_app_rtx5090_preview_pointer_release_cancel_and_expand`: actual
   offscreen PhotocraftApp, menu, live pointer Preview texture readback, Cancel/OK,
   one journal/history entry, Expand Appearance and Undo.
3. `path_effects45_native_old_document_editable_roundtrip_and_one_journal`: repository
   v1 pcraft fixture, native nine-effect roundtrip, projected pixels, registered command journal.

GPU gates assert RTX 5090, Vulkan, discrete device and Rgba32Float, finite premultiplied
pixel max difference <=1/255; a software adapter or missing provider cannot pass. They open
no desktop window. Native/GPU integration gates require actual coordinator wiring.

## Remaining integration and limits

00 must add the additive native field, install Access and aggregate command specifications;
00/08 must wire menu/panel/frame/display and source refresh; 09 must consume projected child
paint and install its real shared scaled provider in the GPU gate; 36 must bake effects;
14/21/23 must consume the shared geometry and warning. These are named dependencies under
the task's no-competing-edits rule, with complete handoff fragments. 00 also merges i18n and
the four attribution rows and regenerates shared attribution artifacts.

Clip/live/on-path combinations unsupported by the current 36 base provider fail explicitly;
35/20 must supply their shared projection before those combinations are enabled. Nonlinear
fitting uses numerical subdivision/sample guards, not a formal interval error proof. No
Illustrator proprietary pixel corpus or external golden artwork was supplied or claimed as a
passing gate. All integration, GPU, native and performance evidence remains separate from the
isolated module checks above. The package cannot be marked integrated-complete until those
required hooks and integration acceptance pass.

The existing lightcraft-engine build script reports missing `craft-fonts` (Chinese/Japanese
embedded fonts). No external fonts were installed; these generated shape fixtures do not
exercise text. This missing dependency is separate from the new path-effect test results.

## Sources and attribution

Read the required architecture, persona (including Owner decisions), quality plan, package
README and gap audit, and existing providers before implementation. Read-only local
VectorCraft checkout HEAD was verified at `d522c1d7be4035bd4f4a84cd6ebfca44f5155092`.
Followed [distort.rs](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/effects/src/distort.rs),
[stylize.rs](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/effects/src/stylize.rs),
[warp.rs](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/effects/src/warp.rs),
[live.rs](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/effects/src/live.rs),
util/doc live map helpers and
[effectcmd.rs](https://github.com/storytold/vectorcraft/blob/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/engine/src/cmd/effectcmd.rs)
and its effect-edit tests. Typed validation, geometry budgets, bounded mapping, rendering
adapter, cache integration and UI are Local Image adaptations/original work where no existing
provider met the task. Reused the actual 06/V1/08 stroke/shape providers instead of replacing them.

Consulted the official [Adobe Appearance attributes](https://helpx.adobe.com/illustrator/using/appearance-attributes.html)
documentation for effect/paint targeting, editable order and visibility. Its linked reshape
effects endpoint returned an error; the pinned upstream was used for formulas.
MIT/Apache copyright and licence notices are retained in adapted sources and existing
`licenses/vectorcraft-*`; one PORTS row per adapted file is supplied to 00. Generated fixtures
are original GPL-3.0-or-later Rust tests. Tool versions, parameters, formula/oracle sources and
missing corpus evidence are recorded in the owned vecfx/engine/UI `SOURCES.md` files.
