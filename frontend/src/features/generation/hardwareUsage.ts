import type { HardwareUsage } from './types.ts';

const gib = 1024 ** 3;
const metric = (value: number | null | undefined) => typeof value === 'number' && Number.isFinite(value) && value >= 0;
export function hardwareUsageSummary(sample: HardwareUsage | null, error: string | null, loading = false) {
  const device = sample?.devices.find(value => value.is_backend_device) ?? sample?.devices[0];
  if (error || !device) return { label: loading ? 'GPU checking…' : 'GPU unavailable', description: error ? 'Live GPU usage could not be read. Monitoring will retry automatically.' : 'Live GPU monitoring is not available on this device.' };
  const load = metric(device.utilization_percent) ? `${Math.round(Math.min(100, device.utilization_percent!))}%` : 'n/a';
  const memory = metric(device.vram_used_bytes) && metric(device.vram_total_bytes) && device.vram_total_bytes! > 0
    ? `${(device.vram_used_bytes! / gib).toFixed(1)} / ${(device.vram_total_bytes! / gib).toFixed(1)} GB` : 'n/a';
  const scope = device.memory_scope === 'backend' ? 'Memory reported by the AI backend.' : device.memory_scope === 'shared' ? 'Shared GPU memory; dedicated VRAM is unavailable.' : 'Device-wide memory; includes other applications.';
  return { label: `GPU ${load} · VRAM ${memory}`, description: `${device.name}. ${metric(device.utilization_percent) ? 'Current GPU load.' : 'GPU load is unavailable.'} ${scope}` };
}
