# Rotate View all-target constructor correction

Candidate-based, twelve files only. Patch SHA256: 2b653d220bdc3e9431e5b2bf6e4b23089dff5cfa0605f412e9be29ffb707846a.

The desktop all-target compile identified twelve acceptance/performance helper mappings missed by the earlier src-only inventory. Each helper reads a real `ui.views[0]`; the added rotation comes from that same `v` or `view` value. Existing flip behavior, inputs, fixtures and every assertion remain unchanged. TEST-CHANGED: mechanical required ViewXform field migration only.

Reviewed all apps/crates Rust ViewXform literal sites, including examples and external integration-test modules: 56 sites, zero remaining missing explicit rotation fields (struct-update literals inherit their source). Function return types and definitions were excluded before brace traversal, not edited.

The candidate was read only. Updated files and exact before copies are archived here; apply constructors.patch to candidate. Do not overwrite unrelated files. canvas Panel, rotate_view test-module path, and examples/layout_bench rotation fixes are absent from this patch and preserved. Immutable activation v1/v2 snapshots and source checkout remain unchanged.

Standalone rustfmt only; no Cargo, tests, builds or runtime. Coordinator compilation remains required.
