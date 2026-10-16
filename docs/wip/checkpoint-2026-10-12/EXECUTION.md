# Authorized v2 completion execution

User authorization, 2026-10-11: implement the accepted plan completely, include UX testing, push the verified result to remote `v2`, and install that verified Linux build on this PC. No additional confirmation is needed for that push or installation. Preserve unrelated remote commits, owner documents, models and settings. Never force-push.

## Sequence

1. Finish/review the active stage-1 recovery worker on `codex/v2-recovery`; it integrates crash/files/dsfix/vector/Develop/contracts and performs the bounded integration gate.
2. Watchdog lifecycle and clean-inactive-cache admission follow-ups are installed. The later coordinator run recorded 39 passing Python regressions; `recovery/queue-installed-audit.json` rechecks all ten installed bin/dashboard hashes against source. Do not reinstall unchanged tools or change wrappers mid-validation. Port reviewed development tools selectively into the v2 candidate before final push. The real cache-only build stall and targeted kernel reclaim are recorded in `recovery/cache-stall-recovery*.json`.
3. Complete state/save/cancellation/colour correctness (all FJ, XC, SF findings not already recovered), then formats/metadata/settings (all FC/FS). Treat triage reports as suggestions until independently verified. Recheck original O/U/F/FB findings against landed reports; no finding silently drops out.
4. Finish consolidated UX: original bugfix-ux plus menus/feedback/dialogs/tooltips; actual interaction tests and rendered UI inspection, including narrow/fractional-scale layouts.
5. Measure original speed proposals and GPU candidates on real hardware in exclusive benchmark intervals. Implement justified changes with equivalence and before/after evidence; record proposals already resolved or disproved.
6. Final exact-candidate formatting, strict Clippy, full supported workspace suite, selected real GPU/AI tests, complete user workflows and package/upgrade checks. Fix real failures; rerun only invalidated evidence until the final candidate gate passes. No ignore/assertion weakening to clear the queue.
7. Review final diff/evidence, fetch remote v2, integrate any remote advancement and revalidate affected behavior, then push fast-forward. Build/package the verified source, retain a recoverable previous installation, install as this user, confirm installed version/commit and a smoke workflow. No secrets, owner media or runtime credentials in commits/packages.

Linux is first. Apple Silicon Mac, Windows, Library scaling and the full translation review are after the core Linux release. Existing feature behavior and readable documents/settings remain supported. Do not expand scope into new feature invention.

## Coordination

Recovery is complete. The user-approved 2026-10-12 parallel replan supersedes the earlier two-owner/six-job limits: at most three simultaneous coding owners with disjoint files, up to eight configured model subagents, and a three-build-lane ceiling with four Cargo jobs per lane, ten global compiler slots and two ordinary test threads. Actual live model slots must be checked after session restart. Keep one persistent cache/toolchain per checkout. Heavy release/LTO/full-workspace validation, GPU benchmarks and AI acceptance are exclusive; extra lanes are for independent light validations only. Follow recovery/PARALLEL-RESTART-PLAN.md and parallel-workers.json. The recurring heartbeat was deleted at the user's request; do not recreate it. Inspect active jobs during ongoing work and report actionable failures/completions. Keep legacy automatic dispatch held; coordinator-owned records cannot auto-launch/reconcile/land.

Validation order for new frozen batches: verify patch/hash ownership and exact-file formatting, then strict affected-crate all-target lint before expensive test code generation; fix lint once at the source boundary. Follow with focused behavior checks, then affected suites at the integration milestone. Keep package/feature selections stable when possible to retain Cargo artifacts; avoid rerunning unrelated heavy GPU/model quality evidence. Test failures still require diagnosis and focused correction, never assertion weakening.

DeepSeek authorization currently covers the two watchdog files only. That review completed. Further DeepSeek source submissions need their own authorized scope; future approved requests have no custom output token or word budget.

## Current evidence

- Active worker identity: `recovery/active-worker.json`; this is runtime coordination data, not release metadata.
- Candidate progress/report: `docs/wip/V2-RECOVERY-PROGRESS.md` and `docs/wip/CODEX-REPORT-v2-recovery-stage1.md` inside the managed recovery checkout.
- Remaining reviewer inventory: `recovery/review-item-inventory.json`; cross-feature/flow findings are tracked separately as XC-01..07 and SF-01..07.
- Watchdog assessment and before/after regressions: `deepseek-pilot/ASSESSMENT.md` and `watchdog-regression-check.txt`.

The active goal stays incomplete until the verified v2 push and local installation both succeed. A worker report or successful compilation alone does not complete it.
