# Task: verify and fix DeepSeek review findings (post-ds batch)

DeepSeek reviewed every Rust file changed since commit 438dc96b (Restore, AI Denoise, Enhance/Upscale, icons,
Overwrite Original, Z-Image Variety, fix4). Findings (83: 25 high, 49 medium, 9 low), one JSON per line with
file/line/severity/summary/evidence/fix:

  /home/zdavidson/.local/share/local-image-dev/ds-review-post-ds/findings.jsonl

For EACH finding:
1. Read the code and decide REAL / NOT REAL (DeepSeek is often wrong; e.g. a claimed compile error that compiles is NOT REAL).
2. Fix every REAL one minimally, with a regression test where practical (panics from crafted files/ONNX graphs,
   shared model file deletion, metadata loss in tiff_replace, overwrite-original marking a layered doc as saved,
   PSD warnings bypassing the damaged-original check, progress thread hang on panic, external_only URL, etc.).
   For examples/ (dev-only eval tools) just add clean errors instead of panics; keep it small.
3. Do not change behaviour beyond the fix. New user-visible strings go into the i18n pending/KEEP_AS_IS lists.

Write docs/wip/CODEX-REPORT-ds2-fix.md: a table of every finding with verdict, one-line reason, and fix/test.
Gates: cargo +1.98.1 fmt; strict clippy (-D warnings, all targets); tests for the touched crates. Leave changes
uncommitted; the coordinator merges. Never force-push. Do not open GUI windows or kill the running app.
