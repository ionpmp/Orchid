# Password manager

Catalog **Passwords**. Vault: `%APPDATA%\Orchid\Orchid\data\passwords.kdbx`
(KDBX4, Argon2id). Optional Windows Hello (`passwords.master.dpapi`).

Idle lock: `[privacy].vault-auto-lock-seconds` (default 300). Leader `l` /
`password lock`.

**In the widget:** unlock (master password or Windows Hello), search, group
chips, copy password/username/TOTP (clipboard auto-clear; failures show a
toast when the OS clipboard is unavailable), **add** / **edit** an entry
(group picker or new group name), **generate** a password (copy without
saving), lock.

Hello/DPAPI protect against other Windows users, not same-user malware.
