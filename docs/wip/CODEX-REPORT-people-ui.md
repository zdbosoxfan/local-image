# Smart Sort Phase 3 — People UI

Date: 2026-10-09. Continued the branch's existing Smart Sort dialog, worker and keyboard implementation. Changes are uncommitted.

## Completed

1. **Opt-in and analysis.** The per-library face-recognition checkbox remains off by default. Missing models use the existing host downloader's `yunet` bundle (YuNet plus SFace), progress, cancellation and licence links. Face inference runs on a separate worker alongside tagging, skips cached content and videos, reports failures, saves completed results and updates People during analysis. It uses a dedicated Rayon pool with half the available CPUs, matching §5.10. Library People and Find This Person can also start analysis. Changing libraries drops the old workers and transient face textures.
2. **People workspace.** The Library People view and the Smart Sort People section share round face crops, editable names, pasted-name suggestions, folder/total counts, pending-match badges, Ignore/restore, pinning, merge by drag or menu, and an undoable split review. Minimum photos defaults to 3 and is editable from 1–10. Seeded and pinned people remain available, Show everyone reveals smaller groups, and the first 60 bubbles have a Show all action. Headshot covers decode on workers. Imported XMP people remain usable without face recognition; recognized people do not reappear as duplicate or ignored XMP choices.
3. **Confirmation.** A full-screen workspace shows the face crop beside the full photo, least sure first. Y/N/S, buttons, arrows and Command-Z use the existing queue keyboard policy. Yes updates membership and re-ranks; No uses durable engine rejection; Not sure is a local skip that can be undone. Likely duplicate identities prompt Same / Different / Not sure before review. Merge/split undo uses the engine history. The underlying People controls stop querying/clustering while review is open.
4. **Seeding.** Add People from Photos uses the host folder picker and the engine's exactly-one-face importer on a worker. Filename stems become editable names; skipped inputs are reported. A unique sure match can name an existing unnamed gallery identity without changing its selected folder/filter ID. Paste names accepts the engine's `name[,title]` roster format and supplies bubble type-ahead.
5. **Library filtering.** Bubbles filter the actual Library grid by durable person identity. The filter bar supports multiple people and Any/Everyone, includes imported XMP names, and excludes ignored identities. The Library shows readable names in active filters. Loupe's context menu opens Find This Person, with actual clickable face boxes, naming, matching photos and Create Folder. Smart collection rules have a name/identity picker. Catalog rule validation and `album.ruleFields` now accept the engine's existing `anyOf`/`allOf` person operators. Old filter/settings JSON loads with defaults. Merged identities resolve in active filters without rewriting them, and saved smart album rules update in the same merge undo step; undo restores their prior membership. Folder bubbles and Review also follow merged IDs.
6. **Export.** Person folders are explicitly enabled, never created just by naming a bubble. Step 3 has People toggles per folder, bubble selection, Any/Everyone, tag AND/OR, and Review matches. Folder edits are retained rather than regenerated each frame. Bubble counts use the selected row’s planned membership, including duplicate folder names and disabled rows. Unknown/ignored-person notices from the plan are visible. Include people's names defaults off; when enabled, rendered metadata and copied-original sidecars gain deduplicated `People|name` keywords. No IDs or embeddings are written to export metadata. Saved smart albums use the same person/tag conditions as the plan, including first-match exclusions, rather than generic empty tag rules.
7. **Privacy.** Face records, prototypes, seeds, rejections and names use the existing library-local engine store. Settings → General → Clear face data calls the model-independent engine command, with optional removal of names Smart Sort added. It clears transient cover textures and the loaded face model. UI state is transient; durable flags and names live in the engine's library data.

Automation IDs required by the task/spec are registered, including `smartSort:faces`, `smartSort:bubble:<id>`, `smartSort:bubbleName:<id>`, `smartSort:minFaces`, `smartSort:ignore:<id>`, `smartSort:pin:<id>`, `smartSort:folderPeople:<index>`, `smartSort:review:<id>`, `smartSort:reviewYes`, `smartSort:reviewNo`, and `smartSort:reviewSkip`. `ui.inspect` exposes the People list, finder, picked face, current queue rows, skips and merge prompt.

## Validation

All Cargo commands are offline, use Rust 1.98.1, `CARGO_BUILD_JOBS=3`, and run sequentially. Development used filtered tests/checks. Final full suites are restricted to the three changed crates.

- People UI development tests: **12 passed, 1 ignored** (the separately run GPU check). Actual clicks, text input, keys, wheel scrolling and drag/drop cover opt-in/download cancellation, concurrent worker/caching/cancellation, naming/type-ahead, thresholds and 60-item limit, Ignore/restore/pin, grid Any/Everyone, Loupe face picking, queue rejection/skip/undo/live re-ranking, duplicate prompts, merge/split undo, three headshots and gallery matches, model-free clearing, export metadata opt-in, and live smart albums/rule editing.
- Added catalog tests cover Any/Everyone grid filters, old serialized filters, and person identity-set rule validation/evaluation.
- Added engine tests cover reusing an already named automatic identity when finding from a picked face, name metadata opt-in for rendered metadata/copied originals without changing catalog keywords or original bytes; and merged-ID filters/smart albums/current-view saving through merge and undo.
- The final full run executed once: **1,087 passed, 0 failed, 6 ignored**. Catalog: 76 unit + 111 coverage passed, 1 ignored; engine: 419 unit + 261 coverage passed, 3 ignored; UI: 220 passed, 2 ignored. All doc tests passed (zero cases). Command: `cargo +1.98.1 test --offline -p lightcraft-catalog -p lightcraft-engine -p lightcraft-ui-egui`, with the NVIDIA Vulkan environment below. Log: `target/people-final-tests.log` (catalog 0.36 s + coverage; engine 67.01 s + 2.61 s coverage; UI 179.42 s).
- After that full run, final review added the merged-identity reference regression/fix. The affected engine People/export tests were rerun: **31 passed, 1 ignored**, including the new regression and existing merge/split behavior. This deliberately keeps the full-suite run to one, per the owner's test budget. Post-fix UI tests: **12 passed, 1 ignored**, including selected-filter merge/undo and review through a stored former ID. The selected-folder count regression was also rerun after changing lookup from names to row order: **1 passed**, including duplicate folder names (`target/people-folder-count-tests.log`).
- Clippy passed for all three changed crates and all targets with `-D warnings`: `CARGO_TARGET_DIR=target/clippy cargo +1.98.1 clippy --offline -p lightcraft-catalog -p lightcraft-engine -p lightcraft-ui-egui --all-targets -- -D warnings`. The four new style warnings were corrected without suppressing lints. Log: `target/people-clippy.log`.
- `cargo +1.98.1 fmt` was run for the changed crates. Its only unrelated source adjustment is formatting two existing expressions/signatures in `softpaint.rs`. `git diff --check` passes. The Cargo.lock change only adds an existing Rayon dependency edge; no package entries, unsafe code, native dependencies or downloaded models were added. `lightcraft-codecs`, already used by this crate's tests, is now a runtime dependency for headshot covers.

### Real GPU interaction check

Added and ran ignored `tests_people::people_ui_real_gpu_interactions` with:

```sh
PATH="$HOME/.cargo/bin:$PATH" CARGO_BUILD_JOBS=3 \
LIGHTCRAFT_GPU_BACKEND=vulkan WGPU_BACKEND=vulkan \
VK_DRIVER_FILES=/usr/share/vulkan/icd.d/nvidia_icd.x86_64.json \
cargo +1.98.1 test --offline -p lightcraft-ui-egui people_ui_real_gpu_interactions -- --ignored --nocapture
```

**1 passed**, adapter **NVIDIA GeForce RTX 5090 (Vulkan)**, explicitly rejecting llvmpipe. In the complete app's windowless Headless path, with 2,000 deterministic MockFaces photos: warm People frames **0.20 ms/frame**, open review **14.92 ms**, and Yes plus live re-ranking **16.86 ms**. This measures actual app widgets/session/render workers with the real GPU available, including input handling and subsequent frames; the benchmark measures app logic/layout/input and render requests. Headless screenshots rasterize egui on CPU and are not included in these frame timings. These numbers are fixture interaction timings, not real face inference throughput or desktop presentation/FPS claims. Log: `target/people-gpu.log`.

`LI_SEG_TEST_MODELS` is **unset**. The expected YuNet/SFace ONNX files were not found in the local model/cache locations checked. Therefore `smart_sort::people::tests::real_weight_people_end_to_end` was **not run**. No weights were downloaded. Real-weight inference validation remains for the coordinator when verified weights are provided. No desktop windows were opened and no running app was killed.

## Sources and implementation choices

- [Lightroom Classic facial recognition](https://helpx.adobe.com/lightroom-classic/desktop/organize-photos-in-lightroom-classic/face-recognition.html): named/unnamed people, accepting similar faces, and drag-based reassignment informed the name fields, focused review and merge interaction.
- [Apple Photos: find and name people](https://support.apple.com/en-gb/guide/photos/phtad9d981ab/mac): reviewing additional matches and merging duplicates informed the pre-review duplicate prompt.
- [digiKam People View](https://docs.digikam.org/en/left_sidebar/people_view.html): explicit unknown/unconfirmed/confirmed/ignored states informed the pending badges and Ignore/restore controls. This implementation follows the task's durable rejection requirement.
- Local `docs/specs/smart-sort.md` (§5.8/§5.11 and owner additions 6–11/15/17), `docs/specs/smart-sort-ux-research.md`, and the already merged engine People commands/store/tests are the behavioral source of truth. The documented .60 sure cut-off is retained in the engine.
- Reused this branch's `smart_sort_task.rs`, `smart_sort_keys.rs`, `headless.rs`, Library `widgets.rs`/`theme.rs`, existing `Session::face_job`/variant renderer, host download/folder-picker services, and export/sidecar APIs. Consulted the installed egui 0.36.2 source for popup persistence, drag payloads and scroll viewport geometry. The project's sampled source inventory under `~/.local/share/local-image-dev/engine-sources/` was inspected read-only; no external face-recognition implementation was copied. The shared People layout and worker/result merge are project-specific glue around those existing APIs.

## English strings / translations

Translations remain paused. Static labels/explanations go through `tr` (directly or existing helper widgets). Reused labels include People, Ignore, Ignored, Same, Different, Unpin, Undo, Cancel, Done, Download and Create Folder. New English labels/explanations used by this work:

- Also sort by person (face recognition)
- Finds faces and groups photos of the same person. Face data is stored only in this library and can be cleared at any time.
- Download face models (39 MB); YuNet · MIT; SFace · Apache-2.0; Cancel face analysis; Face analysis is already running
- Analyse faces in this view; Analyse faces in this photo; Imported people
- Add People from Photos…; Paste names…; Paste one name[,title] per line; Add names; Add name
- Minimum photos; Show all; No people to show. Analyse photos or add headshots.
- Pin to top; Merge with…; Not the same person…; Not the same person; Review matches…
- Yes (Y); No (N); Not sure; Not sure (S); Undo (⌘Z); All matches reviewed
- Find This Person…; Find Photos; Any person; Choose people…
- Create a folder for this person; In this folder; People folders; Any of these people; Everyone in the photo together; Also use tag rules; Tagged OR showing
- Include people's names in exported files
- Face recognition; Face data stays in this library. Clearing it does not load face models.; Clear face data…; Clear face data; Also remove names Smart Sort added; Delete stored faces, headshot embeddings and person matches?
- Rule operator labels: any of; everyone

Eight formatted English fallback entries were added to **all 12** `*-formats.json` catalogs, without translations:

- `Skipped {n} headshots without exactly one face`
- `Faces: {done} of {total}`
- `{folder} / {total} photos`
- `? {n} matches to review`
- `Show everyone ({n} more)`
- `Restore {name}`
- `Unnamed person {id}`
- `Merge with {name}?`

The compact painted `? <count>` badge consists only of punctuation and the count. Person/file/folder names are user data. Engine-provided errors and existing plan notices remain the existing engine text.

## Remaining validation / limits

The requested UI is implemented; no requested UI work remains. Builds retain the existing missing craft-fonts warning for Chinese/Japanese glyphs, which was not introduced by this task. The real-weight test remains unexecuted because weights are absent; mock tests and the real-adapter interaction test do not substitute for inference accuracy. Headshot cancellation discards seed results after the in-memory engine importer finishes; it does not interrupt an individual model call. Copy originals preserves the original bytes, including any metadata already embedded by another application; the name opt-in controls newly emitted People keywords.
