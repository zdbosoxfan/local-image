"""Page-specific browser credentials signed by the backend's process secret.

There is deliberately no timer: a native file picker may remain open for hours.
A backend restart rotates the existing secret, just as it did before migration.
"""
import hashlib
import hmac
import re
import secrets


_PAGE_TOKEN = re.compile(r'v1\.([A-Za-z0-9_-]{32})\.([a-f0-9]{64})\Z')


def _signature(secret: str, page: str) -> str:
    return hmac.new(secret.encode('ascii'), ('local-image-browser-v1:' + page).encode('ascii'), hashlib.sha256).hexdigest()


def issue_browser_token(secret: str) -> str:
    page = secrets.token_urlsafe(24)
    return 'v1.' + page + '.' + _signature(secret, page)


def valid_browser_token(token: str, secret: str) -> bool:
    if not isinstance(token, str) or not token.isascii() or len(token) > 128:
        return False
    # Keep trusted in-process callers and existing isolated fixtures compatible.
    # The raw process secret is no longer rendered into browser HTML.
    if secrets.compare_digest(token, secret):
        return True
    match = _PAGE_TOKEN.fullmatch(token)
    return bool(match and secrets.compare_digest(match[2], _signature(secret, match[1])))
