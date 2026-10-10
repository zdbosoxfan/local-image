Microsoft Bringing Old Photos Back to Life, scratch detection only
================================================================
Copyright (c) Microsoft Corporation. MIT; full terms in
restoration-scratches-LICENSE.txt.

Source: https://github.com/microsoft/Bringing-Old-Photos-Back-to-Life
Revision: 33875eccf4ebcd3665cf38cc56f3a0ce563d3a9c
Weights: official v1.0 global_checkpoints.zip,
checkpoints/detection/FT_Epoch_latest.pt.

Local Image exports only the one-channel detection UNet to FP32 ONNX and ports
its image/mask contract to Rust. This is not Microsoft's global repair, face
restoration or dlib pipeline. Detected masks are reviewed and filled using Local
Image's existing AI Remove. See docs/RESTORE-MODELS.md for pins and export recipe.
