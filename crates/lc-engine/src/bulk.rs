//! Small CPU worker pool for bulk preview/export work. Results keep input order. Pixel
//! work holds a weighted permit through rendering/encoding, rather than just file decoding.
use crate::media::SourceRef;

pub(crate) fn source_weight(source: &SourceRef) -> usize {
    let (file, edge) = match source {
        SourceRef::File { path, max_edge, .. } | SourceRef::RawFile { path, max_edge, .. } =>
            (std::fs::metadata(path).map_or(0, |m| m.len() as usize), *max_edge),
        SourceRef::Loaded(s) => return s.image.data.len().saturating_mul(96),
        SourceRef::Demo { max_edge, .. } => (0, *max_edge),
        SourceRef::Smart { path } => (std::fs::metadata(path).map_or(0, |m| m.len() as usize), 2560),
    };
    // Preview-size mosaics are binned by the loader. Full-size work is deliberately large
    // enough to run alone under the default budget; one oversized item can still proceed.
    if edge == usize::MAX { return file.max(1 << 20).saturating_mul(24).max(crate::memory::budget()); }
    let pixels = edge.saturating_mul(edge);
    file.saturating_mul(3).saturating_add(pixels.saturating_mul(32)).max(1 << 20)
}

pub(crate) fn map<T: Send, R: Send>(items: Vec<T>, work: impl Fn(T) -> R + Sync + Send) -> Vec<R> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let threads = std::thread::available_parallelism().map_or(1, |n| n.get()).min(4);
        if threads > 1 && items.len() > 1 {
            // Memory permits are acquired by these ordinary workers, outside Rayon. A Rayon
            // worker waiting for nested pixel work could otherwise steal another photo and
            // block on its permit while still holding the first photo's permit.
            return std::thread::scope(|scope| {
                let chunk_size = items.len().div_ceil(threads);
                let mut items = items.into_iter();
                let mut workers = Vec::new();
                for _ in 0..threads {
                    let chunk: Vec<_> = items.by_ref().take(chunk_size).collect();
                    if chunk.is_empty() { break; }
                    let work = &work;
                    workers.push(scope.spawn(move || chunk.into_iter().map(work).collect::<Vec<_>>()));
                }
                workers.into_iter().flat_map(|worker| match worker.join() {
                    Ok(results) => results,
                    Err(panic) => std::panic::resume_unwind(panic),
                }).collect()
            });
        }
    }
    items.into_iter().map(work).collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn results_keep_input_order_with_nested_pixel_work_and_one_memory_permit() {
        let gate = crate::memory::WorkGate::new(1);
        let actual = super::map((0..16).collect(), |n| {
            let _permit = gate.acquire(1);
            use rayon::prelude::*;
            (0..8).into_par_iter().map(|i| n * 8 + i).sum::<usize>()
        });
        let expected: Vec<_> = (0..16).map(|n| (0..8).map(|i| n * 8 + i).sum::<usize>()).collect();
        assert_eq!(actual, expected);
    }
}
