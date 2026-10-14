use tract_onnx::prelude::*;
fn main() -> anyhow::Result<()> {
    li_seg::init_executor();
    let p = std::env::args().nth(1).unwrap();
    let start = std::time::Instant::now();
    let model = tract_onnx::onnx().model_for_path(p)?;
    println!("inputs={:?} output={:?}", model.input_outlets()?, model.output_fact(0)?);
    let plan = model.into_optimized()?.into_runnable()?;
    println!("load_ms={}", start.elapsed().as_millis());
    let start = std::time::Instant::now();
    let input = std::env::args().nth(2).map(image::open).transpose()?.map(|im| im.to_rgb8());
    let image = tract_ndarray::Array4::<f32>::from_shape_fn((1, 3, 512, 512), |(_, c, y, x)| {
        input.as_ref().map_or(0.5, |rgb| rgb.get_pixel(x as u32, y as u32)[c] as f32 / 255.0)
    });
    let mask = tract_ndarray::Array4::<f32>::from_shape_fn(
        (1, 1, 512, 512),
        |(_, _, y, x)| if (200..300).contains(&x) && (200..300).contains(&y) { 1.0 } else { 0.0 },
    );
    let out = plan.run(tvec!(image.into_tensor().into(), mask.into_tensor().into()))?;
    println!("run_ms={} output={:?}", start.elapsed().as_millis(), out[0].shape());
    if let Some(path) = std::env::args().nth(3) {
        let values = out[0].to_plain_array_view::<f32>()?;
        std::fs::write(path, values.iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<u8>>())?;
    }
    Ok(())
}
