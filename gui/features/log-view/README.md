# @umbra/log-view

Domain feature (tier `domain`): the engine log (M8 WI-11, ADR 0071). Depends on `@umbra/engine-stream` (the journal source), `@umbra/ui-kit` and `@umbra/desktop` (the engine process output on desktop; a stub that returns nothing in the browser).

## Public API
- `LogView({ label, compact? })` (`LogViewProps`): the merged journal (event, run, connection and gui records, plus the engine process output on desktop) as a `VirtualList` `role="log"` named by `label`, one line per record with a Badge for warnings and errors, following the newest line until the user scrolls away.
  - Filters: a minimum severity, source chips (the "Engine process" chip appears only while that source exists or is pressed) and a case-insensitive text search over the line, kind, source id and source label. They are AND'd. `compact` hides them behind a "Filters" button; they stay in force while hidden.
  - A polite "N of M records" status; the count follows at once on the first render and on a filter change, and at most once per second when only the records changed (ASM-945).
  - "Download" saves the shown records as JSON Lines (`umbra-log-<time>.jsonl`), disabled when nothing is shown.
  - "No records" and "No records match" empty states.

The journal is read imperatively (`useJournalSource()`): a notification is at most one per animation frame and one state update here, so only this component renders when records arrive. Needs an `EngineStreamProvider` above it; no `DataProvider` (it makes no REST call).

Pure model (not exported): `formatRecord`, `mergeSources`, `filterRecords` (`NO_FILTER`), `toJsonLines`. `saveJsonLines` is the one browser-API helper.
