"""Assemble the editor's local assets into one nonce-protected document.

Keeping delivery at /remove preserves the desktop host's trusted-page boundary.
The sources remain separate for development and require no network dependencies.
"""
import base64
from pathlib import Path


RESOURCE_DIR = Path(__file__).resolve().parent


def render_editor(nonce: str, token: str) -> str:
    # Windows editors may write a BOM. Inside an inline style block it becomes
    # part of the first selector and can silently invalidate the design tokens.
    template = (RESOURCE_DIR / 'local_remove.html').read_text(encoding='utf-8-sig')
    frontend = RESOURCE_DIR / 'frontend'
    styles = ('workflow-panels.css', 'generation-workflows.css', 'batch-tools.css',
              'editor.css', 'usability.css', 'stock-studio.css', 'generation-studio.css',
              'quiet-controls.css', 'layers-studio.css', 'generation-composer.css', 'stock-connections.css')
    scripts = ('editor.js', 'usability.js', 'batch-tools.js', 'studio-shell.js',
               'stock-studio.js', 'generation-studio.js', 'quiet-controls.js',
               'layers-studio.js', 'generation-size.js', 'generation-composer.js',
               'generation-guidance.js', 'stock-connections.js')
    html = template.replace('__EDITOR_STYLE__', '\n'.join((frontend / name).read_text(encoding='utf-8-sig')
        for name in styles if (frontend / name).is_file()))
    html = html.replace('__EDITOR_SCRIPT__', '\n'.join((frontend / name).read_text(encoding='utf-8-sig')
        for name in scripts if (frontend / name).is_file()))
    if '__APP_ICON__' in html:
        icon = base64.b64encode((frontend / 'app-icon.png').read_bytes()).decode('ascii')
        html = html.replace('__APP_ICON__', 'data:image/png;base64,' + icon)
    return html.replace('__NONCE__', nonce).replace('__TOKEN__', token)
