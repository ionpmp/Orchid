# orchid-core

Core abstractions for Orchid. This crate is the heart of the system: it defines the event bus, the action and command systems, and the input layer that every other crate produces and consumes against.

## Subsystems

- **Event bus** (`event`) — a priority-ordered, multi-producer, multi-consumer dispatcher. Subscribers choose between an `mpsc::Receiver`, an async closure, or an inline sync closure, and filter by event type, source, or a custom predicate.
- **Action system** (`action`) — the `Action` trait plus an `ActionDispatcher` with middleware support. `HistoryRecorder` middleware persists every executed action into `orchid-storage` so they can be browsed, undone, and replayed.
- **Command system** (`command`) — a registry keyed by stable command ids, a shell-like command parser (`orc fs move ...`), a fuzzy-search palette over the registry, and a keyboard shortcut type with canonical parsing and reserved-shortcut detection.
- **Input system** (`input`) — platform-agnostic input events (touch / mouse / keyboard / pen), ergonomic screen zones, a conservative gesture recogniser, and an `InputMapper` that resolves gestures and shortcuts to command ids.

## Scope

This crate stays deliberately generic: it does not register any domain-specific actions or commands (those live in `orchid-fs`, `orchid-widgets`, etc.) and it does not touch the Windows API, the renderer, or the UI layer. Domain crates depend on `orchid-core` to get the right traits to implement and the right registries to plug into.
