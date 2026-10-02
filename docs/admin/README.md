# Orchid admin guide

Operator docs for the current pre-alpha binary. End-user how-to:
[user guide](../user/README.md). Planned work: [roadmap](../ROADMAP.md).

There is no enterprise control plane. Update checks use public GitHub
releases and do not install binaries. Telemetry is opt-in.

| Chapter | Contents |
|---------|----------|
| [Install](install.md) | Desktop install, portable zip, DLLs, `.orchid` association |
| [Configuration](configuration.md) | `config.toml` |
| [Data and operations](data-and-operations.md) | Paths, redb, search index, backups |
| [Network](network.md) | rclone RC + CLI, credentials |
| [Security](security.md) | Vault, encryption, chunks |
