# @umbra/engine-stream

Foundation feature (tier `foundation`): the live WebSocket stream (ADR 0045). One connection, latest frame wins, one store update per animation frame, reconnect with backoff 0.5, 1, 2, 4, 8 s (reset on `hello`). REST data is `@umbra/data`.

## Public API
- `connectEngineStream({ url, encoding?, subscription?, journalCapacity?, now?, createSocket?, scheduleFrame?, cancelFrame?, setTimer?, clearTimer? })` returns `EngineStreamConnection { store, source, journal, getState(), setSubscription(sub), close() }`. Only `encoding: "json"` exists; anything else throws. The default subscription is `{ max_rate_hz: 30 }`, sent after every `hello`; `setSubscription` replaces it (M8 viewport filter).
- `DEFAULT_JOURNAL_CAPACITY` (5000): the journal default when `journalCapacity` is not given.
- `streamUrl(engineUrl, pageOrigin)`: http->ws, https->wss, plus `/api/v1/stream?encoding=json`; an empty engine URL is the page origin.
- `EngineStreamProvider({ connection })` and hooks `useStreamStatus()`, `useRunStatus()` (identity kept while unchanged), `useRunState()` (only the state word `idle`/`running`/..., or `null` before the first run status; re-renders when the word changes, not on every tick, WI-14 review), `useControlSeq()` (the number of control frames, the frames sent after a command, counted before coalescing and never reset; `StreamState.controlSeq`; re-renders only when it changes; a reader that must react to every command compares it with its last value, ASM-938) and `useEntitySource()` (imperative `EntitySource { getFrame(), subscribe(listener) }`, for the map: no React render per entity).
- Event journal (ADR 0071, amends ADR 0045 for events): `connection.journal` / `useJournalSource()` is a `JournalSource { snapshot(), subscribe(listener), capacity, evicted() }`. Every decoded frame is journaled before coalescing, so no event is lost to latest-frame-wins; the store still holds no events. Records (`JournalRecord { seq, source, level, kind, message, tick?, time_s?, session?, entity_id?, detail?, received_at }`): per frame a `run:session` record when the session changes, `run:<state>` when state or stop reason changes (error level for `plugin_fault`/`network_fault`/`error`, warning for an unknown reason or a fault on a live run), one `event` record per event (kind verbatim, `detail` = the event as received), then `events_truncated` and `frames_dropped` (`detail: { dropped }`) warnings; connection records `connected`, `reconnected`, `disconnected`, `closed`, `incompatible`. The ring keeps the newest `journalCapacity` (default 5000, the app passes `settings.journal.capacity`; this package does not import `@umbra/settings`), numbers `seq` from 1 without restarting, spans sessions and reconnects. The snapshot is frozen and replaced, and listeners are called, once per animation frame in the store's flush (`close()` publishes at once). `now` (default `Date.now`) stamps `received_at`, display only.
- Run faults (WI-7): `RunStatus.fault` (plugin) and `RunStatus.network_fault` (`{ adapter, reason }`, API 1.1.0) are decoded; a malformed object is dropped. A stop reason this client does not know decodes to `stop_reason: "unknown"` with the word kept in `stop_reason_raw` (ASM-909). A change of either fault makes a `run:<state>` journal record.
- `useEntity(id | undefined)`: one entity of the newest frame by binary search; re-renders only when that entity's fields change (route by `len`/`next`) and keeps the previous object while equal.
- `useApplySubscription(subscription)`: applies on mount and when the value changes (deep equality, an inline literal is fine); nothing is reverted on unmount.
- Types `EventSummary`, `JournalRecord`, `JournalSource`, `JournalLevel`, `JournalSourceName`, `EngineStreamConnection`, `EngineStreamOptions`, `EntityFrame`, `EntitySource`, `EntitySummary`, `Clock`, `RunStatus`, `StreamStatus` (`connecting | open | reconnecting | closed | incompatible`), `Subscription`, `WebSocketLike`.

**Control gating must test `entity.ownership === "owned"`**: `remote` and `unknown` both mean no controls. `EntitySummary` and `RunStatus` are GUI domain types whose enums include `"unknown"` (ASM-359); `RunStatus.tick`/`time_s` are always kept.

Unknown message types, enum values and fields are tolerated (ASM-359); unknown messages are counted in `ignored`, `error` messages in `errors`, server drops in `dropped`, skipped invalid entities in `skipped`. Wire types are generated into `src/generated/stream.d.ts` by `bun run gen:api` (never edited).

```ts
// src/main.tsx, after the config loaded
const connection = connectEngineStream({ url: streamUrl(config.engineUrl, location.origin) });
<EngineStreamProvider connection={connection}>...</EngineStreamProvider>
```
