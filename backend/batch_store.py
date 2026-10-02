"""Transactional queue metadata: one row per image, with legacy JSON support.

The caller validates the owned directory. Connections are short lived and use
SQLite's default durable rollback journal, rather than rewriting every image
when a single image finishes. No extra service or dependency is required.
"""
from contextlib import closing
import json
import os
from pathlib import Path
import re
import sqlite3
import uuid

DATABASE = 'queue.sqlite3'
PROGRESS_KEYS = ('modified', 'message', 'running', 'phase', 'archive_ready', 'cancel_requested')


def staging_file(name):
    return bool(re.fullmatch(r'\.queue-[0-9a-f]{32}\.sqlite3(?:-journal)?', name))


def database_path(directory, linked):
    path = Path(directory) / DATABASE
    for candidate in (path, path.with_name(path.name + '-journal'),
                      path.with_name(path.name + '-wal'), path.with_name(path.name + '-shm')):
        if linked(candidate):
            raise ValueError('Batch metadata must be a regular local file.')
    return path


def load(directory, linked):
    path = database_path(directory, linked)
    # mode=rw prevents a missing queue from creating an empty database.
    with closing(sqlite3.connect(path.as_uri() + '?mode=rw', uri=True)) as connection:
        value = {key: json.loads(data) for key, data in connection.execute('SELECT key, value FROM metadata')}
        value['items'] = [json.loads(data) for (data,) in connection.execute('SELECT value FROM items ORDER BY position')]
    return value


def save(directory, value, linked, changed_items=None, progress=False):
    path = database_path(directory, linked)
    creating = not path.exists()
    pending = path.with_name('.queue-' + uuid.uuid4().hex + '.sqlite3') if creating else path
    try:
        with closing(sqlite3.connect(pending)) as connection:
            with connection:
                connection.execute('CREATE TABLE IF NOT EXISTS metadata (key TEXT PRIMARY KEY, value TEXT NOT NULL)')
                connection.execute('CREATE TABLE IF NOT EXISTS items (id TEXT PRIMARY KEY, position INTEGER UNIQUE NOT NULL, value TEXT NOT NULL)')
                keys = PROGRESS_KEYS if progress and not creating else [key for key in value if key != 'items']
                connection.executemany('INSERT INTO metadata(key, value) VALUES (?, ?) '
                                       'ON CONFLICT(key) DO UPDATE SET value=excluded.value WHERE metadata.value != excluded.value',
                                       ((key, json.dumps(value[key], ensure_ascii=False, separators=(',', ':')))
                                        for key in keys if key in value))
                full_save = changed_items is None or creating
                if full_save:
                    connection.execute('DELETE FROM items')
                    rows = enumerate(value['items'])
                else:
                    rows = changed_items
                item_sql = 'INSERT INTO items(id, position, value) VALUES (?, ?, ?)'
                if not full_save:
                    item_sql += ' ON CONFLICT(id) DO UPDATE SET value=excluded.value WHERE items.value != excluded.value'
                connection.executemany(item_sql,
                                       ((item['id'], position, json.dumps(item, ensure_ascii=False, separators=(',', ':')))
                                        for position, item in rows))
        if creating:
            # Publish the complete initial database only after its transaction commits.
            os.replace(pending, path)
        return creating
    finally:
        if creating:
            pending.unlink(missing_ok=True)
            pending.with_name(pending.name + '-journal').unlink(missing_ok=True)


class StorageLedger:
    """Account for touched files without walking all previous images again."""
    def __init__(self, directory, linked):
        self.directory, self.linked = Path(directory), linked
        self.files = {}
        self.folders = {}
        self.total = 0
        for base, dirs, files in os.walk(self.directory, followlinks=False):
            folder = Path(base)
            if linked(folder) or any(linked(folder / name) for name in dirs + files):
                raise ValueError('Batch storage contains a linked file or folder.')
            for name in files:
                self.refresh_file(folder / name)

    def refresh_file(self, path):
        path = Path(path)
        if self.linked(path):
            raise ValueError('Batch storage contains a linked file or folder.')
        old = self.files.get(path, 0)
        size = path.stat().st_size if path.is_file() else 0
        self.total += size - old
        if path.is_file():
            self.files[path] = size
            self.folders.setdefault(path.parent, set()).add(path)
        else:
            self.files.pop(path, None)
            self.folders.setdefault(path.parent, set()).discard(path)

    def refresh_item(self, identifier):
        folder = self.directory / identifier
        if self.linked(folder):
            raise ValueError('The batch image folder is linked.')
        previous = self.folders.get(folder, set()).copy()
        current = set()
        # Item snapshots have no nested folders. Update just this image's files,
        # including replacements/removals, instead of revisiting earlier images.
        if folder.is_dir():
            for file in folder.iterdir():
                current.add(file)
                self.refresh_file(file)
        for removed in previous - current:
            self.refresh_file(removed)

    def refresh_metadata(self):
        for name in ('job.json', DATABASE, DATABASE + '-journal', 'exports.zip', 'exports.pending.zip'):
            self.refresh_file(self.directory / name)

    def refresh_export(self, item):
        for key in ('output_name', 'credits_name'):
            if item.get(key):
                self.refresh_file(self.directory / 'exports' / item[key])
