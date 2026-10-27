# Architecture decision records

This folder holds the architecture decision records (ADRs) for Local Image. An ADR records one
significant decision, the context in which it was made, the consequences that were accepted, and
the conditions under which it should be revisited.

## Index

| Number | Title | Status |
|---|---|---|
| [0001](0001-journal-catalog.md) | Library catalog: in-memory maps with a JSON snapshot and an operation log | Accepted (recorded retroactively) |
| [0002](0002-output-gamut-clipping.md) | Output gamut clipping and scene compression | Superseded by 0003 |
| [0003](0003-blue-band-output-clipping.md) | Blue-band output clipping | Accepted |
| [0004](0004-rendered-source-gamut-identity.md) | Rendered-source gamut identity | Accepted |
| [0005](0005-template-native-ai-runtime.md) | Official ComfyUI templates are the recipe for models without a builder | Accepted |

## Conventions

- File names are `NNNN-short-title.md`, numbered in the order they are written.
- Each record has a status: Proposed, Accepted, Superseded (with a link to the replacement), or
  Deprecated.
- An accepted record is not rewritten to change the decision. To change a decision, write a new
  record that supersedes it and update the status of the old one.
- Records describe the decision and its reasons at the time. Facts about the code belong in the
  code and its tests, and the records link to them only where that helps a reader.
