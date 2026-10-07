"""Real durable storage and proportional single-item updates for large queues."""
import json
from contextlib import closing
from pathlib import Path
import sqlite3
import sys
import tempfile
import unittest
import uuid

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'backend'))
import batch_store


class BatchStorageTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.directory = Path(self.temporary.name)
        self.linked = lambda path: path.is_symlink()

    def tearDown(self):
        self.temporary.cleanup()

    def queue(self, count):
        return {'id': str(uuid.uuid4()), 'modified': 1, 'phase': 'preparing', 'running': True,
                'message': 'Preparing', 'cancel_requested': False,
                'items': [{'id': str(uuid.uuid4()), 'name': 'Product-' + str(index) + '-' + 'a' * 240,
                           'status': 'pending', 'revision': 0} for index in range(count)]}

    def test_queue_larger_than_legacy_metadata_limit_reopens_in_order(self):
        value = self.queue(5001)
        self.assertGreater(len(json.dumps(value)), 1024**2)
        batch_store.save(self.directory, value, self.linked)
        self.assertEqual(batch_store.load(self.directory, self.linked), value)

    def test_single_item_save_only_changes_one_image_record(self):
        value = self.queue(5001)
        batch_store.save(self.directory, value, self.linked)
        path = self.directory / batch_store.DATABASE
        with closing(sqlite3.connect(path)) as connection, connection:
            connection.execute('CREATE TABLE updates (id TEXT)')
            connection.execute('CREATE TRIGGER changed_image AFTER UPDATE ON items BEGIN INSERT INTO updates VALUES (NEW.id); END')
        value['items'][3017]['status'] = 'ready'
        value.update(modified=2, message='Image ready')
        batch_store.save(self.directory, value, self.linked, [(3017, value['items'][3017])], progress=True)
        with closing(sqlite3.connect(path)) as connection:
            self.assertEqual(connection.execute('SELECT id FROM updates').fetchall(), [(value['items'][3017]['id'],)])
        self.assertEqual(batch_store.load(self.directory, self.linked), value)

    def test_failed_transaction_keeps_metadata_and_all_image_states(self):
        value = self.queue(3)
        batch_store.save(self.directory, value, self.linked)
        original = batch_store.load(self.directory, self.linked)
        value.update(modified=3, cancel_requested=True)
        value['items'][0]['status'] = 'ready'
        invalid = {'id': str(uuid.uuid4()), 'status': 'ready'}
        with self.assertRaises(sqlite3.IntegrityError):
            batch_store.save(self.directory, value, self.linked, [(0, value['items'][0]), (1, invalid)], progress=True)
        self.assertEqual(batch_store.load(self.directory, self.linked), original)

    def test_failed_initial_save_publishes_no_partial_database(self):
        value = self.queue(2)
        value['items'][1]['id'] = value['items'][0]['id']
        legacy = self.directory / 'job.json'
        legacy.write_text('legacy queue remains intact')
        with self.assertRaises(sqlite3.IntegrityError):
            batch_store.save(self.directory, value, self.linked)
        self.assertFalse((self.directory / batch_store.DATABASE).exists())
        self.assertEqual(legacy.read_text(), 'legacy queue remains intact')
        self.assertEqual(list(self.directory.iterdir()), [legacy])

    @unittest.skipIf(sys.platform == 'win32', 'POSIX symlink fixture')
    def test_linked_database_or_journal_is_rejected(self):
        for suffix in ('', '-journal', '-wal', '-shm'):
            path = self.directory / (batch_store.DATABASE + suffix)
            path.symlink_to(self.directory / 'outside')
            with self.assertRaises(ValueError):
                batch_store.save(self.directory, self.queue(1), self.linked)
            path.unlink()

    def test_storage_ledger_tracks_replacement_removal_and_export_without_full_rescan(self):
        item_id = str(uuid.uuid4())
        folder = self.directory / item_id
        folder.mkdir()
        asset = folder / 'preview.png'
        asset.write_bytes(b'x' * 10)
        (self.directory / 'job.json').write_bytes(b'x' * 5)
        ledger = batch_store.StorageLedger(self.directory, self.linked)
        self.assertEqual(ledger.total, 15)
        asset.write_bytes(b'x' * 17)
        ledger.refresh_item(item_id)
        self.assertEqual(ledger.total, 22)
        asset.unlink()
        ledger.refresh_item(item_id)
        self.assertEqual(ledger.total, 5)
        exports = self.directory / 'exports'
        exports.mkdir()
        (exports / 'product.png').write_bytes(b'x' * 20)
        ledger.refresh_export({'output_name': 'product.png'})
        ledger.refresh_export({'output_name': 'product.png'})
        self.assertEqual(ledger.total, 25)


if __name__ == '__main__':
    unittest.main()
