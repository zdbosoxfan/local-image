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

## Independent v2 PR validation

The fix is isolated onto v2 `d4d24ead`; it contains no Match Look implementation or
RAW changes. Upstream has already advanced the render-cache version to 19, so this
PR bumps it to 20 (the older feature checkpoint used 18).

Fresh baseline versus fix checks on current v2:

| Check | v2 baseline | With fix |
| --- | --- | --- |
| Full pipeline unit suite | 231 pass, 4 fail, 3 existing ignores | 234 pass, 1 fail, 3 existing ignores |
| Unchanged render golden rows | 11 of 20 exact | 19 of 20 exact |
| Fourteen settings hashes | All exact | All exact |

The remaining failure is `tests_toolset::old_settings_load_and_match_detail_goldens`,
specifically `optics + geometry + crop/raw`. Its actual hash is **identical before and
after this fix**, `0xb7a97257711456dc`, while the unchanged expectation is
`0xded90c5d0013a5d1`. Newer v2 lens/geometry work lies outside this PR. No new golden
row fails; eight previously changed rows are restored. No expected value, fixture,
threshold or ignore was edited. The full pipeline suite is therefore not reported green.

Shared/Library raster tests pass (35 total, one existing ignore); the host GPU-plan
test, full app check and narrow formatting/diff checks also pass. Fresh physical
RTX 5090/Vulkan checks pass all 32 Gaussian cases (maximum float difference 7.15e-7)
and eighteen full-render cases (maximum one output code), with hardware required.
A compact machine-readable summary is in `develop-gaussian-validation.json`. Strict Clippy
remains unavailable for the compiler/driver mismatch described above.
