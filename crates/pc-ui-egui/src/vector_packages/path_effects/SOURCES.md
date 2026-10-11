# UI 45 acceptance fixtures

Original Local Image tests, GPL-3.0-or-later. egui/egui_kittest0.36, cargo/rustc1.98.1.
Synthetic 128×96 F32 RGB document and a 70×50 rectangle. The package kittests really
click the menu/Preview/OK/Cancel, type numeric angle/radius inputs, press/drag/release
an Amount slider and reorder/delete/hide effect rows. They inject test storage and
commit explicitly; this is not production journal/native acceptance.
The first execution failed before these interactions at exact submenu-label lookup:
egui appends an arrow to the label. Submenu queries were corrected to the established
`get_by_label_contains` API and compiled/linted, without a second execution under the
owner's once-only test rule. Corrected interaction results are still pending integration.

The ignored full-app test uses the actual PhotocraftApp in the offscreen eframe/wgpu
harness with native storage and real menu/frame/canvas routing from 00. It requires
RTX 5090/Vulkan/discrete/Rgba32Float and checks actual canvas texture readback against
CPU premultiplied pixels <=1/255, live pointer Preview/Cancel/OK, Expand/Undo and one
journal entry. Missing production hooks fail. No desktop window is opened.
The final speed rules forbid running the GPU gate or release benchmarks here.
