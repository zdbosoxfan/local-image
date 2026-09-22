import os
import io
import json
import uuid
import time
import base64
import logging
import shutil
import hashlib
import mimetypes
import math
import copy
import asyncio
import tempfile
from datetime import datetime, timezone
from pathlib import Path, PurePosixPath
from collections import OrderedDict
from typing import Optional, Dict, Any

import aiohttp
import aiofiles
import websockets
import numpy as np
from PIL import Image
from pydantic_settings import BaseSettings
from app_paths import APP_PORT, cache_dir, read_config, workflow_file

logger = logging.getLogger("Engine")

REMOVAL_PROMPT = (
    "empty background, seamless continuation of the surrounding scene, "
    "matching surrounding textures, same lighting, colors and perspective, natural photograph"
)
REMOVAL_NEGATIVE_PROMPT = (
    "person, people, human, face, body, limbs, clothing, statue, mannequin, "
    "new objects, duplicate objects, text, logo"
)

class Settings(BaseSettings):
    HOST: str = "127.0.0.1"
    PORT: int = APP_PORT
    COMFY_HOST: str = "127.0.0.1"
    COMFY_PORT: int = read_config()['comfy_port']
    CACHE_DIR: Path = cache_dir()
    WORKFLOW_FILE: Path = workflow_file()
    MAX_CACHE_FILES: int = 20
    MAX_CACHE_SIZE_MB: int = 2048

    @property
    def source_cache_dir(self) -> Path:
        path = self.CACHE_DIR / "sources"
        path.mkdir(parents=True, exist_ok=True)
        return path

    @property
    def sent_cache_dir(self) -> Path:
        path = self.CACHE_DIR / "sent"
        path.mkdir(parents=True, exist_ok=True)
        return path

    @property
    def comfy_url(self) -> str:
        return f"{self.COMFY_HOST}:{self.COMFY_PORT}"

    @property
    def ws_url(self) -> str:
        return f"ws://{self.COMFY_HOST}:{self.COMFY_PORT}/ws"

    @property
    def http_url(self) -> str:
        return f"http://{self.COMFY_HOST}:{self.COMFY_PORT}"

config = Settings()

class SourceCache:
    def __init__(self):
        self._map: OrderedDict[str, Path] = OrderedDict()
        self._sync()

    def _sync(self):
        logger.info("Synchronizing cache with disk...")
        try:
            files = list(config.source_cache_dir.glob("*"))
            files.sort(key=lambda f: f.stat().st_mtime)
            for f in files:
                self._map[f.stem] = f
            logger.info(f"Cache synchronized. {len(self._map)} items found.")
        except Exception as e:
            logger.error(f"Failed to sync cache: {e}")

    def get(self, source_id: str) -> Optional[Path]:
        if source_id in self._map:
            logger.info(f"Cache HIT for {source_id}")
            self._map.move_to_end(source_id)
            path = self._map[source_id]
            if path.exists():
                path.touch()
                return path
            else:
                logger.warning(f"File missing for {source_id}, removing from map.")
                del self._map[source_id]
        else:
            logger.info(f"Cache MISS for {source_id}")
        return None

    async def add(self, source_id: str, content: bytes, extension: str) -> Path:
        logger.info(f"Adding {source_id} to cache ({len(content)} bytes)")
        self._enforce_limits()
        filename = f"{source_id}{extension}"
        filepath = config.source_cache_dir / filename

        async with aiofiles.open(filepath, "wb") as f:
            await f.write(content)

        self._map[source_id] = filepath
        self._map.move_to_end(source_id)
        logger.info(f"Successfully cached {source_id}")
        return filepath

    def _enforce_limits(self):
        while len(self._map) >= config.MAX_CACHE_FILES:
            _, path = self._map.popitem(last=False)
            logger.info(f"Evicting {path.name} due to count limit")
            self._delete(path)

        try:
            current_size = sum(f.stat().st_size for f in config.source_cache_dir.glob("*"))
            limit_bytes = config.MAX_CACHE_SIZE_MB * 1024 * 1024
            while current_size > limit_bytes and self._map:
                _, path = self._map.popitem(last=False)
                size = path.stat().st_size
                logger.info(f"Evicting {path.name} due to size limit")
                self._delete(path)
                current_size -= size
        except Exception as e:
            logger.error(f"Error checking cache size: {e}")

    def _delete(self, path: Path):
        try:
            if path.exists():
                os.remove(path)
        except Exception as e:
            logger.error(f"Failed to delete {path}: {e}")

cache = SourceCache()

class ImageProcessor:
    @staticmethod
    def process_mask_for_comfyui(mask_bytes: bytes) -> bytes:
        logger.info("Processing mask to ComfyUI-compatible format (black w/ alpha channel)")
        with Image.open(io.BytesIO(mask_bytes)) as mask_image:
            grayscale_mask = mask_image.convert("L")
            black_image = Image.new("RGB", grayscale_mask.size, (0, 0, 0))
            black_image.putalpha(grayscale_mask)
            
            output_buffer = io.BytesIO()
            black_image.save(output_buffer, format="PNG")
            return output_buffer.getvalue()

    @staticmethod
    def crop_and_pack(full_image_bytes: bytes, mask_bytes: bytes) -> Dict[str, Any]:
        logger.info("Processing output image: Cropping to mask area")
        start_time = time.perf_counter()

        with Image.open(io.BytesIO(full_image_bytes)).convert("RGBA") as img:
            with Image.open(io.BytesIO(mask_bytes)).convert("L") as mask:
                if img.size != mask.size:
                    logger.warning(f"Resizing mask from {mask.size} to {img.size}")
                    mask = mask.resize(img.size, Image.NEAREST)

                mask_arr = np.array(mask)
                rows = np.any(mask_arr > 0, axis=1)
                cols = np.any(mask_arr > 0, axis=0)

                if not np.any(rows) or not np.any(cols):
                    logger.warning("Mask is empty. Returning 1x1 pixel.")
                    empty = Image.new("RGBA", (1, 1), (0, 0, 0, 0))
                    return ImageProcessor._pack(empty, empty, 0, 0)

                ymin, ymax = np.where(rows)[0][[0, -1]]
                xmin, xmax = np.where(cols)[0][[0, -1]]

                pad = 16
                width, height = img.size
                ymin = max(0, ymin - pad)
                ymax = min(height, ymax + pad + 1)
                xmin = max(0, xmin - pad)
                xmax = min(width, xmax + pad + 1)

                logger.info(f"Crop bounds: x={xmin}, y={ymin}, w={xmax-xmin}, h={ymax-ymin}")

                crop_box = (int(xmin), int(ymin), int(xmax), int(ymax))
                img_crop = img.crop(crop_box)
                mask_crop = mask.crop(crop_box)

                logger.info(f"Image processing took {time.perf_counter() - start_time:.4f}s")
                return ImageProcessor._pack(img_crop, mask_crop, int(xmin), int(ymin))

    @staticmethod
    def _pack(color: Image.Image, mask: Image.Image, x: int, y: int) -> Dict[str, Any]:
        def b64(i):
            buf = io.BytesIO()
            i.save(buf, format="PNG")
            return base64.b64encode(buf.getvalue()).decode("utf-8")

        return {
            "x": x, "y": y,
            "width": color.width, "height": color.height,
            "color": b64(color), "mask": b64(mask)
        }

class ComfyClient:
    def __init__(self):
        self.client_id = str(uuid.uuid4())
        self.session = None

    @staticmethod
    async def check_health() -> bool:
        try:
            async with aiohttp.ClientSession() as session:
                async with session.get(config.http_url) as resp:
                    return True
        except Exception:
            return False

    async def execute(self, workflow: dict) -> bytes:
        logger.info(f"Starting ComfyUI execution. Client ID: {self.client_id}")
        start_time = time.perf_counter()

        async with aiohttp.ClientSession() as session:
            self.session = session
            try:
                # Current ComfyUI only permits LoadImage files inside its input
                # directory. Upload both cached inputs through its supported API.
                for node in workflow.values():
                    if node.get("class_type") == "LoadImage":
                        local_path = Path(node["inputs"]["image"])
                        node["inputs"]["image"] = await self._upload_image(local_path)

                async with websockets.connect(f"{config.ws_url}?clientId={self.client_id}") as ws:
                    logger.info("WebSocket connected")

                    prompt_id = await self._queue_prompt(workflow)
                    logger.info(f"Prompt queued. ID: {prompt_id}")
                    deadline = time.monotonic() + 20 * 60
                    while True:
                        remaining = deadline - time.monotonic()
                        if remaining <= 0:
                            raise TimeoutError("ComfyUI did not finish within 20 minutes")
                        try:
                            msg = await asyncio.wait_for(ws.recv(), timeout=remaining)
                        except asyncio.TimeoutError as error:
                            raise TimeoutError("ComfyUI did not finish within 20 minutes") from error
                        if isinstance(msg, str):
                            data = json.loads(msg)
                            event = data.get("data", {})
                            if event.get("prompt_id") != prompt_id:
                                continue
                            if data.get("type") in ("execution_error", "execution_interrupted"):
                                detail = event.get("exception_message") or data["type"]
                                raise RuntimeError(f"ComfyUI generation failed: {detail}")
                            if data.get('type') == 'executing':
                                if event.get('node') is None:
                                    logger.info("Execution finished signal received")
                                    break

                    history = await self._get_history(prompt_id)
                    image_data = await self._fetch_image(history[prompt_id]['outputs'])

                    logger.info(f"Workflow completed in {time.perf_counter() - start_time:.4f}s")
                    return image_data
            except (aiohttp.ClientError, websockets.exceptions.WebSocketException) as e:
                logger.error(f"ComfyUI Connection Error: {e}")
                raise ConnectionError(f"Failed to communicate with ComfyUI: {e}")
            except Exception as e:
                logger.error(f"ComfyUI execution failed: {e}")
                raise

    async def _upload_image(self, path: Path) -> str:
        async with aiofiles.open(path, "rb") as image_file:
            content = await image_file.read()

        # Stable names let ComfyUI deduplicate unchanged source images and masks.
        filename = hashlib.sha256(content).hexdigest() + path.suffix.lower()
        form = aiohttp.FormData()
        form.add_field(
            "image", content, filename=filename,
            content_type=mimetypes.guess_type(filename)[0] or "application/octet-stream",
        )
        form.add_field("type", "input")
        form.add_field("subfolder", "rapidraw")
        async with self.session.post(f"{config.http_url}/upload/image", data=form) as response:
            if response.status != 200:
                detail = await response.text()
                raise RuntimeError(f"ComfyUI image upload failed ({response.status}): {detail}")
            uploaded = await response.json()

        name = uploaded.get("name")
        subfolder = uploaded.get("subfolder", "")
        if not isinstance(name, str) or not name or not isinstance(subfolder, str):
            raise RuntimeError("ComfyUI image upload returned invalid file metadata")
        relative_path = PurePosixPath(subfolder.replace("\\", "/")) / name
        if (uploaded.get("type") != "input" or relative_path.is_absolute()
                or ".." in relative_path.parts or ":" in str(relative_path)):
            raise RuntimeError("ComfyUI image upload returned an invalid input path")
        return f"{relative_path.as_posix()} [input]"

    async def _queue_prompt(self, workflow: dict) -> str:
        payload = {"prompt": workflow, "client_id": self.client_id}
        async with self.session.post(f"{config.http_url}/prompt", json=payload) as resp:
            if resp.status != 200:
                text = await resp.text()
                logger.error(f"Queue prompt failed: {text}")
                raise Exception(f"ComfyUI Error: {text}")
            data = await resp.json()
            return data['prompt_id']

    async def _get_history(self, prompt_id: str) -> dict:
        async with self.session.get(f"{config.http_url}/history/{prompt_id}") as resp:
            return await resp.json()

    async def _fetch_image(self, outputs: dict) -> bytes:
        for node_id, node_output in outputs.items():
            if 'images' in node_output:
                img_meta = node_output['images'][0]
                logger.info(f"Fetching result image: {img_meta['filename']}")
                params = {
                    "filename": img_meta['filename'],
                    "subfolder": img_meta['subfolder'],
                    "type": img_meta['type']
                }
                async with self.session.get(f"{config.http_url}/view", params=params) as resp:
                    if resp.status != 200:
                        raise Exception("Failed to download result image")
                    return await resp.read()
        raise Exception("No output images found in workflow response")

async def save_inputs_for_debug(source_path: Path, mask_bytes: bytes):
    try:
        dest_img_path = config.sent_cache_dir / f"last_sent_image{source_path.suffix}"
        dest_mask_path = config.sent_cache_dir / "last_sent_mask.png"

        async with aiofiles.open(source_path, 'rb') as f_src:
            content = await f_src.read()
            async with aiofiles.open(dest_img_path, 'wb') as f_dst:
                await f_dst.write(content)

        async with aiofiles.open(dest_mask_path, "wb") as f:
            await f.write(mask_bytes)

        logger.info(f"Saved debug inputs to {config.sent_cache_dir.absolute()}")
    except Exception as e:
        logger.error(f"Failed to save debug inputs: {e}", exc_info=True)

def generation_size(source_size, mask_bbox, max_edge, context, blend=0):
    """Keep the masked region's aspect ratio within an SDXL-sized pixel budget."""
    source_width, source_height = source_size
    left, top, right, bottom = mask_bbox or (0, 0, source_width, source_height)
    left, top = max(0, left - blend), max(0, top - blend)
    right, bottom = min(source_width, right + blend), min(source_height, bottom + blend)
    pad_x = (right - left) * max(0, context - 1) / 2
    pad_y = (bottom - top) * max(0, context - 1) / 2
    width = max(1, min(source_width, right + pad_x) - max(0, left - pad_x))
    height = max(1, min(source_height, bottom + pad_y) - max(0, top - pad_y))
    ratio = max(0.25, min(4.0, width / height))
    max_edge = max(512, min(2048, int(max_edge)))
    width, height = (max_edge, max_edge / ratio) if ratio >= 1 else (max_edge * ratio, max_edge)
    scale = min(1.0, math.sqrt((1536 * 1536) / (width * height)))
    return tuple(max(512, int(d * scale // 64) * 64) for d in (width, height))


def load_workflow() -> dict:
    if not config.WORKFLOW_FILE.exists():
        logger.error(f"Workflow file not found at {config.WORKFLOW_FILE.absolute()}")
        raise FileNotFoundError("Workflow JSON file is missing")

    try:
        with open(config.WORKFLOW_FILE, "r", encoding="utf-8") as f:
            wf = json.load(f)
    except json.JSONDecodeError as e:
        logger.error(f"Invalid JSON in workflow file: {e}")
        raise ValueError("Workflow JSON is invalid")

    return wf


def build_workflow(source_path: str, mask_path: str, prompt: str, neg_prompt: str,
                   seed: int, workflow_template=None, target_size=None) -> dict:
    # Every region in one request uses the same immutable settings snapshot.
    wf = copy.deepcopy(workflow_template if workflow_template is not None else load_workflow())
    logger.info(f"Building workflow from {config.WORKFLOW_FILE}. Seed: {seed}")

    try:
        wf["28"]["inputs"]["seed"] = seed
        # RapidRAW sends an empty prompt for an unprompted fill. Treat that as
        # background removal, while preserving explicit replacement prompts.
        if not prompt.strip():
            prompt = REMOVAL_PROMPT
            neg_prompt = ", ".join(filter(None, [REMOVAL_NEGATIVE_PROMPT, neg_prompt]))
            logger.info("Blank prompt: using background-removal conditioning")
        wf["7"]["inputs"]["text"] = ", ".join(filter(None, [prompt, wf["7"]["inputs"]["text"]]))
        wf["8"]["inputs"]["text"] = ", ".join(filter(None, [neg_prompt, wf["8"]["inputs"]["text"]]))

        source = source_path.replace("\\", "/")
        mask = mask_path.replace("\\", "/")

        wf["30"]["inputs"]["image"] = source
        wf["47"]["inputs"]["image"] = mask

        # The connector's original workflow forced every selection into a square.
        # Match the crop aspect to the actual mask so wide selections spend their
        # generation pixels on the selected area, not on padded context.
        if target_size is None:
            with Image.open(source_path) as source_image, Image.open(mask_path) as mask_image:
                mask_alpha = mask_image.getchannel("A") if "A" in mask_image.getbands() else mask_image.convert("L")
                target_size = generation_size(
                    source_image.size, mask_alpha.getbbox(), wf["37"]["inputs"]["value"],
                    wf["36"]["inputs"]["context_from_mask_extend_factor"],
                    wf["36"]["inputs"].get("mask_blend_pixels", 0),
                )
        target_width, target_height = target_size
        wf["36"]["inputs"]["output_target_width"] = target_width
        wf["36"]["inputs"]["output_target_height"] = target_height
        wf["9"]["inputs"]["width"] = target_width
        wf["9"]["inputs"]["height"] = target_height
        logger.info(f"Adaptive generation size: {target_width}x{target_height}")

        return wf
    except KeyError as e:
        logger.error(f"Workflow JSON missing expected node ID: {e}")
        raise ValueError(f"Workflow JSON missing node: {e}")


def _array_bbox(mask, offset=(0, 0)):
    rows, cols = np.any(mask, axis=1), np.any(mask, axis=0)
    if not rows.any() or not cols.any():
        return None
    ys, xs = np.flatnonzero(rows), np.flatnonzero(cols)
    return [int(xs[0] + offset[0]), int(ys[0] + offset[1]),
            int(xs[-1] + offset[0] + 1), int(ys[-1] + offset[1] + 1)]


def _best_region_split(significant, box, minimum_gap):
    """Split only where a wide horizontal or vertical gap separates real mask areas.

    This conservative projection method keeps overlapping/nearby components
    together. A cut partitions the original mask, including its feathered pixels;
    thresholding is used for planning only, never to replace the original mask.
    """
    left, top, right, bottom = box
    crop = significant[top:bottom, left:right]
    best = None
    for axis, offset in ((0, left), (1, top)):
        counts = crop.sum(axis=axis)
        occupied = counts > 0
        indices = np.flatnonzero(occupied)
        if len(indices) < 2:
            continue
        starts, ends = indices[:-1] + 1, indices[1:]
        lengths = ends - starts
        cumulative = np.cumsum(counts)
        total = int(cumulative[-1])
        for index in np.flatnonzero(lengths >= minimum_gap):
            # Avoid creating separate expensive jobs for isolated noisy pixels.
            before = int(cumulative[starts[index] - 1])
            if before < 16 or total - before < 16:
                continue
            candidate = (int(lengths[index]), axis,
                         int(offset + (starts[index] + ends[index]) // 2))
            if best is None or candidate[0] > best[0]:
                best = candidate
    return best


def analyze_mask(mask_image: Image.Image, workflow: dict) -> dict:
    """Return JSON-compatible region planning shared by generation and settings UI."""
    mask = np.asarray(mask_image.convert("L"))
    height, width = mask.shape
    crop_settings = workflow["36"]["inputs"]
    threshold = max(1, int(math.ceil(float(crop_settings.get("mask_hipass_filter", 0.1)) * 255)))
    significant = mask >= threshold
    # A deliberately very soft mask still remains a valid selection.
    if not significant.any():
        significant = mask > 0
    selection_bbox = _array_bbox(mask > 0)
    partitions = [[0, 0, width, height]] if selection_bbox else []
    blend = int(crop_settings.get("mask_blend_pixels", 32))
    minimum_gap = max(64, blend * 4)
    # Bound additional inference work for fragmented selections.
    while len(partitions) < 8:
        candidates = [(_best_region_split(significant, box, minimum_gap), i)
                      for i, box in enumerate(partitions)]
        candidates = [(split, i) for split, i in candidates if split is not None]
        if not candidates:
            break
        (gap, axis, cut), index = max(candidates, key=lambda item: item[0][0])
        left, top, right, bottom = partitions.pop(index)
        if axis == 0:
            partitions.extend([[left, top, cut, bottom], [cut, top, right, bottom]])
        else:
            partitions.extend([[left, top, right, cut], [left, cut, right, bottom]])
    partitions.sort(key=lambda box: (box[1], box[0]))
    regions = []
    for box in partitions:
        left, top, right, bottom = box
        bbox = _array_bbox(mask[top:bottom, left:right] > 0, (left, top))
        core_bbox = _array_bbox(significant[top:bottom, left:right], (left, top)) or bbox
        dimensions = generation_size(
            (width, height), core_bbox, workflow["37"]["inputs"]["value"],
            crop_settings["context_from_mask_extend_factor"], blend,
        )
        regions.append({"partition": box, "bbox": bbox, "core_bbox": core_bbox,
                        "generation_size": list(dimensions)})
    return {"canvas_size": [width, height], "selection_bbox": selection_bbox,
            "selected_fraction": float(np.count_nonzero(mask) / mask.size),
            "region_count": len(regions), "regions": regions,
            "mask_threshold": threshold, "minimum_separation": minimum_gap}


_last_diagnostics = None
_preview_diagnostics = None
_preview_key = None


def _diagnostics_status(plan, status, **extra):
    global _last_diagnostics
    _last_diagnostics = {**copy.deepcopy(plan), "status": status,
                         "timestamp": datetime.now(timezone.utc).isoformat(), **extra}


def get_last_diagnostics() -> dict:
    global _preview_diagnostics, _preview_key
    if _last_diagnostics is not None:
        return copy.deepcopy(_last_diagnostics)
    mask_path = config.sent_cache_dir / "last_sent_mask.png"
    if not mask_path.exists() or not config.WORKFLOW_FILE.exists():
        return {"status": "unavailable", "region_count": 0, "regions": []}
    key = (mask_path.stat().st_mtime_ns, config.WORKFLOW_FILE.stat().st_mtime_ns)
    if key != _preview_key:
        with Image.open(mask_path) as saved_mask:
            mask = saved_mask.getchannel("A") if "A" in saved_mask.getbands() else saved_mask.convert("L")
            plan = analyze_mask(mask, load_workflow())
        _preview_diagnostics = {**plan, "status": "saved_mask_preview",
                                "timestamp": datetime.fromtimestamp(mask_path.stat().st_mtime, timezone.utc).isoformat()}
        _preview_key = key
    return copy.deepcopy(_preview_diagnostics)


async def run_inpaint(source_path: Path, mask_bytes: bytes, prompt: str,
                      negative_prompt: str, seed: int) -> dict:
    workflow_template = load_workflow()
    with Image.open(source_path) as source_image:
        source_size = source_image.size
    with Image.open(io.BytesIO(mask_bytes)) as received_mask:
        mask = received_mask.convert("L")
    if mask.size != source_size:
        raise ValueError(f"Mask dimensions {mask.size} must match source dimensions {source_size}")
    plan = await asyncio.to_thread(analyze_mask, mask, workflow_template)
    _diagnostics_status(plan, "planning")
    # Keep the combined original selection for diagnosis; region jobs must not
    # overwrite this with only the last disconnected region.
    processed_mask = ImageProcessor.process_mask_for_comfyui(mask_bytes)
    await save_inputs_for_debug(source_path, processed_mask)
    if not plan["regions"]:
        result_bytes = await asyncio.to_thread(source_path.read_bytes)
        response = ImageProcessor.crop_and_pack(result_bytes, mask_bytes)
        _diagnostics_status(plan, "complete", active_region=0)
        return response
    logger.info("Processing %s spatially separated mask groups", plan["region_count"])
    try:
        if not prompt.strip():
            from specialized_removal import run_removal
            return await run_removal(source_path, mask, seed, plan)
        config.CACHE_DIR.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="inpaint_", dir=config.CACHE_DIR) as temporary:
            current_source = source_path
            for index, region in enumerate(plan["regions"]):
                _diagnostics_status(plan, "running", active_region=index + 1)
                # These partitions are disjoint and cover the whole mask canvas,
                # so every original feathered pixel belongs to exactly one job.
                region_mask = Image.new("L", source_size, 0)
                box = tuple(region["partition"])
                region_mask.paste(mask.crop(box), box[:2])
                black = Image.new("RGB", source_size, (0, 0, 0))
                black.putalpha(region_mask)
                temp_mask_path = Path(temporary) / f"mask_{index}.png"
                await asyncio.to_thread(black.save, temp_mask_path, format="PNG")
                workflow = build_workflow(
                    str(current_source.absolute()), str(temp_mask_path.absolute()),
                    prompt, negative_prompt, (seed + index) % (2 ** 64),
                    workflow_template=workflow_template,
                    target_size=region["generation_size"],
                )
                # A uniformly soft selection can sit wholly below the crop
                # node's cutoff. Keep its original strength and feathering,
                # but do not let that cutoff erase the entire region mask.
                maximum_mask_value = region_mask.getextrema()[1]
                cutoff = float(workflow["36"]["inputs"].get("mask_hipass_filter", 0.1))
                if 0 < maximum_mask_value / 255.0 < cutoff:
                    workflow["36"]["inputs"]["mask_hipass_filter"] = 0.0
                result_bytes = await ComfyClient().execute(workflow)
                with Image.open(io.BytesIO(result_bytes)) as generated:
                    if generated.size != source_size:
                        raise ValueError("ComfyUI returned an unexpected canvas size")
                if index + 1 < plan["region_count"]:
                    # Each subsequent job sees and preserves completed edits.
                    current_source = Path(temporary) / f"source_{index}.png"
                    await asyncio.to_thread(current_source.write_bytes, result_bytes)
            response = ImageProcessor.crop_and_pack(result_bytes, mask_bytes)
        _diagnostics_status(plan, "complete", active_region=plan["region_count"])
        return response
    except Exception as error:
        _diagnostics_status(plan, "error", error=str(error))
        raise
