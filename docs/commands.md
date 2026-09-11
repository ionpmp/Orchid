# Commands and file-manager actions

Orchid has two layers that look similar in Settings and the palette.
They are **not** the same.

| Layer | Id shape | Typed as | Who runs it |
|-------|----------|----------|-------------|
| **Command** | `settings.open`, `widget.create` | `orc settings open` | `CommandRegistry` → `Action` |
| **FM action** | `fs.copy`, `viewer.open` | not an `orc` verb | File-manager widget + shortcut profiles |

The design goal is still “gesture = command = widget control”. File-manager
work is remappable (`PROFILE_BINDINGS`) but most `fs.*` ids are **not**
registered as `orc fs …` verbs. Do not invent `orc fs copy` in docs or
scripts until a command with that verb exists.

## `orc` commands (current tree)

Prefix every line with `orc `.

### Shell

| Verb | Command id |
|------|------------|
| `settings open` | `settings.open` |
| `settings open config file` | `settings.open_config_file` |
| `password lock` | `password.lock` |
| `diagnostics export` | `diagnostics.export_bundle` |
| `data export backup` | `data.export_backup` |
| `navigation show workspace panel` | `navigation.show_workspace_panel` |
| `notification show center` | `notification.show_center` |
| `dock show` | `dock.show` |
| `search show universal` | `search.show_universal` |
| `onboarding toggle hint mode` | `onboarding.toggle_hint_mode` |

### Widgets and workspaces

| Verb | Command id |
|------|------------|
| `widget create <type>` | `widget.create` |
| `widget close` | `widget.close` |
| `widget move` | `widget.move` |
| `widget resize` | `widget.resize` |
| `widget focus next` | `widget.focus_next` |
| `widget show all` | `widget.show_all` |
| `workspace create` | `workspace.create` |
| `workspace delete` | `workspace.delete` |
| `workspace switch` | `workspace.switch_to` |
| `workspace switch next` | `workspace.switch_next` |
| `workspace switch previous` | `workspace.switch_previous` |
| `widget group dissolve` | `group.dissolve` |

### Terminal

| Verb | Command id |
|------|------------|
| `terminal split horizontal` | `terminal.split_horizontal` |
| `terminal split vertical` | `terminal.split_vertical` |
| `terminal tab new` | `terminal.tab_new` |
| `terminal close` | `terminal.close` |
| `terminal focus next pane` | `terminal.focus_next_pane` |
| `terminal focus previous pane` | `terminal.focus_previous_pane` |
| `terminal tab next` | `terminal.tab_next` |
| `terminal tab previous` | `terminal.tab_previous` |

Source: `crates/orchid-ui/src/commands.rs`,
`crates/orchid-widgets/src/commands.rs`,
`crates/orchid-ui/src/widgets/terminal/commands.rs`.

## File-manager actions (not `orc` verbs)

These ids appear in Settings shortcut remaps and in FM menus. They are
resolved by `orchid_core::lookup_fm_action` / profile bindings.

| Id | Typical Orchid profile |
|----|------------------------|
| `fs.copy` | Ctrl+C |
| `fs.cut` | Ctrl+X |
| `fs.paste` | Ctrl+V |
| `fs.undo` | Ctrl+Z |
| `fs.redo` | Ctrl+Y |
| `fs.rename` | F2 |
| `fs.delete` | F8 |
| `fs.delete-permanent` | Shift+Delete |
| `fs.new-folder` | F7 |
| `fs.new-file` | Shift+F4 |
| `fs.copy-to-other` | F5 |
| `fs.move-to-other` | F6 |
| `fs.open-tab` | Ctrl+Shift+T |
| `fs.open-other-pane` | Ctrl+Shift+Enter |
| `fs.branch-view` | Ctrl+B |
| `fs.find` | Alt+F7 |
| `fs.properties` | Alt+Enter |
| `fs.address-bar` | Ctrl+L |
| `fs.tab-new` | Ctrl+T |
| `fs.drive-root` | Ctrl+\ |
| `fs.drives-menu` | Alt+F1 |
| `fs.invert-selection` | * |
| `fs.select-mask-add` | + |
| `fs.select-mask-sub` | − |
| `viewer.open` | F3 |
| `viewer.edit` | F4 |

Menu-only FM ids (`fs.copy-verify`, `fs.delete-recycle`, …) live on the
context menu and are **not** in `PROFILE_BINDINGS`.

When adding a user-visible operation: register a `CommandDescriptor` with a
`TerminalInvocation` if it should be an `orc` verb; add a `ProfileBinding`
if it is a remappable FM/viewer action; do both only when the same id is
truly both.