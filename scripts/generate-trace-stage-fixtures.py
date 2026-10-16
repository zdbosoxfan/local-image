#!/usr/bin/env python3
"""Regenerate Potrace 1.16 numeric stages with an optional, out-of-build C oracle.

Only Python's standard library and a C compiler are needed. No tracer CLI or
Cargo dependency is used. All extracted/compiled C stays in target/refvec/.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tarfile
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
WORK = ROOT / "target/refvec/trace-stages"
FIXTURE = ROOT / "crates/pc-trace/tests/golden/potrace-stages.json"
SHA256 = "be8248a17dedd6ccbaab2fcc45835bb0502d062e40fbded3bc56028ce5eb7acc"
URL = "https://potrace.sourceforge.net/download/1.16/potrace-1.16.tar.gz"
CACHED = Path.home() / ".local/share/local-image-dev/engine-sources/potrace/potrace-1.16.tar.gz"


def boundary(pixels):
    """Clockwise unit edges of our original CC0 masks, starting at min (x, y)."""
    edges = {}
    for x, y in sorted(pixels):
        for neighbour, start, end in [
            ((x, y - 1), (x, y), (x + 1, y)),
            ((x + 1, y), (x + 1, y), (x + 1, y + 1)),
            ((x, y + 1), (x + 1, y + 1), (x, y + 1)),
            ((x - 1, y), (x, y + 1), (x, y)),
        ]:
            if neighbour not in pixels:
                if start in edges:
                    raise ValueError("mask has an ambiguous boundary")
                edges[start] = end
    start = min(edges)
    points = [start]
    point = edges[start]
    while point != start:
        points.append(point)
        point = edges[point]
    if len(points) != len(edges):
        raise ValueError("mask must have one boundary")
    return points


def inputs():
    return [
        ("rectangle", boundary({(x, y) for y in range(3, 11) for x in range(2, 18)})),
        ("disk", boundary({(x, y) for y in range(32) for x in range(32)
                           if (2 * x + 1 - 32) ** 2 + (2 * y + 1 - 32) ** 2 <= 22 ** 2})),
        ("stair", boundary({(x, y) for y in range(2, 22) for x in range(2, 22) if x + y <= 23})),
    ]


def source(archive):
    data = archive.read_bytes()
    if hashlib.sha256(data).hexdigest() != SHA256:
        raise ValueError("Potrace archive SHA-256 differs from the documented 1.16 pin")
    # Extract only regular source files, never links or arbitrary archive paths.
    with tarfile.open(archive) as tar:
        for member in tar:
            path = Path(member.name)
            if member.isfile() and path.parts[:2] == ("potrace-1.16", "src") and ".." not in path.parts:
                dest = WORK / path
                dest.parent.mkdir(parents=True, exist_ok=True)
                with tar.extractfile(member) as stream:
                    dest.write_bytes(stream.read())
    return WORK / "potrace-1.16/src"


HARNESS = r'''
#include "trace.c"

static int emit(const char *name, point_t *points, int n) {
  path_t *p = path_new();
  if (!p) return 1;
  privpath_t *pp = p->priv;
  pp->len = n;
  pp->pt = calloc(n, sizeof(point_t));
  if (!pp->pt) { path_free(p); return 1; }
  memcpy(pp->pt, points, n * sizeof(point_t));
  if (calc_sums(pp) || calc_lon(pp) || bestpolygon(pp) || adjust_vertices(pp)) {
    path_free(p); return 1;
  }
  smooth(&pp->curve, 1.0);
  if (opticurve(pp, 0.2)) { path_free(p); return 1; }
  printf("{\"name\":\"%s\",\"polygon\":[", name);
  for (int i = 0; i < pp->m; i++) printf("%s%d", i ? "," : "", pp->po[i]);
  printf("],\"segments\":[");
  for (int i = 0; i < pp->ocurve.n; i++) {
    int corner = pp->ocurve.tag[i] == POTRACE_CORNER;
    printf("%s{\"corner\":%s,\"points\":[", i ? "," : "", corner ? "true" : "false");
    for (int j = 0; j < 3; j++) {
      /* c[0] is undefined for corners; normalize it for deterministic JSON. */
      double x = corner && j == 0 ? 0 : pp->ocurve.c[i][j].x;
      double y = corner && j == 0 ? 0 : pp->ocurve.c[i][j].y;
      printf("%s[%.17g,%.17g]", j ? "," : "", x, y);
    }
    printf("]}");
  }
  printf("],\"points\":[");
  for (int i = 0; i < n; i++) printf("%s[%ld,%ld]", i ? "," : "", points[i].x, points[i].y);
  printf("]}");
  path_free(p);
  return 0;
}
'''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", type=Path, help="pinned potrace-1.16.tar.gz (otherwise use the local cache or download)")
    parser.add_argument("--check", action="store_true", help="compare regenerated JSON without changing the fixture")
    args = parser.parse_args()
    WORK.mkdir(parents=True, exist_ok=True)
    archive = args.archive or CACHED
    if not archive.exists() and args.archive is None:
        archive = WORK / "potrace-1.16.tar.gz"
        if not archive.exists():
            with urllib.request.urlopen(URL, timeout=60) as response:
                archive.write_bytes(response.read())
    src = source(archive)
    code = HARNESS + "\nint main(void) {\n  printf(\"[\");\n"
    for i, (name, points) in enumerate(inputs()):
        coords = ",".join("{%d,%d}" % point for point in points)
        code += f"  point_t p{i}[] = {{{coords}}};\n"
        if i:
            code += '  printf(",");\n'
        code += f'  if (emit("{name}", p{i}, {len(points)})) return 1;\n'
    code += '  printf("]\\n");\n  return 0;\n}\n'
    harness = WORK / "oracle.c"
    harness.write_text(code)
    executable = WORK / "oracle"
    subprocess.run([os.environ.get("CC", "cc"), "-std=c99", "-O0", "-ffp-contract=off",
                    "-I", str(src), str(harness), str(src / "curve.c"), "-lm", "-o", str(executable)], check=True)
    generated = json.loads(subprocess.check_output([str(executable)], text=True))
    if args.check:
        # JSON numeric equality accepts cosmetic changes such as 0 versus 0.0.
        if generated != json.loads(FIXTURE.read_text()):
            raise SystemExit("regenerated stages differ from the checked-in fixture")
        print("Potrace stages match all 3 checked-in fixtures")
    else:
        FIXTURE.write_text(json.dumps(generated, indent=2) + "\n")
        print(f"Wrote {FIXTURE.relative_to(ROOT)} (3 fixtures)")


if __name__ == "__main__":
    main()
