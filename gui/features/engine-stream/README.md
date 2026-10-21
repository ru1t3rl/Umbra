# @umbra/engine-stream

Foundation feature (tier `foundation`): the live WebSocket stream (store, connection, hooks). **WI-9 skeleton:** only the wire types generated from `api/stream.schema.json` into `src/generated/stream.d.ts` (`bun run gen:api`, never edited) are exported, as types: `StreamMessage`, `Subscription`, `StreamFilter`, `GeoBox`, `Hello`, `StreamSettings`, `Frame`, `EntitySummary`, `RouteProgress`, `EventDto`, `RunStatus`, `PluginFaultDto`, `StreamError`. The connection, Zustand store and hooks arrive in WI-10 (ADR 0045).
