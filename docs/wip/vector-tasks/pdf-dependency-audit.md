# PDF selected-feature dependency audit — 2026-10-10

The final PDF choice is krilla 0.8.2 for export and hayro-interpret 0.7.0 / hayro-syntax
0.7.2 for editable import. This is metadata/source verification, not a production adapter
or a PDF render/performance result. Package 00 registers this owner-approved graph once.

```toml
krilla = { version = "=0.8.2", default-features = false, features = ["raster-images"] }
hayro-interpret = { version = "=0.7.0", default-features = false, features = ["embed-fonts", "embed-cmaps"] }
hayro-syntax = { version = "=0.7.2", default-features = false, features = ["std", "images"] }
```

Krilla's entire `graphics::image` module is gated by raster-images, including custom and
raw-RGBA constructors. Full document export and warned raster fallbacks require that feature.
Its decoders are Rust. Do not enable simple-text (pc-text shapes) or pdf (no hayro-write).
Hayro interpreter unconditionally enables syntax's unsafe feature, which enables pure Rust
SIMD codecs; no unsafe adapter code is allowed. Dependency-internal unsafe is already present
in our image/GPU stack and is outside the workspace source lint's scope.

An isolated manifest under `target/refvec/vector-pdf-audit/` was resolved with
`PATH=~/.cargo/bin:$PATH CARGO_BUILD_JOBS=3 cargo +1.98.1 metadata --offline --format-version 1`.
Its initial lock was copied read-only from VectorCraft at
`d522c1d7be4035bd4f4a84cd6ebfca44f5155092`. A fresh unseeded resolution fails because the cached
index's yoke-derive 0.8.3 is yanked; both our current and VC locks already pin it, and seeded
offline resolution succeeds. Retain that existing lock entry during package 00 integration.
The source metadata and resolved lock are retained under target; all registry checksums and
resolved features are in [pdf-dependencies.tsv](pdf-dependencies.tsv).

The selected graph has **82 registry packages, 19 additional crate/version pairs** relative
to this branch's baseline Cargo.lock. The earlier 18-package graph omitted images; enabling
raster-images adds imagesize 0.14.0 and activates already-locked Rust decoders. No workspace
manifest or runtime dependency was changed by this audit.

| Additional crate | Version | License |
|---|---|---|
| brotli | 8.0.4 | BSD-3-Clause AND MIT |
| font-types | 0.11.3 | MIT OR Apache-2.0 |
| hayro-ccitt | 0.3.0 | Apache-2.0 OR MIT |
| hayro-cmap | 0.1.0 | Apache-2.0 OR MIT |
| hayro-interpret | 0.7.0 | Apache-2.0 OR MIT |
| hayro-jbig2 | 0.3.0 | Apache-2.0 OR MIT |
| hayro-jpeg2000 | 0.3.5 | Apache-2.0 OR MIT |
| hayro-postscript | 0.1.0 | Apache-2.0 OR MIT |
| hayro-syntax | 0.7.2 | Apache-2.0 OR MIT |
| imagesize | 0.14.0 | MIT |
| krilla | 0.8.2 | MIT OR Apache-2.0 |
| pdf-writer | 0.15.0 | MIT OR Apache-2.0 |
| read-fonts | 0.39.2 | MIT OR Apache-2.0 |
| ryu | 1.0.23 | Apache-2.0 OR BSL-1.0 |
| skrifa | 0.42.1 | MIT OR Apache-2.0 |
| subsetter | 0.2.6 | MIT OR Apache-2.0 |
| tiny-skia-path | 0.12.0 | BSD-3-Clause |
| write-fonts | 0.48.1 | MIT OR Apache-2.0 |
| xmp-writer | 0.3.3 | MIT OR Apache-2.0 |

All 19 additional packages have Rust targets and no custom build script. The full graph has
no `cc`, cmake, bindgen, `-sys` package, native `links` declaration or C/C++ compiler step.
The four existing build scripts (crc32fast, num-traits, proc-macro2, quote) perform Rust
configuration/version detection; they do not compile C. The selected flate2 graph uses
miniz_oxide/rust_backend, not a system zlib. MIT, Apache-2.0, BSD and BSL license alternatives
in this graph are compatible with the project's GPL-3.0-or-later distribution policy.

Package 00 must compare the **unified workspace** graph against this snapshot: preserve
existing version pins, identify any additional target/feature dependency, inspect source
and license, reject new C/-sys, and regenerate the app's attributions. A metadata-only
snapshot does not replace the integrated build and PDF fixture gates in packages 21/22.

Evidence: cached normalized Cargo.toml, Rust sources and checksum files for each exact
version; [krilla feature manifest](https://docs.rs/crate/krilla/0.8.2/source/Cargo.toml.orig),
[krilla image module gate](https://docs.rs/crate/krilla/0.8.2/source/src/graphics/mod.rs),
[hayro-interpret feature manifest](https://docs.rs/crate/hayro-interpret/0.7.0/source/Cargo.toml.orig),
[hayro-syntax manifest](https://docs.rs/crate/hayro-syntax/0.7.2/source/Cargo.toml.orig),
and the [VectorCraft PDF adapter](https://github.com/storytold/vectorcraft/tree/d522c1d7be4035bd4f4a84cd6ebfca44f5155092/crates/pdf).
