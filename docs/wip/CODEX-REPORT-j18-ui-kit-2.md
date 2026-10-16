# J18 report — shared widgets, notices and histogram

Continued the branch's existing J17 `li-ui-kit` foundation. Changes are uncommitted.

## Completed slices

- **D24:** `li-ui-kit::widgets` now owns the paired selection/tab and text buttons, checkbox,
  icon-action state/chrome, colour tile/swatch, dropdown shells, heading/disclosure-enable controls,
  separators, disabled-reason tooltip and automation registry. Both UI crates retain their public
  signatures as translation/domain/icon adapters. Unique cards, document colour editors and numeric
  draft ownership remain frontend-specific. Inherited geometry/type tokens are in `widget_style`;
  palette roles come from the shared `Tokens`.
- **Slider pair:** one range/quantization/drag/release/fine-motion engine and gradient painter;
  Editor and Library thumb profiles retain their look. Library reset, numeric owner/draft cancellation,
  label formatting and temperature nudge policy remain unchanged. Continuous interpolation replaces
  sampled Library tracks. Sub-step Shift motion accumulates before rounding; final release is retained.
- **D26:** one serialized `Notice` model, active/history caps, warning-preserving operation completion,
  and details/history renderer. Editor remains the public notice facade. Library toast entry points
  retain their timing and capture notices immediately, including same-frame replacements; expired
  diagnostics remain reviewable/copyable. Existing direct HUD tuple writers are observed through a
  compatibility adapter. Library adds persistent Details / History and Dismiss; control inspection
  exposes history, omission count and open state. History is session-only (`serde(skip)`).
- **D4 widget:** one borrowed 256-bin `u32`/`u64` histogram view and shared painter with both inherited
  profiles: Editor ribbons and Library sqrt/luma/neutral trace. Both old renderer sites delegate.
  Scope colour roles moved to shared tokens without changing values. Library clipping supports focus
  and keyboard activation. Missing legacy bins are zero-padded safely. Statistics, cached EXIF and
  core histogram calculations remain in their callers.

## Quality and cost evidence

| Small sample / resource | Before | After |
| --- | --- | --- |
| Widget profile geometry/paint parity, 5 themes × 2 panel widths × 2 profiles | Frozen pre-J18 source | Exact shape equality in 20 scenes |
| Histogram geometry, 5 fixtures × 5 themes × 2 widths × 2 profiles | Frozen pre-J18 source | Exact shape equality in 100 scenes |
| Histogram CPU output, 2 themes × 2 widths × 2 scales × 2 profiles | Frozen pre-J18 output | 1,183,000 RGBA pixels checked; 0 different pixels; max channel error 0 |
| Library bin/smoothing scratch per displayed histogram | 8 heap Vec allocations / 8,192 bin bytes | 0 bin scratch heap allocations; 4,096 bytes of fixed stack arrays; painter geometry buffers remain |
| Library gradient, 97 edge/midpoint samples vs intended channel interpolant | Max grayscale channel error 5; 48 sampled rectangle shapes | Max error 0; 1 continuous mesh (2 triangles for 2 stops) |
| Library diagnostics after toast expiry | 0 retained, after 1.4 s / errors 6 s | Up to 256 session entries; 300-entry test retains 256 and reports 44 omissions |
| Active editor notices | 3, with persistent guidance retention | Same cap/policy; warning counts and complete operation details retained |

The real shared-widget facade pointer/keyboard/slider/dropdown path was exercised with final CPU
rendering, once per screenshot sample. ProMedium 180/270 pt took 4.426/4.595 ms; StudioLight 180/270 pt
4.385/4.283 ms for the complete scripted sample including its final CPU frame. These are small frontend
samples, not GPU timings, frame-rate promises or a benchmark sweep. Structural allocation/primitive
reductions above are hardware-independent; no hardware-specific tuning was added.

User-visible changes: Library has review/copy after HUD expiry and transparent swatches display the
same checker as Editor; Library gradients reach exact stops/endpoints; shared selection/checkbox
controls expose checked/selected state and keyboard focus; section-enable controls are named and appear
on keyboard focus. Editor Studio notice row height becomes 34 pt (from 32) to fit shared action buttons;
Pro remains 32 pt. History warning titles use the contrast-adapted Library caution role in light mode: StudioLight title-on-chrome contrast is 5.0315, compared with 3.0436 when reusing the Editor warning colour (existing frozen token values, sRGB/709 luminance calculation).
The HUD no longer fades out before expiry; ordinary egui entrance animation remains.

## Validation and visual review

`cargo +1.98.1 fmt -p li-ui-kit -p lightcraft-ui-egui -p photocraft-ui-egui`: completed.
`cargo +1.98.1 check --offline` on those same three packages: **passed**, final run 6.41 s.
`cargo +1.98.1 clippy --offline -p li-ui-kit -p lightcraft-ui-egui -p photocraft-ui-egui --all-targets -- -D warnings`: **passed**, final run 29.22 s. The first lint pass found two identity-translation bindings in the frozen English-only widget oracle; those redundant bindings were removed and exact shape parity rerun successfully.

Focused results: **24 unique passing tests** (18 new, 6 relocated parity tests):

- `li-ui-kit j18_`: **19 passed** (16 unit + 3 integration; unrelated tests filtered out).
- `lightcraft-ui-egui j18_`: **1 passed** (real expiry/History/clipboard/HUD automation).
- `photocraft-ui-egui`: **4 passed** using `j18_`, then the two failing setup cases retried with
  `j18_editor_notice_` and `j18_histogram_widget_` after fixing font initialization/keyboard focus.
  The initial run's widget gallery and frozen pixel tests passed; corrected notice and keyboard
  tests subsequently passed. No existing test assertions were weakened.

Logs: `target/j18-kit-final-tests.log`, `target/j18-library-final-tests.log`,
`target/j18-editor-tests.log` (initial 2 pass / 2 harness failure evidence),
`target/j18-editor-notice-tests.log`, `target/j18-histogram-interaction-tests.log`,
`target/j18-widget-oracle-final.log`, `target/j18-final-check.log`, `target/j18-clippy.log`; initial lint diagnostics remain in `target/j18-clippy-initial.log`.
Only task-specific `j18_` tests were executed; full crate/workspace/GPU acceptance suites and benchmarks
were omitted under the task's **ABSOLUTE MINIMUM TESTING** override. One build process at a time,
`CARGO_BUILD_JOBS=4` inherited unchanged, one retained `target/`, Rust 1.98.1, offline Cargo commands.

Generated **60 CPU screenshots**, with visual review of both themes and both widths and exact pixel comparison of the frozen pairs: 4 mixed-profile widget galleries, 40 histogram before/after/shell captures, and 16 notice HUD/row/history captures. Generated test screenshots stay under `target/design-system/j18-widgets`,
`target/design-system/j18-histogram` and `target/notices-ux`. Both themes and both widths are exercised;
histogram samples also cover 1.0/1.5 pixels per point. No owner photos or derived content were used.

The existing optional craft-fonts build-script message remains (also recorded by J17); J18 changes no font fallback or embedding policy.

GPU tests added: **0**; this slice changes egui model/geometry, not GPU code. No GPU performance claims.
Early harness failures concerned egui 0.36 `run_ui`, explicit modifier events, texture-delta disposal,
named-font initialization, explicit keyboard focus and settling Area geometry after HUD expiry.
Assertions were retained; corrected harness setup was rerun with focused filters.

## Compatibility, scope and sources

Public widget/notice APIs and registry keys remain adapters; no dependency or Cargo.lock changes,
unsafe code, C or `-sys` dependencies were introduced. Catalog/document/edit schemas are unchanged;
new Library state is skipped by serde, and the old notice wire shape is covered by default-loading tests.
Match Look files, the desktop app and external services were untouched.

Necessary files beyond the original owned widget/notice sites: new kit modules and tests plus kit
`lib.rs`/`theme.rs`; `pc-ui-egui/src/rgb_histogram.rs` (actual existing renderer), `pc-ui-egui/src/lib.rs`
(test registration), the new PC facade test module; Library `state.rs` (session history) and `control.rs`
(read-only inspect fields); shared design-system documentation and the i18n pending list. Panel histogram
and HUD call sites were within the authorized panel migration. The pre-existing task-file change remains.

Followed `docs/DESIGN-SYSTEM.md`, audit §1/D4/D24/D26/J18, and the inherited
`docs/upstream/photocraft/ui-design.md`. The implementation retains the existing product grammar rather
than defining new proprietary-product interactions. Used installed egui 0.36.2 primary source:
`response.rs`, `context.rs` keyboard/accessibility click dispatch, `widgets/slider.rs`, and ecolor
`color32.rs` gamma-multiply semantics; also the existing vendored `egui.wgsl` gamma-space interpolation
path. [Official egui Sense documentation](https://docs.rs/egui/0.36.2/egui/struct.Sense.html) specifies
focusable click/drag controls. Numeric fine-motion accumulation and borrowed legacy histogram views
are local integration work around these established APIs.

TEST-CHANGED: the six `pc-ui-egui::rgb_histogram::tests` tests moved one-for-one into
`li-ui-kit::histogram::tests`, prefixed `j18_histogram_`; their assertions remain. The frozen pre-J18
painter copies exist only in parity fixtures, never in production. Existing notice tests retain their
assertions; their CPU renderer helper moved to test-module scope for reuse by the new notice test.
No implementation work remains. Translation catalog consolidation and full merged/GPU validation are left to the coordinator under the explicit task rules. Translations remain pending in `docs/wip/i18n-pending-j18-ui-kit-2.md`.
