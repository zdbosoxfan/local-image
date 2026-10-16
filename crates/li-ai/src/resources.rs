//! ComfyUI telemetry. Endpoint shapes follow Comfy-Org/ComfyUI server.py. Probes are read-only
//! and run on the existing AI status worker; disconnected/stale samples expire in lc-sysmem.
use lightcraft_sysmem::gpu::Sample;
use serde_json::Value;
/// A loopback endpoint is local. LAN/Tailscale servers have independent VRAM. For a tunnel,
/// do not infer locality from its GPU name (two machines can have the same adapter).
pub fn local(host: &str) -> bool {
    host.parse::<std::net::SocketAddr>().is_ok_and(|a| a.ip().is_loopback()) || host.rsplit_once(':').is_some_and(|(h, _)| h.eq_ignore_ascii_case("localhost"))
}
pub fn accelerator(stats: &Value) -> bool {
    stats.get("devices").and_then(Value::as_array).is_some_and(|d| d.iter().any(|d| d.get("type").and_then(Value::as_str).is_some_and(|kind| kind != "cpu")))
}
pub fn parse(host: &str, stats: &Value, queue: &Value) -> Option<Sample> {
    let busy = ["queue_running", "queue_pending"]
        .into_iter()
        .map(|k| queue.get(k)?.as_array().map(|a| !a.is_empty()))
        .collect::<Option<Vec<_>>>()?
        .into_iter()
        .any(|b| b);
    let devices = stats.get("devices")?.as_array()?;
    // Comfy's active device is first; never add VRAM from different GPUs.
    let d = devices.first()?;
    let kind = d.get("type")?.as_str()?;
    if kind == "cpu" {
        return None;
    }
    let unified = kind == "mps";
    let total = d.get("vram_total")?.as_u64()?;
    let free = d.get("vram_free")?.as_u64()?.min(total);
    if total == 0 {
        return None;
    }
    Some(Sample { local: local(host), busy, total, free, unified })
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn vram_probe_local_remote_unified_and_malformed() {
        let stats = json!({"devices":[{"type":"cuda","vram_total":16000,"vram_free":4000}]});
        let q = json!({"queue_running":[[1,"other-owner"]],"queue_pending":[]});
        let s = parse("127.0.0.1:8188", &stats, &q).unwrap();
        assert!(s.local && s.busy && !s.unified);
        assert!(!parse("192.168.1.20:8188", &stats, &q).unwrap().local);
        assert!(local("[::1]:8188"));
        assert!(local("localhost:8188"));
        assert!(parse("localhost:8188", &json!({"devices":[{"type":"mps","vram_total":32000,"vram_free":9000}]}), &q).unwrap().unified);
        assert!(parse("localhost:8188", &stats, &json!({})).is_none());
        assert!(parse("localhost:8188", &json!({"devices":[]}), &q).is_none());
    }
}

#[cfg(test)]
mod hardware_tests {
    #[test]
    #[ignore = "real ComfyUI VRAM probe and one 64px smoke job; run on idle local GPU"]
    fn comfy_live_vram_budget_uses_real_device_and_queue() {
        let client = crate::ComfyClient::new("127.0.0.1:8188");
        let stats = client.system_stats().unwrap();
        let queue = client.queue_state().unwrap();
        let sample = super::parse(client.host(), &stats, &queue).expect("real accelerator required");
        assert!(sample.local && sample.total > 0);
        assert!(super::accelerator(&stats));
        let budget = lightcraft_sysmem::gpu::budget(4 << 30, sample, lightcraft_sysmem::ram_available());
        eprintln!(
            "live VRAM device={} total={} free={} busy={} app_cache_budget={}",
            stats["devices"][0]["name"], sample.total, sample.free, sample.busy, budget
        );
        assert!(budget <= sample.free / 4);
        assert!(!sample.busy, "owner queue must be idle before a smoke job");
        let graph = serde_json::json!({
            "1": {"class_type":"EmptyImage", "inputs":{"width":64,"height":64,"batch_size":1,"color":2245734}},
            "2": {"class_type":"PreviewImage", "inputs":{"images":["1",0]}}
        });
        let output = client.run_when_idle(graph, &[], &crate::JobControl::new()).expect("one small ComfyUI job");
        assert_eq!(image::load_from_memory(&output).unwrap().to_rgba8().dimensions(), (64, 64));
        assert!(!lightcraft_sysmem::gpu::pressured(), "local reservation released at terminal result");
    }
}
