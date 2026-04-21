# orchid-storage

Storage layer for Orchid. Owns two independent subsystems:

- **State store** — a typed wrapper around [`redb`](https://docs.rs/redb) holding user settings, action history, widget instances, workspaces, file tags, session state, and caches. Values are encoded with [`bincode`](https://docs.rs/bincode) 2.x via a small `Value<T>` adapter, so any `Encode + Decode<()>` type can be stored without boilerplate. A lightweight migration engine in `state::migrations` advances on-disk schemas between versions on open.
- **Configuration** — a TOML file (`config.toml`) with `serde`-driven schema, atomic saves, and an optional async [`ConfigWatcher`] that hot-reloads the configuration via `notify-debouncer-full` and broadcasts updates over a `tokio::sync::broadcast` channel.

OS-appropriate filesystem locations for both live on [`OrchidPaths`], resolved via the [`directories`](https://docs.rs/directories) crate.

## Scope

Only storage primitives live here. Business logic that uses the tables — action-history pruning schedulers, widget lifecycle, cache eviction policies beyond simple age-based eviction — belongs in consuming crates (`orchid-widgets`, `orchid-fs`, `orchid-app`, ...).
