# orchid-widgets

Widget framework for Orchid. Provides:

* **Widget trait** — `Widget` with async lifecycle callbacks
  (`on_create` / `on_activate` / `on_sleep` / `on_unload` / `on_close` /
  `on_resize`) plus a cheap, frame-rate-safe `snapshot()` method.
* **Registry** — `WidgetRegistry` of `WidgetDescriptor`s, one per widget
  type (`"terminal"`, `"weather"`, …).
* **Manager** — `WidgetManager` creates instances, enforces lifecycle
  transitions, sleeps / unloads idle widgets, persists state through
  `orchid-storage`, and publishes events on `orchid-core`'s bus.
* **Workspace manager** — up to `MAX_WORKSPACES` (9) virtual desktops
  with dense ordinals, switch-next / switch-previous / switch-by-ordinal,
  and storage round-trips.
* **Layout engine** — 16 × 10 grid by default, auto-placement (first-fit
  or spiral), collision detection, pixel-space snapshots for the
  renderer, and a free-form mode toggle.
* **Groups** — tab stacks (`WidgetGroup`) persisted in a dedicated redb
  table via `StateStore::raw_database()`. `GroupManager` supports create /
  dissolve / add / remove / `switch_active` / `reorder_members` /
  `update_slot`. The UI binds a group tab strip (switch, close-to-ungroup,
  ‹› reorder, dissolve) and forms stacks by dropping one widget header onto
  another; **Alt+drag** detaches the active member.
* **Commands** — `build_command_set(...)` produces every widget /
  workspace / group command ready to register on a
  `CommandRegistry`.

## Stability

The crate is source-compatible with the rest of the workspace as of the
widget-framework task: `orchid-core` provides the event bus and the
action system, `orchid-storage` stores widget and workspace rows, and
`orchid-ui` consumes `WidgetSnapshot` through the
`WidgetViewDispatcher` (see the `orchid-ui` README).

The Slint workspace dashboard — drag / resize / dock / switcher — is a
downstream concern; the crate's responsibilities stop at shipping the
`WidgetPayload`, `LayoutSnapshot`, and command/event surface the UI
shell can bind to.

## Example

```rust,no_run
use std::sync::Arc;
use orchid_core::{EventBus, EventBusConfig};
use orchid_widgets::{
    CreateWidgetRequest, WidgetManager, WidgetManagerOptions, WidgetRegistry,
    WorkspaceManager,
};

# async fn demo() -> orchid_widgets::Result<()> {
let bus = Arc::new(EventBus::new(EventBusConfig::default()));
let storage = Arc::new(orchid_storage::StateStore::open_in_memory("demo").unwrap());
let config = Arc::new(parking_lot::RwLock::new(orchid_storage::OrchidConfig::default()));

let registry = Arc::new(WidgetRegistry::new());
let jobs = Arc::new(orchid_core::BackgroundJobQueue::new());
let widgets = WidgetManager::new(
    registry.clone(),
    bus.clone(),
    storage.clone(),
    config.clone(),
    /* locale */ Arc::new(orchid_i18n::LocaleManager::new(orchid_i18n::default_language(), None).unwrap()),
    jobs,
    WidgetManagerOptions::default(),
);
let workspaces = WorkspaceManager::new(bus.clone(), storage.clone());

let ws = workspaces.create("Main".into()).await?;
let id = widgets.create(CreateWidgetRequest {
    type_id: "terminal".into(),
    workspace_id: ws,
    position: None,
    size: None,
    initial_lifecycle: None,
    config_bytes: None,
}).await?;

assert!(widgets.get_instance(id).is_ok());
# Ok(())
# }
```
