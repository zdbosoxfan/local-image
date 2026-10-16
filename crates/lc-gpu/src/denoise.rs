//! RawNIND UtNet2 / NIND compute, using validated ONNX weights and the shared CPU tiler.
//! GPU failure is an error; the engine retains GPU/NPU-only admission and can retry a fresh context.
use crate::ctx::{self, Buf};
use li_seg::{
    denoise::{Variant, match_variant_gain, run_tiled_channels},
    restoration_graph::{Graph, Op, Shape},
};
use std::path::Path;

pub struct Denoiser {
    serial: std::sync::Mutex<()>,
    graph: Graph,
    weights: Vec<Option<(Buf, Buf)>>,
    buffers: Vec<Buf>,
    kind: Variant,
    // Weights and tensors belong to this context, even if the global cache is
    // released and a later render creates a new one. Drop buffers before it.
    gpu: std::sync::Arc<ctx::Gpu>,
}
impl Denoiser {
    /// Allocated FP32 tensor/weight bytes, excluding the shared device and driver allocations.
    pub fn resident_bytes(&self) -> usize {
        let tensors = self.graph.input.len() + self.graph.nodes.iter().map(|n| n.shape.len()).sum::<usize>();
        let weights: usize = self
            .graph
            .nodes
            .iter()
            .map(|n| match &n.op {
                Op::Conv { weights, bias, .. } => weights.len() + bias.len(),
                _ => 0,
            })
            .sum();
        (tensors + weights) * std::mem::size_of::<f32>()
    }
    pub fn load(path: &Path, kind: Variant) -> Result<Self, String> {
        Self::load_at(path, kind, kind.tile())
    }
    pub fn load_at(path: &Path, kind: Variant, t: usize) -> Result<Self, String> {
        let graph = Graph::load(path, Shape { c: kind.channels(), h: t, w: t }).map_err(|e| format!("{e:#}"))?;
        Self::from_graph(graph, kind)
    }
    pub fn from_graph(graph: Graph, kind: Variant) -> Result<Self, String> {
        if !crate::enabled() || lightcraft_sysmem::gpu::pressured() {
            return Err("GPU denoise is temporarily unavailable; retry after other GPU work finishes".into());
        }
        let g = crate::device().ok_or_else(|| crate::unavailable_reason().unwrap_or_else(|| "no GPU".into()))?;
        let _scope = ctx::RenderScope::new(&g);
        let _ = ctx::take_failure();
        if std::iter::once(graph.input).chain(graph.nodes.iter().map(|n| n.shape)).any(|s| !g.fits(s.len())) {
            return Err("denoise tensor exceeds GPU limits".into());
        }
        let buffers = std::iter::once(graph.input).chain(graph.nodes.iter().map(|n| n.shape)).map(|s| g.buffer(s.len())).collect();
        let weights = graph
            .nodes
            .iter()
            .map(|n| match &n.op {
                Op::Conv { weights, bias, .. } => Some((g.upload(weights), g.upload(bias))),
                _ => None,
            })
            .collect();
        if let Some(f) = ctx::take_failure() {
            return Err(f.reason);
        }
        Ok(Self { graph, weights, buffers, kind, serial: std::sync::Mutex::new(()), gpu: g.clone() })
    }
    /// Whether the weights' owning context remains healthy. Normal cache release does not
    /// invalidate a retained device lease; actual loss/strikes do.
    pub fn is_current(&self) -> bool {
        self.gpu.health.is_ok()
    }

    /// Raw graph output, CHW. Exposed for CPU/GPU equivalence tests.
    pub fn run_tile(&self, input: &[f32]) -> Result<Vec<f32>, String> {
        let _serial = self.serial.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if input.len() != self.graph.input.len() || !input.iter().all(|v| v.is_finite()) {
            return Err("invalid denoise tile".into());
        }
        if !crate::enabled() || !self.is_current() || lightcraft_sysmem::gpu::pressured() {
            return Err("GPU denoise needs a healthy context with free VRAM; retry after other GPU work finishes".into());
        }
        let g = &self.gpu;
        let epoch = lightcraft_sysmem::gpu::idle_epoch();
        g.health.resume_retry(std::time::Instant::now(), Some(epoch));
        if !g.health.can_render() {
            return Err("GPU denoise is waiting to retry".into());
        }
        g.health.tick();
        g.health.begin_operation(epoch);
        let errors_before = g.health.errors();
        let _scope = ctx::RenderScope::new(g);
        let _ = ctx::take_failure();
        let errors = ctx::ErrorScopes::push(g);
        g.queue.write_buffer(self.buffers[0].raw(), 0, bytemuck::cast_slice(input));
        let mut enc = g.encoder();
        for (i, n) in self.graph.nodes.iter().enumerate() {
            let a_id = n.inputs[0];
            let a_shape = if a_id == 0 { self.graph.input } else { self.graph.nodes[a_id - 1].shape };
            let Shape { c: ci, h: hi, w: wi } = a_shape;
            let Shape { c: co, h: ho, w: wo } = n.shape;
            let mut p = vec![ci as u32, hi as u32, wi as u32, co as u32, ho as u32, wo as u32, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0];
            let (name, groups) = match &n.op {
                Op::Conv { kernel, stride, pad, transpose, .. } => {
                    p[6] = kernel[0] as u32;
                    p[7] = kernel[1] as u32;
                    p[8] = stride[0] as u32;
                    p[9] = stride[1] as u32;
                    p[10] = pad[0] as u32;
                    p[11] = pad[1] as u32;
                    p[12] = u32::from(*transpose);
                    ("denoise_conv", [(ho * wo).div_ceil(16) as u32, co.div_ceil(16) as u32, 1])
                }
                Op::Activation { alpha, sigmoid } => {
                    p[13] = if *sigmoid { 2 } else { 1 };
                    p[14] = alpha.to_bits();
                    ("denoise_point", ctx::groups1(n.shape.len()))
                }
                Op::Pool { kernel, stride } => {
                    p[13] = 3;
                    p[6] = kernel[0] as u32;
                    p[7] = kernel[1] as u32;
                    p[8] = stride[0] as u32;
                    p[9] = stride[1] as u32;
                    ("denoise_point", ctx::groups1(n.shape.len()))
                }
                Op::Concat => {
                    p[13] = 4;
                    ("denoise_point", ctx::groups1(n.shape.len()))
                }
                Op::Shuffle { block, crd } => {
                    p[13] = 5;
                    p[15] = *block as u32;
                    p[16] = u32::from(*crd);
                    ("denoise_point", ctx::groups1(n.shape.len()))
                }
            };
            let w = self.weights[i].as_ref();
            g.run(
                &mut enc,
                name,
                &p,
                &[
                    Some(&self.buffers[a_id]),
                    n.inputs.get(1).map(|&b| &self.buffers[b]),
                    w.map(|w| &w.0),
                    w.map(|w| &w.1),
                    Some(&self.buffers[i + 1]),
                ],
                groups,
            );
        }
        let output_shape = if self.graph.output == 0 { self.graph.input } else { self.graph.nodes[self.graph.output - 1].shape };
        let out = g.finish_and_read(enc, &self.buffers[self.graph.output], output_shape.len());
        if let Some(f) = errors.pop().or_else(ctx::take_failure) {
            if matches!(f.kind, ctx::FailKind::Fatal | ctx::FailKind::OutOfMemory) {
                g.health.operation_failed(&f.reason, epoch);
            }
            return Err(f.reason);
        }
        if !g.health.is_ok() || g.health.errors() != errors_before {
            return Err(g.health.fault().map_or_else(|| g.health.last_error().unwrap_or_default(), |fault| fault.to_string()));
        }
        if !out.iter().all(|v: &f32| v.is_finite()) {
            return Err("GPU denoise produced non-finite output".into());
        }
        g.health.operation_succeeded();
        Ok(out)
    }
    pub fn denoise(&self, pixels: &[f32], w: usize, h: usize, progress: &(dyn Fn(usize, usize) -> bool + Sync)) -> Result<Vec<[f32; 3]>, String> {
        let kind = self.kind;
        let mut out = run_tiled_channels(
            pixels,
            kind.channels(),
            w,
            h,
            self.graph.input.w,
            kind.scale(),
            |t| self.run_tile(t).map_err(|e| std::io::Error::other(e).into()),
            progress,
        )
        .map_err(|e| format!("{e:#}"))?;
        match_variant_gain(&mut out, pixels, kind, w * kind.scale());
        Ok(out)
    }
}

#[cfg(test)]
mod recovery_tests {
    #[test]
    #[ignore = "real GPU denoise transient recovery; run with LC_REQUIRE_GPU=1"]
    fn gpu_denoiser_resumes_its_own_health_after_transient_backoff() {
        use super::*;
        use li_seg::restoration_graph::Node;
        let _gpu = crate::test_device().expect("real GPU required");
        let shape = Shape { c: 3, h: 16, w: 16 };
        let graph =
            Graph { input: shape, nodes: vec![Node { inputs: vec![0], shape, op: Op::Activation { alpha: 0.0, sigmoid: false } }], output: 1 };
        let d = Denoiser::from_graph(graph, Variant::Rgb).unwrap();
        let input = vec![0.3; shape.len()];
        assert_eq!(d.run_tile(&input).unwrap(), input);
        d.gpu.health.operation_failed("injected transient operation error", lightcraft_sysmem::gpu::idle_epoch());
        assert!(d.run_tile(&input).is_err());
        std::thread::sleep(std::time::Duration::from_millis(4100));
        assert_eq!(d.run_tile(&input).unwrap(), input, "cached denoiser retries without needing a Develop render first");
    }
}
