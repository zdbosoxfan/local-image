# OpenCV face algorithms and OpenCV Zoo models

The independent Rust YuNet decoding and SFace alignment implementation follows the equations
in OpenCV `modules/objdetect/src/face_detect.cpp` at
`52100328d82d0502534323e9524a701baa3a1e2a` and
`modules/objdetect/src/face_recognize.cpp` at
`13c571a801ad5c67a752e5cd58a8a7e7725f99d2`.
OpenCV is licensed under Apache-2.0; a copy is in `lightcraft-LICENSE-APACHE`.

YuNet (Shiqi Yu / OpenCV Zoo) weights: MIT.
SFace (OpenCV Zoo) weights: Apache-2.0.
These optional weights are downloaded separately, not distributed in the app.
Pinned source URLs, SHA-256 and byte counts are in `li_seg::MODELS` and
`docs/specs/smart-sort.md`. No face data is sent to these sources.
