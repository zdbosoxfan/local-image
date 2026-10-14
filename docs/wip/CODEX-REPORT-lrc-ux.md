# Library/Develop Classic UX — QW1, QW2, QW8, N4

Continued the branch at `c83768d9`, including PR 12's shared AI Preferences shell and the existing
design-system work. The only initial working-tree change was the owner's task document; it is
preserved. No commits, dependencies, lockfile changes, unsafe code, rendering-pipeline changes or
Match Look edits were introduced. Engine changes are limited to preference namespaces and common
configuration/model-directory resolution.

The holistic review was absent from this checkout and Git history. Read the coordinator's
`../integrate/docs/wip/REVIEW-holistic-fable.md` read-only: executive summary, W7, U1/U2/U3/U7,
and roadmap QW1/QW2/QW8/N4. Its N4 row specifies one **application** settings file; catalog-specific
preferences remain with each catalog, preserving existing catalog behavior. The owner then explicitly
added duplicate-systems audit D22 / J16: the AI adapter, common configuration directory and shared models root are included.

## Delivered

- **QW1:** Basic replaces the Light caption; its existing WB preset/picker/Temperature/Tint controls
  precede Exposure, Contrast, Highlights, Shadows, Whites, Blacks. The existing Curve control follows
  Tone. WB also moves in Develop layers, with the used-tools filter updated. Negative appears below
  Detail/Optics only when conversion is enabled or revealed using Add tool → Negative. Revealing it
  does not enable conversion. Saved Develop settings, legacy section IDs/eye switches and slider
  commands remain unchanged.
- **QW2:** One shared module widget, first in the title bar's right-to-left group, gives Library,
  Develop, Pixel and Vector the same upper-right anchor. Workspace/search/theme controls are to its
  left; custom captions stay at the far right. Module IDs and keyboard commands remain unchanged.
- **QW8:** Catalog / Collections / Loupe throughout Library-facing strings, dialogs, import options,
  menus, shortcut catalog and help text. Legacy engine action labels are renamed at the UI boundary;
  commands, catalog names and stored history remain unchanged. Collection icons use Lucide stacked
  photos, including a monochrome filtered Smart Collection icon under its legacy asset filename.
  Documentation and tests are updated. New English strings are on i18n pending lists.
- **N4 / J16:** The existing Preferences shell renders application, Library & Develop, Compositing,
  AI, shortcuts and attributions pages. `app.settings {tab}` opens the corresponding page in this
  single modal; AI entry points reuse it. Application settings draft until Apply/OK; Cancel drops
  unapplied edits. Catalog import/XMP/cache and maintenance actions remain immediate and labelled.
  There is one `preferences.json`, with `library.*` for UiState/settings/module and typed `ai.*`
  for AiSettings, alongside existing editor sections. AI's default save uses the same checked,
  fsynced native writer. A shared write lock protects section merges; stale editor/UI writes after
  an AI update fail and retry instead of replacing that update. Existing sources (`library-ui.json`,
  `mode`, AI `config.json`, previous `dialogs` host keys) are read leniently when the relevant namespace
  is absent, migrated durably on first native launch, and retained as read-only migration inputs.
  Exact legacy bytes are backed up under `preference-backups`; unknown AI fields survive. Invalid
  fields use defaults, while invalid catalog paths block automatic opening. Unified rejected Library
  values remain in `library.recoveredOriginal`; AI rejections are recorded in migration metadata.
  UI serialization retains its three-second cadence with forced capture on Apply/OK and exit.
  `lightcraft_sysmem::app_dirs` supplies the one config directory (including portable mode), with
  compatibility reexports for the app and camera profiles. Profiles in the old LightCraft config folder
  remain read-only fallback inputs; canonical profiles take precedence and new writes use the shared
  config folder. Explicit profile-directory overrides remain exclusive. One models root serves AI, Compositing
  segmentation/cutout and Develop masks; existing legacy model folders survive, even without an old
  config file. Changing the root updates open Develop sessions. SAM 3 remains optional and off.

Shared widgets and inherited theme metrics are used throughout; shared additions and usage are
recorded in `docs/DESIGN-SYSTEM.md`. User/automation behavior and migration keys are documented in
`docs/LIGHTROOM-CLASSIC-UX.md`. The standalone LightCraft harness retains a Preferences fallback;
Local Image's hosted modules all use the single shell.

## Validation

Offline Rust 1.98.1 throughout, one cargo process at a time, launcher-provided `CARGO_BUILD_JOBS=4`,
retaining this worktree's `target/`. During implementation only focused tests were run.

- New Library headless click/drag coverage: **2 passed**, WB before Tone, folding Basic, WB picker,
  temperature drag, Add tool/reveal/conversion, enabled conversion visibility, old UI-state loading,
  Catalog/Collections navigation and the existing Loupe ID. Dark/light, 250/400 pt, 1.0/1.5 scaling.
- New host coverage: shared shell, Apply/Cancel/reopen, persistence, lenient legacy/unified migration,
  path preservation, module click geometry, and Pixel/Vector positioning. Final counts below.
- Real hardware GPU interactive test added: uses the real editor GPU canvas and clicks all three
  module buttons in the actual host path. Software adapters fail the test. Adapter and interaction
  times are logged; no hardware-specific thresholds or runtime tuning were added.
- Before/after offscreen screenshots under `target/lrc-ux/`: Library/Develop/Compositing/Preferences,
  dark/light and 250/400 pt panels. Synthetic test photos only; no desktop windows or client photos.

Final full-suite results (unit, integration and doc tests combined) are logged under
`target/j16-full-tests-*.log`. Strict `clippy --all-targets -- -D warnings` logs are under
`target/j16-clippy-*.log`; all seven changed crates pass strict lint, including final corrections.
Formatting and `git diff --check` pass for our changes (the owner task edit is preserved).

| Crate | Passed | Failed | Ignored | Result |
| --- | ---: | ---: | ---: | --- |
| lightcraft-sysmem | 32 | 0 | 0 | Passed |
| li-ai | 512 | 0 | 6 | Passed |
| photocraft-engine | 2103 | 1 | 32 | Audit corrected; all 4 focused checks passed, doc tests passed |
| lightcraft-engine | 896 | 0 | 6 | Passed |
| lightcraft-ui-egui | 340 | 0 | 9 | Passed |
| photocraft-ui-egui | 1764 | 0 | 80 | Passed |
| local-image | 157 | 0 | 7 | Passed |

The table preserves the full-run results, including the resolved audit failure. The corrected
audit passed all four focused tests; remaining editor-engine doc tests also passed. No failing check
remains. Additional coverage includes 4 migration/concurrent-store tests, the AI namespace and alias
round trips, and a real-click AI Apply/Cancel test against a single root store.

Screenshots: 16 `before-*` and 16 `after-*` host images in `target/lrc-ux/`, covering
Library/Develop/Compositing/Preferences × dark/light × 250/400 pt. One additional standalone
Preferences screenshot checks the retained non-host fallback. Inspected panel order, title-bar
placement, collection icons, clipping and modal layout. Only synthetic fixture photos were used.
The final after capture passed explicitly after the full suites (1 host screenshot test and
1 standalone screenshot/click test); ignored screenshot tests are not
counted as a substitute for regular UI tests.

The added GPU test requires a physical adapter and real editor GPU-canvas texels before clicking
Library, Develop and Compositing through the host. The focused run (`target/lrc-host-final-focused.log`)
used **NVIDIA GeForce RTX 5090 / Vulkan**: Library **20.42 ms**, Develop **21.61 ms**, Compositing
**18.82 ms** for each press/release plus follow-up host frames. These are offscreen interaction
measurements with a small synthetic fixture, not display presentation or full-resolution raw/AI
benchmarks. No hardware-specific thresholds or runtime tuning were introduced. Existing opt-in
Comfy/model/raw benchmark tests remain ignored; no live Comfy jobs were submitted.

The initial three-crate full validation exposed stale standalone Preferences click coordinates,
three assertions for the retired My Photos translation, and two host tests expecting a separate
Library dialog. The modal fallback now has a stable top anchor, constrained rows and scrolling;
the original click checks pass. Naming assertions and host singleton expectations were updated.
Failure logs are retained under `target/lrc-*`; focused passing corrections are in
`target/j16-focused-*` and `target/lrc-regression-*`. The expanded preference-usage audit also identified the legacy
`auto_start_comfy` key as unused.
It remains round-trippable and is marked hidden under the existing policy; automatic startup behavior
was not added. The expanded seven-crate final suites run once;
any subsequent correction is checked with the affected test filter. All failures are resolved.
The final correction logs are `j16-prefs-usage-corrected.log`, `j16-photocraft-engine-doc.log`,
and the final lint logs for both engines and the native app. The two late migration-message additions
to the pending translation list passed final Library UI lint and the focused catalog check
(`j16-final-i18n-pending.log`, 1 passed). Camera-profile directory compatibility passed strict
engine lint and the focused `camera_profiles` tests (**5 passed**); the added regression verifies legacy reads,
canonical precedence, unchanged legacy bytes and exclusive explicit overrides
(`j16-clippy-profile-compatibility.log`, `j16-profile-compatibility.log`).

## Scope outside the initial UI files

J16 necessarily changes `lc-sysmem` (common dirs), `li-ai` (typed settings adapter and cutout root),
`pc-engine` (Library/AI preference sections and segmentation root), `lc-engine` (config/model root),
and native services (migration/store binding), alongside both UI crates and the host. No new crate
or dependency was needed. Existing catalog-specific files remain compatible; they are catalog data.

## Sources and decisions

- Adobe [tone/color](https://helpx.adobe.com/lightroom-classic/desktop/process-and-develop-photos/image-tone-color.html)
  documents WB and Tone within Basic, including relative Temperature for rendered files. Adobe
  [Develop options](https://helpx.adobe.com/lightroom-classic/desktop/process-and-develop-photos/develop-module-options.html)
  identifies Basic's tone controls. UI order changes only; the existing engine conversion order stays.
- Adobe [workspace basics](https://helpx.adobe.com/lightroom-classic/desktop/workspace/workspace-basics.html)
  places the module picker at the upper right and names Loupe among the view modes.
- Adobe [Collections](https://helpx.adobe.com/lightroom-classic/desktop/organize-photos-in-lightroom-classic/photo-collections.html)
  describes photo grouping without moving files, collection sets, smart collections and target
  collections. [Browse/compare](https://helpx.adobe.com/lightroom-classic/desktop/viewing-photos/browse-compare-photos.html)
  uses the Loupe name for single-photo viewing.
- Existing `docs/DESIGN-SYSTEM.md`, `theme.rs` and shared widgets are the visual specification.
  PR 12's `prefs_ui.rs` / `dialogs.rs` shell and its checked writer are the established implementation.
  The host-page adapter, typed AI adapter and namespaced JSON sections are Local Image integration
  decisions; they avoid adding dependencies or coupling either UI crate to the other.

## Final state

All four requested items and the expanded D22/J16 store/directory requirements are implemented.
Changes are uncommitted. The owner task and supplied reference documents are preserved.
Generated workflow-test PNGs were moved from the app-local test output folder to
`target/lrc-app-test-artifacts/library-workflow-ux/`; build caches remain in the shared worktree target.
The retained compiler warning about optional craft-fonts is pre-existing; translation work remains
on the pending lists as requested. No task implementation is left outstanding.
