//! Frozen pre-J18 Library histogram painter, test-only parity oracle.
#![allow(clippy::needless_range_loop)] // Preserve the frozen painter verbatim.
use egui::epaint::{Mesh, Vertex};
use egui::{Color32, Pos2, Rect, Stroke, pos2};
pub fn paint(p: &egui::Painter, plot: Rect, channels: &[[u32; 256]; 3], luma: &[u32]) {
    let smooth = |v: &[u32]| -> Vec<f32> {
        (0..256)
            .map(|i| {
                let (a, b) = (i.max(2) - 2, (i + 3).min(256));
                v[a..b].iter().map(|x| *x as f32).sum::<f32>() / (b - a) as f32
            })
            .collect()
    };
    let chans = [
        (smooth(&channels[0]), Color32::from_rgb(0xdf, 0x39, 0x39)),
        (smooth(&channels[1]), Color32::from_rgb(0x44, 0xb0, 0x72)),
        (smooth(&channels[2]), Color32::from_rgb(0x3b, 0x6f, 0xe0)),
    ];
    let peak = chans.iter().flat_map(|(v, _)| v.iter().skip(2).take(252)).fold(1.0f32, |a, b| a.max(*b));
    let to = |i: usize, v: f32| pos2(plot.left() + plot.width() * i as f32 / 255.0, plot.bottom() - plot.height() * (v / peak).sqrt().min(1.0));
    // luminance fill
    let lum = smooth(luma);
    let mut mesh = Mesh::default();
    for i in 0..256 {
        let top = to(i, lum[i]);
        mesh.vertices.push(Vertex { pos: top, uv: Pos2::ZERO, color: Color32::from_gray(120).gamma_multiply(0.35) });
        mesh.vertices.push(Vertex { pos: pos2(top.x, plot.bottom()), uv: Pos2::ZERO, color: Color32::from_gray(120).gamma_multiply(0.35) });
        if i > 0 {
            let k = (i as u32) * 2;
            mesh.indices.extend([k - 2, k - 1, k, k - 1, k + 1, k]);
        }
    }
    p.add(mesh);
    // greyscale images: the three channels coincide — draw one neutral line
    let spread: f32 = (0..256).map(|i| (chans[0].0[i] - chans[1].0[i]).abs() + (chans[1].0[i] - chans[2].0[i]).abs()).sum();
    let total: f32 = chans[1].0.iter().sum::<f32>().max(1.0);
    if spread / total < 0.02 {
        let pts: Vec<Pos2> = (0..256).map(|i| to(i, chans[1].0[i])).collect();
        p.add(egui::Shape::line(pts, Stroke::new(1.5, Color32::from_gray(200))));
    } else {
        for (v, c) in &chans {
            let pts: Vec<Pos2> = (0..256).map(|i| to(i, v[i])).collect();
            p.add(egui::Shape::line(pts, Stroke::new(1.5, *c)));
        }
    }
}
