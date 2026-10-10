#!/usr/bin/env python3
"""Reproduce package 00's approved dependency check from offline Cargo metadata.

Run cargo +1.98.1 metadata --offline --locked --format-version 1
--features xtask/vector-bootstrap > target/refvec/vector-00/workspace-metadata.json,
then run this file with that path. --write regenerates the unified allowlist.
This is a source/lock audit, not a rendering or dependency-internal unsafe check.
"""

import argparse
import csv
import hashlib
import json
from pathlib import Path
import subprocess
import tomllib

ROOT = Path(__file__).resolve().parents[4]
BASELINE = "2e8c671fb3733c7b8b5c2cf85b136b2f5578183c"
PINS = {
    "hayro-interpret": "0.7.0",
    "hayro-syntax": "0.7.2",
    "krilla": "0.8.2",
    "kurbo": "0.13.1",
    "linesweeper": "0.4.0",
    "polycool": "0.4.0",
}
OUTPUT = ROOT / "docs/wip/vector-tasks/integration/00-dependencies.tsv"


def key(package):
    return package["name"], package["version"]


def audit(metadata):
    workspace = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["dependencies"]
    for name, version in PINS.items():
        assert workspace[name]["version"] == f"={version}", ("unpinned root", name)
    for name, features in {
        "krilla": ["raster-images"],
        "hayro-interpret": ["embed-fonts", "embed-cmaps"],
        "hayro-syntax": ["std", "images"],
    }.items():
        assert workspace[name]["default-features"] is False, name
        assert workspace[name]["features"] == features, ("unaudited root features", name)
    before = tomllib.loads(subprocess.check_output(
        ["git", "show", f"{BASELINE}:Cargo.lock"], cwd=ROOT, text=True
    ))["package"]
    after = tomllib.loads((ROOT / "Cargo.lock").read_text())["package"]
    baseline = {key(p): p for p in before}
    locked = {key(p): p for p in after}
    assert not baseline.keys() - locked.keys(), "existing version removed or upgraded"
    for k, package in baseline.items():
        assert package.get("checksum") == locked[k].get("checksum"), k
        assert package.get("source") == locked[k].get("source"), k
    with (ROOT / "docs/wip/vector-tasks/pdf-dependencies.tsv").open() as f:
        pdf = list(csv.DictReader(f, delimiter="\t"))
    approved = {(p["crate"], p["version"]): p["checksum"]
                for p in pdf if p["baseline"] == "additional"}
    # V1's one additional registry package; polycool and kurbo are already locked.
    approved[("linesweeper", "0.4.0")] = "9c19728333c060c6569a53c9a0e56c4be0df52cb4e6e07a8fbe16084cecce769"
    added = {k for k, p in locked.items() if p.get("source") and k not in baseline}
    assert added == approved.keys(), ("unapproved/missing additions", added ^ approved.keys())
    for k in added:
        assert locked[k]["checksum"] == approved[k], ("checksum", k)

    packages = {p["id"]: p for p in metadata["packages"]}
    nodes = {n["id"]: n for n in metadata["resolve"]["nodes"]}
    roots = {p["id"] for p in packages.values() if PINS.get(p["name"]) == p["version"]}
    assert len(roots) == len(PINS), "missing pinned dependency root"
    seen, todo = set(), list(roots)
    while todo:
        ident = todo.pop()
        if ident in seen:
            continue
        seen.add(ident)
        todo.extend(d["pkg"] for d in nodes[ident]["deps"])

    rows = []
    for ident in sorted(seen, key=lambda i: key(packages[i])):
        package, node = packages[ident], nodes[ident]
        k = key(package)
        assert package["source"].startswith("registry+"), ("non-registry dependency", k)
        manifest = Path(package["manifest_path"])
        source = tomllib.loads(manifest.read_text())
        assert not package["name"].endswith("-sys"), ("native dependency", k)
        assert package["name"] not in {"cc", "cmake", "bindgen"}, k
        assert not source["package"].get("links"), ("native links", k)
        custom_build = any("custom-build" in t["kind"] for t in package["targets"])
        if k in added:
            assert not custom_build, ("new build script", k)
            assert any("lib" in t["kind"] or "proc-macro" in t["kind"] for t in package["targets"]), k
        license_expression = package["license"]
        assert license_expression and any(s in license_expression for s in
            ["MIT", "Apache-2.0", "BSD", "0BSD", "BSL-1.0", "Zlib", "Unicode-3.0"]), ("license", k)
        assert "AGPL" not in license_expression, k
        archive = manifest.parents[3] / "cache" / manifest.parents[1].name / f"{k[0]}-{k[1]}.crate"
        assert hashlib.sha256(archive.read_bytes()).hexdigest() == locked[k]["checksum"], ("cached archive checksum", k)
        if package["name"] == "krilla":
            assert set(node["features"]) == {"raster-images"}, node["features"]
        if package["name"] == "hayro-interpret":
            assert set(node["features"]) == {"embed-cmaps", "embed-fonts"}, node["features"]
        if package["name"] == "hayro-syntax":
            assert set(node["features"]) == {"images", "std", "unsafe"}, node["features"]
        if package["name"] == "flate2":
            assert "rust_backend" in node["features"], node["features"]
            assert not any("zlib" in f for f in node["features"]), node["features"]
        rows.append([*k, license_expression, ",".join(node["features"]), locked[k]["checksum"],
                     "additional" if k in added else "existing", "existing" if custom_build else "none"])
    assert added <= {tuple(r[:2]) for r in rows}, "added package outside the approved graph"
    return rows


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("metadata", type=Path)
    parser.add_argument("--write", action="store_true")
    args = parser.parse_args()
    rows = audit(json.loads(args.metadata.read_text()))
    header = ["crate", "version", "license", "features", "checksum", "baseline", "build_script"]
    if args.write:
        with OUTPUT.open("w") as f:
            csv.writer(f, delimiter="\t", lineterminator="\n").writerows([header, *rows])
    with OUTPUT.open() as f:
        assert list(csv.reader(f, delimiter="\t")) == [header, *rows], "unified allowlist out of date"
    print(f"Approved unified graph: {len(rows)} registry packages; "
          f"{sum(r[5] == 'additional' for r in rows)} additions; no version upgrades or new build scripts")


if __name__ == "__main__":
    main()
