"""Assemble the editor's local assets into one nonce-protected document.

Keeping delivery at /remove preserves the desktop host's trusted-page boundary.
The sources remain separate for development and require no network dependencies.
"""
from pathlib import Path


RESOURCE_DIR = Path(__file__).resolve().parent


def render_editor(nonce: str, token: str) -> str:
    # Windows editors may write a BOM. Inside an inline style block it becomes
    # part of the first selector and can silently invalidate the design tokens.
    template = (RESOURCE_DIR / 'local_remove.html').read_text(encoding='utf-8-sig')
    frontend = RESOURCE_DIR / 'frontend'
    html = template.replace('__EDITOR_STYLE__', (frontend / 'editor.css').read_text(encoding='utf-8-sig'))
    html = html.replace('__EDITOR_SCRIPT__', (frontend / 'editor.js').read_text(encoding='utf-8-sig'))
    return html.replace('__NONCE__', nonce).replace('__TOKEN__', token)
