# Pending English strings — J22 CMS soft proofing

Translations remain deferred. The controls use the existing translated shared widgets.

| Source | English string | Context |
|---|---|---|
| `lc-ui-egui/src/panels/edit.rs` | `ICC profile path` | Accessible name and hint for the custom proof profile field. |
| same | `Load ICC Profile` | Load and validate a local ICC profile. |
| same | `Relative Colorimetric` | Rendering intent. |
| same | `Perceptual` | Rendering intent. |
| same | `Saturation` | Rendering intent. |
| same | `Absolute Colorimetric` | Rendering intent. |
| same | `Black Point Compensation` | Match source and printer black points. |
| same | `Simulate Paper & Ink` | Absolute proof-to-monitor media-white/black simulation. |
| same | `Destination Gamut Warning` | Red warning control. |
| same | `Monitor Gamut Warning` | Blue warning control; both warnings produce pink. |
| `lc-engine/src/view.rs` | `Cannot open proof profile: {}` | Local file-open error. |
| same | `Cannot read proof profile: {}` | Local file-read error. |
| same | `Invalid proof profile: {}` | ICC validation error. |
| same | `Proof profile must be a regular file` | Reject non-file profile sources. |
| same | `Proof profile exceeds 16 MiB` | Bound profile loading memory. |
| same | `view.softProof: profilePath must be an ICC file path` | Invalid command input. |
| same | `view.softProof: unknown rendering intent` | Invalid command input. |
| same | `view.softProof: {} must be a boolean` | Invalid command input. |
| `lc-ui-egui/src/menus.rs` | `view.softProof: on must be a boolean` | Invalid command input. |
| `lc-engine/src/media.rs` | `Cannot soft-proof selected profiles: {}` | Report a link failure without substituting another printer or monitor. |
| `lc-pipeline/src/output.rs` | `proof requires a device or colour-space profile` | Reject abstract/device-link/named-color profiles. |
| same | `proof profile exceeds 16 MiB` | Bound serialized profile loading. |
| same | `proof monitor must be RGB` | Reject invalid transient display targets. |

Existing `Soft Proofing`, `Create Proof Copy`, `Proof Copy ({})`, `Proof Preview · {}` and
`Proof copy created` remain unchanged. ICC descriptions are supplied by profiles.
