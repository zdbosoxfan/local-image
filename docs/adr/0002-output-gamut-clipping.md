# 0002. Output gamut clipping and scene compression

- Status: Superseded by [0003](0003-blue-band-output-clipping.md)
- Scope: Develop scene compression and output gamut mapping

## Context

Output mapping needs to preserve destination colours already inside the selected gamut,
including colours on detached OkLab hue intervals.

## Decision

Output mapping preserves any colour whose destination RGB is inside the selected output cube.
Only colours outside it move, choosing the nearest valid OkLab chroma below or above the input
at fixed lightness and hue; ties go down. A hue line can leave and re-enter a gamut, so membership
must be tested in destination RGB, independently of the path from neutral. In particular, sRGB blue and
imaginary ProPhoto primaries remain valid output colours.

Membership and boundary search use the same destination cube expanded by an absolute RGB
tolerance of `1e-6`. The final clamp to `[0,1]` runs after mapping, in the output caller.

When no chroma is valid at the input lightness, the mapper first finds the highest feasible
lightness for that hue. Face-crossing partitions and fixed-count bisection are shared by the
CPU and WGSL implementations. A final channel clamp removes numerical residue without a
second luminance-based desaturation.

Output mapping does not soften an interior shell and does not require a continuous derivative
at the boundary. Scene compression retains its exposure-independent working-space knee and
smooth ACES reference compression curve. Scene compression and output clipping serve distinct
purposes and must be tested separately.

## Consequences

Nearest-interval selection can amplify small input differences near the detached blue intervals.
[0003](0003-blue-band-output-clipping.md) replaces this decision in full, including its still-valid
identity, lightness endpoint and separation of scene compression from output clipping.
