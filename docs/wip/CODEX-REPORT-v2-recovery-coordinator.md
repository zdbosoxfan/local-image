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
