# Blend provenance and package 00 attribution fragment

VectorCraft local checkout: `~/.local/share/local-image-dev/engine-sources/vectorcraft`.
Pinned commit: `d522c1d7be4035bd4f4a84cd6ebfca44f5155092`, MIT OR Apache-2.0.
Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors. Existing complete
notices are retained in `licenses/vectorcraft-LICENSE-MIT`,
`licenses/vectorcraft-LICENSE-APACHE` and `licenses/vectorcraft-NOTICE`. The adapted Rust
files retain per-file copyright/license references. No brand assets or source fonts copied.

Read source fingerprints (SHA-256):

| Upstream file | SHA-256 |
|---|---|
| `crates/doc/src/blend.rs` | `b4633020ddab5329ba9d0b78047c4c622f48a188dd9b753cc50cf8976dc79ac4` |
| `crates/doc/src/live.rs` | `2c792e4149de3df98e1b8057ec1351cc43453c4a2ae5c27da059249d14172981` |
| `crates/engine/src/tests_blendopts.rs` | `2ca60e18d7ff15c1a35591d1293121313724609da87b0b33992a8079c705bcd6` |
| `crates/engine/src/tests_blendspine.rs` | `0fab54f055e5c97a6a251f586c231605c435481ff7a0a831be90dfac711bb900` |
| `crates/engine/src/tests_blendfidelity.rs` | `efe3a7ed69d32fa7696ebba431baff1565ca3c6b46e77c6a6ce40b4cc80fa78d` |

The port follows the ordered key/child expansion, segment splitting, winding/start
normalization, gradient/stroke interpolation and spine orientation structure. Local
changes preserve the native ordinary Layer tree, exact endpoint keys and zero steps;
explicit compound operation order plus hole-depth matching; common arclength samples
for unequal node counts; Oklab instead of raw RGB; finite budgets, two-pass cache budget,
captured atomic commands and cancellable egui state. Scene projection uses only the
vector_design::exchange DTO. No UI or engine dependency in pc-vecfx.

Oklab published reference: [Björn Ottosson's Oklab post](https://bottosson.github.io/posts/oklab/),
2021-01-25 matrices (page fetched 2026-10-10; SHA-256 `763738fd931b6d3feea3d91b61db5d40028758c3e228d11748f6b00ad1df7cf7`).
Code is offered as public domain or MIT; the port selects MIT. Copyright (c) 2020 Björn
Ottosson. Full permission/warranty notice retained in [18-oklab-LICENSE-MIT.txt](18-oklab-LICENSE-MIT.txt),
from [the author's license](https://bottosson.github.io/misc/License.txt). This is a Rust
translation of the published formulas; no C/C++ source enters the build. pc-color's
existing sRGB transfer functions are reused. Its planned shared Oklab interface is
absent on this branch, so 00 can promote the owned helpers later.

Documented behavior: [Adobe blended objects overview](https://helpx.adobe.com/illustrator/desktop/manage-colors/apply-transparency-and-blending/blended-objects-overview.html)
(official page, updated 2026-08-03, read 2026-10-10): editable source keys update a blend;
expansion creates ordinary editable objects. The exact spacing/orientation interface
comes from the pinned VectorCraft BlendSpec and tests, adapted to shared V9 BlendSpec.
Affinity documentation was not needed for this blend-only task; no uncited affinity
behavior or proprietary fixture is claimed. The architecture/quality plan/persona
owner decisions and existing V1 pc-pathops/pc-vector code govern host integration.

## Append to docs/PORTS.md (00 owns the shared inventory)

| Our file | Upstream project | Upstream path | Upstream commit | Licence | Date |
|---|---|---|---|---|---|
| `crates/pc-vecfx/src/blend.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/doc/src/blend.rs`, `crates/doc/src/live.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-vecfx/src/blend/correspondence.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/doc/src/blend.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-vecfx/src/blend/paint.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/doc/src/blend.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-vecfx/src/blend/paint.rs` | [Oklab](https://bottosson.github.io/posts/oklab/) | linear sRGB/Oklab conversion formulas | 2021-01-25 matrices; page SHA above | MIT | 2026-10-10 |
| `crates/pc-vecfx/src/blend/spine.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/doc/src/blend.rs` | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |
| `crates/pc-ui-egui/src/vector_packages/blend.rs` | [VectorCraft](https://github.com/storytold/vectorcraft) | `crates/engine/src/tests_blendopts.rs`, `tests_blendspine.rs` (interaction adaptation) | `d522c1d7be4035bd4f4a84cd6ebfca44f5155092` | MIT OR Apache-2.0 | 2026-10-10 |

Engine capture/transaction code, app hooks, and new tests are original Local Image
GPL-3.0-or-later code. New tests use independently derived geometry/color/coverage
oracles and real input events; upstream expectations are references, not a copied corpus.

## Attribution generator handoff

Keep VectorCraft's existing attributions entry and add these blend port paths to its
scope. Copy `18-oklab-LICENSE-MIT.txt` to `licenses/oklab-LICENSE-MIT` and add:

- Name: Oklab (Björn Ottosson)
- Source: https://bottosson.github.io/posts/oklab/
- License: MIT; copyright (c) 2020 Björn Ottosson
- Scope: pc-vecfx blend/paint.rs linear sRGB/Oklab matrices
- Full text: licenses/oklab-LICENSE-MIT

Regenerate li-attributions through 00's existing inventory tool; no shared generated
attribution or license catalog was edited by this package.

Fixtures, version/parameter records and absent corpus: [blend/SOURCES.md](../../../../crates/pc-vecfx/tests/blend/SOURCES.md).
