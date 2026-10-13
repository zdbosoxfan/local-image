# Match Look v2 plan: synthesis of three research reports (2026-10-13)

Sources: `RESEARCH-matchlook-algorithms.md`, `RESEARCH-matchlook-opensource.md`, `RESEARCH-matchlook-products.md` (all in docs/wip), plus `REVIEW-matchlook-fable.md`. All three point the same way. Owner decisions still stand:
- Analyze workflow
- up to 10 references
- unlimited saved looks
- match the RESULT, not copied sliders
- editable sliders as output
- brush, adjust, confirm unrelated

## Solver v2 (fixes review B7, B5 and the end-points)
1. **Closed-form warm start** (darktable `exposure.c` deflicker/spot idea, GPL-3.0-or-later, port with attribution):
   - exposure from the log-median or percentile ratio between target and reference
   - white balance from a shades-of-gray / neutral-region illuminant gap, skin-anchored when faces exist
   - no fixed per-pass cap: continue until converged or the slider range ends
2. **Tone from a CDF-matched curve** (RawTherapee `rtengine/histmatching.cc`, GPL-3.0-or-later, port with attribution):
   - match luminance CDFs, ignore empty tail bins, build a monotone sparse curve
   - clamp slopes to 0.5–2.0 (the "Exposure" paper)
   - project it onto sliders in a fixed order: Exposure, then Blacks/Whites, then Shadows/Highlights, then Contrast, and curve regions last
   - this targets the L5/L95 end-points directly
3. **Regularised bounded least squares:**
   - Gauss-Newton / Levenberg-Marquardt in normalised slider space
   - a ridge (L2) prior toward the target's current values, with per-slider cost weights and "comfort ranges"
   - an L1-ish sparsity preference so most sliders stay put
   - a smooth bounded reparameterisation (tanh) so values approach limits asymptotically and never pin at ±100
   - Huber loss on statistics, scaled by the reference spread
   - penalty continuation plus coarse-to-fine proxies
   - **Write the small solver ourselves. Do not add `levenberg-marquardt` / `argmin` dependencies.**
4. **Scene-invariant statistics:**
   - luminance quantiles (5/25/50/75/95)
   - luminance-zone chroma (shadow, mid and highlight a/b means and chroma)
   - neutral-region chromaticity
   - skin statistics when faces exist
   - global mean/std only as a minor term
5. **Colour targets** from per-zone or clustered transfer (darktable `colormapping.c` k-means in a/b with soft weights; GPL-3.0-or-later, adapt), used to set colour-grading and colour-balance targets that the sliders are then fitted to.
6. **Multi-reference aggregation:** a per-statistic weighted median across references (similarity weights), with outlier rejection.
7. **Report:** rail-hit rate (sliders within the outer 10% of their range) before and after, and the reviewer's harness metrics:
   - ΔE must stay ≤ about 3 (better if possible)
   - |ΔL5| and |ΔL95| must improve clearly
   - the 500-photo batch stays ≤ about 80 s

## Product and UX additions (ranked, from the products research)
1. **Skin protection:** with faces present (existing People/face data, optional), weight the fit toward skin and limit skin hue shifts. This mitigates the most common user complaint (dark skin tinted or harsh).
2. **Normalize is not scaled by Strength** (as in Capture One). Strength covers the other groups, with a range of 0–125% like Capture One's Impact. Add optional locks for "keep white balance" and "keep exposure".
3. **Compare view:** reference vs result side by side or wipe in the dialog preview, plus per-photo Strength adjustable after apply via the existing layer opacity or a per-photo amount.
4. **Similarity indicator:** show each target's similarity to the look, and warn "Very different from the references — results may be weak".
5. **"What changed" summary per photo,** with per-group reset.
6. **Contrast guard** (caps the contrast share) and a **"Colour only" quick preset** (groups toggled).

Every borrowed source gets a PORTS.md row and a licence note. All UI uses shared design-system widgets.
