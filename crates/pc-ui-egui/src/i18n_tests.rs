//! Coverage of the editor catalog (`li-i18n` `catalogs/editor`) against this shell's own menus,
//! `tl!` literals and labels. Engine and catalog-format tests live in `li-i18n`.

use li_i18n::{Domain, Lang, catalog, fmt, tr, tr_ctx};

/// Every `@id` entry names a live command or a menu catalogue command.
#[test]
fn editor_catalog_ids_are_live_commands() {
    for l in Lang::ALL {
        for (ctx, src, _) in li_i18n::catalog::parse_entries(l.catalog_source(Domain::Editor)).0 {
            if ctx == "@id" {
                assert!(crate::menus::is_live(&src) || crate::menu_catalog::CATALOG.iter().any(|m| m.3 == src), "{}: unknown command id {src:?}", l.code());
            }
        }
    }
}

// Translations for AI Remove and
// Draft/Refinement are deferred. These English additions intentionally use runtime fallback.
// Remove entries as their translations land; all other labels remain required.
// Overwrite Original: English until the owner resumes translations.
const PENDING_OVERWRITE_TRANSLATIONS: &[&str] = &[
    "Save Over Original…",
    "Save Over Original and Return to Library…",
    "Overwrite Original",
    "Don't ask again",
    "This document has no Library original",
    "Raw files can't be overwritten; save a copy instead",
    "This format can't be overwritten; save a copy instead",
    "This PSD colour mode can't be overwritten; save a copy instead",
    "Animated, multi-page or damaged originals can't be overwritten; save a copy instead",
    "Couldn't keep the previous version in Trash: {error}",
    "Overwrite requires a recoverable system Trash",
    "This replaces the original file. The previous version will be kept in Trash so you can recover it.",
    "Overwrite {name}?",
    "Overwrote {path}; previous version kept in Trash",
    "Couldn't overwrite the original: {error}",
    "Overwrote {path}; updated {name} in the Library; previous version kept in Trash",
    "The Library shows a preview of your layers. Save a copy or overwrite the original to keep this edit.",
];

// Pet Eye strings of the editor's Red Eye tool (translations deferred).
const PENDING_LRC_UX_TRANSLATIONS: &[&str] = &["Library & Develop", "Pet Eye", "Add Catchlight", "Click a pet's glowing pupil"];

const PENDING_AI_TRANSLATIONS: &[&str] = &[
    "Needs a newer Local Image: {names}",
    "Not used (the previous copy stays): {names}",
    "Your own profiles stay in use: {names}",
    "Cloud limit reached: {used} of {cap} images used this session, and this run needs {images}. Raise or clear the limit in Local AI › Cloud.",
    "Generate with {provider}?",
    "Provider: {provider}",
    "Model: {model}",
    "Images: {count}",
    "Your prompt and any selected or reference pixels are uploaded to {provider} and billed to your own API key.",
    "{used} of {cap} cloud images used this session.",
    "Don't ask again this session",
    "Confirm before each cloud generation",
    "Session limit (images):",
    "0 means no limit",
    "Automatic",
    "Loading on-device AI Remove",
    "Removing on this computer",
    "LaMa · On-device",
    "AI Remove (On-device)",
    "On-device · LaMa",
    "On-device Tools…",
    "Download AI Remove in Preferences › AI › On-device Tools first.",
    "Repair brushed areas on this computer, without an AI server.",
    "Repairs brushed areas with LaMa on this computer. No AI server needed; photos stay on this device.",
    "Repairs brushed areas locally using the surrounding texture and structure. Runs on the CPU, including Macs; no GPU or AI server needed. Large areas use a resized context crop.",
    "Keep full speed on battery",
    "CPU estimate: about {min}–{max} s (first run may take longer)",
    "CPU estimate: about {min}–{max} s (based on recent runs)",
    "CPU estimate: about {min}–{max} min. This may take a while; cancellation remains available.",
    "Your selected restoration model. Requires a GPU.",
    "{name} · {size} download · GPU required · {licence}",
    "Removes noise from raw photos locally, using RawNIND UtNet2. Includes Bayer demosaic and linear Rec.2020 models. Requires a GPU.",
    "Raw denoising and Bayer demosaic require a GPU. Linear Rec.2020 variant for other sensors.",
    "Local denoising for JPEG, TIFF and edited RGB images. Requires a GPU.",
    "CPU estimate: about {min}–{max} min. Too slow for an interactive CPU job; use a GPU or a smaller input.",
    "CPU estimate: about {min}–{max} s. This may take a while; cancellation remains available.",
    "Estimated CPU analysis: {min}–{max} min. Runs in the background; you can cancel at any time.",
    "Estimated CPU analysis: {min}–{max} s. Runs in the background; you can cancel at any time.",
    "Otherwise use fewer compute threads and pause indexing, Smart Sort and background preview builds on battery.",
    "AI enlargement requires a GPU/NPU.",
    "Choose a GPU enlargement model in Settings › Local AI.",
    "The job is no longer in ComfyUI's queue or history. The server may have restarted; try again.",
    "Cannot read the ComfyUI queue: {error}",
    "Cannot read ComfyUI job history: {error}",
    "Read document layers failed unexpectedly: {error}",
    "Read clipboard image failed unexpectedly: {error}",
    "This refinement recipe does not support styles (LoRAs).",
    "This refinement recipe does not support styles (LoRAs). Remove the selected styles or choose another refinement model.",
    "The selected style is incompatible with this refinement model: {style}",
    "Clear styles",
    "Model inventory changed during refinement testing; retry.",
    "Model file changed during refinement testing; retry.",
    "Refinement recipe was not resolved.",
    "Install the selected SeedVR2 model and VAE, then refresh ComfyUI.",
    "Invalid enhancement choices file",
    "invalid convolution padding",
    "invalid output padding",
    "concat channels overflow",
    "invalid PixelShuffle block",
    "PixelShuffle height overflow",
    "PixelShuffle width overflow",
    "Invalid upscale output dimensions",
    "Restoration architecture inspection timed out",
    "Probe stdout reader failed",
    "Probe stderr reader failed",
    "Missing probe stdout",
    "Missing probe stderr",
    "This model shares files with another installed model; remove the shared bundle instead.",
    "AI Enhance…",
    "Faithful",
    "Upscale Faithful",
    "Upscale Rescue",
    "Generative",
    "Change…",
    "Use default",
    "Choose a fixed-tile ONNX RGB model with 2× or 4× output. It is tested before use.",
    "Enter installed SeedVR2-compatible model and VAE filenames.",
    "Use custom models",
    "Your own models, run on the GPU.",
    "Custom ONNX restoration models run on the CPU.",
    "Choose the custom model again in Settings › Local AI.",
    "Start ComfyUI to use the custom SeedVR2 models.",
    "Rescue",
    "Generative (SeedVR2)",
    "Enlarges clean photos while preserving their detail.",
    "Enlarges photos and cleans noise, JPEG artefacts and mild blur.",
    "Adds detail with SeedVR2. The result may change textures.",
    "Enlarges the image. The result opens as a new document.",
    "Download CPU model",
    "Download GPU model",
    "Download the CPU model first.",
    "Enlarges clean photos while preserving their detail. Fixed tiles with overlap blending; runs locally on the CPU.",
    "Enlarges photos and cleans noise, JPEG artefacts and mild blur. The download includes both scales; runs locally on the CPU.",
    "Fix JPEG artefacts",
    "Clean up scan",
    "Repair old photo",
    "No scratches detected.",
    "Restore model installed.",
    "Runs on this computer with the installed Restore model.",
    "Choose the approved ONNX model to install locally.",
    "Preview marks detected scratches in red. Review and adjust the mask in AI Remove, then click Remove to fill it.",
    "Colourise after repair",
    "Review scratches in AI Remove",
    "Import Restore model…",
    li_ai::restoration::byo::NOTICE,
    li_ai::restoration::byo::DATA_NOTICE,
    "Permissive models",
    "Bring your own model",
    "Bring your own model…",
    "Use models you installed in ComfyUI. Local Image does not download these models or install their nodes.",
    "Detect installed models",
    "No supported user-installed models detected.",
    "I have read the licence and understand the commercial-use restrictions",
    "Choose your installed StableSR-compatible diffusion checkpoint",
    "LED",
    "Zero-DCE",
    "Zero-DCE++",
    "StableSR",
    "CodeFormer",
    "PromptIR",
    "GFPGAN v1.4",
    "RestoreFormer++",
    li_ai::restoration::COLOURISE_UNAVAILABLE,
    li_ai::restoration::OLD_PHOTO_UNAVAILABLE,
    "Restoration",
    "Restore…",
    "Restore Photo",
    "Task",
    "Before",
    "After",
    "JPEG cleanup",
    "Scan cleanup",
    "Colourise",
    "Old-photo preset",
    "Before / after split",
    "The image changed. Preview restoration again.",
    "Preview restoration first.",
    "Restoration returned unexpected dimensions.",
    "Open an image to restore.",
    "Original resolution. Apply creates a new layer and hides the source layers.",
    "Preview time",
    "Preview restoration",
    "Apply as new layer",
    "Install restoration models…",
    "Restoration installs only vetted official weights with pinned size and SHA-256.",
    "Use RawNIND linear variant",
    "Linear Rec.2020 input (RawNIND)",
    "Custom Raw Denoise models use three-channel linear Rec.2020 input and output.",
    "Your selected restoration model, with CPU fallback.",
    "Selected model · {size} · licence from its provider",
    "{name} · {size} download · GPU with CPU fallback · {licence}",
    "Raw Denoise",
    "RGB Denoise",
    "Removes noise from raw photos locally, using RawNIND UtNet2. Includes Bayer demosaic and linear Rec.2020 models. Uses the GPU when available, with a CPU fallback.",
    "Removes noise from JPEG, TIFF and edited RGB images locally with NIND UNet.",
    "Raw denoising and Bayer demosaic on the GPU, with CPU fallback. Linear Rec.2020 variant for other sensors.",
    "Local denoising for JPEG, TIFF and edited RGB images. Uses the GPU with a CPU fallback.",
    "+",
    "Choose or generate an image.",
    "Compare with input",
    "Comparison",
    "Create a draft, then choose Draft result as the refinement input.",
    "Denoise",
    "Describe the draft…",
    "Draft",
    "Draft / Refinement",
    "Draft / Refinement…",
    "Draft preview",
    "Draft result",
    "Edit models — refine by reference",
    "Generate draft",
    "Grid: {multiple} px",
    "Image models — refine by denoise",
    "Input",
    "Keep painting",
    "Low variety by design: different seeds give similar layouts. Change the prompt (or raise Variety) for different compositions.",
    "Higher Variety can change compositions and loosen prompt details. Set to 0 to turn it off.",
    "Variety must be between 0 and 100.",
    "Variety",
    "Open as new document",
    "Open image (composite)",
    "Paint to mark removal; Alt/Option-drag subtracts",
    "Refine image",
    "Refinement",
    "Refinement preview",
    "Refinement uses the current pixels of the open image.",
    "Refines by denoise: lower keeps more of the image",
    "Refines by reference: the image guides the result",
    "Remove immediately on release",
    "Running…",
    "Select a layer first.",
    "Selected layer",
    "Wheel to zoom; drag to pan",
    "−",
    "Model input:",
    "Model output:",
    "Depth output:",
    "The model crashed while it was being tested.",
];

const PENDING_VECTOR_TRANSLATIONS: &[&str] = include!("i18n_pending_vector.rs");

const PENDING_MODEL_SETUP_TRANSLATIONS: &[&str] = include!("i18n_pending_model_setup.rs");

/// Languages that claim complete menus have an entry for every label and path segment.
#[test]
fn complete_languages_translate_every_menu_string() {
    let mut strings = std::collections::BTreeSet::new();
    for &(path, label, _, _) in crate::menu_catalog::CATALOG {
        strings.extend(path.iter().copied());
        strings.insert(label);
    }
    for &(_, label, path, _) in crate::menus::UI_COMMANDS {
        strings.extend(path.iter().copied());
        strings.insert(label);
    }
    for c in photocraft_engine::command_specs().iter().filter(|c| !c.menu.is_empty()) {
        strings.extend(c.menu.iter().copied());
        strings.insert(c.label);
    }
    strings.remove("---");
    for l in Lang::ALL.into_iter().filter(|l| l.complete_menus()) {
        let cat = catalog(l, Domain::Editor);
        let missing: Vec<_> = strings
            .iter()
            .filter(|s| {
                !PENDING_LRC_UX_TRANSLATIONS.contains(s)
                    && !PENDING_AI_TRANSLATIONS.contains(s)
                    && !PENDING_OVERWRITE_TRANSLATIONS.contains(s)
                    && !PENDING_VECTOR_TRANSLATIONS.contains(s)
                    && !PENDING_MODEL_SETUP_TRANSLATIONS.contains(s)
                    && cat.plain(s).is_none()
            })
            .collect();
        assert!(missing.is_empty(), "{}: untranslated menu strings: {missing:#?}", l.code());
    }
}

/// Every literal passed to `tl!` in the shell has an entry in each language that claims complete menus
/// (so a new label can't ship untranslated by accident). Literals that are deliberately shown as
/// they are (names, units) are listed in `KEEP_AS_IS`.
#[test]
fn every_tl_literal_is_translated() {
    // English until the next translation pass (GPU start-up recovery, October 2026).
    // English until translations resume (owner, 2026-10-09: language work paused).
    const KEEP_AS_IS: &[&str] = &[
        "Method",
        "Classic",
        // Approved AI Preferences controls remain English during the owner's translation pause.
        "Cancel {function} download",
        "Cancel LAN search",
        "Custom model selected",
        "Download the tools you need. Models and technical options are under Manage.",
        "Generative enhancement settings",
        "Generative models run in ComfyUI, locally or on your server.",
        "Installed only",
        "Manage",
        "Manage {function}",
        "Managed model storage: {size}",
        "Manage…",
        "Needs setup",
        "No models match. Connect to your server or show all models.",
        "Non-commercial licence",
        "On-device tools work without a ComfyUI server.",
        "Optional online models use your own API keys and provider accounts.",
        "Repair",
        "Search model families…",
        "Server offline",
        "Server selected. Click Connect or Apply to use it.",
        "Storage and GPU requirements",
        "Storage and cleanup",
        "This computer · CPU",
        "This computer · GPU required",
        "This computer · GPU with CPU fallback",
        "ready",
        "{action} {function}",
        "CPU",
        "GPU",
        "Denoise",
        "Refine denoise",
        "Refine steps",
        "Refine guidance",
        "ComfyUI denoise: 1 regenerates the image, lower values keep more of it",
        // Remote ComfyUI controls stay in English under the owner's translation pause.
        "Use a remote ComfyUI server",
        "Connect / Check Server",
        "Find on LAN",
        "On macOS, allow Local Image in System Settings > Privacy & Security > Local Network.",
        "Search nearby LAN addresses on port 8188 and the selected port. For other networks, enter the address manually.",
        "Searching LAN… {checked}/{total}",
        "Use this ComfyUI server",
        "No ComfyUI server found. Start ComfyUI with --listen 0.0.0.0 on the server and allow its port through the firewall, then search again.",
        "Install on server",
        "Install generative models on the remote ComfyUI server, then check the connection again.",
        "Models and generation run on the server; images and results transfer automatically. Use its LAN or Tailscale address, or 127.0.0.1 with an SSH tunnel.",
        "Start ComfyUI and install generative models on the server. On-device tools still run on this computer.",
        "Other installed models",
    ];
    // Translation of the new background
    // controls is deferred. Keep this exception scoped to their keys; remove it when translated.
    const PENDING_BACKGROUND_TRANSLATIONS: &[&str] = &[
        "Background blur amount",
        "Background colour",
        "Background removal options",
        "Colour",
        "IS-Net · CPU",
        "Install a Subject & Background model to use Standard.",
        "Keep…",
        "Output",
        "Remove the background using the chosen model and output",
        "Subject & Background model: {model}",
    ];
    // The notices history window's "warnings" count label (UI kit part 2) shipped without a
    // translation or a pending-list entry; English until translations resume.
    const PENDING_NOTICE_TRANSLATIONS: &[&str] = &["warnings"];
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut literals = std::collections::BTreeSet::new();
    let mut stack = vec![dir];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") && !path.ends_with("lib.rs") {
                let text = std::fs::read_to_string(&path).unwrap_or_default().replace("\r\n", "\n");
                // Test modules aside, scan every literal `tl!` call. Cut at the test *module*: a
                // `#[cfg(test)]` on a single item earlier in the file must not hide the rest.
                let code = text.split("#[cfg(test)]\nmod ").next().unwrap_or("");
                let mut rest = code;
                while let Some(at) = rest.find("tl!(\"") {
                    rest = &rest[at + 5..];
                    let mut end = 0;
                    let bytes = rest.as_bytes();
                    while end < bytes.len() && !(bytes[end] == b'"' && (end == 0 || bytes[end - 1] != b'\\')) {
                        end += 1;
                    }
                    let lit = rest.get(..end).unwrap_or("").replace("\\\"", "\"");
                    if rest.get(end + 1..end + 2) == Some(")") {
                        literals.insert(lit);
                    }
                }
            }
        }
    }
    assert!(literals.len() > 300, "scan found only {} literals", literals.len());
    for l in Lang::ALL.into_iter().filter(|l| l.complete_menus()) {
        let cat = catalog(l, Domain::Editor);
        let missing: Vec<_> = literals
            .iter()
            .filter(|s| {
                !KEEP_AS_IS.contains(&s.as_str())
                    && !PENDING_NOTICE_TRANSLATIONS.contains(&s.as_str())
                    && !PENDING_BACKGROUND_TRANSLATIONS.contains(&s.as_str())
                    && !PENDING_LRC_UX_TRANSLATIONS.contains(&s.as_str())
                    && !PENDING_AI_TRANSLATIONS.contains(&s.as_str())
                    && !PENDING_OVERWRITE_TRANSLATIONS.contains(&s.as_str())
                    && !PENDING_VECTOR_TRANSLATIONS.contains(&s.as_str())
                    && !PENDING_MODEL_SETUP_TRANSLATIONS.contains(&s.as_str())
                    && cat.plain(s).is_none()
            })
            .collect();
        assert!(missing.is_empty(), "{}: untranslated tl! strings: {missing:#?}", l.code());
    }
}

/// Section names are dynamic labels, so the literal scanner cannot cover them.
#[test]
fn brush_section_names_are_translated() {
    for lang in Lang::ALL.into_iter().filter(|l| l.complete_menus()) {
        for (name, _) in crate::brush_panel::SECTIONS {
            assert!(catalog(lang, Domain::Editor).plain(name).is_some(), "{} missing brush section: {name}", lang.code());
        }
    }
}

/// Blend mode names come from the colour crate; each must be translated.
#[test]
fn blend_mode_names_are_translated() {
    for l in Lang::ALL.into_iter().filter(|l| l.complete_menus()) {
        for m in std::iter::once(photocraft_color::BlendMode::PassThrough).chain(photocraft_color::BlendMode::LAYER_MODES) {
            assert!(catalog(l, Domain::Editor).plain(m.label()).is_some(), "{}: blend mode {:?}", l.code(), m.label());
        }
    }
}

/// Camera Raw includes dynamic colour-band labels and contextual labels that the generic
/// tl! scanner cannot see. Cover the partial catalog too, without claiming whole-app coverage.
#[test]
fn camera_raw_labels_are_translated_in_every_available_language() {
    let sources = [include_str!("camera_raw_ui.rs"), include_str!("camera_raw_scope_ui.rs")];
    let mut labels = std::collections::BTreeSet::new();
    for source in sources {
        let code = source.split("#[cfg(test)]").next().unwrap();
        for marker in ["tl!(\"", "row(ui, &mut dirty, \"", "row(ui, dirty, \"", "section(ui, \"", "wheel(ui, &mut dirty, \"", "=> \""] {
            for tail in code.split(marker).skip(1) {
                labels.insert(tail.split('"').next().unwrap());
            }
        }
    }
    let bands = sources[0].split("const BANDS:").nth(1).unwrap().split(" = ").nth(1).unwrap().split(';').next().unwrap();
    for band in bands.split('"').skip(1).step_by(2) {
        labels.insert(band);
    }
    assert!(labels.len() >= 66, "missing Camera Raw source labels: {labels:?}");
    for lang in Lang::ALL.into_iter().filter(|l| *l != Lang::En) {
        let catalog = li_i18n::catalog(lang, Domain::Editor);
        let missing: Vec<_> = labels.iter().filter(|s| catalog.contextual("cameraRaw", s).or_else(|| catalog.plain(s)).is_none()).collect();
        assert!(missing.is_empty(), "{}: Camera Raw labels: {missing:?}", lang.code());
        assert_ne!(tr_ctx(lang, "cameraRaw", "Highlights"), tr_ctx(lang, "cameraRaw", "Lights"), "{}: distinct curve regions", lang.code());
        assert_ne!(tr_ctx(lang, "cameraRaw", "Shadows"), tr_ctx(lang, "cameraRaw", "Darks"), "{}: distinct curve regions", lang.code());
    }
    let ru = Lang::from_code("ru").unwrap();
    assert_eq!(tr_ctx(ru, "cameraRaw", "Vibrance"), "Красочность");
    assert_eq!(tr_ctx(ru, "cameraRaw", "Aqua"), "Голубые");
    let text = fmt(tr(ru, "Camera Raw Filter ({layer})"), &[("layer", "{Background} 影像")]);
    assert_eq!(text, "Фильтр Camera Raw ({Background} 影像)", "user layer names are not translated");
}
