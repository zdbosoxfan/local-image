#!/usr/bin/env bash
set -euo pipefail
cd -- "$(dirname -- "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_BUILD_JOBS=3
cargo +1.98.1 run --offline -p lightcraft-engine --example tagging_eval_fetch
