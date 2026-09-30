"""Short-lived, coalesced read-only node inventory; execution validates fresh."""
import asyncio
import copy
from pathlib import Path
import time

from app_paths import model_directory, read_config


class InventoryCache:
    def __init__(self, ttl=5.0, clock=time.monotonic):
        self.ttl, self.clock = ttl, clock
        self.entries, self.pending, self.version = {}, {}, 0
        self.hits, self.fetches, self.coalesced = 0, 0, 0

    def invalidate(self):
        self.version += 1
        self.entries.clear()

    async def get(self, key, loader, *, refresh=False):
        key = (self.version, key)
        cached = self.entries.get(key)
        if not refresh and cached and self.clock() - cached[0] < self.ttl:
            self.hits += 1
            return copy.deepcopy(cached[1])
        task = self.pending.get(key)
        if task is None:
            async def fetch():
                self.fetches += 1
                result = await loader()
                if not isinstance(result, dict):
                    raise ValueError('Invalid ComfyUI node inventory')
                if key[0] == self.version:
                    if len(self.entries) >= 8:
                        self.entries.pop(next(iter(self.entries)))
                    self.entries[key] = (self.clock(), copy.deepcopy(result))
                return result
            task = asyncio.create_task(fetch())
            self.pending[key] = task
            def release(done):
                if self.pending.get(key) is done:
                    self.pending.pop(key, None)
                # A cancelled HTTP reader must neither cancel the shared fetch
                # nor leave an unobserved exception when nobody awaits it.
                if not done.cancelled():
                    done.exception()
            task.add_done_callback(release)
        else:
            self.coalesced += 1
        return copy.deepcopy(await asyncio.shield(task))


inventory_cache = InventoryCache()


async def read_inventory(loader, *, refresh=False):
    settings = read_config()
    root = str(Path(model_directory()).absolute()).casefold()
    # The reader identity also changes when the metadata implementation is
    # replaced; failures are never cached. Model/port changes never reuse it.
    key = ('127.0.0.1', settings['comfy_port'], root, loader)
    return await inventory_cache.get(key, loader, refresh=refresh)
