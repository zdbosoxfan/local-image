import os
import uuid
import base64
import time
import logging
import logging.config
import asyncio
import aiofiles
from contextlib import asynccontextmanager
from fastapi import FastAPI, UploadFile, File, Form, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel
from engine import config, cache, ComfyClient, run_inpaint
from quality_settings import router as quality_settings_router
from local_remove import router as local_remove_router
from setup_routes import router as setup_router
from qwen_setup import router as qwen_setup_router
from image_generation import router as image_generation_router
from hardware_guide import router as hardware_guide_router
from app_update import router as app_update_router
from lora_library import router as lora_library_router
from stock_library import router as stock_library_router
from generation_library import router as generation_library_router
from image_upscale import router as image_upscale_router
from batch_tools import router as batch_tools_router

class EndpointFilter(logging.Filter):
    def filter(self, record: logging.LogRecord) -> bool:
        return record.args and len(record.args) >= 3 and record.args[2] != "/health"

LOGGING_CONFIG = {
    "version": 1,
    "disable_existing_loggers": False,
    "formatters": {
        "standard": {
            "format": "%(asctime)s [%(levelname)s] %(name)s: %(message)s",
            "datefmt": "%Y-%m-%d %H:%M:%S"
        },
        "system_formatter": {
            "format": "%(asctime)s [%(levelname)s] System: %(message)s",
            "datefmt": "%Y-%m-%d %H:%M:%S"
        },
        "routes_formatter": {
            "format": "%(asctime)s [%(levelname)s] Routes: %(message)s",
            "datefmt": "%Y-%m-%d %H:%M:%S"
        },
    },
    "handlers": {
        "default": {
            "formatter": "standard",
            "class": "logging.StreamHandler",
            "stream": "ext://sys.stdout",
        },
        "system_handler": {
            "formatter": "system_formatter",
            "class": "logging.StreamHandler",
            "stream": "ext://sys.stdout",
        },
        "routes_handler": {
            "formatter": "routes_formatter",
            "class": "logging.StreamHandler",
            "stream": "ext://sys.stdout",
        },
    },
    "loggers": {
        "API": {"handlers": ["default"], "level": "INFO"},
        "Engine": {"handlers": ["default"], "level": "INFO"},
        "uvicorn": {"handlers": ["default"], "level": "INFO"},
        "uvicorn.error": {
            "handlers": ["system_handler"],
            "level": "INFO",
            "propagate": False
        },
        "uvicorn.access": {"handlers": ["routes_handler"], "level": "INFO", "propagate": False},
    },
}

logging.config.dictConfig(LOGGING_CONFIG)
logging.getLogger("uvicorn.access").addFilter(EndpointFilter())
logger = logging.getLogger("API")

@asynccontextmanager
async def lifespan(app: FastAPI):
    logger.info("AI Connector Starting...")
    logger.info(f"Listen: {config.HOST}:{config.PORT}")
    logger.info(f"Target: {config.comfy_url}")
    logger.info(f"Cache:  {config.source_cache_dir.absolute()}")

    is_up = await ComfyClient.check_health()
    if is_up:
        logger.info("Connection to ComfyUI established")
    else:
        logger.info("ComfyUI is not connected. Quick Heal remains available.")

    yield
    logger.info("Shutting down...")

app = FastAPI(title="Local Image", lifespan=lifespan)
app.include_router(quality_settings_router)
app.include_router(local_remove_router)
app.include_router(setup_router)
app.include_router(qwen_setup_router)
app.include_router(image_generation_router)
app.include_router(generation_library_router)
app.include_router(image_upscale_router)
app.include_router(batch_tools_router)
app.include_router(hardware_guide_router)
app.include_router(lora_library_router)
app.include_router(stock_library_router)
app.include_router(app_update_router)

app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_methods=["*"],
    allow_headers=["*"],
)

class InpaintPayload(BaseModel):
    source_id: str
    prompt: str
    negative_prompt: str = "blur, low quality, distortion, watermark"
    mask_image_base64: str
    seed: int = 0


generation_lock = asyncio.Lock()

@app.get("/health")
async def health():
    is_up = await ComfyClient.check_health()
    return {
        "status": "ok" if is_up else "error",
        "comfy_url": config.comfy_url,
        "connected": is_up,
        "removal_model": "FLUX.2 Klein base 4B + fal object-removal LoRA"
    }

@app.post("/upload_source")
async def upload_source(file: UploadFile = File(...), source_id: str = Form(...)):
    start_time = time.perf_counter()
    logger.info(f"Received upload request for {source_id}")

    try:
        ext = os.path.splitext(file.filename)[1] or ".png"
        content = await file.read()
        logger.info(f"Read {len(content)} bytes from request")

        path = await cache.add(source_id, content, ext)

        logger.info(f"Upload completed in {time.perf_counter() - start_time:.4f}s")
        return {"status": "cached", "path": str(path.absolute())}
    except Exception as e:
        logger.error(f"Upload failed: {e}", exc_info=True)
        raise HTTPException(500, f"Upload error: {str(e)}")

@app.post("/inpaint")
async def inpaint(req: InpaintPayload):
    req_start = time.perf_counter()
    logger.info(f"Inpaint Request | Source: {req.source_id} | Prompt: {req.prompt}")

    source_path = cache.get(req.source_id)
    if not source_path:
        logger.warning(f"Source {req.source_id} not found in cache. Returning 404.")
        raise HTTPException(404, "Source ID not found. Upload required.")

    try:
        mask_bytes = base64.b64decode(req.mask_image_base64, validate=True)
        seed = req.seed or int(time.time())
        async with generation_lock:
            response = await run_inpaint(
                source_path, mask_bytes, req.prompt, req.negative_prompt, seed,
            )

        logger.info(f"Total Request Time: {time.perf_counter() - req_start:.4f}s")
        return response

    except ConnectionError as ce:
        logger.error(f"ComfyUI Unavailable: {ce}")
        raise HTTPException(502, f"ComfyUI Unavailable: {str(ce)}")
    except Exception as e:
        logger.error(f"Inpaint processing failed: {e}", exc_info=True)
        raise HTTPException(500, f"Processing error: {str(e)}")

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host=config.HOST, port=config.PORT, log_config=LOGGING_CONFIG)
