# Local Remove — design and usability review

Historical 0.3.0 proposal. The user rejected this expanded layout; [DESKTOP-REFERENCES.md](DESKTOP-REFERENCES.md) defines the current compact desktop direction.

Reviewed 22 September 2026. Evidence: current editor HTML, README, desktop host documentation, regression-test structure, and primary sources below. This is an expert heuristic review with simulated user roles; no real focus group or observed-user research has been conducted.

## Direction

Build a quiet photo workstation. The photograph owns the center; a narrow tool rail supports selection, and one inspector groups the removal method, action, and editable result layers. Neutral charcoal surfaces, clear type, precise dividers, and a restrained warm accent are appropriate. Avoid marketing headings in the working editor, purple gradients, decorative cards, floating ornaments, glowing controls, unnecessary badges, and claimed intelligence. The interface should explain the next concrete action rather than advertise the technology.

The established structure is sound. The current implementation's weaknesses are tiny type, compressed controls, equal emphasis on unrelated commands, AI availability dominating first use, and operation settings scattered across the screen. This is a hierarchy and flow problem, not an opportunity to add a dashboard.

Suggested design tokens (design recommendation, not a cited standard): canvas `#191b1b`; panel `#222526`; raised control `#2b2f30`; divider `#414647`; text `#edf0ed`; secondary text `#a8b0af`; action `#e1b981` with `#251d13` text. Calculated contrast: primary text/panel 13.44:1, secondary text/panel 6.98:1, action text/action background 9.07:1. Color decisions around the photograph should remain neutral. Use 13px reading text, 11–12px supporting metadata, 20px line icons, 32–36px control targets, 6px control radii, 16–20px panel padding, and a 4px spacing rhythm. These are starting values for browser inspection, not a blanket conformance claim.

## Implementation priorities

1. **Clarify the central loop.** Open photo → mark distraction → choose repair → apply → compare → save. Empty state: short heading, one Open photo button, secondary Open folder, accepted formats and a one-line local-processing statement. Keep restoration of recent sessions available. Do not make the landing state look blocked because ComfyUI is absent.
2. **Group decisions at the point of action.** Keep selection size/Add/Subtract near the selected tool, and place method choice, method explanation, readiness and the Apply button together. Quick Heal: “Small distractions and texture. Runs on this PC.” AI Remove: “Larger objects and complex backgrounds.” Expose the selected healing algorithm only when Quick Heal is active. Model details belong beneath AI choice or in settings.
3. **Make a useful default.** Quick Heal works without a model download and is appropriate for initial use. Consider defaulting to it, or choosing the available mode during initialization. Never silently change a mode after a user explicitly selects it. If AI is unavailable, explain the remedy beside its controls and keep healing usable.
4. **Preserve semantics of reversal.** Ctrl+Z currently undoes only selection strokes. A visible command must say “Undo selection.” Applied results are reversed through layer visibility/discard/restore. Do not imply full edit history unless implemented. Make the base Original row unmistakable and differentiate a visible layer from a selected tool.
5. **Clarify saving.** Prefer “Save copy” as the common image-saving action. “Save project” retains editable layers; “Overwrite original…” is a separate explicit action. Current Save Overwrite/Save Unique/Export buttons compete visually and use terminology that does not explain outcome. Pending selection must not look like a saved repair. Preserve Capture One's overwrite workflow and native/browser differences.
6. **Make comparison self-explanatory.** Persistent “Show original”/“Show edited” labeling should reflect current state. While showing the original, indicate that state on the image and suppress editing consistently. A checkbox/toggle provides a discoverable alternative to any hold gesture.
7. **Keep error recovery local.** Busy state names the running operation, locks conflicting actions, and preserves viewing/navigation when safe. An unavailable engine names the specific remedy without replacing general app readiness. Successful application should direct attention to the new layer; exports should identify success without covering the image with a large modal.
8. **Improve legibility and resilience.** Replace critical 9–10px labels and 22px controls. At narrow desktop widths, collapse secondary labels/commands before compressing primary targets. Keep Remove/Heal, comparison and save reachable. Avoid horizontal clipping in the inspector. Ensure text survives 200% zoom and long file/model names.

## Interaction and accessibility checks

- All enabled controls need a visible keyboard focus state, descriptive accessible names, and correct pressed/expanded state. Icon-only tools need tooltips containing their label and shortcut. Keep ordinary buttons tabbable unless a complete toolbar arrow-key pattern is implemented.
- Native ranges already offer keyboard adjustment. Preserve numeric brush-size feedback and keyboard shortcuts. Offer explicit zoom controls and Fit, rather than requiring wheel/pinch gestures. Pen selection offers a click-based option for many selection tasks. These affordances improve access but do not establish full accessibility of pixel editing.
- Target 32px or larger desktop controls. WCAG 2.2's minimum is 24×24 CSS pixels, subject to specified exceptions. Small legacy close/zoom/folder/layer controls deserve special scrutiny.
- Treat color as supplemental: selected tool gets outline/fill and pressed state; modified file gets words; unavailable engine gets text. Text contrast requires 4.5:1 for normal text, and meaningful non-text controls need separate checks.
- Native dialog focus trapping is useful; verify initial focus, Escape, and focus return. Menus require keyboard checks. Avoid claiming a WCAG audit from visual review alone.

## Simulated role reviews and acceptance scenarios

These are hypotheses and test scenarios, not participant feedback or pass results.

| Simulated role | Task | Likely current friction | Acceptance evidence |
| --- | --- | --- | --- |
| First-time home photographer | Remove a blemish without installing models | “Connecting to GPU” and AI-first state imply the app is unusable | Open a fixture, see usable Quick Heal, paint, apply and save a copy with no AI setup |
| Frequent Capture One editor | Clean a rendered TIFF and return it | Three similarly prominent save commands and small controls | Open through native handoff, heal twice, compare original, overwrite deliberately and retain file format |
| Careful portrait retoucher | Refine a mask without damaging previous repairs | Undo selection versus undo result is ambiguous | Add/subtract, undo stroke, apply, hide/discard/restore result; original remains intact |
| Keyboard user | Navigate common commands and inspect result | Icon tools, focus order and modal returns need checking | Open, switch tool/mode, change size, fit/zoom, open/close settings and compare with visible focus |
| Laptop user | Edit at 1280×720 and high display scaling | Stacked bars consume space; tiny controls encourage misclicks | No overlap/clipping; apply/save always reachable; image retains useful space; sidebar scrolls naturally |
| Folder reviewer | Work on several images and return to an earlier edit | Thumbnail names/states are tiny and easy to confuse | Clear current photo, readable unsaved state, retained edits per image, close-review protection |
| AI user with disconnected engine | Recover from unavailable model | App-wide status can overshadow local healing | Specific availability message, reachable settings, existing layers preserved, healing remains available |

Recommended cycles: (1) run current behavioral regression tests after structural changes; (2) inspect empty/open/selected/applied/comparison states in a real browser at common desktop widths; (3) run the role scenarios above with synthetic fixtures; (4) inspect adverse states and keyboard behavior; (5) rerun only affected checks after fixes. Screenshots should contain a real loaded fixture, not fabricated editor results.

## Primary references and application

- [Microsoft: Command bar](https://learn.microsoft.com/en-us/windows/apps/develop/ui/controls/command-bar) recommends ordering commands by importance, stable placement, short labels, and secondary-command overflow when space is constrained. Applied here to save hierarchy and visible repair actions.
- [Adobe Lightroom: Remove objects manually](https://helpx.adobe.com/ca/lightroom/web/edit-photos/remove-objects/remove-objects-manually.html) documents brushing over an object and its shadow and refining with Add/Subtract. Applied here to task language and grouping selection controls; this is evidence of an established domain interaction, not a request to copy Adobe's visual design.
- [W3C: Target size minimum](https://www.w3.org/WAI/WCAG22/Understanding/target-size-minimum.html) explains the 24 CSS pixel target requirement and exceptions. Applied here to small document/layer/zoom controls.
- [W3C: Dragging movements](https://www.w3.org/WAI/WCAG22/Understanding/dragging-movements.html) explains alternatives to drag interactions and the essential-drag exception. Applied here to explicit navigation controls and click-based selection options.
- [W3C: Contrast minimum](https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum.html) provides text-contrast criteria. Applied here to the measured palette and avoiding low-contrast fine print.
- [W3C APG: Toolbar pattern](https://www.w3.org/WAI/ARIA/apg/patterns/toolbar/) describes focus and arrow-key behavior for a semantic toolbar. Applied here as a warning to implement the complete interaction when introducing that role.

## Architecture boundary

Separate visual tokens/layout from editor state and command logic. Keep existing command IDs and native bridge contracts stable while moving controls. A thin presentation updater can derive next-step text, busy feedback, method description and comparison state from actual editor state. Do not duplicate state in decorative components. Existing JavaScript tests parse an inline script; moving scripts must intentionally update test loading and backend asset/CSP handling. This design review does not recommend a framework migration solely to obtain a new appearance.
