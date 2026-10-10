# V1-Q original external-oracle inputs

All 105 SVG inputs and their geometry/parameter descriptions in `manifest.json` are
original Local Image synthetic fixtures, dedicated under CC0-1.0. No brand assets or
third-party drawings are included. Each SVG carries its licence identifier. The manifest
is the canonical source record: path commands, fill rule, scale, origin, join/cap/miter,
width-relative dash settings, offset distance and simplify tolerance are explicit.
Inputs are smaller than 50 KB each. Existing `quality_corpus.rs`, `quality.rs`, their
source record and `ratchet/geometry.json` are retained unchanged.

Binary corpus: 20 families at normal scale, translated to an origin of 1e6, and at
scale 1e-4 (60 files). Families cover shared/coincident/near-coincident edges, point
and circle tangencies, both figure-eight fill rules, 50 overlapping spikes, nested
holes, a hole touching the outer boundary, zero-area spikes, cubic cusps/loops,
thin slivers, containment and open fills. Page width/height remain 96; viewBox maps
the scaled/translated geometry back to that viewport. Cubic circles use the standard
four-cubic approximation, not exact circular arcs or an implementation-generated oracle.

Unary corpus: 24 signed offsets (square/star, round/miter/bevel, -20/-4/+4/+20),
18 stroke expansions (all three caps and joins, solid/dashed), and 3 simplify tolerances
(0.05/0.25/1 px) on an original 101-node sampled sine. Native dash lengths/offsets are
width-relative; SVG records absolute lengths 16/8 and offset 4 for an 8 px stroke.

## External outputs: absent, not successful

Required reference tool: **Inkscape 1.4.4**, as pinned by quality-plan A.1.
Required normalizer: **usvg 0.45.1**, already locked, via pc-testkit's existing test-only
resvg 0.45.1 dependency. There are currently **zero of 345** required Inkscape output
SVGs/sidecars. No external CLI was installed or executed by package 31. The empty
`goldens/ratchet.json` is a separate, fail-closed external ratchet; it does not declare
the absent corpus green. Missing outputs/provenance are not ratchetable failures.

The coordinator generates `goldens/<id>/<operation>.svg`, retaining each 96x96 viewport,
then writes `goldens/generation.json` from the actual run, with this shape:

```json
{
  "tool": "Inkscape 1.4.4",
  "version_output": "actual inkscape --version output, including build identifier",
  "command_template": "actual invocation including batch/export flags",
  "inputs": {"<id>": "exact original input SVG bytes decoded as UTF-8"},
  "actions": {"<id>/<operation>": "exact executed action sequence"},
  "parameters": {"<unary-id>/<operation>": "the manifest parameter object"},
  "settings": {"<id>/<operation>": "effective tool/preferences/LPE settings"}
}
```

The `parameters` and `settings` values above are objects, not literal placeholder strings.
Retain the actual version, command, settings, and output licence here when references land.
For booleans, the action prefixes are `select-by-id:a,b;path-union`, `path-difference`,
`path-intersection`, `path-exclusion`, or `path-division`. For strokes, use
`select-by-id:a;object-stroke-to-path`. A coordinator command template is:

```sh
inkscape --batch-process --export-area-page --export-plain-svg \
  --export-filename=<output.svg> \
  --actions="select-by-id:a,b;path-union;export-do" <input.svg>
```

Check action names against the pinned tool's `--action-list` before generation. Offset
requires the actual Offset/LPE distance, join and miter settings matching the manifest;
do not substitute repeated fixed-step `path-offset` calls for specified -4/+4/-20/+20.
Simplify's preference threshold is tool-relative, not a document-pixel tolerance. Record
its actual settings and verify that the result achieves the manifest's requested
Hausdorff tolerance. These two action recipes are deliberately unspecified until the
coordinator measures the pinned tool; no unverified parameter mapping is claimed.

After all outputs/provenance exist, the coordinator runs the ignored
`geometry31_prepare_inkscape_sidecars` test in photocraft-testkit. It invokes no external
CLI: usvg parses/normalizes the actual SVG, resolves root/group transforms, retains
evenodd/nonzero fill and cubic/quadratic handles, then writes native Path sidecars.
Source SVG and input SVG are embedded to reject stale sidecars. Strokes, text, images,
clips, masks, filters, an altered viewport, or >50 KB outputs are rejected. Empty path
results remain legitimate empty references. usvg uses f32 coordinates internally;
its normalization regression is checked, but precision on external large-origin
outputs remains an integration gate, not a proven property of absent fixtures.

The ignored pathops gate compares area, sampled symmetric Hausdorff, 8x IoU, node budget
and crossings; it needs >=95% passing cases and rejects new failures, unknown/duplicate
ratchet entries, blank reasons, or a passing entry that remains ratcheted. A listed
failure must be removed once repaired. Simplify is checked against the source tolerance
and reference node budget; different valid fitted curves need not match each other.
Division compares the bottom object's faces: Inkscape divides the bottom path, whereas
Pathfinder Divide includes all union faces. References are independent of Local Image's
constructive kernel; its adapter is used only for input coordinate conversion.

## Sources followed

- Local `docs/specs/vector-quality-plan.md` A.1–A.5 and architecture section 4.
- [Official Inkscape action descriptions](https://wiki.inkscape.org/wiki/Action),
  including bottom-minus-top Difference, bottom-only Division, and Stroke to Path.
- [Official Inkscape command-line guidance](https://wiki.inkscape.org/wiki/Using_the_Command_Line).
- [kurbo 0.13.1 BezPath API](https://docs.rs/kurbo/0.13.1/kurbo/struct.BezPath.html),
  and the pinned cached `kurbo-0.13.1/src/bezpath.rs` for input SVG parsing.
- Locked `usvg-0.45.1/src/tree/mod.rs` (absolute transforms, fill rule, normalized path
  segments) and tiny-skia-path's locked PathSegment API. No upstream source is copied.

New Rust gate/normalizer code is original Local Image GPL-3.0-or-later code. Fixture
licence is CC0-1.0. PORTS additions: none; existing notices and attribution are retained.
