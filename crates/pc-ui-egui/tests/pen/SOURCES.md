# Pen interaction fixtures

Own GPL-3.0-or-later Rust sequences in `main.rs`, `gpu.rs`, `app_acceptance.rs`.
No external images, fonts, proprietary files, or network are used.
The isolated host is 700x320 egui points, a 128x128 transparent document, tolerance 4 px,
with a 300x200 canvas hit rectangle. Real egui pointer button/motion, keyboard and widget
click events drive the production pen adapter. Full-app source uses 1100x800 points,
128x128 white document and physical scales 1/2; it requires package 00 registration.

Sources/behavior and upstream adaptation rows: engine `pen/SOURCES.md` and
`docs/wip/vector-tasks/integration/04.md`. Engine strict GPU parity checks complete
premultiplied document pixels. Isolated GPU test proves visible event-driven preview,
cancellation and commit. Full-app GPU source separately checks the actual application
canvas and must not be reported as passed until it is registered and executed.
