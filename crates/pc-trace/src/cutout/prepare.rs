//! Compatibility entry to the existing T1 frontend pending package 00 factoring.
//! No clustering, threshold, palette or fitter algorithm is copied or changed.
use crate::colorfit::ColorFitter;
use crate::frontend::{Frontend, Threshold};
use crate::{Error, Image, Params, Preset};
pub(super) fn segment(image: Image<'_>, params: &Params, cancel: &dyn Fn() -> bool) -> Result<crate::ir::Segmentation, Error> {
    if cancel() {
        return Err(Error::Cancelled);
    }
    let mut img = crate::vc::ColorImage { width: image.width as usize, height: image.height as usize, pixels: image.rgba.to_vec() };
    let mut has_alpha = false;
    for (i, px) in img.pixels.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        if i % 1024 == 0 && cancel() {
            return Err(Error::Cancelled);
        }
        if !params.use_alpha {
            px[3] = 255;
        }
        if params.ignore_color == Some([px[0], px[1], px[2]]) {
            px[3] = 0;
        }
        if px[3] < 128 {
            has_alpha = true;
            px[3] = 0;
        } else {
            px[3] = 255;
        }
    }
    if img.pixels.as_chunks::<4>().0.iter().all(|p| p[3] == 0) {
        return Ok(crate::ir::Segmentation::new(image.width, image.height));
    }
    let binary = matches!(params.preset, Preset::BlackWhite | Preset::Silhouette);
    let mut seg = if binary {
        if params.preset == Preset::Silhouette && has_alpha {
            for p in img.pixels.as_chunks_mut::<4>().0 {
                let v = if p[3] == 0 { 255 } else { 0 };
                p[..3].fill(v);
            }
        } else {
            for p in img.pixels.as_chunks_mut::<4>().0 {
                if p[3] == 0 {
                    p[..3].fill(255);
                }
            }
        }
        crate::frontend::BinaryFrontend {
            threshold: if params.adaptive {
                Threshold::Adaptive { window: params.adaptive_window, t: params.adaptive_sensitivity }
            } else {
                Threshold::Fixed(params.threshold)
            },
            diagonal: false,
            min_area: params.speckle,
        }
        .segment_cancellable(&img, &cancel)?
    } else if params.preset == Preset::Photo && !has_alpha {
        crate::frontend::WatershedFrontend { detail: params.watershed_detail, min_area: params.speckle }.segment_cancellable(&img, &cancel)?
    } else {
        crate::frontend::ColorClusterFrontend {
            color_precision_loss: params.color_precision_loss,
            layer_difference: params.layer_difference,
            good_min_area: params.speckle,
        }
        .segment_cancellable(&img, &cancel)?
    };
    if cancel() {
        return Err(Error::Cancelled);
    }
    if !params.palette.is_empty() {
        crate::colorfit::FixedPalette::new(params.palette.iter().map(|c| crate::vc::Color::new(c[0], c[1], c[2])).collect()).fit(&mut seg);
    } else if !binary {
        crate::colorfit::AutoQuantize { max_colors: params.max_colors }.fit(&mut seg);
    }
    Ok(seg)
}
