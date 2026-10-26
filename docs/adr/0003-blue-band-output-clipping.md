# Blue-band output clipping

Status: accepted

Supersedes [0002](0002-output-gamut-clipping.md) for out-of-gamut chroma selection.

A constant-lightness OkLab hue slice can contain detached blue intervals. Preserving identity inside those intervals while preserving exact hue outside them forces a discontinuity. Choosing the nearest interval still amplifies small upstream CPU/GPU differences at the gap midpoint.

Keep identity when every destination channel is at least `-OUTPUT_EPS * Y(rgb)` and at most `1 + OUTPUT_EPS`, where `OUTPUT_EPS=1e-6` and `Y` uses the destination luminance weights. Relative lower slack keeps the boundary shape consistent down to black. Identity, exact clipping, ray casts and lightness endpoints use these same faces. For outside colours take the first boundary crossing walking out from gray, bisecting each monotone cubic piece 24 times and keeping its inside end. Keep the existing lightness endpoint and scene compression.

Around the padded sRGB hue window `[-1.674503,-1.671768]` and Rec.2020 window `[-2.005771,-2.001952]`, blend this clip toward a three-cast linear RGB ray trace. The blend is one within 0.01 radians of a window and falls by smoothstep to zero at 0.10 radians. Cast from gray at the clamped input OkLab lightness into that same destination volume; after each cast reproject its chroma to the input lightness and hue. Use the third boundary point. CPU and WGSL share window constants and fixed iteration counts.

This trades at most 0.045 radians of hue and 0.001 of OkLab lightness within the blue band for continuity, with sampled chroma drops limited to 0.001 there. Other hues retain their previous checks. Display P3, Adobe RGB and ProPhoto use the exact clip without this blend. In-cube colours, including pure blue and imaginary ProPhoto primaries, remain identical. The caller retains the final numerical clamp.
