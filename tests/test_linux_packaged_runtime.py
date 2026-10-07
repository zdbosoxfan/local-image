"""Exercise production environment boundaries without optional Qt GUI imports."""
import ast
from contextlib import contextmanager
import os
from pathlib import Path
from types import SimpleNamespace
import unittest
from unittest.mock import patch


class LinuxPackagedRuntimeTests(unittest.TestCase):
    def setUp(self):
        source = Path(__file__).resolve().parents[1] / 'desktop' / 'linux' / 'local_image.py'
        tree = ast.parse(source.read_text())
        functions = [node for node in tree.body if isinstance(node, ast.FunctionDef)
                     and node.name in ('backend_launch', 'external_desktop_environment')]
        self.namespace = {'os': os, 'sys': SimpleNamespace(executable='/usr/bin/python3'),
                          'Path': Path, 'ROOT': Path('/opt/local-image'), 'FROZEN': True,
                          'contextmanager': contextmanager}
        exec(compile(ast.Module(body=functions, type_ignores=[]), str(source), 'exec'), self.namespace)

    def test_frozen_backend_starts_its_own_executable_and_restores_user_libraries(self):
        original = {'LD_LIBRARY_PATH': '/opt/local-image/_internal:/custom/lib',
                    'LD_LIBRARY_PATH_ORIG': '/custom/lib',
                    'QT_PLUGIN_PATH': '/opt/local-image/_internal/plugins', 'OTHER': 'retained'}
        with patch.dict(os.environ, original, clear=True):
            command, environment = self.namespace['backend_launch']()
            self.assertEqual(command, ['/opt/local-image/backend/LocalImageBackend'])
            self.assertEqual(environment['LD_LIBRARY_PATH'], '/custom/lib')
            self.assertEqual(environment['PYINSTALLER_RESET_ENVIRONMENT'], '1')
            self.assertNotIn('LD_LIBRARY_PATH_ORIG', environment)
            self.assertNotIn('QT_PLUGIN_PATH', environment)
            self.assertEqual(environment['OTHER'], 'retained')
            self.assertEqual(dict(os.environ), original)

    def test_backend_does_not_inherit_bundle_path_when_user_had_none(self):
        with patch.dict(os.environ, {'LD_LIBRARY_PATH': '/opt/local-image/_internal'}, clear=True):
            _, environment = self.namespace['backend_launch']()
            self.assertNotIn('LD_LIBRARY_PATH', environment)

    def test_source_run_keeps_python_and_environment(self):
        self.namespace['FROZEN'] = False
        with patch.dict(os.environ, {'LD_LIBRARY_PATH': '/user/library'}, clear=True):
            command, environment = self.namespace['backend_launch']()
            self.assertEqual(command, ['/usr/bin/python3', '/opt/local-image/backend/run_local_image.py'])
            self.assertEqual(environment, dict(os.environ))

    def test_system_browser_environment_restores_after_failure(self):
        original = {'LD_LIBRARY_PATH': '/opt/local-image/_internal',
                    'LD_LIBRARY_PATH_ORIG': '/user/library', 'QT_PLUGIN_PATH': '/bundle/plugins',
                    'QT_QPA_PLATFORM_PLUGIN_PATH': '/bundle/platforms', 'QML2_IMPORT_PATH': '/bundle/qml'}
        with patch.dict(os.environ, original, clear=True):
            with self.assertRaises(RuntimeError), self.namespace['external_desktop_environment']():
                self.assertEqual(os.environ['LD_LIBRARY_PATH'], '/user/library')
                self.assertNotIn('QT_PLUGIN_PATH', os.environ)
                self.assertNotIn('QT_QPA_PLATFORM_PLUGIN_PATH', os.environ)
                self.assertNotIn('QML2_IMPORT_PATH', os.environ)
                raise RuntimeError('synthetic handler failure')
            self.assertEqual(dict(os.environ), original)

    def test_system_browser_environment_uses_system_default_without_user_path(self):
        with patch.dict(os.environ, {'LD_LIBRARY_PATH': '/bundle'}, clear=True):
            with self.namespace['external_desktop_environment']():
                self.assertNotIn('LD_LIBRARY_PATH', os.environ)
            self.assertEqual(os.environ['LD_LIBRARY_PATH'], '/bundle')


if __name__ == '__main__':
    unittest.main()


class PruneUnusedQtTests(unittest.TestCase):
    def test_wayland_compositor_is_removed_and_client_platform_plugin_kept(self):
        import importlib.util
        import tempfile
        spec = importlib.util.spec_from_file_location('build_linux', Path(__file__).resolve().parents[1] / 'packaging' / 'build_linux.py')
        build = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(build)
        with tempfile.TemporaryDirectory() as root:
            package = Path(root)
            keep = [package / '_internal/PySide6/Qt/lib/libQt6WaylandClient.so.6',
                    package / '_internal/PySide6/Qt/plugins/platforms/libqwayland-generic.so',
                    package / '_internal/PySide6/Qt/qml/QtWayland/Client/libqwaylandclientplugin.so']
            drop = [package / '_internal/PySide6/Qt/lib/libQt6WaylandCompositor.so.6',
                    package / '_internal/PySide6/Qt/lib/libQt6WaylandCompositor.so.6.11.2',
                    package / '_internal/PySide6/Qt/qml/QtWayland/Compositor/QtShell/libwaylandcompositorqtshellplugin.so',
                    package / '_internal/PySide6/QtWaylandCompositor.abi3.so']
            for path in keep + drop:
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(b'x')
            removed = build.prune_unused_qt(package)
            self.assertEqual(len(removed), 4)
            self.assertTrue(all(path.exists() for path in keep))
            self.assertFalse(any(path.exists() for path in drop))
            self.assertFalse((package / '_internal/PySide6/Qt/qml/QtWayland/Compositor').exists())
            self.assertEqual(build.prune_unused_qt(package), [])
