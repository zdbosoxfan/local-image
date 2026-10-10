# Import implementation sources and API adaptation

Primary mapping/placeholder/cascade reference:
[VectorCraft](https://github.com/storytold/vectorcraft/tree/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/svg/src)
`import.rs`, `import/text.rs`, `import/css.rs`, pinned to
`d522c1d7be4035bd4f4a84cd6ebfca44f5155092`. The source was inspected read-only
at `~/.local/share/local-image-dev/engine-sources/vectorcraft`. Copyright
(c) 2026 ArtCraft Team and VectorCraft contributors, MIT OR Apache-2.0;
existing `licenses/vectorcraft-*` notices apply. No document tree or renderer
was transplanted. The small CSS cascade is adapted directly; geometry and
paint conversion use Local Image native types.

Pinned APIs were checked against cached registry source and official
[usvg 0.45.1 documentation](https://docs.rs/usvg/0.45.1/usvg/),
[SVG 1.1 paths](https://www.w3.org/TR/SVG11/paths.html),
[painting](https://www.w3.org/TR/SVG11/painting.html),
[text](https://www.w3.org/TR/SVG11/text.html), and
[masking](https://www.w3.org/TR/SVG11/masking.html).

0.48-to-0.45 differences relevant to this port:

- usvg 0.45.1 uses roxmltree 0.20.0; the reference's newer graph uses a newer
  XML API. Use the explicitly locked direct roxmltree 0.20 API here.
- RadialGradient in 0.45.1 has cx/cy/r/fx/fy, but no `fr()` focal radius;
  the 0.48 reference's `fr()` call must not be copied.
- Our locked usvg has no `text` feature. Options::fontdb/font_resolver and
  Tree::fontdb are feature-gated. XML placeholders and pc-text provide text;
  no fontdb dependency or second shaper is enabled.
- 0.45's SVG tree parser applies stylesheet `!important` before inline
  `!important`, keeping the earlier value. A universal important rule can
  otherwise recolour or hide a synthetic text marker. For documents containing
  text, source selectors are scoped to a fresh attribute on source elements;
  synthetic marker paths have no such attribute. The same locked simplecss
  0.2.2 parser used by usvg preserves its supported selectors and declarations.
  Original paint rules keep their relative specificity and use-instance scope.
- Both versions expose normalized group transforms, CSS/use expansion, image
  data callbacks and cubic paths. 0.45 ImageHrefResolver::resolve_data receives
  MIME, Arc<Vec<u8>>, Options after base64 decoding, so encoded size must be
  checked in the XML preflight first. resolve_string defaults to filesystem
  access and is explicitly replaced with a closure returning None.
- Native Fill lacks the upstream GradientGeom's affine/spread/focal fields.
  pc-vector supplies geometry coverage for normalized imported paint caches;
  native paint mirrors and an aggregated warning state the editability loss.
  The existing stroked-shape split path reconstructs fill from the native mirror, so it may approximate enhanced SVG fills even before an edit. Package 08 is required for enhanced paint projection in that path and after edits.

Own algorithms where the reference cannot be used directly: bounded reference
graph preflight, shared ImportLimits/cancellation, native Session staleness
guards, cache byte accounting, and UI interaction adapters. Security rejects
DTD/entities, recursive references and unsupported recursive embedded SVGs.
