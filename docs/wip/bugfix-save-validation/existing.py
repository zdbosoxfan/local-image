#!/usr/bin/env python3
"""Run final regressions and existing tests belonging to the changed files once."""
from pathlib import Path
import json
import os
import shlex
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[3]
os.chdir(ROOT)
assert "li-rustc-gate" in os.environ.get("RUSTC_WRAPPER", "")
assert "CARGO_BUILD_JOBS" not in os.environ
OUT = Path(__file__).resolve().parent
packages = ["photocraft-engine", "photocraft-ui-egui", "photocraft-io", "lightcraft-engine", "lightcraft-ui-egui", "local-image"]
command = ["cargo", "test", "--offline", "--no-run", "--message-format=json"]
for package in packages:
    command += ["-p", package]
print("COMMAND: " + shlex.join(command), flush=True)
with (OUT / "test-artifacts.jsonl").open("w") as artifacts, (OUT / "test-artifacts-build.log").open("w") as diagnostics:
    result = subprocess.run(command, stdout=artifacts, stderr=diagnostics)
print("ARTIFACT BUILD EXIT: " + str(result.returncode), flush=True)
if result.returncode:
    sys.exit(result.returncode)

prefixes = {
    "photocraft_engine": ("tests::", "prefs::tests::", "float_cmds::tests::", "vector_integration::tests::"),
    "photocraft_ui_egui": ("develop_layer::tests::", "dialogs::tests::", "discard_ui::tests::", "filmstrip_ui::tests::", "overwrite_original::tests::", "prefs_ui::tests::", "prefs_ui::shortcut_capture_tests::", "tiff_options_ui::tests::", "hotfix_critical_tests::"),
    "photocraft_io": ("psd_export::tests::", "vector_fallback::tests::"),
    "lightcraft_engine": ("tests::", "tests_import::", "tests_xmp::", "export::tests::", "enhance::tests::", "tests_export::", "tests_enhance::"),
    "lightcraft_ui_egui": ("headless::tests::", "menubar::tests::", "panels::left::tests::", "panels::grid::tests::", "i18n::tests::"),
    "local-image": ("library_host::tests::",),
}
# PSD integration coverage, including the changed roundtrip.rs file.
integration_prefixes = {"crates/pc-io/tests/it/main.rs": ("vector_fallback::", "vector::", "psd_structure::", "psd32_export::", "psd_block_layout::", "roundtrip::", "large_psb::")}
seen = set()
failed = False
count = 0
new_count = 0
for line in (OUT / "test-artifacts.jsonl").read_text().splitlines():
    try:
        artifact = json.loads(line)
    except ValueError:
        continue
    executable = artifact.get("executable")
    if artifact.get("reason") != "compiler-artifact" or not artifact.get("profile", {}).get("test") or not executable or executable in seen:
        continue
    seen.add(executable)
    target = artifact["target"]
    name = target["name"]
    relative = str(Path(target["src_path"]).relative_to(ROOT))
    allowed = prefixes.get(name, ()) if target["kind"] != ["test"] else integration_prefixes.get(relative, ())
    if not allowed:
        continue
    listing = subprocess.check_output([executable, "--list", "--format", "terse"], text=True)
    tests = [line.removesuffix(": test") for line in listing.splitlines() if line.endswith(": test")]
    selected = [test for test in tests if "bugfix_save_" in test or test.startswith(allowed)]
    if not selected:
        continue
    regressions = sum("bugfix_save_" in test for test in selected)
    new_count += regressions
    count += len(selected) - regressions
    command = [executable, "--exact", *selected]
    print("COMMAND: " + shlex.join(command), flush=True)
    result = subprocess.run(command)
    print("TEST BINARY EXIT: " + str(result.returncode), flush=True)
    failed |= result.returncode != 0
print("NEW REGRESSIONS SELECTED: " + str(new_count), flush=True)
print("EXISTING TESTS SELECTED: " + str(count), flush=True)
if count == 0 or new_count != 28:
    raise RuntimeError("No existing tests found")
sys.exit(1 if failed else 0)
