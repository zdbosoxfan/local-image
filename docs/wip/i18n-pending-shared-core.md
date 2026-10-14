# Shared core diagnostics pending translation

Translation remains deferred, as directed by CODEX-TASK.md. These engine diagnostics are surfaced
by the existing import-warning UI; the shared i18n follow-up should give them structured keys
and substitute the format, reason and dimensions (rather than translating concatenated errors):

- `{format}: {reason}; opened the camera's embedded {width}x{height} JPEG preview instead (8-bit, not the raw sensor data)`
- `{format}: no embedded preview`
- `camera raw files (such as CR2, NEF, ARW or DNG) are not flat images; decode them with the shared Library/Develop raw pipeline`
- `JPEG metadata larger than its APP segment limit`
- `AVIF needs integer samples`
- `{format} developed by LightCraft's pipeline (camera defaults, as-shot white balance) into 16-bit ProPhoto RGB` (existing diagnostic, retained)

No new UI controls or labels were introduced by this slice.
