# U028 — Vector workspace and dockable panels

Implemented on the existing branch/worktree; no commit or installation. The existing dock, feature adapters, undo transactions and persona router were retained.

## Completed

- **Dockable panels and persistence:** Appearance, Graphic Styles, Document Palette, Frame Composition, Mesh, Snapping Manager, Transform, Align, Layout/constraints, Pathfinder, Brushes and the import controls use ordinary `dock::Tab` content. Symbols remains an ordinary tab. Moving, grouping, tearing off, floating geometry, active tabs, group heights and individual feature visibility use the serialized dock layout. The old runtime window set is only a derived Window-menu check cache. Optional feature tabs do not appear in old Pixel layouts.
- **Vector workspace:** Window > Workspace > Vector selects the Vector persona and arranges Color/Swatches/Document Palette, Appearance/Transform/Align/Layout/Pathfinder/Brushes/Graphic Styles, and Layers/Symbols. Specialized panels remain reachable from Window and the existing dock overflow menu. The preset gives object controls sufficient space and works in Pro and Studio Light. Pixel's automatic Properties reveal is limited to Pixel, so Vector selections preserve the chosen studio instead of opening the floating Pixel card over the canvas.
- **Independent layouts:** `Preferences::persona_layouts` stores the outgoing Pixel/Vector layout before restoring the incoming one. This also handles automation, restart, custom layouts, and saved workspaces that include a toolbar/persona. Old preferences and documents still load through serde defaults. Existing Remember Workspace Changes, workspace locking, selection and tool/preview cancellation remain in place.
- **Symbols:** Create Symbol needs only a selection. The engine generates the first available `Symbol N` name; the existing name field and Rename Symbol edit it afterwards. Explicit automation names remain supported. Master paint uses the reusable `widgets::color_control`, which opens the ordinary shared Color Picker, with hex/RGB/HSB/Lab/CMYK and opacity. Opening/applying without editing preserves original HDR paint. Master edits still use detached previews and one undoable publication.
- **Unlock All / Show All:** registered engine commands `vector.object.unlockAll` and `vector.object.showAll` recover locks/visibility throughout the active document's ordinary object tree, including nested groups and placed image layers. Each action is one undo transaction and preserves selection. Object menu rows use actual command eligibility and effective shortcut-profile bindings. Illustrator-profile defaults are Cmd+Alt+Shift+2/3 because Cmd+Alt+2/3 are immutable global Develop/Editor navigation. Affinity-profile defaults follow its documented Unlock All / Show All chords: Cmd+Alt+Shift+L/H on Linux/Windows, Ctrl+Cmd+L/H on macOS. Custom profile overrides are exercised through real keyboard input.
- **Clipping:** verified the existing Release Clipping Mask and Release Compound Path menu paths are visible/enabled for captured eligible objects and invoke the existing release operations. No clipping/compound implementation was replaced.
- **Shared UI:** documented the colour widget in `docs/DESIGN-SYSTEM.md`; reused shared buttons, checkboxes, numeric fields and sliders in the changed panel controls. Appearance action rows wrap at narrow widths. Shared dock tab strips expose their full translated accessible names even when visible labels are shortened; their “panel tab” qualifier distinguishes them from dialog controls. Shared slider rows name both the slider and its numeric value. Tab-hidden panel maintenance is retained.

## Validation

Rust 1.98.1, offline, the supplied `claude-env.sh`, required craft fonts, one cargo build lane, four build jobs, one retained worktree `target/`. Full owning-crate suites ran once; failures were corrected and rechecked only in the affected tests, as required by the testing budget.

Development receipts in `target/` retain both the initial failures and their focused corrections. The interaction fixtures were updated to install the shared font families, to use the actual post-release object IDs, to write the active custom shortcut profile, and to distinguish intentional persona switching from accidental dock changes. Assertions on document pixels, preview capture, selection and undo were retained.

The added physical test is `workspace_ui::vectorws_tests::vectorws_rtx5090_workspace_screens_and_interactive_frames`: strict NVIDIA GeForce RTX 5090 / Vulkan / discrete-adapter selection, full PhotocraftApp UI and GPU canvas, real panel clicks, GPU render/readback, and eight screenshots. It is explicitly run with `--ignored --nocapture --test-threads=1`; it cannot pass through a skipped/software adapter. This is an offscreen app test, with no desktop windows opened.

### Final receipts

| Check | Result | Receipt |
| --- | --- | --- |
| Engine full suite (all targets and doc tests) | **2,106 passed, 0 failed, 33 existing ignored** | `target/vectorws-engine-full.log` |
| UI full suite, once | 1,722 passed, 8 failed, 76 ignored; all eight failures corrected below | `target/vectorws-ui-full.log` |
| Final dock tests | **30 passed** (includes drag/dock/restart, custom persona restoration, saved toolbar/persona, and legacy preferences) | `target/vectorws-dock-final.log` |
| Final Symbols controls | **4 passed** (auto naming/rename, shared picker/master preview, place/detach/undo, cancellation) | `target/vectorws-symbols-complete.log` |
| Tab-hidden panel tests | **4 passed**, including themes and fractional/2x scales | `target/vectorws-visibility-final.log` |
| Layout UI interaction target | **8 passed** | `target/vectorws-layout-2.log` |
| Final Appearance target, including real RTX renderer | **11 passed** | `target/vectorws-appearance-final.log` |
| Updated Transform/layout GPU fixtures | **4 passed, 1 existing ignored** | `target/vectorws-gpu-layout.log` |
| Final Affinity/SVG warning-selection GPU checks | **2 passed** | `target/vectorws-imports-final.log` |
| Updated Symbols full-app GPU colour/zoom/detach/undo check | **1 passed** | `target/vectorws-gpu-symbols-final.log` |
| Camera Raw label/localization/zoom checks | **10 passed** | `target/vectorws-camera-raw.log` |
| New workspace physical GPU gate | **1 passed** | `target/vectorws-gpu-candidate.log` |
| Strict clippy, both changed crates, all targets, `-D warnings` | **Passed** | `target/vectorws-clippy-final-candidate.log` |
| Format check, both changed crates | **Passed** | `target/vectorws-fmt-candidate.log` |

The UI sweep's eight failures were three old `layout.open` assertions, one removed `Master red` interaction, two clipped import-warning hits, and two duplicate Camera Raw `Color` queries. The fixtures now check persisted dock visibility, use the shared picker, and scroll the actual warning controls into view. Production fixes reserve room for optional Vector groups, qualify accessible dock-tab names, and make Symbols' disabled shared buttons wrap correctly. Every initially failing test has a passing focused receipt; there are **no remaining known test failures**. The full UI suite was not repeated, respecting the owner's testing budget. Existing unrelated ignored acceptance/performance tests remain for the coordinator; the new ignored U028 physical gate was explicitly run and passed.

### Screenshots and interactive receipt

All eight final PNGs under `target/vectorws-ux/` were opened and inspected: dark/Studio Light, 1440×900/1024×700, dock widths 250/400 points. Naming: `vector-{dark|light}-{1440x900|1024x700}-dock{250|400}.png`. The new panels remain docked in light themes, the canvas has no automatically opened Pixel Properties card, action rows wrap, and overflow tabs remain reachable. Longer panel content uses the existing vertical scroll area. The clipped Appearance opacity caption discovered during inspection now uses the shared labelled slider row.

Hardware: **NVIDIA GeForce RTX 5090**, discrete, **Vulkan**, NVIDIA driver **615.71.09**. One warmed full-app sample of two real panel clicks, six UI frames and two GPU render/readbacks took **88.99 ms**. This includes screenshot/readback overhead and is not a desktop frame-time claim or a before/after speed comparison.

No new dependencies, unsafe code, desktop windows, commits, installed binaries or owner-file changes outside the worktree. The UI suite's incidental regeneration of `docs/design/vector-tool-icons.png` was restored to its task-intake bytes; accepted U028 images remain in `target/vectorws-ux/`. Protected menu/error-sink files were untouched; existing `vector_integration::menu_items` changes are limited to the two new recovery rows and their metadata. The provided task/checkpoint documents were left as found.

## Remaining

No U028 implementation work remains. Broader native release acceptance, unrelated ignored GPU/performance gates, translations, macOS/Windows execution, publishing and installation remain outside this task. This task deliberately used the required offscreen full-app acceptance path and did not open or replace the owner's running app.

## References followed

- Existing repository implementations: `dock.rs`, `dock_float.rs`, `workspace_ui.rs`, `vector_packages/persona.rs`, Symbols' captured master transactions, `color_picker_ui.rs`, `widgets.rs`, and the existing clipping adapter/engine operations.
- Project design system and inherited PhotoCraft rules: `docs/DESIGN-SYSTEM.md`, `docs/upstream/photocraft/ui-design.md`.
- [Affinity Designer's per-persona workspace](https://s3-eu-west-1.amazonaws.com/affinity-docs/help/designer/en-US.lproj/pages/Workspace/atAglance.html), and [Studio customization](https://www.affinity.studio/help/workspace-about-studios/): separate studio layouts, panel visibility/grouping/positioning.
- [Affinity Designer 2 keyboard shortcuts](https://affinity.help/designer2/English.lproj/pages/Workspace/shortcuts.html): Object control Unlock All/Show All; platform modifier tags were checked in the official HTML.
- [Adobe's grouping, hiding and locking workflow](https://www.adobe.com/learn/illustrator/web/lock-group-hide-content): Object > Unlock All and Object > Show All.
- Installed egui_kittest 0.36.2 `HarnessBuilder::build_eframe` and WGPU setup, following the repository's strict physical GPU screenshot harnesses.

The exact Vector grouping and the extra Shift in the Illustrator recovery shortcuts are local choices, using the existing single-column dock and preserving this app's immutable global workspace chords.
