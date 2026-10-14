# Develop Gaussian compatibility fix

The shared raster consolidation changed Develop's plain Gaussian consumers from
the existing three-box approximation to sampled small-radius kernels. This changed
capture sharpening, Upright and saved-settings renders. Four unchanged pipeline
tests failed on the pre-fix baseline. A controlled experiment selecting the existing
box engine restored all four tests and all twenty expected render rows.

This change makes the consumer policy explicit in `li-raster`: the general
Library/editor Gaussian API keeps its sampled default; Develop's core plain-Gaussian
callers select the existing box engine through a thin `lc-pipeline::blur` adapter.
The GPU consumes the same numerical plan and existing box shaders. Fine/eigen-guided
kernels are unchanged. Render-cache invalidation prevents stale thumbnails.

No reference fixtures, expected hashes, thresholds or ignore attributes were changed.
No Match Look algorithm, RAW decoder, dependency or duplicated blur implementation
is included in this fix.

## Validation on the Match Look checkpoint

Base: `04d9c001`, with the twelve source files frozen and hashed before validation.

- Pipeline: 291 passed, zero failed, eight existing ignores. All twenty render
  golden rows and fourteen settings hashes match their unchanged expectations.
- Shared/Library raster: 36 passed, zero failed, one existing ignore.
- Host GPU plan test passed.
- Physical RTX 5090 / Vulkan: 32 Gaussian field cases (one through four components,
  eight sigma values), maximum absolute float difference 7.15e-7 versus CPU.
- Existing full-render GPU regression: eighteen cases, maximum one output code.
- Full `local-image` app check, narrow formatting and `git diff --check` passed.
- Strict Clippy was unavailable: the installed upstream driver rejected Fedora
  compiler metadata with E0514. This is not recorded as a Clippy pass.

The first sandbox GPU attempt failed the physical-device requirement on software
rendering; the subsequent host/device run passed without a skip or software fallback.
These checks establish rendering compatibility, not Match Look quality or throughput.

The independent PR onto current v2 records its base, cache version and fresh validation
below when prepared. It does not require merging the unfinished Match Look feature.
