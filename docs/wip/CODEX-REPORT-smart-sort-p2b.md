# Smart Sort Phase 2b report

Continued the existing Phase 2a dialog, people-aware planner, and the sessions, tokens, examples, bursts and keyboard helpers already in this branch. No commits were made.

## Implementation

1. **Capture-time sessions:** Step 1 has the enable checkbox, gap control and editable session names with time spans/counts. Timestamp identities keep filters intact after renaming. The preset stores enabled/gap/name/export-folder settings. Export can add standalone session folders or narrow default/custom/Unsorted folders to one session; missing/invalid timestamps use `No time`. Session filtering uses the catalog's current capture times, including corrected times. Disabling sessions makes saved restrictions dormant.
2. **Folder/file tokens:** Event name defaults from the source directory where available. New presets use `{event}/{folder}`; older explicitly empty patterns still export directly into folder names. The optional file pattern overrides only file naming, retaining the selected export preset's remaining settings and extension. All specified tokens, nested folders, safe components and case-insensitive ` (2)` folder collision numbering are connected to preparation/export. Folder counts reflect resolved paths. Existing destination/original guards remain in use.
3. **Folders from examples:** The Review button uses the current Review selection or the Library selection captured when opening the dialog. The shared command validates cached examples, creates a tag-free category and uses the existing exemplar-only classifier path. Tags remain editable afterward. Suggested names use keyword leaves so hierarchical keywords cannot produce invalid category names.
4. **Bursts:** Step 1 exposes grouping and Strict/Normal/Loose. Review collapses stacks to a representative thumbnail with a count badge; selecting it selects its members. Export offers Best of each burst / All photos. Sharpness is measured on the analysis image and stored atomically in `AI/sharpness.json`; legacy embeddings gain sharpness without repeating model inference. Missing embeddings break consecutive stacks. Library stacks are written only on request through `stack.group`, merged into one undo step, preserving the Library selection.
5. **Keyboard review:** Arrows move focus and scroll it into view; Shift extends ranges, Command-click toggles members, and Command+A selects all. 1–9 move, 0 clears assignments, Alt+1–9 also adds, Backspace removes from the current folder, Z/Command+Z undo, and Shift+Command+Z redo. Folder rows display number hints. Review history includes learned exemplars and works through native Edit Undo/Redo as well as key events. Dialog number keys do not rate catalog photos.

No Phase 3 people UI was added. Existing people conditions continue through the shared planner and its preparation/notice path. New preset and folder fields have serde defaults.

## Commands and automation

Registry parameter descriptions cover `smartSort.sessions`, `smartSort.folderFromExamples`, `smartSort.bursts`, and `smartSort.stackBursts`. `smartSort.plan` and `smartSort.export` accept an optional `sortPreset` independently of the existing normal export `preset`. A preset-aware plan includes logical folders, resolved `paths` and existing people notices; old folder-only requests remain supported.

Required IDs are present: `smartSort:sessions`, `smartSort:sessionGap`, `smartSort:session:<index>`, `smartSort:eventName`, `smartSort:folderPattern`, `smartSort:filePattern`, `smartSort:folderFromExamples`.

Additional controls include `smartSort:sessionFolders`, `smartSort:folderSession:<index>`, `smartSort:bursts`, `smartSort:burstStrictness`, `smartSort:burstBadge:<photoId>`, `smartSort:bestBursts`, `smartSort:allBursts`, and `smartSort:libraryStacks`.

## Validation

All builds use toolchain 1.98.1, `--offline`, `CARGO_BUILD_JOBS=3`, and one Cargo build at a time. Full suites run with one test thread and the sandbox GPU drivers disabled; only the changed engine/UI crates are tested.

| Check | Passed | Ignored | Result |
|---|---:|---:|---|
| Full engine unit suite | 417 | 3 | Pass |
| Full engine integration/coverage suite | 261 | 0 | Pass |
| Full UI suite | 208 | 1 | Pass |
| Focused Smart Sort UI suite | 18 | 0 | Pass |
| Final feature layout check at both resolutions | 1 | 0 | Pass |
| Clippy, engine + UI, all targets, `-D warnings` | — | — | Pass |
| Formatting and `git diff --check` | — | — | Pass |

Nine engine tests were added (eight integration-style feature/cache tests and one keyword-name helper test), plus a malformed date assertion with a multibyte character in the existing token test. Five actual headless UI tests cover session rename/filter/nested export, examples from Library/Review selection, burst badges and both export modes, selection/assignment/undo keys including native Undo/Redo, and feature-enabled layout at both sizes. The full engine run includes the existing threshold, sharpness/tie-break, token and people tests.

The final full suites account for **886 passed, 4 ignored, 0 failed**, with zero doctests. The three ignored engine cases are the existing real-weight people test and two timing/100k benchmarks; the ignored UI case is `grid_frame_100k`. Six existing engine GPU tests passed through their no-adapter skips. No whole-workspace suite was run.

Both full Cargo commands exited successfully. After the UI binary passed, the shared external `li-test-runner` emitted an empty-value arithmetic/statistics warning; it did not affect the tests or Cargo's exit status. That runner is outside this worktree and was left untouched.

Validation logs are retained under `target/smart-sort-p2b/`.

Screenshots are produced under `target/smart-sort-p2b/screenshots/` for all three steps at 1280×800 and 1920×1080, including sessions and burst controls enabled.

GPU tests added: **none**. This phase adds pure CPU/planning/UI code and reuses the normal export renderer. Hardware GPU validation remains a coordinator check; this sandbox has no GPU adapter.

## English strings for later translation

New UI messages use `tr` / `tr_format!`. Only the shared format catalogs receive English fallback entries, as required to compile; no translation-quality work was done.

- `Split into sessions when there is a gap of more than`
- ` minutes`
- `{start}–{end} · {n} photos`
- `No time`
- `Group bursts and near-duplicates`
- `Normal`
- `+ Folder from Examples…`
- `Event name`
- `Destination folder pattern`
- `File name pattern (optional)`
- `Folder tokens: {event}, {folder}, {person}, {session}, {date}, {camera}. File names also accept {original} and {seq:4}.`
- `Export sessions as folders`
- `Best of each burst`
- `All photos`
- `Also stack in the Library`
- `Only in session`
- `All sessions`

Default data names remain English (`Session N`, `No time`, `New Folder`); session names and generated example-category names are editable.

## Remaining work

No required Phase 2b work remains. Phase 3 people UI, translation quality, and real GPU/model-weight execution remain outside this task. No dependencies, unsafe Rust, or C/C++ code were added. No GUI windows were opened and no running app was stopped. Changes remain in the working tree without a commit.
