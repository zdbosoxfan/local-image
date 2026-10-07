"""One authenticated local operation's measured stages, never invented ETA."""
from contextlib import contextmanager
from contextvars import ContextVar
import asyncio
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
LABELS['cancelling'] = 'Stopping image operation'


class OperationCancelled(asyncio.CancelledError):
    """Explicit user cancellation, passed through model adapters unchanged."""


class CancellationUnavailable(RuntimeError):
    pass


class OperationProgress:
    def __init__(self, operation='generate', model='', job_id=None):
        self.job_id = job_id or str(uuid.uuid4())
        self.operation, self.model = operation, model
        self.started = self.updated = time.monotonic()
        self.finished = None
        self.stage, self.source, self.prompt_id = 'preparing', 'elapsed', None
        self.progress, self.error, self.connection_lost = None, '', False
        self.active = True
        self.cancel_requested = self.cancel_dispatched = False
        self.cancel_error = ''
        self.cancellable = True
        self._cancel_handler = None
        self._cancel_lock = asyncio.Lock()
        self._cancel_stage = 'preparing'

    def check_cancelled(self):
        if self.cancel_requested and not self.prompt_id:
            raise OperationCancelled('Image operation cancelled.')

    def prevent_cancel(self):
        # Once a usable image exists, keep it even if Stop arrives late. Do
        # not cancel disk publication or leave an untracked generated image.
        self.cancellable = False
        self.cancel_requested = self.cancel_dispatched = False

    async def dispatch_cancel(self):
        async with self._cancel_lock:
            if not self.active or not self.cancel_requested or self.cancel_dispatched:
                return
            if self._cancel_handler is None or not self.prompt_id:
                return  # Preparation/queue submission checks this flag later.
            try:
                accepted = await self._cancel_handler(self.prompt_id)
            except Exception:
                if not self.active or not self.cancellable or not self.cancel_requested:
                    return  # A usable result won the race while HTTP waited.
                self.cancel_requested = False
                self.update(self._cancel_stage)
                raise
            if not self.active or not self.cancellable or not self.cancel_requested:
                return
            self.cancel_dispatched = accepted
            if not accepted and self.active:
                self.cancel_requested = False
                self.update(self._cancel_stage)

    async def request_cancel(self):
        if not self.active or not self.cancellable:
            return False
        if not self.cancel_requested:
            self._cancel_stage = self.stage
            self.cancel_error = ''
            self.cancel_requested = True
            self.update('cancelling')
        await self.dispatch_cancel()
        return self.cancel_requested

    def update(self, stage, *, source='elapsed', value=None, maximum=None):
        if not self.active or stage not in LABELS:
            return
        if self.cancel_requested and stage not in ('cancelled', 'error', 'completed'):
            stage = 'cancelling'
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
                'connection_lost': self.connection_lost, 'error': self.error,
                'can_cancel': self.active and self.cancellable and not self.cancel_requested,
                'cancelling': self.active and self.cancel_requested,
                'cancel_error': self.cancel_error}


def current():
    return _current.get()


def snapshot():
    return _latest.snapshot() if _latest else {'active': False, 'job_id': None, 'stage': 'idle',
        'stage_label': '', 'progress': None, 'elapsed_seconds': 0, 'source': 'elapsed',
        'can_cancel': False, 'cancelling': False}


async def cancel(job_id):
    run = _latest
    if run is None or run.job_id != job_id:
        raise ValueError('The image operation changed. Check its status before stopping it.')
    requested = await run.request_cancel()
    return {**run.snapshot(), 'cancellation_requested': requested}


@contextmanager
def operation(model='', name='generate', *, job_id=None):
    global _latest
    run = OperationProgress(name, model, job_id)
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
            if kind == 'execution_interrupted' and self.run.cancel_requested:
                self.error = OperationCancelled('Image operation cancelled.')
                self.run.finish('cancelled')
                return
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
