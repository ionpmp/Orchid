# Security Policy

## Supported Versions

During the MVP/pre-alpha phase, only the latest release is supported.

| Version | Supported |
|---|---|
| 0.1.x | ✅ Yes |
| < 0.1 | ❌ No |

## Reporting a Vulnerability

**Do not open a public issue for security problems.**

Send a private vulnerability report through GitHub Security Advisories once the public repository URL is finalized. Until then, contact the maintainers privately (see repository metadata).

Please include:
- Description of the vulnerability
- Steps to reproduce
- Potential impact
- Orchid and Windows versions affected
- A suggested fix, if you have one

We will aim to respond within 72 hours.

## Areas of Particular Concern

- **Password Manager** (KDBX4 format, Windows Hello / DPAPI unlock)
- **File Encryption** (age)
- **Biometrics** (Windows Hello integration)
- **Network Clients** (SFTP, SMB, WebDAV, FTP via rclone)
- **Terminal backends** (`Custom` and SSH `extra_args` can spawn arbitrary processes)

## Network mount credentials

Inline `password` fields under `[file-manager.network-mounts]` are stored in **plaintext** in `config.toml`. When used, Orchid may pass them to rclone as `pass=` in the process **command line**, which is visible to other processes on the same machine.

**Preferred:** set `rclone-remote` to a remote defined in `rclone.conf` (or rclone's OS keychain integration) and leave `password` unset.

## Threat-model notes

- DPAPI / Windows Hello unlock protects secrets from *other users* on the machine, not from malware running as the same user.
- Content-addressed chunk storage stores chunk payloads in plaintext by design; encrypt at the managed/encrypted-folder layer when needed.
- `RCLONE_BIN` overrides which rclone binary is executed — treat a compromised environment as out of scope for mount isolation.
