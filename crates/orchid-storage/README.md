# orchid-storage

Storage layer for Orchid. Owns two independent subsystems:

- **State store** — a typed wrapper around [`redb`](https://docs.rs/redb)
  holding history, widget instances, workspaces, file tags, session, and
  caches. Values use [`bincode_reloaded`](https://docs.rs/bincode_reloaded) 3.
  Schema version **2** (`WindowPlacement`); migrations in `state::migrations`.
- **Configuration** — a TOML file (`config.toml`) with `serde`-driven schema, atomic saves, and an optional async [`ConfigWatcher`] that hot-reloads the configuration via `notify-debouncer-full` and broadcasts updates over a `tokio::sync::broadcast` channel. Sections are `general`, `appearance`, `input`, `shortcuts`, `locale`, `privacy`, `file_manager`, `search`, `onboarding`, `terminal`, `agent`, `photos`, `shell`, and the policy URL (`policy` module, separate file). `policy.toml` beside the config file is a separate lock document (`policy` module); it does not rewrite values.

OS-appropriate filesystem locations for both live on [`OrchidPaths`], resolved via the [`directories`](https://docs.rs/directories) crate.

**Backup / diagnostics** — `write_backup_zip` and `write_support_bundle` build
zip archives for in-app export (`orc data export backup`,
`orc diagnostics export`). The backup includes config, `state.redb`, the
vault, chunks, and network bookmarks. It omits the search index, logs,
cache, `data/mail`, `agent-chat.json`, `photo-faces.json`, and
`telemetry.jsonl`. The support bundle redacts network-mount passwords and
never copies the vault.

## Scope

Only storage primitives live here. Business logic that uses the tables — action-history pruning schedulers, widget lifecycle, cache eviction policies beyond simple age-based eviction — belongs in consuming crates (`orchid-widgets`, `orchid-fs`, `orchid-app`, ...).
