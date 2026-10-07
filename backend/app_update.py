"""Check GitHub releases for a newer release package and download it verified.

On Windows the package is the Inno Setup installer; on Linux it is the Debian
package, or the archive on systems without dpkg. The page can ask for a check
and a download; only the desktop host, with its launcher credential, may read
the verified package path and open it. Nothing is installed from here: the
installer or the system's package tool runs after the application has closed.
"""
import asyncio
import json
import logging
import re
import shutil
import sys
import time
from urllib.parse import urljoin, urlsplit

import aiohttp
from fastapi import APIRouter, HTTPException, Request

from app_paths import APP_VERSION, data_root
import managed_ai

REPOSITORY = 'zdbosoxfan/local-image'
RELEASES_URL = f'https://api.github.com/repos/{REPOSITORY}/releases?per_page=30'
RELEASE_PAGE = f'https://github.com/{REPOSITORY}/releases'
# Release asset names per platform. The Windows installer publishes its own
# .sha256 file; the Linux build publishes one SHA256SUMS file for both packages.
PACKAGES = {
    'windows': {'name': re.compile(r'^Local-Image-Setup-(\d+(?:\.\d+)+)\.exe$'), 'checksum': None},
    'linux-deb': {'name': re.compile(r'^Local-Image-(\d+(?:\.\d+)+(?:-[A-Za-z0-9.~+-]*)?)-linux-x86_64\.deb$'),
                  'checksum': 'SHA256SUMS'},
    'linux-tar': {'name': re.compile(r'^Local-Image-(\d+(?:\.\d+)+(?:-[A-Za-z0-9.~+-]*)?)-linux-x86_64\.tar\.gz$'),
                  'checksum': 'SHA256SUMS'},
}


def platform_package():
    """Which release asset this installation can install."""
    if sys.platform.startswith('linux'):
        return 'linux-deb' if shutil.which('dpkg') else 'linux-tar'
    return 'windows'


PACKAGE = platform_package()
MAX_RELEASES_BYTES = 4 * 1024 * 1024
MAX_INSTALLER_BYTES = 2 * 1024 * 1024 * 1024
MAX_NOTES = 20000
CHECK_TTL = 15 * 60
USER_AGENT = f'LocalImage/{APP_VERSION}'
GITHUB_HEADERS = {'Accept': 'application/vnd.github+json', 'X-GitHub-Api-Version': '2022-11-28',
                  'User-Agent': USER_AGENT}

logger = logging.getLogger('local-remove.update')
router = APIRouter()
_lock = asyncio.Lock()
_state = {'checked_at': 0.0, 'checked_time': None, 'release': None, 'check_error': '',
          'download': {'status': 'idle', 'received': 0, 'total': 0, 'error': ''},
          'installer': None}
_download_task = None


class UpdateError(ValueError):
    pass


API_HOSTS = {'api.github.com'}


def checked_api_url(url):
    """Release metadata comes only from GitHub's API over HTTPS."""
    parts = urlsplit(url)
    if (parts.scheme != 'https' or parts.username or parts.password or parts.port not in (None, 443)
            or (parts.hostname or '').lower() not in API_HOSTS):
        raise UpdateError('The update check was redirected away from GitHub.')
    return url


def version_tuple(text):
    """Numeric release order; '0.7.10' sorts after '0.7.9'. A Linux preview
    such as '0.7.2-linux-preview' orders by its numeric part."""
    match = re.match(r'(\d+(?:\.\d+)+)(?:$|[-+~])', text)
    if not match:
        raise ValueError('Not a release version: ' + text)
    return tuple(int(part) for part in match.group(1).split('.'))


def parse_checksum(text, name):
    """The release publishes 'HEX  FILENAME'. Both parts must match."""
    for line in text.splitlines():
        parts = line.strip().split()
        if len(parts) == 2 and parts[1].lstrip('*') == name and re.fullmatch(r'[0-9a-fA-F]{64}', parts[0]):
            return parts[0].lower()
    raise UpdateError('The published checksum for the installer could not be read.')


def newest_release(releases, package=None):
    """Newest non-draft release carrying this platform's package and its checksum.

    Pre-releases count: every preview so far is marked that way. Releases for
    the other platform and releases without a verified package are ignored.
    """
    package = package or PACKAGE
    pattern, checksum_name = PACKAGES[package]['name'], PACKAGES[package]['checksum']
    best = None
    for release in releases if isinstance(releases, list) else []:
        if not isinstance(release, dict) or release.get('draft'):
            continue
        assets = [asset for asset in (release.get('assets') or []) if isinstance(asset, dict)]
        for asset in assets:
            name = asset.get('name', '')
            match = pattern.match(name) if isinstance(name, str) else None
            if not match:
                continue
            checksum = next((item for item in assets if item.get('name') == (checksum_name or name + '.sha256')), None)
            url, size = asset.get('browser_download_url'), asset.get('size')
            if (not checksum or not isinstance(url, str) or not isinstance(size, int)
                    or not 0 < size <= MAX_INSTALLER_BYTES or not isinstance(checksum.get('browser_download_url'), str)):
                continue
            try:
                version = version_tuple(match.group(1))
                managed_ai.checked_download_url(url)
                managed_ai.checked_download_url(checksum['browser_download_url'])
            except (ValueError, managed_ai.SetupError):
                continue
            candidate = {'version': match.group(1), 'tag': str(release.get('tag_name') or ''),
                         'name': str(release.get('name') or match.group(1)),
                         'notes': str(release.get('body') or '')[:MAX_NOTES],
                         'html_url': release.get('html_url') if isinstance(release.get('html_url'), str) else RELEASE_PAGE,
                         'published_at': str(release.get('published_at') or ''),
                         'prerelease': bool(release.get('prerelease')),
                         'asset_name': name, 'bytes': size, 'url': url,
                         'checksum_url': checksum['browser_download_url']}
            if best is None or version > version_tuple(best['version']):
                best = candidate
    if best and best['html_url'] != RELEASE_PAGE:
        try:
            managed_ai.checked_download_url(best['html_url'])
        except managed_ai.SetupError:
            best['html_url'] = RELEASE_PAGE
    return best


async def _read_limited(response, limit):
    data = bytearray()
    async for block in response.content.iter_chunked(65536):
        data.extend(block)
        if len(data) > limit:
            raise UpdateError('The GitHub response was too large to be a release list.')
    return bytes(data)


async def _fetch(client, url, limit, checked):
    address = checked(url)
    for _ in range(8):
        async with client.get(address, allow_redirects=False) as response:
            if response.status in (301, 302, 303, 307, 308):
                address = checked(urljoin(address, response.headers.get('Location', '')))
                continue
            if response.status == 403 and response.headers.get('X-RateLimit-Remaining') == '0':
                raise UpdateError('GitHub is rate limiting update checks from this PC. Try again in an hour.')
            if response.status != 200:
                raise UpdateError(f'GitHub could not provide the release information (HTTP {response.status}).')
            return await _read_limited(response, limit)
    raise UpdateError('GitHub redirected the update check too many times.')


async def fetch_release(session_factory=None):
    """Return the newest package release with its verified checksum, or None."""
    timeout = aiohttp.ClientTimeout(total=30, connect=15)
    factory = session_factory or (lambda: aiohttp.ClientSession(timeout=timeout, headers=GITHUB_HEADERS))
    async with factory() as client:
        try:
            releases = json.loads(await _fetch(client, RELEASES_URL, MAX_RELEASES_BYTES, checked_api_url))
        except json.JSONDecodeError as error:
            raise UpdateError('GitHub returned an unreadable release list.') from error
        release = newest_release(releases)
        if not release:
            return None
        checksum = await _fetch(client, release['checksum_url'], 16384, managed_ai.checked_download_url)
        release['sha256'] = parse_checksum(checksum.decode('utf-8', 'replace'), release['asset_name'])
        return release


def is_newer(release):
    try:
        return bool(release) and version_tuple(release['version']) > version_tuple(APP_VERSION)
    except ValueError:
        return False


def updates_dir():
    return data_root() / 'updates'


def public_status():
    release = _state['release']
    download = dict(_state['download'])
    installer = _state['installer']
    return {
        'current_version': APP_VERSION,
        'checked_at': _state['checked_time'],
        'check_error': _state['check_error'],
        'available': is_newer(release),
        'release': None if not release else {
            key: release[key] for key in ('version', 'tag', 'name', 'notes', 'html_url', 'published_at',
                                          'prerelease', 'asset_name', 'bytes')},
        'download': download,
        # The page learns that a verified package exists, never where it is.
        'installer_ready': bool(installer) and download['status'] == 'ready',
        'package': PACKAGE,
        'release_page': RELEASE_PAGE,
    }


async def check(force=False, session_factory=None):
    async with _lock:
        fresh = _state['checked_at'] and time.monotonic() - _state['checked_at'] < CHECK_TTL
        if fresh and not force and not _state['check_error']:
            return public_status()
        try:
            release = await asyncio.wait_for(fetch_release(session_factory), 60)
            previous = _state['release']
            _state.update({'release': release, 'check_error': '', 'checked_at': time.monotonic(),
                           'checked_time': time.time()})
            if release and (not previous or previous.get('sha256') != release.get('sha256')):
                _reset_download()
        except (UpdateError, managed_ai.SetupError) as error:
            _state.update({'check_error': str(error), 'checked_at': time.monotonic(), 'checked_time': time.time()})
        except (aiohttp.ClientError, asyncio.TimeoutError, OSError):
            _state.update({'check_error': 'Could not reach GitHub to check for updates. Check your internet connection.',
                           'checked_at': time.monotonic(), 'checked_time': time.time()})
        return public_status()


def _reset_download():
    global _download_task
    if _download_task and not _download_task.done():
        _download_task.cancel()
    _download_task = None
    _state['download'] = {'status': 'idle', 'received': 0, 'total': 0, 'error': ''}
    _state['installer'] = None


def _remove_stale(folder, target, release):
    """Clear old installers; keep the target only when it already verifies."""
    for stale in folder.iterdir():
        if stale.name.startswith('.'):
            continue
        if stale == target and not stale.is_symlink() and stale.is_file() and stale.stat().st_size == release['bytes']:
            if managed_ai.sha256_file(stale) == release['sha256']:
                continue
        if stale.is_dir() and not stale.is_symlink():
            shutil.rmtree(stale, ignore_errors=True)
        else:
            stale.unlink(missing_ok=True)


async def _download(release):
    folder = updates_dir()
    target = folder / release['asset_name']
    task = asyncio.current_task()
    try:
        folder.mkdir(parents=True, exist_ok=True)
        await asyncio.to_thread(_remove_stale, folder, target, release)

        def progress(done, total):
            _state['download'].update({'received': done, 'total': total})

        await managed_ai.download_verified(
            {'url': release['url'], 'bytes': release['bytes'], 'sha256': release['sha256']}, target, progress)
        _state['installer'] = {'path': str(target), 'sha256': release['sha256'], 'bytes': release['bytes'],
                               'version': release['version']}
        _state['download'].update({'status': 'ready', 'received': release['bytes'], 'total': release['bytes']})
    except asyncio.CancelledError:
        # A newer download may already own the shared state after a reset.
        if _download_task is task:
            _state['download'] = {'status': 'idle', 'received': 0, 'total': 0, 'error': ''}
        raise
    except managed_ai.SetupError as error:
        _state['download'] = {'status': 'failed', 'received': 0, 'total': 0, 'error': str(error)}
    except (aiohttp.ClientError, asyncio.TimeoutError, OSError) as error:
        _state['download'] = {'status': 'failed', 'received': 0, 'total': 0,
                              'error': f'The update download failed ({error.__class__.__name__}). Try again.'}
    except Exception:
        logger.exception('Update download failed unexpectedly')
        _state['download'] = {'status': 'failed', 'received': 0, 'total': 0,
                              'error': 'The update download failed unexpectedly. Try again.'}


async def start_download():
    global _download_task
    async with _lock:
        release = _state['release']
        if not is_newer(release):
            raise HTTPException(409, 'Check for updates first; there is nothing newer to download.')
        if _state['download']['status'] == 'downloading' or _state['download']['status'] == 'ready':
            return public_status()
        _state['download'] = {'status': 'downloading', 'received': 0, 'total': release['bytes'], 'error': ''}
        _state['installer'] = None
        _download_task = asyncio.create_task(_download(release))
        return public_status()


def verified_installer():
    """Desktop host only: the downloaded package and the checksum it must
    match. The host hashes the file itself before opening it."""
    installer = _state['installer']
    if not installer or _state['download']['status'] != 'ready':
        raise HTTPException(409, 'Download the update before installing it.')
    path = updates_dir() / installer['path'].rsplit('\\', 1)[-1].rsplit('/', 1)[-1]
    if not PACKAGES[PACKAGE]['name'].match(path.name) or path.is_symlink() or not path.is_file():
        raise HTTPException(409, 'The downloaded installer is missing. Download the update again.')
    if path.stat().st_size != installer['bytes']:
        _state['download'] = {'status': 'failed', 'received': 0, 'total': 0,
                              'error': 'The downloaded installer no longer matches its checksum. Download it again.'}
        _state['installer'] = None
        path.unlink(missing_ok=True)
        raise HTTPException(409, _state['download']['error'])
    return {'path': str(path), 'sha256': installer['sha256'], 'bytes': installer['bytes'],
            'version': installer['version'], 'package': PACKAGE}


@router.get('/api/local-remove/update')
async def read_update(request: Request, refresh: bool = False):
    """Current status; with refresh=true, a quiet check unless one is recent."""
    from local_remove import guard
    guard(request)
    return await check() if refresh else public_status()


@router.post('/api/local-remove/update/check')
async def check_update(request: Request):
    from local_remove import guard
    guard(request, write=True)
    return await check(force=True)


@router.post('/api/local-remove/update/download')
async def download_update(request: Request):
    from local_remove import guard
    guard(request, write=True)
    return await start_download()


@router.get('/api/local-remove/update/installer')
async def read_installer(request: Request):
    from local_remove import launcher_guard
    launcher_guard(request)
    return verified_installer()
