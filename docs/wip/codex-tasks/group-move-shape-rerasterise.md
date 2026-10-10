# Codex task: moving groups with stroked shapes re-rasterises them every frame

From the opus-scan pass (branch `wip/opus-scan`). All Rust, no new dependencies, no `unsafe`, no GUI
windows, `cargo +1.98.1 … --offline`.

## Problem and evidence

`layout_bench` (whole app, GPU canvas, RTX 5090; 4000×3000 designer layout, 174 layers, 33 stroked
shapes), after opus-scan commit eaa474ee fixed the split-cache thrash:

| Row | median | p90 |
|---|---|---|
| Move group: drag frames | 19.0 ms | 21.1 ms |
| Move group: release (commit) | 25.6 ms | 26.4 ms |
| Move pixel/text/shape/smart: drag frames | ~0.9 ms | ~8 ms |

gdb samples of `layout_bench --direct --spin move-group` put the drag frames in
`pc-gpu/src/plan.rs` `effects` → `pc-compose/src/shape_split.rs::split` →
`photocraft_vector::render_shape` (+ `fit_stroke` → `bounds::content_bounds`). Moving a group moves
its shapes' paths, so `split`'s key (Debug text of the path) changes every frame and each stroked
shape in the group is rasterised again over the **whole canvas**.

## What to do

- Make the split cache translation-aware: key on the path relative to its own integer-pixel origin
  (plus the sub-pixel fraction, fill, stroke, live, and the shape cache's identity), and translate
  the cached fill/stroke surfaces by whole tiles/pixels on a hit (or have `plan.rs` use the
  layer's offset the way it does for pixel layers). Rasterise only over the shape's bounds grown by
  the stroke (not the whole canvas) — `render_shape(&…, fmt, canvas)` currently takes the canvas.
- Keep the result bit-identical to today's for the same final position (the CPU compositor's
  `split_parts` uses the same function; see `pc-compose/src/stroke_tests.rs`).
- Avoid building the key with `format!("{:?}", …)` of whole paths on every lookup (hash the fields
  directly).

## Tests

- `shape_split` unit test: a stroked shape moved by (dx, dy) whole pixels hits the cache (counter,
  as in `many_stroked_shapes_stay_cached_across_rebuilds`) and equals a fresh rasterisation.
- Existing `stroke_tests.rs`, pc-gpu plan tests, and the GPU image tests in
  `crates/pc-ui-egui/tests/gpu` stay green.
- Re-run `cargo run --release -p photocraft-ui-egui --example layout_bench -- --reps 5` and record
  the Move group rows (target: drag frames < 8 ms like the other layer kinds).
