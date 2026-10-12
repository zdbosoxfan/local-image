"""Shared, locked state transactions. Lock the separate inode, never jobs.json itself."""
from contextlib import contextmanager
import fcntl
import json
import os
from pathlib import Path
import tempfile

DEV = Path(os.environ.get('LI_DEV_DIR', Path.home() / '.local/share/local-image-dev'))

@contextmanager
def transaction():
    DEV.mkdir(parents=True, exist_ok=True)
    with (DEV / '.jobs.lock').open('a') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        path = DEV / 'jobs.json'
        state = json.loads(path.read_text())
        before = json.dumps(state, sort_keys=True)
        yield state
        if json.dumps(state, sort_keys=True) != before:
            fd, tmp = tempfile.mkstemp(prefix='.jobs-', dir=DEV)
            try:
                with os.fdopen(fd, 'w') as stream:
                    json.dump(state, stream, indent=1)
                    stream.write('\n')
                    stream.flush()
                    os.fsync(stream.fileno())
                os.replace(tmp, path)
            finally:
                if os.path.exists(tmp): os.unlink(tmp)

def update(name, **fields):
    with transaction() as state:
        for job in state['jobs']:
            if job['name'] == name:
                job.update(fields)
                return
        raise KeyError(name)
