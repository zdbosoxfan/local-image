# Vector 07 snapping report

Status: owned implementation and validation finished on 2026-10-10. Final integrated acceptance requires package 00/V1; **package not claimed complete**. The task reserves the required shared registries/manifests/router bridges for 00 and explicitly permits coding against an unmerged dependency's documented interface. Exact remaining fragments are supplied in `docs/wip/vector-tasks/integration/07.md`.

The starting branch contained the previous shared vector foundation and compiled empty `vector_packages::snapping` slot; its history/transactions, renderer, existing Move behavior and tests were preserved. No separate snapping WIP patch was present. All changes remain uncommitted.

Implemented in owned files:

- Shared document-space Cache and point/rectangle query; physical-screen tolerance derived from logical zoom × pixels-per-point; finite numeric bounds and object/node cache budgets.
- Stable distance/kind/coordinate/source/anchor ordering, guide tie precedence, atomic node/nearest-curve points; descendants of excluded/hidden ancestors, masks and editable saved/work-path exclusions.
- Nodes, bounds/centres, guides, arbitrary-origin grid, integer pixel grid, document/artboard targets. The current Move bounds bridge is retained pending V1 analytic bounds.
- Alignment guides, bounded cached adjacent-object gap pairs, equal spacing between or repeating fixed objects for a selected multi-object union, angle projection and magenta guide/measurement painter.
- `vector.snap.query` is registered, query-only and nonjournaled; optional document/revision checks reject stale automation calls. V1 native nearest/bounds callbacks have an explicit install boundary. Missing nearest provider returns an error; it is not substituted with a sampled solver.
- Serialized manager settings use defaults for old UI settings. Actual checkbox/numeric editor UI goes through existing source-string catalogue lookup. Captured pointer bridge freezes origin/transform, samples current modifiers, survives crossing the canvas, cancels stale document/revision/focus/Escape/tool routes and supplies identical detached preview/commit geometry. No Session::edit in previews.
- Exact 00 wiring/V1 adapter/prefs/i18n/Move extension fragments in `docs/wip/vector-tasks/integration/07.md`, English-source fragments in `i18n/07.tsv`. Shared registries, manifests/lock, catalogs, snap.rs, snap_ui and move files are untouched.

Test sources are compiled by the existing engine slot and the package-owned `pc-ui-egui/tests/snapping/main.rs`; no aggregate registration edited. Focused engine tests passed **13, 0 failed/ignored**, including the deterministic 256-case grid oracle. Nine actual kittest UI scenarios cover guide drags, capture/bypass/cancellation, multi-object spacing, manager clicks and typed/clamped tolerance. During development, two manager tests exposed egui's default popup close-on-control-click behavior; `CloseOnClickOutside` fixed it, and both focused regressions passed. The initial seven other UI tests passed. The final full run passed all **23 added snapping tests** together, including the tightened independent lattice oracle and the GPU-active assertion on every readback.

Full suites of only the two changed crates ran **once**, exit 0, with these counts:

| Crate / binary | Passed | Failed | Ignored |
|---|---:|---:|---:|
| pc-engine unit | 864 | 0 | 11 |
| pc-engine coverage | 582 | 0 | 0 |
| pc-engine integration | 47 | 0 | 5 |
| pc-ui-egui unit | 991 | 0 | 5 |
| pc-ui-egui GPU | 59 | 0 | 4 |
| pc-ui-egui integration | 14 | 0 | 0 |
| pc-ui-egui snapping | 10 | 0 | 0 |
| Both crates' doc-tests | 0 | 0 | 0 |
| **Total** | **2,567** | **0** | **25** |

Ignored tests remain separate from passing results: engine's 16 are timing/benchmark cases, the slow panic-hunt and child entry point, a manual Photoshop export and the user-supplied Camera Raw PSD fixture. UI's 9 are benchmark/timing cases, a generated icon review sheet and the explicitly ignored GPU icon upload test. The required snapping GPU test is active and passed; no skipped adapter counts as success. Default-feature suites were run; the optional external corpus and proprietary Camera Raw fixture were not supplied/run and do not count as gates.

Post-suite review found that artboard target spans needed the same bounded-rectangle validation already used for other targets. The final one-line guard and oversized-artboard regression were added to the existing invalid-input test. All **13 focused engine tests passed again, 0 failed/ignored**, recorded in `target/snapping-engine-final.log`. Final engine all-target clippy with `-D warnings` also passed, exit 0, recorded in `target/snapping-engine-final-clippy.log`. The full suites were not repeated.

Validation commands use Rust/cargo **1.98.1**, `PATH="$HOME/.cargo/bin:$PATH"`, `CARGO_BUILD_JOBS=3`, offline mode and one Cargo build at a time:

```text
cargo +1.98.1 check --offline -p photocraft-engine
cargo +1.98.1 test --offline -p photocraft-engine vector_packages::snapping
cargo +1.98.1 test --offline -p photocraft-ui-egui --test snapping snapping_ui
cargo +1.98.1 test --offline -p photocraft-ui-egui --test snapping snapping_ui_manager
WGPU_BACKEND=vulkan cargo +1.98.1 test --offline -p photocraft-ui-egui --test snapping snapping_full_app_gpu -- --nocapture
CARGO_TARGET_DIR=target/clippy cargo +1.98.1 clippy --offline -p photocraft-engine -p photocraft-ui-egui --all-targets -- -D warnings
WGPU_BACKEND=vulkan cargo +1.98.1 test --offline -p photocraft-engine -p photocraft-ui-egui -- --test-threads=3
CARGO_TARGET_DIR=target/clippy cargo +1.98.1 clippy --offline -p photocraft-engine --all-targets -- -D warnings
cargo +1.98.1 fmt -p photocraft-engine -p photocraft-ui-egui --check
```

Engine check, both-crate all-target clippy with `-D warnings`, final formatting check and `git diff --check` passed. Logs: `target/snapping-check.log`, `target/snapping-engine-focused.log`, `target/snapping-ui-focused.log`, `target/snapping-ui-manager.log`, `target/snapping-gpu.log`, `target/snapping-clippy.log`, `target/snapping-full-tests.log`. The pre-existing missing craft-fonts build-script warning is separate from lint results; no font assets were supplied/changed.

The focused GPU fixture passed **1 test, 0 failed/ignored**, using the full real PhotocraftApp/egui GPU canvas offscreen. It verifies detached Move guide preview, identical release pixels, one undo step and bypass at PPP1/2. Actual adapter: **NVIDIA GeForce RTX 5090**, Vulkan, DiscreteGpu, PCI `0000:01:00.0`, vendor 4318/device 11141, NVIDIA driver `615.71.09`; required compositor format **Rgba32Float**. Software/absent adapters fail. The independent blue-rectangle/white-background pixel oracle covers all 33,600 swept-region pixels, including edges, with max diff<=1/255. Transient guide/snap-bounds decorations are hidden after press while captured targets remain active, for unobstructed paint comparison; no pixels/edges are excluded. This verifies the existing guide/Move bridge against the shared query, **not** native integration of the new nodes/curves/spacing/manager.

Remaining mandatory gates: 00 native module/router/persistence/catalog/Move extension and V1 dependency install; real V1 nearest/bounds fixtures and full-app node/curve/spacing/manager GPU interactions after integration. The analytic parabola callback fixture checks the adapter and atomic query composition only, not V1's absent solver. No proprietary/external corpus or skipped GPU counted as a passing gate. No interactive latency or performance claim is made, so no microbenchmark is presented as an app measurement.

Sources: pinned VC `geom/snap.rs` and engine `tests_smartguides.rs`, existing Move report/snap/capture code, official [Affinity snapping documentation](https://affinity.help/designer2/en-US.lproj/pages/DesignAids/snapping.html), local locked egui/egui_kittest APIs, architecture and read-only V1 adapter signature. One adapted-file PORTS row is supplied to 00; existing VectorCraft copyright/licenses/notices retained. Fixture parameters/tools/licenses are recorded in each owned snapping `SOURCES.md`.
