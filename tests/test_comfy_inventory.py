"""Request coalescing, scoped cache keys, TTL, force refresh, cancellation."""
import asyncio
from pathlib import Path
import sys
import unittest
from unittest.mock import AsyncMock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'backend'))
import comfy_inventory as inventory


class InventoryTests(unittest.IsolatedAsyncioTestCase):
    def setUp(self):
        self.now = 10
        self.cache = inventory.InventoryCache(ttl=5, clock=lambda: self.now)
        self.loader = AsyncMock(return_value={'UNETLoader': {'models': ['model-a']}})

    async def test_cold_and_warm_request_counts_are_measured(self):
        cold = await self.cache.get('connection/models', self.loader)
        for _ in range(20):
            warm = await self.cache.get('connection/models', self.loader)
            self.assertEqual(warm, cold)
        self.assertEqual(self.loader.await_count, 1)
        self.assertEqual((self.cache.fetches, self.cache.hits), (1, 20))
        warm['UNETLoader']['models'].append('mutation')
        self.assertEqual((await self.cache.get('connection/models', self.loader))['UNETLoader']['models'], ['model-a'])

    async def test_concurrent_reads_coalesce_even_for_force_refresh(self):
        gate = asyncio.Event()
        async def loader():
            await gate.wait(); return {'nodes': {}}
        tasks = [asyncio.create_task(self.cache.get('one', loader, refresh=True)) for _ in range(12)]
        await asyncio.sleep(0); gate.set(); await asyncio.gather(*tasks)
        self.assertEqual(self.cache.fetches, 1); self.assertEqual(self.cache.coalesced, 11)

    async def test_ttl_refresh_connection_folder_and_invalidation_fetch_new(self):
        await self.cache.get(('8188', 'D:/models'), self.loader)
        self.now += 4.999; await self.cache.get(('8188', 'D:/models'), self.loader)
        self.assertEqual(self.loader.await_count, 1)
        self.now = 15; await self.cache.get(('8188', 'D:/models'), self.loader)
        await self.cache.get(('8188', 'D:/models'), self.loader, refresh=True)
        await self.cache.get(('8199', 'D:/models'), self.loader)
        await self.cache.get(('8199', 'E:/models'), self.loader)
        self.cache.invalidate(); await self.cache.get(('8199', 'E:/models'), self.loader)
        self.assertEqual(self.loader.await_count, 6)

    async def test_one_cancelled_reader_does_not_cancel_shared_fetch(self):
        gate = asyncio.Event()
        async def loader():
            await gate.wait(); return {'valid': True}
        one = asyncio.create_task(self.cache.get('one', loader)); two = asyncio.create_task(self.cache.get('one', loader))
        await asyncio.sleep(0); one.cancel()
        with self.assertRaises(asyncio.CancelledError): await one
        gate.set(); self.assertEqual(await two, {'valid': True}); self.assertEqual(self.cache.fetches, 1)

    async def test_failed_or_invalid_inventory_is_not_cached(self):
        self.loader.side_effect = [OSError('offline'), [], {'valid': True}]
        with self.assertRaises(OSError): await self.cache.get('one', self.loader)
        with self.assertRaises(ValueError): await self.cache.get('one', self.loader)
        self.assertEqual(await self.cache.get('one', self.loader), {'valid': True})
        self.assertEqual(self.loader.await_count, 3)

    async def test_invalidation_during_fetch_cannot_repopulate_old_inventory(self):
        gate = asyncio.Event()
        async def loader():
            await gate.wait(); return {'old': True}
        old = asyncio.create_task(self.cache.get('one', loader)); await asyncio.sleep(0)
        self.cache.invalidate(); gate.set(); await old
        fresh = AsyncMock(return_value={'new': True})
        self.assertEqual(await self.cache.get('one', fresh), {'new': True}); fresh.assert_awaited_once()

    async def test_adapter_keys_current_port_and_models_folder(self):
        with patch.object(inventory, 'inventory_cache', self.cache), patch.object(inventory, 'read_config', return_value={'comfy_port': 8188}), patch.object(inventory, 'model_directory', return_value=Path('D:/models')):
            await inventory.read_inventory(self.loader); await inventory.read_inventory(self.loader)
        with patch.object(inventory, 'inventory_cache', self.cache), patch.object(inventory, 'read_config', return_value={'comfy_port': 8199}), patch.object(inventory, 'model_directory', return_value=Path('D:/models')):
            await inventory.read_inventory(self.loader)
        with patch.object(inventory, 'inventory_cache', self.cache), patch.object(inventory, 'read_config', return_value={'comfy_port': 8199}), patch.object(inventory, 'model_directory', return_value=Path('E:/models')):
            await inventory.read_inventory(self.loader)
        self.assertEqual(self.loader.await_count, 3)


if __name__ == '__main__': unittest.main()
