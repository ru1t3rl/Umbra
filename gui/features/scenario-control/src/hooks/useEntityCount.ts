import { useEntitySource } from "@umbra/engine-stream";
import { useCallback, useSyncExternalStore } from "react";

/**
 * The number of entities in the newest frame, or `undefined` before the first frame. A primitive snapshot:
 * React re-renders the consumer only when the count itself changes, not on every frame.
 */
export function useEntityCount(): number | undefined {
  const source = useEntitySource();
  const subscribe = useCallback((onChange: () => void) => source.subscribe(onChange), [source]);
  const getSnapshot = useCallback(() => source.getFrame()?.entities.length, [source]);
  return useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
}
