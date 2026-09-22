"""Entry point bundled with Python; users do not need a Python installation."""
import asyncio
import logging
from logging.handlers import RotatingFileHandler
import os
from pathlib import Path
import sys
import time

from app_paths import APP_PORT, log_dir, prepare_user_folders, prune_thumbnails


def main():
    prepare_user_folders()
    prune_thumbnails()
    # A windowless bundled executable has no stdout/stderr. Provide streams
    # before importing uvicorn or the existing application's logging setup.
    if sys.stdout is None:
        sys.stdout = open(os.devnull, 'w')
    if sys.stderr is None:
        sys.stderr = sys.stdout
    from main import app
    import uvicorn
    handler = RotatingFileHandler(log_dir() / 'backend.log', maxBytes=5 * 1024 * 1024,
                                 backupCount=3, encoding='utf-8')
    handler.setFormatter(logging.Formatter('%(asctime)s [%(levelname)s] %(name)s: %(message)s'))
    for name in ('API', 'Engine', 'uvicorn', 'uvicorn.error', 'uvicorn.access'):
        logger = logging.getLogger(name)
        logger.handlers = [handler]
        logger.propagate = False
    try:
        server = uvicorn.Server(uvicorn.Config(app, host='127.0.0.1', port=APP_PORT,
            log_config=None, access_log=False, loop='asyncio', http='h11', ws='websockets'))

        async def serve():
            import runtime_lifecycle
            from managed_ai import manager as setup_manager
            from main import generation_lock

            async def watch():
                while not server.should_exit:
                    await asyncio.sleep(5)
                    if not generation_lock.locked() and not setup_manager.active and (runtime_lifecycle.shutdown_requested or
                            time.monotonic() - runtime_lifecycle.last_heartbeat > 75):
                        server.should_exit = True

            watcher = asyncio.create_task(watch())
            try:
                await server.serve()
            finally:
                watcher.cancel()
                try:
                    await watcher
                except asyncio.CancelledError:
                    pass

        asyncio.run(serve())
    except Exception:
        logging.getLogger('API').exception('Local Remove could not start')
        raise


if __name__ == '__main__':
    main()
