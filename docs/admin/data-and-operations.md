# Data layout and operations

Qualifier `com` / org `Orchid` / app `Orchid` (`directories` crate).

| Role | Typical path |
|------|----------------|
| Config | `%APPDATA%\Orchid\Orchid\config` |
| `config.toml` | `…\config\config.toml` |
| Themes / locale overlays | `…\config\themes\`, `…\config\locales\` |
| Data | `%APPDATA%\Orchid\Orchid\data` |
| `state.redb` | `…\data\state.redb` (schema **v2**) |
| Vault | `…\data\passwords.kdbx` + `passwords.master.dpapi` |
| Chunks | `…\data\chunks\` |
| Search index | `…\data\search_index\` |
| Network bookmarks | `…\data\network-bookmarks.toml` |
| Logs | `…\data\logs\` (Roaming `data_dir`, not LocalAppData) |
| Cache | `%LOCALAPPDATA%\Orchid\Orchid\cache` |
| Installed exe | `%LOCALAPPDATA%\Programs\Orchid` |

`workspaces` / `widgets` config dirs are reserved (live layout is in redb).

## Operations

- **Backup:** quit Orchid, copy `config\`, `state.redb`, vault files,
  `chunks\` if you use managed folders, `network-bookmarks.toml`,
  `search_index` if you want to skip recrawl. DPAPI sidecar is machine/user
  bound.
- **Rebuild search:** quit, delete `data\search_index`, restart.
- **Reset layout:** delete `state.redb` (config.toml survives). Deleting
  all of `data\` also drops the vault and chunks.
- Logs: `RUST_LOG` overrides (`orchid=info` default).

Chunks are **plaintext**. Encrypt live files or the volume if the disk is
untrusted.
