// Public API of @umbra/engine-stream: the live WebSocket stream (ADR 0045). REST data is @umbra/data.

export type { EngineStreamProviderProps } from "./components/EngineStreamProvider/EngineStreamProvider.tsx";
export { EngineStreamProvider } from "./components/EngineStreamProvider/EngineStreamProvider.tsx";
export type { EngineStreamConnection, EngineStreamOptions } from "./connection.ts";
export { connectEngineStream } from "./connection.ts";
export { useEntitySource } from "./hooks/useEntitySource.ts";
export { useRunStatus } from "./hooks/useRunStatus.ts";
export { useStreamStatus } from "./hooks/useStreamStatus.ts";
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
