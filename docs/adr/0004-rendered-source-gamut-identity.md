# 0004. Rendered-source gamut identity

- Status: Accepted
- Scope: Develop scene containment before the tone response on CPU and GPU
- Refines: scene compression in [0003](0003-blue-band-output-clipping.md)

## Context

Rendered sources already carry their tonal response. Applying the RAW scene knee, which starts
at 0.99 of the Rec. 2020 extent, changes saturated in-gamut colours even with neutral settings.
A radial extent search cannot approach every detached blue interval continuously at the gamut
boundary. Converted film negatives, including RAW negatives after inversion, are finished images
with their own tonal response too.

## Decision

1. **Rendered-source containment.** Nonnegative working RGB stays unchanged up to one f32 rounding step. Outside colours
   move along a linear RGB ray from their nonnegative channel average until the lowest channel
   reaches zero. The existing negative-channel containment supplies this boundary clip,
   continuous at every lower face. The scene gamut is a cone without an upper bound; the
   following tone response handles intensity.

2. **RAW scene compression.** Unconverted RAW tone maps retain the 0.99 knee,
   exposure-normalized extent and 2.5 limit, followed by negative-channel containment.

3. **Explicit mode.** The tone method carries a boolean scene-compression flag into the
   generated GPU finish parameters. CPU and WGSL apply the same fixed scene knee only when
   enabled; rendered tone maps bypass radial scene compression.

4. **Converted negatives.** Film negatives after inversion use the rendered-source tone map,
   including negatives decoded from RAW. Skipping the 0.99 scene knee for these finished
   images is intentional.

## Consequences

Saturated rendered colours stay unchanged up to one f32 rounding step through scene
containment, including blue-edge colours. Converted RAW negatives follow this same rule. Output mapping remains governed by
[0003](0003-blue-band-output-clipping.md). No persisted settings or catalog format changes.

## Related documents

- Output clipping and separate scene compression: [0003](0003-blue-band-output-clipping.md)
- Decision index: [README](README.md)
