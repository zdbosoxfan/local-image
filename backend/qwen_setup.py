"""Native-authorized, checksum-verified generation models with shared jobs."""
import asyncio
import shutil
from pathlib import Path
from typing import Literal

from fastapi import APIRouter, HTTPException, Request
from pydantic import BaseModel, ConfigDict

from app_paths import model_directory
from local_remove import guard, launcher_guard
from managed_ai import SetupError, download_verified, folder_storage, writable_directory, manager as setup_manager
from qwen_download_catalog import QWEN_FILES, LICENSE_URL, LICENSE_NOTE
from hidream_download_catalog import HIDREAM_FILES, LICENSE_URL as HIDREAM_LICENSE_URL, LICENSE_NOTE as HIDREAM_LICENSE_NOTE
from seedvr2_download_catalog import SEEDVR2_FILES, LICENSE_URL as SEEDVR2_LICENSE_URL, LICENSE_NOTE as SEEDVR2_LICENSE_NOTE
from ernie_download_catalog import ERNIE_FILES, LICENSE_URL as ERNIE_LICENSE_URL, LICENSE_NOTE as ERNIE_LICENSE_NOTE
from generation_download_catalog import (Z_IMAGE_FILES, LICENSE_URL as Z_LICENSE_URL,
    LICENSE_NOTE as Z_LICENSE_NOTE, FLUX_DEV_FILES, FLUX_KLEIN_FILES, FLUX_KLEIN_9B_FILES,
    FLUX_DEV_LICENSE_URL, FLUX_DEV_LICENSE_NOTE, KLEIN_LICENSE_URL, KLEIN_LICENSE_NOTE,
    KLEIN_9B_LICENSE_URL, KLEIN_9B_LICENSE_NOTE)

router = APIRouter()


class QwenDownloadRequest(BaseModel):
    model_config = ConfigDict(extra='forbid', strict=True)
    variant: Literal['int8', 'bf16']


class GenerationDownloadRequest(BaseModel):
    model_config = ConfigDict(extra='forbid', strict=True)
    model: Literal['qwen', 'z-image-turbo', 'flux2-dev', 'flux2-klein-4b', 'flux2-klein-9b', 'seedvr2', 'ernie-image']
    variant: Literal['int8', 'bf16', 'fp8', 'fp16']


def _catalogs():
    return {'qwen': QWEN_FILES, 'z-image-turbo': Z_IMAGE_FILES, 'hidream-o1': HIDREAM_FILES, 'seedvr2': SEEDVR2_FILES, 'ernie-image': ERNIE_FILES,
            'flux2-dev': FLUX_DEV_FILES, 'flux2-klein-4b': FLUX_KLEIN_FILES, 'flux2-klein-9b': FLUX_KLEIN_9B_FILES}


def _label(model):
    return {'qwen': 'Qwen Image 2.1', 'z-image-turbo': 'Z-Image Turbo', 'hidream-o1': 'HiDream O1', 'seedvr2': 'SeedVR2 7B', 'ernie-image': 'ERNIE-Image Base',
            'flux2-dev': 'FLUX.2 Dev', 'flux2-klein-4b': 'FLUX.2 Klein 4B', 'flux2-klein-9b': 'FLUX.2 Klein 9B'}[model]


def _license(model):
    return {'qwen': (LICENSE_URL, LICENSE_NOTE), 'z-image-turbo': (Z_LICENSE_URL, Z_LICENSE_NOTE), 'hidream-o1': (HIDREAM_LICENSE_URL, HIDREAM_LICENSE_NOTE),
            'seedvr2': (SEEDVR2_LICENSE_URL, SEEDVR2_LICENSE_NOTE),
            'ernie-image': (ERNIE_LICENSE_URL, ERNIE_LICENSE_NOTE),
            'flux2-dev': (FLUX_DEV_LICENSE_URL, FLUX_DEV_LICENSE_NOTE),
            'flux2-klein-4b': (KLEIN_LICENSE_URL, KLEIN_LICENSE_NOTE),
            'flux2-klein-9b': (KLEIN_9B_LICENSE_URL, KLEIN_9B_LICENSE_NOTE)}[model]


def _matching_size(path, artifact):
    try:
        sizes = [item['bytes'] for item in (artifact, *artifact.get('compatible_existing', ()))]
        return path.is_file() and not path.is_symlink() and path.stat().st_size in sizes
    except OSError:
        return False


class QwenDownloadManager:
    def __init__(self):
        self.task = None
        self.job = {'phase': 'idle', 'model': None, 'variant': None, 'downloaded_bytes': 0, 'total_bytes': 0,
                    'message': 'Choose a model to download.', 'error': None}

    @property
    def active(self):
        return self.task is not None and not self.task.done()

    def status(self):
        root = model_directory()
        models = []
        for model, catalog in _catalogs().items():
            if model in {'flux2-dev', 'hidream-o1', 'seedvr2'}:
                continue  # Retired presets and the upscaler are not Image Gen recommendations.
            variants = []
            for variant, files in catalog.items():
                missing = sum(item['bytes'] for item in files
                              if not _matching_size(root / item['folder'] / item['name'], item))
                variants.append({'id': variant, 'label': {'int8': 'Compact · INT8', 'bf16': 'Full precision · BF16', 'fp8': 'FP8 mixed'}[variant],
                                 'total_bytes': sum(item['bytes'] for item in files), 'missing_bytes': missing,
                                 'installed': missing == 0, 'files_present': missing == 0})
            models.append({'id': model, 'label': _label(model), 'variants': variants,
                           'license_url': _license(model)[0], 'license_note': _license(model)[1]})
        total = self.job['total_bytes']
        return {**self.job, 'running': self.active,
                'progress': min(1.0, self.job['downloaded_bytes'] / total) if total else 0,
                'models': models, 'variants': models[0]['variants'], 'model_directory': str(root),
                'storage': folder_storage(root),
                'license_url': LICENSE_URL, 'license_note': LICENSE_NOTE}

    def begin(self, variant, release=lambda: None, model='qwen'):
        if self.active:
            raise SetupError('A model download is already running.')
        if model == 'hidream-o1':
            raise SetupError('HiDream was removed from the offered models after image-quality validation.')
        if model not in _catalogs() or variant not in _catalogs()[model]:
            raise SetupError('Choose a supported model and precision variant.')
        files = _catalogs()[model][variant]
        root = model_directory().resolve()
        self.job = {'phase': 'verifying', 'model': model, 'variant': variant, 'downloaded_bytes': 0,
                    'total_bytes': sum(item['bytes'] for item in files),
                    'message': 'Checking existing files and available disk space…', 'error': None}
        self.task = asyncio.create_task(self._run(root, model, files, release))

    async def _run(self, root, model, files, release):
        try:
            root = writable_directory(str(root), 'model folder')
            root.mkdir(parents=True, exist_ok=True)
            missing = sum(item['bytes'] for item in files
                          if not _matching_size(root / item['folder'] / item['name'], item))
            if missing and shutil.disk_usage(root).free < missing + 1024 ** 3:
                raise SetupError('There is not enough free disk space for this model. Choose a smaller model or another model folder in Settings.')
            completed = 0
            for item in files:
                destination = root / item['folder'] / item['name']
                if not destination.parent.resolve().is_relative_to(root):
                    raise SetupError('The model subfolder points outside the configured model directory. Choose a regular model folder in Settings.')
                existing = destination.exists()
                self.job.update(phase='verifying' if existing else 'downloading',
                                message=('Verifying ' if existing else 'Downloading ') + item['name'])
                def progress(done, total, offset=completed):
                    self.job['downloaded_bytes'] = offset + done
                await download_verified(item, destination, progress)
                completed += item['bytes']
                self.job['downloaded_bytes'] = completed
            self.job.update(phase='complete', message=_label(model) + ' model files are verified and ready. Refresh the connection.', error=None)
        except asyncio.CancelledError:
            self.job.update(phase='error', message='Model download was interrupted. Completed verified files were preserved.', error='Download interrupted.')
            raise
        except Exception as exc:
            self.job.update(phase='error', message=str(exc), error=str(exc))
        finally:
            release()


manager = QwenDownloadManager()


@router.get('/api/local-remove/generator/download')
@router.get('/api/local-remove/qwen/download')
async def qwen_download_status(request: Request):
    guard(request)
    return manager.status()


@router.post('/api/local-remove/qwen/download')
async def qwen_download_begin(request: Request, payload: QwenDownloadRequest):
    return await _download_begin(request, 'qwen', payload.variant)


@router.post('/api/local-remove/generator/download')
async def generation_download_begin(request: Request, payload: GenerationDownloadRequest):
    return await _download_begin(request, payload.model, payload.variant)


async def _download_begin(request, model, variant):
    launcher_guard(request)
    if model == 'hidream-o1':
        raise HTTPException(422, 'HiDream is no longer offered for download after image-quality validation.')
    if variant not in _catalogs()[model]:
        raise HTTPException(422, 'This model does not support the selected precision variant.')
    from main import generation_lock
    if manager.active or setup_manager.active or generation_lock.locked():
        raise HTTPException(409, 'Wait for the current image operation or model setup to finish.')
    # Existing setup routes use the same lock, keeping the destination configuration
    # stable while a model file is downloaded and preventing concurrent setup jobs.
    await generation_lock.acquire()
    try:
        manager.begin(variant, release=generation_lock.release, model=model)
    except Exception:
        generation_lock.release()
        raise
    return manager.status()
