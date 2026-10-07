#!/usr/bin/env bash
set -euo pipefail
repository=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
task_python=$(command -v python3 || command -v python3.12)
exec "$task_python" "$repository/packaging/build_linux.py" "$@"
