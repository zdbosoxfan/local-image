# V2 recovery — coordinator review and correction

The saved crash/files/dsfix/vector/Develop work and contract additions are integrated into the v2 candidate. The original source worktrees are unchanged: all eight source HEADs, tracked patches and untracked-file inventories/hashes match the startup snapshot in the coordinator preservation audit. Original WIP and the interrupted worker's final working state are backed up.

The CLI worker was stopped after independent review rejected its V484 gradient interpretation. It had changed the compositor to follow the recovered bad vector interpretation, then changed an existing fixture to make that interpretation pass. These are not accepted as correct behavior. The document model explicitly defines midpoints over sorted stop intervals, consistent with the prior compositor and PSD/command semantics.

## Accepted correction

Restored `gradient_fill.rs` (both implementation and its original ramp fixture) from pre-repair candidate fbf779e8. Paint::from_fill now stably sorts stops only, retaining the midpoint vector and its implicit 0.5 defaults. Geometry, opacity, dithering and the existing shape/fill parity test/tolerance remain unchanged. Retained the separate titlebar fixture cache-retirement cleanup.

TEST-CHANGED: renamed/corrected the newly recovered V484 test because its stop-associated midpoint premise contradicts Fill's documented sorted-interval contract. It now checks both intervals independently at their half-colour positions; additional permutation and short-vector cases catch discarded/misassigned midpoints. No original compositor assertion or tolerance was weakened; the worker's changed fixture was restored.

## Fresh coordinator validation

| Evidence label | Actual result |
| --- | --- |
| coordinator-gradient-contract | 23 passed, zero failed/ignored; 15.01 seconds including build |
| coordinator-gradient-components | 262 passed, zero failed, one existing perf_6016 benchmark ignore; 4.08 seconds |
| coordinator-gradient-export | 3 passed, including Poppler PDF and resvg SVG independent gradient oracles; 33.42 seconds including build |
| coordinator-gradient-gpu | One named shape-gradient/stroke/clip/effect parity test passed on RTX 5090/Vulkan; 22.64 seconds including build |
| coordinator-gradient-lint | Strict all-target Clippy for both corrected components, exit 0; 3.66 seconds |

Exact command vectors, source/lock fingerprints, environment, full logs and hashes are in `v2-recovery-validation/`. The new installed gate accounts for clean inactive file cache; the lifecycle-safe watchdog and managed-worktree dashboard update were installed only after the CLI service stopped. All 39 queue-tool tests passed. The coordinator validation records capture the new wrapper/runner hashes separately from historical worker evidence.

## Recovered evidence and limits

The original 38-package union reported 9,723 passes, two failures and 145 ignores; it is NOT an all-green release result. The titlebar cleanup failure was resolved and passed focused/closure validation. The coordinator correction above replaces the worker's incorrect gradient repair. Earlier workspace format/all-target check and scoped lint records, focused recovery cases, two explicit Affinity synth fixtures and required hardware import/live-paint checks remain preserved with their actual revisions. Counts are test executions, not automatically distinct regression intents.

Contract tests now run 36 active passing cases, with seven former ignores re-enabled after explicit probes. Four PSD fidelity cases remain assigned to formats work: text postscript-name normalization, separate shape-mask editable representation, and smart/Develop all-white filter masks. V132 export payload work remains assigned to UX/export. Do not treat these as solved by this recovery milestone.

Proceed to the remaining correctness, UX and performance work. The full Linux release gate, v2 push and local app installation are still outstanding. This report is an interim recovery sign-off for continued development, not completion of the user's app goal.

## Subsequent correctness milestones

These are bounded implementation gates; final Linux release and real app UX acceptance remain outstanding.

- `21c4fd63` — FJ-001: group disclosure changes preserve background job results and current disclosure state. Real edits and cancellation still reject results. Reproduced before fixing; 20 focused passes / one existing ignore; strict engine lint passes.
- `75612bbc` — FJ-007 / XC06: Don't Save no longer publishes Smart Object child edits into the parent. Multi-document Cancel preserves recovery, changed revisions are asked again, and native shutdown joins requested recovery cleanup so in-flight writers cannot resurrect discarded copies. Immediate Save then Quit also clears clean recovery without another frame.
- `3812afda` — O-057: the production Rasterize dialog validates captured document, revision and selection before rasterizing or painting. The combined state/dialog gate passed 44 tests; strict all-target engine/UI/format/native-app lint passes.
- `22266499` — FC-005: first VP8 partition lengths beyond 524287 bytes return a typed encoding error instead of a clamped corrupt-success header. Exact boundary and bounded frame assembly tests plus existing independent lossy WebP decoder/quality checks: 25 passes; strict lint passes.
- `caf03be3` — O-051: model catalogue planning and installation reject traversal/platform path prefixes before IO and validate existing symlinks against the chosen models root. Thirteen browser/install tests pass. TEST-CHANGED: the mock browser fixture closes its pooled HTTP client before stopping the server, then verifies the exact cached body after a real failed connection. This removes a stale keep-alive timeout: the same test group fell from 61.20 seconds to 0.26 seconds without dropping assertions. Strict lint passes.
- `ac96a6d8` — FJ-006 / O-052: transient cancellation failures retain the owned ComfyUI prompt and retry; cancelled completed output is rejected. UI Stop only cancels requested jobs. Generated images remain in hidden, owned staging until Session accepts the job on the UI thread; dropped cancelled payloads delete their staging, while publication atomically renames all entry files together. Completed edits preserve the viewed document and its live lasso. Focused evidence: 2 retry/mock tests, 4 desktop cancellation tests, 6 terminal-state tests (including the real 30-second lost-prompt regression), 7 publication/metadata/Stop tests, and 3 final active-document/gesture tests. These counts overlap and are not a count of distinct regression intents. Strict all-target desktop/AI lint passes.

All evidence labels are retained in `v2-recovery-validation/`: `state-group-view-*`, `state-discard-*`, `formats-vp8-boundary`, `ai-model-destinations*`, `formats-ai-safety-lint`, `ai-cancel-*`, `ai-publication-*`, and `ai-cancellation-publication-lint`. Early failing compile/lint attempts remain recorded; they were corrected before the stated successful gates.

Still open: library/document origin and history isolation, remaining colour boundaries, stalled download cancellation, owned AI server startup, mask targeting, remaining format/persistence cases including four PSD contracts, consolidated UI, measured performance, complete Linux validation and real RTX/desktop UX, packaging and upgrade checks, authorized normal v2 push and rollback-preserving installation. Neither a push nor installation has been performed at this checkpoint.

## Library task and Camera Raw history isolation

- `d5db3b90` — FJ-005: imports, scans, export completion and delayed catalog callbacks capture the originating Library generation. Stale results cannot add records, change selection/history, or run completion actions in a replacement catalog. Switching through the UI refuses an active import/export and clears old import reviews/scans. A queued Move batch owns rollback until commit, including receiver-drop and late-send races; its original remains byte-identical and its uncommitted placed copy is removed.
- `3e772cf9` — XC02: Camera Raw Undo/Redo only reaches filter operations. Library Redo survives filter edits and closing a live filter interaction; background Library commits preserve filter Redo and remain independently undoable after close. Mixed-scope operations fail before catalog mutation, and filter history capacity cannot evict Library history. Menus and command/query availability use the same scope.
- Evidence: `library-origin-focused` 7 passes; `library-history-focused` 27 passes including existing related controls; `library-history-menu` 11 passes including the production menu path; `library-isolation-lint-final` strict all-target desktop lint passes. One test setup helper initially violated the unwrap lint; it now returns Result. No assertions were removed or weakened.
- `0888291c` adds explicit per-validation LIGHTCRAFT_LIBRARY isolation and includes font environment values in evidence records. Existing focused fixtures use temporary files and in-memory stores; broader upcoming runs cannot fall back to the owner's default Pictures library.

Persistent Library identity in Develop links, source/virtual-copy matching, host previews, pending saves and return requests (XC01) remains open. A second coding owner is handling only FC002/003/006 TIFF/JPEG metadata preservation on disjoint files; the coordinator exclusively owns builds/tests and will review that result. Full release, native UX, push and installation remain outstanding.

## Metadata preservation and persistent Library identity

- `76745237` — FC002/003/006: raw TIFF ASCII retains type/count/bytes; TIFF and JPEG overwrite reconcile orientation/dimensions without replacing foreign XMP or relocating JPEG MakerNote payloads. Extended XMP is bounded, reassembled, normalized and rechunked with coherent links and content GUIDs. Malformed or incoherent metadata fails before returning replacement bytes. Independent review found four XML semantic/RDF-scope gaps; all were corrected and re-reviewed before acceptance. This does not close the separate TIFF MakerNote relocation issue FC010.
- `2e28e97e` — XC01: persistent catalog UUID plus a lossless storage-location hash qualifies photo/virtual-copy identity. Develop links carry their original source through native/private layered-format metadata, including PSD embedded-source UUIDs. Open/sync job keys, previews, queued save/overwrite requests and Develop return requests honor those identities. Virtual copies do not cancel each other's jobs. Existing-tab opens supersede stale pending source opens. Save origins are captured before Save Document scripts can change tabs. Foreign or source-less legacy links cannot silently switch to another Library's photo. Legacy links remain readable and unbound; moving/copying a catalog location requires deliberate relinking rather than implicit adoption. Unreadable identity data is retained with a warning and a temporary nonreconnecting identity.
- `929f0349` — additionally, Save Document scripts that close/reorder tabs cannot retarget automatic image-asset generation. The saved DocId is re-resolved after callbacks; a closed saved document produces no assets for another tab.

Final evidence includes `metadata-tiff-roundtrip` (16), `metadata-xml-review-regressions` (56), `metadata-jpeg-and-origin-roundtrip` (16), `library-identity-desktop` (19), `library-identity-review-regressions` (22), `library-identity-legacy-final` (7), `library-identity-native-roundtrip` (2), `identity-final-behavior` (9, including save-script asset routing and GUID known-answer vectors), and `identity-final-format-roundtrips` (5). These are overlapping execution groups, not distinct bug counts. `identity-metadata-lint-accepted` passes strict all-target lint across all affected metadata/document/engine/UI/app packages. The final shared identifier representation and mechanical fixed-size chunk changes are covered by the final behavior/roundtrip groups.

The Extended XMP digest convention is corroborated by [Adobe's XMPUtils packageForJPEG documentation](https://developer.adobe.com/experience-manager/reference-materials/cloud-service/javadoc/com/adobe/internal/xmp/XMPUtils.html): the digest covers the serialized extended packet. The implementation uses MD5 only for that file-format identifier; independent standard vectors and integration fixture digests verify it.

The metadata coding owner is now assigned FC009/010 and the four outstanding PSD editable-persistence contracts. Root owns remaining colour/AI/clipboard/UI work. Builds remain serialized. Complete supported-workspace validation, real hardware/native UX, packaging, normal v2 push and rollback-preserving installation are still required.


## Colour boundaries, editable PSD contracts and TIFF safety

- `7fcdb6b1` (XC07/SF01): AI receives sRGB after document-profile conversion; model output converts into document space with alpha unchanged. Remove/fill/background/enhance, composite and selected-layer references, file references, batch cutouts, generated placement and restoration use the boundary. Selected-layer rendering retains document profiles/patterns/light. Prompt-only Create avoids unrelated document CMS errors. Unsaved Library previews convert before quantization and report failed revisions once.
- `55c6dd34` (FC009/PSD contracts): opaque TIFF tagged blocks and unknown shmd items retain exact source-order payloads in bounded envelopes. Native editable text/simple masked shapes use bounded, digest-bound records beside standard PSD data; external edits invalidate stale records. Explicit white smart-filter masks survive. All four formerly ignored PSD contracts now pass. Independent review added pre-stroker dash/geometry budgets and actual-ink admission so a tiny shape on a 24MP canvas stays editable without allocating the whole canvas; full canvas gradient coordinates remain intact.
- `1344acbc` (FC010): safely refuses opaque TIFF MakerNote rewrites rather than relocate unknown pointers, silently drop metadata, or retain old cropped/redacted image bytes. Vendor-preserving overwrite is still unsupported in these cases; original remains untouched and the error suggests an edited copy. The rejected whole-file append proposal is not shipped. All overwrite output, including PSD/PSB, must fit the app's 512MiB reopen limit.

Evidence: `colour-boundary-reviewed` 9 pass; `colour-affected-workflows` 44 pass / 2 explicit real-model ignores; `tiff-safe-overwrite-metadata-v2` 3 pass; `tiff-private-blocks` 24 pass; `psd-editability-admission` 4 pass; `psd-contracts-and-overwrite-reviewed` 8 PSD +12 overwrite pass; `tiff-layered-persistence` 22 pass. These groups overlap. `colour-psd-tiff-strict-lint` passes affected all-target strict lint; scoped formatting/diff checks pass and source hashes remain unchanged afterward. Initial test corrections retained assertions: numeric zero tolerance for -7.49e-19 matrix rounding; consistent import filename in the new full-model roundtrip fixture.

The workflow group rendered on the RTX5090 through Vulkan, and representative 1280x800 dark /1920x1080 light Draft/Refinement screenshots were inspected. The compact refinement History row needs a layout correction in the forthcoming UX pass. Harness rendering is not final native/model acceptance: two real restoration tests still require prepared exports/downloads, the NASA fixture and idle ComfyUI. Full release gates, normal v2 push, backup and local installation remain outstanding.


## Stalled downloads and structured creators

`48df02a5` (FJ003) polls raw TCP receives below verified TLS, retains whole-body deadlines across slow TLS fragments, preserves CONNECT/proxy authentication and NO_PROXY, and never replays partial writes. Unique partial files have RAII cleanup; atomic hard-link publication cannot replace a racing destination. The final cancellation check defines commit start. Unsupported hard-link filesystems fail safely; DNS/connect/request writes have finite setup limits rather than a universal subsecond guarantee. Tests use only loopback with a public test certificate/key; the sole dependency change is a test-only edge to existing locked Rustls.

`e5d13ce9` (FC004) retains ordered XMP creators beside scalar display across catalog reopen, metadata merge, existing/new sidecars and export. Literal semicolons are never split. Deliberate scalar/preset edits clear the snapshot even at the same display text, while inverse operations preserve undo/redo. Empty new fields default/omit for existing files.

`download-cancellation-final`: 20 pass, including a measured 156.8ms stalled-body cancellation, clean retry, successful direct TLS and HTTP/HTTPS CONNECT, certificate rejection, no output replay, whole-body budgets, proxy cancellation, no-replace race, integrity failure and unwind cleanup. `creators-metadata`: 60 pass; `creators-catalog-sidecars`: 27 pass. Independent reviews found no remaining concrete issues after the NO_PROXY/setup-deadline corrections. `downloads-creators-strict-lint-final` passes all affected all-target lint; formatting/diff checks pass. Two preceding lint attempts required equivalent contains_key/redundant-borrow cleanups only. The narrower creator test run is not font/UI evidence; final integration lint and future launcher defaults require the pinned production fonts.

Next active work: O050 owned ComfyUI process lifecycle and FS003 durable corrupt-preference backup. Full release acceptance remains pending.


## Owned ComfyUI processes and preference recovery

`ebf78bb8` (O050) serializes ownership across spawn, handle storage, stop and reaping. A live child cannot be replaced by a second start; failed starts release the reservation; exited children are reaped; failed stop/wait retains ownership. Tests use harmless local children and include concurrent starts and shutdown during a held spawn. No external ComfyUI is killed or adopted.

`2610dff4` (FS003) keeps exact damaged JSON/UTF8/invalid-field bytes in private durable backups before replacement, with a persistent dismissible recovery-path notice. Ordinary read failures remain distinct and block writes. When a transient read succeeds, local changes rebase onto the saved settings in both Session and merge baseline; a second save preserves loaded shortcuts/unknown fields. Exact-byte regular-file matching reuses completed backups across repeated recovery-record failures; symlinks are not reused. New partial-write failures clean only their own incomplete file, leaving the source and pre-existing backups untouched. Complete unsynced bytes remain available for verified resync.

`process-preference-safety-final`: 37 pass, no ignores. `process-preference-strict-lint-final`: strict affected all-target lint passes; formatting/diff checks pass with unchanged final source hashes. The initial build exposed a missing direct serde dependency in the native app; the existing workspace package was declared without adding a package/version. Independent review gaps were corrected before acceptance.

The installed development dashboard now shows the current v2 coordinator/validation and labels historical CLI work accurately; seven focused tests pass and browser verification confirms the new state. The automatic queue remains held. Root continues clipboard colour policy; the formats owner handles bounded SVG compatibility. Full native UX, release/hardware gates, normal v2 push and local installation are still pending.
