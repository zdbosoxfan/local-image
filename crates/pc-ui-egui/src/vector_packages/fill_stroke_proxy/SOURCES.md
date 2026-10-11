# Package 46 sources and fixtures — 2026-10-10

Baseline inherited: 529619af, merged wip/vector. There were no uncommitted implementation
files for 46 to discard; existing persona, flyout shell, transaction and icon loaders were
retained. This task's brief was the only untracked input.

- VectorCraft `crates/tools/src/catalog.rs`, commit
  `d522c1d7be4035bd4f4a84cd6ebfca44f5155092`, inspected in the owner's sampled
  `~/.local/share/local-image-dev/engine-sources/vectorcraft` tree. MIT OR Apache-2.0;
  copyright 2026 ArtCraft Team and contributors, retained in layout module and existing
  licenses. Order/keymap data adapted, no vendor geometry copied.
- Official [Illustrator shortcuts](https://helpx.adobe.com/illustrator/using/default-keyboard-shortcuts.html)
  (page updated 2026-02-17, inspected 2026-10-10) and
  [tools](https://helpx.adobe.com/illustrator/using/tools-in-illustrator.html).
  Official [Affinity Tools panel](https://affinity.help/designer2/en-US.lproj/pages/Panels/toolsPanel.html)
  and [Move](https://affinity.help/designer2/en-US.lproj/pages/Tools/tools_move.html), read via
  web on 2026-10-10. The audit's §5 is the owner's agreed compact layout and preset oracle.
  References define behavior/metaphors, not licensed artwork for redistribution.
- Repository architecture, persona spec including Owner decisions, quality plan, gap audit,
  package index and icons-standard report. The final task SPEED RULES supersede older tests.
- Existing `tool_art.rs`: original Local Image nine palette pairs, 24px cell, optical inset,
  dark outline and 28% highlight standard. New `tool_art_vector.rs` paths are original
  Local Image GPL-3.0-or-later. Recognition rows define intent, not empirical user recognition.
- Compact popup behavior follows the locally pinned egui 0.36.2 `containers/popup.rs` API (`Popup::menu`, `from_response`, `open_bool`, CloseOnClickOutside). Colour editor uses its existing Window container; these are in-app surfaces.
- Existing `egui_extras` 0.36.2 SVG loader / resvg 0.45.1 is used through the existing UI API.
  The tiny 5x7 label alphabet in the new sheet test reuses Local Image's existing original
  tool_icon_sheet alphabet. `image` is the already-linked encoder; no dependency was added.

Fixtures are original GPL-3.0-or-later and deterministic. `Host` creates an 80x80 RGB U8
shape with a red fill, 4px blue stroke and 50x50 square at (10,10). The independent swap
oracle is red↔blue; D is white fill/black stroke; green hex input is 00ff00. Bounds/cache
preparation use the existing engine/vector renderer. Multi-target locks/stale checks go
through 08's Gesture and vector_api, not a parallel transaction implementation.

`pixel-menu-keys.json` snapshots every accelerator-bearing row of the inherited
menu_catalog, preserving order. Engine `pixel-tool-keys.json` snapshots every inherited
TOOL_SHORTCUTS row (including unbound Blur). `pixel-accelerators.json` captures the seven
menu conflicts, four channel targets and D/X plus an unaffected clipboard control chord.
These are independent frozen input records; tests compare actual catalog/registry output.
They are not regenerated from the new override table. Renderer-alpha tests enumerate all
new and reused icon mappings at physical 16/20/40px, both styles. Contact sheet parameters:
1080px wide, 68px per mapping, original 5x7 labels, twelve 60px tiles per row for each size's
colour dark/light and mono dark/light (backgrounds 30 and 240). PNG output is
`docs/design/vector-tool-icons.png`.

Hardware tests use offscreen egui_kittest 0.36.2 / egui-wgpu and require specifically
NVIDIA RTX 5090, Vulkan, DiscreteGpu, Rgba32Float compositor and supported f16 canvas.
Absence is a failure when explicitly run, never a successful skip. The full-app test is
ignored pending 00's production wiring; the icon upload test is ignored pending the GPU
milestone. Neither was run here under the owner's speed rule. No external/proprietary
fixtures were supplied or silently counted as passing; no performance measurement/claim.
