# Package 34 attribution rows for lane 00

Existing retained notices: licenses/vectorcraft-LICENSE-MIT, licenses/vectorcraft-LICENSE-APACHE,
licenses/vectorcraft-NOTICE. No notices or brand assets replaced. Append one row per adapted file:

| Our file | Upstream | Commit | License | Port date / adaptation |
|---|---|---|---|---|
| `crates/pc-engine/src/vector_packages/transform_tools.rs` | [VectorCraft](https://github.com/storytold/vectorcraft), `crates/engine/src/cmd/{object,menucmds,path}.rs` and `crates/tools/src/xform/transform.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10; pivot affine formulas, seeded Transform Each, quartic Reshape falloff; transactions adapted to Session/layers |
| `crates/pc-ui-egui/src/vector_packages/transform_tools.rs` | [VectorCraft](https://github.com/storytold/vectorcraft), `crates/tools/src/xform/{transform,free}.rs`, `crates/tools/src/extra.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10; click pivot, Alt-click/Copy, current modifiers, constrained reference/free handle drags, egui integration |
| `crates/pc-engine/src/vector_packages/transform_tools/contours.rs` | Local Image, `crates/pc-engine/src/vector_packages/path_text/curves.rs` | branch baseline `ab753378` | GPL-3.0-or-later | 2026-10-10; established painted-outline contour adapter retained in the owned module because the original adapter is private; 00 may expose the shared adapter |

`pc-vector/src/projective.rs` and metadata/conversion/dialog/fixture modules are original Local
Image implementations, using already-attributed geometry/text/render providers. No source
from proprietary Adobe/Affinity software was copied; official docs describe interaction only.
