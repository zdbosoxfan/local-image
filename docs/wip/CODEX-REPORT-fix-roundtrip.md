# Compositing → Library round trip

Completed the task by continuing the branch's existing Develop-layer bridge, worker-rendered
`unsaved_composites`, PSD save notifications, import/stack logic and headless tests. No commit was made.

## 1. Headless reproduction and coverage

Added a host harness using Library's `HeadlessView` and an eframe test frame. It drives the real
`Host::logic` and `Host::ui`, imports a PNG into a temporary on-disk library, opens it through the
module shortcut, adds and fills a pixel layer, and returns through the module shortcut. It checks
painted grid and loupe pixels against the original, the unsaved label and the notice. The real
Compositing ⌘S shortcut then writes `photo-Edit.psd`; the test verifies import, stack order, selection,
saved grid/loupe pixels and the destination/stack notice.

With the old loupe drawing code temporarily restored, the CPU test reproduced the problem:
the grid showed the red composite, but the loupe showed the original (`#75743C`) and failed its
pixel assertion. The fixed code passes.

Additional tests actually click Save, Keep Editing and Dismiss. They cover a failed write and
retry, saving the notice's document when another tab is active, saving an existing edit from
Library with ⌘S, one-layer edits, and removal of previews after save or close. The existing Library
headless test now checks painted pixels across every Before/After layout and still clicks the
button back to Compositing.

## 2. Fixes and notices

- A normal return to Library after saving selected the Develop layer's original photo again.
  Library now selects the document's saved catalog file first; Develop continues following the
  Develop layer's source. Save and Return retains its existing behavior.
- Remembered Before/After layouts, the Before key and hover previews could supersede the
  unsaved composite in the loupe. Library now gives the composite priority and suppresses split
  overlays over it. Develop keeps displaying the source photo.
- The preview check excluded all documents with just one Develop layer, including edited ones.
  It now uses the document's dirty state, so one-layer edits also reach the worker renderer.
- Returning with unsaved work shows one non-modal card with Save, Keep Editing and Dismiss.
  Save uses the existing PSD save/import/stack path. Library's ⌘S also saves its selected preview.
- Successful saves post a persistent notice in Compositing and a Library card containing the
  destination and add/stack or reload result. Failed writes preserve the unsaved edit and allow
  retry; failed Library imports report the saved destination.
- Host UI tests skip preference writes so they cannot overwrite the owner's module settings.

The implementation changes are in `apps/local-image/src/library_host.rs` and
`crates/lc-ui-egui/src/panels/detail.rs`; Library test coverage is in
`crates/lc-ui-egui/src/tests_host_composite.rs`. No dependencies, unsafe code, C/C++ code or `-sys`
crates were added. No persisted schema changed, so old settings and documents retain their loading
behavior.

## 3. Validation

All Cargo commands used `cargo +1.98.1`, offline builds and `CARGO_BUILD_JOBS=3`, with one build
running at a time. Formatting used `cargo +1.98.1 fmt` for both changed crates. Sandbox test runs
used `LIGHTCRAFT_GPU=0` after the installed Vulkan driver crashed during device-less enumeration.

| Check | Result |
| --- | --- |
| `test --offline -p local-image library_host::tests` | 10 passed, including the GPU test's explicit skip |
| `test --offline -p local-image roundtrip_notice_actions_keep_editing_retry_save_and_dismiss` | 1 passed, including the other-tab case |
| `test --offline -p lightcraft-ui-egui tests_host_composite` | 1 passed |
| `test --offline -p photocraft-ui-egui develop_` | 6 passed: existing Develop-layer and Camera Raw host/filter tests |
| `test --offline -p photocraft-ui-egui camera_raw_ui::tests` | 4 passed: existing Camera Raw UI tests |
| `check --offline -p local-image -p lightcraft-ui-egui` | Passed |
| `clippy --offline -p local-image -p lightcraft-ui-egui --all-targets -- -D warnings` with `CARGO_TARGET_DIR=target/clippy` | Passed |
| Final full `test --offline -p local-image -p lightcraft-ui-egui`, run once | Local Image: 71 passed; Library UI: 203 passed, 1 existing ignored test; doc-tests: 0 |
| `git diff --check` | Passed |

The ignored test is the existing `tests_masking::grid_frame_100k` scale benchmark. Full-suite test
execution took 138.48 seconds for Library and 0.34 seconds for Local Image. The existing build
warning about missing embedded craft-fonts remains unchanged.

## GPU test and remaining verification

Added `library_host::tests::roundtrip_gpu_grid_loupe_and_save_shortcut_show_the_edited_pixels`.
On hardware it installs the editor's real wgpu canvas, reads back the filled layer's canvas texels,
then exercises the same grid/loupe/save/import/stack assertions as the CPU test. On Linux with
no GPU devices it skips before loading drivers; it also skips when enumeration finds no hardware
adapter. Here it printed `skipped: no GPU adapter for Compositing → Library round trip` and passed.

No implementation work remains. GPU execution is left to the coordinator's RTX 5090, as required
by the task. Run the GPU test by name with `-- --nocapture`, without the sandbox's
`LIGHTCRAFT_GPU=0` override. No GUI windows were opened and the running desktop app was untouched.

## English UI strings

New or revised strings (placeholders denote runtime values):

- `The Library shows a preview of your layers. Save to keep this edit; your original photo is preserved.`
- `Save as {destination} and stack it with the original in the Library.`
- `Keep Editing`
- `Saved {path}; added to the Library, stacked with {name}`
- `Saved {path}; updated {name} in the Library`
- `Saved {path}, but the Library couldn't add it: {error}`
- `Couldn't save the Compositing edit: {error}`

The card reuses the existing `Edited in Compositing · unsaved`, `Save` and `Dismiss` text.
Translations were left for the separate translation work.
