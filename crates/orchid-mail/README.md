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

## Engine behavior

- Search is an IMAP `TEXT` query plus a filter over the cached folder.
- `idle_folder` waits for the `Duration` the caller passes, then the
  widget syncs again when the server reports mail. The Mail widget uses
  about 90 seconds.
- `move_message` copies one message into a destination folder and removes
  it from the source.
- `set_seen_many` / `set_unseen_many` set or clear `\Seen` on the UIDs the
  caller lists, in one command. The widget passes the open folder's cached
  list (at most 100 headers).

## Limits the Mail widget adds

- Compose attachments are read at send or save-draft time (at most 10 files
  and 25 MB). Forward does not copy the original files. Bcc addresses are
  part of the compose message the widget sends.
- The widget only offers move destinations the account already lists.
- Rules and Microsoft Graph are not implemented.
