"""Portable Linux queue paths. Configuration never creates files or launches work."""
import os
from pathlib import Path


def absolute(value, label):
    path = Path(value).expanduser()
    if not str(value).strip() or not path.is_absolute():
        raise ValueError(label + ' must be a nonempty absolute path')
    if any(c in str(path) for c in ('\0', '\n', '\r')):
        raise ValueError(label + ' contains an invalid path character')
    if any(node.is_symlink() for node in [path, *path.parents]):
        raise ValueError(label + ' must not use symlinked paths')
    return path.resolve()


def dev_dir():
    return absolute(os.environ.get('LI_DEV_DIR', Path.home() / '.local/share/local-image-dev'), 'LI_DEV_DIR')


def source_repository():
    root = Path(__file__).resolve().parents[3]
    return root if (root / 'Cargo.toml').is_file() and (root / 'crates').is_dir() and (root / '.git').exists() else None


def repository(required=True):
    value = os.environ.get('LI_REPO')
    root = absolute(value, 'LI_REPO') if value is not None else source_repository()
    if root is None:
        if required:
            raise ValueError('Installed tools require LI_REPO pointing to the intended Local Image checkout')
        return None
    if not (root / 'Cargo.toml').is_file() or not (root / 'crates').is_dir() or not (root / '.git').exists():
        raise ValueError('LI_REPO must name a Local Image Git checkout root (Cargo.toml, crates and .git)')
    return root


def validate_state_dir(path, repo=None):
    path = absolute(path, 'LI_DEV_DIR')
    roots = [p for p in (repo, source_repository(), repository(required=False)) if p is not None]
    if any(path == root or root in path.parents for root in roots):
        raise ValueError('Queue runtime state must be outside the source checkout')
    return path


def build_state_dir():
    return validate_state_dir(absolute(os.environ.get('LI_BUILD_STATE_DIR', '/tmp/li-build-lanes'), 'LI_BUILD_STATE_DIR'))


def recovery_dir():
    value = os.environ.get('LI_RECOVERY_DIR')
    return absolute(value, 'LI_RECOVERY_DIR') if value is not None else None
