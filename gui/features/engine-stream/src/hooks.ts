import { createContext, useContext } from "react";
import { useStore } from "zustand";
import type { EngineStreamConnection } from "./connection.ts";
import type { EntitySource, RunStatus, StreamStatus } from "./types.ts";

export const EngineStreamContext = createContext<EngineStreamConnection | undefined>(undefined);

function useConnection(): EngineStreamConnection {
  const connection = useContext(EngineStreamContext);
  if (!connection) throw new Error("EngineStreamProvider is missing above this component");
  return connection;
}

/** Connection status; re-renders only when it changes. */
export function useStreamStatus(): StreamStatus {
  return useStore(useConnection().store, (s) => s.status);
}

/** The latest run status; re-renders only when its content changes. */
export function useRunStatus(): RunStatus | undefined {
  return useStore(useConnection().store, (s) => s.run);
}

/** Stable imperative source of entity frames, for consumers that must not re-render per frame. */
export function useEntitySource(): EntitySource {
  return useConnection().source;
}
