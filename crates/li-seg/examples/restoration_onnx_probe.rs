use tract_onnx::prelude::*;
fn main() -> anyhow::Result<()> {
    let p = std::env::args().nth(1).ok_or_else(|| anyhow::anyhow!("ONNX model path required"))?;
    let c: usize = std::env::args().nth(2).unwrap_or("3".into()).parse()?;
    let h: usize = std::env::args().nth(3).unwrap_or("512".into()).parse()?;
    let len = c
        .checked_mul(h)
        .and_then(|n| n.checked_mul(h))
        .filter(|n| *n > 0 && *n <= 16_777_216)
        .ok_or_else(|| anyhow::anyhow!("invalid or excessive probe dimensions"))?;
    let start = std::time::Instant::now();
    let plan = tract_onnx::onnx()
        .model_for_path(&p)?
        .with_input_fact(0, InferenceFact::dt_shape(f32::datum_type(), tvec!(1, c, h, h)))?
        .into_optimized()?
        .into_runnable()?;
    println!("load_ms={}", start.elapsed().as_millis());
    let data: Vec<f32> = if let Some(input) = std::env::args().nth(4) {
        let bytes = std::fs::read(input)?;
        let (words, remainder) = bytes.as_chunks::<4>();
        anyhow::ensure!(remainder.is_empty(), "truncated FP32 reference input");
        words.iter().map(|b| f32::from_le_bytes(*b)).collect()
    } else {
        vec![0.5; len]
    };
    let start = std::time::Instant::now();
    let out = plan.run(tvec!(tract_ndarray::Array4::from_shape_vec((1, c, h, h), data)?.into_tensor().into()))?;
    let array = out.first().ok_or_else(|| anyhow::anyhow!("model has no output"))?.to_plain_array_view::<f32>()?;
    println!("run_ms={} shape={:?}", start.elapsed().as_millis(), array.shape());
    if let Some(output) = std::env::args().nth(5) {
        let bytes: Vec<u8> = array.iter().flat_map(|v| v.to_le_bytes()).collect();
        std::fs::write(output, bytes)?;
    }
    Ok(())
}
