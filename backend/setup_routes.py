"""Native-authorized AI setup endpoints; the browser may only read status."""
from fastapi import APIRouter, HTTPException, Request
from pydantic import BaseModel, ConfigDict, Field

from local_remove import guard, launcher_guard
from managed_ai import SetupError, configure, manager
from app_paths import model_directory

router = APIRouter(prefix='/api/local-remove/setup')


class SetupConfiguration(BaseModel):
    model_config = ConfigDict(extra='forbid', strict=True)
    comfy_directory: str | None = Field(default=None, max_length=4096)
    model_directory: str | None = Field(default=None, max_length=4096)
    managed_ai_directory: str | None = Field(default=None, max_length=4096)
    installation_id: str | None = Field(default=None, max_length=64)
    comfy_port: int | None = Field(default=None, ge=1, le=65535)


class InstallDirectory(BaseModel):
    model_config = ConfigDict(extra='forbid', strict=True)
    directory: str = Field(min_length=1, max_length=4096)


class NoOptions(BaseModel):
    model_config = ConfigDict(extra='forbid')


def native_mutation(request):
    launcher_guard(request)
    from main import generation_lock
    if generation_lock.locked() or manager.active:
        raise HTTPException(409, 'Wait for the current removal or setup step to finish.')
    return generation_lock


@router.get('')
@router.get('/detect')
async def setup_status(request: Request):
    guard(request)
    return await manager.status()


@router.post('/configure')
async def setup_configure(request: Request, payload: SetupConfiguration):
    lock = native_mutation(request)
    previous_folder = model_directory().resolve()
    async with lock:
        try:
            selected = configure(**payload.model_dump(exclude_none=True))
            from engine import config
            config.COMFY_HOST = '127.0.0.1'
            config.COMFY_PORT = selected['comfy_port']
        except SetupError as error:
            raise HTTPException(400, str(error)) from error
    result = await manager.status()
    if payload.model_directory is not None:
        selected_folder = model_directory().resolve()
        changed = previous_folder != selected_folder
        running = bool(result.get('service', {}).get('running'))
        status = 'restart_required' if changed and running else 'will_apply_on_start' if changed else 'unchanged'
        message = ('Folder saved for downloads. Close ComfyUI, then choose Start AI backend in Local Image to connect this folder. Refresh the connection afterward.'
                   if changed and running else
                   'Folder saved. Start AI backend from Local Image to connect this folder.' if changed else
                   'The selected model folder is unchanged.')
        result['model_folder_changed'] = changed
        result['model_folder_connection'] = {'status': status, 'message': message,
            'selected_directory': str(selected_folder), 'comfy_running': running}
    return result


async def begin_job(request, action, operation):
    lock = native_mutation(request)
    await lock.acquire()
    try:
        manager.begin(action, operation, release=lock.release)
    except Exception:
        lock.release()
        raise
    return await manager.status()


@router.post('/install')
async def setup_install(request: Request, payload: InstallDirectory):
    return await begin_job(request, 'install', lambda: manager.install(payload.directory))


@router.post('/download-models')
async def setup_download_models(request: Request, payload: NoOptions):
    return await begin_job(request, 'download-models', manager.download_models)


@router.post('/start')
async def setup_start(request: Request, payload: NoOptions):
    return await begin_job(request, 'start', manager.start)


@router.post('/eject')
async def setup_eject(request: Request, payload: NoOptions):
    return await begin_job(request, 'eject', manager.eject)
