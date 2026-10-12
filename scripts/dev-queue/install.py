#!/usr/bin/env python3
"""Preflight or explicitly install portable queue source. Never activate services."""
import argparse
import ast
from contextlib import ExitStack
import fcntl
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time

SOURCE = Path(__file__).resolve().parent
sys.path.insert(0, str(SOURCE / 'bin'))
from li_config import absolute, build_state_dir, dev_dir, repository, validate_state_dir

FILES = [
    'bin/li-dispatch', 'bin/li-land', 'bin/li-rework', 'bin/li-rustc-gate',
    'bin/li-test-runner', 'bin/li-sweep-targets', 'bin/update-gate.sh',
    'bin/li_state.py', 'bin/li_validation.py', 'bin/li_config.py',
    'dashboard/dashboard.py', 'codex-launch.sh', 'codex-rules.md', 'README.md',
]
MANIFEST = 'installed-queue-hashes.json'


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def plain_path(path):
    """Refuse symlinked owned files or ancestors, including broken symlinks."""
    for node in [path, *path.parents]:
        if node.is_symlink():
            raise RuntimeError('Refusing symlinked installation path: ' + str(node))
    if path.exists() and not path.is_file():
        raise RuntimeError('Expected an ordinary file: ' + str(path))


def hashes(path):
    plain_path(path)
    if not path.exists():
        return {}
    data = json.loads(path.read_text())
    if not isinstance(data, dict) or any(not isinstance(k, str) or not isinstance(v, str) or len(v) != 64 or any(c not in '0123456789abcdef' for c in v) for k, v in data.items()):
        raise RuntimeError('Invalid reviewed hash manifest: ' + str(path))
    return data


def verify(dev, reviewed=None):
    baseline = hashes(dev / MANIFEST)
    if reviewed is not None:
        if not reviewed.is_file():
            raise RuntimeError('Missing explicitly reviewed hash manifest')
        baseline.update(hashes(reviewed))
    for name in FILES:
        src, dst = SOURCE / name, dev / name
        plain_path(src)
        plain_path(dst)
        if not src.is_file():
            raise RuntimeError('Missing source file: ' + name)
        text = src.read_text()
        if text.startswith('#!/usr/bin/env python3') or name.endswith('.py'):
            ast.parse(text, filename=str(src))
        elif text.startswith('#!/bin/sh'):
            # These are fixed disabled entrypoints, not general shell programs.
            if not text.endswith('exit 2\n') or '$(' in text or '`' in text:
                raise RuntimeError('Unexpected executable stub: ' + name)
        if dst.exists() and digest(dst) not in (digest(src), baseline.get(name)):
            raise RuntimeError('Unreviewed existing file: ' + str(dst) + '; supply an explicitly reviewed --reviewed-hashes manifest')
    return {name: digest(SOURCE / name) for name in FILES}


def assert_idle():
    units = subprocess.run(
        ['systemctl', '--user', 'list-units', 'li-codex-*', 'li-land-*', 'li-validate-*', 'li-dispatch.service',
         '--no-legend', '--plain', '--no-pager', '--state=running,activating,reloading'],
        capture_output=True, text=True, check=True,
    )
    if units.stdout.strip():
        raise RuntimeError('Queue workers/validation are active; wait before installation')
    for entry in Path('/proc').iterdir():
        if not entry.name.isdigit():
            continue
        try:
            comm = (entry / 'comm').read_text().strip()
            args = (entry / 'cmdline').read_bytes().split(b'\0')
        except FileNotFoundError:
            continue
        if comm in ('cargo', 'rustc') or any(Path(os.fsdecode(arg)).name in ('li-rustc-gate', 'li-test-runner') for arg in args if arg):
            raise RuntimeError('A build/test process is active; wait before installation')


def lock_file(stack, path):
    plain_path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    stream = stack.enter_context(path.open('a+'))
    try:
        fcntl.flock(stream, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError as error:
        raise RuntimeError('Queue lock is busy: ' + str(path)) from error


def atomic_bytes(dst, data, mode=0o644):
    plain_path(dst)
    dst.parent.mkdir(parents=True, exist_ok=True)
    fd, temporary = tempfile.mkstemp(prefix='.' + dst.name + '-', dir=dst.parent)
    try:
        with os.fdopen(fd, 'wb') as stream:
            stream.write(data)
            stream.flush()
            os.fchmod(stream.fileno(), mode)
            os.fsync(stream.fileno())
        os.replace(temporary, dst)
        directory = os.open(dst.parent, os.O_RDONLY | os.O_DIRECTORY)
        try:
            os.fsync(directory)
        finally:
            os.close(directory)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


def quote_unit(value, executable=False):
    if any(c in value for c in ('\0', '\n', '\r')):
        raise ValueError('Invalid systemd value')
    value = value.replace('\\', '\\\\').replace('"', '\\"').replace('%', '%%')
    if executable:
        value = value.replace('$', '$$')
    return '"' + value + '"'


def service_text(dev, repo):
    template = (SOURCE / 'systemd/li-dashboard.service').read_text()
    for key, value in {
        '@PYTHON@': quote_unit(sys.executable, executable=True),
        '@SCRIPT@': quote_unit(str(dev / 'dashboard/dashboard.py'), executable=True),
        '@DEV@': quote_unit('LI_DEV_DIR=' + str(dev)),
        '@REPO@': quote_unit('LI_REPO=' + str(repo)),
        '@LANES@': quote_unit('LI_BUILD_STATE_DIR=' + str(build_state_dir())),
    }.items():
        template = template.replace(key, value)
    return template.encode()


def apply(dev, *, reviewed=None, unit_output=None, repo=None):
    """Called only by explicit --apply; tests supply temporary state and mock idle checks."""
    dev = validate_state_dir(dev, repo)
    # Validate every runtime root before creating even the installation locks.
    lanes = validate_state_dir(build_state_dir(), repo)
    # Check paths before mkdir/lock acquisition. An unreviewed target changes no bytes.
    plan = verify(dev, reviewed)
    fresh = not dev.exists() or not any(dev.iterdir())
    if not fresh:
        plain_path(dev / 'queue-hold')
        plain_path(dev / 'jobs.json')
        if not (dev / 'queue-hold').is_file():
            raise RuntimeError('Existing queue must be held; installation never changes its policy')
        state = json.loads((dev / 'jobs.json').read_text())
        if not isinstance(state, dict) or not isinstance(state.get('jobs'), list):
            raise RuntimeError('Existing jobs.json is invalid')
    unit = service_text(dev, repo) if unit_output is not None else None
    if unit_output is not None:
        if repo is None:
            raise RuntimeError('An explicit dashboard unit requires LI_REPO')
        unit_output = absolute(unit_output, '--unit-output')
        validate_state_dir(unit_output, repo)
        if unit_output == dev or dev in unit_output.parents:
            raise RuntimeError('Unit output must be outside the owned runtime tree')
        plain_path(unit_output)
        if unit_output.exists() and unit_output.read_bytes() != unit:
            raise RuntimeError('A different unit already exists; review it separately')
    assert_idle()
    with ExitStack() as stack:
        for name in ('.install.lock', '.dispatch.lock', '.land.lock', '.jobs.lock'):
            lock_file(stack, dev / name)
        lock_file(stack, lanes / '.lock')
        # Do not remove stale markers; refuse all locked pending/running/slot leases.
        for pattern in ('pending.*', '*/run.*', 'slots/slot-*'):
            for marker in lanes.glob(pattern):
                lock_file(stack, marker)
        assert_idle()
        if fresh and any(p.name not in ('.install.lock', '.dispatch.lock', '.land.lock', '.jobs.lock') for p in dev.iterdir()):
            raise RuntimeError('Fresh installation changed while acquiring locks')
        if verify(dev, reviewed) != plan:
            raise RuntimeError('Source changed during preflight')
        payloads = {name: (SOURCE / name).read_bytes() for name in FILES}
        if any(hashlib.sha256(data).hexdigest() != plan[name] for name, data in payloads.items()):
            raise RuntimeError('Source changed while capturing install bytes')
        if not fresh:
            plain_path(dev / 'queue-hold')
            plain_path(dev / 'jobs.json')
            if not (dev / 'queue-hold').is_file():
                raise RuntimeError('Queue hold changed during preflight')
        backup = dev / 'queue-backups' / (time.strftime('%Y%m%d-%H%M%S') + '-' + str(time.time_ns()))
        plain_path(backup)
        backup.mkdir(parents=True)
        previous = []
        for name in [*FILES, MANIFEST]:
            target = dev / name
            if target.exists():
                saved = backup / name
                saved.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(target, saved)
                previous.append(name)
        atomic_bytes(backup / 'receipt.json', (json.dumps({'previous': previous, 'installed': plan}, indent=2) + '\n').encode())
        if fresh:
            atomic_bytes(dev / 'queue-hold', b'Coordinator-only: legacy automatic work remains held.\n')
            atomic_bytes(dev / 'jobs.json', b'{"paused":true,"dispatch_mode":"coordinator","max":3,"build_lanes":3,"compiler_slots":10,"jobs":[]}\n')
        for name in FILES:
            executable = (name.startswith('bin/') and not name.endswith('.py')) or name.endswith('.sh')
            atomic_bytes(dev / name, payloads[name], 0o755 if executable else 0o644)
        atomic_bytes(dev / MANIFEST, (json.dumps(plan, indent=2) + '\n').encode())
        if unit_output is not None:
            atomic_bytes(unit_output, unit)
        return backup


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--apply', action='store_true', help='explicitly copy checked source into an idle, held installation')
    parser.add_argument('--dev-dir', type=Path, help='absolute private runtime/installation directory')
    parser.add_argument('--reviewed-hashes', type=Path, help='explicit reviewed SHA256 manifest for adopting existing tool bytes')
    parser.add_argument('--unit-output', type=Path, help='optionally write a dashboard unit; never enable/reload/start it')
    args = parser.parse_args(argv)
    dev = validate_state_dir(args.dev_dir or dev_dir())
    reviewed = absolute(args.reviewed_hashes, '--reviewed-hashes') if args.reviewed_hashes else None
    unit = absolute(args.unit_output, '--unit-output') if args.unit_output else None
    repo = repository() if unit is not None else repository(required=False)
    verify(dev, reviewed)
    if not args.apply:
        print('Source syntax and target hashes checked. Nothing installed or activated; use --apply at an idle, held boundary.')
        return 0
    backup = apply(dev, reviewed=reviewed, unit_output=unit, repo=repo)
    print('Copied reviewed source. Backup: ' + str(backup))
    print('No services/timers activated and no jobs launched. Legacy automatic work remains held.')
    return 0


if __name__ == '__main__':
    try:
        raise SystemExit(main())
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
        print('Queue installation refused: ' + str(error), file=sys.stderr)
        raise SystemExit(2)
