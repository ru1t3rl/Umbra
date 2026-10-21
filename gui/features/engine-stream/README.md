# @umbra/engine-stream

Foundation feature (tier `foundation`): the live WebSocket stream (ADR 0045). One connection, latest frame wins, one store update per animation frame, reconnect with backoff 0.5, 1, 2, 4, 8 s (reset on `hello`). REST data is `@umbra/data`.

## Public API
- `connectEngineStream({ url, encoding?, subscription?, createSocket?, scheduleFrame?, cancelFrame?, setTimer?, clearTimer? })` returns `EngineStreamConnection { store, source, getState(), setSubscription(sub), close() }`. Only `encoding: "json"` exists; anything else throws. The default subscription is `{ max_rate_hz: 30 }`, sent after every `hello`; `setSubscription` replaces it (M8 viewport filter).
- `streamUrl(engineUrl, pageOrigin)`: http->ws, https->wss, plus `/api/v1/stream?encoding=json`; an empty engine URL is the page origin.
- `EngineStreamProvider({ connection })` and hooks `useStreamStatus()`, `useRunStatus()` (identity kept while unchanged), `useEntitySource()` (imperative `EntitySource { getFrame(), subscribe(listener) }`, for the map: no React render per entity).
- Types `EngineStreamConnection`, `EngineStreamOptions`, `EntityFrame`, `EntitySource`, `EntitySummary`, `RunStatus`, `StreamStatus` (`connecting | open | reconnecting | closed | incompatible`), `Subscription`, `WebSocketLike`.

**Control gating must test `entity.ownership === "owned"`**: `remote` and `unknown` both mean no controls. `EntitySummary` and `RunStatus` are GUI domain types whose enums include `"unknown"` (ASM-359); `RunStatus.tick`/`time_s` are always kept.

Unknown message types, enum values and fields are tolerated (ASM-359); unknown messages are counted in `ignored`, `error` messages in `errors`, server drops in `dropped`, skipped invalid entities in `skipped`. Wire types are generated into `src/generated/stream.d.ts` by `bun run gen:api` (never edited).

```ts
// src/main.tsx, after the config loaded
const connection = connectEngineStream({ url: streamUrl(config.engineUrl, location.origin) });
<EngineStreamProvider connection={connection}>...</EngineStreamProvider>
```
