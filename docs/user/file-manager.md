# File manager

Catalog **Files** (`file-manager`). Dual-pane, tabs, sidebar of virtual
folders.

## Views and navigation

Icons / List / Details / Gallery, chosen from the view menu. Dual pane;
tabs show the folder name (the full path is on the tooltip) and scroll
when they do not fit. The path is the address bar: click a segment to
jump, click the empty part to type, with autocomplete. The places list
(drives, starred, photos, and the other virtual folders) stays beside
the listing in both layouts. **Branch view** (`Ctrl+B`); **Alt+F1**
drives; **Ctrl+Shift+T** new tab; **Ctrl+Shift+Enter** other pane.
Sort is the menu next to the view button; in Details, click a column
header. Narrow panes keep name and size; modified and type appear when
the pane is wide enough. The column header stays put while the list scrolls.
A narrow toolbar keeps back, forward, up, the address, view, and sort.
Home, history, a new folder, and branch appear as the pane widens. The
quick filter is a button until the field fits beside the address.

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

The places list holds drives, then Recent, Starred, Tags, Photos, Search
results, Recycle Bin, and Network. **Categories** is a section, not one
folder: images, documents, video, audio, and archives
(`virtual:categories/…`). **Managed** lists each managed folder (and a
policy-only row when a policy exists without a matching folder).

## Photos

**Files → Photos** groups tags that contain a slash. `people/Ada` is a
person, `event/2026-10-02/Picnic` is an event, `album/Vacation` is an
album. Smart albums list every person, every event, and every other tag.
The names `people`, `events`, and `other` are reserved under Albums.
**People → Unnamed** holds pictures where Windows found a face and you
have not added a `people/Name` tag.

This finds faces. It does not name the person. Settings → Photos →
**Find faces in open folders** is off until you enable `[photos].detect-faces`.
Orchid then scans images in the folder you open, up to 24 per pass, and
writes rectangles to `data/photo-faces.json`. The image viewer draws
those rectangles on the open picture. Files larger than 40 MiB are
skipped. Builds that are not Windows, or a Windows edition without the
face detector, find nothing. Add a `people/Name` tag to move a picture
out of Unnamed.

Settings → Photos can also tag images from folder names: a picture in
`People/Ada` becomes `people/ada`, and a picture in
`Events/2026-10-02/Picnic` becomes `event/2026-10-02/picnic`. That switch
is off until you enable `[photos].auto-tag`. A file sitting directly in
`People` or `Events`, with no name folder under it, is left alone.

## Archives

Browse as `archive:` folders. Extract / create / test. Password, volumes,
and SFX use **7-Zip** when installed. Native browse covers ZIP, 7z, TAR,
TAR.GZ, TAR.XZ, and TAR.BZ2.

## Encryption and managed folders

age encrypt / decrypt / **reveal**. Hello can store the FM passphrase.
Managed folders ingest into the chunk store and keep the file readable.
On a volume that can share extents, the chunk is a block clone of the
source; otherwise the bytes are copied. Two whole files in that folder
with the same content become one hard link: an in-place edit changes
every name, and a save that renames a new file over the path breaks the
link. Policy dialog for quota / excludes.

## Network

`[file-manager.network-mounts]` plus `network-bookmarks.toml`. List/stat
use rclone RC when available; copies still use the CLI. Prefer
`rclone-remote`. See [admin/network.md](../admin/network.md).

**Connect cloud…** signs in to Google Drive, personal OneDrive, or Dropbox
in the browser through rclone. The token stays in rclone's config. Orchid
bookmarks the remote (`drive://Name/`, `onedrive://Name/`, `dropbox://Name/`).
OneDrive for Business is not part of this wizard.

## Find (`Alt+F7`)

Name/mask/regex, content grep, size/date/attrs, archives, Windows Search
then Tantivy, EXIF/GPS, save as `virtual:search`, BLAKE3 duplicates, large
files. Walks are capped. **Find duplicates** and **Find large** are also
on the Tools menu.

## Tools

The **Tools** menu (`fs.tools`) holds operations that are not `orc` verbs
and are not in the shortcut profile. A few of them (Find, properties)
also have profile bindings.

**Compare and sync.** With two panes, compare by name or by bytes, sync
this pane to the other, the other to this one, both ways, or merge into
the other pane. **Cloud sync** uses the same dual-pane sync against a
network place. Bookmark, connect, and cloud sign-in sit beside those
commands.

**Split and join.** Split one file. Join the selection back into one file.

**Checksums.** MD5, SHA-1, SHA-256, BLAKE3, and CRC-32 write a report for
the selection. Verify checks that selection against a checksum list.

**Encode.** One file can be written as Base64 or UUE, or decoded from
either.

**Names and times.** Change case to lower, upper, or title. Touch sets
the modified time to now, or to a time you type.

**Attributes.** Read-only, hidden, system, and archive can each be turned
on or off. **chmod** prompts for a mode. On Unix, **chown** prompts for
an owner. On Windows, view, grant, or reset an ACL.

**Links.** Create a symbolic link, a hard link, or (on Windows) a
junction.

**Windows only.** SMB shares: view, add, remove, or open the system share
tab. Previous versions: view, restore, copy out, or open the system
versions tab. BitLocker: report protection, lock, unlock with a key you
type, or open the system BitLocker panel.

**Metadata.** One file can show an EXIF report. The metadata submenu
edits fields, GPS, and dates (including a shift), strips all metadata or
only GPS, copies metadata between two files, exports or imports CSV,
exports XML, and saves or applies a template. One audio file shows an
ID3 report. One Office file shows document properties. One file shows a
signature report.

**Images.** The image submenu works on the selection: resize and canvas,
auto straighten, adjust, auto levels / contrast / color, gray, sepia,
invert, a named filter, sharpen, blur, despeckle, cartoon, sketch,
vignette, red-eye, save a look, annotate, text or image watermark, and a
stamp. Batch tools convert, rotate, build thumbnails, rename from a
template, preview / save / cancel a batch, compare, pick, diff, composite,
stitch a panorama, and merge an HDR set. Print, print preview, contact
sheet, N-up, and batch print. Export, email, share, copy, paste, set as
wallpaper, capture a screenshot, and write an ICO or a favicon.

## `.orchid`

**Wrap as .orchid** packs the selection (sealed, or **linked** inside a
managed folder with `ChunkStore`). Opens dispatch by Raw MIME: documents
stay in the editor; other kinds unwrap to a temp for the matching viewer.

Audio/video selections: **Play in Audio/Video Player** / **Add to queue**.
