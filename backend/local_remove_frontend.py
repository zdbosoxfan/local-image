"""Assemble the editor's local assets into one nonce-protected document.

Keeping delivery at /remove preserves the desktop host's trusted-page boundary.
The sources remain separate for development and require no network dependencies.
"""
import base64
from html import escape
import json
from pathlib import Path
import re


RESOURCE_DIR = Path(__file__).resolve().parent
ASSET_PREFIX = '/frontend-assets/'
ENTRY_POINT = 'src/main.tsx'
# Vite's deliberately flat, hashed asset output is the only public directory.
# Source maps, manifests, Python sources and user state are never public assets.
ASSET_NAME = re.compile(r'assets/[A-Za-z0-9_.-]+-[A-Za-z0-9_-]{8,}\.(?:js|css|woff2?|ttf|otf|png|jpe?g|webp|svg|ico)\Z')
ASSET_TYPES = {'.js': 'text/javascript', '.css': 'text/css', '.woff': 'font/woff',
               '.woff2': 'font/woff2', '.ttf': 'font/ttf', '.otf': 'font/otf',
               '.png': 'image/png', '.jpg': 'image/jpeg', '.jpeg': 'image/jpeg',
               '.webp': 'image/webp', '.svg': 'image/svg+xml', '.ico': 'image/x-icon'}


def _asset_path(name: str) -> Path:
    if not isinstance(name, str) or not ASSET_NAME.fullmatch(name):
        raise ValueError('Invalid frontend asset name')
    package_root = RESOURCE_DIR.resolve()
    root = (package_root / 'frontend_dist').resolve()
    candidate = (root / name).resolve()
    if not root.is_relative_to(package_root) or not candidate.is_relative_to(root) or not candidate.is_file():
        raise ValueError('Frontend asset is missing or outside its package')
    return candidate


def frontend_manifest() -> tuple[dict, list[str], frozenset[str]]:
    """Validate the reachable Vite manifest graph, including lazy chunks."""
    try:
        manifest = json.loads((RESOURCE_DIR / 'frontend_dist' / '.vite' / 'manifest.json').read_text(encoding='utf-8-sig'))
        if not isinstance(manifest, dict) or not manifest[ENTRY_POINT].get('isEntry'):
            raise ValueError('Missing frontend entry')
        visited, eager, assets = set(), [], set()

        def visit(key, is_eager=True):
            # A shared chunk first seen through a lazy import can still be an
            # eager dependency of a later branch; promote that branch as needed.
            state = (key, is_eager)
            if state in visited:
                return
            visited.add(state)
            chunk = manifest[key]
            if not isinstance(chunk, dict):
                raise ValueError('Invalid frontend chunk')
            names = [chunk['file']]
            for field in ('css', 'assets'):
                values = chunk.get(field, [])
                if not isinstance(values, list):
                    raise ValueError('Invalid frontend assets')
                names.extend(values)
            for name in names:
                _asset_path(name)
                assets.add(name)
            if is_eager:
                eager.append(key)
            for field in ('imports', 'dynamicImports'):
                values = chunk.get(field, [])
                if not isinstance(values, list):
                    raise ValueError('Invalid frontend imports')
                for dependency in values:
                    visit(dependency, is_eager and field == 'imports')

        visit(ENTRY_POINT)
        return manifest, eager, frozenset(assets)
    except (OSError, ValueError, KeyError, TypeError, AttributeError) as error:
        raise RuntimeError('The React frontend build is unavailable. Build frontend/ before packaging or reinstall a complete Local Image package.') from error


def frontend_asset(name: str) -> tuple[Path, str]:
    """Resolve only manifest-listed hashed files; this is not a static mount."""
    if not isinstance(name, str) or not ASSET_NAME.fullmatch(name):
        raise FileNotFoundError(name)
    try:
        _, _, allowed = frontend_manifest()
        if name not in allowed:
            raise FileNotFoundError(name)
        path = _asset_path(name)
        return path, ASSET_TYPES[path.suffix]
    except (RuntimeError, ValueError) as error:
        raise FileNotFoundError(name) from error


def _script_json(value) -> str:
    return json.dumps(value, separators=(',', ':')).replace('&', '\\u0026').replace('<', '\\u003c').replace('>', '\\u003e').replace('\u2028', '\\u2028').replace('\u2029', '\\u2029')


def _module_tags(nonce: str) -> tuple[str, str]:
    manifest, eager, _ = frontend_manifest()
    css, preloads = [], []
    for key in eager:
        chunk = manifest[key]
        for name in chunk.get('css', []):
            if name not in css:
                css.append(name)
        if key != ENTRY_POINT:
            preloads.append(chunk['file'])
    attributes = f'nonce="{escape(nonce, quote=True)}" crossorigin'
    head = ''.join(f'<link rel="stylesheet" {attributes} href="{ASSET_PREFIX}{name}">\n' for name in css)
    head += ''.join(f'<link rel="modulepreload" {attributes} href="{ASSET_PREFIX}{name}">\n' for name in preloads)
    script = f'<script type="module" {attributes} src="{ASSET_PREFIX}{manifest[ENTRY_POINT]["file"]}"></script>\n'
    return head, script


def render_editor(nonce: str, token: str) -> str:
    html = (RESOURCE_DIR / 'frontend' / 'react.html').read_text(encoding='utf-8-sig')
    head, module = _module_tags(nonce)
    # This template contains only the persistent Canvas2D subtree and mounts.
    # The module owns every control.
    html = html.replace('__FRONTEND_HEAD__', head).replace('__FRONTEND_MODULE__', module)
    html = html.replace('__BOOTSTRAP__', _script_json({'nonce': nonce, 'token': token}))
    html = html.replace('__NONCE__', escape(nonce, quote=True))
    if '__APP_ICON__' in html:
        icon = base64.b64encode((RESOURCE_DIR / 'frontend' / 'app-icon.png').read_bytes()).decode('ascii')
        html = html.replace('__APP_ICON__', 'data:image/png;base64,' + icon)
    return html
