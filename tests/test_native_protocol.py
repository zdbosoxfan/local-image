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


if __name__ == '__main__':
    unittest.main()
