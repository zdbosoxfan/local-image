# Package 23 — PSD advanced fallback

Status: package-owned implementation and integration probe supplied. **Not yet
fully accepted in the shared application**: package 00 must activate the exact
IO, importer, engine, compositor, test and i18n hooks in
[`integration/23.md`](vector-tasks/integration/23.md); native 11/17–20 providers
are not present in this branch. Missing Photoshop corpus is separately reported,
not counted as success.

Continued branch `wip/vec-23-psd-advanced-fallback` from `453689d0`, including the
previous builders' committed appearance/path-text/symbol/PDF work. Initial tree
had only the task file untracked. No commits were made. Only package-owned source,
tests and handoff records were changed; actual importer/UI/registries/manifests,
lock, compositor/GPU source and catalogs remain untouched.

## Implementation by acceptance point

- Added the public IO boundary at `photocraft_io::vector_map::fallback` through
  the owned `vector_map.rs`. `LossCollector` accepts shared `VectorWarning`,
  deduplicates by feature key, merges object-specific losses to document scope,
  and produces stable lexical output and one localized-capable notice. It names
  appearance stacks, multiple fills/strokes, paint opacity/blend/visibility,
  profiles, brushes, arrows, dash alignment, scale/gradient stroke options,
  path text, symbols/master metadata, design shapes/corners, constraints, crop
  and all three live feature kinds. Hidden/empty objects are included.
- PSD/PSB export writes the authoritative bounded native cache, deterministic
  **08's topmost visible supported** fill/stroke mirror and existing supported
  vector blocks. `Some(empty)`/hidden-only stacks disable both paints, including
  the imported undecodable-fill fidelity exception. Ordinary `None` appearances
  retain existing behavior. Legacy shape/raw-block guards are reused.
- Path text exports its cache without an unrelated ordinary `TySh`; symbol
  instances export cache pixels without stale placed/type/shape identity.
  The private strict/versioned `liVF` marker identifies raster, shape and path-text
  rendering roles, shape-proxy flag, BLAKE3 cache fingerprint and relative
  ordinary-effect frame. Separate shape vector masks are baked through the native
  mask renderer while pixel masks stay independent, with a named loss notice. Its raw blocks are retained on
  unchanged re-export; raster pixel edits invalidate the vector proxy and refresh rendering metadata,
  while unrelated raw blocks survive. It stores no editable native feature data.
- `prepare` clones the export, validates finite/bounded DPI, appearance and path
  inputs, group depth and required caches. Common live/crop projection is required
  and must preserve IDs/compositing envelope and clear evaluated metadata; no
  provider produces an explicit error. Source losses are captured before projection.
  No live algorithm or format-only evaluator is implemented here.
- The engine boundary checks captured document ID/revision before serialization,
  remains read-only, and adds no revision/history/journal event. Shared warning
  formatting is available to 14/21; adapters add their own actual format losses.
- Exact package-00 hooks include cache import as raster, checked normal save,
  preserved user vector masks, engine registration, localized presentation and
  CPU/GPU rendering-kind behavior. Type blending gamma and shape stroke effects
  must survive the loss of the editable content kind. The private probe validates
  these hooks without taking another owner's files.
- Supplied offscreen full-app real-GPU Save As/warning/dismissal and held/released
  Move test. It generates native 08 multi-fill/stroke, 12 path text and 15 symbol
  caches, compares native/reopened CPU and actual GPU textures, checks detached
  preview and one undo on release. Its reusable helper lets 00 add real native
  11/17–20 fixtures when those providers land. No desktop window or owner file
  was opened/written.
- Supplied 23 i18n source rows, exact APIs/registration fragments, independent
  data/format/cache/block oracles and fixture/source provenance. No dependencies,
  unsafe code, upstream ports or new license obligations were introduced.

## Validation environment and commands

All builds used `PATH="$HOME/.cargo/bin:$PATH"`, Rust/Cargo **1.98.1**,
`--offline`, `CARGO_BUILD_JOBS=3`, and one cargo build at a time in this job.
Normal workspace cargo fails before compilation because the previously merged
package 21 source has no `crates/pc-pdf/Cargo.toml`/crate root/engine dependency.
Those files belong to 00. `tests/vector_fallback/bootstrap.py` registers **21's
existing manifest fragment** and all proposed hooks only under
`target/psd23-workspace`; it does not change dependency pins or actual shared files.

Reproduce the probe:

```sh
python3 crates/pc-io/tests/vector_fallback/bootstrap.py
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_BUILD_JOBS=3
export CARGO_TARGET_DIR="$PWD/target"
cargo +1.98.1 fmt --manifest-path target/psd23-workspace/Cargo.toml -p photocraft-io -p photocraft-engine
cargo +1.98.1 test --offline --manifest-path target/psd23-workspace/Cargo.toml -p photocraft-io vector -- --nocapture
cargo +1.98.1 test --offline --manifest-path target/psd23-workspace/Cargo.toml -p photocraft-engine vector_export_warnings -- --nocapture
cargo +1.98.1 test --offline --manifest-path target/psd23-workspace/Cargo.toml -p photocraft-ui-egui --test vector_fallback_probe vector_fallback_app -- --nocapture
cargo +1.98.1 test --offline --manifest-path target/psd23-workspace/Cargo.toml -p photocraft-io -p photocraft-engine -- --test-threads=1
CARGO_TARGET_DIR="$PWD/target/clippy" cargo +1.98.1 clippy --offline --manifest-path target/psd23-workspace/Cargo.toml -p photocraft-io -p photocraft-engine --all-targets -- -D warnings
cargo +1.98.1 fmt --manifest-path target/psd23-workspace/Cargo.toml -p photocraft-io -p photocraft-engine --check
rustfmt +1.98.1 --edition 2024 --check crates/pc-io/tests/vector_fallback/app_gpu.rs
```

Focused IO final run: **56 passed, 0 failed, 0 ignored** (18 library, 26 existing
vector-map coverage, 6 existing vector/mask integration regressions, 6 new
round-trip checks). Native 08 PSD/PSB cache parity was **max 0** at U8/U16/F32.
Both existing vector round-trip behavior and unchanged vector-block coverage
actually ran, rather than relying on an unregistered aggregate. Evidence:
`target/psd23-focused-final.log`.

Engine focused: **2 passed, 0 failed, 0 ignored**; evidence
`target/psd23-engine-focused.log`.

Final full suites ran **once**, only the changed IO and engine crates:

| Crate / suite | Passed | Failed | Ignored |
|---|---:|---:|---:|
| Engine library | 984 | 0 | 13 |
| Engine coverage | 582 | 0 | 0 |
| Engine integration | 47 | 0 | 5 |
| Engine vector V1 | 12 | 0 | 1 |
| IO library | 109 | 0 | 1 |
| IO coverage | 120 | 0 | 0 |
| IO integration | 205 | 0 | 1 |
| IO fallback probe | 6 | 0 | 0 |
| **Total** | **2,065** | **0** | **21** |

Both doc-test targets ran, each containing 0 tests. Evidence: `target/psd23-full.log`.
The 21 **pre-existing** ignored cases are 13 timing/performance benchmarks, 2 V1
node tests awaiting 00's actual adapters, 2 panic-hunt entry/fuzz cases, 2
user-supplied Camera Raw fixtures, 1 manual Photoshop file-writing check and
1 ~12-GB large-PSB release check. None of package 23's tests is ignored. The two
missing PSD vector corpus gates below are **additional**, feature-gated cases;
they are not part of this ignored count.

All-target clippy passed for both changed crates with **`-D warnings`**, using
the required separate `target/clippy` directory; evidence:
`target/psd23-clippy.log`. Cargo fmt check, direct rustfmt check of the supplied
UI/GPU module, and `git diff --check` all passed. The inherited build-script
notice about unavailable CJK craft-fonts is unchanged; the real ABC glyph cache
was nonempty and no CJK acceptance is claimed.

Full-app GPU: **1 passed, 0 failed, 0 ignored**, NVIDIA GeForce **RTX 5090**,
Vulkan, discrete, driver **615.71.09**, required accumulator **Rgba32Float**.
Native/reopened premultiplied CPU max **0.000000000**. Actual native/reopened,
held Move preview and released canvas GPU/CPU max **0.002149656**, below 1/255.
Warning list has one notice/one line/every expected loss once and is really
dismissed by clicking. Preview leaves history/revision unchanged; release adds
one undo step and matches held preview pixels. Evidence: `target/psd23-gpu.log`.
It also actually clicks Save As again after Move: no repeat loss notice and
moved/re-exported premultiplied max **0.000000000**.
No software-adapter fallback, skipped adapter, release performance claim or
synthetic profile/brush/live renderer is counted as GPU acceptance.

During development the original stroke-effect app fixture exposed rendering-kind
loss (native/reopened max .599215686 despite exact caches); a color-overlay
diagnostic independently exposed path-text gamma loss over white (.155005217).
The final marker/00 compositor hook fixes both, retaining the **original outside
stroke-effect fixture and default text gamma**. Neither threshold nor background
was weakened. The earlier exact-label Save As harness query was also corrected
to match the actual accessible shortcut-bearing label. Failed development runs
are not counted as successful acceptance.

The final mask/effect review also caught a separate shape vector mask being lost
(premultiplied max .65), and gradient overlay alignment changing to the raster
cache's bounds (.10484159). The native mask renderer now bakes that unsupported
separate mask with a specific loss notice; the marker carries a relative effect
frame, and proxy invalidation preserves pixel blending metadata. The focused
round-trip test checks masks, gradient effects, Move and re-export against the
native source, each at <=1/255.

## Missing gates / coordinator work

1. Apply the serial package-00 activation in the actual shared tree, including
   package 21 manifest registration. The private source copies prove proposed
   integration; they do not activate the shared app. Re-run the registered GPU
   fixture there before calling package 23 done.
2. Native width/profile/brush/blend/repeat/envelope acceptance is **missing**.
   `vector_packages::{width,brushes,blend,repeat,envelope}` currently have empty
   registration boundaries; relevant 11/17–20 implementations are not merged.
   Format tests inject caches for some of this metadata and one test injects
   a common live projection; those are **data/proxy evidence only**. Use the
   supplied real-GPU helper with actual native fixtures/common providers after
   integration. Missing live/crop projection errors rather than writing stale
   children and claiming a rendered feature.
3. **2 missing external corpus gates, 0 passed**:
   `vector::corpus_shape_coverage_matches_photoshop` and
   `vector::corpus_vector_blocks_survive_roundtrip`. `corpus/psd` is absent and
   no licensed local reference corpus was available. They are feature-gated out
   of the ordinary suite, not counted as ignored/passed fidelity checks. 00 runs
   them with `--features corpus` when the corpus is supplied and nonempty.
4. Merge `i18n/23.tsv` and wire the structured localized notice at the frontend
   boundary. The existing normal IO save route emits one English warning string,
   consistent with its current API. Dedicated intentional raster-conversion
   commands should clear the rendering marker when changing blending behavior.

## Sources and attribution

Read the complete task, architecture, persona spec including Owner decisions,
quality plan and vector package README. Followed architecture §7's shared
`vector_design::exchange`, topmost-supported mirror and authoritative-cache rule;
reused existing pc-io PSD vector/raw guards, tests, 08 renderer/mirror, 12 text and
15 symbol providers, and existing full-app GPU/menu/notice/Move patterns.

Official primary format reference: [Adobe Photoshop File Formats Specification,
November 2019](https://www.adobe.com/devnet-apps/photoshop/fileformatashtml/), layer
channels, vector masks and additional layer information; consulted 2026-10-10.
Adobe describes storage; the private marker and fallback editability policy are
original Local Image code implementing the repository architecture. There is no
upstream implementation of this native metadata bridge to copy. Existing
PhotoCraft/VectorCraft notices and PORTS records are retained; no new adapted file
means no new PORTS row. 00's attribution handoff is in integration/23.md.

Original GPL-3.0-or-later synthetic fixture parameters, tool versions, source and
oracle distinctions are recorded in
[`SOURCES.md`](../../crates/pc-io/tests/vector_fallback/SOURCES.md). No brand,
proprietary fixture, font, C/C++ code or new registry dependency is bundled.
