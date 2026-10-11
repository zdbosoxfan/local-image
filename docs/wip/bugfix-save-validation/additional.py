#!/usr/bin/env python3
"""Run inline test modules missed by the initial changed-file module selection."""
import json
from pathlib import Path
import shlex
import subprocess
import sys

directory = Path(__file__).resolve().parent
prefixes = {
    "photocraft_engine": ("vector_integration::integration_tests::",),
    "lightcraft_engine": ("cmd::convert::tests::", "cmd::library::gps_tests::"),
    "lightcraft_ui_egui": ("drop_tests::", "cache_tests::"),
    "photocraft_ui_egui": ("clipboard_tests::",),
}
count = 0
failed = False
for line in (directory / "test-artifacts.jsonl").read_text().splitlines():
    artifact = json.loads(line)
    target = artifact.get("target", {})
    if not artifact.get("executable") or not artifact.get("profile", {}).get("test") or target.get("kind") != ["lib"] or target.get("name") not in prefixes:
        continue
    executable = artifact["executable"]
    listing = subprocess.check_output([executable, "--list", "--format", "terse"], text=True)
    names = [line.removesuffix(": test") for line in listing.splitlines() if line.endswith(": test")]
    names = [name for name in names if name.startswith(prefixes[target["name"]])]
    count += len(names)
    command = [executable, "--exact", *names]
    print("COMMAND: " + shlex.join(command), flush=True)
    result = subprocess.run(command)
    print("TEST BINARY EXIT: " + str(result.returncode), flush=True)
    failed |= result.returncode != 0
print("ADDITIONAL EXISTING TESTS: " + str(count), flush=True)
sys.exit(1 if failed or count != 13 else 0)
