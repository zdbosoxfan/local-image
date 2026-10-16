# J19 — one shared merge backend and architecture HDR

Continued the inherited J10/pixel-shift branch (`9cbf75e4`), retaining its MOPS decision and Sony implementation. Changes are uncommitted. The pre-existing owner edits to `CODEX-TASK.md` were not modified.

## What is consolidated

The existing `lightcraft-merge` package is the single backend; its directory/name stays `crates/lc-merge` to avoid breaking current callers or creating a redundant facade package named `li-merge`.

- Native MOPS/ANMS detection, full-float descriptors and native motion fitting/RANSAC serve both Library and editor production registration. Editor integer pixel centres are explicitly adapted from native half-pixel centres. Public binary descriptor APIs remain as compatibility adapters; production engine paths use full-float MOPS.
- `pc-algo::{features,hdr,panorama}` retain public types/functions through thin re-exports or geometry adapters. HDR tone mapping remains editor-specific. Rendered sources keep response recovery; linear/raw sources keep calibrated scene-linear radiometry. A response-recovery algorithm is not applied to sensor samples.
- Both panorama paths use one borrowed binomial pyramid/expansion implementation and one multi-channel accumulator/normalizer/collapse in `pano::blend::Blender`. Editor ROI padding, channel layout, layer placement, transparency and seam behavior remain adapters. Native log-RGB input remains log-RGB; editor channel input retains its encoding.
- The dense numerical solver is shared; dead duplicate editor solver/normal-equation bodies were removed. Camera-rotation bundle adjustment and editor layer/radial-distortion objectives remain distinct supported models inside the shared backend. J10 did not select a winner between these different geometry objectives, and this report does not invent one.
- Editor HDR uses the existing background job runner, with progress and cancellation, MOPS subpixel similarity registration, and Ward MTB fallback. Uncovered alignment borders are transparent and excluded from response calibration/fusion/deghosting instead of fabricating clamped-edge samples. Integer offset results remain available alongside precise alignment matrices.

## HDR production behavior

`merge_raw_hdr` black-subtracts and normalizes native samples, builds complete-CFA-tile registration proxies, aligns through the shared backend, fuses CFA samples, and demosaics once with the existing AHD implementation. Bayer resampling interpolates on the same phase's period-two lattice with a positive tent kernel. Saturation is checked on the footprint before averaging, preventing a clipped source tap from becoming falsely valid after interpolation. Camera calibration, normalized analog balance, as-shot white balance, orientation, crop, metadata and baseline exposure follow the reference capture. Negative sensor noise remains signed; scene radiance above one is retained.

Library HDR and HDR panorama now write **32-bit float LinearRaw DNG**, previously 16-bit half float. They develop through the normal raw pipeline. Previews fuse the same original sensor lattice and then resize the result/overlay. They therefore need full-resolution merge memory, rather than silently producing a different small-CFA merge.

Registration includes robust Lucas–Kanade-style similarity increments after feature initialization. Refinement excludes sensor clipping by channel, not just by luminance, and accepts only steps that lower its robust photometric residual with adequate overlap and plausible geometry. This fixed a measured small-scene failure without relaxing the 0.5-pixel test gate. Alignment `rms` remains the feature-fit residual before direct refinement, as documented; ground-truth alignment error is measured separately below.

`None/Low/Medium/High` deghost strength and the red ghost-mask preview overlay are retained. Tests exercise chromatic motion, increasing strength coverage, crop/overlay coordinates and moving-foreground seam avoidance. The positive CFA kernel is designed to avoid interpolation ringing; a general absence of window halos/banding across all cameras is **not** established by the small tests.

Bayer, monochrome and LinearRaw support the sensor-domain alignment path. Other CFA layouts retain their old aligned demosaic-first Frame adapter through the engine, with `rawDomain: false` in result information; the new raw API rejects unsupported fractional CFA layouts directly. This preserves existing default X-Trans merge support without calling a Bayer sampler on X-Trans. Exact unaligned irregular-CFA fusion is supported. Mixed raw/rendered sets retain the established scene-linear Frame adapter. No existing catalog or document schema changed.

## Shoot grouping and background batch

The shared grouping policy uses distinct/evenly spaced EVs, capture times, camera/dimensions and an exposure-normalized spatial log-luminance descriptor. Repeated EV cycles, time gaps and changed scenes split sets. Flat, unknown or irregular captures remain ungrouped. Thresholds are explicit serializable options; this is a conservative project policy, not an undocumented recreation of Adobe's grouping algorithm.

Library HDR has a shared-widget **Automatically Group Brackets** checkbox, default off for old settings. The preview shows the first detected set, group count and ungrouped count, with the existing deghost overlay. Final batching runs outside the session. It processes one set at a time, provides overall progress/cancel, and reports failed/ungrouped sets.

File write/sync, metadata hashing and optional Auto Settings are prepared on the worker. The UI attaches the prepared file to the catalog and applies settings. A bounded result channel plus import acknowledgement prevents accumulating a shoot's DNG buffers. Verified worker probes avoid a second full-file read/hash in the UI import path. New generated files clean themselves up if cancelled/rejected or the library changes before attachment; accepted files and subsequently replaced owner files are preserved. Catalog/settings mutation and its journal operations still happen on the UI thread.

Sensor decode is sequential within a set, rather than multiplied by logical CPU count. Every raw source is probed for matching extent/layout before decoder allocation. Existing hardware-adaptive memory admission and decoder budgets protect full merges; the checked estimate is `pixels × (shots × 4 × cpp + 112) + 8 MiB`. For a three-frame 50 MP Bayer bracket it is approximately **6.21 GB**, independent of this machine's CPU/GPU configuration. This is an admission estimate, not measured RSS. Batch raster storage scales with one set, not all 30–60 sets. No 60-set performance sweep was run under the owner's minimum-testing override.

## Measured quality and speed

A single small sample uses the same fixed CC0 [Poly Haven Lebombo](https://polyhaven.com/a/lebombo) radiance as J10, licensed [CC0](https://polyhaven.com/license). Original: `target/j19/data/lebombo_1k.hdr`; SHA-256 `a02c9615adeba91a753c3e4737c3ce7a719273df822eeaa85d24982bd43c9ecb`.

The example `j19_quality` makes identical 256×128 RGGB captures at −4/0/+4 EV using J10's fixed noise generator. Both paths receive the same Bayer samples. The reference is a noiseless Bayer capture developed once with AHD; this isolates fusion order from CFA reconstruction. It is simulated capture data derived from CC0 radiance, not an independently captured raw bracket. Alignment is disabled for this radiometric sample; no tone mapping or fitted post-output rescaling is used.

| Static interior metric | Previous demosaic-first path | New raw-first path |
| --- | ---: | ---: |
| Truth DR (1%–99.9%) | 7.427565 stops | same reference |
| Output DR | 7.426674 stops | 7.430649 stops |
| Green log RMSE | 0.031686 stops | **0.021016 stops** |
| Shadow relative RMSE | 10.9330% | **2.0676%** |
| Highlight log RMSE | 0.048002 stops | **0.034047 stops** |
| Edge log RMSE | 0.103550 stops | **0.048415 stops** |
| One-shot CPU merge/develop time | 9.573 ms | **3.868 ms** |

DR is not a quality score by itself; errors can increase apparent DR. Shadows/highlights/edges follow the fixed J10 conditions, scored against the noiseless demosaic reference.

| Moving-rectangle interior metric | Previous path | New path |
| --- | ---: | ---: |
| Motion-region log RMSE, None | 0.083295 stops | 0.075884 stops |
| Motion-region log RMSE, High | 0.031859 stops | **0.026587 stops** |
| Non-motion log RMSE, High | 0.029168 stops | **0.021026 stops** |
| Shadow relative RMSE, High | 6.5455% | **2.5405%** |
| Edge log RMSE, High | 0.085302 stops | **0.049681 stops** |
| One-shot CPU merge/develop time, High | 10.747 ms | **4.298 ms** |

Separate focused evidence:

- Synthetic window highlights remain above **3.99** scene-linear units. Measured CFA-site radiance RMSE is **5.4e−8**, equal to the previous path on this noiseless simple scene; no improvement is claimed where the numbers show parity.
- The full handheld sensor→proxy→registration→fusion path initially had **1.959379 px** maximum error on the unchanged two-point ground-truth gate. Clipping-aware direct refinement reduces it to **0.216176 px**, passing the original **<0.5 px** criterion. It is a small procedural scene, not a camera-wide handheld accuracy guarantee.
- J10's selected detector quality remains the prerequisite evidence: **8/8** cases versus old editor **7/8**, median transform error **0.03423 versus 0.47521 px**. These are inherited J10 measurements, not rerun results or a new panorama bundle-adjuster comparison.
- Pre-J19 rendered response/radiance, exposure gain, seam and multiband outputs were harvested from exact HEAD snapshots under ignored `target/j19/legacy/`. Literal numerical goldens now independently gate the migrated production code: four HDR pixels/DR, six response points, two gains, six blend samples and a global sum. No reference implementation copy remains in repository source.
- One authorized owner sample check passed using three actual ARWs and 128×128 center crops retained only in memory. The selected captures decode as reduced camera-RGB raw, so this is not a real-CFA acceptance result. Owner-scene numerical measurements remain **only** in ignored `target/j19/logs/test-owner-crop-final.log`; they are deliberately not reproduced in this potentially publishable report. No independent ground truth/superiority is inferred. Originals were read only; no photo content/crops/renders were put in repository files or uploaded.

These are diagnostic CPU timings in Rust 1.98.1's inherited dev profile on AMD Ryzen 7 9800X3D, not repeated benchmarks, GPU measurements or full-resolution interactive latency claims. The new actual engine command/prepare tests include decode, DNG output, catalog attachment and Auto; their timings are recorded with the final test results below.

## Reference behavior and provenance

- [HDRMerge's official description](https://jcelaya.github.io/hdrmerge/) and [manual](https://jcelaya.github.io/hdrmerge/documentation/2014/07/11/user-manual.html): raw exposure merging, low-noise pixel selection, floating DNG and ghost editing. This implementation fuses normalized sensor observations before demosaic, but writes LinearRaw RGB and provides strength/overlay rather than HDRMerge's manual source-mask painting.
- [darktable's upstream HDR alignment design](https://github.com/darktable-org/darktable/blob/master/dev-doc/HDR_Alignment_Design.md), read 2026-10-14: CFA-free proxies, calibration/exposure-aware accumulation and same-color tent resampling informed the new raw path. This newer upstream design includes alignment; the older [4.6 manual](https://docs.darktable.org/usermanual/4.6/en/module-reference/utility-modules/lighttable/selected-image/) cited by J10 describes the tripod-only limitation. No darktable C/C++/OpenCV is in the build.
- [Lightroom Classic HDR merge documentation](https://helpx.adobe.com/lightroom-classic/desktop/process-and-develop-photos/hdr-photo-merge.html): handheld auto-align, four deghost strengths, preview overlay and DNG behavior. Those controls and behavior are present. **32-bit** is the owner's requirement, not a bit-depth assertion attributed to Adobe.
- [Lucas & Kanade, IJCAI 1981](https://www.ri.cmu.edu/pub_files/pub3/lucas_bruce_d_1981_2/lucas_bruce_d_1981_2.pdf) informs direct gradient-based registration. Robust weights, clipping validity, bounded similarity increments and the acceptance gate are project implementation decisions; no claim of a bit-exact upstream port.
- Existing native MOPS/Brown–Lowe/Hartley–Zisserman/Burt–Adelson module citations and the established raw normalization, AHD, DNG writer, safe-file and job APIs were followed. `docs/PORTS.md` and existing tests/references were inspected before changes.

`hdrmerge` and `darktable-cli` are not installed, and no Lightroom reference render is available. External renderer pixel-level parity and best-in-class superiority remain **unmeasured**. The report compares documented reference behavior and measures actual outputs against controlled scene truth plus replaced implementations; it does not claim proprietary-product equivalence. Real moving people/curtains/trees/car brackets, full-size 100 MP window edges, broad camera/color accuracy and a 30–60-set shoot are coordinator acceptance work under the explicit no-long-sweeps override.

## Scope, compatibility and validation

Unavoidable edits outside the listed owned implementation files:

1. `crates/pc-algo/Cargo.toml`: reference the existing shared `lightcraft-merge` package.
2. `Cargo.lock`: only two existing workspace dependency edges (`pc-algo → lightcraft-merge`, `lc-merge → li-raster`). **No package/version additions/removals, no new C/-sys dependencies.** `li-raster` preserves the established rendered deghost blur.
3. `crates/pc-algo/src/photo_util.rs`: remove dead duplicate numerical implementation after its callers move, retain old utility tests via shared aliases.
4. `crates/lc-ui-egui/src/merge.rs`: required actual batch controls, background delivery/prepare and headless click tests. Existing tokens/shared widgets are used for new controls. Old dialog JSON still defaults to single merging.
5. `docs/wip/i18n-pending-j19-li-merge.md`: pending new UI/worker strings; no translation project/catalog rewrite.

No Match Look files, original Desktop folders, GUI windows, GPU services or ComfyUI were touched. No commit/push/upload was performed. The retained target directory and inherited `CARGO_BUILD_JOBS=4` were used throughout, with one Cargo process at a time, offline.

Final command/test results are recorded below. Full crate/workspace suites, GPU acceptance runs, repeated benchmarks and long quality sweeps were not run, following the owner's overriding ABSOLUTE MINIMUM TESTING rule. GPU tests added: **0**; UI screenshots use the existing CPU headless renderer.

TEST-CHANGED: No existing test assertions were weakened or removed. Existing tests remain through the compatibility facades. Only new `j19_` tests ran. The initial normalization-fixture and alignment failures, owner sample layout-fixture failure, compiler format-macro/egui API failures, UI harness font/texture-delta setup failures, and two strict-clippy test-style failures are retained under `target/j19/logs/`; fixes corrected fixture/API/implementation behavior rather than lowering thresholds.


## Final targeted verification

All commands used `PATH="$HOME/.cargo/bin:$PATH"`, toolchain `+1.98.1`, offline, the same retained target directory, and the inherited four build jobs. Only the following new test filters ran; existing integration harnesses were not exercised.

| Changed package | New filter | Final passing test definitions |
| --- | --- | ---: |
| `lightcraft-merge` | `--lib j19_` | 10, plus one optional owner test below |
| `photocraft-algo` | `--lib j19_` | 12 |
| `lightcraft-engine` | `--lib j19_` | 5 |
| `photocraft-engine` | `--lib j19_` | 3 |
| `lightcraft-ui-egui` | `--lib j19_` | 4 |

**35 new test definitions passed**, including the optional local owner-sample test. The default merge filter leaves that owner test ignored; it was explicitly run successfully with `LOCAL_IMAGE_OWNER_SHOTS` and `j19_owner_shots -- --ignored --nocapture`. No skipped test is counted as a pass. A temporary old-source numeric-harvest test also ran once to obtain independent golden constants, then was removed from repository source.

The UI's first new filter run passed three tests and exposed missing font/texture-delta setup in the fourth test's custom bare context. That test now uses the established `HeadlessView`; only `j19_hdr_batch_rejected_prepared` was rerun after its harness corrections, and passed. The last numeric hardening extends `j19_hdr_rejects_empty_and_nonfinite_inputs` to finite EV values whose exponentials overflow/underflow; only that new test was rerun. No thresholds or existing expectations were lowered.

Actual engine-path diagnostic CPU timings from the tiny 48×32 fixtures:

- HDR command decode + fusion + float DNG write + catalog import: **19.597 ms**.
- Worker DNG write/sync + metadata probe/hash + Auto Settings preparation: **22.876 ms**.

These include real production APIs, with synthetic inputs under target. They are one-shot small-path timings; previous full interactive latency and 50/100 MP app responsiveness are not measured. Merger core before/after timings are in the CC0 tables above.

Eight CPU screenshots were generated and visually inspected: `target/design-system/j19-hdr-batch-{dark|light}-{250|400}-{1|1.5}.png`. The test actually clicks automatic grouping, the ghost overlay and Cancel, asserting option/cancellation changes. Other tests exercise stale-library rejection, partial completion and prepared-file cleanup. All eight variants fit and remain readable.

Final formatting, `cargo check --all-targets` and strict `cargo clippy --all-targets -- -D warnings` cover all five changed packages together. Exact logs are retained in `target/j19/logs/` (`check-last.log`, `clippy-last.log`, per-package new-test logs, and narrowly filtered fix logs). The existing build-script notice about optional craft-fonts remains; the English HDR UI uses the installed shared fonts and passes headless checks.

`git diff --check` passes. `Cargo.lock` package/version sets match HEAD exactly; only the two documented workspace dependency edges changed. All changes are left uncommitted.
