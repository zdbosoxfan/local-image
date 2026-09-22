# Original backup provenance

Commit `2bf0c2b2f10aa14e2a8cf67d334db249778350a7` is the original private backup captured on September 21, 2026 (America/New_York). It contains 62 original files plus a README and manifest. Every copied file was checked against its source with SHA-256.

[BACKUP-MANIFEST.json](../BACKUP-MANIFEST.json) records the original local paths, sizes, and hashes. It is an inventory of that historical snapshot, not a manifest of the current installation. Its recorded development paths are not required to run the application.

The initial snapshot combined the installed backend, the latest `local-remove-v5` development source, and the installed desktop executable. The backend editor files and executable matched their development counterparts byte for byte.

Subsequent commits replace the old machine-specific launch/update scripts and duplicate source snapshots with the per-user installer. The old files remain recoverable from the initial Git commit; the original folders on the PC were not changed when preparing the source repository.

Credentials, local settings, personal photos, editable photo projects, browser profiles, caches, logs, virtual environments, and model downloads were not part of the original code backup. Save personal work as `.lremove` projects and back up those files separately.
