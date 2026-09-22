# Local Remove — second-cycle usability and visual review

Historical 0.3.0 review. The current compact workspace is documented in [DESKTOP-REFERENCES.md](DESKTOP-REFERENCES.md) and [DESKTOP-VALIDATION.md](DESKTOP-VALIDATION.md).

Reviewed 22 September 2026. This is an expert heuristic review using simulated roles, source inspection, actual browser screenshots, and the implementation team's automated checks. No participants or real focus groups were involved. The visual review covers `browser-cycle-1`, `browser-cycle-2`, and `browser-cycle-3` screenshots, including 1440px desktop, 1100px laptop, 800×560 minimum native viewport, and additional 560px/430px browser layouts.

## Result

The revised editor now has a coherent, restrained photo-workstation design. Neutral charcoal surfaces keep the photograph central; the warm accent identifies selection state and the main action. Tools have labels, selection controls form one band, and repair choice/readiness/action share an inspector. Saving a flattened copy is visually separated from saving editable layers. There is no decorative dashboard, gradient branding, ornamental statistics, or unsupported claim about the editing technology.

No remaining blocking visual issue was found in cycle 2/3 screenshots. Source and automated evidence support the four requested simulated-role scenarios below. This is not a full accessibility certification, a test of AI image quality, or validation with real users.

## Findings resolved during the review

1. **Theme failed in first browser capture.** Large canvas and inspector areas were white, type fell back to Times, and several labels became unreadable. Component rules applied, but `:root` tokens did not. The implementation team traced this to a UTF-8 byte-order mark before the embedded stylesheet selector. The source was fixed; the renderer now reads UTF-8 with BOM support and has regression coverage. Actual cycle 2/3 screenshots show the intended dark palette and sans-serif typography.
2. **Space intercepted button activation.** The global canvas shortcut handler consumed Space on ordinary focused buttons. Root excluded interactive targets and added keyboard/browser checks. The review specifically requested that Space activate the focused AI-mode button without entering pan state.
3. **Short-window inspector clipped layers and saving.** The first layout used a fixed repair section plus an overflow-hidden sidebar. At the minimum native height, lower actions were outside the available height. The inspector now scrolls at short desktop heights; tests verify project saving can be scrolled into view. The repair action remains visible at 800×560.
4. **Extra-narrow browser layouts hid the repair action.** Long guidance crowded the 220px bottom inspector. The compact layout now hides secondary prose, retains the state heading, and shows method choice and the repair button. The 430px and 560px screenshots confirm this.
5. **Potential brush/method confusion.** The B tool changes to AI removal, whereas J selects Quick Heal. The revised rail labels these “AI brush” and “Heal,” preserving the established shortcut behavior without implying a shared method-neutral brush.
6. **Save terminology diverged.** The document bar used “Save a copy” while legacy menus and dialogs retained “Save Unique.” Root aligned visible strings and generated hints while retaining implementation IDs.
7. **Guidance could become stale after abandoning a pen path.** Root refreshed derived state on tool changes and made guidance match the selected tool. Individual healing-method availability is also respected.

## Simulated role outcomes

| Simulated role | Review outcome | Evidence and boundary |
| --- | --- | --- |
| First-time photographer | The opening action is obvious; Quick Heal starts without AI setup; a selected area produces an explicit ready state and becomes an editable layer. Copy export and project saving have distinct controls. | Empty, selected, and repaired desktop screenshots; real texture repair, export, and project checks reported passing. Fixture is synthetic and does not establish photographic output quality. |
| Keyboard user | Focused controls retain native Space/Enter activation, canvas shortcuts remain available, selection Undo is labeled accurately, comparison has a shortcut and persistent toggle, settings closes with Escape. | Source inspection plus added regression/browser coverage. No screen-reader session or complete keyboard accessibility audit was performed. |
| Narrow-laptop user | Repair action stays inside the 800×560 viewport; lower project/layer actions remain reachable through scrolling. Additional 430/560 layouts retain canvas, method, repair and save controls. | Actual screenshots; root reports action vertical bounds and project-save reachability assertions passing. At short height, editing layers requires scrolling; this is acceptable for the supported minimum size. |
| User without local AI running | Unavailable AI has a dedicated connection/settings action, cannot submit a repair, and does not discard the existing selection. Switching back to Quick Heal restores a usable workflow. | Actual state derivation and browser assertions. No successful AI generation was attempted; only offline recovery behavior is covered. |

## Final polish and evidence note

An early selected-state screenshot captured the repair button during its color
transition. Final captures now disable animations and transitions; the final
selected-state image was inspected and shows the warm, enabled repair action.
The browser suite also checks the computed theme tokens and font family.

Use the empty editor screenshot as the general visual preview. Keep the green synthetic-texture captures as test evidence; they intentionally test image loading, selection and repair without using private photographs.

For future evaluation with actual participants, run the scenarios in the first design review without guidance and record task completion, mistaken save choices, brush/method confusion, recovery from unavailable AI, and ability to compare/discard a result. No participant completion rates or preference claims can be inferred from these simulated reviews.
