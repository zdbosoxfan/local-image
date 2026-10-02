#!/usr/bin/env python3
"""Linux desktop host for the existing Local Image editor and native protocol."""
import fcntl
import hashlib
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import time
from urllib.error import HTTPError
from urllib.request import Request, build_opener, ProxyHandler
import uuid

from PySide6.QtCore import QFile, QIODevice, QObject, QRunnable, QThreadPool, QTimer, QUrl, Signal, Slot
from PySide6.QtGui import QDesktopServices, QIcon
from PySide6.QtWidgets import QApplication, QFileDialog, QMainWindow, QMessageBox
from PySide6.QtWebChannel import QWebChannel
from PySide6.QtWebEngineCore import QWebEnginePage, QWebEngineScript, QWebEngineProfile
from PySide6.QtWebEngineWidgets import QWebEngineView
from shiboken6 import delete

from protocol import BASE, CloseGate, batch_payload, identifier, project_payload, trusted_download, trusted_page

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'backend'))
from app_paths import APP_VERSION, cache_dir, data_root, log_dir, prepare_user_folders, state_dir


def create_desktop_profile(parent):
    # A named Qt profile persists local UI preferences across normal launches.
    # Its storage belongs to the same user profile as the backend documents.
    storage, cache = data_root() / 'webview', cache_dir() / 'webview'
    for directory in (storage, cache):
        directory.mkdir(mode=0o700, parents=True, exist_ok=True)
    profile = QWebEngineProfile('LocalImage', parent)
    profile.setPersistentStoragePath(str(storage))
    profile.setCachePath(str(cache))
    profile.setPersistentCookiesPolicy(QWebEngineProfile.PersistentCookiesPolicy.NoPersistentCookies)
    return profile


class Client:
    def __init__(self):
        self.key = ''
        self.http = build_opener(ProxyHandler({}))

    def call(self, path, payload=None, timeout=600):
        body = None if payload is None else json.dumps(payload).encode()
        request = Request(BASE + path, data=body, headers={
            'x-local-launcher': self.key, 'Content-Type': 'application/json'})
        try:
            with self.http.open(request, timeout=timeout) as response:
                return json.load(response)
        except HTTPError as error:
            try:
                detail = json.load(error).get('detail', 'The command failed.')
            except (ValueError, AttributeError):
                detail = 'The command failed.'
            raise ValueError(str(detail)) from error

    def register(self, paths):
        paths = [str(Path(path).absolute()) for path in paths]
        if not paths:
            return None
        if any(Path(path).suffix.lower() == '.lremove' for path in paths):
            if len(paths) != 1:
                raise ValueError('Open one project at a time.')
            return self.call('/api/local-remove/open-project', {'path': paths[0]})
        if any(Path(path).is_dir() for path in paths):
            if len(paths) != 1:
                raise ValueError('Open one folder at a time.')
            return self.call('/api/local-remove/register-folder', {'path': paths[0]})
        return self.call('/api/local-remove/open-files', {'paths': paths})


class Result(QObject):
    finished = Signal(object, object)

    def __init__(self, callback, parent):
        super().__init__(parent)
        self.callback = callback
        self.finished.connect(self.deliver)

    @Slot(object, object)
    def deliver(self, value, error):
        try:
            self.callback(value, error)
        finally:
            self.deleteLater()


class Task(QRunnable):
    def __init__(self, function, callback, parent):
        super().__init__()
        self.function = function
        self.result = Result(callback, parent)

    def run(self):
        try:
            self.result.finished.emit(self.function(), None)
        except Exception as error:
            self.result.finished.emit(None, str(error))


class Page(QWebEnginePage):
    def acceptNavigationRequest(self, url, navigation_type, is_main_frame):
        address = url.toString()
        if is_main_frame and (trusted_page(address) or trusted_download(address)):
            return True
        if is_main_frame and url.scheme() == 'https' and not url.userInfo():
            QDesktopServices.openUrl(url)
        return False

    def createWindow(self, window_type):
        # Remote popup links use the system browser and receive no native channel.
        page = QWebEnginePage(self.profile(), self)
        def external(url):
            if url.scheme() == 'https' and not url.userInfo():
                QDesktopServices.openUrl(url)
            page.deleteLater()
        page.urlChanged.connect(external)
        return page

    def javaScriptConsoleMessage(self, level, message, line, source):
        # Diagnostic only; never execute page text as a host command.
        print(f'Web [{level.name}] {source}:{line}: {message}', flush=True)


class Bridge(QObject):
    outbound = Signal(str)

    def __init__(self, window):
        super().__init__(window)
        self.window = window

    @Slot(str)
    def receive(self, raw):
        if len(raw) > 16384 or not trusted_page(self.window.view.url().toString()):
            return
        try:
            message = json.loads(raw)
            if (not isinstance(message, dict) or not isinstance(message.get('id'), str)
                    or not 1 <= len(message['id']) <= 128 or not isinstance(message.get('action'), str)):
                return
        except ValueError:
            return
        QTimer.singleShot(0, lambda: self.window.command(message))

    def send(self, message):
        if trusted_page(self.window.view.url().toString()):
            self.outbound.emit(json.dumps({'type': 'local-remove-native', **message}))


class Window(QMainWindow):
    def __init__(self, client, process, paths, smoke=False):
        super().__init__()
        self.client, self.process, self.paths, self.smoke = client, process, paths, smoke
        self.ready = self.busy = False
        self.close_gate = CloseGate()
        self.setWindowTitle('Local Image')
        self.resize(1440, 960)
        self.setWindowIcon(QIcon(str(ROOT / 'backend' / 'frontend' / 'app-icon.png')))
        self.view = QWebEngineView(self)
        self.view.setAcceptDrops(False)  # Page strings must never supply OS paths.
        self.profile = create_desktop_profile(self)
        self.page = Page(self.profile, self.view)
        self.view.setPage(self.page)
        self.page.loadFinished.connect(lambda ok: print('LOCAL_IMAGE_PAGE_LOADED', ok, self.view.url().toString(), flush=True))
        self.setCentralWidget(self.view)
        self.bridge = Bridge(self)
        self.channel = QWebChannel(self.page)
        self.channel.registerObject('localImageHost', self.bridge)
        self.page.setWebChannel(self.channel)
        resource = QFile(':/qtwebchannel/qwebchannel.js')
        if not resource.open(QIODevice.OpenModeFlag.ReadOnly):
            raise RuntimeError('Qt WebChannel resource is unavailable.')
        channel_js = bytes(resource.readAll()).decode()
        resource.close()
        shim = """
(() => {
  if (location.origin !== 'http://127.0.0.1:51247' || location.pathname !== '/remove') return;
  const listeners = new Set(), queue = []; let host = null;
  window.chrome = window.chrome || {};
  window.chrome.webview = {
    addEventListener(type, listener) { if (type === 'message') listeners.add(listener); },
    removeEventListener(type, listener) { if (type === 'message') listeners.delete(listener); },
    postMessage(message) { const value = JSON.stringify(message); if (host) host.receive(value); else queue.push(value); }
  };
  new QWebChannel(qt.webChannelTransport, channel => {
    host = channel.objects.localImageHost;
    host.outbound.connect(raw => { const event = {data: JSON.parse(raw)}; for (const listener of listeners) listener(event); });
    for (const value of queue.splice(0)) host.receive(value);
  });
})();
"""
        script = QWebEngineScript()
        script.setName('Local Image native bridge')
        script.setInjectionPoint(QWebEngineScript.InjectionPoint.DocumentCreation)
        script.setWorldId(QWebEngineScript.ScriptWorldId.MainWorld)
        script.setRunsOnSubFrames(False)
        script.setSourceCode(channel_js + '\n' + shim)
        self.page.scripts().insert(script)
        self.profile.downloadRequested.connect(self.download)
        self.heartbeat = QTimer(self)
        self.heartbeat.setInterval(15000)
        self.heartbeat.timeout.connect(lambda: self.work(lambda: client.call('/api/local-remove/heartbeat', {}, 3), lambda *_: None))
        self.heartbeat.start()
        self.client.call('/api/local-remove/heartbeat', {}, 3)
        self.open_initial()

    def work(self, function, callback):
        QThreadPool.globalInstance().start(Task(function, callback, self))

    def open_initial(self):
        def completed(result, error):
            if error:
                QMessageBox.warning(self, 'Local Image', error)
            query = ''
            for kind in ('collection', 'session'):
                if result and isinstance(result.get(kind), dict):
                    query = '?' + kind + '=' + identifier(result[kind]['id'])
                    break
            self.view.setUrl(QUrl(BASE + '/remove' + query))
        self.work(lambda: self.client.register(self.paths), completed)

    def reply(self, identity, result=None, error=None):
        self.bridge.send({'id': identity, 'result': result, 'error': error})

    def command(self, message):
        identity, action = message['id'], message['action']
        if not trusted_page(self.view.url().toString()):
            return
        if action == 'ready':
            self.ready = True
            self.reply(identity, {'native': True, 'version': 2, 'projects': True,
                                  'closeRequests': True, 'setup': True, 'batch': True})
            print('LOCAL_IMAGE_NATIVE_READY ' + APP_VERSION, flush=True)
            if self.smoke:
                QTimer.singleShot(4000, lambda: QApplication.instance().exit(0))
            return
        if action == 'closeReady':
            print('LOCAL_IMAGE_CLOSE_DECISION', message.get('approved'), flush=True)
            if self.close_gate.complete(identity, message.get('approved'), self.busy):
                self.close()
            return
        if self.busy or self.close_gate.pending:
            self.reply(identity, error='Finish the current desktop command first.')
            return
        self.busy = True
        try:
            function = self.prepare_command(action, message)
            if function is None:
                self.busy = False
                self.reply(identity)
                return
            def completed(result, error):
                self.busy = False
                self.reply(identity, result, error)
            self.work(function, completed)
        except Exception as error:
            self.busy = False
            self.reply(identity, error=str(error))

    def folder(self, title):
        return QFileDialog.getExistingDirectory(self, title)

    def prepare_command(self, action, message):
        call = self.client.call
        if action in ('openFiles', 'openProject', 'openFolder'):
            if action == 'openFolder':
                folder = self.folder('Open a folder of images')
                paths = [folder] if folder else []
            else:
                filters = 'Local Image projects (*.lremove)' if action == 'openProject' else 'Images and projects (*.jpg *.jpeg *.png *.tif *.tiff *.webp *.lremove)'
                paths, _ = QFileDialog.getOpenFileNames(self, 'Open images or project', '', filters)
            return (lambda: self.client.register(paths)) if paths else None
        if action == 'drop':
            raise ValueError('Use File → Open to choose local files in this Linux window.')
        if action == 'saveProject':
            payload = project_payload(message)
            session = call('/api/local-remove/session/' + payload['session_id'])
            if message.get('saveAs') is True or not session.get('has_project_path'):
                suggested = Path(session.get('project_name') or session.get('name') or 'Untitled').name
                suggested = str(Path(suggested).with_suffix('.lremove'))
                destination, _ = QFileDialog.getSaveFileName(self, 'Save editable project', suggested, 'Local Image project (*.lremove)')
                if not destination:
                    return None
                if Path(destination).suffix.lower() != '.lremove':
                    destination += '.lremove'
                    if Path(destination).exists() and QMessageBox.question(self, 'Replace project?', 'This project already exists. Replace it?') != QMessageBox.StandardButton.Yes:
                        return None
                payload['path'] = str(Path(destination).absolute())
                def save():
                    if Path(destination).exists():
                        with open(destination, 'rb') as stream:
                            payload['expected_hash'] = hashlib.file_digest(stream, 'sha256').hexdigest()
                    return call('/api/local-remove/save-project', payload)
                return save
            return lambda: call('/api/local-remove/save-project', payload)
        if action == 'batchExportFolder':
            job, items = batch_payload(message)
            folder = self.folder('Export reviewed batch copies')
            return (lambda: call('/api/local-remove/batch/jobs/' + job + '/export-folder', {'path': folder, 'item_ids': items})) if folder else None
        if action == 'chooseBackgroundFolder':
            folder = self.folder('Choose background images')
            return (lambda: call('/api/local-remove/backgrounds/register-folder', {'path': folder})) if folder else None
        folder_keys = {'setupChooseComfyDirectory': 'comfy_directory', 'setupChooseModelDirectory': 'model_directory',
                       'setupChooseInstallDirectory': 'managed_ai_directory', 'configureAi': 'comfy_directory'}
        if action in folder_keys:
            folder = self.folder('Choose ComfyUI installation' if folder_keys[action] == 'comfy_directory' else 'Choose model folder')
            return (lambda: call('/api/local-remove/setup/configure', {folder_keys[action]: folder})) if folder else None
        if action == 'setupUseInstallation':
            item = message.get('installation_id')
            if not isinstance(item, str) or not item or len(item) > 128:
                raise ValueError('Choose a detected ComfyUI installation.')
            return lambda: call('/api/local-remove/setup/configure', {'installation_id': item})
        if action == 'setupInstall':
            raise ValueError('Choose an existing Linux ComfyUI installation in Settings.')
        endpoints = {'setupStart': '/setup/start', 'setupEject': '/setup/eject', 'setupDownloadModels': '/setup/download-models'}
        if action in endpoints:
            return lambda: call('/api/local-remove' + endpoints[action], {})
        if action in ('setupDownloadQwen', 'setupDownloadGenerationModel'):
            model, variant = message.get('model', 'qwen'), message.get('variant', 'int8')
            allowed = {'qwen': ('int8', 'bf16'), 'z-image-turbo': ('bf16',), 'flux2-klein-4b': ('bf16',),
                       'ernie-image': ('bf16',), 'flux2-dev': ('fp8',), 'flux2-klein-9b': ('fp8',), 'seedvr2': ('fp16',)}
            if model not in allowed or variant not in allowed[model]:
                raise ValueError('Choose a supported model preset.')
            suffix = '/qwen/download' if action == 'setupDownloadQwen' else '/generator/download'
            return lambda: call('/api/local-remove' + suffix, {'model': model, 'variant': variant})
        if action == 'loraDownload':
            keys = ('model', 'repo_id', 'filename', 'revision')
            if any(not isinstance(message.get(key), str) or not 0 < len(message[key]) <= 512 for key in keys):
                raise ValueError('Choose a supported LoRA file.')
            payload = {key: message[key] for key in keys}
            payload['allow_unverified'] = message.get('allow_unverified') is True
            return lambda: call('/api/local-remove/loras/download', payload)
        raise ValueError('Unknown desktop action.')

    def download(self, request):
        if not trusted_page(self.view.url().toString()) or not trusted_download(request.url().toString()):
            request.cancel()
            return
        destination, _ = QFileDialog.getSaveFileName(self, 'Export image or project', Path(request.suggestedFileName()).name)
        if not destination:
            request.cancel()
            return
        path = Path(destination).absolute()
        request.setDownloadDirectory(str(path.parent))
        request.setDownloadFileName(path.name)
        request.accept()

    def closeEvent(self, event):
        print('LOCAL_IMAGE_CLOSE_REQUEST', self.ready, self.busy, self.close_gate.approved, flush=True)
        if self.busy:
            event.ignore()
            return
        if self.ready and not self.close_gate.approved:
            event.ignore()
            identity = self.close_gate.request()
            self.bridge.send({'action': 'requestClose', 'id': identity})
            return
        self.heartbeat.stop()
        event.accept()

    def dispose(self):
        # Qt requires disk profiles to be destroyed before QApplication exits.
        # Delete their pages first so profile storage can finish flushing safely.
        self.heartbeat.stop()
        delete(self.view)
        delete(self.profile)


def main():
    smoke = '--smoke-test' in sys.argv[1:]
    test_profile = tempfile.TemporaryDirectory(prefix='local-image-native-smoke-') if smoke else None
    if smoke:
        os.environ['LOCAL_IMAGE_DATA_DIR'] = test_profile.name
        if len(sys.argv) != 2:
            raise ValueError('The desktop smoke test accepts no document paths.')
    app = QApplication(sys.argv)
    app.setApplicationName('Local Image')
    app.setDesktopFileName('local-image')
    prepare_user_folders()
    lock = (state_dir() / 'desktop.lock').open('a')
    try:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError:
        QMessageBox.information(None, 'Local Image', 'Local Image is already open.')
        return 0
    try:
        with socket.create_connection(('127.0.0.1', 51247), timeout=.2):
            QMessageBox.warning(None, 'Local Image', 'The editor port is already in use. Close the other Local Image process and try again.')
            return 1
    except OSError:
        pass
    client = Client()
    output = (log_dir() / 'desktop-backend.log').open('a')
    process = subprocess.Popen([sys.executable, str(ROOT / 'backend' / 'run_local_remove.py')],
                               cwd=ROOT / 'backend', stdout=output, stderr=subprocess.STDOUT)
    window = None
    try:
        deadline = time.monotonic() + 45
        while time.monotonic() < deadline:
            if process.poll() is not None:
                raise RuntimeError('The backend could not start. See Local Image logs.')
            try:
                client.call('/health', timeout=.5)
                client.key = (state_dir() / 'launcher.key').read_text().strip()
                break
            except (OSError, ValueError):
                app.processEvents()
                time.sleep(.1)
        else:
            raise RuntimeError('The backend did not start within 45 seconds.')
        smoke = '--smoke-test' in sys.argv[1:]
        paths = [value for value in sys.argv[1:] if value != '--smoke-test']
        window = Window(client, process, paths, smoke)
        window.show()
        if smoke:
            QTimer.singleShot(30000, lambda: app.exit(2))
        code = app.exec()
        return code if not smoke or window.ready else 2
    except Exception as error:
        QMessageBox.critical(None, 'Local Image', str(error))
        return 1
    finally:
        if window is not None:
            window.dispose()
            delete(window)
        # Only the backend spawned by this process is stopped; never a pre-existing service.
        if process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
        output.close()
        lock.close()


if __name__ == '__main__':
    raise SystemExit(main())
