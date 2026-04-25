## Universal search not working (notes)

### Symptom
- Universal Search иногда/часто **не показывает кандидатов** при вводе текста (UI остаётся пустым).

### What was changed recently (related)
- `orchid-ui`: callbacks for search use `slint::spawn_local` and now wrap async code with `async_compat::Compat` to allow awaiting Tokio primitives.
- `orchid-widgets`: universal-search debouncer is restarted from `universal_search_push_query` so it can recover after `on_sleep`.
- `orchid-widgets`: `WidgetManager` subscribes to `WidgetSnapshotUpdated` and refreshes snapshot cache on demand; subscription is dropped in `shutdown`.
- Legacy alias: `"search"` normalized to `"universal-search"`; Slint accepts both.

### Fast repro checklist
- Open a workspace with a universal-search widget.
- Type a query that should match commands (e.g. `wid`, `term`, etc).
- Expect list to populate after ~150–300ms debounce.

### Things to verify (instrumentation)
- Does `universal_search_push_query` log the warning:
  - `"instance not in SEARCH_LIVE"`?
  - If yes: widget instance is not live/active/unloaded, or UI is sending the wrong `instance_id`.
- Does the widget snapshot actually contain `WidgetPayload::UniversalSearch` with `candidates.len() > 0`?
- Does UI rebuild / patch pick up updated `snapshot_cache` row for this instance?

### Hypotheses (likely)
- Widget instance is `Sleeping`/`Unloaded` and not re-activated when it becomes visible; `SEARCH_LIVE` can be missing (or debouncer stopped).
- UI path awaits Tokio primitives in a context not driven by Tokio (should be fixed via `async_compat`, but re-check other async paths).
- Snapshot cache refresh may be skipped due to `snapshot_renders_unchanged` (currently only compares terminal payloads; universal-search always considered “changed”).

### Next steps
- Add targeted logs/counters:
  - when universal-search widget transitions lifecycle (activate/sleep/unload)
  - when `SEARCH_LIVE` entry is inserted/removed
  - when UI receives `search-query-changed` and which instance_id it carries
- Confirm that on “wakeup” the widget gets `on_activate` (or debouncer is restarted) and `SEARCH_LIVE` exists.

