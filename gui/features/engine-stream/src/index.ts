// Public API of @umbra/engine-stream: the live WebSocket stream (ADR 0045). REST data is @umbra/data.

export type { EngineStreamConnection, EngineStreamOptions } from "./connection.ts";
export { connectEngineStream } from "./connection.ts";
export type { EngineStreamProviderProps } from "./EngineStreamProvider.tsx";
export { EngineStreamProvider } from "./EngineStreamProvider.tsx";
export { useEntitySource, useRunStatus, useStreamStatus } from "./hooks.ts";
export { streamUrl } from "./model/url.ts";
export type {
  EntityFrame,
  EntitySource,
  EntitySummary,
  RunStatus,
  StreamStatus,
  Subscription,
  WebSocketLike,
} from "./types.ts";
