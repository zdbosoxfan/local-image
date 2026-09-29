# Compact desktop workspace specification

Design review, 22 September 2026. This specification responds to the user's rejection of the current exposed controls and boxed presentation, and preference for the Photoshop-based layout at `cd77bdf`. These are developer recommendations informed by primary documentation and actual application screenshots; no human focus group was conducted.

## What went wrong

The redesign made occasional actions and explanatory text permanently compete with the photograph. It widened the tool rail from 45 to 66 px and the right panel from 240 to 288 px. Menu/options/document/status rows grew from 31/40/29/29 px to 58/49/47/37 px. At 1440 × 960, before a folder strip, this reduces the nominal canvas area by about 13%. The new repair panel also consumes roughly the upper third of the right column before Layers begins.

Screenshots reviewed: `work/browser-final/02-selection-desktop.png` and `work/setup-review-final/setup-1366.png`. The first shows an instructional headline, paragraph, repair label, method toggle, method selector, selection message, Apply, Layers, Merge, Project Save and another explanatory paragraph all displayed simultaneously. Each can be useful at a particular moment; their simultaneous prominence is the problem. Layer rows were additionally changed into separated rounded cards. The second screenshot demonstrates that even a settings dialog needs a clearer distinction between its normal connected state and installation work.

Use `cd77bdf` as the shell reference. Retain the newer implementation, functioning setup, accessibility fixes and save protection. A color change or smaller padding alone will not resolve the command hierarchy.

## Permanent workspace

| Region | Target | Visible content |
| --- | --- | --- |
| Menu bar | 28–31 px | Small app name, File, Edit, Layer, View. Preferences belongs in Edit. An additional menu is justified only by an actual command group. |
| Tool options | 34–40 px, one row | Active operation, active tool settings, selection Add/Subtract, a single Apply action. |
| Tool rail | 40–45 px | Compact 30–33 px icon buttons; active tool has a restrained selected background. Tooltips and accessible names provide labels and shortcuts. |
| Document tab | 27–30 px | Filename, unsaved marker, close. No row of saving commands. |
| Canvas | Remaining space | Photo, selection and brush cursor. No promotional content or floating controls over the image. |
| Layers | 220–240 px, full height | Flat header and contiguous layer rows with visibility, thumbnail/name and selected state. |
| Status bar | 24–28 px | One concise contextual status, zoom percentage and Fit. Backend status may occupy a short secondary text position. |

Keep menu labels visible. Do not replace the menu bar with a hamburger or an always-expanded button dashboard. Use neutral gray surfaces and thin separators between functional regions. Keep normal tools borderless until hover, focus or selection. Use 12 px interface text, 11 px metadata and 2–3 px corner radii. Preserve legible contrast, visible keyboard focus and comfortable target areas; compactness is not a reason to make text unreadable. The photo and selection should provide most of the color. One restrained blue accent for the active tool/Apply is sufficient.

## Command placement

| Command | Home | Permanent duplicate? |
| --- | --- | --- |
| Open photo, folder, project | File, followed by Recent submenu | Only Open in the empty workspace. Remove the header Open button. |
| Save project, Save project as | File project group | No Layers footer button. |
| Save a copy / Export copy | File image-output group | No document-tab buttons. Keep their different destinations explicit if both remain. |
| Overwrite original | File, separated from ordinary saving | No permanent destructive button. Preserve existing confirmation and shortcut behavior. |
| Copy format | Output dialog or File > Copy format submenu | No embedded form and explanatory paragraph inside a command menu. |
| Undo selection, Clear selection | Edit | Remove the broad permanent Undo/Clear text pair. A small contextual Clear may appear only while a selection exists. |
| Heal / AI Remove | Operation control in tool options | A single compact selector or two flat mode buttons, never another inspector section. |
| Brush size | Tool options for brush tools | Hide for rectangle, ellipse, pen and pan. A compact numeric size with optional popover is preferable to a long permanent slider. |
| Add/Subtract | Tool options for selection tools | Small adjacent controls with one selected state, no surrounding card. |
| Texture repair / Dust & scratches | Tool options when Heal is active | Do not display beside AI Remove. |
| Apply | End of tool options, stable position | One button only. Disabled until an actionable selection exists; label clearly indicates Heal or Remove. |
| Close path | Tool options while pen path is open | Hidden at other times. Enter remains available. |
| Merge visible to new layer | Layer menu and Layers panel menu | Remove the full-width persistent merge button. |
| Discard / Restore layer | Layer menu or relevant row menu | Visibility remains directly available; avoid a permanent boxed action for every row. |
| Original comparison | View menu; optional small toggle near zoom | One location in chrome at most. |
| Fit, 100%, zoom in/out | View; compact zoom control in status | No second zoom toolbar. |
| AI setup, model paths, install/download/start/eject | Preferences > Local AI | Never part of the main repair panel. |

Do not invent new commands merely to imitate Photoshop. Do not relabel Ctrl+S as project saving while it still overwrites the original. Menu labels and shortcut hints must reflect actual behavior. Saving changes are separate from this presentation correction.

## Tool and panel behavior

Six current tools can remain as compact icons: Heal, AI brush, Pen, Rectangle, Ellipse, Hand. Removing their permanent text labels provides most of the rail improvement without adding hidden-tool complexity. Group rectangle/ellipse into a flyout only if that interaction is actually implemented and keyboard-accessible; a decorative group arrow would be misleading.

The operation selector remains necessary for geometry tools: a rectangle selection must clearly indicate whether Apply will heal or use AI. Switching between Heal and AI must not discard the selection. Pan should suppress irrelevant options. The active tool name may be short text in the options row, so icon-only tools remain understandable.

Layers should behave as a list, with thin row dividers and a single full-row highlight. Remove per-row rounded borders, ornamental whitespace, tutorials, project-save promotion and the separate repair area. Keep Original locked and visibility obvious. If a selected-layer delete command is introduced, make row selection explicit before removing existing discard affordances.

At 800 × 560, keep the options row and Apply visible. Move secondary options into a popover; do not introduce a second tall toolbar row or an inspector that pushes Apply below the fold. A collapsible Layers panel is useful, but full docking/custom-workspace machinery would be disproportionate. Show the folder strip only for an actual multi-image set, and allow View to collapse it. A single photo does not need a separate folder-name row or 1/1 navigation.

## Empty, offline and setup states

The empty canvas needs an understated Open button and a short drop hint. Remove the illustration, slogan, privacy badge and repeated instructions from the permanent workspace. No photo means a quiet empty Layers panel and disabled editing controls.

AI unavailable is a compact status such as “AI offline” with an accessible route to Local AI settings. Selecting AI may expose one contextual Setup action in the existing options position; it should not create a tutorial panel. Errors need useful recovery detail in status or the setup dialog, not an unexplained disabled Apply.

In Preferences, the normal connected view should show the current runtime, model readiness and GPU state in plain rows. Installation, download details, file lists, path management and diagnostic fields belong behind “Change…” or an expanded setup section. An active installation exposes its progress and error in the dialog until resolved. No capabilities should be deleted to make the page look simpler.

## Acceptance checks

1. With a photo open and no selection, one tool rail, one contextual options row, one document tab and one Layers list are visible. No permanent tutorial or save/merge section remains.
2. Brush, pen, shape and hand choices show only relevant options. Method and Apply remain unambiguous after each switch.
3. File commands are discoverable through menus, and their existing shortcuts and unsaved-change protections still work. Menus dismiss with Escape/outside click and support arrow-key navigation and focus return.
4. At 1366 × 768 and 800 × 560, no editing action requires scrolling a sidebar. Test the real photograph view, not only the empty canvas.
5. Compare screenshots directly against `cd77bdf`: the new version should restore its compact work-area allocation while improving consistency and preserving the newer functionality.

## Primary references and interpretation

- [Adobe Photoshop desktop workspace overview](https://helpx.adobe.com/photoshop/desktop/get-started/learn-the-basics/workspace-overview.html), updated June 2026: separates menus, tools, document, panels and the active tool's options. The recommendation here uses that division of responsibilities. Photoshop's additional floating contextual bar is not needed when this small app already has a fixed options row.
- [Adobe: Arrange and group panels](https://helpx.adobe.com/photoshop/desktop/get-started/learn-the-basics/manipulate-panel-groups.html): panels can be organized for a particular workflow. Local Remove principally needs Layers; the presence of many Photoshop panels is not a reason to invent several here.
- [Microsoft: Menus and context menus](https://learn.microsoft.com/en-us/windows/apps/develop/ui/controls/menus-and-context-menus): menus organize commands while keeping them hidden until needed; frequent commands may be direct controls. This supports moving occasional save, merge and setup actions out of the workspace.
- [Microsoft: Command bar](https://learn.microsoft.com/en-us/windows/apps/develop/ui/controls/command-bar): separates primary commands from secondary overflow. Keep Apply and active-tool adjustments direct; overflow secondary options when width becomes constrained.
- [Microsoft desktop toolbar guidance](https://learn.microsoft.com/en-us/windows/win32/uxguide/cmd-toolbars): explicitly prioritizes frequent, immediate, recognizable commands and efficient work-area use. This is historical Windows 7 guidance, used for command organization rather than copied visual styling. It supports compact familiar icons, relevant tool options, and avoiding destructive toolbar neighbors.
- [Microsoft desktop menu guidance](https://learn.microsoft.com/en-us/windows/win32/uxguide/cmd-menus): familiar menu categories, task-specific labels, grouped commands and meaningful ellipses. This is likewise historical guidance; it supports the menu structure and clear saving distinctions rather than exact legacy typography.
