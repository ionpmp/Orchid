# orchid-fs

Filesystem layer for Orchid. Exposes a pluggable provider abstraction (with a working `LocalProvider`), a cross-provider `FileWatcher` that fans notify events into the Orchid event bus, tagging via `orchid-storage`, archive browsing (ZIP / 7z / TAR / TAR.GZ / TAR.XZ), high-level file operations (copy / move / delete / recycle-bin), and two domain engines: managed (content-addressed dedup) and encrypted (`age` + reveal sessions) folders.

Network listing uses a long-lived `rclone rcd` on localhost when available,
falling back to per-operation CLI. Transfers still spawn the CLI. **Connect
cloud…** runs rclone's browser flow for Google Drive, personal OneDrive, and
Dropbox, then bookmarks the remote name. The token stays in rclone's config.

## Managed folders

Tracked files are recorded in the content-addressed `ChunkStore`. The chunk file is a block clone of the source range when the volume allows it, and a normal copy otherwise. Two whole files that still hash the same become one hard link, so an in-place edit changes every name. A save that replaces the file by rename breaks that link.

## Security posture

Encrypted-path records persist only the `IdentityKind` (passphrase / X25519). The user's actual secret material never lives in redb; it is supplied fresh at every `reveal()` call and held only in memory for the duration of the operation.
