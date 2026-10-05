# Security operations

Full policy: [docs/SECURITY.md](../SECURITY.md).

- **Vault:** `passwords.kdbx`; Hello sidecar is not portable across Windows
  accounts. Auto-lock and clipboard clear via `[privacy]`.
- **age:** encrypt/decrypt/reveal from FM. Reveal wipe is best-effort, not
  SSD-secure deletion. Use BitLocker for disk-at-rest.
- **`.orchid`:** per-region age; sealed saves may sign C2PA over Clean-Text
  (ephemeral signer). Passphrase prompt on encrypted open; identity kept
  for re-encrypt on save.
- **Chunks:** plaintext CAS. Do not treat as a vault.
- **Mail:** passwords and OAuth refresh tokens are `data\mail\secrets.dpapi`.
  `accounts.json` has hosts and usernames only. The SQLite cache holds
  message bodies and attachment bytes in the clear.
- **Agent:** `[agent].api-key` is DPAPI-wrapped after save. The transcript
  `data\agent-chat.json` is local. Enabling the agent sends prompts to the
  configured endpoint.
- **Calendar / contacts:** the collection password is DPAPI-wrapped in that
  widget's config inside `state.redb`.
- **Mounts:** prefer `rclone-remote`. An inline password is stored as
  `dpapi:<hex>` and can still appear on the rclone command line.
- **Terminal Custom / SSH extra args** = a shell.
- **Policy:** `policy.toml` locks Settings fields. It does not rewrite
  `config.toml`. `audit.log` records policy apply, update checks, and shell
  changes, and is not uploaded.
- **Shell:** `[shell].replace` changes only this user's HKCU Winlogon
  `Shell`. `orchid.exe --restore-shell` writes the previous value back.
- Update checks read the public GitHub releases API and do not install
  binaries. Telemetry is opt-in, off by default, and stays in
  `data\telemetry.jsonl` unless `telemetry-endpoint` is https. Report vulns via GitHub Security
  Advisories, not public issues.
