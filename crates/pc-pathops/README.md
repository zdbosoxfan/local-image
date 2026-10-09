# pc-pathops

Constructive geometry for `photocraft_doc::Path`. Coordinates remain document pixels;
results remain editable knots and are painted by `pc-vector`.

```rust
use photocraft_pathops::{boolean, BoolOp};
let union = boolean(&back_path, &front_path, BoolOp::Union)?;
```

The document APIs return `Result` for invalid coordinates/options. They include
`finish_compound`, `boolean`, `pathfinder`, `regions`, `shape_builder`, `merge_regions`,
`offset_path`, `outline_stroke`, `simplify`, `smooth`, `reverse`, `join`, `split_at`,
`fit_path`, `area`, `contains`, and `nearest`. `Path::bounds()` is in `pc-doc` and computes
curve extrema with kurbo. Flattening uses the existing pc-vector tolerance contract.

`geom` and `kernel` expose the adapted upstream calculation types and algorithms for
specialized callers and ported tests. They are transient data, never a document tree or
file format. `to_geometry` maps coordinates and handles without changing them;
`finish_compound` resolves the document's ordered PSD component operations before
filled-area algorithms. Geometry outputs have nonzero winding, a leading `Combine`
component and `Join` contours for holes. Using independent `Combine` operations for
holes would fill them in the PSD-calibrated rasterizer. Inverted geometry requires a
finite clip; the engine supplies the document rectangle. Open fills close with a
straight edge, matching pc-vector; unused endpoint handles remain intact for editing.
The document adapter uses fallible kernel operations so sweep failures propagate as
errors before a command mutates its input layers.

The native `outline_stroke` adapter uses pc-vector's stroke polygons, including its
PSD width-relative dashes, cap/join tessellation and closed-path alignment. The
VectorCraft curve stroker remains available as `kernel::outline_stroke`. This avoids
changing established Photoshop stroke coverage when expanding a native stroke.

Engine commands (all journaled, one undo step):

- `path.boolean { layers: [ids], op: "add|subtract|intersect|xor|divide", keep_compound: false }`
- `path.pathfinder { layers: [ids], op: "trim|merge|crop|outline|minusBack" }`
- `path.outlineStroke { layers: [ids] }`
- `path.offset { layers: [ids], delta: 10, join: "miter|round|bevel", miter: 4 }`
- `path.simplify { layers: [ids], tolerance: 0.5 }`
- `path.smooth { layers: [ids], amount: 0.5 }`
- `path.reverse { layers: [ids] }`
- `path.join { layers: [ids], tolerance: 0.5 }`
- `path.splitAt { layers: [ids], subpath: 0, segment: 0, t: 0.5 }`
- `path.finishCompound { layers: [ids] }`

Each returns `{ layers: [result_ids] }`. Omitting `layers` uses the current selection.
Layer order follows document stacking, rather than caller order. All options and
geometry are validated before the edit. Locked or non-shape inputs are rejected.
Constructive results inherit the bottom object's appearance. Expanding a stroke
moves its paint/opacity into a fill. An existing fill and its outline become children
of an isolated group, retaining the original layer opacity/masks/effects once. Live-shape and stale PSD shape blocks are cleared.
Divide and split select every result; undo/redo restores their history states.

The source is adapted from VectorCraft (MIT OR Apache-2.0), pinned to
`d522c1d7be4035bd4f4a84cd6ebfca44f5155092`. Copyright (c) 2026 ArtCraft Team and the
VectorCraft contributors. Source file pins are in `docs/PORTS.md`; license texts
and the upstream notice are in `licenses/vectorcraft-*`. No renderer, text engine,
document tree, brand assets or entire upstream crates are vendored.
