# Password manager

Catalog **Passwords**. Vault: `%APPDATA%\Orchid\Orchid\data\passwords.kdbx`
(KDBX4, Argon2id). Optional Windows Hello (`passwords.master.dpapi`).

Idle lock: `[privacy].vault-auto-lock-seconds` (default 300). Leader `l` /
`password lock`.

**In the widget:** search, copy password/username/TOTP (clipboard auto-clear),
**add** an entry (root group), lock.

**Not in the UI yet:** edit, group tree, standalone generate screen.
Hello/DPAPI protect against other Windows users, not same-user malware.
