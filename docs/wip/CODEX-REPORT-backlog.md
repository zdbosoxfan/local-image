# Bug backlog — 2026-10-09

Continued the inherited work at `bd7ccd96`; no commits or dependency changes.
The branch already contained the earlier input guards and corrected coverage
tests. The initial and final Rust-only searches for `ignore = "BUG` found no
remaining ignored bug tests. The excluded ComfyUI, translations and vectorizer
entries are preserved verbatim in the backlog.

## Per-item decisions

| Item | Result and evidence |
| --- | --- |
| Empty catalog text `contains` | Deliberate empty-substring match, inherited `186ae312`. The focused regression passes (1). |
| JPEG width zero | Already guarded before row chunking, inherited `5f94b904`; regression passes (1). |
| RawProcessing array | An empty sequence produces defaults through serde's struct-sequence support, inherited `8e7632c6`; regression passes (1). Preserved compatibility rather than replacing the deserializer. |
| Luminar NaN settings | Inherited `5e116465` rejects non-finite values; focused Luminar coverage passes (22). |
| Non-XMP sidecar | Existing XML validation rejects it; malformed-sidecar regression passes (1). |
| LLF `dl(0, level)`, empty LUT, zero-size levels | Inherited `5085dcc9` handles each input before subtraction/indexing; focused coverage passes (26). |
| Installed model required nodes | Existing availability uses the live registered model and builder-required nodes. The stale test now follows that catalog contract (`f4d67c91`, serialized catalog fixture `c5575c9d`); regression passes (1). No fallback `ModelDef` is used to declare an installed model available. |
| RANSAC unequal point arrays | Inherited `cfb3873c` returns `None`; strengthened the regression to cover both mismatch directions and Translation, Similarity and Homography (1 test). |
| MVC membrane above eight channels | Continued `a3cc9c3a`'s eight-channel batches; added checked pixel/channel/capacity arithmetic and zero-channel handling. A new 19-channel test compares every output channel against its independent one-channel membrane; overflow and zero-channel tests cover invalid dimensions. |
| Subsample limit zero | Inherited `cfb3873c` returns empty before division; five focused subsample tests pass. |
| Display rectangle NaN | Inherited `b5922ab5` returns `None`; focused regression passes (1). |
| Truncated PSD path | Deliberately tolerant of trailing incomplete 26-byte records, inherited `42b0630d`; complete records remain usable. Focused regression passes (1). |
| Procedural seed overflow | Inherited `84e1d10f` uses wrapping arithmetic for deterministic hashing. Pattern and tip regressions pass (2). |
| PSD v6 origin 1 without an ID | Adobe requires four ID bytes. `None` writes ID 0 and reads as `Some(0)`; inherited `63931cfb` corrected the impossible round-trip expectation. Added public documentation of this format limitation. |
| PSD v7 alignments and color | Replaced the earlier guessed vertical enum IDs with Adobe SDK terminology (`Mddl`, `Bttm`, `baseline`); retained earlier spellings as read aliases. Corrected layer origin to `layer` and color to integer `NativeQuad`. Added an untouched descriptor extracted from a Photoshop CS6 PSD, plus serialized-ID and compatibility tests. The independent fixture proves default alignment, `Clr `, integer RGBA and transparent nonzero RGB. Nondefault mappings follow Adobe terminology and documented slice-alignment order; that fixture does not independently prove them. |
| UTF-16 run lengths | Original conversion is correct. The apparent extra ASCII byte is Photoshop's terminal `\r` (`ffdb2f85` corrected the test). Added direct ASCII, accent, emoji, CJK, terminal-return, malformed surrogate-boundary, NaN/negative/infinite-length tests; production conversion is unchanged. |
| Lost trace-stage generator | Recreated the standard-library Python helper from the documented masks and stage contract. SHA-pinned Potrace 1.16 runs only as an optional reference oracle under `target/refvec/trace-stages`. Two `--check` runs reproduce all three numeric fixtures exactly; checked-in JSON is unchanged. |
| NVIDIA exit crash | Fixed a source-level gap: the detached warm-up thread was absent from the exit barrier. Warm-up is counted before spawn, duplicate compiler threads are prevented, and the compiler thread is joined before initialization is counted out, including thread-local teardown. A counted waiter joins it away from the UI thread, keeping shutdown's existing timeout effective. `quiesce` uses one deadline and returns device-poll errors. A last-drop app field waits after renderer pools join, covering the real Library host and its headless harness. New regressions cover pre-spawn counting, blocked thread-local teardown, separate-process immediate warm-up, and Cmd+Q from an empty Library. The focused hardware shutdown cases pass, but the full hardware Library process still aborts after its assertions pass. These changes fix missing synchronization; they do not eliminate the driver's process-exit failure. |
| NVIDIA banded-render compiler hang | Did not reproduce: the original filtered run, three additional attempts requesting disabled NVIDIA disk caching, and the final full compute suite all pass on the 5090 (five runs). The 3000×2000 result differs from CPU by mean 0.0002 LSB and maximum 1 LSB. This does not establish the historical driver cause or prove that an intermittent hang is eliminated. The supported compute-off fallback is documented below. |
| Colour icons and compositor/vector GPU validation | Added an offscreen test using production wgpu setup and the SVG/texture code shared by Compositing and Develop: all 93 enabled and disabled icons at 1× and 2× pass on the RTX 5090 Vulkan (372 variants). Existing preference-click tests exercise the real UI. Compositor parity passes 41 tests; vector-mask parity passes 1 test containing 18 comparisons (8/16-bit, inversion, density, editing and disabling paths). Vector-mask validation now logs its adapter and cannot silently skip when hardware is required. |

## Validation

All cargo build/test/clippy commands use Rust 1.98.1, `--offline` and `CARGO_BUILD_JOBS=3`.
Only one cargo command runs at a time in this job. Development checks are
filtered; final clippy and the changed crates' full suites are recorded below.
Logs are under `target/backlog/`, with machine-readable summaries in
`verify-results.json`, `final-results.json` and `finish-results.json`.
The final all-target clippy run passed for all eight changed crates with
`CARGO_TARGET_DIR=target/clippy` and `-- -D warnings`. An initial
`items_after_test_module` failure was corrected by moving the added test
modules to the ends of their files; the corrected run passed. The final
`cargo fmt --check` also passed for all eight crates. Formatting initially
conflicted because Compositing imports Library's software painter by path:
the two crates use 160- and 150-column settings. The imported module now
leaves formatting to its home crate; only its two pre-existing formatting
hunks changed. Compositing's all-target clippy passed again after that
formatting-only adjustment; no executable test behavior changed.

Completed full suites (raw test-harness counts, including the PSD doc-test):

| Changed crate | Passed | Existing ignored | Configuration |
| --- | ---: | ---: | --- |
| `photocraft-algo` | 703 | 3 | Standard full suite |
| `photocraft-psd` | 389 | 0 | Includes one doc-test |
| `photocraft-text` | 153 | 3 | Standard full suite |
| `lightcraft-gpu` | 89 | 5 | NVIDIA Vulkan; doc-tests completed separately after the combined run stopped |
| `lightcraft-ui-egui` | 209 | 1 | `LIGHTCRAFT_GPU=0`; normal exit |
| `photocraft-ui-egui` | 1,030 | 7 | Includes 54 GPU integration tests and 14 UI tests; NVIDIA Vulkan |
| `photocraft-gpu` | 85 | 0 | Includes all 45 GPU tests; NVIDIA Vulkan |
| `pc-trace` | 17 | 3 | Numeric fixture regression passes; competitor checks unavailable |
| **Total** | **2,675** | **22** | **Zero assertion failures in the successful completed suite runs** |

The earlier hardware Library run also passed 209 assertions but aborted at
process exit; that failed command is retained in the evidence and is not
counted as a successful full suite. After that failure, only Library's full
suite was rerun with the documented compute-off workaround. The other six
crates had not run in the aborted combined command, so their full suites ran
once afterward. The compute crate's doc-tests had not been reached; they ran
separately and contain zero tests. No whole-workspace suite was run.

These are raw harness counts, not a claim that every optional path ran. The
compute unit suite printed 12 explicit unavailable **CPU Vulkan numerical-test
adapter** returns (the NVIDIA-only ICD deliberately excludes software
adapters); those returns provide no GPU evidence. Four optional `craft-fonts`
checks reported missing embedded CJK fonts, and trace competitor/corpus checks
printed 20 missing-reference notices. No fonts or competitor assets were
installed, and none of these skips closes an excluded gate. Existing ignored
tests are benchmarks, visual dumps/diagnostics, or owner/coordinator gates;
none has a `BUG` reason. Available hardware evidence is identified separately
below. A count breakdown is saved in `target/backlog/full-suite-counts.json`.

Focused changes pass: membrane 7,
slice unit tests 6 plus slice coverage 23, direct text runs 2 plus inherited
UTF-16 regression 1, initial-count barrier 1, joined-thread cleanup 1,
background-render barrier 1, immediate hardware warm-up 1, Library shutdown 1,
hardware icons 1, and upstream trace-stage regression 1. The original
17-command backlog verification passed 94 tests. One added fixture assertion
initially expected alpha 255; the independent bytes and ag-psd data both specify
0, so the assertion was corrected. No reference data was changed.

Hardware is NVIDIA GeForce RTX 5090, driver 615.71.09, Vulkan. Required runs set
`VK_DRIVER_FILES=/usr/share/vulkan/icd.d/nvidia_icd.x86_64.json`,
`LIGHTCRAFT_GPU_BACKEND=vulkan` and `LOCAL_IMAGE_REQUIRE_GPU=1`; icon, warm-up,
Library and vector-mask logs confirm the actual adapter. Compositor logs also
identify that adapter. Explicit GPU-disabled or unavailable-adapter returns do
not count as hardware evidence.

The old Library test binary exited normally in two gdb runs (scroll and empty
failed-save cases) and three isolated-cache-path Cmd+Q attempts. The final
hardware run reproduced a related process-exit failure: all 209 assertions
passed (one unrelated benchmark ignored), then glibc reported `double free or
corruption (out)` and the process exited with SIGABRT. The core's crashing
thread is inside `libnvidia-gpucomp.so.615.71.09`, `_nv002nvvm` in
`libnvidia-glvkspirv.so.615.71.09`, and `libnvidia-glcore.so.615.71.09`. The main
thread is in Vulkan `free_loader_settings` from `_dl_fini`. Our warm-up/compiler
threads have already joined; no barrier timeout was reported. The stacks are
consistent with driver compiler teardown racing library unload, but do not
prove where the heap corruption began. Core metadata is saved in
`target/backlog/library-exit-core-info.log`, for this job's PID 2859831 only.

The cached compute `Gpu` remains in a static `OnceLock` and is never dropped.
A safe explicit resource-release fix needs a change to context ownership:
render/cache users currently receive static references and retain device
buffers. Calling `Device::destroy` alone is not an equivalent fix: the cached
wgpu-core 30 implementation only invalidates the device; it does not release
the retained native resources. No unsafe static mutation or unverified device
invalidation was introduced. The verified compute-off workaround below is the
completion path allowed by the task; the hardware exit issue remains listed.

Fresh cache paths did not produce cache files, so they are not proof of cold
compilation. The further banded attempts explicitly set
`__GL_SHADER_DISK_CACHE=0`; no claim is made that cache isolation fixes the bug.

For a process that avoids compute initialization, start Library with
`LIGHTCRAFT_GPU=0`; this uses the existing CPU Develop renderer. Turning off
**GPU** in Library settings stops future GPU renders, but does not unload an
already initialized device, so restarting with the environment override is
the verified exit-crash workaround. Five Library icon/interaction tests
pass in a separate compute-off process. The empty-Library Cmd+Q case exercises
shutdown but has no adapter in that configuration, so it supplies no hardware
initialization evidence. The full compute-off Library run passed 209 tests,
with one existing ignored 100k-grid timing benchmark, zero failed tests, zero doc-tests and normal
process exit (94.23 seconds for the unit suite).
This is a fallback for the compute path, not a fix for the NVIDIA driver. Set
the environment variable before starting the process; the environment choice
is cached. No GUI was opened, so no claim is made about interactive performance
or reproducing a real desktop application's quit failure.

## References followed

- [Adobe PSD File Format specification](https://www.adobe.com/devnet-apps/photoshop/fileformatashtml/), slices resource 1050: v6 layer-ID condition and v7/8 descriptors.
- [ag-psd descriptor definitions](https://github.com/Agamnentzar/ag-psd/blob/04d927df87130ad6d765bc63d51ade0a98d66c14/src/descriptor.ts), [slice reader/writer](https://github.com/Agamnentzar/ag-psd/blob/04d927df87130ad6d765bc63d51ade0a98d66c14/src/imageResources.ts), and [Photoshop CS6 fixture](https://github.com/Agamnentzar/ag-psd/blob/04d927df87130ad6d765bc63d51ade0a98d66c14/test/read/slices/src.psd). Fixture extraction, SHA and MIT license are in `crates/pc-psd/tests/references/`.
- [psd-tools slice reader](https://github.com/psd-tools/psd-tools/blob/b58704c1c9c9b2459f961560b1e368dfb102b513/src/psd_tools/psd/image_resources.py): mandatory origin-1 ID and descriptor preservation.
- Adobe [PITerminology.h](https://github.com/AdobeDocs/photoshop-cpp-sdk/blob/7e9aca4353a3d4615e9522f47521bf840fb4388c/pluginsdk/photoshopapi/photoshop/PITerminology.h), [PIStringTerminology.h](https://github.com/AdobeDocs/photoshop-cpp-sdk/blob/7e9aca4353a3d4615e9522f47521bf840fb4388c/pluginsdk/photoshopapi/photoshop/PIStringTerminology.h), and [CS6 Help](https://helpx.adobe.com/pdf/cs6/photoshop_reference.pdf), page 660, supply enum terminology and slice cell-alignment order. The nondefault slice mappings are an inference from these official sources because the sampled PSD projects only specify `default`.
- [wgpu Device::poll](https://docs.rs/wgpu/30.0.1/wgpu/struct.Device.html#method.poll), also checked against the locally cached wgpu 30 implementation: wait for queue completion and honor poll errors/timeouts. [wgpu-core device destruction](https://github.com/gfx-rs/wgpu/blob/40f4a34ebaf56f9a046231f54125ad046239d3f3/wgpu-core/src/device/global.rs#L1945-L1967), inspected in the locally cached release at that source pin, invalidates rather than releases retained native resources. Rust's [field destruction order](https://doc.rust-lang.org/reference/destructors.html) places the app barrier after its worker owners.
- Rust [JoinHandle::join](https://doc.rust-lang.org/std/thread/struct.JoinHandle.html#method.join): joins include thread-local destructors that run after the worker function returns. The new deterministic test holds precisely that cleanup open.
- [NVIDIA shader-cache settings](https://http.download.nvidia.com/XFree86/Linux-x86_64/555.58/README/openglenvvariables.html): investigation uses private cache directories under this worktree; the owner's cache is untouched.
- [Potrace 1.16 upstream source](https://potrace.sourceforge.net/download/1.16/potrace-1.16.tar.gz), already pinned in `crates/pc-trace/tests/golden/SOURCES.md`. The generator defines the original CC0 masks directly; no traced output is fed back as the reference input.

## Remaining

The backlog retains three current one-line statuses: NVIDIA process-exit
failure with the verified compute-off workaround; the intermittent compiler
hang, not reproduced; and the missing Photoshop nondefault slice-alignment
fixture. The excluded owner jobs retain their original text. No GUI windows
were opened and no owner application processes, settings or documents were
modified. No commits were made.
