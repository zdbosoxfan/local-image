# Vector 05 — Pencil / Sculpt

Status: owned Pencil/Sculpt implementation and validation complete; final integrated acceptance
remains open. App/persona registration and V1 wiring are reserved to 00/01 and the V1 provider;
advanced appearance rendering requires 08. Those providers are not merged in this branch.

The previous branch work was inspected and preserved: vector foundations and compiled empty
Pencil slot. The initial working tree contained only the untracked task brief; no separate
Pencil WIP was present. No commits, dependencies, lock/catalog/router edits, desktop windows,
owner file edits, ComfyUI requests, or raster Pencil changes.

Implemented owned modules: bounded freehand cubic fitting with tolerance/smoothness,
rope/window/no stabilization and cancellation, detached cached preview, explicit document ID/
revision/target capture, one journaled command/undo, Alt continuation at either endpoint and
closure, Sculpt anchor intervals and exact subcurve boundaries (including closed wraps).
Stabilized pressure is available for WidthProfile consumption without brush rendering.
Old option/UI-state JSON loads via serde defaults; no persisted document schema changes.

Exact registration/V1/appearance hooks, translation and PORTS fragments are in
`docs/wip/vector-tasks/integration/05.md`; `i18n/05.tsv` covers the twelve existing languages.
Fixture licenses/parameters, pinned source hashes, official sources and gaps are recorded in
`docs/wip/vector-tasks/pencil/SOURCES.md`.

Validation completed (one Cargo build at a time, offline cargo 1.98.1, three build jobs):

- Engine check passed.
- Current focused engine `--lib pencil`: 20 passed, 1 existing ignored latency test,
  859 filtered; includes eighteen new geometry/transaction/GPU tests and two legacy raster
  Pencil checks. Log: `target/pencil-focused.log`.
- Independent circle gate: radii 10/50/100, 241 samples, tolerance 1.5 pixels, <=8 nodes,
  symmetric Hausdorff <= tolerance and no crossings. Corner/sparse cases, 256 deterministic
  sine regressions, spiral, flat ellipse, jitter and interval tests pass, including independent
  Bernstein evaluation of exact Sculpt boundary splits.
- UI focused run: twelve checks passed (six new widget/gesture tests and six existing raster
  tests). Initial full-app parity failed because the existing contextual task bar covered
  canvas pixels after shape selection. Corrected capture uses its existing in-memory hide
  preference and removes kittest's synthetic cursor, preserving actual app/GPU execution.
- Corrected full-app GPU focused test: 1 passed, 0 ignored, 1002 filtered;
  `target/pencil-ui-gpu-focused.log`. Live/release max premultiplied diff 0/255;
  detached float CPU/preview and preview/commit max diff both 0.
- Hardware: NVIDIA GeForce RTX 5090, discrete Vulkan, driver 615.71.09,
  PCI 0000:01:00.0; Rgba32Float asserted. Neither new GPU gate is ignored or skipped.
- Engine, registered vector-helper, and full UI rehearsal clippy all-targets with
  -D warnings passed. Logs: `target/pencil-clippy-{engine,vector,ui}.log`.
- Release full-app GPU test: 1 passed, 0 ignored, 1001 filtered. Log:
  `target/pencil-release-gpu.log`; same strict hardware/format gate, max pixel difference 0/255.
- Full engine suite (once): 1,498 passed, 0 failed, 16 existing ignored.
  Unit binary 869/11; integration binaries 582/0 and 47/5; doc tests 0.
  `target/pencil-full-engine.log`. The eighteen new owned tests pass, including the final
  independent Bernstein boundary oracle and strict GPU test.
- Full registered vector-helper suite (once): 90 passed, 0 failed, 1 existing ignored
  performance benchmark. Unit 48/1, integration 42/0, doc tests 0.
  `target/pencil-full-vector.log`. All nine owned geometry tests run in the actual vector crate.
- Full registered UI coverage: 1,071 passing cases, 9 existing ignored; doc tests 0.
  Unit run: 997 passed, 1 failed, 5 ignored. Its sole failure was the scratch workspace's
  missing link to the existing repository `pc-plugins/tests/fixtures/invert.wasm`, not a missing
  external corpus. Added that read-only sibling link to the owned rehearsal script; the exact
  failed test then passed (1 passed, 1002 filtered), without repeating the unit suite.
  Remaining integration binaries: GPU 59 passed/4 ignored, CPU 14 passed/0 ignored; both passed.
  Logs: `target/pencil-full-ui.log`, `target/pencil-ui-fixture-recheck.log`,
  `target/pencil-full-ui-integration.log`, `target/pencil-full-ui-doc.log`.
  All seven new owned UI tests passed, including strict full-app GPU preview/release/cancel.
  The coordinator pause interrupted the first UI compilation before any tests ran; its log
  is `target/pencil-full-ui-interrupted.log`. Resuming reused cached artifacts.
- Formatting checks for all three changed crates and the otherwise unregistered UI module,
  `git diff --check`, and byte equality between owned sources and tested scratch registrations
  passed. No original shared registration files were changed.

Existing ignored engine tests cover latency/benchmark cases, a full-registry panic fuzz gate,
an isolated child-process entry, user-supplied Camera Raw PSD, a smart-object export benchmark,
and manual Photoshop verification. The vector ignore is its existing 6016 px performance
benchmark. They are not represented as passing gates. The existing build-script warning about
the absent optional craft-fonts checkout is separate from Pencil fixtures and Rust lint results.
UI ignores are existing channel/marquee/transform benchmarks, a contact-sheet generator,
the explicitly opt-in GPU icon test, and four GPU canvas/move benchmarks. The owner's existing
`li-test-runner` emitted summary arithmetic warnings after the multi-test binaries; Cargo's
individual test results and exit codes above are the evidence, rather than that wrapper summary.

Release measurements (repository release profile, thin LTO, codegen-units=1; 40 real moves
in a warmed offscreen PhotocraftApp on a 240x160 document, default tolerance 1.5 / smoothness
0.5 / no stabilizer / 2 px stroke). The shared machine was not isolated. Median uses the upper
middle observation; p95/p99 use the indicated order statistics of 40 samples. No extrapolation
to large documents, tablets or the final merged persona/backend is claimed.

| Actual app stage | Median ms | p95 ms | p99 ms |
|---|---:|---:|---:|
| Pointer frame (capture, fit, cache, app UI) | 1.214 | 4.574 | 6.844 |
| Subsequent presentation frame | 1.432 | 4.489 | 5.272 |
| GPU submission/readback | 3.287 | 7.871 | 8.978 |
| Measured per-move total | 7.134 | 12.253 | 14.273 |

The scratch registration rehearsal runs the real PhotocraftApp, canvas callbacks and compositor
without native windows. Until package 01 lands it uses Pen solely as an activation sentinel,
explicitly distinguished from the final vector-pencil registry. Passing this rehearsal does
not establish final persona/toolbar preference registration, the merged V1 fitter, or 08's
advanced appearance renderer. Those named dependencies require integrated acceptance afterward.
00/01 must apply the exact module/tool/router/compositor/preference/catalog/PORTS fragments;
V1 must supply the actual fitter callback and rerun the numerical gates; 08 must enable enhanced
appearance caches before those edits are accepted. External tablet captures/manual tablet QA
and proprietary/reference corpus gaps in SOURCES.md remain unclaimed. No work outside this
package's ownership was substituted for those integrations, and no changes were committed.
