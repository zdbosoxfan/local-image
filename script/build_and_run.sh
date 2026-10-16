#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
mode="${1:-run}"
case "$mode" in
    run|--verify|--debug|--logs|--telemetry) ;;
    *) echo "usage: $0 [run|--verify|--debug|--logs|--telemetry]" >&2; exit 2 ;;
esac
pkill -x local-image || true
cargo xtask package macos
app="target/macos-package/Local Image.app"
if [[ "$mode" == --debug ]]; then
    exec lldb -- "$app/Contents/MacOS/local-image"
fi
open -n "$app"
case "$mode" in
    --verify)
        sleep 1
        pgrep -x local-image >/dev/null
        ;;
    --logs|--telemetry)
        exec /usr/bin/log stream --info --style compact --predicate 'process == "local-image"'
        ;;
esac
