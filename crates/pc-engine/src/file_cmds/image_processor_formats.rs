//! Image Processor's per-format mode: one run writes JPEG, PSD and TIFF copies of every input,
//! each format with its own settings.

use super::*;

const CMD: &str = "file.scripts.imageProcessor";

fn bad(msg: impl Into<String>) -> EngineError {
    EngineError::BadParams { cmd: CMD.into(), msg: msg.into() }
}

struct ProcessorFormat {
    extension: &'static str,
    save: SaveOpts,
    fit: Option<Value>,
    to_srgb: bool,
}

fn bool_option(options: &Value, key: &str, default: bool) -> Result<bool> {
    match options.get(key) {
        None => Ok(default),
        Some(value) => value.as_bool().ok_or_else(|| bad(format!("{key} must be true or false"))),
    }
}

fn number_option(options: &Value, key: &str, default: f64, max: f64) -> Result<f64> {
    match options.get(key) {
        None => Ok(default),
        Some(value) => value.as_f64().filter(|n| (1.0..=max).contains(n)).ok_or_else(|| bad(format!("{key} must be a number from 1 to {max}"))),
    }
}

/// The settings of one format's section; `None` when the section turns the format off.
fn processor_format(key: &str, extension: &'static str, options: &Value) -> Result<Option<ProcessorFormat>> {
    if options.is_null() || options == &Value::Bool(false) {
        return Ok(None);
    }
    if !options.is_object() {
        return Err(bad(format!("{key} must be an object or false")));
    }
    if !bool_option(options, "enabled", true)? {
        return Ok(None);
    }
    let fit = if bool_option(options, "resize", false)? {
        let width = number_option(options, "width", 1.0, 30_000.0)?;
        let height = number_option(options, "height", 1.0, 30_000.0)?;
        Some(json!({"width": width, "height": height, "dontEnlarge": true}))
    } else {
        None
    };
    let (save, to_srgb) = match key {
        "jpeg" => (SaveOpts::from(Some(number_option(options, "quality", 5.0, 12.0)?)), bool_option(options, "convertToSrgb", false)?),
        "psd" => (SaveOpts::default(), false),
        _ => {
            let compression =
                if bool_option(options, "lzw", false)? { photocraft_codecs::TiffCompression::Lzw } else { photocraft_codecs::TiffCompression::None };
            (SaveOpts { tiff_layers: true, tiff_compression: Some(compression), ..Default::default() }, false)
        }
    };
    Ok(Some(ProcessorFormat { extension, save, fit, to_srgb }))
}

pub(super) fn run(p: &Value, inputs: &[crate::batch::Input], output: &str) -> Result<Value> {
    // Every section is checked before any format writes a file.
    let mut formats = Vec::new();
    for (key, extension) in [("jpeg", "jpg"), ("psd", "psd"), ("tiff", "tif")] {
        if let Some(options) = p.get(key)
            && let Some(format) = processor_format(key, extension, options)?
        {
            formats.push(format);
        }
    }
    if formats.is_empty() {
        return Err(bad("You must save to at least one file type."));
    }
    // One error list across the formats: the naming and error options apply with `format` only.
    let options = crate::batch::Options::plain("");
    let mut files = Vec::new();
    let mut errors = Vec::new();
    for format in formats {
        let mut result = process_files(inputs, output, format.extension, format.save, &options, None, &|scratch, out| {
            // A PSD or TIFF input in the output folder would otherwise be replaced by its own copy.
            let target = std::path::Path::new(out);
            if target.is_file() && inputs.iter().any(|input| li_fs::same_file(std::path::Path::new(&input.path), target)) {
                return Err(EngineError::Other(format!("output would replace a source file: {out}")));
            }
            process_image(scratch, format.to_srgb, format.fit.as_ref())?;
            Ok(false)
        })?;
        if let Some(Value::Array(written)) = result.get_mut("files").map(Value::take) {
            files.extend(written);
        }
        if let Some(Value::Array(failed)) = result.get_mut("errors").map(Value::take) {
            errors.extend(failed.into_iter().map(|mut failure| {
                failure["format"] = json!(format.extension);
                if let Some(Value::String(message)) = failure.get_mut("error") {
                    *message = format!("{}: {message}", format.extension);
                }
                failure
            }));
        }
    }
    Ok(json!({"files": files, "errors": errors}))
}
