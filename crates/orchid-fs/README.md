# orchid-fs

Filesystem abstraction for Orchid. Defines the provider trait that the file manager operates against, with backends for the local filesystem and network mounts (SFTP / SMB / WebDAV / FTP), plus content-addressed chunked storage.

It leans on `orchid-crypto` for hashing and chunk boundaries and on `orchid-storage` for the chunk index and deduplication metadata. Directory change notifications are delivered via the `notify` crate.
