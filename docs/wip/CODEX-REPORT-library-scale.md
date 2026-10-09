# Library performance at event scale

Work continues the branch's existing grid virtualization, thumbnail-input memoization, priority/cancellation handling, content/version keyed caches, bounded import probing, and neighbour prefetch. No commits, dependencies, unsafe code, GPU changes, or Smart Sort changes were added.

## Reproduction

Paired benchmark processes set `RAYON_NUM_THREADS=4` and explicitly disable GPU rendering.

All builds use `PATH="$HOME/.cargo/bin:$PATH"`, `CARGO_BUILD_JOBS=3`, `cargo +1.98.1`, and `--offline`. Only one cargo build runs at a time.

```sh
CARGO_BUILD_JOBS=3 cargo +1.98.1 test --release --offline -p lightcraft-engine --test library_scale -- --ignored --nocapture --test-threads=1
CARGO_BUILD_JOBS=3 cargo +1.98.1 test --release --offline -p lightcraft-ui-egui --test library_scale -- --ignored --nocapture --test-threads=1
```

The engine benchmark defaults to 2,000 files: 1,600 uncompressed 6000×4000, 14-bit Bayer DNGs written using `lightcraft_raw::write_dng`, each with a 1536×1024 embedded JPEG, plus 400 full-resolution 6000×4000 JPEGs. Deterministic gradients provide nonuniform sensor values; the full JPEGs also include deterministic noise. Appended identifiers make content hashes unique without changing pixels. Fixture construction is outside the timers; temporary files use about 77 GB under the worktree's ignored `.library-scale-artifacts/` directory, and are removed on completion. File generation holds one template, rather than 2,000 images in RAM.

Useful environment variables:

- `LIBRARY_BENCH_DIR=/read-only/folder`: recursively discover supported real files, reference them in Add mode, and process at most `LIBRARY_BENCH_N` of them. The originals are read only. Catalogs, smart previews, cache files, and exports go into disposable directories under this worktree's `.library-scale-artifacts/`. Auto-XMP writing is disabled. No source paths, names, or per-photo errors are printed.
- `LIBRARY_BENCH_N=2000`: actual file count. Query sizes stay at 2,000 and 20,000.
- `LIBRARY_BENCH_CASE=queries,thumbnails,smart,export`: select phases. File phases always include import and catalog close/open.
- `LIBRARY_BENCH_RAW_WIDTH=6000`: synthetic sensor width, with 3:2 height. Smaller values are for smoke tests, not event-scale claims.
- `LIBRARY_BENCH_KEEP_SYNTHETIC=1`: retain only the generated fixture for paired runs; `LIBRARY_BENCH_SYNTHETIC_DIR` reuses it without writing it. These are separate from the real-input option.

The engine prints phase wall time and RSS sampled every 10 ms on Linux. Query rows time 20 queries; reported per-query latency divides by 20. First-thumbnail rows time the first completed result; the first-visible-screen row waits for the first 24 photos. Scratch directories are created atomically with unique names, so cleanup never adopts an existing directory. The thumbnail pipeline limits submitted/in-flight/completed work to 12 items with four workers. Disk reuse empties the memory cache and asserts a disk hit for every photo. Export renders 200 photos to 2048-pixel JPEGs using the actual batch writer. Smart previews are actual editable proxies, not embedded JPEG stand-ins.

The UI benchmark uses the existing `headless.rs` harness with real wheel events and next/previous keyboard events, without a window or GPU. Scroll measurements cover both grids and both catalog sizes (120 frames, downward then upward). Its metadata-only scroll workload separates UI cost from raw decode. Loupe uses deterministic 768×512 working-space sources, waits for the actual detail panel's neighbour prefetch, and measures input through a completed Main texture (20 switches each direction). It reports median/p95 and process high-water RSS. With `LIBRARY_BENCH_DIR`, it also imports real files read-only into an in-memory session and runs the same neighbour navigation using the native decoder; the engine benchmark separately measures full sensor-size file work.

## Changes

- `lc-raw`: bounded seek-based TIFF/DNG/Sony embedded-preview extraction. Reads metadata directories and JPEG ranges, skips sensor payloads, preserves largest-preview selection and orientation, and falls back to the original buffer extractor for unsupported layouts. Metadata reads are capped at 1 MiB, JPEG allocations at 64 MiB each, directory traversal at 64 IFDs, and directory entries at 8,192. Invalid ranges, cycles, duplicate tags, ambiguous multiple Exif directories, and unsupported vendor layouts fall back. Lossless JPEG sensor strips are rejected using at most a 4 KiB prefix; child next pointers and maker-preview traversal match the original buffer parser.
- `lc-engine/files.rs`: use that extractor for the first grid/Loupe image; bound working memory around both fast and fallback preview reads. TIFF import metadata uses the existing parsed directory facts instead of copying the entire sensor file into an EXIF buffer several times. The full-sensor clipping-confidence pass processes independent tiles in parallel with unchanged per-tile arithmetic. Full developed thumbnails and smart-preview pixels retain their existing pipelines.
- `lc-catalog`: lowercase search tokens once per query and filename sort keys once per photo; use a set for large explicit ID filters instead of scanning the selection once per photo. Other sort keys use an in-place unstable sort with the existing unique-ID tie break, preserving total order.
- `lc-preview`: account for replaced disk files by size delta, serialize writes/pruning/clearing, subtract corrupt-file removals, and avoid deleting a concurrent replacement after reading an old corrupt file. JPEG encoding stays outside the maintenance lock.
- `lc-engine`: bulk previews and smart proxies use a maximum of four workers with weighted memory permits held through decoding/rendering/encoding. Export computes at most four results ahead; names, collisions, sidecars, original-file protection, and writes retain selection order. Cancellation writes no later results and leaves at most one bounded speculative wave to finish. Full-size render work runs alone under the default batch budget.
- `lc-ui-egui`: neighbour prefetch borrows the cached visible list instead of cloning all photo IDs every settled detail frame.

Image appearance and export bytes remain unchanged apart from the existing volatile ICC profile creation timestamp (header bytes 24–36); encoded-byte comparisons normalize that timestamp. The embedded JPEG continues to be a temporary stand-in replaced by the developed thumbnail. Cache keys/version are unchanged because rendered pixels are unchanged. No persisted schema fields change; old settings and documents retain their load behavior.

## Measurements

Pending paired release measurements.

## Validation

Pending final test and lint results.

No new GPU code or GPU tests are needed: these changes are CPU/file/UI scheduling paths. The required existing GPU tests are included in the engine test run; this sandbox has no GPU adapter. Real ARW timings remain a coordinator run using the environment variable above; no owner files are needed for the synthetic measurements.
