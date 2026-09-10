# Network mounts (rclone)

List/stat prefer a long-lived **`rclone rcd`** on localhost (HTTP
keep-alive). Copy/move/`sync` still spawn the rclone CLI.

## Prerequisites

`rclone` on `PATH` or `RCLONE_BIN`. OAuth clouds need a remote already
created with `rclone config` — no in-app wizard.

## `config.toml`

```toml
[[file-manager.network-mounts]]
name = "Home SFTP"
uri = "sftp://myserver/home/alice"
user = "alice"
rclone-remote = "myserver"
enabled = true
```

Prefer `rclone-remote`. Inline `password` may appear on the rclone command
line. Runtime bookmarks: `data\network-bookmarks.toml`.

Schemes: sftp, scp, smb, ftp, ftps, webdav, s3, plus OAuth remotes when
named. `RCLONE_BIN` selects the binary; a compromised session can replace
it.
