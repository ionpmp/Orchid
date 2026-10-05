# Security Policy

## Supported Versions

During the MVP/pre-alpha phase, only the latest release is supported.

| Version | Supported |
|---|---|
| 0.1.x | ✅ Yes |
| < 0.1 | ❌ No |

## Reporting a Vulnerability

**Do not open a public issue for security problems.**

Send a private vulnerability report through
[GitHub Security Advisories](https://github.com/ionpmp/Orchid/security/advisories/new)
for [ionpmp/Orchid](https://github.com/ionpmp/Orchid).

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
- **Mail, CalDAV, CardDAV, and the agent** (passwords and API keys at rest)
- **Network Clients** (SFTP, SMB, WebDAV, FTP via rclone)
- **Terminal backends** (`Custom` and SSH `extra_args` can spawn arbitrary processes)
- **Sign-in shell** (`HKCU` Winlogon `Shell` for this user only)

## Secrets at rest

| Secret | Where it is stored | Notes |
|--------|--------------------|--------|
| Vault master | `passwords.kdbx`; optional `passwords.master.dpapi` | Argon2id. Hello sidecar is not portable |
| Mail password / OAuth refresh | `data/mail/secrets.dpapi` | Not in widget state or `accounts.json` |
| Agent API key | `[agent].api-key` as `dpapi:<hex>` after save | Blank field keeps the saved key |
| CalDAV / CardDAV password | Widget config inside `state.redb`, DPAPI-wrapped | Not shown again in the widget |
| Inline mount password | `[file-manager.network-mounts].password` as `dpapi:<hex>` | Still passed on the rclone argv at use time |
| Cloud OAuth token | rclone's own config | Connect cloud… does not copy the token into Orchid |

DPAPI protects those blobs from other Windows users and from an offline copy of the profile. It does not protect them from malware running as the same user.

## Network mount credentials

Prefer `rclone-remote` (credentials stay in `rclone.conf` or rclone's OS keychain) and leave `password` unset.

When an inline password is required, Orchid stores it as a Windows DPAPI blob in `config.toml`. At use time it may still appear as `pass=` on the rclone **command line**, which other processes on the machine can see.

## Threat-model notes

- DPAPI / Windows Hello unlock protects secrets from *other users* on the machine, not from malware running as the same user.
- Content-addressed chunk storage stores chunk payloads in plaintext by design; encrypt at the managed/encrypted-folder layer when needed.
- `RCLONE_BIN` overrides which rclone binary is executed — treat a compromised environment as out of scope for mount isolation.
- The agent can read a local text file, list one folder, and search the open index, and it can propose a file write. Nothing is written until the user confirms. There is no shell tool. The transcript in `agent-chat.json` stays on this computer. The question and any tool results are sent to the configured endpoint.
- Mail HTML is shown in WebView2 with scripts disabled. Remote images stay off until allowed.
- `policy.toml` and `audit.log` are local. An https policy URL is fetched at startup; a failed fetch leaves the previous file. The audit log is not uploaded.
- Telemetry is off by default. When on, it posts only an anonymous `app-start` JSON object, and only to an `https` URL. Redirects are not followed.
- Replacing the sign-in shell writes only the per-user `HKCU` Winlogon value. `orchid --restore-shell` puts the previous value back.

## Disk wipe after encryption / reveal

When Orchid encrypts a file in place or tears down a reveal session, it may
overwrite the plaintext with zeros before `unlink` (best-effort, size-capped).

**This is not a guarantee of physical erasure.** On NTFS with journaling, and
especially on SSDs with wear-leveling / TRIM, overwriting a file often writes
new blocks while the previous plaintext sectors remain until the drive
reclaims them. Treat overwrite-before-delete as defense-in-depth against
casual recovery tools, not as a substitute for full-disk encryption (BitLocker)
or media sanitization when the threat model requires cryptographic erasure.
