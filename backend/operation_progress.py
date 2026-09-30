"""One authenticated local operation's measured stages, never invented ETA."""
from contextlib import contextmanager
from contextvars import ContextVar
import math
import time
import uuid

_current = ContextVar('local_image_operation', default=None)
_latest = None
LABELS = {'preparing': 'Preparing image', 'queued': 'Queued in ComfyUI',
          'loading': 'Loading model', 'conditioning': 'Preparing prompt and references',
          'sampling': 'Generating image', 'decoding': 'Decoding image',
          'saving': 'Saving image', 'running': 'Processing image',
          'completed': 'Image ready', 'error': 'Image operation failed',
          'cancelled': 'Image operation cancelled'}


class OperationProgress:
    def __init__(self, operation='generate', model=''):
        self.job_id = str(uuid.uuid4())
        self.operation, self.model = operation, model
        self.started = self.updated = time.monotonic()
        self.finished = None
        self.stage, self.source, self.prompt_id = 'preparing', 'elapsed', None
        self.progress, self.error, self.connection_lost = None, '', False
        self.active = True

    def update(self, stage, *, source='elapsed', value=None, maximum=None):
        if not self.active or stage not in LABELS:
            return
        self.stage, self.source, self.updated = stage, source, time.monotonic()
        self.progress = None
        # Comfy's max/value describes one node's sampler progress. It cannot
        # describe model-loading time or a percentage of the whole operation.
        if stage == 'sampling' and type(value) in (int, float) and type(maximum) in (int, float):
            if math.isfinite(value) and math.isfinite(maximum) and maximum > 0 and 0 <= value <= maximum:
                self.progress = {'value': value, 'max': maximum, 'percent': round(value / maximum * 100, 1)}

    def finish(self, stage='completed', error=''):
        if not self.active:
            return
        self.update(stage)
        self.active, self.finished = False, time.monotonic()
        self.error = str(error)[:300]

    def snapshot(self):
        now = time.monotonic()
        return {'active': self.active, 'job_id': self.job_id, 'operation': self.operation,
                'model': self.model, 'prompt_id': self.prompt_id, 'stage': self.stage,
                'stage_label': LABELS[self.stage], 'progress': dict(self.progress) if self.progress else None,
                'elapsed_seconds': round((self.finished or now) - self.started, 1),
                'updated_seconds_ago': round(now - self.updated, 1), 'source': self.source,
                'connection_lost': self.connection_lost, 'error': self.error}


def current():
    return _current.get()


def snapshot():
    return _latest.snapshot() if _latest else {'active': False, 'job_id': None, 'stage': 'idle',
        'stage_label': '', 'progress': None, 'elapsed_seconds': 0, 'source': 'elapsed'}


@contextmanager
def operation(model='', name='generate'):
    global _latest
    run = OperationProgress(name, model)
    _latest = run
    token = _current.set(run)
    try:
        yield run
    except BaseException as error:
        import asyncio
        run.finish('cancelled' if isinstance(error, asyncio.CancelledError) else 'error', error)
        raise
    else:
        run.finish()
    finally:
        _current.reset(token)


def node_stage(class_type):
    name = str(class_type).casefold()
    if 'sampler' in name:
        return 'sampling'
    if 'decode' in name:
        return 'decoding'
    if 'saveimage' in name or 'previewimage' in name:
        return 'saving'
    if 'loader' in name or 'loadimage' in name or 'cache' in name:
        return 'loading'
    if 'encode' in name or 'conditioning' in name:
        return 'conditioning'
    return 'running'


class GraphProgress:
    """Filter websocket events to the one submitted prompt, including races."""
    def __init__(self, run, workflow):
        self.run, self.workflow = run, workflow
        self.prompt_id, self.error = None, None
        self.pending = []

    def queued(self, prompt_id):
        self.prompt_id = self.run.prompt_id = prompt_id
        self.run.update('queued')
        pending, self.pending = self.pending, []
        for event in pending:
            self.accept(event)

    def accept(self, event):
        if not isinstance(event, dict) or not isinstance(event.get('data'), dict):
            return
        data, kind = event['data'], event.get('type')
        if self.prompt_id is None:
            if data.get('prompt_id') and len(self.pending) < 50:
                self.pending.append(event)
            return
        if data.get('prompt_id') != self.prompt_id:
            return
        if kind in ('execution_error', 'execution_interrupted'):
            self.error = RuntimeError('Image operation failed: ' + str(data.get('exception_message') or kind)[:300])
            self.run.finish('error', self.error)
            return
        if kind == 'execution_start':
            self.run.update('loading', source='comfyui')
        elif kind == 'executing':
            node = data.get('node')
            # None is the completed-graph signal; history remains the source
            # of truth for usable image output and errors.
            self.run.update(node_stage(self.workflow.get(str(node), {}).get('class_type')) if node is not None else 'saving', source='comfyui')
        elif kind == 'progress':
            node = self.workflow.get(str(data.get('node')), {})
            stage = node_stage(node.get('class_type'))
            self.run.update(stage, source='comfyui', value=data.get('value'), maximum=data.get('max'))
