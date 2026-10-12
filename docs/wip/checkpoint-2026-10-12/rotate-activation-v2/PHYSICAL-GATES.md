# Required physical UI acceptance

These12filters must each pass explicitly on the exact candidate. Hosted CI skips are scheduling only, never passes or waived coverage. Use the coordinator’s isolated profile/target/toolchain, with other GPU tests/builds/inference idle. Existing release-only benchmark keeps its prior schedule; never run every ignored test indiscriminately.

```sh
export LC_REQUIRE_GPU=1 LOCAL_IMAGE_REQUIRE_GPU=1 LIGHTCRAFT_GPU=1
export LIGHTCRAFT_GPU_BACKEND=vulkan WGPU_BACKEND=vulkan
export RUSTC_WRAPPER=/home/zdavidson/.local/share/local-image-dev/bin/li-rustc-gate

cargo +1.98.1 test --offline --frozen -p photocraft-ui-egui --lib \
  gpu_canvas::view_tests::scaled_full_app_rtx5090_parity_2_4_8x_hidpi_flip_and_warm_cache \
  -- --ignored --exact --nocapture --test-threads=1

cargo +1.98.1 test --offline --frozen -p photocraft-ui-egui --lib \
  gpu_canvas::view_tests::scaled_full_app_actual_pan_zoom_pointer_events \
  -- --ignored --exact --nocapture --test-threads=1

cargo +1.98.1 test --offline --frozen -p photocraft-ui-egui --lib \
  gpu_canvas::view_tests::scaled_full_app_node_drag_preview_release_and_cancel_rtx5090 \
  -- --ignored --exact --nocapture --test-threads=1

cargo +1.98.1 test --offline --frozen -p photocraft-ui-egui --lib \
  gpu_canvas::view_tests::scaled_full_app_display_p3_and_float_hdr_texture_rtx5090 \
  -- --ignored --exact --nocapture --test-threads=1

cargo +1.98.1 test --offline --frozen -p photocraft-ui-egui --lib \
  gpu_canvas::view_tests::scaled_compositor_reuses_pipelines_across_zoom_buckets \
  -- --ignored --exact --nocapture --test-threads=1

cargo +1.98.1 test --offline --frozen -p photocraft-ui-egui --lib \
  gpu_canvas::view_tests::scaled_compositor_reuses_pipelines_across_viewport_aprons \
  -- --ignored --exact --nocapture --test-threads=1

cargo +1.98.1 test --offline --frozen -p photocraft-ui-egui --lib \
  gpu_canvas::view_tests::scaled_projection_refusal_is_memoized_only_for_exact_source_and_view \
  -- --ignored --exact --nocapture --test-threads=1

cargo +1.98.1 test --offline --frozen -p photocraft-ui-egui --lib \
  gpu_canvas::view_tests::scaled_pipeline_refusal_keeps_correct_cpu_rendering_without_repeated_construction \
  -- --ignored --exact --nocapture --test-threads=1

cargo +1.98.1 test --offline --frozen -p photocraft-ui-egui --lib \
  gpu_canvas::view_tests::rotated_presentation_rtx5090_raster_proxy_and_live_move_keep_pixels_and_coverage \
  -- --ignored --exact --nocapture --test-threads=1

cargo +1.98.1 test --offline --frozen -p photocraft-ui-egui --lib \
  gpu_canvas::view_tests::rotated_scaled_projection_covers_inverse_viewport_corners \
  -- --ignored --exact --nocapture --test-threads=1

cargo +1.98.1 test --offline --frozen -p photocraft-ui-egui --lib \
  gpu_canvas::view_tests::rotated_shadow_rtx5090_preserves_interior_and_screen_down_light \
  -- --ignored --exact --nocapture --test-threads=1

cargo +1.98.1 test --offline --frozen -p photocraft-ui-egui --lib \
  gpu_canvas::view_tests::rotate_view_full_app_rtx5090_pointer_angle_pixels_and_reset \
  -- --ignored --exact --nocapture --test-threads=1
```

Every invocation must report exactly one passing test with RTX5090/Vulkan/DiscreteGpu admission intact. The shared harness additionally checks Rgba32Float/F16 support. Missing/wrong hardware fails; no successful adapter fallback is permitted.
