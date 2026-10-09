# Rich tool tips and named tool sets

Continued the committed stopped-builder WIP on this branch. No commits, dependencies, or GUI windows were created.

## A. Tool tips

- Compositing: exhaustive text for all 53 toolbar tools, all 11 Liquify strip tools, and Camera Raw Zoom, Hand, and Color Sampler. Flyout entries have accessible names and effective shortcut labels; tips are suppressed while a flyout is open.
- Library/Develop: one table covering the strip, all 11 mask-creation tiles, removal modes, red/pet-eye modes, straightening/guided tools, white-balance, targeted adjustment, Point Color and film-base pickers.
- Rich cards use the theme tokens, a 32-point icon slot, a shortcut chip when bound, descriptions, 2–4 verified usage lines, and sibling names for tool groups. Width is bounded to about 320 points; the first hover waits 0.5 seconds, neighboring tools open immediately, and pointer holds/popups suppress cards.
- Rich/Simple/Off preferences default to Rich and survive old settings and JSON round trips. Existing global tooltip-disable preferences still apply in Compositing.
- Compositing's toolbar groups and modal tools now share their shortcut lookup with their key handlers and shortcut editor, including reassignment/unbinding. Library uses its existing UI/engine command lookup (Library has no shortcut-remapping preference).
- Corrected WIP claims against source code, including History Brush's opening-state source, Pen gestures, Crop versus Straighten, and AI Remove's actual Remove button.

## B. Tool sets

- Read-only All Tools, Photographer, Essentials, Retouching, and AI templates; All Tools is the default. AI includes the actual learned-model tools, including Quick Selection's optional Subject Assist.
- Toolbar name switcher and Window > Tool Set menu. Edit Toolbar supports New from current, Duplicate, Rename, Delete custom, Save as New Set, reset, checkboxes, and Up/Down ordering. Cancel reverts the toolbar preview; OK keeps it.
- `toolset.list/select/save/rename/delete/reset` plus `toolset.duplicate`, persisted with Pixel persona defaults. Invalid tool arrays are rejected without changing preferences; empty custom layouts are allowed; deleting the active set falls back to All Tools. Reset keeps custom sets.
- Non-default legacy hidden/order preferences migrate to My Tools, preserving both membership and order. A canonical/default legacy order stays on All Tools. Hidden tools remain reachable by their effective key and group cycling.
- Toolbar order applies to groups and their flyout members; canonical slot IDs preserve last-used tools across set switches.

## Validation

Pending build, filtered tests, final changed-crate suites and clippy; counts will be filled from their results.

## GPU coverage and remaining work

No GPU code changed and no GPU tests were added. The new interaction tests use egui_kittest or Library's windowless CPU harness. GPU execution remains the coordinator's responsibility on the RTX 5090. New Compositing translation entries are only the literals required by the existing translation-coverage test; new Library text may fall back to English.
