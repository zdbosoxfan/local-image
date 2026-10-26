use std::sync::Arc;

use li_raster::RgbFrame;
use lightcraft_develop::{DevelopSettings, LocalAdjustments, Mask, MaskComponent, MaskOp, MaskShape};
use lightcraft_pipeline::{RenderRequest, SourceInfo};

#[test]
fn oversized_color_range_matches_the_bounded_cpu_and_gpu() {
    let _gpu = crate::support::lock();
    assert!(lightcraft_gpu::available(), "real GPU required: {:?}", lightcraft_gpu::unavailable_reason());
    eprintln!("adapter: {}", lightcraft_gpu::adapter_name().unwrap());
    let src = Arc::new(RgbFrame::from_fn(32, 24, |_, _| [0.3, 0.2, 0.1]));
    let info = SourceInfo::default();
    let req = RenderRequest::fit(32, 24);
    let mut settings = DevelopSettings::default();
    settings.masks.push(Mask {
        adjust: LocalAdjustments { exposure: 1.0, ..Default::default() },
        components: vec![MaskComponent {
            name: None,
            op: MaskOp::Add,
            invert: false,
            shape: MaskShape::ColorRange { samples: vec![[10.0, 10.0, 10.0]; 64], refine: 50.0 },
        }],
        ..Default::default()
    });
    let bounded_cpu = lightcraft_pipeline::render(&src, &info, &settings, &req).image;
    let bounded_gpu = lightcraft_gpu::render(&src, &info, &settings, &req, None).expect("bounded GPU render").image;
    let matching = lightcraft_pipeline::color_range_sample(&src, &info, &settings, &req, lightcraft_geom::Point::new(0.5, 0.5)).unwrap();
    let MaskShape::ColorRange { samples, .. } = &mut settings.masks[0].components[0].shape else { panic!("color range") };
    samples.extend(vec![matching; 1000]);
    let oversized_cpu = lightcraft_pipeline::render(&src, &info, &settings, &req).image;
    let oversized_gpu = lightcraft_gpu::render(&src, &info, &settings, &req, None).expect("oversized GPU render").image;
    assert_eq!(oversized_cpu.data, bounded_cpu.data);
    assert_eq!(oversized_gpu.data, bounded_gpu.data);
    let MaskShape::ColorRange { samples, .. } = &mut settings.masks[0].components[0].shape else { panic!("color range") };
    samples[0] = matching;
    let selected_cpu = lightcraft_pipeline::render(&src, &info, &settings, &req).image;
    let selected_gpu = lightcraft_gpu::render(&src, &info, &settings, &req, None).expect("selected GPU render").image;
    assert_ne!(selected_cpu.data, bounded_cpu.data);
    assert_ne!(selected_gpu.data, bounded_gpu.data);
    for (cpu, gpu) in [(&oversized_cpu, &oversized_gpu), (&selected_cpu, &selected_gpu)] {
        let mut total = 0u64;
        let mut maximum = 0u8;
        for (cpu, gpu) in cpu.data.iter().zip(&gpu.data) {
            for channel in 0..3 {
                let difference = cpu[channel].abs_diff(gpu[channel]);
                total += u64::from(difference);
                maximum = maximum.max(difference);
            }
        }
        let mean = total as f64 / (cpu.data.len() * 3) as f64;
        eprintln!("color range: mean {mean:.4} LSB, max {maximum} LSB");
        assert!(mean < 0.5 && maximum <= 3, "mean {mean}, max {maximum}");
    }
}
