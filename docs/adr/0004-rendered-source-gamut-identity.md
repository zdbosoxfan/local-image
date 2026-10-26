# Rendered-source gamut identity

Status: accepted

Refines the scene-stage decision in [0002](0002-output-gamut-clipping.md), retained by
[0003](0003-blue-band-output-clipping.md). The output mapping decision is unchanged.

Rendered sources already carry their tonal response. Applying the RAW scene knee, which starts
at 0.99 of the Rec. 2020 extent, changes saturated in-gamut colours even with neutral settings.
Their tone maps therefore start gamut compression at the extent itself, at relative chroma 1.0.
At that threshold the existing negative-channel containment supplies the boundary clip:
nonnegative working RGB stays unchanged, and outside colours move along a linear RGB ray from
their nonnegative channel average until the lowest channel reaches zero. This is continuous at
every lower face, including detached blue intervals that a radial search from neutral cannot
approach continuously. The scene gamut is a cone without an upper bound; the following tone
response handles intensity.

The tone method carries its knee into the generated GPU finish parameters. At a knee of one,
CPU and WGSL bypass the radial scene compression and use the existing negative-channel
containment. RAW tone maps retain the 0.99 knee, exposure-normalized extent and 2.5 limit.
Converted negatives use the rendered-source tone map. No persisted settings or catalog format
changes.
