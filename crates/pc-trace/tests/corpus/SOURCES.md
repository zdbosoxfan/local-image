# T1 deterministic corpus

`tests/common/mod.rs` generates 30 original logos at test time at 256, 512 and
1024 pixels. Input geometry and generator are Local Image originals; the fixture
images/shapes are dedicated to CC0-1.0. Code follows the workspace GPL-3.0-or-later
licence. No generated bitmap is committed. The corpus is below 5 MB.

Each logo has clean AA, JPEG quality 60, Gaussian sigma 1, deterministic noise
sigma 4/255, half-size down/up sampling, and a scan variant (0.7 degree rotation,
paper noise and uneven lighting). Clean vectors are rasterised with `pc-vector`;
every degradation is compared to this clean render. The shapes include circles,
rounded rectangles, stars, arrows, counters, overlaps, and rotations. The OBO
wordmark comes from the committed `assets/fonts/Inter-Regular.ttf`, SIL OFL 1.1
(see `assets/fonts/OFL-Inter.txt`), outlined with the already-locked ttf-parser.

The normal suite runs six images across all six T1 fill presets, one image for each degradation. The ignored full
suite runs all cases for Logo, Few Colors, Photo and Pixel Art, and the binary
subset for B&W/Silhouette. It also includes supplied external cases, compares both Potrace and spline fitters, records per-preset median summaries, and writes
`target/trace-bench/scores.json`. Fitter defaults remain **provisional** until
competitor goldens are supplied: Potrace for Logo/B&W/Few Colors/Silhouette,
spline for Photo, exact pixel boundaries for Pixel Art. The full run records both
candidates so the coordinator can choose the best median fidelity and node count.
Do not call this a benchmark-selected default before running those comparisons.

The coordinator has not supplied the specified external 15 CC0/PD clip-art SVGs
or five licensed pixel-art/line-art scans. Add those here with a per-item URL,
author and licence before the complete merge benchmark. No stand-in is labelled
as external ground truth. `external_corpus` in benchmark.rs renders SVGs with
resvg and checks optional licensed input-only PNGs; `inputs.json` lists their licences.

No reference SVG or score is fabricated. See ../references/SOURCES.md.
