# Cutout package 27 sources and fixtures

Own fixtures in mod.rs: lattice checkerboard, three-colour junction, nested counter,
alpha cross, sinusoidal abutting edge, disk on a full opaque canvas, cancellation,
invalid numeric values, inherited locks and stale transactions. Generated in Rust
at test time; GPL-3.0-or-later as Local Image code. No brand assets.

Pixel art: Params::for_preset(PixelArt), speckle=0, precision_loss=0,
layer_difference=0, max_colors=64, fitter=Pixel. Fitted boundary tests use
Potrace/Spline, detail=0.7; topology guard uses flatten tolerance 0.01.
JPEG: own 32px two-colour disk, image 0.25.10 JPEG quality=60, Logo and Photo
preset parameters; tests assert finite/closed/topology-valid rather than inventing
an external fidelity baseline. Alpha/ignore follows T1's alpha>=128 mask policy.

256 randomized 8×8 partitions use proptest's normal case generation and an
independent pc-vector exact-area coverage oracle at 1/2/4/8x. Shared cubic controls
are compared by f64::to_bits in emitted document paths, including open chains.
32 further opaque random partitions exercise fitted Potrace/Spline geometry.
Thread tests use 1/2/4 concurrent std threads; there is no internal thread-dependent
ordering. The whole arrangement falls back together when fitted edges cross.

Benchmark imports T1's existing tests/it/common/mod.rs without changing it:
three original synthetic logos (disk/rounded rectangle/star), 256px, clean/JPEG60/
blur1/noise4/resample/scan; OFL Inter helper remains part of that existing fixture
module but the three-case subset does not use a wordmark. Re-rasterized fidelity
and node comparisons use committed SVGs under tests/references/vtracer-alpha4 and
vtracer-065/logo. SVG generator comments identify versions 1.0.0-alpha.4 and 0.6.5.
The supplied reference SOURCES.md still lacks updated generation parameters and
scores.json; those provenance gaps are explicitly reported. No baseline was
regenerated or fabricated. Measurements go to target/cutout27/competitor-scores.json.
The available 18-case subset is not the full B.4 corpus.

Missing: complete references for six fill presets/sizes and Potrace 1.16, licensed
external CC0/PD clip art and owner scans (no corpus inputs.json), full-app package
28 interaction and package 09 registered scaled-canvas acceptance. No interactive
performance claim is made from library timing.

Algorithm sources read locally at the architecture pin:
- https://github.com/visioncortex/vtracer/tree/928ed0a6f654408e28fb741b6133d4c456bd0160/crates/vtracer/src/mosaic
- https://github.com/visioncortex/vtracer/blob/928ed0a6f654408e28fb741b6133d4c456bd0160/docs/design/mosaic.md
- kurbo 0.13.1 cached official source simplify.rs (optimized fit_to_bezpath_opt,
  angle threshold, endpoint behavior). Online docs fetch failed; cached source
  was inspected. Existing T1 closed Potrace/spline implementations are reused.

## Verified upstream hashes
Tree pin: 928ed0a6f654408e28fb741b6133d4c456bd0160.
- `mosaic.rs` SHA-256 `2e9a71901ddfae748b5216ef47cddcc943c09a51f8109894b5d0836347a31bc6`.
- `mosaic/graph.rs` SHA-256 `2121cfc60110b6c41ec7ed22f5d5076269f554b41bad2928d226d9a61f0bbcf4`.
- `mosaic/face.rs` SHA-256 `eedbb50ba7940f4123f9295836e6a5288fc11141caa9e5d64bfc9f84ff5ca058`.
- `mosaic/fit.rs` SHA-256 `b48ea5970a351ba50291b907c66f1fa334f351db3f10154580cf6c540005fe2e`.
- `mosaic/compose.rs` SHA-256 `3b7a17c7e82812d3801de4baabbf1c3e8e030ba3ea12576fe523cc3e6d5c5d47`.

Gradient merging ports upstream's adjacency/running-area-mean policy at the
same mosaic.rs hash above. Root identity uses the last original paint occurrence
for stable paint order. Explicit palettes and Pixel Art do not undergo near-colour
merging. Open staircase centering follows the upstream symmetric preprocessing
policy; fitted junction endpoints and canvas-border corners stay fixed.

Open fit calls are limited to 256-edge slices, closed T1 fit calls to rings of
at most 4096 lattice points. Larger rings retain exact collinear pixel geometry.
The arrangement guard has a bounded comparison budget and preserves shared
geometry during fallback. These bounds can increase node counts; they are not a
claim that the full B.4 node/performance gates pass.
