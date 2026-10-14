# J2 shared colour math — integration report

Status: ready for owner review. The isolated branch checks below passed.

Base: published `origin/v2` commit `8e2e763b21a0f0c1e6184c2a69ea5043ebfed4e1`.
This branch contains J2 colour math and LUT integration only. It retains the latest v2
library-safety, demosaic, RAW/CMS routing, camera metadata, Basic/WB layout, preferences
and DeviceHealth behavior. There are no edits to `crates/lc-raw` and no Match Look feature changes.

## Shared backend and compatibility

`li-color` absorbs the former `lc-color` implementation. `lightcraft-color` remains a thin
re-export facade so existing callers, including the RAW job, continue to compile. The new
backend has only `serde` as a production dependency; there are no new native dependencies.

The migration preserves public function names and the serialized representation of existing
settings. White points, hue units, clamping and extended-range policies remain explicit:
ICC's stored D50 and a chromaticity-derived D50 are different definitions, not interchangeable
rounding choices. The shared formula kernels retain native f32/f64 paths and necessary
compatibility operation order. These policies must not become copies of the same formula.

Ordinary luminance helpers in Develop, its UI and camera-preview fitting delegate to the
shared backend. Published reference-vector tests and Develop render goldens retain their
original expected values. Match Look call-site changes remain in the separate feature branch.

## LUT behavior

One parser serves `.cube`, `.3dl` and `.look`. Develop gains the latter two formats, including
folder/ZIP import and reopen. Domain metadata must survive import and native/PSD storage.
Legacy public parser APIs remain adapters. Previously stored explicit interpolation choices
remain readable.

The shared tetrahedral sampler uses f64 weights/accumulation and rounds once to f32. Initial
20,000-sample numerical measurements against f64 interpolation found mean absolute error
`4.60366e-8`, versus `1.21020e-7` for the old editor weighted sampler and `2.75255e-7` for the
old ICC difference-form sampler. These are numerical precision measurements, not a universal
claim that tetrahedral interpolation approximates every function better than trilinear.
For example, trilinear interpolation exactly represents a bilinear `r*g` table; tetrahedral
does not. Both the counterexample and a correlated nonlinear table belong in the checks.

Supported LUT dialect limits are explicit: `.3dl` accepts linear identity input shapers and
infers 8/10/12/14/16-bit output scaling; nonlinear shapers are rejected until their separate
1-D transform can be represented. `.look` accepts unmasked baked RGB/RGBA float tables;
masked looks, invalid hex and incomplete floats are rejected. Table tags are scoped to the
LUT block. The old 8-bit 3DL scaling bug is corrected rather than silently turning white into
approximately one quarter of full scale. All original shared-parser oracles still pass.

The GPU counterpart uses the same tetrahedral cell selection with native shader f32 arithmetic;
CPU accumulation uses f64. Existing explicit trilinear document choices remain compatible.

Hardware tests exposed an upstream input-precision limit before the LUT: ordinary high-depth
source pages used RGBA16F. Color Lookup now uses the existing precise-upload path for U16/F32
documents. Working-set estimation and resident invalidation use the same policy; U8 documents,
BlendIf/native-tone behavior, fallback and DeviceHealth recovery stay unchanged. Ordinary
high-depth input pages cost 16 instead of 8 bytes/pixel; mixed high-depth documents can also
promote U8 source pages under this existing document-wide mechanism, and the estimator counts
that cost.

Physical Vulkan checks were repeated successfully on this isolated v2 branch (RTX 5090),
covering both interpolation modes and unpaged/paged rendering:

| Depth | Maximum CPU/GPU difference |
| --- | --- |
| U8 | 1 integer code |
| U16 | 1/32768 |
| F32 | 1.192092896e-7 |

The new test measures U8 differences in integer-code space to avoid a false floating-point
failure at exactly one code, and uses stricter depth-specific bounds for U16/F32. The existing
generic GPU test helper/tolerance is unchanged. Physical 8/10-bit display readback also passed
in the source integration checkout.

## Measured formula corrections

100,000 deterministic vectors compared with independent f64 formula evaluations:

| Conversion | Old maximum / RMS component error | Shared maximum / RMS component error |
| --- | --- | --- |
| SLIC D65 Lab, Lab units | 1.736403e-4 / 1.037042e-5 | 3.814619e-6 / 1.323674e-6 |
| GRD nominal D50 Lab to sRGB, encoded channel | 3.408404e-3 / 9.329066e-5 | 2.980156e-8 / 9.913233e-9 |
| Editor HSL inverse, encoded channel | 4.364567e-7 / 3.902066e-8 | 2.745693e-7 / 2.584034e-8 |

The GRD correction removes a separately rounded matrix while preserving the importer's original
nominal D50 `[.9642, 1, .8249]`. Adobe specifies Lab D50 without establishing a replacement numeric
white for this descriptor; an unproven white-point change was therefore rejected. An independent
exact-rational Bradford/sRGB matrix and high-precision fixtures validate the replacement. Existing
document-Lab compatibility functions retain their original white and byte output. The brush also corrects a hue seam: a tiny
negative turn hue that rounded to exactly one used to emit magenta; the shared endpoint
policy correctly emits red. Profile calibration keeps its historical output.

Native-turn HSV was retained after measuring that an intermediate degree conversion worsened
hue RMS error from 1.708678e-8 to 1.967812e-8 turns. Native f32 Lab versus f64 CIE evaluation
has maximum component error 3.4645910546e-5 over 4,913 XYZ samples. Affinity import preserves
all clipped f32/byte output across 109,989 Lab vectors; the greatest unclipped f64 change is
1.776e-15. Its historical white point and matrices remain explicit compatibility inputs.

Small optimized scalar-kernel diagnostics (local, not app throughput): one million LUT
samples took 22.446 to 26.153 ms with f64 tetrahedral accumulation; 100,000 SLIC colours took
5.092 to 8.844 ms with f64 CIE evaluation; HSL inverse took 2.862 to 1.910 ms. The kernels
allocate no per-pixel memory. These measurements describe tradeoffs, not hardware-tuned limits.

## Validation on the isolated v2 branch

Base: `8e2e763b`. The regenerated lockfile adds the internal `li-color` package and its caller
edges; no new registry or native dependencies are introduced.

- Full `local-image` app check: passed.
- Shared colour kernels: 62 unit tests and 2 independent caller/precision tests passed.
- Develop pipeline: 224 passed, 3 intentionally ignored. Existing reference vectors and render
  hashes are unchanged, and their test sources/fixtures are unchanged in this PR.
- Existing document colour/compositor/document suites: 31 / 185 / 62 passed.
- ICC and Library codec unit/integration suites: 150 / 117 passed; one intentional ignored
  test in each package.
- LUT command/PSD/GPU-program checks: 4 / 2 / 3 passed. Native document adjustment roundtrip
  passed, including the optional domain metadata and old-document compatibility.
- Workspace formatting and strict `li-color` Clippy for all targets: passed.
- Physical RTX 5090 Vulkan: all 12 depth/interpolation/paging LUT cases passed, with required
  hardware enforced. U8 maximum difference is 1 code; U16 is 1/32768; F32 is 1.192092896e-7.

The implementation was also tested in its original integration checkout: 8/10-bit hardware
display readback passed with the identical display-shader source, as did the larger codec
suite and the downstream feature checks. Seven old-source numeric bit fingerprints match
for matrices, Lab, Bradford, primary matrices, ICC curves and serialized transfer curves.
Portable accuracy tests accompany Linux/x86 last-bit fingerprints because platform powf/cbrt
implementations can vary at the final bit.

TEST-CHANGED: no Develop expected reference vectors or render hashes changed. Existing LUT
constructor tests gain the optional domain field where required; old-document serialization,
PSD/native metadata roundtrips and new domain parity are additionally checked. New tests
cover the explicitly measured corrections instead of weakening existing assertions.

## Scope notes

Additional small call-site edits beyond the initial audit inventory include plain luma in
codecs, compositor effects, editor channels/patterns/exports, SVG masks and Library culling;
transfer forwarding in restoration; Affinity import and vector selection; the reference
fixture generator and existing attribution source paths. These remove the same formulas or
keep moved fixture/provenance paths correct. The new native document field and GPU/PSD
adapters are required to stop discarding non-default Cube input domains.

## Reference definitions

Published definitions were checked against the [ICC sRGB registry](https://www.color.org/chardata/rgb/srgb.xalter),
[CIE Lab definition](https://cie.co.at/eilv/157), [ICC.1:2022](https://www.color.org/specification/ICC.1-2022-05.pdf),
[Oklab author's specification](https://bottosson.github.io/posts/oklab/) and
[Academy CLF interpolation specification](https://docs.acescentral.com/clf/specification/).
Parser edge cases were checked against OpenColorIO's
[3DL reader](https://github.com/AcademySoftwareFoundation/OpenColorIO/blob/main/src/OpenColorIO/fileformats/FileFormat3DL.cpp)
and [Iridas Look reader](https://github.com/AcademySoftwareFoundation/OpenColorIO/blob/main/src/OpenColorIO/fileformats/FileFormatIridasLook.cpp).
