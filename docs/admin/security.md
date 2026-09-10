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
- **Terminal Custom / SSH extra args** = a shell.
- Auto-update / telemetry are **stubs**. Report vulns via GitHub Security
  Advisories, not public issues.
