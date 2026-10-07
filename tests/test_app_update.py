"""Update checks against simulated GitHub responses; no network, no installer run."""
import asyncio
import hashlib
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'backend'))
from fastapi import HTTPException
import app_update
import managed_ai

INSTALLER = b'MZ fake installer bytes for the update test'
DIGEST = hashlib.sha256(INSTALLER).hexdigest()
ASSET_URL = 'https://github.com/zdbosoxfan/local-image/releases/download/v9.9.0/Local-Image-Setup-9.9.0.exe'
CDN_URL = 'https://objects.githubusercontent.com/github-production-release-asset/installer'


def release(version='9.9.0', **changes):
    name = f'Local-Image-Setup-{version}.exe'
    base = f'https://github.com/zdbosoxfan/local-image/releases/download/v{version}/'
    value = {'draft': False, 'prerelease': True, 'tag_name': f'v{version}', 'name': f'Local Image {version}',
             'body': 'Release notes', 'html_url': f'https://github.com/zdbosoxfan/local-image/releases/tag/v{version}',
             'published_at': '2026-10-10T00:00:00Z',
             'assets': [{'name': name, 'size': len(INSTALLER), 'browser_download_url': base + name},
                        {'name': name + '.sha256', 'size': 95, 'browser_download_url': base + name + '.sha256'}]}
    value.update(changes)
    return value


class FakeResponse:
    def __init__(self, status, body=b'', headers=None):
        self.status, self.headers = status, headers or {}
        self._body = body

    class _Content:
        def __init__(self, body):
            self.body = body

        async def iter_chunked(self, size):
            for index in range(0, len(self.body), size):
                yield self.body[index:index + size]

    @property
    def content(self):
        return self._Content(self._body)

    async def __aenter__(self):
        return self

    async def __aexit__(self, *_):
        return False


class FakeSession:
    """Only the aiohttp surface the update code uses; records every URL."""
    def __init__(self, routes):
        self.routes, self.requests = routes, []

    def get(self, url, allow_redirects=False):
        self.requests.append(url)
        handler = self.routes.get(url)
        if handler is None:
            return FakeResponse(404)
        return handler() if callable(handler) else handler

    async def __aenter__(self):
        return self

    async def __aexit__(self, *_):
        return False


def routes(releases, installer=INSTALLER):
    """GitHub API list, each release's checksum file, and a CDN redirect for its package."""
    table = {app_update.RELEASES_URL: lambda: FakeResponse(200, json.dumps(releases).encode()),
             CDN_URL: lambda: FakeResponse(200, installer)}
    for item in releases:
        assets = item.get('assets', [])
        packages = [asset['name'] for asset in assets if asset.get('name', '').endswith(('.exe', '.deb', '.tar.gz'))]
        for asset in assets:
            name, url = asset.get('name', ''), asset.get('browser_download_url', '')
            if name.endswith('.exe.sha256'):
                table[url] = (lambda checksum: lambda: FakeResponse(200, checksum.encode()))(f'{DIGEST}  {name[:-7]}\n')
            elif name == 'SHA256SUMS':
                table[url] = (lambda checksum: lambda: FakeResponse(200, checksum.encode()))(
                    ''.join(f'{DIGEST}  {package}\n' for package in packages))
            elif name in packages:
                table[url] = lambda: FakeResponse(302, headers={'Location': CDN_URL})
    return table


def linux_release(version='9.9.5-linux-preview', **changes):
    base = f'https://github.com/zdbosoxfan/local-image/releases/download/v{version}/'
    names = [f'Local-Image-{version}-linux-x86_64.tar.gz', f'Local-Image-{version}-linux-x86_64.deb']
    value = {'draft': False, 'prerelease': True, 'tag_name': f'v{version}', 'name': f'Local Image {version}',
             'body': 'Linux notes', 'html_url': f'https://github.com/zdbosoxfan/local-image/releases/tag/v{version}',
             'published_at': '2026-10-12T00:00:00Z',
             'assets': [{'name': name, 'size': len(INSTALLER), 'browser_download_url': base + name} for name in names]
             + [{'name': 'SHA256SUMS', 'size': 200, 'browser_download_url': base + 'SHA256SUMS'}]}
    value.update(changes)
    return value


class ReleaseSelectionTests(unittest.TestCase):
    def test_newest_windows_installer_with_checksum_wins_and_others_are_ignored(self):
        linux = {'draft': False, 'tag_name': 'v9.9.5-linux-preview', 'assets': [
            {'name': 'Local-Image-9.9.5-linux-preview-linux-x86_64.deb', 'size': 5, 'browser_download_url': 'https://github.com/x'},
            {'name': 'SHA256SUMS', 'size': 5, 'browser_download_url': 'https://github.com/y'}]}
        draft = release('9.9.9', draft=True)
        unchecked = release('9.9.8')
        unchecked['assets'] = unchecked['assets'][:1]
        foreign = release('9.9.7')
        foreign['assets'][0]['browser_download_url'] = 'https://evil.example/Local-Image-Setup-9.9.7.exe'
        chosen = app_update.newest_release([release('9.9.0'), linux, draft, unchecked, foreign, release('9.9.1')], 'windows')
        self.assertEqual(chosen['version'], '9.9.1')
        self.assertEqual(chosen['asset_name'], 'Local-Image-Setup-9.9.1.exe')
        self.assertTrue(chosen['prerelease'])
        self.assertIsNone(app_update.newest_release([linux, draft], 'windows'))
        self.assertIsNone(app_update.newest_release({'message': 'rate limited'}, 'windows'))

    def test_linux_picks_its_own_package_and_ignores_windows_installers(self):
        releases = [release('9.9.9'), linux_release('9.9.5-linux-preview'), linux_release('9.9.4-linux-preview')]
        deb = app_update.newest_release(releases, 'linux-deb')
        self.assertEqual(deb['version'], '9.9.5-linux-preview')
        self.assertEqual(deb['asset_name'], 'Local-Image-9.9.5-linux-preview-linux-x86_64.deb')
        self.assertTrue(deb['checksum_url'].endswith('/SHA256SUMS'))
        archive = app_update.newest_release(releases, 'linux-tar')
        self.assertEqual(archive['asset_name'], 'Local-Image-9.9.5-linux-preview-linux-x86_64.tar.gz')
        no_sums = linux_release('9.9.6-linux-preview')
        no_sums['assets'] = no_sums['assets'][:2]
        self.assertEqual(app_update.newest_release([no_sums, linux_release('9.9.5-linux-preview')], 'linux-deb')['version'],
                         '9.9.5-linux-preview')
        self.assertIsNone(app_update.newest_release([release('9.9.9')], 'linux-deb'))

    def test_version_order_and_newer_check(self):
        self.assertGreater(app_update.version_tuple('0.7.10'), app_update.version_tuple('0.7.9'))
        self.assertEqual(app_update.version_tuple('0.7.2-linux-preview'), (0, 7, 2))
        with self.assertRaises(ValueError):
            app_update.version_tuple('linux-preview')
        with patch.object(app_update, 'APP_VERSION', '0.7.0'):
            self.assertTrue(app_update.is_newer({'version': '0.8.0'}))
            self.assertFalse(app_update.is_newer({'version': '0.7.0'}))
            self.assertFalse(app_update.is_newer(None))
        with patch.object(app_update, 'APP_VERSION', '0.7.2-linux-preview'):
            self.assertTrue(app_update.is_newer({'version': '0.7.3-linux-preview'}))
            self.assertFalse(app_update.is_newer({'version': '0.7.2-linux-preview'}))

    def test_checksum_file_must_name_the_installer(self):
        name = 'Local-Image-Setup-9.9.0.exe'
        self.assertEqual(app_update.parse_checksum(f'{DIGEST.upper()} *{name}\n', name), DIGEST)
        sums = f'{"0" * 64}  Local-Image-9.9.5-linux-preview-linux-x86_64.tar.gz\n{DIGEST}  Local-Image-9.9.5-linux-preview-linux-x86_64.deb\n'
        self.assertEqual(app_update.parse_checksum(sums, 'Local-Image-9.9.5-linux-preview-linux-x86_64.deb'), DIGEST)
        with self.assertRaises(app_update.UpdateError):
            app_update.parse_checksum(f'{DIGEST}  Local-Image-Setup-9.9.1.exe\n', name)
        with self.assertRaises(app_update.UpdateError):
            app_update.parse_checksum('not a checksum', name)

    def test_api_url_must_stay_on_github(self):
        app_update.checked_api_url('https://api.github.com/repos/x/y/releases')
        for url in ('http://api.github.com/repos', 'https://evil.example/', 'https://user@api.github.com/',
                    'https://api.github.com:8443/'):
            with self.subTest(url=url), self.assertRaises(app_update.UpdateError):
                app_update.checked_api_url(url)


class UpdateFlowTests(unittest.IsolatedAsyncioTestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='local-image-update-')
        self.environment = patch.dict(os.environ, {'LOCAL_IMAGE_DATA_DIR': str(Path(self.temporary.name) / 'profile')})
        self.environment.start()
        self.version = patch.object(app_update, 'APP_VERSION', '0.7.0')
        self.version.start()
        self.package = patch.object(app_update, 'PACKAGE', 'windows')
        self.package.start()
        app_update._state.update({'checked_at': 0.0, 'checked_time': None, 'release': None, 'check_error': '', 'installer': None,
                                  'download': {'status': 'idle', 'received': 0, 'total': 0, 'error': ''}})

    def tearDown(self):
        self.package.stop()
        self.version.stop()
        self.environment.stop()
        self.temporary.cleanup()

    async def test_check_reports_a_newer_release_without_exposing_urls(self):
        session = FakeSession(routes([release()]))
        status = await app_update.check(force=True, session_factory=lambda: session)
        self.assertTrue(status['available'])
        self.assertEqual(status['release']['version'], '9.9.0')
        self.assertEqual(status['current_version'], '0.7.0')
        self.assertEqual(status['check_error'], '')
        self.assertGreater(status['checked_at'], 1.7e9)
        self.assertFalse(status['installer_ready'])
        self.assertEqual(status['package'], 'windows')
        self.assertNotIn('url', status['release'])
        self.assertNotIn('sha256', status['release'])
        self.assertEqual(session.requests, [app_update.RELEASES_URL, ASSET_URL + '.sha256'])
        # A fresh result is reused without another request unless forced.
        await app_update.check(session_factory=lambda: session)
        self.assertEqual(len(session.requests), 2)

    async def test_current_version_is_not_offered_and_errors_are_reported(self):
        status = await app_update.check(force=True, session_factory=lambda: FakeSession(routes([release('0.7.0')])))
        self.assertFalse(status['available'])
        self.assertEqual(status['release']['version'], '0.7.0')
        limited = FakeSession({app_update.RELEASES_URL: lambda: FakeResponse(403, headers={'X-RateLimit-Remaining': '0'})})
        status = await app_update.check(force=True, session_factory=lambda: limited)
        self.assertIn('rate limiting', status['check_error'])
        offline = FakeSession({app_update.RELEASES_URL: lambda: (_ for _ in ()).throw(OSError('down'))})
        status = await app_update.check(force=True, session_factory=lambda: offline)
        self.assertIn('internet connection', status['check_error'])
        with self.assertRaises(HTTPException):
            await app_update.start_download()

    async def test_download_verifies_checksum_and_only_the_host_reads_the_path(self):
        session = FakeSession(routes([release()]))
        await app_update.check(force=True, session_factory=lambda: session)
        with patch.object(managed_ai.aiohttp, 'ClientSession', lambda *args, **kwargs: session):
            status = await app_update.start_download()
            self.assertEqual(status['download']['status'], 'downloading')
            await app_update._download_task
        status = app_update.public_status()
        self.assertEqual(status['download']['status'], 'ready')
        self.assertTrue(status['installer_ready'])
        self.assertNotIn('path', json.dumps(status))
        installer = app_update.verified_installer()
        self.assertEqual(installer['sha256'], DIGEST)
        self.assertEqual(Path(installer['path']).parent, app_update.updates_dir())
        self.assertEqual(Path(installer['path']).read_bytes(), INSTALLER)
        # A changed file is refused when the host asks for it, removed, and a
        # fresh download replaces it instead of failing on the leftover.
        Path(installer['path']).write_bytes(INSTALLER + b'!')
        with self.assertRaises(HTTPException):
            app_update.verified_installer()
        self.assertEqual(app_update.public_status()['download']['status'], 'failed')
        self.assertFalse(Path(installer['path']).exists())
        with patch.object(managed_ai.aiohttp, 'ClientSession', lambda *args, **kwargs: session):
            await app_update.start_download()
            await app_update._download_task
        self.assertEqual(app_update.public_status()['download']['status'], 'ready')
        self.assertEqual(Path(app_update.verified_installer()['path']).read_bytes(), INSTALLER)

    async def test_linux_download_verifies_against_the_shared_checksum_list(self):
        session = FakeSession(routes([release('9.9.9'), linux_release()]))
        with patch.object(app_update, 'PACKAGE', 'linux-deb'), patch.object(app_update, 'APP_VERSION', '0.7.2-linux-preview'):
            status = await app_update.check(force=True, session_factory=lambda: session)
            self.assertTrue(status['available'])
            self.assertEqual(status['release']['asset_name'], 'Local-Image-9.9.5-linux-preview-linux-x86_64.deb')
            self.assertEqual(status['package'], 'linux-deb')
            with patch.object(managed_ai.aiohttp, 'ClientSession', lambda *args, **kwargs: session):
                await app_update.start_download()
                await app_update._download_task
            self.assertEqual(app_update.public_status()['download']['status'], 'ready')
            package = app_update.verified_installer()
            self.assertEqual(package['package'], 'linux-deb')
            self.assertEqual(Path(package['path']).name, 'Local-Image-9.9.5-linux-preview-linux-x86_64.deb')
            self.assertEqual(Path(package['path']).read_bytes(), INSTALLER)

    async def test_corrupt_download_is_rejected_and_nothing_is_published(self):
        session = FakeSession(routes([release()], installer=INSTALLER[:-1] + b'?'))
        await app_update.check(force=True, session_factory=lambda: session)
        with patch.object(managed_ai.aiohttp, 'ClientSession', lambda *args, **kwargs: session):
            await app_update.start_download()
            await app_update._download_task
        status = app_update.public_status()
        self.assertEqual(status['download']['status'], 'failed')
        self.assertIn('checksum', status['download']['error'])
        self.assertFalse(status['installer_ready'])
        self.assertEqual([item.name for item in app_update.updates_dir().iterdir() if not item.name.startswith('.')], [])
        with self.assertRaises(HTTPException):
            app_update.verified_installer()


if __name__ == '__main__':
    unittest.main()
