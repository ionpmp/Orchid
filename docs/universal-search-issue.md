## Universal search not working (notes)

### Symptom
- Universal Search иногда/часто **не показывает кандидатов** при вводе текста (UI остаётся пустым).

### Root cause (fixed)
- `on_search_query_changed` вызывал **полный rebuild workspace** на каждый keystroke. Это пересоздавало `SearchView`, срывало фокус и гонялось с debouncer (~150 ms).
- Debouncer останавливался в `on_sleep` / `on_unload`, хотя виджет оставался на экране.

### Fix
- UI: только `universal_search_push_query` + wake/touch; обновление списка через `WidgetSnapshotUpdated` → `patch_workspace_frames`.
- Widget: debouncer больше не останавливается в `on_sleep` / `on_unload` (только при close/drop).

### Fast repro checklist
- Open a workspace with a universal-search widget.
- Type a query that should match commands (e.g. `wid`, `term`, etc).
- Expect list to populate after ~150–300ms debounce.

### If it regresses
- Check logs for `universal_search_push_query: instance not in SEARCH_LIVE` (wrong/closed instance id).
- Confirm `WidgetManager::start()` snapshot subscriber is running.
- Confirm `patch_workspace_frames` runs when `drain_frame_dirty_ids` is non-empty.
