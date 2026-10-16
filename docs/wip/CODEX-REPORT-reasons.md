# U023 / U029 completion report

Completed on 2026-10-13, continuing the branch's existing menu implementation, shared check gutter, keyboard profiles and retained AI Remove confirmation flow. Changes are uncommitted. Only `photocraft-ui-egui` changed as a Rust crate; no dependencies were added.

| Task point | Result and evidence |
| --- | --- |
| U023 plain-English errors | `command_feedback.rs` centralizes mapping and reporting. `PhotocraftApp::run`, main-menu invocation, vector integration reporting and the final status-bar sink use it. Ten reproduced error families cover missing document/layer/selection, empty clipboard, unavailable undo/redo, pixel-layer requirements, locked layers, `NoLayer` and `BadParams`. Returned API errors remain raw; status text is plain English and original detail is retained in the bounded operation history. |
| Details / History | Command errors enter history without adding a row or moving the dock. The status bar uses the existing shared Info icon widget with accessible name and tooltip **Details / History**. Clicking it reveals the raw error in the existing history window. File-operation notices keep their results row. The new notice field has a serde default, verified with an old-format notice. |
| U029 disabled reasons | Main-menu entries receive reasons from the shared resolver. Layers menus, the alignment popup, Paths context menus and canvas menus use the shared command-row/tooltip widgets. Preconditions cover absent documents/selections, inherited locks, Background restrictions, layer types, vector persona, vector targets, clipping/compound requirements and stale captures. Right-clicking an inactive locked layer evaluates that clicked layer without selecting or invoking it. |
| Selection AI routes | Added **Generative Fill…** and **AI Remove…**. Menu capture includes document ID/revision, active layer, complete target set, selection mask and persona. Generative Fill retains that capture through its prompt dialog and refuses retargeting before Generate. The existing engine operation is used; the mock HTTP test verifies output alpha against every captured selection pixel. |
| AI Remove confirmation | Importing the selection marks the pending mask as requiring confirmation. The real-click test enables immediate mode, chooses AI Remove from the canvas context menu, adjusts the mask and verifies zero submissions until clicking Remove, then exactly one submission with the adjusted captured mask. |
| Empty selections | Reproduced Select All → Inverse, which leaves an empty mask object. AI enablement and selection-context routing now require usable coverage inside the document, rather than only `Option::Some`. |
| Vector object menu | Move, Path Selection, Direct Selection and shape/Pen tools reach the object menu in Vector. It offers Group/Ungroup, Arrange, Move/Rotate/Scale/Reflect/Shear, Make/Release Clipping Mask, Make/Release Compound Path, Lock/Hide and Create Outlines through the existing `type.convertToShape` operation. Actual clicks exercise grouping, ungrouping, transform-dialog capture, locking and both clipping/compound make/release flows. |
| Vector Move | Auto-Select uses the vector hit/scope and preview/apply selection path. A shape with its raster cache removed remains selectable, and the test verifies no `layer.pickAt` invocation. |
| Profiles / design system | Context shortcuts come from `shortcut_label`; custom binding and removal are tested. `command_menu_item` uses the existing check gutter and egui menu style. Reasons wrap within the viewport. All new chrome uses shared widgets/theme spacing, and the widget is documented in `docs/DESIGN-SYSTEM.md`. Translation additions only append to `pending_vector.rs`. |

The source anchors are `crates/pc-ui-egui/src/command_feedback.rs`, `canvas_tool_menu.rs`, `vector_integration.rs::disabled_reason` / `pick_object`, `widgets.rs::command_menu_item` / `disabled_reason_tooltip`, `notices.rs::command_error` / `history_button`, and the focused tests in `reasons_ux_tests.rs`, `ai_ui.rs` and `ai_remove_ui/tests.rs`.

Validation used the sourced development environment, Rust 1.98.1, offline cargo, the same retained target directory and `CARGO_BUILD_JOBS=4`, with one cargo operation at a time:

| Check | Result | Receipt |
| --- | --- | --- |
| Focused `reasons_ux` tests, including ignored GPU test, one test thread | **15 passed, 0 failed** | `target/reasons-final-focused.log` |
| Existing dock geometry regression, unchanged test | **1 passed, 0 failed**; also passed in the final full suite | `target/reasons-dock-regression.log` |
| `cargo +1.98.1 clippy --offline -p photocraft-ui-egui --all-targets -- -D warnings` | **Passed** | `target/reasons-clippy.log` |
| `cargo +1.98.1 fmt -p photocraft-ui-egui --check` | **Passed** | `target/reasons-fmt.log` |
| Full `cargo +1.98.1 test --offline -p photocraft-ui-egui` | **1,739 passed, 0 failed, 76 ignored**: 1,593 unit tests and 146 integration tests; 0 doctests | `target/reasons-full-accepted.log` |
| Changed-source/document whitespace check | **Passed** | `git diff --check -- crates/pc-ui-egui docs/DESIGN-SYSTEM.md docs/wip/CODEX-REPORT-reasons.md` |

One new opt-in GPU test was added: `reasons_ux_tests::reasons_ux_rtx5090_menu_and_tooltip_screenshots`. It strictly selects the **NVIDIA GeForce RTX 5090**, Vulkan, discrete adapter, driver **615.71.09**; absence of that adapter fails the test. It passed explicitly. The other 75 ignored tests are existing opt-in cases outside this task's changes.

**30 final screenshots** are retained under `target/reasons-ux/`: dark Pro and Studio Light main-menu tooltips, panel/selection/vector menus and reasons at 250 and 400 points, plus status/history controls at 760 and 1440 points. All were visually inspected using both contact sheets (`review-dark.png`, `review-light.png`) and original images. Inspection prompted fixes for button-pill context rendering and narrow tooltip clipping; final shots have flat aligned menu rows, elided long captions and fully wrapped reasons. AI routing tests use `MockComfy`, not the owner's shared inference queue.

The first full-suite failure exposed the extra error-row dock displacement; it was corrected in the report/status sinks without editing dock sources or weakening its test. The initial failure receipt remains at `target/reasons-full-tests.log`. The suite-generated `docs/design/vector-tool-icons.png` was restored to its initially clean HEAD bytes because that asset belongs outside this task.

Established references followed:

- `docs/DESIGN-SYSTEM.md` and `docs/upstream/photocraft/ui-design.md`: shared menu widgets, tokens, check gutter and theme verification.
- Local pinned egui 0.36.2 source (`response.rs`, `containers/menu.rs`, `containers/popup.rs`): disabled-hover UI, menu style, justified menu layout and tooltip wrapping.
- Existing `toolsets_ui::persona::GestureTarget`, `shortcuts::shortcut_label` and `shortcut_dispatch::binding_strings`: delayed target authority and active keyboard profiles.
- `crates/pc-engine/src/vector_packages/selection/SOURCES.md`, `clip_compound/SOURCES.md` and `path_commands/SOURCES.md`: the established VectorCraft-derived selection, clipping and object-command operations. Reused these providers rather than adding another selection or geometry implementation.

Nothing remains for U023/U029 in this task's scope. Unlock All / Show All object operations are absent from the current registry, so the conditional menu rows were not invented; only Show All Effects exists, which is a different action. Protected dock/workspace/symbol code and the previously fixed PSD projection code were not edited. The pre-existing modified task document and untracked status document were preserved.
