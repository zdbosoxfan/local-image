# Selection package sources and fixtures

Upstream reference: VectorCraft at `d522c1d7be4035bd4f4a84cd6ebfca44f5155092`, verified with `git rev-parse HEAD` on 2026-10-10. MIT OR Apache-2.0; existing `licenses/vectorcraft-LICENSE-MIT`, `licenses/vectorcraft-LICENSE-APACHE` and `licenses/vectorcraft-NOTICE` apply. Copyright (c) 2026 ArtCraft Team and contributors. No brand assets or vendor fixtures were copied.

| Read-only upstream file | SHA-256 |
|---|---|
| `crates/tools/src/select.rs` | `205dde08620aaf6d8c0f8aae77186febe51abdb38e3b3a687caea206c2120480` |
| `crates/tools/src/xform/wand.rs` | `bbfeecc9814e4da49562cf5519e632f490029f422c9c8df7d01a401c279edc76` |
| `crates/engine/src/cmd/select.rs` | `1ed85674676469a72c03ebe59ac73214adb49411e636d54dac7a9ef02455f83d` |
| `crates/engine/src/cmd/wand.rs` | `333afa79e94b1ae1016eb5a419a8eb99bdada8c5ec0a1cef15cdd36577189441` |

Adaptation: `selection.rs` follows the select/attribute command families and wand conjunction rules. Unlike upstream RGB-byte distance, this task requires 100× Oklab distance using continuous sRGB values. Matrices/transfer function follow [Ottosson's Oklab definition](https://bottosson.github.io/posts/oklab/), also used by the existing `pc-testkit::vector` oracle. The polygon enclosure is adapted from the upstream wand tool. Segment contact uses ordinary segment intersection/distance with V1 flattening at 0.025 document pixels and V1 contains/bounds/nearest; no second curve-boolean engine is introduced.

Behavior checked against [Adobe Select by characteristics](https://helpx.adobe.com/illustrator/desktop/manage-objects/select-objects/select-objects-by-characteristics.html), [Adobe Group Selection](https://helpx.adobe.com/illustrator/desktop/manage-objects/select-objects/select-object-groups-and-nested-object-groups.html), [Adobe Magic Wand](https://helpx.adobe.com/illustrator/desktop/manage-objects/select-objects/select-objects-magic-wand-tool.html), and [Affinity Select Same/Object](https://affinity.help/designer2/en-US.lproj/pages/ObjectControl/selectByAttribute.html), read 2026-10-10. Affinity's disabled Edit All Layers scope is the active artboard, including nested groups. The task's root-level group fixture uses an explicitly documented sibling fallback when no artboard exists; the nested shape is eligible with Edit All Layers enabled.

All fixtures are original synthetic Rust source under GPL-3.0-or-later, generated in tests; no external corpus is required or claimed. Parameters are explicit in each test: six 10×10 rectangles with exact red IDs and locked/nested variants; widths 1/2/3 document pixels; two ordered fills; open/singleton/empty path components; sRGB gray 0.49/0.51 around an independently computed gray-0.5 ΔE boundary; 256 deterministic enclosure cases. GPU fixtures use 500 4×4 rectangles in a 160×200 document; app fixtures use 500 6×4 rectangles in a 320×240 document. Toolchain Rust 1.98.1, egui/egui_kittest 0.36.2, wgpu 30.0.1, pinned workspace geometry dependencies. No network is used by tests.

No Illustrator/Affinity proprietary files were supplied. Their applications' output is not claimed as a corpus gate. The app test source is in the UI package, and requires the package 00 registration fragment.
