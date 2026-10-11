# Vector package 47: one implementation per tool across the Pixel and Vector personas

Report file: `docs/wip/CODEX-REPORT-shared-tools.md`.

Owner (2026-10-10): "For tools such as the pen tool that exist in both the pixel tools and the vector tools make sure
that the tools are identical as they are in Affinity. So for example we would not have a much better pen tool in the
vector tools and the lame one in pixel tools. Audit all tools to see what is shared and make sure that it is actually
sharing and optimized."

Run this after the tool packages it touches have merged into `wip/vector` (check `git log wip/vector` for 02 live
shapes, 03 node editing, 04 pen, 05 pencil, 06 corners, 07 snapping, 08 appearance, 12 path text, 32–36, 41); code
against what's there.

## Do
1. **Audit** every tool in both personas (Compositing/pixel tools in `crates/pc-ui-egui`, `pc-engine` tool commands,
   tool sets in `pc-engine/src/toolsets.rs`; vector tools from the packages above): build a table of tools that exist in
   both (expected at least: Move, Pen, Node/Direct Selection, Path Selection, shape tools, Text/Type (art, frame, on
   path), Gradient/Fill, Eyedropper, Crop, Hand, Zoom, Pencil, Corner, snapping/smart guides, Transform), and for each:
   which code paths implement it in each persona, feature differences, and duplicated code. Follow Affinity Photo 2 /
   Designer 2's documented behaviour (affinity.help: the Pen, Node, shape, Artistic/Frame Text tools are the same tools
   in Photo and Designer; persona/context decides the result) — cite pages.
2. **Unify**: one implementation per tool, shared by both personas, with the persona/context only choosing the
   output (vector layer / shape layer / path / selection / pixels) and default options. The pixel persona gets the
   full-featured version (e.g. the vector Pen's Smart/Polygon/Line modes, rubber band, node handling, snapping, live
   corners). Remove the duplicates; keep old documents and settings loading (serde defaults), keep shortcuts per the
   owner-approved per-persona scheme (package 46).
3. **Optimise** the shared paths: no per-frame allocations/clones of large data, previews via detached overlays/GPU
   canvas, one undo step per gesture; measure on the real RTX 5090 (frame times for pen/shape/node drags on a large
   document) before/after.
4. **Tests**: for each shared tool, interaction tests run in both personas against the same code (same results apart
   from the output kind); a test guarding against reintroducing a second implementation (e.g. both personas resolve
   the same tool handler).

{RULES}
