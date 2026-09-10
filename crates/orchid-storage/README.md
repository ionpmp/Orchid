# orchid-storage

Storage layer for Orchid. Owns two independent subsystems:

- **State store** — a typed wrapper around [`redb`](https://docs.rs/redb)
  holding history, widget instances, workspaces, file tags, session, and
  caches. Values use [`bincode_reloaded`](https://docs.rs/bincode_reloaded) 3.
  Schema version **2** (`WindowPlacement`); migrations in `state::migrations`.
- **Configuration** — a TOML file (`config.toml`) with `serde`-driven schema, atomic saves, and an optional async [`ConfigWatcher`] that hot-reloads the configuration via `notify-debouncer-full` and broadcasts updates over a `tokio::sync::broadcast` channel.

OS-appropriate filesystem locations for both live on [`OrchidPaths`], resolved via the [`directories`](https://docs.rs/directories) crate.

## Scope

Only storage primitives live here. Business logic that uses the tables — action-history pruning schedulers, widget lifecycle, cache eviction policies beyond simple age-based eviction — belongs in consuming crates (`orchid-widgets`, `orchid-fs`, `orchid-app`, ...).
