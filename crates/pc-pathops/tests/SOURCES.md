# Geometry quality corpus

All synthetic inputs in `quality.rs` and `quality_corpus.rs` are original Local Image test data,
CC0-1.0. Shapes are generated deterministically, including seed `0x93ea71` for 200 pairs.
The oracles are the independent PSD component coverage engine, exact translated Green
integrals, cubic nearest queries, and Felzenszwalb–Huttenlocher EDT from the published paper.
`ratchet/geometry.json` contains known native-corpus failures; a passing listed case is an error.

The upstream property and operation suites retain VectorCraft's MIT/Apache notices and pin
`d522c1d7be4035bd4f4a84cd6ebfca44f5155092` in their headers and `docs/PORTS.md`.

External Inkscape 1.4.4 golden generation is assigned to the coordinator by quality-plan A.1.
No Inkscape outputs were supplied to this job; synthetic/native checks are not mislabeled
as Inkscape goldens. The independent resvg 0.45.1 cubic SVG check lives in pc-testkit tests.
