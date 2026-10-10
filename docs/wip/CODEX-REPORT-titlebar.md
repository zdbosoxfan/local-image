# Title-bar dragging report

Continued the previous builder's `render_titlebars_probe` and existing host harness. Changes are uncommitted.

## Diagnosis and fix

The module switch's `with_layout(left_to_right(...))` retained the whole right-to-left parent rectangle. Its buttons started at the left of that rectangle, while its frame stretched to the right. This consumed the free drag span in Library, Develop and Compositing. The new host regression failed before the fix with a **zero-width** span at 1800×1000 in Pro.

On custom title bars, the switch now allocates the measured width of its three translated button labels, padding and spacing before changing layout direction. The buttons sit together beside the right-hand controls, leaving room for the window title and dragging. The native-title-bar path keeps its original layout; macOS window settings and traffic-light spacing are unchanged.

The broad Pro Medium module strip reproduces the reported dead grey space. The exact absence of a brand mark and caption buttons from the owner's description was not reproduced in this branch: those controls already existed. They remain visible and clickable in the new renders and interaction tests.

## Renders and interaction coverage

- Saved **45 full-window PNGs** under `target/titlebar/`: 1800×1000, 1280×800 and 2560×1440 × Library, Develop and Compositing × Pro, Pro Medium, Studio, Studio Light and Classic. Saved the corresponding 45 pre-fix renders under `target/titlebar/before/`, along with the builder's original cropped probes.
- Inspected every post-fix title bar using the nine `overview-<Module>-<width>x<height>.png` sheets, plus full-window and pre-fix examples. Each sheet's rows follow the theme order above. Captions are flush with the top/right edges; no panel sits above the title bar. Heights are 32 logical pixels in Pro themes and 38 in the other themes.
- Library/Develop: the real host and `HeadlessView` test **30 combinations**, including the CPU-status pill and unsaved Compositing work. Each checks a substantial free span, three real pointer drags emitting exactly one `StartDrag` apiece, double-click maximize, all three caption actions and top-edge `BeginResize(North)` without `StartDrag`.
- Compositing: `egui_kittest` drives the real app in **15 combinations** with the host mode switch enabled. It checks the same drag/caption/resize behavior, double-click restore, menu/module drag attempts that must not move the window, and a click on Library that requests the module change.
- Extended the probe to save the entire window, use the actual selected theme/preferences, retain Library CPU textures and provide overview sheets. It is explicitly ignored in routine suites because it writes visual-QA artifacts; it was run successfully for this task.

## Verification

All builds, lints and tests use `cargo +1.98.1`, `--offline`, `CARGO_BUILD_JOBS=3` and `PATH=~/.cargo/bin:$PATH`; only one cargo build is active at a time. Both changed crates were formatted. The two new interaction tests and the explicit render probe pass.

Final checks, with each changed crate's full suite run once:

- `CARGO_TARGET_DIR=target/clippy cargo +1.98.1 clippy --offline -p photocraft-ui-egui --all-targets -- -D warnings`: **passed**.
- The same clippy command for `local-image`: **passed**.
- `cargo +1.98.1 test --offline -p local-image -- --test-threads=3 --nocapture`: **79 passed, 0 failed, 1 ignored** (5.71 s). The ignored screenshot probe was run explicitly and passed, generating all 45 post-fix PNGs.
- `cargo +1.98.1 test --offline -p photocraft-ui-egui -- --test-threads=3 --nocapture`: **962 unit tests passed**, 4 ignored; **54 GPU integration tests passed**, 3 ignored; **14 CPU integration tests passed**; 0 doctests. **0 failures**. Unit/GPU/CPU suite times: 28.41 / 49.70 / 5.02 s.
- Total full-suite passes: **1,109**. Existing ignored tests are benchmarks and an unrelated tool-icon artifact generator. One existing craft-fonts coverage check skips internally because those fonts are not embedded; this does not affect the English title-bar renders.
- `git diff --check`: **passed**.

Logs: `target/titlebar/clippy-photocraft-ui-egui.log`, `clippy-local-image.log`, `tests-local-image.log` and `tests-photocraft-ui-egui.log`.

GPU verification ran on **NVIDIA GeForce RTX 5090, Vulkan, DiscreteGpu, driver 615.71.09**. Tests used `VK_DRIVER_FILES=/usr/share/vulkan/icd.d/nvidia_icd.x86_64.json`, `WGPU_BACKEND=vulkan` and `WGPU_ADAPTER_NAME=NVIDIA GeForce RTX 5090`. The round-trip test and GPU integration suite printed the real adapter; there were **no GPU-adapter/capability skips or llvmpipe results**. Existing GPU timing output in the suite log also comes from this adapter. Test settings/data/cache paths were redirected under `target/titlebar/`.

No GPU code or new GPU-specific test was needed for this egui layout change. The existing Compositing → Library GPU round-trip test now prints its selected adapter and asserts that it is a discrete/integrated GPU.

## Remaining work / limits

No implementation work remains. Actual KDE window movement is not exercised because the task prohibits opening GUI windows; the tests verify egui's real native-window commands. The owner's running app and ComfyUI queue have not been modified. No dependencies, unsafe code, installs, downloads, settings formats or saved-document formats were changed. Nothing was committed.
