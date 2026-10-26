# Rendered-source gamut identity

Status: accepted

Refines the scene-stage decision in [0002](0002-output-gamut-clipping.md), retained by
[0003](0003-blue-band-output-clipping.md). The output mapping decision is unchanged.

Rendered sources already carry their tonal response. Applying the RAW scene knee, which starts
at 0.99 of the Rec. 2020 extent, changes saturated in-gamut colours even with neutral settings.
Their tone maps therefore start gamut compression at the extent itself, at relative chroma 1.0.
Nonnegative working RGB is preserved directly, including detached blue intervals that a search
from neutral cannot recognize as part of its prefix.

For outside colours the zero-width knee is its continuous limiting clip: relative chroma up to
one stays unchanged and values above one land on the extent. This avoids the smooth curve's
zero scale at a knee of one. RAW tone maps retain the 0.99 knee, exposure-normalized extent and
2.5 limit. Converted negatives use the rendered-source tone map.

The tone method carries its knee into the generated GPU finish parameters. CPU and WGSL use the
same membership check and limiting curve. The subsequent tone response and negative-channel
containment remain in their existing order. No persisted settings or catalog format changes.
