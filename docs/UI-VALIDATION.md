# Interface overhaul validation

> **Historical record.** The browser scripts and legacy interface files this report refers to were removed on 2026-10-06 (last present in `926fdd7`). Current checks are listed in [DEVELOPMENT.md](DEVELOPMENT.md).

Historical 0.3.0 validation. Current interface changes and checks are recorded in [DESKTOP-VALIDATION.md](DESKTOP-VALIDATION.md).

The overhaul was reviewed across visual design, editor architecture, interaction
behavior, and simulated user roles. These are expert reviews and automated
acceptance checks. No participant focus group was conducted.

## Design decisions

- Keep the photo central, with neutral charcoal surfaces, readable type and one
  warm action color. No external fonts, decorative dashboard cards, or remote
  design dependencies are needed.
- Group repair method, readiness, guidance, and the apply action in one inspector.
  Selection controls remain beside the canvas; editable layers remain visible.
- Start with Quick Heal and remember an explicit method choice. Disconnected AI
  and unavailable texture repair explain recovery before a repair is submitted.
- Keep image export separate from saving an editable project. Preserve native
  overwrite confirmation, Capture One return, and unsaved-layer close protections.
- Separate markup, style, and behavior in source while retaining one
  nonce-protected response at the desktop host's trusted `/remove` route.

The primary design references and initial role scenarios are in [UI-DESIGN.md](UI-DESIGN.md).

## Review cycles

1. Baseline navigation and layer regressions passed before changes. Independent
   design and architecture reviews identified compressed controls, scattered
   repair settings, and an AI-first starting state.
2. The workflow pass added explicit selection/readiness/comparison states. Review
   caught stale pen guidance after tool changes and missing per-method healing
   readiness. Focused regressions cover both corrections.
3. Keyboard review caught Space and Enter being consumed by canvas shortcuts on
   focused buttons. Browser acceptance checks native button activation, real
   texture repair, layer comparison, export, project download, and narrow windows.
4. Visual inspection caught a Windows file-encoding issue that prevented the
   theme from applying, plus clipped controls in short windows. Both were fixed;
   rendering and browser checks now cover theme tokens, typography, repair-action
   bounds, and project-save reachability. Final visual and architecture reviews
   found no remaining blockers. See [UI-REVIEW.md](UI-REVIEW.md).

## Automated evidence

- Python: 63 passed across resource rendering, application paths, layer/project
  lifecycle, backend texture behavior, and fast inpainting. One additional
  historical photo acceptance test is skipped because its private fixture is
  unavailable.
- Existing JavaScript navigation and layer/project regression suites pass with
  added workflow, availability, comparison, and preference checks.
- Source-backend smoke: generated 16-bit compressed TIFF, both healing methods,
  editable project save, original-file preservation, and rejected unauthenticated
  runtime changes all pass.
- Real-browser acceptance and screenshots are produced by
  `tests/test_ui_browser.cjs` using a generated synthetic image and an isolated
  development data folder. See [DEVELOPMENT.md](DEVELOPMENT.md) for reproduction.

## Initial source-only pass limits

The following limits describe the initial UI pass. See [Windows and AI setup validation](AI-SETUP-VALIDATION.md) for the subsequent installed 0.3.0 tests.

The real-browser pass uses Microsoft Edge against the source backend. The native
installer was not rebuilt or exercised in this pass. Native bridge behavior is
covered by the existing regression harness and source-backend smoke, but actual
Capture One handoff, native file dialogs, and a packaged Windows installation
still need release-machine verification. ComfyUI and AI model files were not
available; AI connectivity/recovery was checked, but GPU removal was not run.

Accessibility checks cover visible focus, labels, keyboard interaction, reduced
motion, and layout resilience. They are not a full accessibility-conformance audit.
