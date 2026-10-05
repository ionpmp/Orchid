# Network mounts (rclone)

List/stat prefer a long-lived **`rclone rcd`** on localhost (HTTP
keep-alive). Copy/move/`sync` still spawn the rclone CLI.

## Prerequisites

`rclone` on `PATH` or `RCLONE_BIN`. **Connect cloud…** in Files runs
`rclone config create` for Google Drive (`scope drive`), personal OneDrive
(`drive_type personal`), and Dropbox, with `config_is_local true` so rclone
opens the browser. The token stays in rclone.conf. A bookmark stores only
the remote name. OneDrive for Business still needs `rclone config`.

## `config.toml`

```toml
[[file-manager.network-mounts]]
name = "Home SFTP"
uri = "sftp://myserver/home/alice"
user = "alice"
rclone-remote = "myserver"
enabled = true
```

Prefer `rclone-remote`. An inline `password` is stored as a DPAPI blob
(`dpapi:<hex>`) in `config.toml` and may still appear on the rclone command
line when the mount is used. Runtime bookmarks: `data\network-bookmarks.toml`.

Schemes: sftp, scp, smb, ftp, ftps, webdav, s3, plus OAuth remotes when
named. `RCLONE_BIN` selects the binary; a compromised session can replace
it.
