"""Native bridge boundaries, independent of a GUI or Qt installation."""
import importlib.util
import ast
import json
from pathlib import Path
from types import SimpleNamespace
import unittest
import uuid
from unittest.mock import Mock, patch

_spec = importlib.util.spec_from_file_location(
    'local_image_linux_protocol', Path(__file__).resolve().parents[1] / 'desktop' / 'linux' / 'protocol.py')
protocol = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(protocol)


class NativeBatchProtocolTests(unittest.TestCase):
    def setUp(self):
        self.job = 'e2419d61-d78b-4f8a-b138-8e65386aa3e9'
        self.items = [str(uuid.uuid4()) for _ in range(4096)]

    def test_large_reviewed_selection_passes_transport_and_payload_without_truncation(self):
        raw = json.dumps({'id': 'export-request', 'action': 'batchExportFolder',
                          'job_id': self.job, 'item_ids': self.items,
                          'path': '/untrusted/destination', 'command': 'untrusted'})
        self.assertGreater(len(raw), 16384)
        decoded = protocol.decode_message(raw)
        self.assertIsNotNone(decoded)
        self.assertEqual(protocol.batch_payload(decoded), (self.job, self.items))

    def test_empty_or_non_list_reviewed_selection_is_rejected(self):
        for items in ([], None, self.items[0], tuple(self.items[:1])):
            with self.subTest(items=items), self.assertRaises(ValueError):
                protocol.batch_payload({'job_id': self.job, 'item_ids': items})

    def test_invalid_job_and_item_identifiers_are_rejected(self):
        for invalid in (None, 7, '../outside-batch', self.job.upper(), uuid.UUID(self.job).hex):
            with self.subTest(invalid=invalid):
                with self.assertRaises((ValueError, AttributeError)):
                    protocol.batch_payload({'job_id': invalid, 'item_ids': self.items[:1]})
                with self.assertRaises((ValueError, AttributeError)):
                    protocol.batch_payload({'job_id': self.job, 'item_ids': [self.items[0], invalid]})

    def test_duplicate_in_large_reviewed_selection_is_rejected(self):
        with self.assertRaisesRegex(ValueError, 'distinct'):
            protocol.batch_payload({'job_id': self.job, 'item_ids': [*self.items, self.items[0]]})

    def test_message_budget_counts_utf8_bytes_and_accepts_exact_boundary(self):
        with patch.object(protocol, 'MAX_NATIVE_MESSAGE_BYTES', 64):
            self.assertTrue(protocol.message_within_limit('x' * 64))
            self.assertFalse(protocol.message_within_limit('x' * 65))
            self.assertTrue(protocol.message_within_limit('\u6c34' * 21 + 'x'))
            self.assertFalse(protocol.message_within_limit('\u6c34' * 22))
            raw = json.dumps({'id': '1', 'action': 'ready'})
            self.assertIsNotNone(protocol.decode_message(raw))
            self.assertIsNone(protocol.decode_message(raw + ' ' * 64))
        self.assertEqual(protocol.MAX_NATIVE_MESSAGE_BYTES, 16 * 1024 * 1024)

    def test_invalid_message_envelopes_do_not_dispatch(self):
        for value in (None, 7, '', '{', '[]', '{}', json.dumps({'id': '', 'action': 'ready'}),
                      json.dumps({'id': 'x' * 129, 'action': 'ready'}),
                      json.dumps({'id': '1', 'action': None}), '\ud800', '[' * 2000):
            with self.subTest(value=repr(value)[:80]):
                self.assertIsNone(protocol.decode_message(value))

    def test_trusted_page_remains_local_and_exact(self):
        self.assertTrue(protocol.trusted_page(protocol.BASE + '/remove?collection=anything'))
        for address in ('https://127.0.0.1:51247/remove', 'http://localhost:51247/remove',
                        'http://127.0.0.1:51248/remove', protocol.BASE + '/remove-other',
                        'http://user@127.0.0.1:51247/remove', 'https://example.com/remove'):
            with self.subTest(address=address):
                self.assertFalse(protocol.trusted_page(address))

    def test_native_drop_accepts_only_local_file_manager_urls(self):
        self.assertEqual(protocol.local_drop_paths(['file:///tmp/Photo%20folder', 'file:///tmp/Photo%20folder']), ['/tmp/Photo folder'])
        self.assertEqual(protocol.local_drop_paths(['file:///tmp/%E6%B0%B4%E5%BD%A9.png']), ['/tmp/水彩.png'])
        for url in ('https://example.com/image.png', 'file://remote/tmp/image.png', 'file:///tmp/image.png?path=other', 'file:///tmp/a%00b'):
            self.assertEqual(protocol.local_drop_paths([url]), [])

    def test_image_export_uses_only_revision_checked_image_options(self):
        values = dict(session_id=self.job, revision=7, format='png', filename='Product.png', width=400, height=300)
        self.assertEqual(protocol.image_export_payload(dict(values, path='/untrusted/destination', command='untrusted')), values)
        for change in ({'revision': -1}, {'format': 'original'}, {'filename': ''}, {'width': True}, {'height': 0}, {'width': 32769}):
            with self.subTest(change=change), self.assertRaises(ValueError):
                protocol.image_export_payload(dict(values, **change))

    def test_close_approval_remains_correlated_and_cannot_be_replayed(self):
        gate = protocol.CloseGate()
        first = gate.request()
        self.assertEqual(gate.request(), first)
        self.assertFalse(gate.complete('unrelated', True))
        self.assertEqual(gate.pending, first)
        self.assertFalse(gate.complete(first, True, busy=True))
        self.assertFalse(gate.approved)
        second = gate.request()
        self.assertFalse(gate.complete(first, True))
        self.assertTrue(gate.complete(second, True))
        self.assertFalse(gate.complete(second, True))


class NativeSetupCommandTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        # Execute the production command handler without importing optional Qt
        # GUI bindings. These actions only need a native client's HTTP method.
        path = Path(__file__).resolve().parents[1] / 'desktop' / 'linux' / 'local_image.py'
        source = ast.parse(path.read_text(encoding='utf-8'), filename=str(path))
        window = next(node for node in source.body if isinstance(node, ast.ClassDef) and node.name == 'Window')
        command = next(node for node in window.body if isinstance(node, ast.FunctionDef) and node.name == 'prepare_command')
        namespace = {}
        exec(compile(ast.Module(body=[command], type_ignores=[]), str(path), 'exec'), namespace)
        cls.command = staticmethod(namespace['prepare_command'])

    def setUp(self):
        self.client = SimpleNamespace(call=Mock(return_value={'ok': True}))
        self.window = SimpleNamespace(client=self.client)

    def test_qwen_download_sends_only_variant_to_strict_qwen_endpoint(self):
        for variant in ('int8', 'bf16'):
            with self.subTest(variant=variant):
                self.client.call.reset_mock()
                operation = self.command(self.window, 'setupDownloadQwen',
                                         {'model': 'ignored-page-model', 'variant': variant, 'path': '/untrusted'})
                self.assertEqual(operation(), {'ok': True})
                self.client.call.assert_called_once_with('/api/local-remove/qwen/download', {'variant': variant})

    def test_drop_consumes_host_owned_paths_once_and_ignores_page_paths(self):
        self.window.pending_drop = {'id': 'trusted-drop', 'paths': ['/tmp/Photo folder']}
        self.client.register = Mock(return_value={'collection': {'id': 'test'}})
        operation = self.command(self.window, 'acceptDrop', {'drop_id': 'trusted-drop', 'accept': True, 'paths': ['/untrusted']})
        self.assertEqual(operation(), {'collection': {'id': 'test'}})
        self.client.register.assert_called_once_with(['/tmp/Photo folder'])
        with self.assertRaises(ValueError):
            self.command(self.window, 'acceptDrop', {'drop_id': 'trusted-drop', 'accept': True})

    def test_cancelled_drop_does_not_register_any_paths(self):
        self.window.pending_drop = {'id': 'trusted-drop', 'paths': ['/tmp/Photo folder']}
        self.assertIsNone(self.command(self.window, 'acceptDrop', {'drop_id': 'trusted-drop', 'accept': False}))
        self.assertIsNone(self.window.pending_drop); self.client.call.assert_not_called()

    def test_batch_folder_selection_is_native_owned_and_cancel_preserves_destination(self):
        self.window.folder = Mock(return_value='/tmp/Reviewed photos')
        job = str(uuid.uuid4()); item = str(uuid.uuid4())
        namespace = self.command.__globals__
        with patch.dict(namespace, {'Path': Path, 'batch_payload': protocol.batch_payload}):
            operation = self.command(self.window, 'batchChooseExportFolder', {'path': '/untrusted'})
            self.assertEqual(operation(), {'directory': '/tmp/Reviewed photos'})
            self.window.folder.return_value = None
            self.assertIsNone(self.command(self.window, 'batchChooseExportFolder', {}))
            self.assertEqual(self.window.batch_export_directory, '/tmp/Reviewed photos')
            self.command(self.window, 'batchExportFolder', {'job_id': job, 'item_ids': [item], 'use_selected_folder': True,
                                                           'path': '/untrusted', 'naming_template': 'product-{index}'})()
            self.client.call.assert_called_once_with('/api/local-remove/batch/jobs/' + job + '/export-folder',
                                                     {'path': '/tmp/Reviewed photos', 'item_ids': [item], 'naming_template': 'product-{index}'})
            self.window.batch_export_directory = None
            with self.assertRaisesRegex(ValueError, 'Choose a batch output folder'):
                self.command(self.window, 'batchExportFolder', {'job_id': job, 'item_ids': [item], 'use_selected_folder': True})

    def test_single_export_uses_the_native_selected_folder_and_cancel_keeps_it(self):
        self.window.folder = Mock(return_value='/tmp/Export images')
        values = dict(session_id=str(uuid.uuid4()), revision=4, format='png', filename='Product.png', width=800, height=600)
        with patch.dict(self.command.__globals__, {'Path': Path, 'image_export_payload': protocol.image_export_payload}):
            self.assertEqual(self.command(self.window, 'imageChooseExportFolder', {'path': '/untrusted'})(), {'directory': '/tmp/Export images'})
            self.window.folder.return_value = None
            self.assertIsNone(self.command(self.window, 'imageChooseExportFolder', {}))
            self.command(self.window, 'imageExportFolder', dict(values, path='/untrusted'))()
            self.client.call.assert_called_once_with('/api/local-remove/session/' + values['session_id'] + '/export-folder', dict({key: value for key, value in values.items() if key != 'session_id'}, path='/tmp/Export images'))
            self.window.image_export_directory = None
            with self.assertRaisesRegex(ValueError, 'Choose an image output folder'):
                self.command(self.window, 'imageExportFolder', values)

    def test_qwen_download_default_matches_windows_and_invalid_variants_do_not_call_backend(self):
        operation = self.command(self.window, 'setupDownloadQwen', {})
        operation()
        self.client.call.assert_called_once_with('/api/local-remove/qwen/download', {'variant': 'int8'})
        self.client.call.reset_mock()
        for variant in ('fp8', None, 7):
            with self.subTest(variant=variant), self.assertRaises(ValueError):
                self.command(self.window, 'setupDownloadQwen', {'variant': variant})
        self.client.call.assert_not_called()

    def test_generation_download_preserves_model_and_variant_payload(self):
        for model, variant in (('qwen', 'bf16'), ('z-image-turbo', 'bf16'), ('flux2-dev', 'fp8'), ('seedvr2', 'fp16')):
            with self.subTest(model=model, variant=variant):
                self.client.call.reset_mock()
                self.command(self.window, 'setupDownloadGenerationModel', {'model': model, 'variant': variant})()
                self.client.call.assert_called_once_with('/api/local-remove/generator/download', {'model': model, 'variant': variant})

    def test_generation_download_rejects_unsupported_model_presets(self):
        for model, variant in (('unknown', 'bf16'), ('qwen', 'fp8'), ('flux2-dev', 'bf16')):
            with self.subTest(model=model, variant=variant), self.assertRaises(ValueError):
                self.command(self.window, 'setupDownloadGenerationModel', {'model': model, 'variant': variant})
        self.client.call.assert_not_called()

    def test_update_install_verifies_the_backend_package_then_closes_the_window(self):
        import hashlib
        import tempfile
        with tempfile.TemporaryDirectory() as root:
            updates = Path(root) / 'updates'
            updates.mkdir()
            package = updates / 'Local-Image-9.9.5-linux-preview-linux-x86_64.deb'
            package.write_bytes(b'debian package bytes')
            info = {'path': str(package), 'sha256': hashlib.sha256(b'debian package bytes').hexdigest(),
                    'bytes': len(b'debian package bytes'), 'version': '9.9.5-linux-preview', 'package': 'linux-deb'}
            self.client.call.return_value = info
            self.window.pending_installer = None
            self.window.close = Mock()
            namespace = self.command.__globals__
            with patch.dict(namespace, {'verified_update_package': protocol.verified_update_package,
                                        'data_root': lambda: Path(root), 'QTimer': SimpleNamespace(singleShot=lambda _, fn: fn())}):
                function = self.command(self.window, 'updateInstall', {'path': '/untrusted', 'command': 'untrusted'})
                self.assertEqual(function(), {'ok': True})
            self.client.call.assert_called_once_with('/api/local-remove/update/installer')
            self.assertEqual(self.window.pending_installer, package.resolve())
            self.window.close.assert_called_once_with()
            # A package outside the updates folder, a renamed file or changed bytes never opens.
            outside = Path(root) / package.name
            outside.write_bytes(b'debian package bytes')
            for bad in ({**info, 'path': str(outside)}, {**info, 'bytes': 3}, {**info, 'sha256': '0' * 64},
                        {**info, 'path': str(updates / 'evil.sh')}, 'not a dict', {**info, 'sha256': 'short'}):
                with self.subTest(bad=bad), self.assertRaises(ValueError):
                    protocol.verified_update_package(bad, updates)
            link = updates / 'Local-Image-9.9.6-linux-preview-linux-x86_64.deb'
            link.symlink_to(package)
            with self.assertRaises(ValueError):
                protocol.verified_update_package({**info, 'path': str(link)}, updates)


if __name__ == '__main__':
    unittest.main()
