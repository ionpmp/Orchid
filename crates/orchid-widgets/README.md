# orchid-widgets

Widget infrastructure for Orchid. Defines the widget trait, the grid layout model, lifecycle (mount / tick / unmount) and the persistence format used to remember workspaces between sessions.

Built-in widgets (weather, moon, system indicators, media player, RSS, search) will live here once the infrastructure stabilises. This crate depends on `orchid-core` for shared types and on `orchid-storage` for layout persistence.
