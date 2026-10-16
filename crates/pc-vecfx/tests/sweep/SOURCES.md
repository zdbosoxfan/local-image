# Swept-tip fixtures

Original Local Image GPL-3.0-or-later geometry and tests; no external port or fixture.
The existing 17 calligraphic evaluator is read-only. Its fixed support-normal outline
does not model the complete continuous sweep of differently sized tips, so this module
uses the convex hull of endpoint homothetic ellipses, then V1 union. This is the exact
region for linearly interpolated centre and radius, up to tip polygon approximation.
Tip angle follows 17's counter-clockwise degrees on a y-down document.

Parameters: 96 ellipse sides, size 0.1..1000 px, roundness 0.01..1, angle -360..360°,
pressure modulation 0..1, coordinates +/-1e6, max 1024 samples / 131072 output knots.
A regular 96-gon has area ratio sin(2*pi/96)/(2*pi/96), error <0.072%; tested capsule
area oracle tolerance 0.5%. Geometry uses the integrated Rust kurbo 0.13.1 / linesweeper
0.4.0 adapters. Random tests use an explicitly recorded LCG seed 0x40, multiplier
6364136223846793005, increment 1, 256 cases. Loops, stationary pressure changes and
zero-pressure cases are explicit source fixtures. No proprietary corpus is included.
