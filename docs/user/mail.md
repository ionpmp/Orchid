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

Passwords and OAuth refresh tokens are stored in
`data/mail/secrets.dpapi` (DPAPI). Account metadata, without secrets, is
`data/mail/accounts.json`. Message headers, bodies, and saved attachment
bytes are cached in `data/mail/cache.db`. The in-app backup zip does not
include this folder.

For Google OAuth set `ORCHID_MAIL_GOOGLE_CLIENT_ID`. Microsoft OAuth uses
a public client id that you can override with `ORCHID_MAIL_MS_CLIENT_ID`.

## Reading and sending

The three panes show folders, the message list, and the reading pane.
Plain-text bodies render in Slint. HTML bodies use WebView2 with scripts
disabled and remote images off until you allow them.

Toolbar actions: reply, reply all, forward, flag, read/unread, delete
(move to Trash), compose, **Mark read**, and **Mark unread**. Mark read sets
the seen flag on unread messages in the open folder's current list (the
latest 100 headers). Messages outside that list stay unread. Mark unread
clears that flag on messages in the same list that are already read.
Messages outside that list keep their flags. When a message is open, **Move to** lists
the account's other folders and moves that one message. Compose can send
via SMTP, including a Bcc line, or save a draft on the IMAP Drafts folder.

Accounts are shared across Mail widgets. Each widget remembers the
selected account and folder.

## Limits

- IMAP can use implicit TLS (usually port 993) or STARTTLS (usually port
  143). The password is sent only after the TLS handshake. Cleartext IMAP
  login is refused.
- The search box filters the cached folder immediately and also runs an
  IMAP `TEXT` search so matches outside the latest 100 headers can appear.
  A search string with a control character is rejected. Rules and
  Microsoft Graph are not included.
- Messages in one folder are grouped by subject after `Re:` / `Fwd:` (and
  a few translated prefixes) are removed. The newest message in a group is
  flush left; older ones are indented. This is not the IMAP THREAD command.
- The background refresh waits on IMAP IDLE for about 90 seconds. New mail
  starts another sync. Refresh in the toolbar still syncs immediately and
  does not wait on IDLE. A server without IDLE keeps the timed sync.
- Opening a message stores its attachment names and bytes in the local
  cache. The reading pane lists those names, and Save writes the cached
  bytes to a file you choose. There is no preview. A message cached before
  this version is downloaded again once.
- Compose can attach files from this computer. Attach lists the name and
  size. Send and Save draft read those files then. At most 10 files and
  25 MB. A few extensions (text, PDF, PNG, JPEG, GIF, WebP, ZIP, JSON)
  get a matching type; other files are sent as `application/octet-stream`.
  There is no preview and no drag-and-drop. Forward does not copy the
  original message's files. Bcc is a separate line. Those addresses are
  on the SMTP envelope, and the sent message also keeps a Bcc header.
  Reply and forward start with that line empty.
- Move sends one open message to a folder the account already lists. It
  does not move a whole selection, and it does not create a folder.
- Mail does not read or write the calendar or contacts widgets.
