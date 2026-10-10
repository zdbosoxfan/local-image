#!/usr/bin/env python3
"""Compile the owned helper with package-00's module declaration under target/ only."""
from pathlib import Path
import re
import shutil

root = Path(__file__).resolve().parents[4]
out = root / 'target/pencil-vector-registration'
out.mkdir(exist_ok=True)
manifest = (root / 'Cargo.toml').read_text()
manifest = re.sub(r'members = .*', 'members = ["crates/pc-vector"]', manifest, count=1)
manifest = re.sub(r'default-members = .*', 'default-members = ["crates/pc-vector"]', manifest, count=1)
manifest = re.sub(r'path = "(crates/[^\"]+)"', lambda m: 'path = "' + str(root / m[1]) + '"', manifest)
(out / 'Cargo.toml').write_text(manifest)
shutil.copy2(root / 'Cargo.lock', out / 'Cargo.lock')
vector = out / 'crates/pc-vector'
shutil.copytree(root / 'crates/pc-vector', vector, dirs_exist_ok=True)
lib = vector / 'src/lib.rs'
lib.write_text(lib.read_text() + '\npub mod pencil_fit;\n')
engine = out / 'crates/pc-engine'
if not engine.exists():
    engine.symlink_to(root / 'crates/pc-engine', target_is_directory=True)
print(out)
