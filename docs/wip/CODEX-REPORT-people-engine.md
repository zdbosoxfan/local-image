# Smart Sort Phase 3 people engine

Continues the merged Phase 1 tagging engine and YuNet/SFace core. No commits, new dependencies,
network access, desktop app interaction, or Smart Sort dialog changes.

## Implementation

1. **Models:** `Group::Faces` has one official Settings row, **Faces**, backed by the SFace
   specification plus a required YuNet companion. Both models are also registered individually
   in `MODELS` with the pinned files, sizes, hashes and URLs. The installed/in-use state requires
   both verified files; download, cumulative progress, cancellation and removal use the existing
   bundle mechanism. Custom models are unavailable for Faces. Added all four label/about strings
   to all 12 Compositing TSVs, OpenCV port rows and a notice. The attributions generator handles
   single-companion formatting and maps both face tasks to opt-in Smart Sort.
2. **Boundary:** `people::FaceTagger` wraps the existing CPU `li_seg::faces::FaceModels` through
   `RealFaces`; `mock::MockFaces` is deterministic and accepts white 8x8 markers followed by
   identity colours, including multiple faces in one photo. No inference or weights are required
   by normal tests.
3. **Analysis/cache:** per-library `faces_enabled` remains default-off. Face-only prepared-input
   batches use the existing full-frame, oriented 1024px raw-preview/render jobs and offline
   thumbnail fallback, half the available CPU threads, cancellation and resumable content-key
   caching (including zero-face records and virtual copies). User rotation and cached-thumbnail
   crop/straighten coordinates map back to the full upright MWG frame. `faces_store.rs` implements the
   `LIFACE1` binary header and key/count/rect/score/128-float records, validated reload, atomic
   append batches, retry after write failure and damaged-cache recovery. Inference filtering,
   alignment and normalisation reuse the existing core. `smartSort.analyze {faces:true}` checks
   opt-in and invokes the face path as well as tagging; face-only analysis needs no CLIP.
4. **People:** atomic `AI/people.json` stores IDs, names, pinned/ignored state, seed/confirmed/
   cluster/rejected face references, centroids, folder opt-in, name suggestions and merge/split
   evidence. Named imported XMP face regions seed matching detections at IoU >= .5. Chinese
   Whispers suggestions are lazy and deterministic. Matches require cosine >= .45 and a .05
   lead; the conservative **sure folder cutoff is .60**. Owner items 8/11 also retain stable
   singleton bubbles, hidden by default but available with Show everyone or min photos = 1.
   Human confirmations own their face and
   refine the centroid; rejected pairs remain excluded. Only accepted identity IDs and named
   `Region { auto:true }` rectangles enter catalog metadata. `Region.auto` defaults to false and
   false is omitted from JSON; all literals were fixed. Imported/manual regions stay intact.
   Identity changes and catalog regions share one undo/redo entry, with atomic people-file
   persistence; vectors never enter catalog snapshots, op logs, XMP, export metadata or responses.
5. **Bubbles:** pinned first, then descending distinct-photo count, stable ID ties; min photos
   defaults to 3 and validates 1..=10. Returns hidden count, ignored/restorable people, lazy limit
   (60 default), pending review counts, selected-folder counts and best face cover (largest rect,
   then score, then key). Headshot covers include their read-only original source path.
6. **Finder/review:** `facesInPhoto` exposes only rectangles/scores/indexes. Find reuses an
   existing bubble when a picked face belongs to it, ranks matches, supports least-sure-first
   queues, and returns probable same-person suggestions before review. Confirm/reject re-rank
   live; merge/split and Different decisions are undoable. Merges retain saved folder selections
   through retired-ID mappings, and union rejections. Splits retain negative evidence; undo
   preserves the ID high-water mark so a later identity cannot reuse an undone person's ID.
7. **Seeds:** a headshot directory is decoded/oriented at analysis resolution; exactly one face
   creates a person named by its file stem. Unsupported, unreadable, zero-face and multi-face
   files are skipped. Repeat imports are idempotent. Seed vectors are stored locally, and seeded
   bubbles appear before gallery matches. Pasted `name[,title]` lines provide deduplicated name
   suggestions without inventing embeddings.
8. **Folders:** catalog `person` rules accept stable numeric IDs (any/all) and case-insensitive
   imported/local names. Local IDs are a serde-defaulted, empty-omitted `Meta.person_ids` field;
   they are not XMP metadata. Folder definitions persist `peopleEnabled`, `personIds`, `everyone`,
   `peopleOr` and `useRules`; any/everyone and tag/rules AND/OR work for unnamed people too.
   Person folders are off by default and created only by explicit opt-in. Presets carry a
   `peopleLayout`; unknown/ignored IDs are skipped with notices, and merged IDs resolve to the
   survivor. An empty people condition never selects everyone.
9. **Clear:** deletes all face-model cache versions and `people.json`, drops models/state and
   sensitive identity snapshots from history. Interrupted atomic scratch files in AI are removed
   too. Tag embedding files remain. Optionally removes only auto regions in one catalog undo
   step; imported/manual names remain. Works after opting out, without loading models.
10. **Commands:** the separate registered `cmd/smart_sort_people.rs` module exposes:
    `facesStatus`, `facesEnable`, `facesAnalyze`, `facesInPhoto`, `people`, `findPerson`,
    `namePerson`, `confirmFace`, `rejectFace`, `mergePeople`, `splitPerson`, `peopleDifferent`,
    `peoplePin`, `peopleIgnore`, `personFolder`, `peoplePasteNames`, `peopleSeedHeadshots`,
    `clearFaceData`, all under `smartSort.`. Registry schemas describe parameters/results.
    Existing status, analyze, classify (face-count gates) and plan commands are integrated.

## Validation

Clippy passed for all six changed crates with `CARGO_TARGET_DIR=target/clippy` and
`--all-targets -- -D warnings`. Formatting checks and `git diff --check` passed. The complete
requested test suite passed: **1,518 passed, 0 failed, 14 ignored**, including 22 new normal
people-engine tests. GPU-dependent cases use their no-adapter skips; ignored weights and
benchmarks were not run.

| Crate | Passed | Ignored |
| --- | ---: | ---: |
| li-seg | 44 | 4 |
| lightcraft-catalog | 74 | 1 |
| lightcraft-engine | 359 | 3 |
| lightcraft-meta (unit + robust integration) | 54 | 0 |
| photocraft-ui-egui (909 unit + 68 integration) | 977 | 6 |
| xtask | 10 | 0 |

All five library doctest targets passed with zero tests. Final test command:

```sh
cargo +1.98.1 test --offline -p lightcraft-engine -p lightcraft-catalog -p lightcraft-meta -p li-seg -p photocraft-ui-egui -p xtask -- --nocapture
```

Clippy used the same six `-p` selections with `--all-targets -- -D warnings`. Each changed
crate was formatted with `cargo +1.98.1 fmt -p <crate>` and the six-crate `-- --check` passed.
`cargo +1.98.1 --offline xtask attributions` completed and regenerated the bundled attributions
(690 dependency entries, plus the model/port additions). `Cargo.lock` and dependency manifests
are unchanged. Logs are in `target/people-tests.log`, `target/people-clippy.log`, and
`target/people-attributions.log`.

Builds use `PATH="$HOME/.cargo/bin:$PATH"`, `CARGO_BUILD_JOBS=3`, toolchain `+1.98.1` and
`--offline`, with one cargo build at a time. Final tests additionally use `RUST_TEST_THREADS=1`
because parallel UI harnesses can consume the global removal-status note used by
`ai_ui::tests::old_models_can_be_removed_from_settings`. On this sandbox, default Vulkan
discovery crashed the existing `adjust_preview_gpu` test with SIGSEGV. Final tests use
`VK_DRIVER_FILES=/dev/null VK_ICD_FILENAMES=/dev/null __EGL_VENDOR_LIBRARY_FILENAMES=/dev/null`
to select no-adapter skips. Diagnostic GL runs identified Mesa llvmpipe: the three Camera Raw
GPU UI tests pass there, but compositor tests require float targets it lacks. Added no-adapter
guards to the three Camera Raw tests which previously panicked instead of skipping. Their
GPU assertions are unchanged; no RTX 5090 validation is claimed.

Normal acceptance coverage includes opt-in/privacy/registry; clustering, frequency/cover/filter/
ignore/pin; naming and catalog rules with undo/redo; face picking and permanent rejections;
least-sure queues, confirmations and folder gating; probable duplicates and merge/split undo;
XMP seeding; any/everyone and AND/OR folders, unknown IDs and merged selections; unnamed people
and layout serde; face-count classification gates; atomic binary/JSON writes and recovery;
per-library persistence; clear-data isolation; cancellation/resume, virtual copies, unreadable
photos, videos and failed-save retries; headshot seeds and pasted names; metadata compatibility;
model bundle registration/removal, allow-listed URLs and a headless **Cancel click** on the Faces
bundle.

Added one ignored engine end-to-end test,
`smart_sort::people::tests::real_weight_people_end_to_end`, using `LI_SEG_TEST_MODELS`. It checks
real YuNet/SFace inference on the reference mother/doubled mother and mountains, then runs real
analysis jobs through caching, finding, naming and catalog regions. It is for the coordinator;
normal tests never download or use weights.

No GPU code or new GPU tests were added: this task uses CPU inference. Added adapter-availability
guards to these existing `camera_raw_scope` GPU tests, which run unchanged on a real adapter:
`both_clipping_warnings_leave_unclipped_preview_pixels_visible_on_gpu`,
`curve_gpu_paints_the_moved_handle_in_every_theme`, and
`pixel_hover_gpu_badge_single_band_and_crosshair_appear_and_clear_in_all_themes`.
Existing GPU-dependent tests remain in the requested suite; the coordinator must run them on
the RTX 5090. The guard change is needed to honor the task's no-adapter skip rule in this sandbox.

## Integration notes

Dialog/face-bubble drawing, Y/N/S keys, UI worker channels and export UI belong to the parallel
UI job. It can use `prepare_inputs` plus `people::detect_input` (including coordinate mapping),
the public face trait/store, `commit_people`, and registry
commands without tract types. Dropping `smart.people.tagger` releases loaded face models.
Names are still not written to MWG XMP by the existing metadata writer; an export dialog's
explicit names option can add `People|<name>` keywords through its export metadata path.
The .60 sure threshold and real-gallery accuracy/performance need coordinator validation.
