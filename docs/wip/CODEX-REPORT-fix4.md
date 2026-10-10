# Four post-merge test failures

Continued the existing branch, including its CPU timing subprocess and scoped pending
translation lists. No commits, new dependencies, unsafe code, or GUI windows.

## Causes and fixes

1. **GPU slider timing:** `lightcraft_gpu::ENABLED` and fallback diagnostics are
   process-wide. Other headless apps apply their GPU preference during UI frames.
   The GPU timing test shared those globals with parallel tests; its assertion at
   the reported line checks the enabled backend, not a millisecond threshold.
   The GPU measurement now runs in its own test subprocess, following the existing CPU
   measurement. Preserved pointer tracking, draft bounds, rendered pixels, full
   quality on release, undo grouping, GPU stage storage, fallback checks, and all
   timing output. Hardware validation uses the RTX 5090 with Vulkan and
   `LC_REQUIRE_GPU=1`.

2. **Smart Sort setup:** the merged `smart_sort_task::start` contained two calls
   to start face analysis. The first was before embedding-cache validation. An
   invalid cache therefore left a face worker running, and valid setup subsequently
   failed because face analysis was already running. Removed the premature call;
   retained the existing start after setup and cancellation if spawning the tagging
   worker fails. Extended the existing UI regression to repair the invalid cache and
   click Analyse successfully with both analyses enabled.

3. **Unload completion:** the worker already publishes its note before calling
   `ctx.request_repaint()`. The regression assumed eight frames and a 200 ms delay
   produced an idle UI, but egui coalesces requests while a repaint is pending.
   Its service override and status note also competed with unrelated tests.
   Isolated the regression in a test subprocess, gated note publication with its
   existing mutex, removed the hover, and settled the harness before releasing the
   worker. Required an immediate repaint from `ai-unload`, then verified the published
   success status. No production unload behavior changes are needed.

4. **Translation coverage:** the existing branch already includes the Overwrite
   Original, Z-Image Variety, and Restoration strings in the pending lists. The
   literal scan finds no missing entries across all 12 translated catalogs.
   Preserved that completed work and English fallback without translating or broadening
   the exceptions. The Rust coverage test passed.

Required linting also found another merge seam: the existing
`zimage_variety_settings_presets_and_requests_keep_old_defaults` test was nested
inside `ds2_refine_lora_preset_roundtrip_and_invalid_step_limits`, so Rust could
not register it. Moved it to the surrounding scope with `#[cfg(test)]`, following
the neighboring tests. Its body is unchanged, and it now runs and passes.
This fixes both `unnameable_test_items` and `dead_code` without suppressing lints.

## Sources followed

- [egui 0.36.2 Context documentation](https://docs.rs/egui/0.36.2/egui/struct.Context.html#method.request_repaint)
  describes coalesced requests and worker wakeups. The installed version's
  `src/context.rs`, `ContextImpl::request_repaint_after`, confirms the callback is
  emitted only when the requested delay becomes smaller.
- Installed `egui_kittest` 0.36.2 `src/lib.rs`, `Harness::run` / `try_run`, provides
  the established settling behavior rather than a fixed frame count.
- Existing repository code: `crates/lc-ui-egui/src/tests_slider_speed.rs` CPU
  subprocess; `crates/lc-ui-egui/src/people_task.rs` task ownership and cancellation;
  `crates/lc-ui-egui/src/lib.rs::apply_settings` preference application;
  `crates/lc-gpu/src/lib.rs` global preference and fallback state;
  `crates/pc-ui-egui/src/i18n/mod.rs` pending lists and coverage gates.
- Existing translation additions in commits `59c539bb` (Overwrite Original),
  `53d4b1d0` (Variety), and `68554571` (Restoration) were retained.

## Validation

All Cargo builds use Rust 1.98.1, `--offline`, `CARGO_BUILD_JOBS=3`, and one build
at a time. Test temporary/configuration/data directories are scoped under
`target/fix4/`. Hardware validation sets `LIGHTCRAFT_GPU=1`,
`LIGHTCRAFT_GPU_BACKEND=vulkan`, `WGPU_BACKEND=vulkan`, `LC_REQUIRE_GPU=1`, and
`LOCAL_IMAGE_REQUIRE_GPU=1`.

The original Smart Sort test reproduced its reported failure at line 690:
`h.app.people_task.is_none()`. The original slider group and unload test passed
in isolation; those initial runs do not reproduce parallel-suite interference.
The shared-state hazards and repaint coalescing were verified in the source
listed above, and the revised tests retain the functional/backend assertions.

Focused verification after the fixes:

| Command/filter | Passed | Failed | Ignored |
| --- | ---: | ---: | ---: |
| `test --offline -p lightcraft-ui-egui ds2_smart_sort_setup_failure_does_not_start_face_worker -- --nocapture` | 1 | 0 | 0 |
| `test --offline -p lightcraft-ui-egui tests_slider_speed -- --nocapture --test-threads=4` | 5 | 0 | 1 |
| `test --offline -p photocraft-ui-egui ds_finding_unload_completion_wakes_idle_ui -- --nocapture` | 1 | 0 | 0 |
| `test --offline -p photocraft-ui-egui every_tl_literal_is_translated -- --nocapture` (existing lists retained) | 1 | 0 | 0 |
| `test --offline -p photocraft-ui-egui zimage_variety_settings_presets_and_requests_keep_old_defaults -- --nocapture` | 1 | 0 | 0 |

`cargo +1.98.1 fmt -p lightcraft-ui-egui -p photocraft-ui-egui` and
`cargo +1.98.1 check --offline -p lightcraft-ui-egui -p photocraft-ui-egui` passed.
Focused logs are `target/fix4/smart-sort-after.log`, `slider-after.log`,
`unload-after.log`, and `i18n.log`.

### Interactive hardware measurements

Actual Library/Develop `Headless` pointer presses, six moves per control, and
release run through the production UI, session, render workers, GPU pipeline,
readback, and egui texture staging. Adapter: **NVIDIA GeForce RTX 5090 (Vulkan)**.
Drafts are 861×574; release restores 1435×956 full-quality output. The focused
run recorded 24 hardware slider samples, with no missing-adapter skips or GPU fallbacks.

| Control | UI mean (ms) | Change → texture mean (ms) | Change → texture p95 (ms) | Release → full (ms) |
| --- | ---: | ---: | ---: | ---: |
| Exposure | 0.69 | 6.66 | 8.88 | 10.84 |
| Highlights | 0.64 | 9.49 | 16.28 | 18.63 |
| Clarity | 0.58 | 7.92 | 15.75 | 18.95 |
| Saturation | 0.59 | 7.56 | 8.77 | 8.26 |

During the unfiltered four-thread suite, the same hardware test recorded mean
step latencies of 10.73–15.09 ms and p95 latencies of 13.64–34.57 ms across the
four controls. All rendering, interaction, and GPU assertions passed under that
concurrent load too. Per-step measurements are retained in `target/fix4/full-tests.log`.

These are observations on the shared host, not performance thresholds or a claim
of improvement. They measure texture readiness, not final desktop presentation.
No new GPU test was added: the existing automatic GPU slider test was hardened,
and its actual UI drag, output, history, cache, backend, and fallback checks remain.
The ignored slider test is the existing optional 120-step continuous benchmark.

### Linting and full suites

Passed strict linting for both changed crates and all their targets:

```sh
CARGO_TARGET_DIR=target/clippy cargo +1.98.1 clippy --offline \
  -p lightcraft-ui-egui -p photocraft-ui-egui --all-targets -- -D warnings
```

The initial lint run exposed the nested Z-Image test described above; the corrected
run passed. Logs: `target/fix4/clippy-before.log` and `target/fix4/clippy.log`.
Cargo still reports the existing optional `craft-fonts` build-script warning;
the worktree has no embedded Chinese/Japanese fonts. Rust/Clippy lint checks pass.

Ran the full suites once, with four test threads and the hardware environment
above:

```sh
cargo +1.98.1 test --offline -p lightcraft-ui-egui -p photocraft-ui-egui \
  -- --nocapture --test-threads=4
```

| Crate / suite | Passed | Failed | Ignored |
| --- | ---: | ---: | ---: |
| `lightcraft-ui-egui` unit tests | 251 | 0 | 3 |
| `photocraft-ui-egui` unit tests | 1021 | 0 | 6 |
| `photocraft-ui-egui` GPU integration tests | 62 | 0 | 4 |
| `photocraft-ui-egui` other integration tests | 14 | 0 | 0 |
| **Total** | **1348** | **0** | **13** |

Both crates' doctest suites also passed (zero doctests). All four reported test
names passed in the full run, as did the restored Z-Image regression. The run
reported NVIDIA GeForce RTX 5090 (Vulkan) for Develop and PhotoCraft colour-icon
rendering at both 1× and 2× scales. There were no missing-GPU skips.
Full log: `target/fix4/full-tests.log`.

The 13 ignored cases are existing optional benchmarks, hardware diagnostics,
the tool-icon review-sheet writer, and restoration testing requiring pinned model
downloads and an idle ComfyUI. Their default ignore status was retained; no new
test was ignored. No real ComfyUI jobs were submitted.

Nothing remains for the four task points or strict linting. The existing English
fallbacks intentionally remain pending the owner's translation pass. Changes
are left uncommitted in the working tree.
