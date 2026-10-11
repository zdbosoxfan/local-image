# Package 41 — vector liquify tools and Puppet Warp

Owned implementation is complete and left uncommitted. Runtime integration and real-GPU
acceptance remain explicitly pending with 00/01/46; this report does **not** claim integrated
acceptance or measured 60 Hz performance. The task's final SPEED RULES prohibit those runs.

## Implementation

- Continued the existing branch's native Path/V1 adapters, vector transactions, corner and
  appearance projection, type conversion, stylus bridge and offscreen app harness. No
  replacement of those foundations or edits to raster Liquify/Puppet Warp were made.
- Added seven vector brush kernels: Warp, Twirl, Pucker, Bloat, Scallop, Crystallize and
  Wrinkle. Elliptical width/height/angle, intensity multiplied by optional tablet pressure,
  detail, simplify, twirl rate, complexity/affected controls and wrinkle axes have checked,
  finite ranges and serde defaults. Wrinkle/noisy tools are seeded and deterministic.
- Adaptive curve subdivision queries actual nearest points in normalized brush coordinates,
  so a narrow brush can bend a long straight segment. Global path node and stroke work
  budgets bound subdivision; excessive work errors before publication. Changed contours
  use V1 simplification/refit and retain fill rule, operations, closure and untouched paths.
- Added native vector Puppet Warp with grid triangles restricted by normalized artwork,
  barycentric pins, automatic limb pins, local rotation constraints, two-stage banded ARAP,
  bounded local/global rigid-fit refinement and segment-wise V1 refit. Identity/common pin
  translation preserve native coordinates and handles exactly. Add/delete/select/rotate pins
  and group pin translation are implemented. Each drag publishes one destructive edit.
- Added six supplied engine CommandSpecs. Every gesture captures document/revision/target
  IDs. Preview documents/caches are detached and never journaled. Release repeats checked
  evaluation, preflights the whole target set and aggregate bounds-limited appearance cache
  allocation, then publishes one Session::edit. Locks, stale input, absent/duplicate targets,
  unsupported live groups and invalid numeric inputs error atomically. Evaluated corners
  are baked and corner indices/stale PSD geometry cleared only for changed targets.
- Brush targeting uses selected shape paths/group children, or visible unlocked paths when
  no layer is selected, including paths met later in a stroke. Live shapes and selected text
  require a real confirm/cancel conversion prompt; conversion is a separate undoable edit
  using existing shared projection/type commands. Unselected live shapes away from the
  brush are preserved.
- Added real egui canvas input, all eight tool controls, Alt resize with Shift proportions,
  pressure, held dabs, pin hit testing/selection/rotation, Delete, Escape/focus/tool/document
  cancellation and detached filled-preview AppBridge hooks. Numeric settings use real
  widgets. Pins are transient: committed geometry/current pins become the next rest pose;
  undo/revision change discards the rig. No unowned native-model or preference fields added.

## Validation

All cargo commands used Rust 1.98.1, --offline, CARGO_BUILD_JOBS=3, the existing
`/home/zdavidson/.local/share/local-image-dev/bin/li-rustc-gate` wrapper and one build at a time.
The sole workspace check preceded the test-driven local fixes; final all-target clippy
checks the final sources in all three touched crates without repeating a workspace build.

```sh
cargo +1.98.1 fmt -p photocraft-vecfx -p photocraft-engine -p photocraft-ui-egui
cargo +1.98.1 check --offline --workspace --all-targets
CARGO_TARGET_DIR=target/clippy cargo +1.98.1 clippy --offline -p photocraft-vecfx -p photocraft-engine -p photocraft-ui-egui --all-targets -- -D warnings
cargo +1.98.1 test --offline -p photocraft-vecfx --lib distort41
cargo +1.98.1 test --offline -p photocraft-vecfx --lib distort41_puppet_rotation_with_fixed_pins_has_under_two_percent_edge_stretch
cargo +1.98.1 test --offline -p photocraft-engine --lib distort41
cargo +1.98.1 test --offline -p photocraft-ui-egui --lib distort41
cargo +1.98.1 test --offline -p photocraft-ui-egui --lib distort41_kittest_alt_rotate_selected_pin_and_all_pins_rigid_drag
```

| Check | Result |
| --- | --- |
| fmt for pc-vecfx, pc-engine, pc-ui-egui | Passed |
| check --workspace --all-targets (one run) | Passed; target/distort41-check.log |
| strict clippy, all targets, three touched crates | Passed on final sources; target/distort41-clippy-final.log |
| New pc-vecfx distort41 tests | 10 passed initially; one ARAP oracle failed at 2.537% stretch, fixed by refinement and its narrow retry passed (11 unique tests passed). 7 existing tests filtered; target/distort41-vecfx-tests.log and distort41-arap-retest.log |
| New pc-engine distort41 tests | 9 passed, 0 failed, 0 ignored; 1162 existing tests filtered; target/distort41-engine-tests.log |
| New pc-ui-egui distort41 tests | 8 passed initially; one group-drag precision failure fixed by computing a shared pointer delta, and its narrow retry passed (9 unique tests passed). 2 GPU tests ignored, 1122 existing tests filtered; target/distort41-ui-tests.log and distort41-ui-rigid-retest.log |
| Full suites / GPU / release / benchmark | Not run, per overriding SPEED RULES |
| Proprietary/external Illustrator corpus | Missing; zero corpus gates claimed passed |

The geometry tests use independent dense cubic area and edge-length/pin oracles, exact
native rigid translation, 256 generated multi-contour cases, finite/path/node-budget checks,
seed determinism, pressure/zero identity, incremental/replay parity and boundary rejection.
Engine tests cover detached preview, release parity, atomic/stale/locked targeting, one undo,
no-op metadata, live conversion and appearance/corners. UI tests actually click, drag, type
and press keys (including focused/unfocused Shift+R), dispatching the supplied package specs
in a small host; they establish
Session::edit history behavior, not runtime command journaling or full-app integration.
There are 29 unique new passing CPU/interaction tests and two ignored GPU gates. Only the
two failing new tests were retried after their fixes; successful tests and full suites were
not repeatedly executed.

Two **compiled, ignored full-PhotocraftApp GPU gates** were supplied in
`crates/pc-ui-egui/src/vector_packages/distort_tools/app_gpu.rs`: all eight tools' live-filled
preview/release pixels (premultiplied max diff <=1/255), one journal/history edit, undo/cancel;
and release pointer+app+GPU/readback p95 <=16.667 ms on a 2000-node path for each tool.
They assert RTX 5090/Vulkan/discrete hardware, Rgba32Float compositor and float canvas support,
and fail when runtime hooks are absent. Their existence is not a passed hardware gate.

## Handoff and remaining gates

Exact fragments and signatures are in [integration/41.md](vector-tasks/integration/41.md):
00 aggregates the command specs and canvas/display/cache/context hooks; 01/46 install tool
IDs, Width/Free Transform flyouts, Shift+R, icons and preference persistence. 00 merges
[i18n/41.tsv](vector-tasks/i18n/41.tsv) and
[41-PORTS.tsv](vector-tasks/integration/41-PORTS.tsv), regenerates attribution and runs the
integrated offscreen GPU/pixel/release-performance acceptance. These shared files are outside
this package's ownership. Only append-only module declarations were added, per REGISTRATION.

Persistent original rest poses/pins across saves or undo would require 00's native-model
storage. This package instead provides transient pins and destructive per-drag path edits.
Inverted artwork must be expanded before Puppet Warp; existing type-converter fidelity and
live-group expansion remain with their owners. No unsupported corpus is counted as success.

Sources followed: pinned VectorCraft commit
`d522c1d7be4035bd4f4a84cd6ebfca44f5155092`,
[Igarashi/Moscovich/Hughes ARAP paper](https://www-ui.is.s.u-tokyo.ac.jp/~takeo/papers/rigid.pdf),
[Adobe tool reference](https://helpx.adobe.com/illustrator/using/tools-in-illustrator.html) and
[Adobe transform/Puppet Warp reference](https://helpx.adobe.com/illustrator/desktop/manage-objects/reshape-transform-objects/transform-objects.html).
Pinned hashes, license/fixture parameters, local adaptations and test tool versions are in
[41-SOURCES.md](vector-tasks/integration/41-SOURCES.md). Existing MIT/Apache notices retained;
no brand assets, dependencies, manifest/lock changes, unsafe code, commits or desktop windows.
