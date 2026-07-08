# orchid-fs

Filesystem layer for Orchid. Exposes a pluggable provider abstraction (with a working `LocalProvider`), a cross-provider `FileWatcher` that fans notify events into the Orchid event bus, tagging via `orchid-storage`, archive browsing (ZIP / 7z / TAR / TAR.GZ / TAR.XZ), high-level file operations (copy / move / delete / recycle-bin), and two domain engines: managed (content-addressed dedup) and encrypted (`age` + reveal sessions) folders.

## Managed-folder MVP trade-off

Managed folders mirror every tracked file into the content-addressed `ChunkStore` while leaving the original file on disk. This preserves compatibility with external tools (Explorer, editors, Git, backups) at the cost of redundant storage; real on-disk savings only kick in when the same content recurs across files. The reflink / NTFS-hardlink-based strategy that would eliminate the redundant copy is planned for v1.x.

## Security posture

Encrypted-path records persist only the `IdentityKind` (passphrase / X25519). The user's actual secret material never lives in redb; it is supplied fresh at every `reveal()` call and held only in memory for the duration of the operation.
