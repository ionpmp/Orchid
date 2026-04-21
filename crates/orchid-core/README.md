# orchid-core

Core abstractions for Orchid. This crate defines the shared type vocabulary used by every other workspace member: event-bus primitives, the command registry trait, and cross-cutting domain types (identifiers, addresses, errors).

It intentionally carries a small dependency footprint — `serde`, `tokio`, `tracing`, and `thiserror` — so that any crate in the graph can depend on it without pulling in unrelated subsystems.
