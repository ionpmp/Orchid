# Mail

The **Mail** widget (`mail`) is an IMAP/SMTP client on the workspace.

## Add an account

1. Add **Mail** from the widget catalog.
2. Enter your email address and tap **Find servers**.
3. Orchid fills IMAP/SMTP hosts from built-in profiles, Mozilla ISPDB,
   DNS SRV, or common hostnames. Edit the fields if needed.
4. Enter a password (or app password) and tap **Save & connect**, or use
   **Sign in with browser** when the provider requires OAuth (Gmail /
   Microsoft 365).

Passwords and OAuth refresh tokens are stored under `data/mail/` in a
DPAPI-protected blob. Account metadata is in `data/mail/accounts.json`.
Message headers and bodies are cached in `data/mail/cache.db`.

For Google OAuth set `ORCHID_MAIL_GOOGLE_CLIENT_ID`. Microsoft OAuth uses
a public client id that you can override with `ORCHID_MAIL_MS_CLIENT_ID`.

## Reading and sending

The three panes show folders, the message list, and the reading pane.
Plain-text bodies render in Slint. HTML bodies use WebView2 with scripts
disabled and remote images off until you allow them.

Toolbar actions: reply, reply all, forward, flag, read/unread, delete
(move to Trash), and compose. Compose can send via SMTP or save a draft
on the IMAP Drafts folder.

Accounts are shared across Mail widgets. Each widget remembers the
selected account and folder.

## Limits

- Implicit TLS IMAP (port 993) is the supported receive path in this
  release. STARTTLS IMAP is not wired yet.
- Search, conversation threading, IMAP IDLE, rules, and Microsoft Graph
  are not included.
- Calendar and contacts stay separate from Mail.
