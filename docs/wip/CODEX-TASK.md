# Codex task: hunt the intermittent native crash in test binaries

Under heavy machine load, test binaries occasionally abort with "double free or corruption (out)" (exit 134) or similar
native crashes, without any test failing; run alone they pass. Log: ~/.local/share/local-image-dev/flaky-crashes.log
(first seen: lightcraft-engine --lib during a merge-queue land; earlier reports: "two native GPU crashes did not recur",
package 24 "engine tests crash in parallel but pass serially").
The codebase is pure Rust; candidates are the one crate allowed `unsafe` (lc-sysmem), FFI-free-but-native paths in
dependencies (wgpu/Vulkan driver in GPU tests running concurrently, tract, rayon pools, mmap), process-wide globals mutated
by parallel tests, and tests that fork child processes.
Do: reproduce under load (run the affected test binaries repeatedly in parallel with CPU/GPU stress; capture core dumps with
`coredumpctl`, backtraces, and which test was running — use --test-threads and per-test logging); find the root cause;
fix it (or, if it is a driver bug, isolate GPU tests behind the existing GPU test lock and document). Add a stress
regression if feasible. Report docs/wip/CODEX-REPORT-crash-hunt.md with evidence. Rules as usual: li-rustc-gate, Rust
1.98.1, --offline, leave changes uncommitted, never stop the running app or ComfyUI.

Also: make every test listed in ~/.local/share/local-image-dev/flaky-tests.log robust under machine load (no wall-clock
budgets that fail when CPUs are saturated — measure work done / use logical deadlines or generous budgets with a
documented reason). Re-read that log when you finish; more entries may have appeared.
