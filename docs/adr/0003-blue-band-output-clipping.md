# 0003. Blue-band output clipping

- Status: Accepted
- Scope: Develop output gamut mapping on CPU and GPU
- Supersedes: [0002](0002-output-gamut-clipping.md)

## Context

A constant-lightness OkLab hue slice can contain detached blue intervals. Preserving identity
inside those intervals while preserving exact hue outside them forces a discontinuity. Choosing
the nearest interval still amplifies small upstream CPU/GPU differences at the gap midpoint.
Scene compression and output clipping serve distinct purposes and need separate checks.

## Decision

1. **Destination RGB identity.** Preserve any colour whose destination channels are at least
   `-OUTPUT_EPS * Y(rgb)` and at most `1 + OUTPUT_EPS`, where `OUTPUT_EPS=1e-6` and `Y` uses
   the destination luminance weights. Relative lower slack keeps the boundary shape consistent
   down to black. Identity, exact clipping, ray casts and lightness endpoints use these same
   faces. This includes pure blue and imaginary ProPhoto primaries.

2. **First crossing from gray.** For outside colours take the first boundary crossing walking
   out from gray at fixed OkLab lightness and hue. Bisect each monotone cubic piece 24 times,
   keeping its inside end. If no chroma is valid at the input lightness, first find the highest
   feasible lightness at that hue. The caller clamps numerical residue to `[0,1]` after mapping,
   without a second luminance-based desaturation. No interior shell is softened and no continuous
   derivative at the boundary is required.

3. **Blue-band blend.** Around the padded sRGB hue window `[-1.674503,-1.671768]` and Rec.2020
   window `[-2.005771,-2.001952]`, blend this clip toward a three-cast linear RGB ray trace.
   The blend is one within 0.01 radians of a window and falls by smoothstep to zero at 0.10
   radians. Cast from gray at the clamped input OkLab lightness into that same destination
   volume; after each cast reproject its chroma to the input lightness and hue. Use the third
   boundary point. CPU and WGSL share window constants and fixed iteration counts.

4. **Separate scene compression.** Scene compression retains its exposure-independent working
   gamut knee and smooth ACES reference compression curve. Its application to rendered sources
   and converted negatives is refined by [0004](0004-rendered-source-gamut-identity.md).

## Consequences

The blue-band blend trades at most 0.045 radians of hue and 0.001 of OkLab lightness for
continuity, with sampled chroma drops limited to 0.001 there. Other hues retain their existing
checks. Display P3, Adobe RGB and ProPhoto use the exact clip without this blend. In-cube colours
remain identical.

## Related documents

- Rendered-source scene containment: [0004](0004-rendered-source-gamut-identity.md)
- Decision index: [README](README.md)
