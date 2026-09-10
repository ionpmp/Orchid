# File manager

Catalog **Files** (`file-manager`). Dual-pane, tabs, sidebar of virtual
folders.

## Views and navigation

Icons / List / Details / Gallery; dual pane; tabs; breadcrumbs; editable
address bar with autocomplete; **branch view** (`Ctrl+B`); **Alt+F1**
drives; **Ctrl+Shift+T** new tab; **Ctrl+Shift+Enter** other pane.

## Selection, clipboard, undo

Click / Ctrl / Shift / marquee. Copy/cut/paste with Explorer via `CF_HDROP`
(remote-only selections stay in-app). **Ctrl+Z** / **Ctrl+Y** undo copy,
move, rename, create, Recycle Bin delete (not overwrites or permanent
deletes).

## Orchid / Commander defaults

F5/F6 copy/move to other pane, F7 folder, Shift+F4 file, F8/Del recycle,
Shift+Del permanent, F2 rename, F3/F4 viewer/edit, Alt+Enter properties,
Alt+F7 Find. Other shortcut profiles remap some of these.

## Virtual folders

Recent, Starred, Tags, Search results, Recycle Bin, Categories, Network.

## Archives

Browse as `archive:` folders. Extract / create / test. Password, volumes,
and SFX use **7-Zip** when installed. Viewer preview: ZIP / 7z / TAR(+gz/xz).

## Encryption and managed folders

age encrypt / decrypt / **reveal**. Hello can store the FM passphrase.
Managed folders ingest into CAS **and keep the original file**. Policy
dialog for quota / excludes. Reflink ingest is not implemented.

## Network

`[file-manager.network-mounts]` plus `network-bookmarks.toml`. List/stat
use rclone RC when available; copies still use the CLI. Prefer
`rclone-remote`. See [admin/network.md](../admin/network.md).

## Find (`Alt+F7`)

Name/mask/regex, content grep, size/date/attrs, archives, Windows Search
then Tantivy, EXIF/GPS, save as `virtual:search`, BLAKE3 duplicates, large
files. Walks are capped.

## `.orchid`

**Wrap as .orchid** packs the selection (sealed, or **linked** inside a
managed folder with `ChunkStore`). Opens dispatch by Raw MIME: documents
stay in the editor; other kinds unwrap to a temp for the matching viewer.

Audio/video selections: **Play in Audio/Video Player** / **Add to queue**.
