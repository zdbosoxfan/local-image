# RAW decoder comparison — 2026-10-14

The measured result is **mixed**. Keep native DNG, CR2, NEF, ARW, PEF, uncompressed ORF and
RW2/RWL packing 4 first. Use the unmodified rawler engine first for RAF, NRW and the other
RW2/RWL packings; keep native decoding when that reader cannot decode the file. CR3, SRW and
other recognized TIFF RAW containers remain rawler-first. These choices are implemented,
not just recommendations. No camera-model allow-list was added.

A wholesale switch would regress known bit-exact NEF and packing-4 RW2 samples and the iPhone
12 Pro lossless linear-DNG case. The owner's **mixed-results/per-format** clause is therefore
applied, including the additional per-compression rule. Correctness decides the changes below;
no speed advantage is claimed without the requested timings.

**Timing is deferred.** The same follow-up requests median-of-five timings and ends with
“Do NOT run … benchmarks.” The clarification remains unanswered. The final prohibition was
honored: no timed comparison, full crate suite, GPU acceptance or benchmark was run. The
permanent harness implements explicit opt-in five-run timings and records cores; the time
columns are NaN/unmeasured, not zero. Preview-speed cost remains unmeasured. It can be filled
by one command after the owner authorizes timings/merged validation. This part of the original
measurement request is outstanding; it has not been represented as complete.

## Corpus, method and artifacts

Sources are CC0 raw.pixls.us files, downloaded only into `target/raw-comparison`. The checked-in
`crates/lc-raw/tests/fixtures/decoder-comparison-samples.tsv` records every source ID, maker,
model, variant, URL and SHA-256. The driver verifies source hashes. No Desktop/client images
were opened. The **56-file corpus** includes at least two bodies for every requested format family; the
floating HDR-merge variants use the available Canon 5D III series.

The host is AMD Ryzen 7 9800X3D: **8 physical cores / 16 logical CPUs**. Both Rust decoders
run inside the same four-worker Rayon pool. Cargo's existing `CARGO_BUILD_JOBS=4` remains
unchanged. Only one Cargo compile lane was used. Results are measured on the worktree source;
rawer's source and version pin remain unmodified, with no new dependency or C build library.

The comparison calls the native decoder directly, **without silently taking its rawler fallback**,
and separately calls rawler's public API. DNG application saturation lifting is excluded from
codec comparison; it remains in the production native path. The comparison retains decoded
samples at the sensor origin and compares their common active-area intersection when full
sensor dimensions agree. Geometry mismatches are recorded rather than “fixed” by resizing.

Three metrics are written for each sample/pair:

- Stored sample agreement, before black subtraction.
- Black-subtracted agreement in sensor DN: `sample - site_black`.
- Canonical normalized agreement: `(sample - site_black) / (site_white - site_black)`.

No clipping is used in this diagnostic normalization, so shadow differences remain visible.
Black patterns are aligned at their correct sensor/active origins. Percent identical is exact
numeric equality, not a tolerance band. Means and percentages in the tables are pixel-weighted;
maximum is the largest sample difference. Per-pair metadata mismatch lists cover black, white,
normalized RGB WB coefficients, active area, absolute default crop, orientation, CFA and geometry.
DNG AsShotNeutral is converted to WB for comparison; scalar/broadcast whites are equivalent.
The complete black/white/WB values and rectangles remain in the metadata ledger, not just booleans.

Permanent outputs under `target/raw-comparison`:

| Artifact | Contents |
|---|---|
| `comparison.tsv` | Per-file format/compression, statuses, source labels, component count and opt-in timing columns |
| `agreement.tsv` | Every pair/metric: count, max/mean absolute difference, identical percentage, metadata mismatch list |
| `metadata.txt` | Full compared black/white/WB, crop, active area, orientation and CFA values |
| `*.libraw.meta`, `*.libraw.f32` | Independent LibRaw sensor output and metadata, cached with extraction-schema hashes |
| `*.darktable.tif`, `*.rawtherapee.tif`, logs | Cached third-opinion RGB renders, including CLI errors |
| `fuzz-process-status.json`, `robustness-summary.json` | Isolated-process outcomes, abort cases, panic counts and peak RSS |
| `machine.json`, `measurement-config.txt` | Worker/core counts and whether five-run timings were enabled |

LibRaw **0.22.2** is an external system library used through a small reference executable;
C/header files are generated and compiled under `target/refvec/decoder-comparison-libraw`,
never by Cargo. Float-to-integer conversion is disabled so 16/24/32-bit floating DNGs remain
comparable. Darktable **5.6.2** and RawTherapee **5.13** were exercised through the owner's
Flatpaks, with isolated configuration/cache paths, neutral/default processing and no OpenCL.
Two representative bodies per main family have cached renders. `dcraw_emu` is not installed.

The CLI programs export **RGB**, not an accessible unpacked CFA buffer. Their renders are
retained for third-opinion inspection, but no fictitious “darktable CFA bit-exact” percentage
is reported. CFA/reference percentages below come from LibRaw. The driver records this
capability distinction explicitly. Lossy RGB DNG reference decoding also expands gamma/polynomial
curves: that domain is reported separately from CFA and is not used to manufacture a CFA winner.

## Results and routing

The detailed final numbers and compression rows are in the permanent ledgers above. The table
below records the principal discriminating cases; stored-CFA agreement is distinguished from
metadata/normalization agreement rather than treating every deliberate crop/white policy as
an unpacking bug.

| Format / compression | Bodies / representative IDs | Measured correctness and metadata evidence | Applied recommendation |
|---|---|---|---|
| DNG uncompressed integer; lossless JPEG CFA; Deflate float 16/24/32; uncompressed float 24/32 | Leica M9/M240, iPhone 6s Plus, Pentax K-3, Canon HDR series: 752, 2317, 2318, 915, 1074, 883–885, 4178, 4183 | CFA after black subtraction and normalization: **100% exact**, max/mean **0/0**, both engines vs LibRaw. WB equivalence includes DNG AsShotNeutral. | **Native first, rawler fallback.** Equal basic pixels; preserve native DNG profiles/opcodes and float handling. |
| DNG lossless linear RGB / predictor 7 | iPhone 12 Pro, 4264 | Native stored data **100% exact** to LibRaw. Rawler only **2.467258% exact**, max **65535**, mean **10.168374 DN**; normalized mean **0.000155159**. This is linear RGB, not CFA. | **Native first.** A blanket rawler switch loses correctness. |
| DNG lossy JPEG / linear RGB | Adobe-converted Canon 5D III, 1023, 1033 | Both Rust outputs agree **100%** before application opcodes. LibRaw applies loss-DNG gamma/polynomial expansion during decoding; black/white-only RGB-domain differences are max normalized **0.372534**, mean **0.245921** / **0.270469**. | **Native first.** Rawler does not improve this shared pre-processing/domain gap. No CFA score assigned to RGB data. |
| CR2 lossless JPEG | 5D III / 70D, 771, 784 | Stored CFA is **100% exact** for both vs LibRaw. Native masked-column floating blacks differ by <1 DN from reference, rawler by 1 DN. Pixel-weighted normalized reference mean: native **0.00393352**, rawler **0.01491517**. White and active/crop policies differ. | **Native first, rawler fallback.** Native normalization is closer on this pair. |
| NEF lossless/lossy compressed and uncompressed | D750 / D850, 895, 898, 1840, 1842 | Native after-black CFA **100% exact**, max/mean **0/0**. Rawler **49.557445% exact**, max **6**, mean **0.506283 DN**. Normalized mean vs reference: native **0.00003004**, rawler **0.00171852**. | **Native first, rawler fallback.** |
| NRW uncompressed | P1000 / P950, 3381, 3806 | Both after-black CFA **100% exact**. Native normalized reference mean **0.000206802**, max **0.00438145** from white estimation; rawler normalized **100% exact**, **0/0**. | **Rawler first, native fallback.** NRW is recognized by the same exact `NIKON` maker convention used by rawler, not a list of Coolpix bodies. |
| ARW uncompressed / lossless | A7 III / A7R IV / A1 / A7 IV, 2418, 3481, 4466, 6929 | Stored/black-subtracted CFA **100% exact** to reference. Native white is closer; rawler's normalization differs by up to **0.0688982**. | **Native first, rawler fallback.** |
| ARW2 compressed | A7 III / A7R IV, 2414, 3480 | Across all six ARW files, native after-black reference mean **0.00935972 DN**, **99.983214% exact**, vs rawler **0.489104 DN**, **74.104310% exact**. Native has rarer but larger outliers (max **245** vs **8** DN); rawler additionally has broad white-level normalization error. Native normalized reference mean **5.90e-7**, rawler **0.0136991**. | **Native first.** Preserve the better overall reference agreement; disclose the outliers rather than hiding them. |
| RAF uncompressed Bayer and X-Trans | X-T20 / X-T3 / X-A5 / X-A10, 1177, 2784, 2526, 3570 | Both after-black CFA **100% exact**. Native normalized reference mean **0.000253751**, rawler **0.00000265911**. Rawler wins white-level agreement overall, despite its 1-DN white difference on X-A10. Default crop/WB agree; active borders differ. | **Rawler first, native fallback.** Keep our binning from its mosaic, including X-Trans. Compressed RAF already uses it. |
| RW2 / RWL packing 4 | GH5 / GX80 / GH4 / GX1 / LX3 / GH2; Leica Typ 109 / V-Lux 4 | GH5 native CFA **100% exact**, rawler **99.978601%**, max **1792**, mean **0.00813579 DN**. On the original three RW2 files, native after-black mean **0.00159984**, **99.999692% exact**, vs rawler **0.0137438**, **99.979751%**. Some native white heuristics differ from reference; rare native outliers are also disclosed. | **Native first for packing 4**, rawler fallback. Preserve materially better CFA agreement, while keeping metadata differences visible. |
| RW2 words / packed 2 | FZ50 / FZ8, 2234, 2282 | Rawler stored CFA **100% exact**; native **99.999579%** / **99.999788%**, with outliers **2456** / **2014 DN**. Both black policies differ by ~15 DN from LibRaw here; rawler avoids the pixel outliers. | **Rawler first**, native fallback. |
| RW2 packing 6 | S1 / S1R, 3038, 3652 | Rawler stored and after-black CFA **100% exact**. Native after-black **99.640550%** / **99.198207%**, max **15**, means **0.0237382** / **0.0258821 DN**. Native normalization is slightly closer because white policies differ; exact underlying CFA takes priority. | **Rawler first**, native fallback. |
| RW2 packing 8 | S1R II, 7716 | Both CFA **100% exact**. Normalized reference means native **0.000350049**, rawler **0.000333703**. | **Rawler first**, native fallback. |
| RW2 packed 5/7 | G9 / S5 live-composite, 2348, 6339 | Both stored/after-black CFA **100% exact** to LibRaw, max/mean **0/0**. Metadata/normalization differences remain in the per-file ledger. | **Rawler first**, native fallback; inspect the per-file ledger on updates. |
| PEF compressed / uncompressed | K-1 / K-5 / K100D Super, 2469, 1480, 5804 | Stored CFA agrees; measured floating black corrections cause fractional differences. Native after-black reference mean **0.0687352**, **86.252963% exact**, vs rawler **0.276519**, **72.348058%**. Normalized means **0.00009686** vs **0.00111951**. | **Native first, rawler fallback.** |
| ORF uncompressed words / packed 12 / blocks / two-field | E-1 / E-400 / XZ-2 / E-300 / C5060WZ / C7070WZ, 1800, 2151, 1432, 2878, 1799, 780 | Comparable reference CFA after black is native **100% exact**; rawler E-400 black differs by up to 1 DN. On the four directly aligned files, normalized reference mean native **0.000144248**, rawler **0.00162277**. Two-field geometry differences are recorded rather than silently aligned. | **Native first, rawler fallback.** Compressed ORF still falls back to rawler. |

RW2/RWL routing reads the packing tag, not a model name. Rawler-primary failure can use native
code if that succeeds; a safety Limit never falls through to an allocation-heavy decoder. Native
Corrupt errors remain errors on native-first paths. Missing camera calibration still uses rawler's
public data after our first-choice matrix table; embedded/file calibration wins.

## Robustness

The harness reuses the actual builders in `robust.rs` / `vendor_robust.rs` rather than inventing
a separate permissive corpus. It includes every existing stride-3 vendor truncation and stride-7
DNG truncation, 2,000 vendor mutations, 400 DNG mutations, 400 random TIFF-like inputs, 400
malformed LJPEG streams, and the saved minimized `robust.proptest-regressions` case. There are
**7,947** identical input files for each backend. This is deterministic replay of the existing
strategies, not a claim to exhaust every possible mutation.

Each backend is run in isolated processes with four workers and a **768 MiB virtual-memory cap**.
The cap protects the shared machine from allocator aborts; it is a test-process limit, not an app
configuration or a hardware-tuned default. Rawler aborts are recorded with their input and the
remaining cases continue in fresh processes. Peak physical RSS is measured per process with
`/usr/bin/time` and `/proc`, including peak before abort, not guessed from vector capacities.

| Backend | Successful decodes | Returned errors | Escaped panics | Internally caught panics | Allocation aborts | Peak RSS |
|---|---:|---:|---:|---:|---:|---:|
| Native-only | 895 | 7,052 | 0 | 0 | 0 | 32,296 KiB |
| Unprotected rawler public API | 130 | 7,578 | 0 | 55 | **239** | 48,900 KiB under the cap |
| Protected production path (final routing) | 897 | 7,050 | 0 | 1 | **0** | 33,084 KiB |

Success counts are not a quality ranking: many synthetic vendor seeds have model `TEST` or no
model and are deliberately not in rawler's camera database. LJPEG-stream cases also exercise its
direct codec under an outer panic catcher. The important resource result is that unprotected
rawler attempted allocations such as **14,696,841,220 bytes** on a 712-byte TIFF and aborted,
while our boundary rejected them safely. A type-150 malformed SubIFD exposed a gap in the earlier
adapter guard (attempted **2,006,761,958 bytes**); this is now fixed. Preflight validates unknown
field types, all rawler-traversed IFD forms including ICC/undefined pointers, and private RW2
sensor dimensions. Existing nesting/metadata/box/CRX bounds remain.

The protected fuzz run was repeated on the final routing; it completed all 7,947 cases. The
final routing tests cover every CC0 file, without running a whole crate suite. The driver also
supports `--robustness --backends adapter` for a focused future boundary check. Detailed segment/progress logs preserve every
allocator failure. Rawler remains whole/unpatched, so these protections live in our adapter.

## Adapter correction found by this work

The earlier adapter used rawler `CFA::color_at(x,y)`, but that API accepts **(row,column)**.
It transposed asymmetric X-Trans and GRBG/GBRG layouts; RGGB Canon/Olympus layouts hid the issue.
It now calls `color_at(y,x)`. A new test explicitly checks both asymmetric Bayer patterns and
X-Trans. The six compressed-Fuji contract pattern rows were intentionally corrected. Sensor
fingerprints did not change; CFA metadata and the resulting colour development did. The corrected
patterns are compared against the independent LibRaw sensor-origin colour map.

## Validation and rerun after updates

Only `lightcraft-raw` code was touched. No dependency pin/lock change, rawler patch, C build,
GUI launch, GPU test, full suite or timed benchmark was added to this run. Changes are uncommitted.
Check and strict all-target clippy pass. The five new filtered comparison/metric/routing/CFA tests
pass; the isolated fuzz builder and child tests were exercised through their driver. One earlier
broad `--include-ignored` filter invoked the internal fuzz child without its required backend
environment and failed; the retained log explains that invocation error. The documented filter
excludes that internal entry point. No test was weakened to suppress it.

```sh
# Source-pinned corpus and external LibRaw CFA references:
python3 scripts/raw_decoder_comparison.py --fetch --references --threads 4
# Correctness/metadata only (no timed benchmark):
python3 scripts/raw_decoder_comparison.py --run --threads 4
# Existing malformed corpus, both engines plus protected production path:
python3 scripts/raw_decoder_comparison.py --robustness --threads 4
# Optional installed CLI third-opinion renders (CPU only, cache reused):
python3 scripts/raw_decoder_comparison.py --renders --threads 4
# Specific new tests, excluding the internal driver-only fuzz entry points:
PATH=~/.cargo/bin:$PATH cargo +1.98.1 test --offline -p lightcraft-raw --lib decoder_compare_ -- --include-ignored --skip decoder_compare_fuzz_child --skip decoder_compare_build_fuzz_corpus --test-threads=1
# ONLY after timing authorization / merged validation:
python3 scripts/raw_decoder_comparison.py --run --timings --threads 4
```

The timing flag warms each decoder once, then records **median of five** full-size decodes and
end-to-end preview paths in the same pool. Native previews use `develop_binned` where possible
then fit to 512 pixels; rawler's comparison preview performs rescale, full demosaic, active/default
crop and Box downscale. No WB/calibration/tone stage is added to one side alone. The application
uses **our binning from rawler's mosaic**, not the slower rawler full-demosaic comparison path;
therefore a switch's real preview cost is mainly its decode/adapter overhead, not a requirement
to demosaic the whole sensor. That cost still needs the authorized timing run.

After a rawler version bump, follow `docs/RAW-DECODERS.md`, regenerate references only when their
version/extraction schema changes, run these new filtered tests, inspect every metadata mismatch
and per-compression row, then reconsider `native_first_for`. Never turn a divergent reference
into a new golden without investigation. CLI RGB renders do not substitute for CFA validation.

Sources: [rawler public CFA API/source](https://github.com/dnglab/dnglab/blob/v0.8.0/rawler/src/cfa.rs),
[rawler format dispatch](https://github.com/dnglab/dnglab/blob/v0.8.0/rawler/src/decoders/mod.rs),
[LibRaw 0.22.2 DNG decoder/curve behavior](https://github.com/LibRaw/LibRaw/blob/0.22.2/src/decoders/dng.cpp),
[darktable CLI manual](https://docs.darktable.org/usermanual/development/en/special-topics/program-invocation/darktable-cli/),
[RawTherapee CLI documentation](https://rawpedia.rawtherapee.com/Command-Line), and
[raw.pixls.us](https://raw.pixls.us/). Per-file source URLs and SHA-256 values are in the manifest.
