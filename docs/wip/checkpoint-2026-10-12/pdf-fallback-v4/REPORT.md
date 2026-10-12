# PDF fallback v4 — custom-modal first-frame ownership

One-file source-only delta atop frozen v3: `correction-from-v3.patch`, SHA-256 `21114d1878f007385ff1d4f55e0994373ea17307356c72cf8227cb57a271e71d` (5,110 bytes). Exact base/final hash and source snapshot adjacent. Apply after v2 and v3. No new shared-file edits, strings, dependencies, builds/tests/runtime/network or commits. Pinned rustfmt, whitespace and reverse-apply checks passed.

The app-aware PDF gate now mirrors existing shortcut-router modal exclusions: generic PC dialogs, Restore, Camera Raw, Develop prompt and embedded Draft. Both PDF show and canvas adapter reserve these owners before their later modal paint. Detached worker output can wait, but cannot publish or consume their input.

A raw Escape withholds PDF publication for its entire pass. PDF cancellation uses only an unconsumed Escape that belongs to the PDF/canvas focus owner. This prevents an Escape already consumed by Restore's early handler from cancelling or accepting the PDF reply in the same close pass. V3 pointer-down and prepass-focus barriers remain.

An authored regression opens actual `ai.restore` with a ready PDF reply before its first modal paint, renders PDF before Restore as the app does, checks no publication, uses Restore's real Escape handler, and then checks acceptance only after the foreground owner releases. Native helper/lease and v3 multipage geometry tests are unchanged. All authored tests remain unexecuted. Frozen for UI re-review; runtime/UX/release acceptance remains coordinator-owned.
