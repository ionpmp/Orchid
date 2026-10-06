# orchid-mail

IMAP/SMTP engine for the Mail widget. The desktop UI lives in
`orchid-widgets` (`builtin/mail`) and `orchid-ui`. This crate does not draw.

Product how-to: [`docs/user/mail.md`](../../docs/user/mail.md).

## Modules

| Module | Role |
|--------|------|
| `engine` | [`MailEngine`] facade the widget calls |
| `account` | Non-secret account records, folders, headers, compose |
| `store` | `data/mail/accounts.json` |
| `secrets` | `data/mail/secrets.dpapi` (passwords and OAuth tokens) |
| `cache` | SQLite `data/mail/cache.db` (folders, headers, bodies, attachment bytes) |
| `autodiscover` | Built-in profiles, Mozilla ISPDB, DNS SRV, hostname guesses |
| `imap` / `smtp` | Sync, IDLE, search, append, send |
| `oauth` | Gmail and Microsoft 365 browser sign-in |
| `tls` | Implicit TLS and STARTTLS. Cleartext login is refused |
| `thread` | Subject grouping after reply prefixes. Not IMAP THREAD |
| `mime_util` | HTML to text, and a script-stripped body for WebView2 |

`MailEngine::open` is constructed in `OrchidApp::bootstrap` and handed to
`mail::descriptor`. Accounts are shared by every Mail widget.

## Limits that are part of the crate

- Search is an IMAP `TEXT` query plus a filter over the cached folder.
- IDLE waits about 90 seconds, then syncs again when the server reports mail.
- Compose attachments are read at send or save-draft time (at most 10 files
  and 25 MB). Forward does not copy the original files. Bcc addresses are
  part of the compose message the widget sends.
- `move_message` copies one message into a destination folder and removes
  it from the source. The widget only offers folders the account already
  lists.
- Mark read stores `\Seen` on the UIDs in the open folder's cached list
  (at most 100) in one command. It does not walk the rest of the mailbox.
- Rules and Microsoft Graph are not implemented.
